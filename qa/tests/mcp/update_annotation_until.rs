//! The `until` argument of the MCP tools: each add tool sets it on its
//! type, `update_annotation` without `until` keeps it, a number changes
//! it, and `null` removes it, as the `effect` argument does.

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient};

const CODE: &str = "fn main() {\n    let s = String::from(\"hi\");\n    println!(\"{s}\");\n}\n";

fn annotation(live: &Live, id: &str) -> Value {
    live.api_ok("scene.get", json!({}))["document"]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap_or_else(|| panic!("no {id}"))
        .clone()
}

#[test]
fn each_add_tool_sets_until() {
    let live = Live::from_code(CODE, "rs", "mcp-until-add");
    let mut mcp = McpClient::start(&live.socket);
    for (tool, args) in [
        ("add_box", json!({"from_line": 1, "to_line": 3})),
        (
            "add_arrow",
            json!({"from": {"find": "s ="}, "to": {"find": "{s}"}}),
        ),
        ("add_note", json!({"line": 2, "text": "x"})),
        ("add_callout", json!({"from_line": 2, "text": "y"})),
        ("highlight", json!({"at": {"find": "hi"}})),
        (
            "add_mark",
            json!({"kind": "read", "at": {"find": "println"}}),
        ),
        ("add_spotlight", json!({"from_line": 2})),
    ] {
        let mut a = args;
        a["id"] = json!(tool);
        a["step"] = json!(1);
        a["until"] = json!(2);
        let (err, text, v) = mcp.call(tool, a);
        assert!(!err, "{tool}: {text} {v}");
        assert_eq!(annotation(&live, tool)["until"], json!(2), "{tool}");
    }
}

#[test]
fn update_annotation_keeps_changes_and_removes_until() {
    let live = Live::from_code(CODE, "rs", "mcp-until");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, v) = mcp.call(
        "add_note",
        json!({"id": "n1", "line": 2, "text": "x", "step": 1, "until": 2}),
    );
    assert!(!err, "{text} {v}");
    let (err, text, v) = mcp.call("update_annotation", json!({"id": "n1", "color": "red"}));
    assert!(!err, "{text} {v}");
    assert_eq!(annotation(&live, "n1")["until"], json!(2));
    let (err, text, v) = mcp.call("update_annotation", json!({"id": "n1", "until": 3}));
    assert!(!err, "{text} {v}");
    assert_eq!(annotation(&live, "n1")["until"], json!(3));
    let (err, text, v) = mcp.call("update_annotation", json!({"id": "n1", "until": null}));
    assert!(!err, "{text} {v}");
    let a = annotation(&live, "n1");
    assert!(a.get("until").is_none(), "{a}");
}
