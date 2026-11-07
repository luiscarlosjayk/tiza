//! `center` through the socket: `scene.patch` sets it and clears it,
//! `scene.get` hands it out with the schema it needs (2), and a document
//! put with `scene.put` draws centered at once. `render.text` draws what
//! the screen draws across.

use serde_json::{Value, json};
use tiza_qa::Live;

const CODE: &str = "fn main() {\n    let x = 1;\n}\n";

fn document(live: &Live) -> Value {
    live.api_ok("scene.get", json!({}))["document"].clone()
}

/// The blank columns before `fn main` in `render.text` at 60 columns.
fn indent(live: &Live) -> usize {
    let text = live.api_ok("render.text", json!({"width": 60}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    let row = text
        .lines()
        .find(|l| l.contains("fn main"))
        .unwrap_or_else(|| panic!("no code in {text:?}"));
    row.find("fn main").unwrap()
}

#[test]
fn a_patch_sets_center_and_a_get_hands_it_out_with_schema_2() {
    let live = Live::from_code(CODE, "rs", "center-patch");
    let before = indent(&live);
    assert_eq!(document(&live)["schema"], 1);
    assert_eq!(document(&live).get("center"), None);
    live.api_ok("scene.patch", json!({"center": true}));
    let doc = document(&live);
    assert_eq!(
        (doc["center"].clone(), doc["schema"].clone()),
        (json!(true), json!(2))
    );
    // 60 columns, the gutter (3) + 2 + 14 + 2 columns: 39 are left.
    assert!(
        indent(&live) >= before + 19,
        "{} after {before}",
        indent(&live)
    );
    live.api_ok("scene.patch", json!({"center": false}));
    assert_eq!(document(&live).get("center"), None);
    assert_eq!(document(&live)["schema"], 1);
    assert_eq!(indent(&live), before);
}

#[test]
fn a_put_with_center_draws_centered() {
    let live = Live::from_code(CODE, "rs", "center-put");
    let before = indent(&live);
    let mut doc = document(&live);
    doc["center"] = json!(true);
    live.api_ok("scene.put", json!({"document": doc}));
    assert!(indent(&live) > before);
    assert_eq!(document(&live)["center"], true);
}
