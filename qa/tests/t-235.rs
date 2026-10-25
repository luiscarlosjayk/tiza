//! `annotation.update` took keys it does not know in silence: `{"id": "n0",
//! "text": "x"}` answered OK and changed nothing (a note's text is its
//! `label`), so an agent believed the change was made.
//!
//! Acceptance criteria:
//! - AC1 An unknown key fails with `invalid_argument`, names the key and
//!   lists the accepted ones.
//! - AC2 `text` is refused like any unknown key, with one more sentence:
//!   use `label` to change the text of a note or the title of a callout,
//!   and a new `shape` for the text of a callout (no alias: on a callout
//!   `text` and `title` would be ambiguous).
//! - AC3 What the docs name keeps working: the agent guide's examples, the
//!   MCP tool `update_annotation`, the TUI commands that edit an annotation
//!   and `file`.
//! - AC4 The API schema says so (`additionalProperties: false`); CHANGELOG.
//!
//! Corner cases: a typo of a real key (`colour`) names the closest one;
//! `id` alone stays a change of nothing that answers OK.

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient};

const CODE: &str = "fn main() {\n    let a = 1;\n}\n";

fn live(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.api_ok(
        "annotation.add",
        json!({"id": "n0", "type": "note", "line": 2, "text": "one", "step": 1}),
    );
    live
}

fn note_text(live: &Live) -> String {
    let doc = live.api_ok("scene.get", json!({}))["document"].clone();
    doc["annotations"][0]["text"].as_str().unwrap().to_string()
}

/// The error of an `annotation.update` that must fail: (code, message).
fn refused(live: &Live, params: Value) -> (String, String) {
    let r = live.api("annotation.update", params.clone());
    let e = r
        .error
        .unwrap_or_else(|| panic!("{params} answered {:?}", r.result));
    (e.code, e.message)
}

#[test]
fn step1_ac1_an_unknown_key_is_refused_and_named() {
    let live = live("t235-unknown");
    let (code, msg) = refused(&live, json!({"id": "n0", "bogus": 1}));
    assert_eq!(code, "invalid_argument", "{msg}");
    assert!(msg.contains("unknown field `bogus`"), "{msg}");
    for accepted in ["`label`", "`color`", "`step`", "`move`", "`file`"] {
        assert!(msg.contains(accepted), "{accepted}: {msg}");
    }
    assert_eq!(note_text(&live), "one");
}

#[test]
fn step2_ac2_text_says_to_use_label() {
    let live = live("t235-text");
    let (code, msg) = refused(&live, json!({"id": "n0", "text": "x"}));
    assert_eq!(code, "invalid_argument", "{msg}");
    assert!(msg.contains("unknown field `text`"), "{msg}");
    assert!(
        msg.contains("Use `label` to change the text of a note or the title of a callout."),
        "{msg}"
    );
    assert_eq!(note_text(&live), "one", "nothing changed");
    // What the sentence says works.
    live.api_ok("annotation.update", json!({"id": "n0", "label": "x"}));
    assert_eq!(note_text(&live), "x");
}

#[test]
fn corner_a_typo_names_the_closest_key() {
    let live = live("t235-typo");
    let (_, msg) = refused(&live, json!({"id": "n0", "colour": "red"}));
    assert!(msg.contains("Did you mean `color`?"), "{msg}");
    // Far from every key: no guess.
    let (_, msg) = refused(&live, json!({"id": "n0", "xyzzy": 1}));
    assert!(!msg.contains("Did you mean"), "{msg}");
}

#[test]
fn corner_id_alone_still_answers_ok() {
    let live = live("t235-id");
    live.api_ok("annotation.update", json!({"id": "n0"}));
    assert_eq!(note_text(&live), "one");
}

#[test]
fn step3_ac3_what_the_docs_name_keeps_working() {
    let live = live("t235-docs");
    // The agent guide's socket example: a move.
    live.api_ok(
        "annotation.update",
        json!({"id": "n0", "move": {"at": {"line": 1}}}),
    );
    // Every other key the schema names, with `null` where it may be.
    live.api_ok(
        "annotation.update",
        json!({"id": "n0", "step": 1, "color": "", "label": "two", "file": null}),
    );
    assert_eq!(note_text(&live), "two");
    // The MCP tool, as the guide shows it.
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, v) = mcp.call(
        "update_annotation",
        json!({"id": "n0", "label": "three", "color": "green"}),
    );
    assert!(!err, "{text} {v}");
    assert_eq!(note_text(&live), "three");
    // A TUI command that edits an annotation.
    live.command("set n0 label=four");
    assert_eq!(note_text(&live), "four");
}

#[test]
fn ac4_the_schema_refuses_other_keys() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("schema/api.schema.json")).unwrap(),
    )
    .unwrap();
    let method = schema["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["method"] == "annotation.update")
        .expect("annotation.update in the schema");
    assert_eq!(
        method["params"]["additionalProperties"],
        json!(false),
        "{}",
        method["params"]
    );
}
