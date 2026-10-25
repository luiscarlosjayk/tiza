//! t-094 — Arrow styles: stroke, corners and heads (Figma-like, cell-grid version).
//!
//! Acceptance criteria:
//! - AC1 `stroke`: solid (default), dashed (╌╎), dotted (┄┆), thick (━┃), double (═║);
//!   corners follow the stroke (thick/double have no rounded corners).
//! - AC2 `route`: rounded (╭╰, default) or sharp (┌└).
//! - AC3 `heads.start` / `heads.end`: none | arrow | dot | bar | open; default
//!   start none, end arrow; none+none is a plain connector. Same-line arrows too.
//! - AC4 (amended in t-091, 2026-10-17) Where a stub crosses another arrow's
//!   rail it breaks for one cell and the rail shows through (the arrow passes
//!   under); crossings with a box border pick the junction by the two
//!   strokes (┼ ┿ ╂ ╋ ╪ ╫ ╬).
//! - AC5 `:arrow … stroke=… heads=… route=…`, `:set ID key=value` (and `e` on a
//!   selected annotation), socket fields, MCP add_arrow/update_annotation;
//!   wrong values list the options; style keys on a non-arrow fail naming its type.
//! - AC6 Documents without the fields load unchanged and are saved without them.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::{Live, fixture, tiza_ok};

fn render() -> String {
    tiza_ok(&[
        "render",
        fixture("t-094/arrows.tiza").to_str().unwrap(),
        "--width",
        "90",
        "--out",
        "-",
    ])
}

fn line(out: &str, n: usize) -> String {
    out.lines()
        .find(|l| l.trim_start().starts_with(&format!("{n} ")))
        .unwrap_or_else(|| panic!("no line {n}:\n{out}"))
        .to_string()
}

#[test]
fn ac1_ac2_strokes_and_corners() {
    let out = render();
    assert!(
        line(&out, 2).contains("╭─"),
        "solid rounded: {}",
        line(&out, 2)
    );
    assert!(
        line(&out, 3).contains("─│─") && !line(&out, 3).contains('┼'),
        "the connector passes under the solid vertical: {}",
        line(&out, 3)
    );
    assert!(line(&out, 5).contains("╭╌"), "dashed: {}", line(&out, 5));
    assert!(
        line(&out, 6).contains("╎"),
        "dashed vertical: {}",
        line(&out, 6)
    );
    assert!(
        line(&out, 8).contains("┌┄"),
        "dotted + sharp: {}",
        line(&out, 8)
    );
    assert!(
        line(&out, 9).contains("┆"),
        "dotted vertical: {}",
        line(&out, 9)
    );
    assert!(
        line(&out, 11).contains("┏━"),
        "thick corners are square: {}",
        line(&out, 11)
    );
    assert!(
        line(&out, 5).contains("┃"),
        "thick vertical (the long red one): {}",
        line(&out, 5)
    );
    assert!(
        line(&out, 14).contains("╔═"),
        "double corners: {}",
        line(&out, 14)
    );
    assert!(
        line(&out, 15).contains("║"),
        "double vertical: {}",
        line(&out, 15)
    );
}

#[test]
fn ac3_heads_at_both_ends_and_plain_connector() {
    let out = render();
    assert!(
        line(&out, 4).contains("▶"),
        "default end head: {}",
        line(&out, 4)
    );
    assert!(
        line(&out, 11).contains("●"),
        "dot start head: {}",
        line(&out, 11)
    );
    assert!(
        line(&out, 13).contains("┤"),
        "bar end head: {}",
        line(&out, 13)
    );
    assert!(
        line(&out, 14).contains(">") && line(&out, 16).contains(">"),
        "open heads"
    );
    // Plain connector L3→L18: no head on L18's lane row.
    let l18 = line(&out, 18);
    assert!(
        !l18.contains('▶') && l18.contains("───"),
        "connector: {l18}"
    );
    // Same-line dashed with arrow start and dot end.
    let under = out
        .lines()
        .find(|l| l.contains("◀╌") && l.contains('●'))
        .expect("same-line arrow row");
    assert!(under.contains("◀╌╌"), "{under}");
}

#[test]
fn ac4_stubs_pass_under_rails_and_box_borders_get_junctions() {
    let out = render();
    let l3 = line(&out, 3);
    assert!(
        l3.contains("─┃─│─") && !l3.contains('╂') && !l3.contains('┼'),
        "light horizontal under a thick and a light rail: {l3}"
    );
    let l12 = line(&out, 12);
    assert!(
        l12.contains("━┃━") && !l12.contains('╋'),
        "thick under thick: {l12}"
    );
    let live = Live::from_code("a\nb\nc\nd\ne\n", "rs", "t094-junction");
    live.command("arrow 2 5 x stroke=thick");
    live.command("arrow 1 3 y stroke=double heads=none,none");
    let s = live.with(|app| app.layout(60).canvas.to_plain_text());
    assert!(
        s.contains("━║━") && !s.contains('╫'),
        "thick horizontal under a double rail:\n{s}"
    );
    // A box border is not a rail: the stub crosses it with a junction glyph.
    let live = Live::from_code("a\nb\nc\nd\ne\n", "rs", "t094-box-junction");
    live.command("box 2-4 b");
    live.command("arrow 3 5 z");
    let s = live.with(|app| app.layout(60).canvas.to_plain_text());
    assert!(s.contains('┼'), "stub × box border:\n{s}");
}

#[test]
fn ac5_commands_socket_and_mcp_set_styles() {
    let live = Live::from_code(
        "fn main() {\n    let s = 1;\n    takes(s);\n}\nfn takes(s: i32) {}\n",
        "rs",
        "t094-cmd",
    );
    live.command("arrow 2:s 5:s move stroke=dashed heads=dot,bar route=sharp");
    assert!(
        matches!(live.message(), Some((_, false))),
        "{:?}",
        live.message()
    );
    let scene = live.api_ok("scene.get", json!({}));
    let a = &scene["document"]["annotations"][0];
    assert_eq!(
        a["label"], "move",
        "trailing key=value are not part of the label"
    );
    assert_eq!(a["stroke"], "dashed");
    assert_eq!(a["route"], "sharp");
    assert_eq!(a["heads"], json!({"start": "dot", "end": "bar"}));
    let id = a["id"].as_str().unwrap().to_string();
    // :set changes one key at a time; wrong values list the options.
    live.command(&format!("set {id} stroke=thick end=open"));
    assert!(
        matches!(live.message(), Some((_, false))),
        "{:?}",
        live.message()
    );
    live.command(&format!("set {id} stroke=wavy"));
    let (m, err) = live.message().unwrap();
    assert!(
        err && m.contains("solid, dashed, dotted, thick, double"),
        "{m}"
    );
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(scene["document"]["annotations"][0]["stroke"], "thick");
    assert_eq!(scene["document"]["annotations"][0]["heads"]["end"], "open");
    assert_eq!(
        scene["document"]["annotations"][0]["heads"]["start"], "dot",
        "start kept"
    );
    // Socket update with heads patch and a style key on a non-arrow.
    live.api_ok(
        "annotation.update",
        json!({"id": id, "heads": {"start": "none"}, "route": "rounded"}),
    );
    live.command("note 3 n");
    let r = live.api("annotation.update", json!({"id": "a2", "stroke": "dashed"}));
    let e = r.error.unwrap();
    assert_eq!(e.code, "invalid_argument");
    assert!(
        e.message.contains("arrows only") && e.message.contains("note"),
        "{}",
        e.message
    );
    live.command("set a2 stroke=dashed");
    assert!(
        matches!(live.message(), Some((m, true)) if m.contains("not an arrow")),
        "{:?}",
        live.message()
    );
    // `e` on a selected annotation pre-fills :set.
    live.keys(&[KeyCode::Tab, KeyCode::Char('e')]);
    let mode = live.with(|app| app.mode.clone());
    assert!(
        matches!(mode, tiza_tui::app::Mode::Command(ref c) if c.starts_with("set ")),
        "{mode:?}"
    );
    live.keys(&[KeyCode::Esc]);
    // MCP add_arrow with styles and a bad value.
    let msgs = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"qa","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"add_arrow","arguments":{"id":"m","from":{"find":"let s"},"to":{"find":"takes(s: i32)"},"stroke":"dotted","head_end":"none","head_start":"dot"}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"update_annotation","arguments":{"id":"m","route":"zigzag"}}}),
    ];
    let input = msgs
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut child = std::process::Command::new(tiza_qa::bin())
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
    let r2 = replies.iter().find(|r| r["id"] == 2).unwrap();
    assert!(
        r2["error"].is_null() && r2["result"]["isError"] != true,
        "{r2}"
    );
    let r3 = replies.iter().find(|r| r["id"] == 3).unwrap();
    let msg = r3["error"]["message"]
        .as_str()
        .or(r3["result"]["content"][0]["text"].as_str())
        .unwrap_or("");
    assert!(msg.contains("rounded, sharp"), "{r3}");
    let scene = live.api_ok("scene.get", json!({}));
    let m = scene["document"]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "m")
        .unwrap();
    assert_eq!(m["stroke"], "dotted");
    assert_eq!(m["heads"], json!({"start": "dot", "end": "none"}));
}
