//! t-112 — `:numbers on|off` and the document field `numbers`.
//!
//! Acceptance criteria:
//! - AC1 `:numbers off` hides the gutter (code, lanes and boxes shift
//!   left), `:numbers on` restores it, `:numbers` reports; saved in the
//!   document (`numbers: false`, default true), so exports and the next
//!   session match.
//! - AC2 `render` honours the field; `--no-numbers` forces it off and
//!   `--numbers` forces it on.
//! - AC3 `FILE:A-B` ranges without numbers still know their real lines.
//! - AC4 `status.numbers`; `scene.patch {numbers: false}`; schema.
//! - AC5 This file; README; CHANGELOG.

use serde_json::json;
use tiza_qa::{Live, fixture, scratch, tiza, tiza_ok};

fn code_col(screen: &str) -> usize {
    screen
        .lines()
        .find(|l| l.contains("fn main()"))
        .map(|l| l.find("fn main()").unwrap())
        .expect("fn main() on screen")
}

#[test]
fn ac1_ac4_toggle_in_the_tui_is_saved_and_reported() {
    let dir = scratch("t112-live");
    let doc = tiza_core::fs::open(fixture("ownership/ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 7}));
    let with = live.screen(100, 40);
    assert!(with.contains(" 15 ") && with.contains("fn takes"), "{with}");
    let x_with = code_col(&with);
    live.command("numbers");
    assert_eq!(live.message().unwrap().0, "numbers on");

    live.command("numbers off");
    let (m, err) = live.message().unwrap();
    assert!(!err && m.starts_with("numbers off"), "{m}");
    let without = live.screen(100, 40);
    assert!(!without.contains(" 15 "), "gutter still there:\n{without}");
    let x_without = code_col(&without);
    assert!(x_without + 2 < x_with, "{x_without} vs {x_with}");
    // Boxes and arrows moved with the code: the takes box is still complete.
    assert!(without.contains("╭─ takes owns s") && without.contains("fn takes"));
    assert!(without.contains("╰─"), "{without}");
    // Status and the saved document carry the field.
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["numbers"], false);
    live.command("w");
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("doc.tiza")).unwrap()).unwrap();
    assert_eq!(saved["numbers"], false);
    // Undo brings the gutter back; redo hides it again.
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert!(live.message().unwrap().0.contains("undone: numbers"));
    assert!(live.screen(100, 40).contains(" 15 "));
    live.keys(&[crossterm::event::KeyCode::Char('U')]);
    assert!(!live.screen(100, 40).contains(" 15 "));

    // Agents: scene.patch and the exports follow the field.
    live.api_ok("scene.patch", json!({"numbers": true}));
    assert_eq!(live.api_ok("status", json!({}))["numbers"], true);
    assert!(live.screen(100, 40).contains(" 15 "));
    live.api_ok("scene.patch", json!({"numbers": false}));
    let txt = live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!txt.contains(" 15 ") && txt.contains("fn takes"), "{txt}");
    let out = dir.join("x.txt");
    live.command(&format!("export {}", out.display()));
    assert!(!std::fs::read_to_string(&out).unwrap().contains(" 15 "));

    live.command("numbers on");
    assert!(live.screen(100, 40).contains(" 15 "));
    live.command("numbers on");
    assert_eq!(live.message().unwrap().0, "numbers already on");
    live.command("numbers maybe");
    let (m, err) = live.message().unwrap();
    assert!(err && m.contains(":numbers on"), "{m}");
    // The saved file drops the field once it is back to the default.
    live.command("w");
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("doc.tiza")).unwrap()).unwrap();
    assert!(saved.get("numbers").is_none(), "{saved}");
}

#[test]
fn ac2_render_honours_the_field_and_the_flags_override_it() {
    let dir = scratch("t112-render");
    let src = fixture("ownership/ownership.rs.tiza");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&src).unwrap()).unwrap();
    doc["numbers"] = json!(false);
    doc["code"] = json!({"file": fixture("ownership/ownership.rs").to_str().unwrap()});
    let path = dir.join("no-numbers.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let p = path.to_str().unwrap();
    let plain = tiza_ok(&["render", p, "--step", "7", "--width", "100"]);
    assert!(
        !plain.contains(" 15 ") && plain.contains("fn takes"),
        "{plain}"
    );
    let forced = tiza_ok(&["render", p, "--step", "7", "--width", "100", "--numbers"]);
    assert!(forced.contains(" 15 "), "{forced}");
    let off = tiza_ok(&[
        "render",
        fixture("ownership/ownership.rs.tiza").to_str().unwrap(),
        "--step",
        "7",
        "--width",
        "100",
        "--no-numbers",
    ]);
    assert!(!off.contains(" 15 "), "{off}");
    let both = tiza(&["render", p, "--numbers", "--no-numbers"]);
    assert!(!both.status.success(), "the two flags conflict");
    // The cast and the GIF frames follow the field too.
    let cast = dir.join("x.cast");
    tiza_ok(&["render", p, "--out", cast.to_str().unwrap()]);
    assert!(!std::fs::read_to_string(&cast).unwrap().contains(" 15 "));
    // Old documents without the field: numbers on.
    let old = tiza_core::fs::open(fixture("ownership/ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document;
    assert!(old.numbers);
}

#[test]
fn ac3_a_range_without_numbers_still_knows_its_lines() {
    let dir = scratch("t112-range");
    let src = dir.join("main.rs");
    std::fs::copy(fixture("ownership/ownership.rs"), &src).unwrap();
    let opened = tiza_core::fs::open(&format!("{}:15-17", src.display())).unwrap();
    let live = Live::open(
        opened.document,
        dir.join("main.rs.tiza"),
        dir.join("live.sock"),
    );
    live.command("numbers off");
    live.keys(&[crossterm::event::KeyCode::Char('j')]);
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["numbers"], false);
    let s = live.screen(80, 20);
    assert!(s.contains("L16"), "{s}");
    // A text anchor still resolves to the real line.
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 16, "find": "println!(\"{s}\")"}, "label": "here"}),
    );
    assert!(r["id"].is_string(), "{r}");
    let list = live.api_ok("scene.list", json!({}));
    assert!(list.to_string().contains("L16"), "{list}");
}
