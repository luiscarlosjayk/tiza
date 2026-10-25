//! Texts and messages that misled a user or an agent (found while the
//! documentation pages were checked against the code).
//!
//! Acceptance criteria:
//! - AC1 The review footer says what `a` and `d` do (`a accept all`,
//!   `d reject all`). The footer test of t-239 checks it.
//! - AC2 `annotation.update` with `text` says what to send: `label` for the
//!   text of a note or the title of a callout, a new `shape` for the text
//!   of a callout. The MCP tool, which has no `shape`, says to remove the
//!   callout and add it again, and names no field that it does not take.
//! - AC3 An argument that an MCP tool refuses itself (border, corners,
//!   emphasis, stroke, route, a head, placement, the path, theme and scale
//!   of export_image) ends with `(invalid_argument)`, as a refusal of the
//!   instance does, so that an agent can tell the cases apart.

use serde_json::json;
use tiza_core::model::Shape;
use tiza_qa::{Live, McpClient};

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

#[test]
fn ac2_text_on_update_says_what_to_send() {
    let live = Live::from_code(CODE, "rs", "t257-text");
    let id = live.api_ok(
        "annotation.add",
        json!({"type": "callout", "from_line": 1, "text": "body", "title": "head"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = live.api("annotation.update", json!({"id": id, "text": "x"}));
    let e = r.error.expect("text was accepted");
    assert_eq!(e.code, "invalid_argument");
    assert!(
        e.message.contains("title of a callout") && e.message.contains("as a new `shape`"),
        "{}",
        e.message
    );
    // `label` changes the title of a callout, not its text.
    live.api_ok("annotation.update", json!({"id": id, "label": "new head"}));
    let shape = {
        let id = id.clone();
        live.with(move |a| a.doc.find_annotation(&id).unwrap().shape.clone())
    };
    match shape {
        Shape::Callout { text, title, .. } => {
            assert_eq!(
                (text.as_str(), title.as_deref()),
                ("body", Some("new head"))
            );
        }
        other => panic!("{other:?}"),
    }
    // The MCP tool refuses it itself, with the code, and names no field
    // that only the socket takes.
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("update_annotation", json!({"id": id, "text": "x"}));
    assert!(err, "{text}");
    assert!(text.contains("(invalid_argument)"), "{text}");
    assert!(text.contains("remove it and add it again"), "{text}");
    for socket_only in ["`shape`", "`move`", "`heads`"] {
        assert!(!text.contains(socket_only), "{socket_only}: {text}");
    }
}

#[test]
fn ac3_every_refusal_of_the_mcp_tools_has_its_code() {
    let live = Live::from_code(CODE, "rs", "t257-codes");
    let mut mcp = McpClient::start(&live.socket);
    let out = tiza_qa::scratch("t257-codes-out");
    let line = |n: u32| json!({"line": n});
    let cases = [
        (
            "add_box",
            json!({"from_line": 1, "to_line": 2, "border": "zigzag"}),
        ),
        (
            "add_box",
            json!({"from_line": 1, "to_line": 2, "corners": "zigzag"}),
        ),
        ("highlight", json!({"at": line(2), "emphasis": "zigzag"})),
        (
            "add_arrow",
            json!({"from": line(1), "to": line(2), "stroke": "zigzag"}),
        ),
        (
            "add_arrow",
            json!({"from": line(1), "to": line(2), "route": "zigzag"}),
        ),
        (
            "add_arrow",
            json!({"from": line(1), "to": line(2), "head_end": "zigzag"}),
        ),
        (
            "add_callout",
            json!({"from_line": 1, "text": "t", "placement": "zigzag"}),
        ),
        (
            "export_image",
            json!({"path": out.join("a.bmp").to_str().unwrap()}),
        ),
        (
            "export_image",
            json!({"path": out.join("a.png").to_str().unwrap(), "theme": "zigzag"}),
        ),
        (
            "export_image",
            json!({"path": out.join("a.png").to_str().unwrap(), "scale": 99}),
        ),
    ];
    for (tool, args) in cases {
        let (err, text, _) = mcp.call(tool, args.clone());
        assert!(err, "{tool} {args}: accepted: {text}");
        assert!(
            text.ends_with("(invalid_argument)"),
            "{tool} {args}: {text}"
        );
    }
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
}
