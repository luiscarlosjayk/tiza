//! The border of a box on the row of a blank line beside it, in the TUI: the
//! cursor on that blank line does not cover the border (its line number
//! shows where it is), and the view keeps that row with the first line of
//! the box, as it keeps a top border that has a row of its own.

use ratatui::style::Modifier;
use tiza_qa::{Live, fixture, scratch};

/// `shared/box-later.rs.tiza` at step 2: the box of L7-9 has its top border on the row
/// of the blank L6 and its bottom border on the row of the blank L10.
fn later(test: &str) -> Live {
    let dir = scratch(test);
    let doc = tiza_core::fs::load_document(&fixture("shared/box-later.rs.tiza")).unwrap();
    let live = Live::open(doc, dir.join("later.rs.tiza"), dir.join("s.sock"));
    live.with(|a| a.autosave = false);
    live.command("step 2");
    live
}

/// The text of screen row `y` and whether any cell of it is reversed.
fn row(b: &ratatui::buffer::Buffer, y: u16) -> (String, bool) {
    let mut text = String::new();
    let mut reversed = false;
    for x in 0..b.area.width {
        let c = &b[(x, y)];
        text.push_str(c.symbol());
        reversed |= c.modifier.contains(Modifier::REVERSED);
    }
    (text, reversed)
}

fn screen_row(b: &ratatui::buffer::Buffer, line: u32) -> u16 {
    (0..b.area.height)
        .find(|&y| row(b, y).0.split_whitespace().next() == Some(line.to_string().as_str()))
        .unwrap_or_else(|| panic!("no line {line}"))
}

#[test]
fn the_cursor_on_a_blank_line_does_not_cover_the_border_on_it() {
    let live = later("box-border-cursor");
    for line in [6, 10] {
        live.with(move |a| {
            a.cursor_line = line;
            a.cursor_col = 0;
        });
        let b = live.buffer(60, 20);
        let (text, reversed) = row(&b, screen_row(&b, line));
        assert!(text.contains('╭') || text.contains('╰'), "L{line}: {text}");
        assert!(!reversed, "L{line}: the cursor covers the border: {text}");
    }
    // On a line of code the cursor is a reversed cell, as before.
    live.with(|a| {
        a.cursor_line = 7;
        a.cursor_col = 0;
    });
    let b = live.buffer(60, 20);
    assert!(row(&b, screen_row(&b, 7)).1);
}

#[test]
fn the_view_keeps_the_border_on_a_blank_line_with_the_first_line_of_the_box() {
    let live = later("box-border-view");
    // A short view: the cursor goes to the end, then back up to L7.
    live.with(|a| a.cursor_line = 13);
    live.screen(60, 8);
    live.with(|a| a.cursor_line = 7);
    let s = live.screen(60, 8);
    let first = s.lines().next().unwrap();
    assert!(first.contains("╭─ takes owns s"), "{s}");
}
