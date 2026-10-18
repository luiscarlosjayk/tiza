//! t-155 — An annotation added over the socket/MCP at a step the view does
//! not show: the reply tells the agent.
//!
//! Acceptance criteria:
//! - AC1 `annotation.add` / `annotation.update` replies carry `step` and
//!   `visible` (whether it shows at the current step); MCP tool results say
//!   it in words.
//! - AC2 Optional (owner's call, not in this commit): a TUI message for an
//!   agent's addition that is not visible.
//! - AC3 schema/api.schema.json, docs/agent-guide.md, qa test, CHANGELOG.
//!
//! Corner cases: supervised mode (the reply is computed when the presenter
//! accepts, so `shown_step`/`visible` describe that moment); an annotation of
//! a linked file not shown at the current step, or outside the window
//! (t-105); a spotlight past its `until`; a redact masks from step 0.

use std::thread;
use std::time::Duration;

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, example, scratch, tiza_ok};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

const CODE: &str = "fn main() {\n    let s = String::from(\"hello\");\n    takes(s);\n}\n\nfn takes(s: String) {\n    println!(\"{s}\");\n}\n";

fn at_step_0(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.command("step 0");
    live
}

#[test]
fn ac1_add_at_step_0_is_not_visible_and_at_step_1_it_is() {
    let live = at_step_0("t155-add");
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "box", "from_line": 2, "to_line": 3, "label": "hi"}),
    );
    assert_eq!(r["id"], json!("a1"));
    assert!(r["revision"].as_u64().is_some(), "{r}");
    assert_eq!(r["step"], json!(1), "{r}");
    assert_eq!(r["shown_step"], json!(0), "{r}");
    assert_eq!(r["visible"], json!(false), "{r}");
    assert_eq!(r["hidden"], json!("it appears at step 1"), "{r}");
    // The view did not move (t-079).
    assert_eq!(live.with(|a| a.doc.step), 0);

    live.api_ok("step.set", json!({"step": 1}));
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 6, "text": "takes ownership"}),
    );
    assert_eq!(r["step"], json!(1), "{r}");
    assert_eq!(r["shown_step"], json!(1), "{r}");
    assert_eq!(r["visible"], json!(true), "{r}");
    assert!(r.get("hidden").is_none(), "absent when visible: {r}");

    // An explicit later step is not visible either.
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 3, "find": "takes"}, "step": 3}),
    );
    assert_eq!(
        (r["step"].clone(), r["visible"].clone()),
        (json!(3), json!(false))
    );
    assert_eq!(r["hidden"], json!("it appears at step 3"));
}

#[test]
fn ac1_cli_api_reply_carries_the_fields() {
    let live = at_step_0("t155-cli");
    let out = tiza_ok(&[
        "api",
        "annotation.add",
        r#"{"type":"note","line":4,"text":"hello"}"#,
        "--socket",
        live.socket.to_str().unwrap(),
    ]);
    let v: Value = serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("{e}: {out}"));
    let r = if v.get("result").is_some() {
        v["result"].clone()
    } else {
        v
    };
    assert_eq!(r["step"], json!(1), "{r}");
    assert_eq!(r["visible"], json!(false), "{r}");
}

#[test]
fn ac1_update_reports_the_new_step() {
    let live = at_step_0("t155-update");
    live.api_ok("step.set", json!({"step": 1}));
    live.api_ok(
        "annotation.add",
        json!({"id": "b", "type": "box", "from_line": 2, "to_line": 3}),
    );
    let r = live.api_ok("annotation.update", json!({"id": "b", "step": 3}));
    assert_eq!(r["id"], json!("b"));
    assert_eq!(r["step"], json!(3), "{r}");
    assert_eq!(r["visible"], json!(false), "{r}");
    assert_eq!(r["hidden"], json!("it appears at step 3"));
    let r = live.api_ok("annotation.update", json!({"id": "b", "label": "back"}));
    assert_eq!(
        r["visible"],
        json!(false),
        "a label change keeps step 3: {r}"
    );
    let r = live.api_ok("annotation.update", json!({"id": "b", "step": 1}));
    assert_eq!(
        (r["step"].clone(), r["visible"].clone()),
        (json!(1), json!(true))
    );
    // Delete keeps its plain reply.
    let r = live.api_ok("annotation.delete", json!({"id": "b"}));
    assert!(r.get("visible").is_none() && r.get("step").is_none(), "{r}");
}

#[test]
fn ac1_named_step_title_travels_with_the_reply() {
    let live = at_step_0("t155-title");
    live.api_ok(
        "scene.patch",
        json!({"steps": [{"step": 2, "id": "borrow", "title": "Shared borrow"}]}),
    );
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 6, "text": "x", "step": 2}),
    );
    assert_eq!(r["step_title"], json!("Shared borrow"), "{r}");
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 7, "text": "y"}),
    );
    assert!(r.get("step_title").is_none(), "step 1 has no name: {r}");
}

#[test]
fn corner_spotlight_past_its_until_and_redact_from_step_0() {
    let live = at_step_0("t155-spot");
    // Something at step 3, so the view can stand at step 2.
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 7, "text": "later", "step": 3}),
    );
    live.api_ok("step.set", json!({"step": 2}));
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "spotlight", "from_line": 2, "to_line": 3, "step": 1}),
    );
    assert_eq!(r["visible"], json!(false), "{r}");
    assert_eq!(r["hidden"], json!("a spotlight shows only at its step 1"));
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "spotlight", "from_line": 2, "to_line": 3, "step": 1, "until": 3}),
    );
    assert_eq!(r["visible"], json!(true), "{r}");

    // A redact masks from step 0: visible even when its step is ahead.
    live.api_ok("step.set", json!({"step": 0}));
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "redact", "at": {"line": 2, "find": "String::from("}}),
    );
    assert_eq!(
        (r["step"].clone(), r["visible"].clone()),
        (json!(1), json!(true)),
        "{r}"
    );
}

/// A scratch copy of the two-file example of t-105.
fn open_demo(name: &str) -> Live {
    let dir = scratch(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for f in ["src/stack.rs", "src/queue.rs", "stack-and-queue.tiza"] {
        std::fs::copy(example(f), dir.join(f)).unwrap();
    }
    let path = dir.join("stack-and-queue.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    Live::open(doc, path, dir.join("live.sock"))
}

#[test]
fn corner_linked_file_not_shown_and_outside_the_window() {
    let live = open_demo("t155-link");
    // Step 2 shows src/queue.rs: the stack's box a1 (step 1) is not drawn.
    live.api_ok("step.set", json!({"step": 2}));
    let r = live.api_ok("annotation.update", json!({"id": "a1", "label": "LIFO"}));
    assert_eq!(r["step"], json!(1), "{r}");
    assert_eq!(r["visible"], json!(false), "{r}");
    assert_eq!(
        r["hidden"],
        json!("its file src/stack.rs is not shown at step 2 (src/queue.rs is)"),
        "{r}"
    );
    // A new note at step 2 belongs to queue.rs and shows.
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 17, "text": "in at the back"}),
    );
    assert_eq!(
        (r["step"].clone(), r["visible"].clone()),
        (json!(2), json!(true)),
        "{r}"
    );

    // Step 5 shows queue.rs:17-23: the note a4 (line 13) is outside the window.
    live.api_ok("step.set", json!({"step": 5}));
    let r = live.api_ok("annotation.update", json!({"id": "a4", "text": "VecDeque"}));
    assert_eq!(r["visible"], json!(false), "{r}");
    assert_eq!(
        r["hidden"],
        json!("its lines are outside the code shown at step 5 (17-23)"),
        "{r}"
    );
    // Step 4 links the stack back: a1 shows again.
    live.api_ok("step.set", json!({"step": 4}));
    let r = live.api_ok("annotation.update", json!({"id": "a1", "label": "LIFO!"}));
    assert_eq!(r["visible"], json!(true), "{r}");
}

fn send_async(
    socket: &std::path::Path,
    method: &str,
    params: Value,
) -> thread::JoinHandle<tiza_core::protocol::Response> {
    let socket = socket.to_path_buf();
    let method = method.to_string();
    thread::spawn(move || tiza_tui::ipc::call(&socket, &method, params).expect("socket"))
}

fn wait_pending(live: &Live, n: usize) {
    let start = std::time::Instant::now();
    while live.with(|a| a.proposals.len()) < n {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "waiting for {n} proposals"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn corner_supervised_reply_describes_the_moment_of_acceptance() {
    let live = at_step_0("t155-supervised");
    live.command("supervise on");
    // Asked at step 0 …
    let h = send_async(
        &live.socket,
        "annotation.add",
        json!({"type": "note", "line": 2, "text": "hi"}),
    );
    wait_pending(&live, 1);
    // … the presenter moves to step 1 before accepting.
    live.command("step 1");
    live.keys(&[KeyCode::Char('y')]);
    let resp = h.join().unwrap();
    let r = resp.result.expect("accepted");
    assert_eq!(r["step"], json!(1), "{r}");
    assert_eq!(r["shown_step"], json!(1), "{r}");
    assert_eq!(r["visible"], json!(true), "{r}");
}

#[test]
fn ac1_mcp_tools_say_it_in_words() {
    let live = at_step_0("t155-mcp");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("add_box", json!({"from_line": 2, "to_line": 3}));
    assert!(!err, "{text}");
    let first = text.lines().next().unwrap();
    assert_eq!(
        first,
        "a1 added at step 1; not visible now: the view is at step 0 and it appears at step 1 (next_step shows it)."
    );
    // The JSON follows the sentence.
    let json_part: Value = serde_json::from_str(&text[first.len()..]).unwrap();
    assert_eq!(json_part["visible"], json!(false));

    let (_, text, _) = mcp.call("add_note", json!({"line": 6, "text": "x", "step": 3}));
    assert!(
        text.starts_with("a2 added at step 3; not visible now: the view is at step 0 and it appears at step 3 (set_step 3 shows it)."),
        "{text}"
    );
    live.api_ok("step.set", json!({"step": 1}));
    let (_, text, _) = mcp.call("highlight", json!({"at": {"line": 3, "find": "takes"}}));
    assert!(
        text.starts_with("a3 added at step 1; visible now (the view is at step 1)."),
        "{text}"
    );
    let (_, text, _) = mcp.call("update_annotation", json!({"id": "a2", "step": 1}));
    assert!(
        text.starts_with("a2 updated at step 1; visible now"),
        "{text}"
    );
    let instructions = mcp.initialize_result()["instructions"].as_str().unwrap();
    assert!(instructions.contains("whether it shows at the step on screen"));
}

#[test]
fn ac3_schema_guide_and_changelog() {
    let schema = tiza_ok(&["schema", "api"]);
    let committed = std::fs::read_to_string(repo_root().join("schema/api.schema.json")).unwrap();
    assert_eq!(
        schema.trim(),
        committed.trim(),
        "schema/api.schema.json regenerated"
    );
    let v: Value = serde_json::from_str(&schema).unwrap();
    let text = v.to_string();
    for field in ["shown_step", "visible", "hidden", "step_title"] {
        assert!(
            text.contains(&format!("\"{field}\"")),
            "schema lacks {field}"
        );
    }
    let guide = std::fs::read_to_string(repo_root().join("docs/agent-guide.md")).unwrap();
    assert!(guide.contains("## Is it on screen?"));
    assert!(guide.contains("\"visible\": false"));
    let changelog = std::fs::read_to_string(repo_root().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("(t-155)"));
}
