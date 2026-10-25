//! t-162 — MCP discovery: server `instructions` written as a searchable
//! description of Tiza, and four core tools that always load.
//!
//! Acceptance criteria (v0.1 lane: AC1–AC3 and AC5; AC4, the t-161 matrix
//! with variant C, runs on Luis's Mac):
//! - AC1 `initialize` → `instructions` describe Tiza (the README's
//!   positioning sentence), when to use it (explain code to a person or
//!   another agent, live or recorded, step by step) and the words people
//!   use (box, arrow, highlight, note, callout, mark, step, presentation,
//!   explain code, code walkthrough, screencast, tutorial, teach, show on
//!   screen, type code / move, link another file, export PNG/GIF), the flow
//!   in two lines and the `tiza://` resources; under 1,200 characters, and
//!   nothing about features the server does not have.
//! - AC2 `tools/list`: `status`, `get_code`, `render_text` and `add_box`
//!   carry `_meta["anthropic/alwaysLoad"] = true` (the key Claude Code
//!   documents for tools that load upfront when it defers the rest), and no
//!   other tool does.
//! - AC3 Tool names and input schemas are the ones of `main` before this
//!   card (fixtures); the first sentence of each core tool uses the words
//!   of AC1, and says only what is true (`render_text` is the screen as the
//!   presenter sees it, which is the export in presentation mode).
//! - Added by Worker after t-194: `add_move` says its reply's `warning`
//!   also names annotations of a file their step no longer shows, and that
//!   `delete_move` answers the same; `check` names `outside_window` and
//!   `not_shown`.
//! - AC5 docs/agent-guide.md and CHANGELOG.
//!
//! Corner cases: a client that ignores `_meta` (this test's own client)
//! calls the core tools as before; the instructions promise only what the
//! server does (no MP4, which is a shell command).
//!
//! The checks are invariants, not wording: the keywords are looked for in
//! the instructions' prose (tool names and other `code` taken out), the
//! positioning sentence is whatever the README says, and the warnings are
//! compared with what the instance really answers.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, fixture, scratch};

const CORE: [&str; 4] = ["add_box", "get_code", "render_text", "status"];

/// A server with no instance behind it: `initialize` and `tools/list`
/// need none.
fn mcp(name: &str) -> McpClient {
    McpClient::start(&scratch(name).join("none.sock"))
}

fn by_name(tools: &Value) -> BTreeMap<String, Value> {
    tools
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| (t["name"].as_str().unwrap().to_string(), t.clone()))
        .collect()
}

fn description(tools: &BTreeMap<String, Value>, name: &str) -> String {
    tools[name]["description"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------- AC2

#[test]
fn ac2_the_core_tools_always_load_and_no_other_does() {
    let mut mcp = mcp("t162-always-load");
    let tools = by_name(&mcp.tools());
    let marked: Vec<&str> = tools
        .iter()
        .filter(|(_, t)| !t["_meta"].is_null())
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(marked, CORE, "tools with a _meta");
    for name in CORE {
        assert_eq!(
            tools[name]["_meta"],
            json!({"anthropic/alwaysLoad": true}),
            "{name}"
        );
        // Claude Code cuts the description of a tool that loads upfront at
        // 4,096 characters.
        let chars = description(&tools, name).chars().count();
        assert!(chars < 4096, "{name}: {chars} chars");
    }
}

// ---------------------------------------------------------------- AC3

#[test]
fn ac3_tool_names_and_input_schemas_are_the_ones_of_main() {
    let mut mcp = mcp("t162-names");
    let tools = by_name(&mcp.tools());
    let names: Vec<&str> = tools.keys().map(String::as_str).collect();
    let main = std::fs::read_to_string(fixture("t-162/tool-names-main.txt")).unwrap();
    assert_eq!(names, main.lines().collect::<Vec<_>>());
    let schemas: BTreeMap<String, Value> = serde_json::from_str(
        &std::fs::read_to_string(fixture("t-162/input-schemas-main.json")).unwrap(),
    )
    .unwrap();
    for (name, schema) in &schemas {
        assert_eq!(&tools[name]["inputSchema"], schema, "{name}");
    }
}

/// What `render_text` says of itself is true: in presentation mode and
/// with `:layout stable` it is what `export_image` writes for that step at
/// the same width, at every step.
#[test]
fn ac3_render_text_is_the_export_in_presentation_mode() {
    let src = fixture("ownership/ownership.rs.tiza");
    let o = tiza_core::fs::open(src.to_str().unwrap()).unwrap();
    let last = o.document.max_step();
    let dir = scratch("t162-render-text");
    for (mode, setup) in [("present", 0), ("stable", 1)] {
        let live = Live::open(
            o.document.clone(),
            dir.join(format!("{mode}.tiza")),
            dir.join(format!("{mode}.sock")),
        );
        if setup == 0 {
            live.with(|app| app.present = true);
        } else {
            live.command("layout stable");
        }
        let mut mcp = McpClient::start(&live.socket);
        for step in 0..=last {
            live.api_ok("step.set", json!({ "step": step }));
            let (err, shown, _) = mcp.call("render_text", json!({"width": 80}));
            assert!(!err, "{shown}");
            let out = dir.join(format!("{mode}-{step}.txt"));
            let (err, text, _) = mcp.call(
                "export_image",
                json!({"path": out.to_str().unwrap(), "step": step, "width": 80}),
            );
            assert!(!err, "{text}");
            let exported = std::fs::read_to_string(&out).unwrap();
            assert_eq!(shown.trim_end(), exported.trim_end(), "{mode}, step {step}");
        }
    }
}
