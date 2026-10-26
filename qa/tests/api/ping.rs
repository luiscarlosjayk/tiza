//! The local socket: `ping` answers the plain string `"pong"`
//! (test step 2 of the card; `status` carries `protocol`), and the raw
//! wire format is `{"id", "result"}` on one line.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde_json::json;
use tiza_qa::Live;

#[test]
fn ping_answers_pong_on_the_wire() {
    let live = Live::from_code("fn main() {}\n", "rs", "ping");
    assert_eq!(live.api_ok("ping", json!({})), json!("pong"));
    // Raw, like `printf … | nc -U $SOCKET`.
    let mut s = UnixStream::connect(&live.socket).unwrap();
    s.write_all(b"{\"id\":1,\"method\":\"ping\"}\n").unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(v, json!({"id": 1, "result": "pong"}), "{line}");
    // The protocol number lives in `status`.
    assert!(live.api_ok("status", json!({}))["protocol"].is_number());
}

/// An instance that takes the request and closes the connection with no
/// answer: `tiza api` says that, not `no Tiza instance is listening`,
/// which is for a socket that nobody listens on.
#[test]
fn a_closed_connection_is_not_a_missing_instance() {
    let dir = tiza_qa::scratch("closed-conn");
    let sock = dir.join("s.sock");
    let listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        // Read the request, then close with no answer.
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        drop(stream);
    });
    let o = tiza_qa::tiza(&["api", "--socket", sock.to_str().unwrap(), "status"]);
    server.join().unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success(), "{err}");
    assert!(
        err.contains(&format!(
            "the Tiza instance on {} closed the connection before it answered",
            sock.display()
        )) && !err.contains("no Tiza instance is listening"),
        "{err}"
    );
}
