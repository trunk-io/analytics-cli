//! Just enough of the Language Server Protocol to ask a server what a file declares.
//!
//! Framing and JSON-RPC come from [`lsp_server`], and every method name and payload shape
//! from [`lsp_types`], so a request is named by its type rather than by a string literal.
//!
//! A request that times out leaves a reply in flight that would arrive after the next
//! request was sent. Replies are matched by id, so a late one is discarded rather than
//! misread — but the server has also shown it cannot keep up, so it is killed and the
//! caller restarts it instead of waiting on it again.

use std::{
    io::{BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    thread,
    time::{Duration, Instant},
};

use lsp_server::{Message, Notification, Request, RequestId, Response};
use lsp_types::{
    ClientCapabilities, DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbol,
    DocumentSymbolClientCapabilities, DocumentSymbolParams, DocumentSymbolResponse,
    InitializeParams, PartialResultParams, TextDocumentClientCapabilities, TextDocumentIdentifier,
    TextDocumentItem, Uri, WorkDoneProgressParams,
    notification::{DidCloseTextDocument, DidOpenTextDocument, Initialized},
    request::{DocumentSymbolRequest, Initialize},
};

pub struct LanguageServer {
    process: Child,
    stdin: ChildStdin,
    incoming: Receiver<Message>,
    next_id: i32,
    broken: bool,
}

impl LanguageServer {
    /// Start `program` and complete the LSP handshake against workspace `root`.
    pub fn start(
        program: &Path,
        args: &[&str],
        root: &Path,
        timeout: Duration,
    ) -> anyhow::Result<Self> {
        let mut process = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("language server has no stdin"))?;
        let stdout = process
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("language server has no stdout"))?;

        let (sender, incoming) = channel();
        thread::spawn(move || read_messages(BufReader::new(stdout), &sender));

        let mut server = Self {
            process,
            stdin,
            incoming,
            next_id: 1,
            broken: false,
        };
        let root_uri = file_uri(root)?;
        server.request::<Initialize>(
            #[allow(deprecated)] // `root_uri` is how sourcekit-lsp still finds the workspace.
            InitializeParams {
                process_id: Some(std::process::id()),
                root_uri: Some(root_uri),
                capabilities: ClientCapabilities {
                    text_document: Some(TextDocumentClientCapabilities {
                        document_symbol: Some(DocumentSymbolClientCapabilities {
                            hierarchical_document_symbol_support: Some(true),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
            timeout,
        );
        server.notify::<Initialized>(lsp_types::InitializedParams {});
        if server.broken {
            return Err(anyhow::anyhow!(
                "language server did not complete initialize"
            ));
        }
        Ok(server)
    }

    /// The symbols `file_path` declares. The text is sent rather than left for the
    /// server to read, so this answers even where build settings do not resolve.
    pub fn document_symbols(
        &mut self,
        file_path: &Path,
        language_id: &str,
        text: &str,
        timeout: Duration,
    ) -> Option<Vec<DocumentSymbol>> {
        let uri = file_uri(file_path).ok()?;
        self.notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: language_id.to_owned(),
                version: 1,
                text: text.to_owned(),
            },
        });
        let response = self
            .request::<DocumentSymbolRequest>(
                DocumentSymbolParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                },
                timeout,
            )
            .flatten();
        self.notify::<DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
        });
        match response {
            Some(DocumentSymbolResponse::Nested(symbols)) => Some(symbols),
            // Only a server that ignored `hierarchicalDocumentSymbolSupport` answers flat,
            // and without nesting there is nothing to tie a method to its type.
            Some(DocumentSymbolResponse::Flat(_)) => {
                tracing::debug!("{} answered without hierarchy", file_path.display());
                None
            }
            None => None,
        }
    }

    pub fn is_broken(&self) -> bool {
        self.broken
    }

    fn request<R: lsp_types::request::Request>(
        &mut self,
        params: R::Params,
        timeout: Duration,
    ) -> Option<R::Result> {
        if self.broken {
            return None;
        }
        let id = RequestId::from(self.next_id);
        self.next_id += 1;
        let params = serde_json::to_value(params).ok()?;
        self.send(Message::Request(Request {
            id: id.clone(),
            method: R::METHOD.to_owned(),
            params,
        }));

        let deadline = Instant::now() + timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return self.abandon(R::METHOD, "timed out");
            };
            let message = match self.incoming.recv_timeout(remaining) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => return self.abandon(R::METHOD, "timed out"),
                Err(RecvTimeoutError::Disconnected) => return self.abandon(R::METHOD, "exited"),
            };
            match message {
                Message::Response(response) if response.id == id => {
                    return match response.response_result {
                        Ok(result) => serde_json::from_value(result).ok(),
                        Err(error) => {
                            tracing::debug!("language server refused {}: {:?}", R::METHOD, error);
                            None
                        }
                    };
                }
                // sourcekit-lsp registers capabilities and asks for configuration during
                // startup; a peer that never replies leaves those pending for its lifetime.
                Message::Request(request) => {
                    self.send(Message::Response(Response::new_ok(
                        request.id,
                        serde_json::Value::Null,
                    )));
                }
                // A reply to a request we already gave up on, or a diagnostic we ignore.
                Message::Response(_) | Message::Notification(_) => {}
            }
        }
    }

    fn notify<N: lsp_types::notification::Notification>(&mut self, params: N::Params) {
        if self.broken {
            return;
        }
        let Ok(params) = serde_json::to_value(params) else {
            return;
        };
        self.send(Message::Notification(Notification {
            method: N::METHOD.to_owned(),
            params,
        }));
    }

    fn send(&mut self, message: Message) {
        if message.write(&mut self.stdin).is_err() || self.stdin.flush().is_err() {
            self.abandon::<()>("write", "closed its input");
        }
    }

    fn abandon<T>(&mut self, method: &str, reason: &str) -> Option<T> {
        if !self.broken {
            tracing::warn!(
                "language server {} during {}; abandoning it",
                reason,
                method
            );
            self.broken = true;
            let _ = self.process.kill();
        }
        None
    }
}

impl Drop for LanguageServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn read_messages<R: std::io::BufRead>(mut reader: R, sender: &Sender<Message>) {
    while let Ok(Some(message)) = Message::read(&mut reader) {
        if sender.send(message).is_err() {
            return;
        }
    }
}

/// A `file://` URI. A server that cannot parse the URI answers with no symbols rather
/// than an error, so a path with a space fails silently unless it is encoded — and
/// `lsp_types::Uri` is a bare RFC 3986 parser that will not encode one for us.
///
/// The path is made absolute first, because `file://` takes an authority: a relative
/// `file://Tests/Foo.swift` parses with `Tests` as the *host* and loses a path component.
/// Only the URI is absolute — the caller keeps reporting the path it was given, which is
/// what codeowners are resolved against.
fn file_uri(path: &Path) -> anyhow::Result<Uri> {
    // Lexical, so a symlinked checkout is not rewritten to somewhere the caller never named.
    let absolute = std::path::absolute(path)
        .map_err(|e| anyhow::anyhow!("cannot resolve {}: {e}", path.display()))?;
    let url = url::Url::from_file_path(&absolute)
        .map_err(|_| anyhow::anyhow!("not a usable file path: {}", absolute.display()))?;
    url.as_str()
        .parse::<Uri>()
        .map_err(|e| anyhow::anyhow!("{} is not a usable URI: {e}", url.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A server that cannot parse the URI answers with no symbols rather than an error, so
    // both of these fail silently in production if they regress. `lsp_types::Uri` will not
    // encode for us and `file://` takes an authority, so neither is free.
    #[test]
    fn a_path_with_a_space_is_percent_encoded() {
        let uri = file_uri(Path::new("/repo/Tests/My Test.swift")).unwrap();
        assert_eq!(uri.as_str(), "file:///repo/Tests/My%20Test.swift");
    }

    #[test]
    fn a_hash_is_encoded_rather_than_starting_a_fragment() {
        let uri = file_uri(Path::new("/repo/Tests/a#b.swift")).unwrap();
        assert_eq!(uri.as_str(), "file:///repo/Tests/a%23b.swift");
    }

    // A relative path would otherwise parse with its first component as the *host*,
    // silently dropping it: `file://Tests/Foo.swift` is host `Tests`, path `/Foo.swift`.
    #[test]
    fn a_relative_path_becomes_an_absolute_uri() {
        let uri = file_uri(Path::new("Tests/Foo.swift")).unwrap();
        assert!(
            uri.as_str().starts_with("file:///"),
            "expected an absolute file URI, got {}",
            uri.as_str()
        );
        assert!(
            uri.as_str().ends_with("/Tests/Foo.swift"),
            "expected the path to survive, got {}",
            uri.as_str()
        );
    }
}
