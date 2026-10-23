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
//!   `check` use, so they always agree; with `width` nothing changes (until
//!   t-224, which makes it the take at that width too).
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
fn ac2_with_a_width_the_reply_names_it() {
    // (With a width it is the take at that width since t-224, which
    // compares it with `tiza render`.)
    let live = opened("t216-width", 117, false);
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 2, "text": NOTE, "step": 1}),
    );
    for w in [60, 117, 140] {
        let r = live.api_ok("render.text", json!({"width": w}));
        assert!(!r["text"].as_str().unwrap().is_empty(), "{w}");
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
    assert!(
        read("docs/agent-guide.md")
            .contains("without a `width`, at the width of the screen's code area")
    );
    // (The CHANGELOG of 0.1.0 was condensed in t-217.)
    assert!(read("CHANGELOG.md").contains("Without a width, `render.text`"));
}

// ---------- Review ----------

fn opened_doc(name: &str, doc: Value, cols: u16) -> Live {
    let dir = scratch(name);
    std::fs::copy(fixture("t-216/four.env"), dir.join("four.env")).unwrap();
    let path = dir.join("four.env.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path, dir.join("live.sock"));
    live.with(move |a| a.size = (cols, 30));
    live
}

#[test]
fn rev_during_a_typing_animation_the_step_as_it_is_once_typed() {
    let doc = json!({
        "schema": 1, "source": {"path": "four.env", "language": "env", "first_line": 1},
        "code": {"file": "four.env"}, "step": 0,
        "moves": [{"at_step": 1, "action": "code.patch", "op": "replace", "find": "PORT=8080",
                   "content": "PORT=9090", "typing": "char", "speed_ms": 1000}],
        "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 2, "text": NOTE}]
    });
    for cols in [100u16, 117, 140] {
        let live = opened_doc(&format!("t216-typing-{cols}"), doc.clone(), cols);
        live.api_ok("step.next", json!({}));
        assert!(live.with(|a| a.typing.is_some()), "the step is typing");
        let text = live.api_ok("render.text", json!({}))["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.contains("PORT=9090"), "{cols}: typed whole\n{text}");
        // The reply about a note added now says the same of it (a note's
        // text is its `label`: `text` is refused since t-235).
        let r = live.api_ok("annotation.update", json!({"id": "n1", "label": NOTE}));
        assert_eq!(whole(&text), says_whole(&r), "{cols}: {r}\n{text}");
    }
}

#[test]
fn rev_while_authoring_the_take_not_the_screens_own_layout() {
    // Arrows at step 2 take lanes on the left of the take; at step 1 the
    // authoring screen has none, so its margin is wider.
    let doc = json!({
        "schema": 1, "source": {"path": "four.env", "language": "env", "first_line": 1},
        "code": {"file": "four.env"}, "step": 1,
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 2, "text": NOTE},
            {"id": "r1", "step": 2, "type": "arrow", "from": {"line": 1, "col": 0}, "to": {"line": 4, "col": 0}},
            {"id": "r2", "step": 2, "type": "arrow", "from": {"line": 2, "col": 0}, "to": {"line": 4, "col": 1}},
            {"id": "r3", "step": 2, "type": "arrow", "from": {"line": 3, "col": 0}, "to": {"line": 1, "col": 1}}
        ]
    });
    let mut differ = 0;
    for cols in 80..=150u16 {
        let live = opened_doc(&format!("t216-lanes-{cols}"), doc.clone(), cols);
        let r = live.api_ok("annotation.update", json!({"id": "n1", "label": NOTE}));
        let render = live.api_ok("render.text", json!({}));
        let text = render["text"].as_str().unwrap();
        assert_eq!(whole(text), says_whole(&r), "{cols}: {r}\n{text}");
        let w = render["width"].as_u64().unwrap() as usize;
        let own = live.with(move |a| a.layout(w).canvas.to_plain_text());
        if whole(&own) != whole(text) {
            differ += 1;
        }
    }
    assert!(
        differ > 0,
        "the screen's own layout never differs here: no test"
    );
}

#[test]
fn rev_the_mcp_tool_names_the_width_on_a_second_line() {
    let live = opened("t216-mcp", 117, false);
    let mut mcp = tiza_qa::McpClient::start(&live.socket);
    let width = live.api_ok("render.text", json!({}))["width"]
        .as_u64()
        .unwrap();
    let (err, _, v) = mcp.call("render_text", json!({}));
    assert!(!err, "{v}");
    let second = v["result"]["content"][1]["text"].as_str().unwrap_or("");
    assert!(
        second.contains(&format!("take at {width} columns"))
            && second.contains(&format!("`width: {width}`")),
        "{v}"
    );
    // (The take at that width too since t-224.)
    let (_, _, v) = mcp.call("render_text", json!({"width": 90}));
    let second = v["result"]["content"][1]["text"].as_str().unwrap_or("");
    assert!(second.contains("take at 90 columns"), "{v}");
}
