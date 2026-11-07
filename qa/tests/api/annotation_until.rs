//! `until` on each type of annotation through the socket. `annotation.add`
//! takes it and refuses one before the step, with the text of the error
//! `until` of `tiza check`. `annotation.update` changes it alone, removes
//! it with `null`, keeps it through the other fields, and replaces it
//! with a `shape`; `until` and a `shape` together are refused. The reply
//! of an annotation after its `until` says where it shows. `scene.patch`
//! keeps the `until` of an annotation sent without one.

use serde_json::{Value, json};
use tiza_qa::Live;

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";

fn document(live: &Live) -> Value {
    live.api_ok("scene.get", json!({}))["document"].clone()
}

fn annotation(live: &Live, id: &str) -> Value {
    document(live)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap_or_else(|| panic!("no {id}"))
        .clone()
}

/// The error of a call that must fail and change nothing, as `code: message`.
fn refused(live: &Live, method: &str, params: Value) -> String {
    let before = (document(live), live.with(|a| a.revision));
    let r = live.api(method, params);
    let e = r
        .error
        .unwrap_or_else(|| panic!("{method} must fail: {:?}", r.result));
    assert_eq!(
        (document(live), live.with(|a| a.revision)),
        before,
        "a refused {method} changed the document"
    );
    format!("{}: {}", e.code, e.message)
}

/// A live document with a note `n1` at step 1 until step 2, and a note at
/// step 4, so that the steps go to 4.
fn two_notes(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.api_ok(
        "annotation.add",
        json!({"id": "n1", "type": "note", "line": 2, "text": "a", "step": 1, "until": 2}),
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "n4", "type": "note", "line": 4, "text": "c", "step": 4}),
    );
    live
}

#[test]
fn an_add_takes_until_on_each_type() {
    let live = Live::from_code(CODE, "rs", "until-types");
    for (id, shape) in [
        ("h", json!({"type": "highlight", "at": {"find": "a = 1"}})),
        (
            "m",
            json!({"type": "mark", "kind": "read", "at": {"find": "b"}}),
        ),
        ("b", json!({"type": "box", "from_line": 2, "to_line": 3})),
        (
            "r",
            json!({"type": "arrow", "from": {"find": "a"}, "to": {"find": "c"}}),
        ),
        ("n", json!({"type": "note", "line": 3, "text": "x"})),
        ("c", json!({"type": "callout", "from_line": 4, "text": "y"})),
    ] {
        let mut p = shape;
        p["id"] = json!(id);
        p["step"] = json!(2);
        p["until"] = json!(3);
        live.api_ok("annotation.add", p);
        assert_eq!(annotation(&live, id)["until"], json!(3), "{id}");
    }
}

#[test]
fn an_add_with_until_before_its_step_is_refused() {
    let live = Live::from_code(CODE, "rs", "until-add-before");
    let e = refused(
        &live,
        "annotation.add",
        json!({"id": "b1", "type": "box", "from_line": 2, "step": 3, "until": 2}),
    );
    assert_eq!(
        e,
        "invalid_argument: b1: give until 3 or later, or remove it \
         (until 2 is before step 3: the box never shows)"
    );
}

#[test]
fn an_update_changes_removes_and_keeps_until() {
    let live = two_notes("until-update");
    live.api_ok("annotation.update", json!({"id": "n1", "until": 3}));
    assert_eq!(annotation(&live, "n1")["until"], json!(3));
    // Another field keeps it.
    live.api_ok("annotation.update", json!({"id": "n1", "color": "blue"}));
    assert_eq!(annotation(&live, "n1")["until"], json!(3));
    live.api_ok("annotation.update", json!({"id": "n1", "until": null}));
    assert!(annotation(&live, "n1").get("until").is_none());
    // Before its step: refused, as an add.
    live.api_ok("annotation.update", json!({"id": "n1", "step": 2}));
    let e = refused(&live, "annotation.update", json!({"id": "n1", "until": 1}));
    assert!(
        e.ends_with("(until 1 is before step 2: the note never shows)"),
        "{e}"
    );
}

#[test]
fn a_shape_replaces_until_and_does_not_go_with_it() {
    let live = two_notes("until-shape");
    let note = |until: Option<u32>| {
        let mut s = json!({"type": "note", "line": 3, "text": "moved"});
        if let Some(u) = until {
            s["until"] = json!(u);
        }
        s
    };
    // A shape with an `until`: the new one.
    live.api_ok(
        "annotation.update",
        json!({"id": "n1", "shape": note(Some(3))}),
    );
    assert_eq!(annotation(&live, "n1")["until"], json!(3));
    // A shape without one: the annotation has none after it.
    live.api_ok(
        "annotation.update",
        json!({"id": "n1", "shape": note(None)}),
    );
    assert!(annotation(&live, "n1").get("until").is_none());
    // Both: refused, with no change.
    let e = refused(
        &live,
        "annotation.update",
        json!({"id": "n1", "shape": note(Some(2)), "until": 3}),
    );
    assert_eq!(
        e,
        "invalid_argument: give until in the shape or alone, not both"
    );
}

#[test]
fn a_spotlight_shape_keeps_its_until() {
    // The `until` of a spotlight in its shape, as before.
    let live = two_notes("until-spot-shape");
    live.api_ok(
        "annotation.add",
        json!({"id": "s1", "type": "spotlight", "from_line": 2, "step": 1}),
    );
    live.api_ok(
        "annotation.update",
        json!({"id": "s1", "shape": {"type": "spotlight", "from_line": 3, "until": 3}}),
    );
    let s = annotation(&live, "s1");
    assert_eq!(
        (s["from_line"].clone(), s["until"].clone()),
        (json!(3), json!(3))
    );
}

#[test]
fn the_reply_says_where_one_after_its_until_shows() {
    let live = two_notes("until-hidden");
    live.api_ok("step.set", json!({"step": 3}));
    let r = live.api_ok("annotation.update", json!({"id": "n1", "color": "red"}));
    assert_eq!(r["visible"], json!(false), "{r}");
    assert_eq!(r["hidden"], json!("it shows only at steps 1-2"));
    let r = live.api_ok(
        "annotation.add",
        json!({"id": "n2", "type": "note", "line": 3, "text": "b", "step": 2, "until": 2}),
    );
    assert_eq!(r["hidden"], json!("it shows only at step 2"));
    // A highlight whose text a move removes after its `until`: the reason
    // is its `until`, not a lost anchor.
    live.api_ok(
        "annotation.add",
        json!({"id": "h1", "type": "highlight", "at": {"find": "b = 2"}, "step": 1, "until": 1}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "replace",
            "find": "let b = 2;", "content": "    let x = 2;"}),
    );
    let r = live.api_ok("annotation.update", json!({"id": "h1", "color": "red"}));
    assert_eq!(r["hidden"], json!("it shows only at step 1"), "{r}");
}

#[test]
fn a_patch_keeps_until_without_the_key() {
    let live = two_notes("until-patch");
    let n1 = |extra: Value| {
        let mut a = json!({"id": "n1", "type": "note", "line": 2, "text": "patched"});
        for (k, v) in extra.as_object().unwrap() {
            a[k] = v.clone();
        }
        json!({"annotations": [a]})
    };
    live.api_ok("scene.patch", n1(json!({})));
    let a = annotation(&live, "n1");
    assert_eq!(
        (a["text"].clone(), a["until"].clone()),
        (json!("patched"), json!(2))
    );
    live.api_ok("scene.patch", n1(json!({"until": 3})));
    assert_eq!(annotation(&live, "n1")["until"], json!(3));
    live.api_ok("scene.patch", n1(json!({"until": null})));
    assert!(annotation(&live, "n1").get("until").is_none());
}

#[test]
fn scene_get_has_the_schema_that_the_document_needs() {
    let live = two_notes("until-scene-get");
    assert_eq!(document(&live)["schema"], json!(2));
    live.api_ok("annotation.update", json!({"id": "n1", "until": null}));
    assert_eq!(document(&live)["schema"], json!(1));
}
