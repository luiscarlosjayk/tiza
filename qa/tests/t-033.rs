//! t-033 — Mouse support (opt-in): click selects, drag creates a selection
//! for a box, an arrow or a highlight, wheel scrolls.
//!
//! Acceptance criteria:
//! - AC1 Mouse capture is opt-in: off by default, `--mouse` or `:mouse on`
//!   turns it on, `:mouse off` turns it off; with it off every mouse event is
//!   ignored (the terminal keeps its own selection).
//! - AC2 A left click moves the cursor to the clicked line and grapheme
//!   column (tabs and wide glyphs count by their display width); clicking a
//!   virtual row (label, box border) lands on the nearest code line above;
//!   clicking outside the canvas does nothing.
//! - AC3 Dragging selects: on one line a span (`h` highlights it, `a` picks
//!   it as an arrow end, `n` notes it), across lines a line range (`b` draws a
//!   box); releasing keeps the selection open and the status line lists the
//!   actions; Esc cancels.
//! - AC4 The wheel moves the cursor three lines per notch (the frame follows
//!   it), never past either end; mouse events are ignored while an overlay or the command line
//!   is open.
//! - AC5 Documentation: README says the mouse is opt-in and how to keep the
//!   terminal's native selection (Shift on Ghostty/WezTerm/Linux, Option on
//!   iTerm2/Terminal.app), and that Zed keeps Cmd+click for links.

use codedraw_qa::Live;
use codedraw_tui::app::{Mode, MouseKind, MouseOutcome, Overlay};
use crossterm::event::KeyCode;

const CODE: &str =
    "fn main() {\n\tlet s = 1;\n    名前 = 2;\n    takes(s);\n}\nfn takes(x: i32) {}\n";

fn live(name: &str) -> Live {
    let l = Live::from_code(CODE, "rs", name);
    l.command("mouse on");
    let _ = l.screen(80, 20); // a frame fills the hit map
    l
}

/// Screen column of grapheme `col` on `line`, from the frame's hit map.
fn screen_xy(live: &Live, line: u32, col: u32) -> (u16, u16) {
    live.with(move |a| {
        let row = a
            .hit
            .rows
            .iter()
            .position(|r| *r == Some(line))
            .expect("line on screen") as u16
            + a.hit.origin.1;
        let text = a.doc.line_text(line).unwrap().to_string();
        let x = codedraw_core::layout::col_to_x(&text, col, 4) as u16 + a.hit.code_x;
        (x, row)
    })
}

#[test]
fn ac1_opt_in() {
    let l = Live::from_code(CODE, "rs", "t033-optin");
    let _ = l.screen(80, 20);
    assert!(!l.with(|a| a.mouse), "off by default");
    let (x, y) = screen_xy(&l, 4, 4);
    let out = l.with(move |a| a.on_mouse(MouseKind::Down, x, y));
    assert_eq!(out, MouseOutcome::Ignored);
    assert_eq!(l.with(|a| a.cursor_line), 1);
    l.command("mouse on");
    let (msg, err) = l.message().unwrap();
    assert!(
        !err && msg.starts_with("mouse on") && msg.contains("Shift/Option"),
        "{msg}"
    );
    let out = l.with(move |a| a.on_mouse(MouseKind::Down, x, y));
    assert_eq!(out, MouseOutcome::Moved(4, 4));
    l.command("mouse off");
    assert!(!l.with(|a| a.mouse));
    l.command("mouse sideways");
    let (msg, err) = l.message().unwrap();
    assert!(err && msg.contains("usage: :mouse [on|off]"), "{msg}");
    l.command("mouse");
    assert_eq!(l.message().unwrap().0, "mouse off");
}

#[test]
fn ac2_click_maps_to_line_and_column() {
    let l = live("t033-click");
    // `takes(s)` on line 4: the `s` is grapheme 10.
    let (x, y) = screen_xy(&l, 4, 10);
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Down, x, y)),
        MouseOutcome::Moved(4, 10)
    );
    l.with(move |a| a.on_mouse(MouseKind::Up, x, y));
    assert_eq!(l.with(|a| (a.cursor_line, a.cursor_col)), (4, 10));
    assert_eq!(l.with(|a| a.mode.clone()), Mode::Normal);
    // A tab is 4 cells wide: one cell into `let` on line 2 is the `e` (col 2).
    let (x, y) = screen_xy(&l, 2, 1);
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Down, x + 1, y)),
        MouseOutcome::Moved(2, 2)
    );
    let (x, y) = screen_xy(&l, 2, 0);
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Down, x + 2, y)),
        MouseOutcome::Moved(2, 0),
        "inside the tab"
    );
    // Wide glyphs: `名前` are two cells each; the second half of 名 is still col 4.
    let (x, y) = screen_xy(&l, 3, 4);
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Down, x + 1, y)),
        MouseOutcome::Moved(3, 4)
    );
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Down, x + 2, y)),
        MouseOutcome::Moved(3, 5)
    );
    // Past the end of the line: the last grapheme.
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Down, x + 40, y)),
        MouseOutcome::Moved(3, 10)
    );
    // A virtual row (box top border) lands on the nearest code line above.
    l.command("box 4-6 fns");
    let _ = l.screen(80, 20);
    let (x, y) = screen_xy(&l, 4, 0);
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Down, x, y - 1)),
        MouseOutcome::Moved(3, 0),
        "border row above line 4"
    );
    // Outside the canvas (status bar) or left of the gutter: nothing.
    assert_eq!(
        l.with(|a| a.on_mouse(MouseKind::Down, 0, 19)),
        MouseOutcome::Ignored
    );
    let (ox, oy) = l.with(|a| a.hit.origin);
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Down, ox, oy)),
        MouseOutcome::Moved(1, 0),
        "gutter click snaps to column 0"
    );
    if ox > 0 {
        assert_eq!(
            l.with(move |a| a.on_mouse(MouseKind::Down, ox - 1, oy)),
            MouseOutcome::Ignored,
            "the padding is outside the canvas"
        );
    }
}

#[test]
fn ac3_drag_selects_and_actions_apply() {
    let l = live("t033-drag");
    // Drag over `takes` on line 4 (cols 4..9), then `h` highlights it.
    let (x0, y) = screen_xy(&l, 4, 4);
    let (x1, _) = screen_xy(&l, 4, 8);
    l.with(move |a| a.on_mouse(MouseKind::Down, x0, y));
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Drag, x1, y)),
        MouseOutcome::Selecting
    );
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Up, x1, y)),
        MouseOutcome::Selected
    );
    assert!(matches!(
        l.with(|a| a.mode.clone()),
        Mode::Select {
            linewise: false,
            ..
        }
    ));
    let (msg, _) = l.message().unwrap();
    assert!(msg.starts_with("selected: h highlight"), "{msg}");
    l.keys(&[KeyCode::Char('h')]);
    let anns = l.with(|a| a.doc.annotations.clone());
    assert_eq!(anns.len(), 1);
    assert_eq!(anns[0].shape.line_ranges(), vec![(4, 4)]);
    if let codedraw_core::Shape::Highlight { span, .. } = &anns[0].shape {
        assert_eq!((span.start, span.end), (4, 9), "takes");
    } else {
        panic!("highlight");
    }
    // Drag across lines 1..5, then `b` boxes them.
    let (xa, ya) = screen_xy(&l, 1, 0);
    let (xb, yb) = screen_xy(&l, 5, 0);
    l.with(move |a| a.on_mouse(MouseKind::Down, xa, ya));
    l.with(move |a| a.on_mouse(MouseKind::Drag, xb, yb));
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Up, xb, yb)),
        MouseOutcome::Selected
    );
    let (msg, _) = l.message().unwrap();
    assert!(msg.starts_with("lines 1-5 selected"), "{msg}");
    l.keys(&[KeyCode::Char('b')]);
    assert!(
        matches!(l.with(|a| a.mode.clone()), Mode::Command(ref c) if c == "box 1-5 "),
        "b pre-fills the command line for a label"
    );
    l.keys(&[KeyCode::Enter]);
    let anns = l.with(|a| a.doc.annotations.clone());
    assert_eq!(anns.len(), 2);
    assert_eq!(anns[1].shape.line_ranges(), vec![(1, 5)]);
    // Drag upward works the same (anchor after cursor).
    l.with(move |a| a.on_mouse(MouseKind::Down, xb, yb));
    l.with(move |a| a.on_mouse(MouseKind::Drag, xa, ya));
    l.with(move |a| a.on_mouse(MouseKind::Up, xa, ya));
    assert_eq!(
        l.with(|a| a.selection()),
        Some(codedraw_tui::app::Selection::Lines(1, 5))
    );
    l.keys(&[KeyCode::Esc]);
    assert_eq!(l.with(|a| a.mode.clone()), Mode::Normal);
    // Arrow: drag `s` on line 2, `a`; drag `x` on line 6, `a`.
    let (x0, y0) = screen_xy(&l, 2, 5);
    l.with(move |a| a.on_mouse(MouseKind::Down, x0, y0));
    l.with(move |a| a.on_mouse(MouseKind::Drag, x0 + 1, y0));
    l.with(move |a| a.on_mouse(MouseKind::Up, x0 + 1, y0));
    l.keys(&[KeyCode::Char('a')]);
    assert!(l.with(|a| a.arrow_from.is_some()));
    let (x1, y1) = screen_xy(&l, 6, 9);
    l.with(move |a| a.on_mouse(MouseKind::Down, x1, y1));
    l.with(move |a| a.on_mouse(MouseKind::Drag, x1 + 1, y1));
    l.with(move |a| a.on_mouse(MouseKind::Up, x1 + 1, y1));
    l.keys(&[KeyCode::Char('a')]);
    let anns = l.with(|a| a.doc.annotations.clone());
    assert_eq!(anns.len(), 3);
    assert_eq!(anns[2].shape.line_ranges(), vec![(2, 2), (6, 6)]);
    // A click without movement is not a selection.
    l.with(move |a| a.on_mouse(MouseKind::Down, x0, y0));
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Drag, x0, y0)),
        MouseOutcome::Ignored
    );
    assert_eq!(
        l.with(move |a| a.on_mouse(MouseKind::Up, x0, y0)),
        MouseOutcome::Ignored
    );
    assert_eq!(l.with(|a| a.mode.clone()), Mode::Normal);
}

#[test]
fn ac4_wheel_scrolls_and_overlays_block() {
    let code: String = (1..=60).map(|i| format!("line {i}\n")).collect();
    let l = Live::from_code(&code, "txt", "t033-wheel");
    l.command("mouse on");
    let _ = l.screen(80, 12);
    // The wheel moves the cursor three lines; the frame follows the cursor.
    assert_eq!(
        l.with(|a| a.on_mouse(MouseKind::ScrollDown, 10, 5)),
        MouseOutcome::Scrolled
    );
    assert_eq!(l.with(|a| a.cursor_line), 4);
    for _ in 0..40 {
        l.with(|a| a.on_mouse(MouseKind::ScrollDown, 10, 5));
    }
    assert_eq!(l.with(|a| a.cursor_line), 60, "never past the end");
    let screen = l.screen(80, 12);
    assert!(screen.contains("line 60"), "{screen}");
    assert_eq!(l.with(|a| a.scroll), 60 - 11, "viewport at the end");
    l.with(|a| a.on_mouse(MouseKind::ScrollUp, 10, 5));
    assert_eq!(l.with(|a| a.cursor_line), 57);
    for _ in 0..30 {
        l.with(|a| a.on_mouse(MouseKind::ScrollUp, 10, 5));
    }
    assert_eq!(l.with(|a| a.cursor_line), 1, "never before the start");
    // Overlays and the command line ignore the mouse.
    l.keys(&[KeyCode::Char('?')]);
    assert_eq!(l.with(|a| a.overlay), Overlay::Help);
    assert_eq!(
        l.with(|a| a.on_mouse(MouseKind::ScrollDown, 10, 5)),
        MouseOutcome::Ignored
    );
    l.keys(&[KeyCode::Esc]);
    l.keys(&[KeyCode::Char(':')]);
    assert!(matches!(l.with(|a| a.mode.clone()), Mode::Command(_)));
    assert_eq!(
        l.with(|a| a.on_mouse(MouseKind::Down, 10, 5)),
        MouseOutcome::Ignored
    );
}

#[test]
fn ac5_docs_and_flag() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    assert!(readme.contains(":mouse on") && readme.contains("--mouse"));
    assert!(readme.contains("Shift") && readme.contains("Option") && readme.contains("Cmd+click"));
    let help = codedraw_qa::codedraw_ok(&["--help"]);
    assert!(help.contains("--mouse"), "{help}");
}
