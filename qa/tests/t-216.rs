//! `render.text` without a width laid the document out at the terminal's
//! whole width, in the screen's own layout, while `annotation.add` said
//! whether a label is drawn from the take at the width of the code area
//! (the terminal minus its padding). At some widths the text an agent got
//! back showed a note the reply called cut, or cut a note the reply called
//! whole.
//!
//! Acceptance criteria:
//! - AC1 Reproduced with a sweep of 90–150 columns on `four.env` and a note
//!   on line 2 (card note): without a width `render.text` disagreed with
//!   the reply at 94, 95, 98, 99, 129 and 130 columns, in presentation mode
//!   and out of it; at the code area's width it agreed everywhere.
//! - AC2 Without `width`, `render.text` (socket, MCP `render_text`, the
//!   `tiza://render` resources) is the take at the width `visible` and
//!   `check` use, so they always agree; with `width` nothing changes.
//! - AC3 The reply says the width it used (`width`); the agent guide, the
//!   MCP description and the API schema say so.
//! - AC4 This file; CHANGELOG.
//!
//! Corner cases: presentation mode and editing; a terminal narrower than
//! `check`'s narrowest width; no terminal size yet (the MCP at start).

use serde_json::{Value, json};
use tiza_qa::{Live, fixture, scratch};

const NOTE: &str = "the live token: rotate it right after the demo, never commit it again";

/// `four.env` and its `.tiza` (a callout on line 1) in a scratch folder,
/// on a terminal `cols` wide, in presentation mode or not.
fn opened(name: &str, cols: u16, present: bool) -> Live {
    let dir = scratch(name);
    for f in ["four.env", "four.env.tiza"] {
        std::fs::copy(fixture(&format!("t-216/{f}")), dir.join(f)).unwrap();
    }
    let path = dir.join("four.env.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path, dir.join("live.sock"));
    live.with(move |a| {
        a.size = (cols, 30);
        if present {
            a.set_present(true);
        }
    });
    live
}

/// The note's words found in order in `text` (it wraps across rows of the
/// margin): all of them.
fn whole(text: &str) -> bool {
    let mut rest = text;
    NOTE.split(' ').all(|w| match rest.find(w) {
        Some(i) => {
            rest = &rest[i + w.len()..];
            true
        }
        None => false,
    })
}

/// The reply of adding the note: drawn whole, as it says.
fn says_whole(r: &Value) -> bool {
    r["visible"] == json!(true) && r.get("clipped").is_none() && r.get("hidden").is_none()
}

#[test]
fn step1_step2_ac1_ac2_render_text_agrees_with_visible_and_check_at_every_width() {
    for present in [false, true] {
        for cols in 90..=150u16 {
            let live = opened(&format!("t216-{present}-{cols}"), cols, present);
            let r = live.api_ok(
                "annotation.add",
                json!({"type": "note", "line": 2, "text": NOTE, "step": 1}),
            );
            let id = r["id"].as_str().unwrap().to_string();
            let render = live.api_ok("render.text", json!({}));
            let text = render["text"].as_str().unwrap();
            assert_eq!(
                whole(text),
                says_whole(&r),
                "{present} {cols}: render.text vs {r}\n{text}"
            );
            let width = render["width"].as_u64().expect("the width it used");
            assert_eq!(
                width,
                live.with(|a| a.screen_width()) as u64,
                "{present} {cols}"
            );
            // The live check at that width says the same of the note.
            let check = live.api_ok("check", json!({"width": width}));
            let cut = check["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["code"] == "clipped_label" && f["id"] == json!(id));
            assert_eq!(cut, !whole(text), "{present} {cols}: check {check}");
        }
    }
}

#[test]
fn ac2_with_a_width_nothing_changes() {
    let live = opened("t216-width", 117, false);
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 2, "text": NOTE, "step": 1}),
    );
    for w in [60, 117, 140] {
        let r = live.api_ok("render.text", json!({"width": w}));
        let want = live.with(move |a| a.layout(w).canvas.to_plain_text());
        assert_eq!(r["text"].as_str().unwrap(), want, "{w}");
        assert_eq!(r["width"], json!(w));
    }
}

#[test]
fn corner_a_terminal_narrower_than_checks_narrowest_width() {
    let live = opened("t216-narrow", 12, false);
    let r = live.api_ok("render.text", json!({}));
    assert_eq!(
        r["width"].as_u64().unwrap(),
        tiza_core::layout::MIN_WIDTH as u64,
        "{r}"
    );
    assert!(!r["text"].as_str().unwrap().is_empty());
}

#[test]
fn corner_no_terminal_size_yet() {
    // An MCP client may ask before the TUI has drawn a frame.
    let live = opened("t216-nosize", 0, false);
    live.with(|a| a.size = (0, 0));
    let r = live.api_ok("render.text", json!({}));
    assert_eq!(
        r["width"].as_u64().unwrap(),
        tiza_core::check::DEFAULT_WIDTH as u64,
        "{r}"
    );
}

#[test]
fn ac3_docs_schema_and_changelog() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |f: &str| std::fs::read_to_string(root.join(f)).unwrap();
    let schema: Value = serde_json::from_str(&read("schema/api.schema.json")).unwrap();
    let text = schema.to_string();
    let at = text
        .find("\"render.text\"")
        .expect("render.text in the schema");
    assert!(text[at..].contains("\"width\""), "the reply's width");
    assert!(read("docs/agent-guide.md").contains("`render_text` without a `width`"));
    assert!(read("CHANGELOG.md").contains("`render.text` without a width"));
}
