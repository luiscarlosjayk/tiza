//! `←`/`→` changed the step in every mode, as a clicker sends them: after a
//! selection the presenter was back in normal mode without noticing, and
//! the arrows moved the take instead of the cursor.
//!
//! Acceptance criteria:
//! - AC1 Normal mode (also with an annotation selected with Tab): `←`/`→`
//!   move the cursor column, as `h`/`l`, and never change the step.
//! - AC2 Presentation mode (`P`): `←`/`→` change the step (`→` as `n`, `←`
//!   as `p`). PageDown/PageUp change the step in normal and in
//!   presentation mode.
//! - AC3 Selection mode: `←`/`→` move the column; PageUp/PageDown do not
//!   change the step and do not drop the selection.
//! - AC4 Keys typed before the TUI is ready follow the same rules.
//! - AC5 Help, README and CHANGELOG say so.
//! - AC6 This file: each key in each mode on a document of 3 steps, checked
//!   on the step and the cursor column.
//!
//! Corner cases: `→` at the end of a line stays (as `l`); Ctrl-→ and
//! Shift-→ do what `→` does (the modifiers are not read); `loop: true` with
//! PageDown past the last step is step 0, as `n`; the help and the list
//! keep scrolling with PageUp/PageDown.

use crossterm::event::{KeyCode, KeyModifiers};
use tiza_qa::Live;
use tiza_tui::app::{Mode, Overlay};

/// Three steps: 0 (the code), 1 and 2 (a note each). The cursor on line 2,
/// column 4, at step 1.
fn three_steps(name: &str) -> Live {
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", name);
    live.command("note 1 one");
    live.command("note 2 two");
    live.command("step a2 2");
    live.command("step 1");
    live.with(|a| {
        a.cursor_line = 2;
        a.cursor_col = 4;
    });
    live
}

fn step(live: &Live) -> u32 {
    live.with(|a| a.doc.step)
}

fn col(live: &Live) -> u32 {
    live.with(|a| a.cursor_col)
}

fn key(live: &Live, code: KeyCode, mods: KeyModifiers) {
    live.with(move |a| tiza_tui::on_key(a, code, mods));
}

#[test]
fn arrows_move_the_column_in_normal_mode() {
    let live = three_steps("normal");
    live.keys(&[KeyCode::Right, KeyCode::Right]);
    assert_eq!((step(&live), col(&live)), (1, 6));
    live.keys(&[KeyCode::Left]);
    assert_eq!((step(&live), col(&live)), (1, 5));
    // As h/l do.
    live.keys(&[KeyCode::Char('l')]);
    assert_eq!(col(&live), 6);
    live.keys(&[KeyCode::Char('h')]);
    assert_eq!(col(&live), 5);
}

#[test]
fn with_an_annotation_selected_the_arrows_move_the_column() {
    let live = three_steps("tab");
    live.keys(&[KeyCode::Tab]);
    assert!(live.with(|a| a.selected.is_some()), "Tab selects");
    live.keys(&[KeyCode::Right]);
    assert_eq!((step(&live), col(&live)), (1, 5));
    live.keys(&[KeyCode::Left, KeyCode::Left]);
    assert_eq!((step(&live), col(&live)), (1, 3));
}

#[test]
fn n_p_and_the_page_keys_change_the_step() {
    let live = three_steps("steps");
    live.keys(&[KeyCode::PageDown]);
    assert_eq!(step(&live), 2);
    live.keys(&[KeyCode::PageUp, KeyCode::PageUp]);
    assert_eq!(step(&live), 0);
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(step(&live), 1);
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!(step(&live), 0);
    assert_eq!(col(&live), 4, "no key here moved the column");
}

#[test]
fn in_presentation_mode_the_arrows_change_the_step() {
    let live = three_steps("present");
    live.keys(&[KeyCode::Char('P')]);
    assert!(live.with(|a| a.present));
    live.keys(&[KeyCode::Right]);
    assert_eq!((step(&live), col(&live)), (2, 4));
    live.keys(&[KeyCode::Left, KeyCode::Left]);
    assert_eq!((step(&live), col(&live)), (0, 4));
    live.keys(&[KeyCode::PageDown]);
    assert_eq!(step(&live), 1);
    live.keys(&[KeyCode::PageUp]);
    assert_eq!(step(&live), 0);
    // `P` again: they move the cursor.
    live.keys(&[KeyCode::Char('P'), KeyCode::Right]);
    assert!(live.with(|a| !a.present));
    assert_eq!((step(&live), col(&live)), (0, 5));
}

#[test]
fn in_selection_mode_the_page_keys_do_nothing() {
    let live = three_steps("select");
    live.keys(&[KeyCode::Char('v'), KeyCode::Right]);
    assert_eq!((step(&live), col(&live)), (1, 5));
    live.keys(&[KeyCode::PageDown, KeyCode::PageUp]);
    assert_eq!(step(&live), 1, "the step stays");
    assert!(
        live.with(|a| matches!(a.mode, Mode::Select { .. })),
        "the selection stays"
    );
    assert!(live.with(|a| a.selection().is_some()));
}

#[test]
fn early_keys_follow_the_same_rules() {
    // What the terminal sends for →, PageDown and ← before the TUI is
    // ready, turned into keys and handled as the TUI handles them.
    let live = three_steps("early");
    let keys = tiza_tui::early_keys::keys(b"\x1b[C\x1b[6~\x1b[D");
    assert_eq!(
        keys.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        [KeyCode::Right, KeyCode::PageDown, KeyCode::Left]
    );
    for (code, mods) in keys {
        key(&live, code, mods);
    }
    assert_eq!((step(&live), col(&live)), (2, 4));
}

#[test]
fn the_end_of_a_line_and_the_modifiers() {
    let live = three_steps("corners");
    // `    let s = 1;` has 14 columns: → at the last one stays, as `l`.
    live.with(|a| a.cursor_col = 13);
    live.keys(&[KeyCode::Right]);
    assert_eq!((step(&live), col(&live)), (1, 13));
    // Ctrl and Shift are not read: the same as → and ←.
    key(&live, KeyCode::Left, KeyModifiers::CONTROL);
    key(&live, KeyCode::Left, KeyModifiers::SHIFT);
    assert_eq!((step(&live), col(&live)), (1, 11));
    key(&live, KeyCode::Right, KeyModifiers::CONTROL);
    assert_eq!((step(&live), col(&live)), (1, 12));
}

#[test]
fn loop_page_down_past_the_last_step_is_step_0() {
    let live = three_steps("loop");
    live.api_ok("scene.patch", serde_json::json!({"loop": true}));
    live.keys(&[KeyCode::PageDown, KeyCode::PageDown]);
    assert_eq!(step(&live), 0, "{:?}", live.message());
}

#[test]
fn the_help_keeps_scrolling_with_the_page_keys() {
    let live = three_steps("help");
    live.keys(&[KeyCode::Char('?')]);
    assert_eq!(live.with(|a| a.overlay), Overlay::Help);
    // A page is the height of the help on screen.
    let _ = live.screen(80, 24);
    live.keys(&[KeyCode::PageDown]);
    assert_eq!(live.with(|a| a.overlay_scroll), 14, "one page at 80×24");
    live.keys(&[KeyCode::PageUp]);
    assert_eq!(live.with(|a| a.overlay_scroll), 0);
    assert_eq!(step(&live), 1);
}

#[test]
fn the_changelog_says_so() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |f: &str| std::fs::read_to_string(root.join(f)).unwrap();
    assert!(read("CHANGELOG.md").contains("To present with a clicker"));
}
