//! t-092 — Callout annotation (bordered multi-line text block).
//!
//! Acceptance criteria (from the board):
//! - AC1 `placement: margin` draws a bordered block in the right margin aligned
//!   with `from_line`; labels of the lines it covers stay on their own line
//!   (beside the block when ≥ 14 cells remain, pushed down only otherwise).
//! - AC2 `placement: below` inserts the block's rows after `to_line`, inside the
//!   code column; a box that contains the callout's whole range extends around it.
//! - AC3 A below-callout wider than a box ending on `to_line` is drawn after that
//!   box's bottom border, without the box rail or background.
//! - AC4 `auto` picks margin only when the margin can hold the block and a label beside it.
//! - AC5 `:callout`, `C` on a selection, `annotation.add {type: callout}` and the
//!   MCP `add_callout` create one; `Enter` on a selected callout edits its title.
//! - AC6 Existing `.tiza` files load unchanged; `title`, `placement` and `width` are optional.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::{Live, fixture, scratch, tiza_ok};

fn render(step: &str, width: &str) -> String {
    tiza_ok(&[
        "render",
        fixture("t-092/callouts.tiza").to_str().unwrap(),
        "--step",
        step,
        "--width",
        width,
        "--out",
        "-",
    ])
}

#[test]
fn ac1_margin_block_keeps_the_following_label_on_its_line() {
    let out = render("1", "114");
    let lines: Vec<&str> = out.lines().collect();
    let top = lines
        .iter()
        .position(|l| l.contains("╭─ move ─"))
        .expect("block title");
    assert!(
        lines[top].starts_with(" 2") || lines[top].contains("let s"),
        "aligned with L2:\n{out}"
    );
    let l4 = lines
        .iter()
        .find(|l| l.contains("println!(\"{s}\")"))
        .unwrap();
    assert!(
        l4.contains("┆ does not compile"),
        "note stays on L4, beside the block:\n{out}"
    );
}

#[test]
fn ac2_below_block_inside_a_box_that_contains_it() {
    let out = render("2", "80");
    let lines: Vec<&str> = out.lines().collect();
    let block = lines
        .iter()
        .position(|l| l.contains("╭─ inside the box ─"))
        .unwrap();
    let bottom = lines
        .iter()
        .position(|l| l.trim_start().starts_with("╰───") && !l.contains("│  ╰"))
        .unwrap();
    assert!(
        block < bottom,
        "block row {block} comes before the box bottom {bottom}:\n{out}"
    );
    assert!(
        lines[block].trim_start().starts_with('│'),
        "the block row carries the box rail:\n{out}"
    );
}

#[test]
fn ac3_below_block_wider_than_the_box_goes_after_its_border() {
    let out = render("3", "80");
    let lines: Vec<&str> = out.lines().collect();
    let block = lines
        .iter()
        .position(|l| l.contains("╭─ whole main ─"))
        .unwrap();
    let box_bottom = lines
        .iter()
        .position(|l| l.trim_start().starts_with("╰───") && !l.contains("whole main"))
        .unwrap();
    assert!(
        box_bottom < block,
        "box closes before the wider callout:\n{out}"
    );
    assert!(
        lines[block].trim_start().starts_with('╭'),
        "no rail on the callout rows:\n{out}"
    );
}

#[test]
fn ac4_auto_follows_the_available_margin() {
    // Enough lines below for the block in the margin at 140 columns (with
    // three, it had no rows there and, since t-201, falls under its line at
    // every width as a block, as at 50).
    let live = Live::from_code(
        "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn e() {}\nfn f() {}\n",
        "rs",
        "t092-auto",
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "callout", "from_line": 1, "to_line": 2, "text": "auto placed paragraph with enough words to wrap twice over", "placement": "auto"}),
    );
    let wide = live.with(|app| app.layout(140).rows.len());
    let narrow = live.with(|app| app.layout(50).rows.len());
    assert!(
        narrow > wide,
        "narrow terminals insert rows below ({narrow} > {wide})"
    );
}

#[test]
fn ac5_command_selection_and_socket_create_callouts() {
    let live = Live::from_code(
        "fn main() {\n    let s = 1;\n    takes(s);\n}\n",
        "rs",
        "t092-cmd",
    );
    live.command("callout 2-3 @below title=\"why\" because s is moved");
    assert!(
        matches!(live.message(), Some((_, false))),
        "{:?}",
        live.message()
    );
    // Selection: V on L2, j, C → command pre-filled, then type the text.
    live.with(|app| app.cursor_line = 2);
    live.keys(&[KeyCode::Char('V'), KeyCode::Char('j'), KeyCode::Char('C')]);
    live.type_text("typed from the keyboard");
    live.keys(&[KeyCode::Enter]);
    let list = live.api_ok("scene.list", json!({}));
    let list = list.as_array().unwrap();
    assert!(
        list.iter()
            .any(|l| l.as_str().unwrap().contains("why: «because s is moved»")),
        "{list:?}"
    );
    assert!(
        list.iter()
            .any(|l| l.as_str().unwrap().contains("callout L2-3")
                && l.as_str().unwrap().contains("typed from the keyboard")),
        "{list:?}"
    );
    // Socket: out-of-range line is rejected.
    let r = live.api(
        "annotation.add",
        json!({"type": "callout", "from_line": 99, "text": "x"}),
    );
    assert_eq!(r.error.unwrap().code, "out_of_range");
    // Corner: empty text is rejected by the command.
    live.command("callout 2");
    assert!(matches!(live.message(), Some((_, true))));
}

#[test]
fn ac6_old_documents_load_and_optional_fields_default() {
    let dir = scratch("t092-compat");
    let path = dir.join("old.tiza");
    std::fs::write(&path, r#"{"schema":1,"source":{"language":"rs"},"code":["fn a() {}"],"annotations":[{"id":"n","step":1,"type":"note","line":1,"text":"old"}]}"#).unwrap();
    let out = tiza_ok(&["render", path.to_str().unwrap(), "--out", "-"]);
    assert!(out.contains("┆ old"));
    let doc = tiza_core::fs::parse_document(r#"{"schema":1,"source":{"language":"rs"},"code":["fn a() {}"],"annotations":[{"id":"c","step":1,"type":"callout","from_line":1,"text":"minimal"}]}"#, None).unwrap();
    match &doc.annotations[0].shape {
        tiza_core::model::Shape::Callout {
            title,
            placement,
            width,
            to_line,
            ..
        } => {
            assert!(title.is_none() && width.is_none() && to_line.is_none());
            assert_eq!(*placement, tiza_core::model::Placement::Margin);
        }
        other => panic!("{other:?}"),
    }
}
