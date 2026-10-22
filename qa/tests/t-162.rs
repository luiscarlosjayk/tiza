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
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, fixture, scratch};

const CORE: [&str; 4] = ["add_box", "get_code", "render_text", "status"];

/// The words a person says when they want what Tiza does (AC1).
const KEYWORDS: [&str; 22] = [
    "box",
    "arrow",
    "highlight",
    "note",
    "callout",
    "mark",
    "step",
    "presentation",
    "explain code",
    "code walkthrough",
    "screencast",
    "tutorial",
    "teach",
    "show on screen",
    "type code",
    "move",
    "link another file",
    "export",
    "png",
    "gif",
    "another agent",
    "live or recorded",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .into()
}

/// A server with no instance behind it: `initialize` and `tools/list`
/// need none.
fn mcp(name: &str) -> McpClient {
    McpClient::start(&scratch(name).join("none.sock"))
}

fn instructions(mcp: &McpClient) -> String {
    mcp.initialize_result()["instructions"]
        .as_str()
        .expect("instructions")
        .to_string()
}

/// `text` without its `code` spans (tool names such as `add_move` or
/// `next_step`), lowercased and with every run of whitespace as one space:
/// the prose a keyword has to be in.
fn prose(text: &str) -> String {
    let mut out = String::new();
    for (i, part) in text.split('`').enumerate() {
        out.push_str(if i % 2 == 0 { part } else { " " });
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The README's positioning sentence: its first bold line.
fn positioning() -> String {
    let readme = std::fs::read_to_string(repo_root().join("README.md")).unwrap();
    let line = readme
        .lines()
        .find(|l| l.starts_with("**") && l.trim_end().ends_with("**"))
        .expect("a bold positioning line in the README");
    line.trim_matches('*').trim().to_string()
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

/// The first sentence of a description: up to the first `. ` outside
/// backticks and parentheses.
fn first_sentence(d: &str) -> &str {
    let (mut depth, mut code) = (0i32, false);
    let bytes = d.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'`' => code = !code,
            b'(' if !code => depth += 1,
            b')' if !code => depth -= 1,
            b'.' if !code && depth == 0 && bytes.get(i + 1).is_none_or(|n| *n == b' ') => {
                return &d[..=i];
            }
            _ => {}
        }
    }
    d
}

// ---------------------------------------------------------------- AC1

#[test]
fn ac1_instructions_describe_tiza_with_the_words_people_use() {
    let mcp = mcp("t162-instructions");
    let text = instructions(&mcp);
    let chars = text.chars().count();
    eprintln!("instructions ({chars} chars):\n{text}");
    let words = prose(&text);
    let missing: Vec<&str> = KEYWORDS
        .iter()
        .copied()
        .filter(|k| !words.contains(k))
        .collect();
    assert!(
        missing.is_empty(),
        "missing {missing:?} in the prose of:\n{text}"
    );
    // Whatever the README says Tiza is (t-141 may change it).
    let sentence = positioning();
    assert!(
        prose(&text).contains(&prose(sentence.trim_end_matches('.'))),
        "«{sentence}» not in:\n{text}"
    );
    // The flow names its tools in order, and the resources are pointed at.
    let at = |t: &str| {
        text.find(&format!("`{t}`"))
            .unwrap_or_else(|| panic!("`{t}` not in:\n{text}"))
    };
    assert!(at("status") < at("get_code"), "{text}");
    assert!(at("get_code") < at("render_text"), "{text}");
    assert!(at("render_text") < at("export_image"), "{text}");
    assert!(text.contains("tiza://"), "{text}");
    assert!(chars < 1200, "{chars} chars");
}

/// Corner case: only what the server does — no masking, no MP4
/// (a shell command, not a tool).
#[test]
fn ac1_instructions_promise_nothing_the_server_does_not_do() {
    let lower = instructions(&mcp("t162-promise")).to_lowercase();
    for s in ["mask", "secret", "mp4"] {
        assert!(!lower.contains(s), "{s} in:\n{lower}");
    }
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
    }
}

/// Corner case: a client that ignores `_meta` (this one) calls the core
/// tools as before.
#[test]
fn ac2_a_client_that_ignores_meta_calls_the_core_tools_as_before() {
    let live = Live::from_code(
        "fn takes(s: String) {\n    drop(s);\n}\n",
        "rs",
        "t162-calls",
    );
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("status", json!({}));
    assert!(!err, "{text}");
    assert!(text.contains("\"bridge\""), "{text}");
    let (err, text, _) = mcp.call("get_code", json!({}));
    assert!(!err && text.contains("fn takes"), "{text}");
    let (err, text, _) = mcp.call(
        "add_box",
        json!({"from_line": 1, "to_line": 3, "label": "takes", "step": 1}),
    );
    assert!(!err && text.starts_with("a1 added at step 1"), "{text}");
    live.api_ok("step.set", json!({"step": 1}));
    let (err, text, _) = mcp.call("render_text", json!({"width": 80}));
    assert!(
        !err && text.contains("takes") && text.contains('╭'),
        "{text}"
    );
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

#[test]
fn ac3_the_first_sentence_of_each_core_tool_uses_the_searchable_words() {
    let mut mcp = mcp("t162-first");
    let tools = by_name(&mcp.tools());
    let want: [(&str, &[&str]); 4] = [
        ("status", &["tiza", "box", "arrow", "highlight", "explain"]),
        ("get_code", &["tiza", "screen", "box", "arrow", "highlight"]),
        ("render_text", &["tiza", "screen", "box", "arrow", "step"]),
        ("add_box", &["box", "explain", "step"]),
    ];
    for (name, words) in want {
        let d = description(&tools, name);
        let first = prose(first_sentence(&d));
        for w in words {
            assert!(first.contains(w), "{name}: «{w}» not in «{first}»");
        }
        // Claude Code cuts the description of a tool that loads upfront at
        // 4,096 characters.
        let chars = d.chars().count();
        assert!(chars < 4096, "{name}: {chars} chars");
    }
}

/// What `render_text` says of itself is true: in presentation mode and
/// with `:layout stable` it is what `export_image` writes for that step at
/// the same width, at every step.
#[test]
fn ac3_render_text_is_the_export_in_presentation_mode() {
    let src = repo_root().join("examples/ownership.rs.tiza");
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

// ------------------------------------------- add_move and check (t-194)

/// A scratch copy of the two-file fixture (the instance autosaves it).
fn two_files(name: &str) -> PathBuf {
    let dir = scratch(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for f in ["two-files.tiza", "src/a.rs", "src/b.rs"] {
        std::fs::copy(fixture(&format!("t-162/{f}")), dir.join(f)).unwrap();
    }
    dir.join("two-files.tiza")
}

fn open(target: &Path) -> Live {
    let o = tiza_core::fs::open(target.to_str().unwrap()).unwrap();
    let sock = target.parent().unwrap().join("s.sock");
    Live::open(o.document, o.save_path, sock)
}

/// The JSON a tool replies with.
fn reply(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// The 1-based index of the move at `step` that links `file` with window
/// `lines`, as `list_moves` numbers them.
fn link_index(mcp: &mut McpClient, step: u64, file: &str, lines: Option<&str>) -> u64 {
    let (_, text, _) = mcp.call("list_moves", json!({}));
    let moves = reply(&text)["moves"].as_array().unwrap().clone();
    moves
        .iter()
        .find(|m| {
            m["at_step"] == step
                && m["action"] == "code.link"
                && m["file"] == file
                && m["lines"].as_str() == lines
        })
        .unwrap_or_else(|| panic!("no link to {file} at step {step} in {moves:?}"))["index"]
        .as_u64()
        .unwrap()
}

/// The codes of `check`'s findings about annotation `id`.
fn codes_for(mcp: &mut McpClient, id: &str) -> Vec<String> {
    let (err, text, _) = mcp.call("check", json!({}));
    assert!(!err, "{text}");
    reply(&text)["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["id"] == id)
        .map(|f| f["code"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn ac_t194_add_move_and_check_name_the_warnings_the_instance_gives() {
    let target = two_files("t162-t194");
    let live = open(&target);
    let mut mcp = McpClient::start(&live.socket);
    let tools = by_name(&mcp.tools());
    let add_move = description(&tools, "add_move");
    let check = description(&tools, "check");
    // The descriptions name what the replies carry.
    for code in ["`not_shown`", "`outside_window`"] {
        assert!(check.contains(code), "{code} not in check:\n{check}");
    }
    for s in ["`warning`", "`delete_move`"] {
        assert!(add_move.contains(s), "{s} not in add_move:\n{add_move}");
    }

    assert!(codes_for(&mut mcp, "n3").is_empty());

    // A second link at step 3 to a.rs, with a window that leaves n3's line
    // out: n3 was drawn there and is not now. The reply warns, `check` says
    // `outside_window`; deleting that link shows n3 again.
    let (err, text, _) = mcp.call(
        "add_move",
        json!({"at_step": 3, "action": "code.link", "file": "src/a.rs", "lines": "1-5"}),
    );
    assert!(!err, "{text}");
    let added = reply(&text);
    let warning = added["warning"].as_str().expect("a warning").to_string();
    assert!(warning.contains("n3"), "{warning}");
    assert_eq!(codes_for(&mut mcp, "n3"), ["outside_window"]);
    let index = link_index(&mut mcp, 3, "src/a.rs", Some("1-5"));
    let (err, text, _) = mcp.call("delete_move", json!({ "index": index }));
    assert!(!err && reply(&text).get("warning").is_none(), "{text}");
    assert!(codes_for(&mut mcp, "n3").is_empty());

    // Deleting the step-3 link back to a.rs leaves step 3 on b.rs: n3 (of
    // a.rs, step 3) is not shown there. `delete_move` warns too, `check`
    // says `not_shown`.
    let index = link_index(&mut mcp, 3, "src/a.rs", None);
    let (err, text, _) = mcp.call("delete_move", json!({ "index": index }));
    assert!(!err, "{text}");
    let deleted = reply(&text);
    let warning = deleted["warning"].as_str().expect("a warning").to_string();
    assert!(warning.contains("n3"), "{warning}");
    assert_ne!(added["warning"], deleted["warning"], "two kinds of warning");
    assert_eq!(codes_for(&mut mcp, "n3"), ["not_shown"]);
}

// ---------------------------------------------------------------- AC5

#[test]
fn ac5_guide_and_changelog() {
    let guide = std::fs::read_to_string(repo_root().join("docs/agent-guide.md")).unwrap();
    assert!(guide.contains("anthropic/alwaysLoad"), "agent guide");
    assert!(
        guide.contains("the instructions are written to be searched by"),
        "agent guide"
    );
    let changelog = std::fs::read_to_string(repo_root().join("CHANGELOG.md")).unwrap();
    let unreleased = changelog.split("## [0.0.1]").next().unwrap();
    assert!(
        unreleased.contains("- MCP discovery for clients that defer tools:"),
        "CHANGELOG"
    );
}
