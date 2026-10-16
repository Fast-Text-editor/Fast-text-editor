//! End-to-end test of the LSP client against a mock stdio server.
//!
//! Skips gracefully if `python3` is unavailable.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use fast_text_lsp::LspClient;

const MOCK_SERVER: &str = r#"
import sys, json

def read_message():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.decode('utf-8').strip()
        if line == '':
            break
        k, _, v = line.partition(':')
        headers[k.strip().lower()] = v.strip()
    length = int(headers['content-length'])
    return json.loads(sys.stdin.buffer.read(length))

def send(msg):
    data = json.dumps(msg).encode('utf-8')
    sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n' % len(data))
    sys.stdout.buffer.write(data)
    sys.stdout.buffer.flush()

while True:
    msg = read_message()
    if msg is None:
        break
    method = msg.get('method')
    mid = msg.get('id')
    if method == 'initialize':
        send({'jsonrpc': '2.0', 'id': mid, 'result': {'capabilities': {}}})
    elif method == 'initialized':
        pass
    elif method == 'textDocument/didOpen':
        uri = msg['params']['textDocument']['uri']
        send({'jsonrpc': '2.0', 'method': 'textDocument/publishDiagnostics', 'params': {
            'uri': uri,
            'diagnostics': [{
                'range': {'start': {'line': 0, 'character': 0}, 'end': {'line': 0, 'character': 3}},
                'severity': 1, 'message': 'mock error', 'source': 'mock'
            }]
        }})
    elif method == 'textDocument/completion':
        send({'jsonrpc': '2.0', 'id': mid, 'result': {'items': [
            {'label': 'println', 'kind': 3, 'detail': 'fn'},
            {'label': 'print', 'kind': 3}
        ]}})
    elif method == 'textDocument/hover':
        send({'jsonrpc': '2.0', 'id': mid, 'result': {'contents': {'kind': 'plaintext', 'value': 'hover text'}}})
    elif method == 'textDocument/definition':
        send({'jsonrpc': '2.0', 'id': mid, 'result': {
            'uri': 'file:///tmp/other.rs',
            'range': {'start': {'line': 5, 'character': 0}, 'end': {'line': 5, 'character': 1}}
        }})
    elif method == 'shutdown':
        send({'jsonrpc': '2.0', 'id': mid, 'result': None})
    elif method == 'exit':
        break
    else:
        if mid is not None:
            send({'jsonrpc': '2.0', 'id': mid, 'result': None})
"#;

fn python_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn wait_for<F: Fn() -> bool>(f: F, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    f()
}

#[test]
fn talks_to_a_mock_language_server() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }

    let dir = std::env::temp_dir().join("fast-text-lsp-test");
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("mock_server.py");
    let mut f = std::fs::File::create(&script).unwrap();
    f.write_all(MOCK_SERVER.as_bytes()).unwrap();
    f.flush().unwrap();

    let uri = "file:///tmp/fast-text-lsp-test/demo.rs";
    let client = LspClient::spawn("python3", &[script.to_string_lossy().into_owned()], &dir)
        .expect("spawn mock server");

    // Handshake.
    let caps = client.initialize(Path::new("/tmp")).expect("initialize");
    assert!(caps.get("capabilities").is_some());

    // Open a document and wait for the diagnostic notification.
    client.did_open(uri, "rust", "fn main() {}").unwrap();
    assert!(
        wait_for(
            || !client.diagnostics_for(uri).is_empty(),
            Duration::from_secs(3)
        ),
        "expected diagnostics to arrive"
    );
    let diags = client.diagnostics_for(uri);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].message, "mock error");
    assert_eq!(diags[0].severity().label(), "error");
    assert!(diags[0].summary().contains("mock error"));

    // Request/response calls.
    let items = client.completion(uri, 0, 0).expect("completion");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].label, "println");
    assert_eq!(items[0].kind_badge(), "f");

    let hover = client.hover(uri, 0, 0).expect("hover");
    assert_eq!(hover.as_deref(), Some("hover text"));

    let def = client.definition(uri, 0, 0).expect("definition");
    let def = def.expect("a location");
    assert_eq!(def.uri, "file:///tmp/other.rs");
    assert_eq!(def.range.start.line, 5);

    client.shutdown();
}
