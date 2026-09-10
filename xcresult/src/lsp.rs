//! Just enough of the Language Server Protocol to ask a server what a file declares.
//!
//! Framing and JSON-RPC come from [`lsp_server`], and every method name and payload shape
//! from [`lsp_types`], so a request is named by its type rather than by a string literal.
//!
//! A request that times out leaves a reply in flight that would arrive after the next
//! request was sent. Replies are matched by id, so a late one is discarded rather than
//! misread — but the server has also shown it cannot keep up, so it is killed and the
//! caller restarts it instead of waiting on it again.
//!
//! # How source files are read
//!
//! **No file is ever held in memory, whole or in part beyond `CHUNK`.** `didOpen` has to
//! carry a file's entire text — the protocol takes it inline, and neither a path nor a
//! stream — so the naive shape of this is a `String` per file, which for one generated
//! source file is tens of megabytes resident and several copies of it before serialization
//! is done.
//!
//! Instead each file is read **twice, in `CHUNK`-sized pieces, and never retained**:
//!
//! 1. **The measuring pass.** `FileText` reads a chunk, hands it to serde_json's escaper
//!    via [`Serializer::collect_str`], and the escaped bytes land in a `Counting` writer
//!    over [`io::sink`] — counted, then dropped. This yields the exact `Content-Length`
//!    while holding nothing, and is also where an unreadable or non-UTF-8 file is caught,
//!    before anything has been announced to the server.
//! 2. **The emitting pass.** The header goes out, then the same chunked read runs again,
//!    this time escaping into a [`BufWriter`] over the server's stdin.
//!
//! Two reads rather than one is the price of not buffering; the second usually comes from
//! the page cache. Both passes escape through serde_json, so the length announced cannot
//! disagree with the body written — and `send_did_open` tallies the second
//! pass anyway, because a file rewritten between the two would otherwise misframe the
//! stream. Peak memory per `didOpen` is therefore flat in file size, which
//! `a_file_larger_than_a_chunk_is_never_handed_over_whole` exists to keep true.
//!
//! Reading in fixed-size pieces splits multi-byte characters, and [`fmt::Write::write_str`]
//! takes only valid UTF-8, so `FileText::stream` carries the incomplete tail of a chunk
//! over into the next one.

use std::{
    cell::Cell,
    fmt, fs,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    str,
    sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    thread,
    time::{Duration, Instant},
};

use lsp_server::{Message, Notification, Request, RequestId, Response};
use lsp_types::{
    ClientCapabilities, DidCloseTextDocumentParams, DocumentSymbol,
    DocumentSymbolClientCapabilities, DocumentSymbolParams, DocumentSymbolResponse,
    InitializeParams, PartialResultParams, TextDocumentClientCapabilities, TextDocumentIdentifier,
    Uri, WorkDoneProgressParams,
    notification::{DidCloseTextDocument, DidOpenTextDocument, Initialized, Notification as _},
    request::{DocumentSymbolRequest, Initialize},
};
use serde::ser::{Serialize, SerializeStruct, Serializer};

/// How much of a file is read, and buffered towards the pipe, at a time.
///
/// Nothing here ever holds a whole file: a `didOpen` for a 40 MB source file has the same
/// peak footprint as one for a 400 byte source file -- this buffer, plus the `BufWriter` of
/// the same size, plus at most three bytes of a character carried across a read boundary.
const CHUNK: usize = 64 * 1024;

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
    ///
    /// The file is read in chunks and never held: see this module's "How source files are
    /// read", and `send_did_open` below.
    pub fn document_symbols(
        &mut self,
        file_path: &Path,
        language_id: &str,
        timeout: Duration,
    ) -> Option<Vec<DocumentSymbol>> {
        let uri = file_uri(file_path).ok()?;
        if let Err(error) = self.send_did_open(&uri, language_id, file_path) {
            tracing::debug!("could not send {}: {}", file_path.display(), error);
            return None;
        }
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

    /// `didOpen` for `path`, framed and written here rather than through
    /// [`Message::write`], which reaches the pipe only via a `to_string` of the whole
    /// message — one full copy of the file on top of the one being read.
    ///
    /// `Content-Length` precedes the body and a pipe cannot be rewound, so the length has to
    /// be known before any of the body goes out. That is what the two passes are for: the
    /// first serializes into [`io::sink`] to measure, the second writes the header and
    /// streams the body. serde_json does the escaping in both, so the count cannot disagree
    /// with what is emitted.
    ///
    /// Reading twice means the two passes can disagree if the file is rewritten in between,
    /// and a body that does not match the header it was announced with desynchronises the
    /// stream for every message after it. So the second pass is tallied too, and a mismatch
    /// abandons the server rather than corrupting the rest of the scan — the same response
    /// this already has for one that stops answering.
    fn send_did_open(&mut self, uri: &Uri, language_id: &str, path: &Path) -> io::Result<()> {
        if self.broken {
            return Err(io::Error::other("language server was already abandoned"));
        }
        let text = FileText::new(path);
        let params = DidOpenParams {
            uri,
            language_id,
            text: &text,
        };
        let message = Envelope {
            method: DidOpenTextDocument::METHOD,
            params: &params,
        };

        let mut measured = Counting {
            inner: io::sink(),
            count: 0,
        };
        // A read or encoding failure surfaces here, before anything is written -- so an
        // unreadable or non-UTF-8 file costs this file's symbols rather than the stream.
        serde_json::to_writer(&mut measured, &message)
            .map_err(|error| text.take_error().unwrap_or_else(|| io::Error::other(error)))?;
        let length = measured.count;

        // Scoped so the borrow of `stdin` ends before the failure path needs `self`.
        let count = {
            let mut out = BufWriter::with_capacity(CHUNK, &mut self.stdin);
            write!(out, "Content-Length: {length}\r\n\r\n")?;
            let mut written = Counting {
                inner: &mut out,
                count: 0,
            };
            let sent = serde_json::to_writer(&mut written, &message)
                .map_err(|error| text.take_error().unwrap_or_else(|| io::Error::other(error)));
            let count = written.count;
            // Flushed before reporting a failure: the header is already on its way, so as
            // much of the body as exists has to follow it for the length check to mean
            // anything.
            let flushed = out.flush();
            sent?;
            flushed?;
            count
        };

        if count != length {
            self.abandon::<()>(
                DidOpenTextDocument::METHOD,
                "changed size while it was being sent",
            );
            return Err(io::Error::other(format!(
                "announced {length} bytes and sent {count}"
            )));
        }
        Ok(())
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
        let Ok(params) = serde_json::to_value(params) else {
            return;
        };
        self.notify_value(N::METHOD, params);
    }

    fn notify_value(&mut self, method: &str, params: serde_json::Value) {
        if self.broken {
            return;
        }
        self.send(Message::Notification(Notification {
            method: method.to_owned(),
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
/// A file's contents as a JSON string value, read in chunks rather than held.
///
/// [`Serializer::serialize_str`] wants the whole string contiguous, which is what forced the
/// file to be resident. [`Serializer::collect_str`] takes a [`fmt::Display`] instead, and
/// serde_json overrides it to push each fragment through its escaper straight into the
/// writer — so a `Display` that reads in chunks never materializes the file.
struct FileText<'a> {
    path: &'a Path,
    /// `Display::fmt` can only fail with a payload-free [`fmt::Error`], so the real cause is
    /// stashed here. serde_json's own `collect_str` adapter does the same for writer errors.
    error: Cell<Option<io::Error>>,
}

impl<'a> FileText<'a> {
    fn new(path: &'a Path) -> Self {
        Self {
            path,
            error: Cell::new(None),
        }
    }

    /// The stashed cause, if a pass failed.
    fn take_error(&self) -> Option<io::Error> {
        self.error.take()
    }

    fn stream(&self, f: &mut fmt::Formatter<'_>) -> io::Result<()> {
        let mut file = fs::File::open(self.path)?;
        let mut buf = vec![0_u8; CHUNK];
        // Bytes at the front of `buf` held over from the last chunk: a read boundary can fall
        // inside a multi-byte character, and `write_str` takes only valid UTF-8.
        let mut carry = 0_usize;
        loop {
            let read = file.read(&mut buf[carry..])?;
            if read == 0 {
                break;
            }
            let filled = carry + read;
            let valid = match str::from_utf8(&buf[..filled]) {
                Ok(_) => filled,
                // No `error_len` means the input merely stops mid-character, so the rest of
                // it is in the next chunk. Anything else is a file we cannot send.
                Err(error) if error.error_len().is_none() => error.valid_up_to(),
                Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
            };
            let chunk = str::from_utf8(&buf[..valid])
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            f.write_str(chunk)
                .map_err(|_| io::Error::other("the serializer rejected a fragment"))?;
            buf.copy_within(valid..filled, 0);
            carry = filled - valid;
        }
        if carry > 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "file ends inside a multi-byte character",
            ));
        }
        Ok(())
    }
}

impl fmt::Display for FileText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.stream(f) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.error.set(Some(error));
                Err(fmt::Error)
            }
        }
    }
}

/// `{"jsonrpc": "2.0", "method": ..., "params": ...}` — the framing [`Message::write`] would
/// add, rebuilt here because it reaches the pipe only through a fully buffered `to_string`.
struct Envelope<'a, P> {
    method: &'a str,
    params: &'a P,
}

impl<P: Serialize> Serialize for Envelope<'_, P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut envelope = serializer.serialize_struct("JsonRpc", 3)?;
        envelope.serialize_field("jsonrpc", "2.0")?;
        envelope.serialize_field("method", self.method)?;
        envelope.serialize_field("params", self.params)?;
        envelope.end()
    }
}

/// [`DidOpenTextDocumentParams`] with the text streamed instead of owned.
struct DidOpenParams<'a> {
    uri: &'a Uri,
    language_id: &'a str,
    text: &'a FileText<'a>,
}

impl Serialize for DidOpenParams<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut params = serializer.serialize_struct("DidOpenTextDocumentParams", 1)?;
        params.serialize_field("textDocument", &TextDocumentItemRef(self))?;
        params.end()
    }
}

struct TextDocumentItemRef<'a>(&'a DidOpenParams<'a>);

impl Serialize for TextDocumentItemRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut item = serializer.serialize_struct("TextDocumentItem", 4)?;
        item.serialize_field("uri", self.0.uri)?;
        item.serialize_field("languageId", self.0.language_id)?;
        item.serialize_field("version", &1)?;
        item.serialize_field("text", &StreamedText(self.0.text))?;
        item.end()
    }
}

struct StreamedText<'a>(&'a FileText<'a>);

impl Serialize for StreamedText<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self.0)
    }
}

/// Tallies what it passes on, so the body's length can be measured in one pass and checked
/// against the header in the next. [`io::sink`] as the inner writer makes it count alone.
struct Counting<W> {
    inner: W,
    count: usize,
}

impl<W: Write> Write for Counting<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.count += written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

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
    use temp_testdir::TempDir;

    use super::*;

    /// Records what the streamed file is handed to the serializer in, without keeping it.
    struct Fragments {
        largest: usize,
        total: usize,
        text: String,
    }

    impl fmt::Write for Fragments {
        fn write_str(&mut self, fragment: &str) -> fmt::Result {
            self.largest = self.largest.max(fragment.len());
            self.total += fragment.len();
            self.text.push_str(fragment);
            Ok(())
        }
    }

    fn fragments_of(path: &Path) -> Fragments {
        let mut fragments = Fragments {
            largest: 0,
            total: 0,
            text: String::new(),
        };
        // `write!` builds the `Formatter` that `Display::fmt` writes into, so this drives
        // exactly the path `collect_str` does.
        fmt::Write::write_fmt(&mut fragments, format_args!("{}", FileText::new(path)))
            .expect("the file streams");
        fragments
    }

    // The whole point of the streaming payload. A file many chunks long must still reach the
    // serializer a chunk at a time, or nothing has been gained over reading it in one go.
    #[test]
    fn a_file_larger_than_a_chunk_is_never_handed_over_whole() {
        let dir = TempDir::default();
        let path = dir.join("Big.swift");
        let line = "// a line of source that is long enough to matter\n";
        let body = line.repeat((CHUNK * 3) / line.len());
        fs::write(&path, &body).unwrap();
        assert!(body.len() > CHUNK * 2, "the fixture has to span chunks");

        let fragments = fragments_of(&path);
        assert!(
            fragments.largest <= CHUNK,
            "a fragment of {} bytes means {} of the file was held at once",
            fragments.largest,
            fragments.largest
        );
        assert_eq!(fragments.total, body.len(), "and all of it is sent");
    }

    // A read boundary lands wherever it lands, and `write_str` takes only valid UTF-8, so a
    // character split across two reads has to be rejoined rather than dropped or mangled.
    #[test]
    fn a_character_split_across_two_reads_survives() {
        let dir = TempDir::default();
        let path = dir.join("Accented.swift");
        // One byte short of a chunk, so the two-byte character straddles the boundary.
        let body = format!("{}é tail", "a".repeat(CHUNK - 1));
        fs::write(&path, &body).unwrap();

        assert_eq!(fragments_of(&path).text, body);
    }

    // Reported rather than silently emitted as replacement characters, and -- because it is
    // found on the measuring pass -- before any of it has been announced to the server.
    #[test]
    fn a_file_that_is_not_utf8_is_refused() {
        let dir = TempDir::default();
        let path = dir.join("Latin1.swift");
        fs::write(&path, [b'/', b'/', 0xFF, b'\n']).unwrap();

        let text = FileText::new(&path);
        let mut sink = Fragments {
            largest: 0,
            total: 0,
            text: String::new(),
        };
        assert!(fmt::Write::write_fmt(&mut sink, format_args!("{text}")).is_err());
        assert_eq!(
            text.take_error().map(|error| error.kind()),
            Some(io::ErrorKind::InvalidData)
        );
    }

    // The payload is ours now rather than `lsp_types`', so it has to keep describing the
    // same document. A server given a malformed `didOpen` answers with no symbols rather
    // than an error, so a drifted field name would look exactly like a checkout that
    // declares nothing. Compared parsed, since field order carries no meaning in JSON.
    #[test]
    fn the_streamed_did_open_describes_the_same_document_as_the_typed_one() {
        let dir = TempDir::default();
        let path = dir.join("MyTests.swift");
        let body = "class MyTests {\n\t\"quoted\" \\ é\n}\n";
        fs::write(&path, body).unwrap();
        let uri = file_uri(&path).unwrap();

        let text = FileText::new(&path);
        let streamed = serde_json::to_string(&DidOpenParams {
            uri: &uri,
            language_id: "swift",
            text: &text,
        })
        .unwrap();

        let typed = serde_json::to_string(&lsp_types::DidOpenTextDocumentParams {
            text_document: lsp_types::TextDocumentItem {
                uri: uri.clone(),
                language_id: String::from("swift"),
                version: 1,
                text: String::from(body),
            },
        })
        .unwrap();

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&streamed).unwrap(),
            serde_json::from_str::<serde_json::Value>(&typed).unwrap()
        );
    }

    // The header is written before the body and a pipe cannot be rewound, so the measuring
    // pass has to agree with the emitting one exactly -- a byte out either truncates the
    // message or leaves the stream misframed for everything after it.
    #[test]
    fn the_measured_length_matches_what_is_emitted() {
        let dir = TempDir::default();
        let path = dir.join("MyTests.swift");
        fs::write(&path, "class MyTests {\n\t\"q\" \\ é\u{7}\n}\n").unwrap();
        let uri = file_uri(&path).unwrap();

        let text = FileText::new(&path);
        let message = Envelope {
            method: "textDocument/didOpen",
            params: &DidOpenParams {
                uri: &uri,
                language_id: "swift",
                text: &text,
            },
        };

        let mut measured = Counting {
            inner: io::sink(),
            count: 0,
        };
        serde_json::to_writer(&mut measured, &message).unwrap();

        let mut emitted = Counting {
            inner: Vec::new(),
            count: 0,
        };
        serde_json::to_writer(&mut emitted, &message).unwrap();

        assert_eq!(measured.count, emitted.count);
        assert_eq!(measured.count, emitted.inner.len());
    }

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
