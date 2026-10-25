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
