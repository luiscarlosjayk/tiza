//! The `[defaults]` of the config file seen from the socket: `status` gives
//! the values that apply, the document only what it sets itself, and
//! `numbers` is never written as `null`.

use serde_json::{Value, json};
use tiza_qa::Live;

#[test]
fn status_gives_the_gutter_in_effect_and_the_document_only_its_own() {
    let live = Live::from_code("fn main() {}\n", "rs", "config-agents");
    live.with(|a| a.config_defaults.numbers = Some(false));
    assert_eq!(live.api_ok("status", json!({}))["numbers"], json!(false));
    let scene = live.api_ok("scene.get", json!({}));
    assert!(scene["document"].get("numbers").is_none(), "{scene}");
    // What the agents' renders draw follows the screen.
    let text = live.api_ok("render.text", json!({"width": 60}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!text.contains(" 1 "), "{text}");
}

#[test]
fn numbers_is_never_written_as_null() {
    let live = Live::from_code("fn main() {}\n", "rs", "config-null");
    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
    doc["numbers"] = Value::Null;
    // A `null` sent in is taken as absent.
    live.api_ok("scene.put", json!({"document": doc}));
    let scene = live.api_ok("scene.get", json!({}));
    assert!(scene["document"].get("numbers").is_none(), "{scene}");
    live.command("numbers off");
    live.command("undo");
    let saved: Value = live.with(|a| serde_json::to_value(&a.doc).unwrap());
    assert!(saved.get("numbers").is_none(), "{saved}");
    // The schema says a boolean, as an older Tiza reads it.
    let schema: Value = serde_json::from_str(&tiza_qa::tiza_ok(&["schema", "document"])).unwrap();
    assert_eq!(schema["properties"]["numbers"]["type"], json!("boolean"));
}
