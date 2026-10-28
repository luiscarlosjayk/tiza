//! The `effect` argument of the MCP tools (experimental): an add tool sets
//! it, `update_annotation` without `effect` keeps it, and `null` removes
//! it, as the `file` argument does.

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient};

const CODE: &str = "fn main() {\n    let s = String::from(\"hi\");\n    println!(\"{s}\");\n}\n";

/// Annotation `id` as `scene.get` gives it.
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
fn update_annotation_without_effect_keeps_it() {
    let live = Live::from_code(CODE, "rs", "mcp-effect");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, v) = mcp.call(
        "add_box",
        json!({"id": "e1", "from_line": 1, "to_line": 3, "effect": "draw"}),
    );
    assert!(!err, "{text} {v}");
    assert_eq!(annotation(&live, "e1")["effect"], "draw");
    let (err, text, v) = mcp.call("update_annotation", json!({"id": "e1", "color": "red"}));
    assert!(!err, "{text} {v}");
    let a = annotation(&live, "e1");
    assert_eq!(a["effect"], "draw", "{a}");
    assert_eq!(a["color"], "red", "{a}");
    let (err, text, v) = mcp.call("update_annotation", json!({"id": "e1", "effect": null}));
    assert!(!err, "{text} {v}");
    let a = annotation(&live, "e1");
    assert!(a.get("effect").is_none(), "{a}");
}
