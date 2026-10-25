//! The MCP tool `update_annotation` had no `file`: the socket's
//! `annotation.update` takes it (an annotation on a linked file, `null` for
//! the main file), but an agent on MCP could not move an annotation to
//! another file, and the tool refused the key.
//!
//! Acceptance criteria:
//! - AC1 `update_annotation` takes `file` as the socket does: absent keeps
//!   the annotation's file, `null` is the main file, a string sets that
//!   file (as a `code.link` names it), checked as the socket checks it.
//! - AC2 The tool's description says so, in STE.
//! - AC3 `text` on the tool says to use `label`, as the socket does.
//! - AC4 This file (absent, `null` and a string, in `scene.get` and in the
//!   autosaved `.tiza`); CHANGELOG (Fixed).
//!
//! Corner cases: a file no `code.link` shows is refused and nothing changes.

use std::path::PathBuf;

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, fixture, scratch};

const QUEUE: &str = "src/queue.rs";

/// t-231's demo (main.rs, `code.link` to src/queue.rs at steps 3 and 5)
/// opened from a scratch copy, and an MCP client on it.
fn open(name: &str) -> (Live, PathBuf, McpClient) {
    let dir = scratch(&format!("t-236-{name}"));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for f in ["main.rs", QUEUE, "demo.tiza"] {
        std::fs::copy(
            fixture(&format!("api/update_keeps_step_and_file/{f}")),
            dir.join(f),
        )
        .unwrap();
    }
    let tiza = dir.join("demo.tiza");
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza.clone(), dir.join("live.sock"));
    let mcp = McpClient::start(&live.socket);
    (live, tiza, mcp)
}

/// `id` as `scene.get` gives it and as the autosaved `.tiza` holds it.
fn both(live: &Live, tiza: &std::path::Path, id: &str) -> (Value, Value) {
    let find = |doc: &Value| {
        doc["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap_or_else(|| panic!("{id} in {doc}"))
            .clone()
    };
    let scene = live.api_ok("scene.get", json!({}))["document"].clone();
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap();
    (find(&scene), find(&saved))
}

/// The `file` of `id` in `scene.get` and in the `.tiza` (`None`: the
/// main file), the same in both.
fn file_of(live: &Live, tiza: &std::path::Path, id: &str) -> Option<String> {
    let (scene, saved) = both(live, tiza, id);
    assert_eq!(scene["file"], saved["file"], "{id}: {scene} / {saved}");
    scene["file"].as_str().map(str::to_string)
}

#[test]
fn step2_ac1_file_null_puts_it_on_the_main_file() {
    let (live, tiza, mut mcp) = open("null");
    assert_eq!(file_of(&live, &tiza, "q3").as_deref(), Some(QUEUE));
    live.api_ok("step.set", json!({"step": 3}));
    let (err, text, v) = mcp.call("update_annotation", json!({"id": "q3", "file": null}));
    assert!(!err, "{text} {v}");
    assert_eq!(file_of(&live, &tiza, "q3"), None, "{text}");
    // Step 3 shows src/queue.rs: the reply says why q3 is not drawn.
    assert!(
        text.contains("\"visible\": false") && text.contains("main.rs"),
        "{text}"
    );
}

#[test]
fn step3_ac1_a_string_puts_it_on_that_file() {
    let (live, tiza, mut mcp) = open("string");
    assert_eq!(file_of(&live, &tiza, "n0"), None);
    let (err, text, v) = mcp.call("update_annotation", json!({"id": "n0", "file": QUEUE}));
    assert!(!err, "{text} {v}");
    assert_eq!(file_of(&live, &tiza, "n0").as_deref(), Some(QUEUE));
}

#[test]
fn step4_ac1_absent_keeps_the_file() {
    let (live, tiza, mut mcp) = open("absent");
    let (err, text, v) = mcp.call("update_annotation", json!({"id": "q3", "color": "red"}));
    assert!(!err, "{text} {v}");
    assert_eq!(file_of(&live, &tiza, "q3").as_deref(), Some(QUEUE));
    let (scene, saved) = both(&live, &tiza, "q3");
    assert_eq!(scene["color"], "red", "{scene}");
    assert_eq!(saved["color"], "red", "{saved}");
}

#[test]
fn corner_a_file_that_is_no_view_is_refused() {
    let (_live, tiza, mut mcp) = open("nope");
    let before = std::fs::read_to_string(&tiza).unwrap();
    let (err, text, _) = mcp.call(
        "update_annotation",
        json!({"id": "m5", "label": "lost", "file": "src/nope.rs"}),
    );
    assert!(err, "{text}");
    assert!(
        text.contains("src/nope.rs") && text.contains("not a view"),
        "{text}"
    );
    assert_eq!(std::fs::read_to_string(&tiza).unwrap(), before);
}

#[test]
fn ac3_text_says_to_use_label() {
    let (_live, tiza, mut mcp) = open("text");
    let before = std::fs::read_to_string(&tiza).unwrap();
    let (err, text, _) = mcp.call("update_annotation", json!({"id": "n0", "text": "x"}));
    assert!(err, "{text}");
    assert!(
        text.contains("Use `label` to change the text of a note or the title of a callout."),
        "{text}"
    );
    assert_eq!(std::fs::read_to_string(&tiza).unwrap(), before);
}

#[test]
fn ac2_the_tool_names_file_and_not_text() {
    let (_live, _tiza, mut mcp) = open("schema");
    let tools = mcp.request("tools/list", json!({}));
    let tool = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "update_annotation")
        .unwrap()
        .clone();
    let props = &tool["inputSchema"]["properties"];
    assert!(props.get("file").is_some(), "{props}");
    assert!(props.get("text").is_none(), "{props}");
    let description = tool["description"].as_str().unwrap();
    assert!(description.contains("`file`"), "{description}");
}
