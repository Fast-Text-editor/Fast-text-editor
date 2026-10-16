use std::io::{self, BufRead, Write};

use serde_json::Value;

/// A JSON-RPC connection using the LSP `Content-Length` framing.
///
/// Generic over its reader/writer so it can be unit-tested without spawning a
/// process.
pub struct Transport<R, W> {
    reader: R,
    writer: W,
}

impl<R: BufRead, W: Write> Transport<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    /// Writes a message with the appropriate headers.
    pub fn send(&mut self, message: &Value) -> io::Result<()> {
        write_message(&mut self.writer, message)
    }

    /// Reads one message, parsing its headers.
    pub fn receive(&mut self) -> io::Result<Value> {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = self.reader.read_line(&mut line)?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed",
                ));
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break; // end of headers
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                content_length = rest.trim().parse::<usize>().ok();
            }
            // Content-Type and other headers are ignored.
        }

        let len = content_length
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
        let mut buf = vec![0u8; len];
        self.reader.read_exact(&mut buf)?;
        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// Encodes a message body with LSP headers (used for tests and diagnostics).
pub fn encode_message(message: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(message).expect("serialize");
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out
}

/// Writes a single framed message to any writer.
pub fn write_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_over_an_in_memory_pipe() {
        let wire = encode_message(&json!({"jsonrpc": "2.0", "id": 1, "method": "x"}));
        let mut t = Transport::new(&wire[..], Vec::new());
        let msg = t.receive().unwrap();
        assert_eq!(msg["method"], "x");
        assert_eq!(msg["id"], 1);
    }

    #[test]
    fn reads_consecutive_messages() {
        let mut wire = encode_message(&json!({"id": 1}));
        wire.extend(encode_message(&json!({"id": 2})));
        let mut t = Transport::new(&wire[..], Vec::new());
        assert_eq!(t.receive().unwrap()["id"], 1);
        assert_eq!(t.receive().unwrap()["id"], 2);
    }

    #[test]
    fn reports_missing_length() {
        let wire = b"Content-Type: application/vscode-jsonrpc\r\n\r\n".to_vec();
        let mut t = Transport::new(&wire[..], Vec::new());
        assert!(t.receive().is_err());
    }

    #[test]
    fn writes_framed_output() {
        let mut t = Transport::new(&b""[..], Vec::new());
        t.send(&json!({"id": 7})).unwrap();
        let out = t.writer;
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("Content-Length: "));
        assert!(text.contains("\r\n\r\n"));
        assert!(text.ends_with(r#"{"id":7}"#));
    }
}
