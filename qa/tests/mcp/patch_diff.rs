//! `add_move` with `diff: true` reaches the socket: `list_moves` names
//! the diff, and `render_text` shows the `+` at the step of the move.

use serde_json::json;
use tiza_qa::{Live, McpClient};

#[test]
fn add_move_with_diff_marks_the_line_that_it_writes() {
    let live = Live::from_code("fn main() {\n    let a = 1;\n}\n", "rs", "mcp-diff");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call(
        "add_move",
        json!({"at_step": 1, "op": "replace", "find": "let a",
               "content": "    let a = 2;", "diff": true}),
    );
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call("list_moves", json!({}));
    assert!(!err && text.contains("1 line diff"), "{text}");
    let (err, text, _) = mcp.call("set_step", json!({"step": 1}));
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call("render_text", json!({"width": 40}));
    assert!(!err, "{text}");
    let row = text
        .lines()
        .find(|l| l.contains("let a = 2;"))
        .unwrap_or_else(|| panic!("{text}"));
    // The sign is right of the line number.
    assert!(row.starts_with(" 2 + "), "{text}");
}

#[test]
fn get_code_names_the_lines_that_the_diff_removed() {
    let live = Live::from_code("a();\nb();\nc();\n", "rs", "mcp-diff-removed");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call(
        "add_move",
        json!({"at_step": 1, "op": "delete", "find": "b();", "diff": true}),
    );
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call("set_step", json!({"step": 1}));
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call("get_code", json!({}));
    assert!(!err, "{text}");
    // The code of the step, then the note: b is not a line of the code.
    let (code, note) = text
        .split_once("# The diff of this step removed these lines")
        .unwrap_or_else(|| panic!("no note:\n{text}"));
    assert!(!code.contains("b();"), "{text}");
    assert!(note.contains("# before line 2: «b();»"), "{text}");
    // Step 0 has no note.
    let (err, text, _) = mcp.call("set_step", json!({"step": 0}));
    assert!(!err, "{text}");
    let (_, text, _) = mcp.call("get_code", json!({}));
    assert!(!text.contains("removed these lines"), "{text}");
}
