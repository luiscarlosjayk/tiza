//! t-189 — serde «invalid type» errors hid what the agent sent:
//! `invalid type: string «…», expected u32` instead of `"three"`.
//!
//! Acceptance criteria:
//! - AC1 `invalid type` errors from the socket, the MCP and loading a
//!   `.tiza` name the value that was sent (truncated to 40 characters with
//!   …), as serde writes it: `invalid type: string "three", expected u32`.
//! - AC2 Closed values stay as they are: never repeated over the socket or
//!   the MCP, the list of valid names instead.
//! - AC3 This file; the t-167 test that asserted no value for MCP
//!   arguments that do not deserialize now expects the wrong-typed value
//!   named (the others assert «…» for closed values, which keep it);
//!   CHANGELOG (Changed).
//!
//! Corner cases: a very long string value is truncated; a value with
//! control characters or newlines is escaped (serde writes it as Rust's
//! `{:?}`), never raw. A non-UTF-8 file does not load at all (`stream did
//! not contain valid UTF-8`) and quotes nothing.
//!
//! Review (`rev_*`): which kind of message it is comes from how serde's
//! text starts, so a closed value sent as `invalid type: …` stays hidden;
//! the cut cannot be dodged by a value that says `unknown variant` or `",
//! expected`; a load error is printed once.

use serde_json::json;
use tiza_qa::{Live, McpClient, bin, scratch};

fn error_message(live: &Live, method: &str, params: serde_json::Value) -> String {
    let r = live.api(method, params);
    r.error
        .unwrap_or_else(|| panic!("{method} should fail: {:?}", r.result))
        .message
}

#[test]
fn ac1_socket_names_the_value_sent() {
    // Test step 1.
    let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", "t189-socket");
    let m = error_message(
        &live,
        "annotation.add",
        json!({"type": "note", "line": "three", "text": "x"}),
    );
    assert!(
        m.contains("invalid type: string \"three\", expected u32"),
        "{m}"
    );
}

#[test]
fn ac1_corner_a_long_value_is_cut_and_controls_are_escaped() {
    let live = Live::from_code("fn main() {}\n", "rs", "t189-long");
    let value = format!("{}\n\u{1b}[31m", "x".repeat(60));
    let m = error_message(
        &live,
        "annotation.add",
        json!({"type": "note", "line": value, "text": "x"}),
    );
    assert!(
        m.contains(&format!("string \"{}…\", expected u32", "x".repeat(40))),
        "{m}"
    );
    assert!(!m.contains('\u{1b}') && !m.contains('\n'), "{m:?}");
    // Short enough to be whole: the escapes are shown, not sent.
    let m = error_message(
        &live,
        "annotation.add",
        json!({"type": "note", "line": "a\nb\u{1b}", "text": "x"}),
    );
    assert!(m.contains(r#"string "a\nb\u{1b}""#), "{m}");
    assert!(!m.contains('\u{1b}'), "{m:?}");
}

#[test]
fn ac2_closed_values_are_never_repeated() {
    // Test step 2.
    let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", "t189-closed");
    let m = error_message(
        &live,
        "annotation.add",
        json!({"type": "box", "from_line": 1, "to_line": 2, "border": "wavy"}),
    );
    assert!(m.contains("solid"), "{m}");
    assert!(!m.contains("wavy"), "{m}");
    let m = error_message(
        &live,
        "annotation.add",
        json!({"type": "boxy", "from_line": 1, "to_line": 2}),
    );
    assert!(!m.contains("boxy"), "{m}");
}

#[test]
fn ac1_mcp_names_the_value_sent() {
    let dir = scratch("t189-mcp");
    let sock = dir.join("live.sock");
    let doc = tiza_core::Document::from_code("fn main() {\n    let x = 1;\n}\n", "rs", None);
    let _live = Live::open(doc, dir.join("doc.tiza"), sock.clone());
    let mut mcp = McpClient::start(&sock);
    let (err, text, _) = mcp.call("add_note", json!({"line": "three", "text": "x"}));
    assert!(err, "{text}");
    assert!(text.contains("\"three\""), "{text}");
}

#[test]
fn ac1_loading_names_the_value_and_cuts_a_long_one() {
    let dir = scratch("t189-load");
    let file = dir.join("bad.tiza");
    let doc = |line: serde_json::Value| {
        json!({
            "schema": 1, "source": {"language": "rs"}, "code": ["fn main() {}"], "step": 0,
            "annotations": [{"id": "a1", "step": 0, "type": "note", "line": line, "text": "x"}]
        })
        .to_string()
    };
    std::fs::write(&file, doc(json!("three"))).unwrap();
    let out = std::process::Command::new(bin())
        .arg("check")
        .arg(&file)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(r#"string "three", expected u32"#), "{text}");

    std::fs::write(&file, doc(json!(format!("{}\n\u{1b}[31m", "y".repeat(80))))).unwrap();
    for args in [vec!["check"], vec!["render", "--out", "-"]] {
        let out = std::process::Command::new(bin())
            .args(&args)
            .arg(&file)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            text.contains(&format!("string \"{}…\", expected u32", "y".repeat(40))),
            "{args:?}: {text}"
        );
        assert!(!text.contains('\u{1b}'), "{args:?}: {text:?}");
    }
}

const SECRET: &str = "Q7secretXyz";

#[test]
fn rev_a_closed_value_that_says_invalid_type_stays_hidden() {
    let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", "t189-rev-closed");
    let v = format!("invalid type: {SECRET}");
    for (method, params) in [
        ("annotation.add", json!({"type": v, "line": 1})),
        (
            "annotation.add",
            json!({"type": "box", "from_line": 1, "to_line": 2, "border": v}),
        ),
        (
            "scene.patch",
            json!({"annotations": [{"id": "n9", "step": 0, "type": v, "line": 1}]}),
        ),
        (
            "move.add",
            json!({"at_step": 1, "action": "code.patch", "op": v, "position": "1", "content": "x"}),
        ),
    ] {
        let m = error_message(&live, method, params.clone());
        assert!(!m.contains(SECRET), "{method} {params}: {m}");
    }
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call(
        "add_move",
        json!({"at_step": 1, "op": v, "position": "1", "content": "x"}),
    );
    assert!(err, "{text}");
    assert!(!text.contains(SECRET), "{text}");
}

#[test]
fn rev_the_cut_cannot_be_dodged() {
    let live = Live::from_code("fn main() {}\n", "rs", "t189-rev-dodge");
    for tail in ["unknown variant `x", "\", expected u32 and more"] {
        let value = format!("{}{tail}", "A".repeat(300));
        let m = error_message(
            &live,
            "annotation.add",
            json!({"type": "note", "line": value, "text": "x"}),
        );
        assert!(!m.contains(&"A".repeat(41)), "{m}");
        assert!(m.contains(&format!("{}…", "A".repeat(40))), "{m}");
    }
}

#[test]
fn rev_a_load_error_is_printed_once() {
    let dir = scratch("t189-rev-once");
    let file = dir.join("long.tiza");
    let doc = json!({
        "schema": 1, "source": {"language": "rs"}, "code": ["fn main() {}"], "step": 0,
        "annotations": [{"id": "a1", "step": 0, "type": "note", "line": "y".repeat(80), "text": "x"}]
    });
    std::fs::write(&file, doc.to_string()).unwrap();
    let out = std::process::Command::new(bin())
        .args(["render", "--out", "-"])
        .arg(&file)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains(&"y".repeat(41)), "{err}");
    assert_eq!(err.matches("invalid type").count(), 1, "{err}");
}
