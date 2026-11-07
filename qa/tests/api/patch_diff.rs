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

/// An instance at step 1 of a `delete` with `diff` of line 2 (`b();`).
fn deleted(name: &str) -> Live {
    let live = Live::from_code("a();\nb();\nc();\n", "rs", name);
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "find": "b();", "diff": true}),
    );
    live.api_ok("step.set", json!({"step": 1}));
    live
}

#[test]
fn code_get_gives_the_removed_lines_apart_from_the_code() {
    let live = deleted("api-diff-code-get");
    let v = live.api_ok("code.get", json!({}));
    // The code of the step: two lines, b is not in it.
    let code = v["text"].as_str().unwrap();
    assert!(!code.contains("b();"), "{code}");
    assert_eq!(v["last_line"], 2);
    assert_eq!(
        v["removed"],
        json!([{"line": 2, "place": "before", "lines": ["b();"]}])
    );
    // The screen shows it, over line 2.
    let shown = text(&live);
    assert!(shown.contains("b();"), "{shown}");
    // The document keeps its code: the removed line is not saved.
    let doc = live.api_ok("scene.get", json!({}))["document"].clone();
    assert_eq!(doc["code"].as_array().map(Vec::len), Some(3), "{doc}");
    // Step 0 and the undo of the move: no removed lines.
    live.api_ok("step.set", json!({"step": 0}));
    assert_eq!(live.api_ok("code.get", json!({}))["removed"], json!([]));
    live.api_ok("step.set", json!({"step": 1}));
    live.command("undo");
    assert_eq!(live.api_ok("code.get", json!({}))["removed"], json!([]));
}

#[test]
fn a_find_of_a_removed_line_says_why_it_matches_nothing() {
    let live = deleted("api-diff-find");
    let r = live.api(
        "annotation.add",
        json!({"type": "highlight", "at": {"find": "b();"}}),
    );
    let e = r.error.expect("refused");
    assert_eq!(e.code, "text_not_found", "{}", e.message);
    assert!(
        e.message
            .contains("that text is a removed line of the diff at step 1"),
        "{}",
        e.message
    );
    // A text that is nowhere keeps the plain message.
    let r = live.api(
        "annotation.add",
        json!({"type": "highlight", "at": {"find": "zzz"}}),
    );
    let e = r.error.expect("refused");
    assert_eq!(e.code, "text_not_found");
    assert!(!e.message.contains("removed line"), "{}", e.message);
}
