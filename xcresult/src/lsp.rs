//! Just enough of the Language Server Protocol to ask a server what a file declares.
//!
//! Once a request times out the stream cannot be resynchronised — a late reply would be
//! read as the answer to the *next* request — so the process is killed and later calls
//! refused, rather than an upload waiting on a server that stopped answering.

use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

pub struct LanguageServer {
    process: Child,
    stdin: ChildStdin,
    incoming: Receiver<Value>,
    next_id: i64,
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
        thread::spawn(move || read_messages(stdout, &sender));

        let mut server = Self {
            process,
            stdin,
            incoming,
            next_id: 1,
            broken: false,
        };
        server.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": file_uri(root),
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                    }
                }
            }),
            timeout,
        );
        server.notify("initialized", json!({}));
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
    ) -> Option<Value> {
        let uri = file_uri(file_path);
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text
                }
            }),
        );
        let symbols = self.request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
            timeout,
        );
        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        );
        symbols
    }

    pub fn is_broken(&self) -> bool {
        self.broken
    }

    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Option<Value> {
        if self.broken {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));

        let deadline = Instant::now() + timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return self.abandon(method, "timed out");
            };
            let message = match self.incoming.recv_timeout(remaining) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => return self.abandon(method, "timed out"),
                Err(RecvTimeoutError::Disconnected) => return self.abandon(method, "exited"),
            };
            if message.get("id").and_then(Value::as_i64) == Some(id) {
                if let Some(error) = message.get("error") {
                    tracing::debug!("language server refused {}: {}", method, error);
                    return None;
                }
                return message.get("result").cloned();
            }
            // sourcekit-lsp registers capabilities and asks for configuration during
            // startup; a peer that never replies leaves those pending for its lifetime.
            if let (Some(id), Some(_)) = (message.get("id"), message.get("method")) {
                let id = id.clone();
                self.send(json!({ "jsonrpc": "2.0", "id": id, "result": Value::Null }));
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        if self.broken {
            return;
        }
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn send(&mut self, message: Value) {
        let body = message.to_string();
        let framed = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        if self.stdin.write_all(framed.as_bytes()).is_err() || self.stdin.flush().is_err() {
            self.abandon("write", "closed its input");
        }
    }

    fn abandon(&mut self, method: &str, reason: &str) -> Option<Value> {
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

fn read_messages<R: Read>(stdout: R, sender: &Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.trim().eq_ignore_ascii_case("content-length")
            {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
        let Some(content_length) = content_length else {
            return;
        };
        let mut body = vec![0_u8; content_length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        let Ok(message) = serde_json::from_slice::<Value>(&body) else {
            return;
        };
        if sender.send(message).is_err() {
            return;
        }
    }
}

/// A `file://` URI. A server that cannot parse the URI answers with no symbols rather
/// than an error, so a path with a space fails silently unless it is encoded here.
fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(char::from(byte));
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::plain("/repo/Tests/Test.swift", "file:///repo/Tests/Test.swift")]
    #[case::space("/repo/Tests/My Test.swift", "file:///repo/Tests/My%20Test.swift")]
    #[case::hash_is_a_uri_fragment("/repo/a#b.swift", "file:///repo/a%23b.swift")]
    fn a_path_becomes_a_percent_encoded_uri(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(file_uri(Path::new(path)), expected);
    }

    fn framed(bodies: &[&str]) -> String {
        bodies
            .iter()
            .map(|body| format!("Content-Length: {}\r\n\r\n{}", body.len(), body))
            .collect()
    }

    #[test]
    fn framed_messages_are_read_back_in_order() {
        let (sender, receiver) = channel();
        read_messages(
            Cursor::new(framed(&[
                r#"{"id":1,"result":[]}"#,
                r#"{"id":2,"result":7}"#,
            ])),
            &sender,
        );
        drop(sender);
        let received = receiver.iter().collect::<Vec<_>>();
        assert_eq!(received.len(), 2);
        assert_eq!(received[1]["result"], json!(7));
    }

    // The frame carries a byte count, so a non-ASCII body split by character count
    // drifts one message at a time and then hangs on the next read.
    #[test]
    fn a_multibyte_body_is_framed_by_bytes() {
        let (sender, receiver) = channel();
        read_messages(
            Cursor::new(framed(&[r#"{"id":1,"result":"café"}"#])),
            &sender,
        );
        drop(sender);
        assert_eq!(
            receiver
                .iter()
                .next()
                .map(|message| message["result"].clone()),
            Some(json!("café"))
        );
    }

    #[test]
    fn a_lowercased_header_is_still_a_content_length() {
        let (sender, receiver) = channel();
        let body = r#"{"id":1,"result":[]}"#;
        read_messages(
            Cursor::new(format!("content-length: {}\r\n\r\n{}", body.len(), body)),
            &sender,
        );
        drop(sender);
        assert_eq!(receiver.iter().count(), 1);
    }

    #[test]
    fn a_truncated_message_ends_the_stream_instead_of_blocking() {
        let (sender, receiver) = channel();
        read_messages(Cursor::new("Content-Length: 40\r\n\r\n{\"id\":1}"), &sender);
        drop(sender);
        assert_eq!(receiver.iter().count(), 0);
    }
}
