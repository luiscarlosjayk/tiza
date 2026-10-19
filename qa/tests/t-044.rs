//! t-044 — MCP resources with the current code and annotations, and the
//! render tool so an agent sees the result.
//!
//! Acceptance criteria:
//! - AC1 `initialize` advertises the `resources` capability; `resources/list`
//!   returns `tiza://scene` (application/json), `tiza://code`
//!   (text/plain), `tiza://render` (text/plain) and `tiza://render.md`
//!   (text/markdown), each with a description.
//! - AC2 `resources/read` returns the live content: the scene as the same
//!   JSON `scene.get` gives (code embedded), the code with line numbers, the
//!   rendered frame of the current step, and the frame inside a ```text
//!   fence with the file name and step above. Reading again after a change
//!   reflects it (no caching).
//! - AC3 An unknown URI → JSON-RPC error `resource not found` that lists the
//!   available URIs; no instance → the same connection error the tools give.
//! - AC4 The `render_text` tool still returns the frame (regression guard)
//!   and the server instructions mention the resources.

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient};

#[test]
fn ac1_resources_are_listed_with_types() {
    let live = Live::from_code("fn a() {}\n", "rs", "t044-list");
    let mut mcp = McpClient::start(&live.socket);
    let init = mcp.initialize_result();
    assert!(
        init["capabilities"]["resources"].is_object(),
        "resources capability: {init}"
    );
    assert!(
        init["instructions"]
            .as_str()
            .unwrap_or("")
            .contains("tiza://scene")
    );
    let list = mcp.request("resources/list", json!({}));
    let res = list["result"]["resources"].as_array().expect("resources");
    let by_uri: std::collections::BTreeMap<&str, &Value> = res
        .iter()
        .map(|r| (r["uri"].as_str().unwrap(), r))
        .collect();
    assert_eq!(
        by_uri.keys().copied().collect::<Vec<_>>(),
        vec![
            "tiza://code",
            "tiza://render",
            "tiza://render.md",
            "tiza://scene"
        ]
    );
    assert_eq!(by_uri["tiza://scene"]["mimeType"], "application/json");
    assert_eq!(by_uri["tiza://render.md"]["mimeType"], "text/markdown");
    for r in res {
        assert!(
            r["description"].as_str().is_some_and(|d| d.len() > 10),
            "{r}"
        );
        assert!(r["name"].as_str().is_some());
    }
}

#[test]
fn ac2_reads_return_the_live_content() {
    let live = Live::from_code(
        "fn main() {\n    let s = 1;\n    takes(s);\n}\n",
        "rs",
        "t044-read",
    );
    live.command("note 2 born");
    let mut mcp = McpClient::start(&live.socket);
    let read = |mcp: &mut McpClient, uri: &str| -> (String, String) {
        let v = mcp.request("resources/read", json!({"uri": uri}));
        assert!(v["error"].is_null(), "{v}");
        let c = &v["result"]["contents"][0];
        assert_eq!(c["uri"], uri);
        (
            c["mimeType"].as_str().unwrap().to_string(),
            c["text"].as_str().unwrap().to_string(),
        )
    };
    let (mime, scene) = read(&mut mcp, "tiza://scene");
    assert_eq!(mime, "application/json");
    let scene: Value = serde_json::from_str(&scene).unwrap();
    let direct = live.api_ok("scene.get", json!({}));
    assert_eq!(scene["document"], direct["document"]);
    assert_eq!(
        scene["document"]["code"][1], "    let s = 1;",
        "code embedded"
    );
    assert_eq!(scene["document"]["annotations"][0]["text"], "born");

    let (mime, code) = read(&mut mcp, "tiza://code");
    assert_eq!(mime, "text/plain");
    assert!(code.contains("   2      let s = 1;"), "{code}");

    let (_, frame) = read(&mut mcp, "tiza://render");
    assert!(
        frame.contains("born") && frame.contains("takes(s)"),
        "{frame}"
    );
    let (mime, md) = read(&mut mcp, "tiza://render.md");
    assert_eq!(mime, "text/markdown");
    // Code from stdin has no file: just the fence. A file-backed document
    // puts its name and the step above (see below).
    assert!(md.starts_with("```text\n"), "{md}");
    assert!(md.trim_end().ends_with("```"), "{md}");

    // No caching: a change is visible on the next read.
    live.command("note 3 moved");
    let (_, frame2) = read(&mut mcp, "tiza://render");
    assert!(frame2.contains("moved"), "{frame2}");
    let (_, scene2) = read(&mut mcp, "tiza://scene");
    assert!(scene2.contains("\"moved\""));

    // File-backed document: the Markdown view is titled with the file name and step.
    let doc = tiza_core::fs::open(tiza_qa::fixture("t-011/repeated.rs").to_str().unwrap())
        .unwrap()
        .document;
    let dir = tiza_qa::scratch("t044-file");
    let live2 = Live::open(doc, dir.join("repeated.rs.tiza"), dir.join("live.sock"));
    let mut mcp2 = McpClient::start(&live2.socket);
    let (_, md) = read(&mut mcp2, "tiza://render.md");
    assert!(
        md.starts_with("**repeated.rs** (step 1)\n\n```text\n"),
        "{md}"
    );
}

#[test]
fn ac3_unknown_uri_and_no_instance_are_errors() {
    let live = Live::from_code("fn a() {}\n", "rs", "t044-errors");
    let mut mcp = McpClient::start(&live.socket);
    let v = mcp.request("resources/read", json!({"uri": "tiza://nope"}));
    let msg = v["error"]["message"].as_str().unwrap_or("");
    // The URI asked is not repeated (t-167 rework: an unknown name among a
    // fixed set is answered with the valid ones only).
    assert!(
        msg.contains("unknown resource") && !msg.contains("nope") && msg.contains("tiza://scene"),
        "{v}"
    );
    assert_eq!(
        v["error"]["code"],
        json!(-32002),
        "resource not found code: {v}"
    );
    drop(live);
    // The instance is gone: the read reports the connection problem like a tool.
    let v = mcp.request("resources/read", json!({"uri": "tiza://code"}));
    assert!(!v["error"].is_null(), "{v}");
    let msg = v["error"]["message"].as_str().unwrap_or("").to_lowercase();
    assert!(msg.contains("instance") || msg.contains("socket"), "{v}");
}

#[test]
fn ac4_render_text_tool_still_works() {
    let live = Live::from_code("fn a() {}\nfn b() {}\n", "rs", "t044-tool");
    live.command("box 1-2 both");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("render_text", json!({}));
    assert!(!err, "{text}");
    assert!(text.contains("both") && text.contains("fn b()"), "{text}");
    assert!(mcp.tool_names().contains(&"render_text".to_string()));
}
