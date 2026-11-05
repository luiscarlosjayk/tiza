//! A step that types plays its moves in their turn: the line of a later
//! patch keeps its old text until its typing starts, an annotation of an
//! earlier step moves when the move that shifts it comes, and the cursor
//! goes back to its line when the typing ends.

use std::time::{Duration, Instant};

use tiza_core::model::Document;
use tiza_qa::scratch;
use tiza_tui::app::App;

#[test]
fn the_line_of_a_later_typed_patch_keeps_its_text_until_its_typing() {
    let mut doc = Document::from_code(
        "def main():\n    x = compute(1, 2)\n    print(\"x =\", x)\n    return x\n\ndef compute(a, b):\n    return a + b\n",
        "py",
        None,
    );
    for m in [
        serde_json::json!({"at_step": 1, "action": "code.patch", "op": "insert", "after": "x = compute(1, 2)",
                           "content": "    msg = \"hola (mundo\"", "typing": "char", "speed_ms": 100}),
        serde_json::json!({"at_step": 1, "action": "wait", "ms": 800}),
        serde_json::json!({"at_step": 1, "action": "code.patch", "op": "replace", "find": "return a + b",
                           "content": "    return (a + b) * 2", "typing": "char", "speed_ms": 100}),
    ] {
        doc.add_move(serde_json::from_value(m).unwrap()).unwrap();
    }
    doc.step = 0;
    let mut app = App::new(doc, scratch("later-patch-waits").join("doc.tiza"));
    app.autosave = false;
    app.present = true;
    let mut now = Instant::now();
    app.next_step();
    assert!(app.is_typing());
    // The first patch types and the `wait` runs: 2.2 s + 0.8 s.
    let mut seen = Vec::new();
    for _ in 0..2900 {
        now += Duration::from_millis(1);
        app.typing_tick_at(now);
        let text = app.layout(100).canvas.to_plain_text();
        if text.contains("msg = \"h") || text.contains("msg = \"hola (mundo\"") {
            seen.push(text);
        }
    }
    assert!(!seen.is_empty(), "the first patch never showed");
    for (k, t) in seen.iter().enumerate() {
        assert!(
            t.contains("return a + b"),
            "frame {k} of the first patch and the wait: the line of the second patch is not there:\n{t}"
        );
    }
}

/// Every frame of the typing of the step after `doc.step`, in presentation
/// mode, at 1 ms ticks.
fn typed_frames(doc: Document, name: &str) -> (App, Vec<String>) {
    let mut app = App::new(doc, scratch(name).join("doc.tiza"));
    app.autosave = false;
    app.present = true;
    let mut now = Instant::now();
    app.next_step();
    assert!(app.is_typing());
    let mut frames = Vec::new();
    while app.is_typing() {
        now += Duration::from_millis(10);
        if app.typing_tick_at(now) && app.is_typing() {
            frames.push(app.layout(100).canvas.to_plain_text());
        }
    }
    (app, frames)
}

#[test]
fn a_box_of_an_earlier_step_stays_on_its_line_while_the_step_types() {
    let doc = tiza_core::fs::parse_document(
        &serde_json::json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1}, "step": 1,
            "code": ["fn main() {", "    let r = dangle();", "}", "", "fn dangle() -> &String {",
                     "    let s = String::from(\"hello\");", "    &s", "}"],
            "annotations": [{"id": "b1", "step": 1, "type": "box", "from_line": 7, "to_line": 7}],
            "moves": [
                {"at_step": 2, "action": "code.patch", "op": "replace", "find": "fn main() {",
                 "content": "fn main() { // go", "typing": "char", "speed_ms": 50},
                {"at_step": 2, "action": "wait", "ms": 500},
                {"at_step": 2, "action": "code.patch", "op": "insert", "after": "let s",
                 "content": "    let t = 1;", "typing": "char", "speed_ms": 50}
            ]
        })
        .to_string(),
        None,
    )
    .unwrap();
    let (_, frames) = typed_frames(doc, "box-in-its-turn");
    // Both stages are there: before the insert and while it types.
    assert!(frames.iter().any(|t| !t.contains("let t")));
    assert!(frames.iter().any(|t| t.contains("let t")));
    for (k, t) in frames.iter().enumerate() {
        let rows: Vec<&str> = t.lines().collect();
        let at = rows.iter().position(|r| r.contains("&s")).unwrap();
        assert!(
            rows[at - 1].contains('╭') && rows[at + 1].contains('╰'),
            "frame {k}: the box is not around `&s`:\n{t}"
        );
    }
}

#[test]
fn the_cursor_goes_back_to_its_line_when_the_typing_ends() {
    // A typed replace, then an insert at the end and a `cursor` move to a
    // line of that insert: while the replace types, that line is not there.
    let mut doc = Document::from_code("a = 1\nb = 2\nc = 3\n", "py", None);
    for m in [
        serde_json::json!({"at_step": 1, "action": "code.patch", "op": "replace", "find": "a = 1",
                           "content": "a = 10", "typing": "char", "speed_ms": 100}),
        serde_json::json!({"at_step": 1, "action": "code.patch", "op": "insert", "after": "c = 3",
                           "content": "d = 4\ne = 5"}),
        serde_json::json!({"at_step": 1, "action": "cursor", "line": 5, "col": 2}),
    ] {
        doc.add_move(serde_json::from_value(m).unwrap()).unwrap();
    }
    doc.step = 0;
    let mut app = App::new(doc, scratch("cursor-goes-back").join("doc.tiza"));
    app.autosave = false;
    let mut now = Instant::now();
    app.next_step();
    assert!(app.is_typing());
    assert_eq!(app.doc.code.len(), 3);
    assert_eq!(app.cursor_line, 3, "the cursor waits on the last line");
    while app.is_typing() {
        now += Duration::from_millis(50);
        app.typing_tick_at(now);
        assert!(app.cursor_line <= app.doc.last_line());
    }
    assert_eq!(
        (app.cursor_line, app.cursor_col, app.doc.code.len()),
        (5, 2, 5)
    );
}
