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
