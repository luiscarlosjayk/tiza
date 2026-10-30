//! A key that a position does not know, over the socket.
//!
//! Acceptance criteria:
//! - AC1 Every method refuses a key that a position does not know: the
//!   `at`, `from` and `to` of `annotation.add`, `annotation.update` and
//!   `move.add`, the spans and the arrow ends of `scene.put` and
//!   `scene.patch`, and the `rebind` entries. A file with such a key opens
//!   (see `saving::format_compatibility` and `check::preflight`).
//! - AC2 The message names the key that the agent possibly means: one or
//!   two letters from it, or the name of the same field in a position
//!   (`col`) or a span (`start`). So does a method that is not about
//!   positions.

use serde_json::{Value, json};
use tiza_qa::{Live, fixture, scratch};

const CODE: &str = "fn main() {\n    let s = String::from(\"hi\");\n    takes(s);\n}\n";

fn refused(live: &Live, method: &str, params: Value) -> String {
    let r = live.api(method, params.clone());
    let e = r
        .error
        .unwrap_or_else(|| panic!("{method} {params} succeeded"));
    assert_eq!(e.code, "invalid_argument", "{method} {params}: {e:?}");
    e.message
}

#[test]
fn every_method_refuses_a_key_that_a_position_does_not_know() {
    let live = Live::from_code(CODE, "rs", "position-keys");
    live.api_ok(
        "annotation.add",
        json!({"id": "h1", "type": "highlight", "at": {"find": "String"}}),
    );
    let start = "`start` is `col` in a position.";
    let cases = [
        (
            "annotation.add",
            json!({"type": "highlight", "at": {"line": 2, "start": 4, "end": 7}}),
            start,
        ),
        (
            "annotation.add",
            json!({"type": "arrow", "from": {"line": 2, "colum": 8}, "to": {"line": 3, "col": 10}}),
            "Did you mean `col`?",
        ),
        (
            "annotation.update",
            json!({"id": "h1", "move": {"at": {"find": "takes", "ocurrence": 1}}}),
            "Did you mean `occurrence`?",
        ),
        (
            "move.add",
            json!({"at_step": 1, "action": "select", "from": {"line": 2, "start": 4}, "to": {"line": 2, "col": 9}}),
            start,
        ),
    ];
    let cases = cases.into_iter().chain([
        (
            "annotation.add",
            json!({"type": "arrow", "from": {"line": 2, "col": 8}, "to": {"line": 3, "column": 10}}),
            "Did you mean `col`?",
        ),
        (
            "annotation.update",
            json!({"id": "h1", "move": {"at": {"text": "takes"}}}),
            "`text` is `find` in a position.",
        ),
        (
            "annotation.update",
            json!({"id": "h1", "shape": {"type": "highlight", "at": {"line": 2, "start": 4}}}),
            start,
        ),
        (
            "move.add",
            json!({"at_step": 1, "action": "code.patch", "op": "replace", "find": "takes(s);",
                "content": "    takes(s.clone());", "rebind": {"h1": {"at": {"find": "s.clone", "colum": 1}}}}),
            "Did you mean `col`?",
        ),
    ]);
    for (method, params, hint) in cases {
        let m = refused(&live, method, params.clone());
        assert!(m.ends_with(hint), "{method} {params}: {m}");
    }
    // `scene.put` and `scene.patch`: the span and the ends of an arrow.
    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
    let fine = doc.clone();
    doc["annotations"][0]["span"]["col"] = json!(4);
    let m = refused(&live, "scene.put", json!({"document": doc}));
    assert!(m.ends_with("`col` is `start` in a span."), "{m}");
    let mut doc = fine.clone();
    doc["annotations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "a8", "step": 1, "type": "arrow",
        "from": {"line": 2, "col": 8, "start": 8}, "to": {"line": 3, "col": 10}}));
    let m = refused(&live, "scene.put", json!({"document": doc}));
    assert!(m.ends_with(start), "{m}");
    let m = refused(
        &live,
        "scene.patch",
        json!({"annotations": [{"id": "h9", "step": 1, "type": "highlight",
            "span": {"line": 2, "start": 4, "end": 7, "col": 4}}]}),
    );
    assert!(m.ends_with("`col` is `start` in a span."), "{m}");
    let m = refused(
        &live,
        "scene.patch",
        json!({"annotations": [{"id": "a9", "step": 1, "type": "arrow",
            "from": {"line": 2, "col": 8, "start": 8}, "to": {"line": 3, "col": 10}}]}),
    );
    assert!(m.ends_with(start), "{m}");
    // Nothing changed.
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    assert!(live.with(|a| a.doc.moves.is_empty()));
}

#[test]
fn a_method_that_is_not_about_positions_gives_the_hint_too() {
    let live = Live::from_code(CODE, "rs", "key-hint");
    let m = refused(&live, "scene.clear", json!({"stp": 1}));
    assert!(m.ends_with("Did you mean `step`?"), "{m}");
}

#[test]
fn tiza_scene_put_refuses_a_key_inside_a_position() {
    let dir = scratch("position-scene-put");
    let live = Live::open(
        tiza_core::fs::parse_document(CODE_DOC, None).unwrap(),
        dir.join("doc.tiza"),
        dir.join("s.sock"),
    );
    let out = tiza_qa::command()
        .args(["scene", "put"])
        .arg(fixture("compat/v1-newer-unknown-position-key.tiza"))
        .args(["--socket", live.socket.to_str().unwrap()])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(
        err.contains("4 positions have a key that this version of Tiza does not know"),
        "{err}"
    );
}

#[test]
fn the_text_hint_of_annotation_update_is_only_for_its_own_key() {
    // `text` of `annotation.update` itself: use `label`. Inside a position,
    // `text` is `find`, and the advice about `label` would be wrong.
    let live = Live::from_code(CODE, "rs", "text-hint");
    live.api_ok(
        "annotation.add",
        json!({"id": "n1", "type": "note", "line": 2, "text": "x"}),
    );
    let m = refused(&live, "annotation.update", json!({"id": "n1", "text": "y"}));
    assert!(m.contains("Use `label`"), "{m}");
    let m = refused(
        &live,
        "annotation.update",
        json!({"id": "n1", "move": {"at": {"text": "takes"}}}),
    );
    assert!(!m.contains("Use `label`"), "{m}");
}

const CODE_DOC: &str = r#"{"schema": 1, "source": {"language": "rs", "first_line": 1}, "code": ["fn main() {}"], "annotations": []}"#;
