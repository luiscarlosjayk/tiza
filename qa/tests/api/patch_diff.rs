//! `diff` on a `code.patch` through the socket: `move.add` with `diff`
//! shows the `+` at its step, `undo` takes the move and its `+` away, and
//! `scene.get` gives the document with `diff` and the schema 2.
//! `scene.put` with a `diff` that is not a boolean is refused with
//! `invalid_argument`.

use serde_json::json;
use tiza_qa::Live;

/// The text of the step on screen at 40 columns.
fn text(live: &Live) -> String {
    live.api_ok("render.text", json!({"width": 40}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_move_with_diff_shows_its_sign_until_undo() {
    let live = Live::from_code("fn main() {\n    let a = 1;\n}\n", "rs", "api-diff");
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "find": "let a",
               "content": "    let a = 2;", "diff": true}),
    );
    live.api_ok("step.set", json!({"step": 1}));
    let shown = text(&live);
    assert!(shown.lines().any(|l| l.starts_with(" 2 + ")), "{shown}");
    let doc = live.api_ok("scene.get", json!({}))["document"].clone();
    assert_eq!(doc["schema"], 2);
    assert_eq!(doc["moves"][0]["diff"], true);
    live.command("undo");
    let shown = text(&live);
    assert!(!shown.contains('+'), "{shown}");
    assert!(shown.contains("let a = 1;"), "{shown}");
}

#[test]
fn a_put_with_a_diff_that_is_not_a_boolean_is_refused() {
    let live = Live::from_code("fn main() {\n    let a = 1;\n}\n", "rs", "api-diff-put");
    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
    doc["moves"] = json!([{"at_step": 1, "action": "code.patch", "op": "replace",
        "find": "let a", "content": "    let a = 2;", "diff": "yes"}]);
    let r = live.api("scene.put", json!({"document": doc}));
    let e = r.error.expect("refused");
    assert_eq!(e.code, "invalid_argument", "{}", e.message);
    assert!(e.message.contains("expected a boolean"), "{}", e.message);
}
