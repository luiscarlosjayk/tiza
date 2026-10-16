//! t-028 — Steps and the keys that move them.
//!
//! Acceptance criteria:
//! - AC1 `n`/`p`, `→`/`←` and PageDown/PageUp move the step; `h`/`l` move the column.
//! - AC2 Step 0 shows only the code; the document remembers its step.

use codedraw_qa::Live;
use crossterm::event::KeyCode;

#[test]
fn ac1_arrows_change_the_step_and_hl_the_column() {
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "t028-keys");
    live.command("note 1 a");
    live.command("note 2 b");
    live.command("step a2 2");
    live.command("step 0");
    live.keys(&[KeyCode::Right]);
    assert_eq!(live.with(|app| app.doc.step), 1);
    live.keys(&[KeyCode::PageDown]);
    assert_eq!(live.with(|app| app.doc.step), 2);
    live.keys(&[KeyCode::Left, KeyCode::Left, KeyCode::Left]);
    assert_eq!(live.with(|app| app.doc.step), 0, "never below 0");
    live.with(|app| app.cursor_line = 2);
    let col = live.with(|app| app.cursor_col);
    live.keys(&[KeyCode::Char('l')]);
    assert_eq!(live.with(|app| app.cursor_col), col + 1);
    assert_eq!(
        live.with(|app| app.doc.step),
        0,
        "l does not touch the step"
    );
}
