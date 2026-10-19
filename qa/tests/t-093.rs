//! t-093 — `update_annotation` / `annotation.update {move}` moves an annotation.
//!
//! Acceptance criteria:
//! - AC1 `move.at` re-anchors a highlight, mark or note by `find`/line/col,
//!   keeping id, kind, style, label, color and list position.
//! - AC2 `move.from` / `move.to` re-anchor an arrow; `from_line`/`to_line` a box,
//!   callout or spotlight (and `from_line` a note).
//! - AC3 A field that does not apply to the type fails with `invalid_argument`;
//!   nothing changes and the revision does not advance.
//! - AC4 A move whose result is invalid (`out_of_range`, `text_not_found`) is
//!   rejected and rolled back.
//! - AC5 The MCP tool rejects unknown fields instead of ignoring them.

use serde_json::json;
use tiza_qa::{Live, fixture, scratch};

fn live(name: &str) -> Live {
    let dir = scratch(name);
    let doc = tiza_core::fs::open(fixture("t-093/marks.tiza").to_str().unwrap())
        .unwrap()
        .document;
    Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"))
}

#[test]
fn ac1_move_a_mark_by_text_keeps_everything_else() {
    let l = live("t093-ac1");
    let before = l.api_ok("status", json!({}))["revision"].as_u64().unwrap();
    l.api_ok(
        "annotation.update",
        json!({"id": "m1", "move": {"at": {"line": 6, "find": "s: String"}}}),
    );
    let list = l.api_ok("scene.list", json!({}));
    let m1 = list.as_array().unwrap()[0].as_str().unwrap().to_string();
    assert!(m1.starts_with("m1 "), "still first in the list: {m1}");
    assert!(m1.contains("mark move L6 9..18"), "{m1}");
    assert!(m1.contains("wrong: points at takes"), "label kept: {m1}");
    let scene = l.api_ok("scene.get", json!({}));
    let ann = &scene["document"]["annotations"][0];
    assert_eq!(ann["style"]["glyph"], "🔥", "inline style kept");
    assert!(l.api_ok("status", json!({}))["revision"].as_u64().unwrap() > before);
}

#[test]
fn ac2_arrows_boxes_and_notes_move_by_their_own_fields() {
    let l = live("t093-ac2");
    l.api_ok(
        "annotation.update",
        json!({"id": "a1", "move": {"to": {"find": "println!"}}}),
    );
    l.api_ok(
        "annotation.update",
        json!({"id": "b1", "move": {"from_line": 1, "to_line": 4}}),
    );
    l.api_ok(
        "annotation.update",
        json!({"id": "n1", "move": {"from_line": 2}}),
    );
    let list = l.api_ok("scene.list", json!({}));
    let text = list
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("arrow L2:8 → L7:4"), "{text}");
    assert!(text.contains("box L1-4"), "{text}");
    assert!(text.contains("note L2"), "{text}");
}

#[test]
fn ac3_inapplicable_fields_fail_and_change_nothing() {
    let l = live("t093-ac3");
    let before = l.api_ok("scene.get", json!({}));
    for (id, mv) in [
        ("m1", json!({"from_line": 2})),
        ("b1", json!({"at": {"find": "takes"}})),
        ("a1", json!({"to_line": 2})),
        ("m1", json!({})),
    ] {
        let r = l.api("annotation.update", json!({"id": id, "move": mv}));
        assert_eq!(
            r.error.as_ref().map(|e| e.code.as_str()),
            Some("invalid_argument"),
            "{id}: {r:?}"
        );
    }
    let after = l.api_ok("scene.get", json!({}));
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(before["document"], after["document"]);
}

#[test]
fn ac4_invalid_results_are_rolled_back() {
    let l = live("t093-ac4");
    let r = l.api(
        "annotation.update",
        json!({"id": "m1", "move": {"at": {"find": "no such text"}}}),
    );
    assert_eq!(r.error.unwrap().code, "text_not_found");
    let r = l.api(
        "annotation.update",
        json!({"id": "b1", "move": {"to_line": 999}}),
    );
    assert_eq!(r.error.unwrap().code, "out_of_range");
    let list = l.api_ok("scene.list", json!({}));
    let text = list
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("mark move L3 4..9"), "{text}");
    assert!(text.contains("box L6-8"), "{text}");
}

#[test]
fn ac5_mcp_rejects_unknown_fields() {
    let l = live("t093-ac5");
    let msgs = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"qa","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"update_annotation","arguments":{"id":"m1","span":{"line":3}}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"update_annotation","arguments":{"id":"m1","at":{"line":6,"find":"s: String"}}}}),
    ];
    let input = msgs
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut child = std::process::Command::new(tiza_qa::bin())
        .args(["mcp", "--socket", l.socket.to_str().unwrap()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let replies: Vec<serde_json::Value> = stdout
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let r2 = replies.iter().find(|r| r["id"] == 2).unwrap();
    let r3 = replies.iter().find(|r| r["id"] == 3).unwrap();
    // rmcp reports parameter errors as a tool result with isError (not a JSON-RPC error).
    assert_eq!(r2["result"]["isError"], true, "{r2}");
    assert!(
        r2["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .contains("unknown field `span`, expected one of `id`"),
        "{r2}"
    );
    assert!(
        r3["result"]["isError"] != true && r3["error"].is_null(),
        "{r3}"
    );
    let list = l.api_ok("scene.list", json!({}));
    assert!(
        list.as_array().unwrap()[0]
            .as_str()
            .unwrap()
            .contains("L6 9..18")
    );
}
