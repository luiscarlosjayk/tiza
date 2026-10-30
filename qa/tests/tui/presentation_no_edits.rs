//! Presenting, not editing: in presentation mode the keys that undo, redo
//! or edit from the list do nothing. `:undo` and `:redo` still work, and
//! show their result on the last row.

use crossterm::event::{KeyCode, KeyModifiers};
use tiza_qa::Live;

const CODE: &str = "fn main() {\n    let s = 1;\n    let t = 2;\n}\n";

/// Two notes at step 1 and the second one deleted, so that `u` has
/// something to undo; then presentation mode.
fn presenting(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.command("note 2 one");
    live.command("note 3 two");
    live.command("del a2");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    live.with(|a| a.set_present(true));
    live
}

#[test]
fn in_presentation_u_shift_u_and_ctrl_r_do_nothing() {
    // `u` with a delete to undo: nothing.
    let live = presenting("no-undo");
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1, "u");
    // In normal mode `u` undoes, as before; back in presentation mode,
    // `U` and Ctrl-R have that undo to redo, and do nothing.
    live.with(|a| a.set_present(false));
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2);
    live.with(|a| a.set_present(true));
    live.keys(&[KeyCode::Char('U')]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2, "U");
    live.with(|a| tiza_tui::on_key(a, KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2, "Ctrl-R");
}

#[test]
fn in_presentation_the_list_edits_nothing_and_undo_as_a_command_works() {
    let live = presenting("no-list-edit");
    // The list: `x` on the first row, `u`, `U` and Ctrl-R.
    live.keys(&[
        KeyCode::Char('L'),
        KeyCode::Char('x'),
        KeyCode::Char('u'),
        KeyCode::Char('U'),
    ]);
    live.with(|a| tiza_tui::on_key(a, KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    live.keys(&[KeyCode::Esc]);
    // `:undo` is a command: it undoes.
    live.command("undo");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2);
}

#[test]
fn in_presentation_a_result_names_undo_as_a_command() {
    // `u` does nothing while presenting, so a result that can be undone
    // names `:undo`, and `:undo` gives the annotation back.
    let message = |live: &Live| live.with(|a| a.message.as_ref().map(|(m, _)| m.clone()));
    let live = presenting("undo-hint");
    live.command("clear");
    assert_eq!(
        message(&live).as_deref(),
        Some("1 annotation(s) cleared (:undo to undo)")
    );
    live.command("undo");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    // In normal mode the message names `u`, as before.
    live.with(|a| a.set_present(false));
    live.command("clear");
    assert_eq!(
        message(&live).as_deref(),
        Some("1 annotation(s) cleared (u to undo)")
    );
}

#[test]
fn in_presentation_a_ref_that_drops_annotations_names_undo_as_a_command() {
    // `:ref` to code without the line of the note drops it: the message of
    // the drop names `:undo` too, and `:undo` gives the note back.
    let live = presenting("ref-undo-hint");
    let dir = live.with(|a| a.save_path.parent().unwrap().to_path_buf());
    std::fs::write(dir.join("other.rs"), "fn other() {}\n").unwrap();
    live.command("ref other.rs");
    let said = live
        .with(|a| a.message.as_ref().map(|(m, _)| m.clone()))
        .unwrap_or_default();
    assert!(
        said.contains("1 annotation dropped (their text is gone; :undo undoes)"),
        "{said}"
    );
    assert!(!said.contains("; u undoes"), "{said}");
    live.command("undo");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
}

#[test]
fn in_presentation_the_list_redoes_nothing() {
    // An undo in normal mode leaves something to redo; in presentation
    // mode the list's `U` and Ctrl-R do nothing.
    let live = presenting("no-list-redo");
    live.with(|a| a.set_present(false));
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2);
    live.with(|a| a.set_present(true));
    live.keys(&[KeyCode::Char('L'), KeyCode::Char('U')]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2, "U");
    live.with(|a| tiza_tui::on_key(a, KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2, "Ctrl-R");
}

#[test]
fn in_presentation_the_list_offers_only_the_keys_that_work() {
    let live = presenting("list-footer");
    live.keys(&[KeyCode::Char('L')]);
    let screen = live.screen(100, 24);
    assert!(
        screen.contains("select") && screen.contains("go"),
        "{screen}"
    );
    assert!(
        !screen.contains("delete") && !screen.contains("undo"),
        "{screen}"
    );
    live.keys(&[KeyCode::Esc]);
    live.with(|a| a.set_present(false));
    live.keys(&[KeyCode::Char('L')]);
    let screen = live.screen(100, 24);
    assert!(
        screen.contains("delete") && screen.contains("undo"),
        "{screen}"
    );
}
