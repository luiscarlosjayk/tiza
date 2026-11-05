//! A step with two typed patches and a `wait` between them: the line of the
//! second patch keeps its old text until its typing starts.

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
