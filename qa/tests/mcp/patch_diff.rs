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
