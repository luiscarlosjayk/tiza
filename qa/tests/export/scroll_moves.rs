//! A `scroll` move in an export: an export shows the whole take, so the
//! `scroll` moves of a step that types change no frame, and a step with
//! only a `scroll` adds one frame, a pause, the same as the frame before.

use serde_json::{Value, json};
use tiza_qa::scratch;

/// Twenty lines; step 2 types two lines, step 3 types one more.
fn document(moves: Value) -> Value {
    let code: Vec<String> = (1..=20).map(|i| format!("let v{i} = {i};")).collect();
    json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code,
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 2, "text": "start"},
            {"id": "b3", "step": 3, "type": "box", "from_line": 15, "to_line": 16, "effect": "draw"}
        ],
        "moves": moves
    })
}

fn typed() -> Vec<Value> {
    vec![
        json!({"at_step": 2, "action": "code.patch", "op": "insert", "position": "10",
               "content": "let a = 1;\nlet b = 2;", "typing": "line", "speed_ms": 40}),
        json!({"at_step": 3, "action": "code.patch", "op": "insert", "position": "18",
               "content": "let c = 3;", "typing": "line", "speed_ms": 40}),
    ]
}

/// The GIF of `doc` and the line `tiza render` wrote about it.
fn gif(name: &str, doc: Value) -> (Vec<u8>, String) {
    let dir = scratch(name);
    let path = dir.join("doc.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let out = dir.join("x.gif");
    let o = tiza_qa::command()
        .args([
            "render",
            path.to_str().unwrap(),
            "--scale",
            "1",
            "--out",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "{err}");
    (std::fs::read(&out).unwrap(), err)
}

fn frames(wrote: &str) -> u32 {
    let n = wrote.split(" frame").next().unwrap();
    n.rsplit(' ').next().unwrap().parse().unwrap()
}

#[test]
fn a_scroll_in_a_step_that_types_changes_no_frame() {
    let mut with = typed();
    // Before the typing of step 2, after the typing of step 3, and one
    // that goes to the box of step 3.
    with.insert(
        0,
        json!({"at_step": 2, "action": "scroll", "line": 14, "align": "top"}),
    );
    with.push(json!({"at_step": 3, "action": "scroll", "annotation": "b3", "align": "bottom"}));
    let (plain, _) = gif("scroll-export-plain", document(json!(typed())));
    let (scrolled, _) = gif("scroll-export-scrolled", document(json!(with)));
    assert!(plain == scrolled, "a scroll changed the GIF");
}

#[test]
fn a_step_with_only_a_scroll_is_a_pause() {
    let mut with = typed();
    with.push(json!({"at_step": 4, "action": "scroll", "line": 18}));
    let (_, plain) = gif("scroll-export-no-camera", document(json!(typed())));
    let (_, camera) = gif("scroll-export-camera", document(json!(with)));
    assert_eq!(frames(&camera), frames(&plain) + 1, "{plain}\n{camera}");
}
