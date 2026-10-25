//! Range anchors by two text markers (`find_start` / `find_end`):
//! the terminal version of Demo Time placeholders.
//!
//! Acceptance criteria:
//! - AC1 Boxes, callouts and spotlights accept `{"find_start": "…",
//!   "find_end": "…"}` instead of `from_line`/`to_line` on the socket and in
//!   the MCP tools (`add_box`, `add_callout`, `add_spotlight`); the range is
//!   resolved at creation (first occurrence of the start anywhere, first
//!   occurrence of the end at or after it) and stored as lines **plus** the
//!   markers (`markers: {start, end}` on the annotation) for the moves.
//! - AC2 A missing marker fails with `text_not_found` naming it (`start
//!   marker «…»` / `end marker «…» (after line N)`); an end marker that only
//!   exists before the start is `invalid_argument`; markers mixed with line
//!   numbers or given alone are `invalid_argument`.
//! - AC3 `:box @'// begin'..@'// end' label` in the TUI (quotes optional
//!   without spaces: `:box @begin..@end`), same for `:callout` and `:spot`.
//! - AC4 `scene.list` shows the resolved lines and the markers; the file
//!   round-trips with `markers`; moving the annotation by lines
//!   (`update_annotation {move: {from_line}}`) drops the markers.
//! - AC5 Markers may be code text, not only comments; the same marker text
//!   twice resolves to the first occurrence after the start; markers on the
//!   same line give a single-line range.

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, fixture};

fn live(name: &str) -> Live {
    let dir = tiza_qa::scratch(name);
    // A copy next to the sidecar, so the saved document resolves its code.
    std::fs::copy(
        fixture("api/range_markers/markers.rs"),
        dir.join("markers.rs"),
    )
    .unwrap();
    let doc = tiza_core::fs::open(dir.join("markers.rs").to_str().unwrap())
        .unwrap()
        .document;
    Live::open(doc, dir.join("markers.rs.tiza"), dir.join("live.sock"))
}

fn lines_of(live: &Live, id: &str) -> Vec<(u32, u32)> {
    live.with({
        let id = id.to_string();
        move |a| a.doc.find_annotation(&id).unwrap().shape.line_ranges()
    })
}

fn markers_of(live: &Live, id: &str) -> Option<(String, String)> {
    live.with({
        let id = id.to_string();
        move |a| {
            a.doc
                .find_annotation(&id)
                .unwrap()
                .markers
                .as_ref()
                .map(|m| (m.start.clone(), m.end.clone()))
        }
    })
}

#[test]
fn socket_and_mcp_resolve_markers_and_keep_them() {
    let l = live("socket");
    let r = l.api_ok(
        "annotation.add",
        json!({"id": "b", "type": "box", "find_start": "// begin", "find_end": "// end", "label": "ownership"}),
    );
    assert_eq!(r["id"], "b");
    assert_eq!(lines_of(&l, "b"), vec![(2, 5)]);
    assert_eq!(
        markers_of(&l, "b"),
        Some(("// begin".into(), "// end".into()))
    );

    l.api_ok(
        "annotation.add",
        json!({"id": "c", "type": "callout", "find_start": "fn takes", "find_end": "}", "text": "owns s"}),
    );
    // Code text as markers. The first `}` after `fn takes` is the one inside
    // `println!("{s}")` on line 10: a marker is plain text, so pick one that
    // only appears where you mean it.
    assert_eq!(lines_of(&l, "c"), vec![(9, 10)]);
    l.api_ok(
        "annotation.add",
        json!({"id": "s", "type": "spotlight", "find_start": "let s", "find_end": "takes(s)"}),
    );
    assert_eq!(lines_of(&l, "s"), vec![(3, 4)]);

    let mut mcp = McpClient::start(&l.socket);
    let (err, text, _) = mcp.call(
        "add_box",
        json!({"id": "m", "find_start": "fn len", "find_end": "s.len()", "label": "same line"}),
    );
    assert!(!err, "{text}");
    assert_eq!(
        lines_of(&l, "m"),
        vec![(13, 13)],
        "markers on one line: single-line box"
    );
    let (err, text, _) = mcp.call(
        "add_callout",
        json!({"id": "mc", "find_start": "fn takes", "find_end": "println", "text": "prints"}),
    );
    assert!(!err, "{text}");
    assert_eq!(lines_of(&l, "mc"), vec![(9, 10)]);
    let (err, text, _) = mcp.call(
        "add_spotlight",
        json!({"id": "ms", "find_start": "// begin", "find_end": "// end"}),
    );
    assert!(!err, "{text}");
    assert_eq!(lines_of(&l, "ms"), vec![(2, 5)]);
    // The MCP tool schema documents the fields.
    let tools = mcp.tools();
    let add_box = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "add_box")
        .unwrap();
    assert!(add_box["inputSchema"]["properties"]["find_start"].is_object());
    assert!(
        add_box["inputSchema"]["required"]
            .as_array()
            .is_none_or(|r| !r.iter().any(|x| x == "from_line")),
        "from_line is optional now"
    );
}

#[test]
fn errors_name_the_marker() {
    let l = live("errors");
    let e = l
        .api(
            "annotation.add",
            json!({"type": "box", "find_start": "// begin", "find_end": "// finish"}),
        )
        .error
        .unwrap();
    assert_eq!(e.code, "text_not_found");
    assert!(
        e.message.contains("end marker «// finish»") && e.message.contains("after line 2"),
        "{}",
        e.message
    );
    let e = l
        .api(
            "annotation.add",
            json!({"type": "box", "find_start": "// start", "find_end": "// end"}),
        )
        .error
        .unwrap();
    assert_eq!(e.code, "text_not_found");
    assert!(
        e.message.contains("start marker «// start»"),
        "{}",
        e.message
    );
    // The end exists, but only before the start: an invalid range, not a swap.
    let e = l
        .api(
            "annotation.add",
            json!({"type": "box", "find_start": "fn takes", "find_end": "let s"}),
        )
        .error
        .unwrap();
    assert_eq!(e.code, "invalid_argument");
    assert!(e.message.contains("appears only before"), "{}", e.message);
    // Mixed or lonely markers.
    let e = l
        .api(
            "annotation.add",
            json!({"type": "box", "from_line": 1, "find_start": "a", "find_end": "b"}),
        )
        .error
        .unwrap();
    assert_eq!(e.code, "invalid_argument");
    assert!(e.message.contains("not both"));
    let e = l
        .api(
            "annotation.add",
            json!({"type": "box", "find_start": "// begin"}),
        )
        .error
        .unwrap();
    assert!(e.message.contains("go together"), "{}", e.message);
    let e = l
        .api("annotation.add", json!({"type": "box"}))
        .error
        .unwrap();
    assert!(
        e.message
            .contains("from_line (or find_start/find_end) is required"),
        "{}",
        e.message
    );
    let e = l
        .api(
            "annotation.add",
            json!({"type": "box", "find_start": "", "find_end": "x"}),
        )
        .error
        .unwrap();
    assert_eq!(e.code, "invalid_argument");
    assert_eq!(l.with(|a| a.doc.annotations.len()), 0);
}

#[test]
fn tui_commands() {
    let l = live("tui");
    l.command("box @'// begin'..@'// end' ownership");
    let (msg, err) = l.message().unwrap();
    assert!(!err && msg.ends_with(": box 2-5"), "{msg}");
    let id = msg.split(':').next().unwrap().to_string();
    assert_eq!(
        markers_of(&l, &id),
        Some(("// begin".into(), "// end".into()))
    );
    let label = l.with({
        let id = id.clone();
        move |a| match &a.doc.find_annotation(&id).unwrap().shape {
            tiza_core::Shape::Box { label, .. } => label.clone(),
            _ => None,
        }
    });
    assert_eq!(label.as_deref(), Some("ownership"));
    // Bare markers without spaces, double quotes, and the other commands.
    l.command("box @takes(s)..@end");
    let (msg, err) = l.message().unwrap();
    assert!(!err && msg.ends_with(": box 4-5"), "{msg}");
    l.command("callout @\"fn takes\"..@'}' @below owns s and frees it");
    let (msg, err) = l.message().unwrap();
    assert!(!err && msg.contains("callout at L9-10"), "{msg}");
    l.command("spot @'let s'..@'takes(s)'");
    let (msg, err) = l.message().unwrap();
    assert!(!err && msg.contains("spotlight"), "{msg}");
    let last = l.with(|a| a.doc.annotations.last().unwrap().clone());
    assert_eq!(last.shape.line_ranges(), vec![(3, 4)]);
    assert!(last.markers.is_some());
    // Errors in the TUI.
    l.command("box @'// begin'..@'// nope'");
    let (msg, err) = l.message().unwrap();
    assert!(err && msg.contains("end marker «// nope»"), "{msg}");
    l.command("box @'// begin'");
    let (msg, err) = l.message().unwrap();
    assert!(err && msg.contains("joined by .."), "{msg}");
    l.command("box @'unclosed..@end");
    let (msg, err) = l.message().unwrap();
    assert!(err && msg.contains("missing closing"), "{msg}");
    l.command("box");
    let (msg, err) = l.message().unwrap();
    assert!(
        err && msg.contains("@'start'..@'end'"),
        "usage mentions markers: {msg}"
    );
}

#[test]
fn scene_list_round_trip_and_move_drops_markers() {
    let l = live("list");
    l.api_ok(
        "annotation.add",
        json!({"id": "b", "type": "box", "find_start": "// begin", "find_end": "// end", "label": "x"}),
    );
    let list = l.api_ok("scene.list", json!({}));
    let line = list[0].as_str().unwrap();
    assert!(line.contains("box L2-5"), "{line}");
    assert!(line.contains("@«// begin»..@«// end»"), "{line}");
    // Saved and reloaded: markers survive; the schema accepts them.
    l.command("w");
    let path = l.with(|a| a.save_path.clone());
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        saved["annotations"][0]["markers"],
        json!({"start": "// begin", "end": "// end"})
    );
    assert_eq!(saved["annotations"][0]["from_line"], 2);
    let reopened = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    assert!(reopened.annotations[0].markers.is_some());
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("schema/tiza.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        schema.to_string().contains("RangeMarkers"),
        "document schema knows markers"
    );
    // Moving by lines drops the markers (they no longer describe the range).
    l.api_ok(
        "annotation.update",
        json!({"id": "b", "move": {"from_line": 9, "to_line": 11}}),
    );
    assert_eq!(lines_of(&l, "b"), vec![(9, 11)]);
    assert_eq!(markers_of(&l, "b"), None);
    let list = l.api_ok("scene.list", json!({}));
    assert!(!list[0].as_str().unwrap().contains('@'));
    // Re-specifying the shape with markers sets them again.
    l.api_ok(
        "annotation.update",
        json!({"id": "b", "shape": {"type": "box", "find_start": "// begin", "find_end": "// end"}}),
    );
    assert_eq!(
        markers_of(&l, "b"),
        Some(("// begin".into(), "// end".into()))
    );
}

#[test]
fn repeated_marker_text_and_same_line() {
    let l = live("repeat");
    // `// begin` appears on lines 2, 6 and 13: the start is the first one;
    // an end of `// begin` is the first occurrence AFTER the start (line 6).
    l.api_ok(
        "annotation.add",
        json!({"id": "r", "type": "box", "find_start": "// begin", "find_end": "// begin"}),
    );
    assert_eq!(lines_of(&l, "r"), vec![(2, 6)]);
    // Same line, end after start: single line; end before start on the same
    // line does not count (it is searched after the start text).
    l.api_ok(
        "annotation.add",
        json!({"id": "one", "type": "box", "find_start": "fn len", "find_end": "// begin"}),
    );
    assert_eq!(lines_of(&l, "one"), vec![(13, 13)]);
    let e = l
        .api(
            "annotation.add",
            json!({"type": "box", "find_start": "s.len()", "find_end": "fn len"}),
        )
        .error
        .unwrap();
    assert_eq!(
        e.code, "invalid_argument",
        "fn len is before s.len() on the same line: {}",
        e.message
    );
}
