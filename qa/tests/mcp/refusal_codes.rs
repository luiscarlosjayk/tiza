//! Every refusal of the MCP tools carries its error code.

use serde_json::json;
use tiza_qa::{Live, McpClient};

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

#[test]
fn every_refusal_of_the_mcp_tools_has_its_code() {
    let live = Live::from_code(CODE, "rs", "codes");
    let mut mcp = McpClient::start(&live.socket);
    let out = tiza_qa::scratch("codes-out");
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
        // A move of a newer Tiza: a file can hold it, a tool cannot add it.
        ("add_move", json!({"at_step": 1, "action": "teleport"})),
        (
            "add_move",
            json!({"at_step": 1, "action": "scroll", "line": 1, "align": "middle"}),
        ),
        (
            "add_move",
            json!({"at_step": 1, "op": "insert", "position": "1", "content": "x", "typing": "word"}),
        ),
        (
            "add_move",
            json!({"at_step": 1, "op": "swap", "find": "let a"}),
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
    assert_eq!(live.with(|a| a.doc.moves.len()), 0);
}

/// `export_image` checks a number only for the formats that use it, as
/// `tiza render` does: the scale of the pixels, the font size of an image.
#[test]
fn export_image_checks_the_scale_only_for_pixels() {
    let live = Live::from_code(CODE, "rs", "scale-txt");
    let mut mcp = McpClient::start(&live.socket);
    let out = tiza_qa::scratch("scale-txt-out");
    for file in ["a.txt", "a.md", "a.ansi"] {
        let path = out.join(file);
        let args = json!({"path": path.to_str().unwrap(), "scale": 99, "font_size": 0});
        let (err, text, _) = mcp.call("export_image", args);
        assert!(!err && path.exists(), "{file}: {text}");
    }
    let (err, text, _) = mcp.call(
        "export_image",
        json!({"path": out.join("a.svg").to_str().unwrap(), "font_size": 0}),
    );
    assert!(err && text.ends_with("(invalid_argument)"), "svg: {text}");
}
