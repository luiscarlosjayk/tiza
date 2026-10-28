//! The enter effect of an annotation (experimental) over the socket:
//! `annotation.add` sets it, `annotation.update` keeps it when the update
//! has no `effect` and removes it with `null`, and `draw` on a shape with
//! no stroke is refused. An annotation from a document with `draw` and no
//! stroke loads, and other updates of it still work.

use serde_json::{Value, json};
use tiza_qa::{Live, fixture};

const CODE: &str = "fn main() {\n    let s = String::from(\"hi\");\n    println!(\"{s}\");\n}\n";

/// The `effect` of annotation `id` in the scene, `null` when it has none.
fn effect_of(live: &Live, id: &str) -> Value {
    live.api_ok("scene.get", json!({}))["document"]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap_or_else(|| panic!("no {id}"))
        .get("effect")
        .cloned()
        .unwrap_or(Value::Null)
}

/// The error of a call that must fail, as `code: message`.
fn refused(live: &Live, method: &str, params: Value) -> String {
    let r = live.api(method, params);
    let e = r
        .error
        .unwrap_or_else(|| panic!("{method} must fail: {:?}", r.result));
    format!("{}: {}", e.code, e.message)
}

#[test]
fn effect_is_set_kept_and_removed_by_add_and_update() {
    let live = Live::from_code(CODE, "rs", "effect-set");
    live.api_ok(
        "annotation.add",
        json!({"id": "e1", "type": "box", "from_line": 1, "to_line": 3, "effect": "draw"}),
    );
    assert_eq!(effect_of(&live, "e1"), "draw");
    live.api_ok("annotation.update", json!({"id": "e1", "color": "red"}));
    assert_eq!(
        effect_of(&live, "e1"),
        "draw",
        "an update without effect keeps it"
    );
    live.api_ok("annotation.update", json!({"id": "e1", "effect": null}));
    assert_eq!(effect_of(&live, "e1"), Value::Null, "null removes it");
}

#[test]
fn draw_on_a_shape_without_a_stroke_is_refused() {
    let live = Live::from_code(CODE, "rs", "effect-refused");
    let e = refused(
        &live,
        "annotation.add",
        json!({"type": "note", "line": 2, "text": "x", "effect": "draw"}),
    );
    assert!(
        e.starts_with("invalid_argument: effect draw needs a stroke"),
        "{e}"
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "u1", "type": "highlight", "at": {"find": "println"}, "emphasis": "underline", "effect": "draw"}),
    );
    let e = refused(
        &live,
        "annotation.update",
        json!({"id": "u1", "emphasis": "fill"}),
    );
    assert!(
        e.starts_with("invalid_argument: effect draw needs a stroke"),
        "{e}"
    );
    assert_eq!(effect_of(&live, "u1"), "draw");
}

#[test]
fn an_annotation_with_draw_and_no_stroke_from_a_file_can_be_edited() {
    let live = Live::from_code(CODE, "rs", "effect-file");
    let text = std::fs::read_to_string(fixture("shared/enter_effects/mixed.tiza")).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    live.api_ok("scene.put", json!({ "document": doc }));
    assert_eq!(effect_of(&live, "n1"), "draw");
    live.api_ok("annotation.update", json!({"id": "n1", "color": "red"}));
    assert_eq!(effect_of(&live, "n1"), "draw");
}
