//! MCP integration tests with a real client: stdio + socket, errors,
//! reconnection and validation against the published schemas.
//!
//! Acceptance criteria:
//! - AC1 A real JSON-RPC client over stdio completes `initialize`, lists the
//!   23 tools (each with a description and an input schema that is a valid
//!   JSON Schema) and calls them; every tool's call reaches the live instance.
//! - AC2 Without a live instance every tool answers an explicit error naming
//!   the socket and how to start Tiza — it never hangs. When the TUI starts
//!   afterwards, the same MCP process works without a restart (reconnection).
//! - AC3 Error cases map to tool errors with the socket's code in the text:
//!   text_not_found, duplicate_id, out_of_range, not_found, invalid_argument,
//!   plus parameter validation (unknown field, wrong type).
//! - AC4 Results of `scene.get`-backed tools and the socket replies validate
//!   against `schema/api.schema.json`; the document returned by the socket
//!   validates against `schema/tiza.schema.json`.
//! - AC5 Two MCP clients against one instance interleave without mixing replies.

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, scratch};

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn validator(path: &str) -> jsonschema::Validator {
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(root().join(path)).unwrap()).unwrap();
    jsonschema::validator_for(&schema).expect("valid schema")
}

#[test]
fn handshake_tools_and_calls() {
    let live = Live::from_code(
        "fn main() {\n    let s = 1;\n    takes(s);\n}\nfn takes(s: i32) {}\n",
        "rs",
        "ac1",
    );
    let mut mcp = McpClient::start(&live.socket);
    let tools = mcp.tools();
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 30, "{names:?}"); // with goto_step, check, point_at and clear_pointer
    for t in tools.as_array().unwrap() {
        assert!(
            !t["description"].as_str().unwrap_or("").is_empty(),
            "{}",
            t["name"]
        );
        jsonschema::validator_for(&t["inputSchema"])
            .unwrap_or_else(|e| panic!("{}: {e}", t["name"]));
    }
    // One call per family, all against the live document.
    let (err, text, _) = mcp.call("status", json!({}));
    assert!(!err && text.contains("\"lines\": 5"), "{text}");
    let (err, text, _) = mcp.call("get_code", json!({}));
    assert!(!err && text.contains("takes(s)"), "{text}");
    let (err, _, _) = mcp.call(
        "add_mark",
        json!({"id": "m", "kind": "move", "at": {"find": "takes(s)"}, "label": "moves"}),
    );
    assert!(!err);
    let (err, _, _) = mcp.call(
        "add_arrow",
        json!({"id": "a", "from": {"find": "let s"}, "to": {"find": "s: i32"}, "stroke": "dashed"}),
    );
    assert!(!err);
    let (err, _, _) = mcp.call(
        "add_box",
        json!({"id": "b", "from_line": 5, "to_line": 5, "label": "takes", "border": "double"}),
    );
    assert!(!err);
    let (err, _, _) = mcp.call("add_note", json!({"id": "n", "line": 3, "text": "gone"}));
    assert!(!err);
    let (err, _, _) = mcp.call(
        "add_callout",
        json!({"id": "c", "from_line": 1, "to_line": 4, "text": "main", "placement": "below"}),
    );
    assert!(!err);
    let (err, _, _) = mcp.call("highlight", json!({"id": "h", "at": {"find": "let s"}}));
    assert!(!err);
    let (err, _, _) = mcp.call(
        "add_spotlight",
        json!({"id": "s", "from_line": 5, "to_line": 5, "step": 2}),
    );
    assert!(!err);
    let (err, _, _) = mcp.call(
        "define_style",
        json!({"name": "io", "glyph": "⇄", "legend": "io"}),
    );
    assert!(!err);
    let (err, text, _) = mcp.call("list_kinds", json!({}));
    assert!(!err && text.contains("io"), "{text}");
    let (err, _, _) = mcp.call(
        "update_annotation",
        json!({"id": "m", "label": "renamed", "color": "blue"}),
    );
    assert!(!err);
    // A move through `at` is applied (moved here from the API tests: no other test
    // moves an annotation over the MCP with success).
    let (err, text, _) = mcp.call(
        "update_annotation",
        json!({"id": "m", "at": {"line": 5, "find": "s: i32"}}),
    );
    assert!(!err, "{text}");
    let list = live.api_ok("scene.list", json!({}));
    let m = list
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e.as_str())
        .find(|e| e.starts_with("m "))
        .unwrap_or_else(|| panic!("{list}"));
    assert!(m.contains("mark move L5 9..15"), "{m}");
    let (err, _, _) = mcp.call("set_step", json!({"step": 2}));
    assert!(!err);
    let (err, _, _) = mcp.call("prev_step", json!({}));
    assert!(!err);
    let (err, _, _) = mcp.call("next_step", json!({}));
    assert!(!err);
    let (err, _, _) = mcp.call("set_focus", json!({"from_line": 1, "to_line": 2}));
    assert!(!err);
    let (err, _, _) = mcp.call("set_theme", json!({"theme": "light"}));
    assert!(!err);
    let (err, text, _) = mcp.call("render_text", json!({"width": 100}));
    assert!(!err && text.contains("renamed"), "{text}");
    let (err, text, _) = mcp.call("list_annotations", json!({}));
    assert!(
        !err && text.contains("m ") && text.contains("callout"),
        "{text}"
    );
    let dir = scratch("export");
    let png = dir.join("x.png");
    let (err, _, _) = mcp.call(
        "export_image",
        json!({"path": png.to_str().unwrap(), "scale": 1}),
    );
    assert!(!err && png.exists());
    let (err, _, _) = mcp.call("delete_annotation", json!({"id": "n"}));
    assert!(!err);
    let (err, text, _) = mcp.call("clear_annotations", json!({"step": 2}));
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call("clear_annotations", json!({}));
    assert!(!err, "{text}");
    assert_eq!(live.with(|app| app.doc.annotations.len()), 0);
}

#[test]
fn no_instance_then_reconnection() {
    let dir = scratch("ac2");
    let sock = dir.join("later.sock");
    let mut mcp = McpClient::start(&sock);
    let (err, text, _) = mcp.call("status", json!({}));
    assert!(err, "must fail without an instance");
    assert!(
        text.contains("No Tiza instance")
            && text.contains("later.sock")
            && text.contains("tiza <file>"),
        "{text}"
    );
    // The instance appears later: the same MCP process serves it.
    let live = Live::open(
        tiza_core::Document::from_code("fn a() {}\n", "rs", None),
        dir.join("doc.tiza"),
        sock.clone(),
    );
    let (err, text, _) = mcp.call("status", json!({}));
    assert!(!err && text.contains("\"lines\": 1"), "{text}");
    // The instance goes away: explicit error again, then back when it returns.
    drop(live);
    let (err, text, _) = mcp.call("status", json!({}));
    assert!(err && text.contains("No Tiza instance"), "{text}");
    let _live = Live::open(
        tiza_core::Document::from_code("fn b() {}\n", "rs", None),
        dir.join("doc2.tiza"),
        sock,
    );
    let (err, text, _) = mcp.call("get_code", json!({}));
    assert!(!err && text.contains("fn b()"), "{text}");
}

#[test]
fn errors_carry_the_socket_code() {
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "ac3");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call(
        "add_mark",
        json!({"kind": "move", "at": {"find": "nowhere"}}),
    );
    assert!(err && text.contains("text_not_found"), "{text}");
    let (err, _, _) = mcp.call("add_note", json!({"id": "dup", "line": 1, "text": "a"}));
    assert!(!err);
    let (err, text, _) = mcp.call("add_note", json!({"id": "dup", "line": 1, "text": "b"}));
    assert!(err && text.contains("duplicate_id"), "{text}");
    let (err, text, _) = mcp.call("add_box", json!({"from_line": 1, "to_line": 99}));
    assert!(err && text.contains("out_of_range"), "{text}");
    let (err, text, _) = mcp.call("delete_annotation", json!({"id": "ghost"}));
    assert!(err && text.contains("not_found"), "{text}");
    let (err, text, _) = mcp.call("update_annotation", json!({"id": "dup", "to_line": 2}));
    assert!(err && text.contains("invalid_argument"), "{text}");
    // Parameter validation happens before the socket is touched.
    let (err, text, _) = mcp.call("update_annotation", json!({"id": "dup", "span": 1}));
    assert!(err && text.contains("unknown field"), "{text}");
    let (err, text, _) = mcp.call("add_box", json!({"from_line": "one", "to_line": 2}));
    assert!(
        err && (text.contains("invalid type") || text.contains("expected")),
        "{text}"
    );
    let (err, text, _) = mcp.call("set_theme", json!({"theme": "sepia"}));
    assert!(err && text.contains("unknown theme"), "{text}");
    let (err, text, _) = mcp.call("no_such_tool", json!({}));
    assert!(err, "{text}");
    assert_eq!(
        live.with(|app| app.doc.annotations.len()),
        1,
        "nothing leaked in"
    );
}

#[test]
fn replies_validate_against_the_published_schemas() {
    let live = Live::from_code(
        "fn main() {\n    let s = String::from(\"x\");\n    takes(s);\n}\nfn takes(s: String) {}\n",
        "rs",
        "ac4",
    );
    let mut mcp = McpClient::start(&live.socket);
    mcp.call("add_arrow", json!({"from": {"find": "let s"}, "to": {"find": "s: String"}, "label": "move", "stroke": "thick"}));
    mcp.call("add_callout", json!({"from_line": 1, "to_line": 4, "text": "whole main", "title": "main", "border": "double", "fill": true}));
    // The socket's scene.get → document validates against the document schema.
    let doc_validator = validator("schema/tiza.schema.json");
    let scene = live.api_ok("scene.get", json!({}));
    let errors: Vec<String> = doc_validator
        .iter_errors(&scene["document"])
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    // Every method's result validates against api.schema.json.
    let api: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("schema/api.schema.json")).unwrap(),
    )
    .unwrap();
    let result_schema = |m: &str| {
        let ms = api["methods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["method"] == m)
            .unwrap();
        jsonschema::validator_for(&ms["result"]).unwrap()
    };
    for (method, params) in [
        ("ping", json!({})),
        ("status", json!({})),
        ("scene.get", json!({})),
        ("scene.list", json!({})),
        ("code.get", json!({})),
        ("step.next", json!({})),
        ("render.text", json!({"width": 80})),
        ("style.list", json!({})),
        (
            "annotation.add",
            json!({"type": "note", "line": 1, "text": "n"}),
        ),
    ] {
        let v = live.api_ok(method, params);
        let errors: Vec<String> = result_schema(method)
            .iter_errors(&v)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{method}: {errors:?}\n{v}");
    }
    // And the params the MCP sends are valid annotation.add params.
    let add_params = {
        let ms = api["methods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["method"] == "annotation.add")
            .unwrap();
        jsonschema::validator_for(&ms["params"]).unwrap()
    };
    for p in [
        json!({"type": "arrow", "from": {"find": "a"}, "to": {"find": "b"}, "stroke": "dashed", "heads": {"start": "dot", "end": "arrow"}}),
        json!({"type": "callout", "from_line": 1, "text": "t", "placement": "below", "border": "thick"}),
        json!({"type": "box", "from_line": 1, "to_line": 2, "fill": false}),
    ] {
        let errors: Vec<String> = add_params.iter_errors(&p).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{p}: {errors:?}");
    }
}

#[test]
fn two_clients_interleave() {
    let live = Live::from_code("a\nb\nc\nd\n", "rs", "ac5");
    let mut one = McpClient::start(&live.socket);
    let mut two = McpClient::start(&live.socket);
    for i in 0..10 {
        let (e1, t1, _) = one.call(
            "add_note",
            json!({"id": format!("one-{i}"), "line": 1, "text": "one"}),
        );
        let (e2, t2, _) = two.call(
            "add_note",
            json!({"id": format!("two-{i}"), "line": 2, "text": "two"}),
        );
        assert!(!e1 && t1.contains(&format!("one-{i}")), "{t1}");
        assert!(!e2 && t2.contains(&format!("two-{i}")), "{t2}");
    }
    assert_eq!(live.with(|app| app.doc.annotations.len()), 20);
}
