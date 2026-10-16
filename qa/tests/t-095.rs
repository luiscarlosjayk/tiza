//! t-095 — Box and callout border styles.
//!
//! Acceptance criteria:
//! - AC1 `border`: solid (default), dashed, dotted, thick, double; `corners`:
//!   rounded (default) or sharp (thick/double are always square); the label
//!   stays embedded in the top border.
//! - AC2 `fill`: boxes tint their inside by default and `fill: false` removes
//!   it; callouts are not tinted unless `fill: true`.
//! - AC3 Nested boxes keep their own rails and styles; a below-callout never
//!   grows past the code column (long titles are clipped).
//! - AC4 `:box A-B label border=… corners=… fill=…`, `:callout … border=…`,
//!   `:set ID border|corners|fill=…`, socket fields on annotation.add/update,
//!   MCP add_box/add_callout/update_annotation; wrong values list the options;
//!   border keys on an arrow or note fail naming the type.
//! - AC5 Files without the fields load and save unchanged.

use codedraw_qa::{Live, codedraw_ok, fixture};
use serde_json::json;

fn render() -> String {
    codedraw_ok(&[
        "render",
        fixture("t-095/borders.codedraw").to_str().unwrap(),
        "--width",
        "70",
        "--out",
        "-",
    ])
}

#[test]
fn ac1_strokes_and_corners_per_box() {
    let out = render();
    assert!(out.contains("┏━ thick sharp"), "{out}");
    assert!(out.contains("┗━━"), "{out}");
    assert!(out.contains("╔═ double ═"), "{out}");
    assert!(out.contains("║   fn b() {"), "{out}");
    assert!(out.contains("┌╌ dashed sharp ┐"), "{out}");
    assert!(out.contains("╎   fn c() {"), "{out}");
    assert!(out.contains("╭┄ dotted"), "{out}");
    assert!(out.contains("┆   fn d() {"), "{out}");
}

#[test]
fn ac2_fill_is_a_choice() {
    let live = Live::from_code(
        "a // a line long enough for a callout below it\nbb\ncc\ndd\n",
        "rs",
        "t095-fill",
    );
    live.command("box 1-2 tinted");
    live.command("box 3-4 plain fill=no");
    live.command("callout 1-2 @below title=\"t\" tinted text fill=yes");
    let canvas = live.with(|app| app.layout(60).canvas);
    let th = codedraw_core::Theme::dark();
    let bg_of = |text: &str| {
        for y in 0..canvas.height {
            let row = canvas.row_text(y);
            if let Some(x) = row.find(text) {
                let cx = row[..x].chars().count();
                return canvas.get(cx, y).and_then(|c| c.style.bg);
            }
        }
        None
    };
    assert!(bg_of("a //").is_some_and(|bg| bg != th.bg), "tinted box");
    assert!(bg_of("cc").is_none_or(|bg| bg == th.bg), "plain box");
    assert!(
        bg_of("tinted text").is_some_and(|bg| bg != th.bg),
        "filled callout"
    );
}

#[test]
fn ac3_nested_boxes_and_clipped_callout_titles() {
    let out = render();
    assert!(
        out.contains("┆╭─ inner ─"),
        "inner box keeps its own style inside the dotted one:\n{out}"
    );
    assert!(out.contains("┆╰─"), "{out}");
    // The double callout sits inside the double box: its right border is left of the box rail.
    let line = out
        .lines()
        .find(|l| l.contains("fil…"))
        .expect("clipped title");
    assert!(
        line.trim_end().ends_with('║'),
        "box rail still drawn: {line}"
    );
    assert!(
        line.matches('╗').count() == 1 && line.find('╗') < line.rfind('║'),
        "{line}"
    );
}

#[test]
fn ac4_commands_socket_and_mcp() {
    let live = Live::from_code(
        "fn main() {\n    let s = 1;\n}\nfn t() {}\n",
        "rs",
        "t095-cmd",
    );
    live.command("box 1-3 main border=double corners=sharp fill=no");
    assert!(
        matches!(live.message(), Some((_, false))),
        "{:?}",
        live.message()
    );
    let scene = live.api_ok("scene.get", json!({}));
    let a = &scene["document"]["annotations"][0];
    assert_eq!(a["label"], "main");
    assert_eq!(a["border"], "double");
    assert_eq!(a["corners"], "sharp");
    assert_eq!(a["fill"], false);
    let id = a["id"].as_str().unwrap().to_string();
    live.command(&format!("set {id} border=dashed fill=yes"));
    assert!(
        matches!(live.message(), Some((_, false))),
        "{:?}",
        live.message()
    );
    live.command(&format!("set {id} border=wavy"));
    let (m, err) = live.message().unwrap();
    assert!(
        err && m.contains("solid, dashed, dotted, thick, double"),
        "{m}"
    );
    live.command("note 2 n");
    live.command("set a2 border=thick");
    assert!(
        matches!(live.message(), Some((m, true)) if m.contains("boxes and callouts only")),
        "{:?}",
        live.message()
    );
    let r = live.api("annotation.update", json!({"id": "a2", "corners": "sharp"}));
    assert_eq!(r.error.unwrap().code, "invalid_argument");
    live.api_ok(
        "annotation.update",
        json!({"id": id, "corners": "rounded", "fill": false}),
    );
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(scene["document"]["annotations"][0]["border"], "dashed");
    assert_eq!(scene["document"]["annotations"][0]["fill"], false);
    assert!(
        scene["document"]["annotations"][0].get("corners").is_none(),
        "default not written"
    );
    // MCP
    let msgs = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"qa","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"add_box","arguments":{"id":"m","from_line":4,"to_line":4,"border":"thick","fill":false}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_callout","arguments":{"id":"c","from_line":1,"text":"hi","border":"dotted","corners":"sharp","fill":true}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"update_annotation","arguments":{"id":"m","corners":"zigzag"}}}),
    ];
    let input = msgs
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut child = std::process::Command::new(codedraw_qa::bin())
        .args(["mcp", "--socket", live.socket.to_str().unwrap()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let replies: Vec<serde_json::Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    for id in [2, 3] {
        let r = replies.iter().find(|r| r["id"] == id).unwrap();
        assert!(
            r["error"].is_null() && r["result"]["isError"] != true,
            "{r}"
        );
    }
    let r4 = replies.iter().find(|r| r["id"] == 4).unwrap();
    let msg = r4["error"]["message"]
        .as_str()
        .or(r4["result"]["content"][0]["text"].as_str())
        .unwrap_or("");
    assert!(msg.contains("rounded, sharp"), "{r4}");
    let scene = live.api_ok("scene.get", json!({}));
    let anns = scene["document"]["annotations"].as_array().unwrap();
    let m = anns.iter().find(|a| a["id"] == "m").unwrap();
    assert_eq!(m["border"], "thick");
    assert_eq!(m["fill"], false);
    let c = anns.iter().find(|a| a["id"] == "c").unwrap();
    assert_eq!(c["border"], "dotted");
    assert_eq!(c["corners"], "sharp");
    assert_eq!(c["fill"], true);
}

#[test]
fn ac5_defaults_are_not_written() {
    let doc = codedraw_core::fs::parse_document(
        r#"{"schema":1,"source":{"language":"rs"},"code":["a","b"],"annotations":[{"id":"x","step":1,"type":"box","from_line":1,"to_line":2},{"id":"c","step":1,"type":"callout","from_line":1,"text":"t"}]}"#,
        None,
    )
    .unwrap();
    let json = serde_json::to_string(&doc).unwrap();
    assert!(
        !json.contains("border") && !json.contains("corners") && !json.contains("fill"),
        "{json}"
    );
}
