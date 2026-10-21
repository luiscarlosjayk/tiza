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
//!   nothing about the retired redaction (t-183).
//! - AC2 `tools/list`: `status`, `get_code`, `render_text` and `add_box`
//!   carry `_meta["anthropic/alwaysLoad"] = true` (the key Claude Code
//!   documents for tools that load upfront when it defers the rest), and no
//!   other tool does.
//! - AC3 Tool names and input schemas are the ones of `main` before this
//!   card (fixtures); the first sentence of each core tool uses the words
//!   of AC1.
//! - Added by Worker after t-194: `add_move` says its reply's `warning`
//!   also names annotations of a file their step no longer shows, and that
//!   `delete_move` answers the same; `check` names `outside_window` and
//!   `not_shown`, in the words the instance answers with.
//! - AC5 docs/agent-guide.md and CHANGELOG.
//!
//! Corner cases: a client that ignores `_meta` (this test's own client)
//! calls the core tools as before; the instructions promise only what the
//! server does (no MP4, which is a shell command).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, example, fixture, scratch};

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
    "PNG",
    "GIF",
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
    eprintln!("instructions ({} chars):\n{text}", text.chars().count());
    let lower = text.to_lowercase();
    let missing: Vec<&str> = KEYWORDS
        .iter()
        .copied()
        .filter(|k| !lower.contains(&k.to_lowercase()))
        .collect();
    assert!(missing.is_empty(), "missing {missing:?} in:\n{text}");
    // The README's positioning sentence, first.
    assert!(
        text.starts_with("Tiza: draw on top of code in the terminal to explain it on video."),
        "{text}"
    );
    // The flow, the resources and what t-155 asks of the replies.
    for s in [
        "`status`",
        "`get_code`",
        "`find`",
        "`render_text`",
        "`export_image`",
        "tiza://scene",
        "whether it shows at the step on screen",
    ] {
        assert!(text.contains(s), "missing {s} in:\n{text}");
    }
    assert!(
        text.chars().count() < 1200,
        "{} chars",
        text.chars().count()
    );
}

/// Corner case: only what the server does — no redaction (t-183), no MP4
/// (a shell command, not a tool).
#[test]
fn ac1_instructions_promise_nothing_the_server_does_not_do() {
    let lower = instructions(&mcp("t162-promise")).to_lowercase();
    for s in ["redact", "mask", "secret", "mp4", "video file"] {
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
        json!({"find_start": "fn takes", "find_end": "drop", "label": "takes", "step": 1}),
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
        ("status", &["Tiza", "box", "arrow", "highlight", "explain"]),
        ("get_code", &["Tiza", "screen", "box", "arrow", "highlight"]),
        ("render_text", &["Tiza", "screen", "box", "arrow", "step"]),
        ("add_box", &["box", "explain", "step"]),
    ];
    for (name, words) in want {
        let d = description(&tools, name);
        let first = first_sentence(&d);
        for w in words {
            assert!(first.contains(w), "{name}: «{w}» not in «{first}»");
        }
        // Claude Code cuts the description of a tool that loads upfront at
        // 4,096 characters.
        assert!(d.chars().count() < 4096, "{name}: {} chars", d.len());
    }
}

// ------------------------------------------- add_move and check (t-194)

/// A scratch copy of `examples/` (the documents and the code they read).
fn examples_copy(name: &str) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            if p.is_dir() {
                copy(&p, &to.join(e.file_name()));
            } else {
                std::fs::copy(&p, to.join(e.file_name())).unwrap();
            }
        }
    }
    let dir = scratch(name);
    copy(&example(""), &dir);
    dir
}

fn open(target: &Path) -> Live {
    let o = tiza_core::fs::open(target.to_str().unwrap()).unwrap();
    let sock = target.parent().unwrap().join("s.sock");
    Live::open(o.document, o.save_path, sock)
}

/// The JSON after the reply's text, as the tool sends it.
fn reply(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// The tail every message of a kind ends with, after `: `.
fn tail(message: &str) -> &str {
    message.rsplit_once("): ").map_or(message, |(_, t)| t)
}

#[test]
fn ac_t194_add_move_and_check_describe_what_they_answer() {
    let dir = examples_copy("t162-t194");
    let live = open(&dir.join("stack-and-queue.tiza"));
    let mut mcp = McpClient::start(&live.socket);
    let tools = by_name(&mcp.tools());
    let add_move = description(&tools, "add_move");
    let check = description(&tools, "check");

    // A link at step 4 to queue.rs leaves a6 (of stack.rs) unshown there.
    let (err, text, _) = mcp.call(
        "add_move",
        json!({"at_step": 4, "action": "code.link", "file": "src/queue.rs"}),
    );
    assert!(!err, "{text}");
    let added = reply(&text);
    let warning = added["warning"].as_str().expect("warning").to_string();
    assert!(warning.contains("is not shown at step 4"), "{warning}");
    assert!(
        add_move.contains(warning.split_once(": ").unwrap().1),
        "add_move says «{warning}»?\n{add_move}"
    );
    // Deleting it answers with no warning (it shows a6 again); deleting
    // the step-4 link back to stack.rs answers the same warning.
    let (err, text, _) = mcp.call("delete_move", json!({"index": 7}));
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call("delete_move", json!({"index": 5}));
    assert!(!err, "{text}");
    assert_eq!(reply(&text)["warning"], added["warning"], "{text}");
    assert!(add_move.contains("`delete_move`"), "{add_move}");

    // `check` names both codes, each with the words of its message.
    let (err, text, _) = mcp.call("check", json!({}));
    assert!(!err, "{text}");
    let findings = reply(&text)["findings"].as_array().unwrap().clone();
    let message = |code: &str| -> String {
        findings
            .iter()
            .find(|f| f["code"] == code)
            .unwrap_or_else(|| panic!("no {code} in {findings:?}"))["message"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let not_shown = message("not_shown");
    assert!(check.contains("`not_shown`"), "{check}");
    assert!(check.contains(tail(&not_shown)), "«{not_shown}»\n{check}");
    assert!(
        check.contains("is not shown at step") && not_shown.contains("is not shown at step"),
        "{not_shown}\n{check}"
    );
    assert!(check.contains("`outside_window`"), "{check}");
    // Step 2's link shows queue.rs:10-36 there; a note at the end of the
    // file is outside it.
    live.api_ok("step.set", json!({"step": 2}));
    let (err, text, _) = mcp.call(
        "add_note",
        json!({"line": 1, "text": "out of the window", "step": 2}),
    );
    assert!(!err, "{text}");
    let (_, text, _) = mcp.call("check", json!({}));
    let findings = reply(&text)["findings"].as_array().unwrap().clone();
    let outside = findings
        .iter()
        .find(|f| f["code"] == "outside_window")
        .unwrap_or_else(|| panic!("no outside_window in {findings:?}"))["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(check.contains(tail(&outside)), "«{outside}»\n{check}");
    assert!(
        check.contains("outside the window at step"),
        "{outside}\n{check}"
    );
}

// ---------------------------------------------------------------- AC5

#[test]
fn ac5_guide_and_changelog() {
    let guide = std::fs::read_to_string(repo_root().join("docs/agent-guide.md")).unwrap();
    assert!(guide.contains("anthropic/alwaysLoad"), "agent guide");
    assert!(guide.contains("t-162"), "agent guide");
    let changelog = std::fs::read_to_string(repo_root().join("CHANGELOG.md")).unwrap();
    let unreleased = changelog.split("## [0.0.1]").next().unwrap();
    assert!(unreleased.contains("t-162"), "CHANGELOG");
}
