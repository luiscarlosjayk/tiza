//! The MCP tools of a talk: `talk_status`, `goto_scene`, `insert_scene`
//! and `set_talk_scenes`, with a real client over a talk.
//!
//! Acceptance criteria:
//! - AC1 Each tool calls its socket method: an agent sees the talk, goes
//!   to a scene, adds a scene with a new file and orders the list, and
//!   `next_step` at the end of a scene says the next scene.

use std::path::Path;

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, fixture, scratch};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

fn ok(mcp: &mut McpClient, tool: &str, args: Value) -> Value {
    let (is_error, text, raw) = mcp.call(tool, args);
    assert!(!is_error, "{tool}: {text} {raw}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{tool}: {e}: {text}"))
}

#[test]
fn an_agent_sees_drives_and_writes_a_talk_with_the_talk_tools() {
    let dir = scratch("mcp-talk");
    copy_dir(&fixture("shared/talk"), &dir);
    let live = Live::open_talk(
        &dir.join("talk.tiza"),
        Some("own"),
        tiza_tui::SessionStart::default(),
        dir.join("s.sock"),
    );
    let mut mcp = McpClient::start(&live.socket);

    let t = ok(&mut mcp, "talk_status", json!({}));
    assert_eq!(t["position"], 3, "{t}");
    assert_eq!(t["scenes"].as_array().unwrap().len(), 7, "{t}");

    let r = ok(
        &mut mcp,
        "goto_scene",
        json!({"scene": "borrow", "step": "shared"}),
    );
    assert_eq!(r["scene"]["position"], 2, "{r}");
    assert_eq!(r["id"], "shared", "{r}");

    let doc: Value =
        serde_json::to_value(tiza_core::fs::load_document(&dir.join("2024.tiza")).unwrap())
            .unwrap();
    let r = ok(
        &mut mcp,
        "insert_scene",
        json!({"file": "end.tiza", "at": 8, "scene": doc, "name": "end"}),
    );
    assert_eq!(r["created"], "end.tiza", "{r}");
    assert!(dir.join("end.tiza").exists());

    let mut list: Vec<Value> = r["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["entry"].clone())
        .collect();
    let end = list.pop().unwrap();
    list.insert(2, end);
    let r = ok(&mut mcp, "set_talk_scenes", json!({"scenes": list}));
    assert_eq!(r["scenes"][2]["name"], "end", "{r}");

    // From the last step of `borrow` (2), next_step goes to `end` (3).
    let last = r["scenes"][1]["steps"].as_u64().unwrap();
    ok(&mut mcp, "set_step", json!({"step": last}));
    let r = ok(&mut mcp, "next_step", json!({}));
    assert_eq!(r["scene"], json!({"position": 3, "name": "end"}), "{r}");

    let (is_error, text, _) = mcp.call("goto_scene", json!({"scene": 99}));
    assert!(is_error && text.contains("not_found"), "{text}");
}
