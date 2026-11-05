//! `J` and `K` go half the rows of the code on screen: the terminal less
//! the status bar (normal mode), the row of the step counter (presentation
//! mode, when it shows and no padding is at the bottom) and the padding.
//! The same count moves the cursor in normal mode and in presentation mode
//! with the live pointer, and the view in presentation mode without it.
//! It is one row at least.

use crossterm::event::KeyCode;
use tiza_qa::Live;

const COLS: u16 = 100;

/// Where `J` and `K` act.
#[derive(Clone, Copy, Debug)]
enum Mode {
    Normal,
    PresentPointer,
    PresentView,
}

/// A code of `lines` lines, with a box at step 2 when `two_steps` (then
/// presentation mode shows the step counter), padding `pad` rows at the
/// top and the bottom, in `mode`, after one frame of `COLS`×`rows`.
fn opened(name: &str, lines: u32, two_steps: bool, pad: u16, mode: Mode, rows: u16) -> Live {
    let code: String = (1..=lines)
        .map(|i| format!("row {i} of the code\n"))
        .collect();
    let live = Live::from_code(&code, "txt", name);
    live.command("autosave off");
    live.command(&format!("pad {pad} 1"));
    if two_steps {
        live.api_ok(
            "annotation.add",
            serde_json::json!({"type": "box", "from_line": 1, "to_line": 2, "step": 2}),
        );
    }
    match mode {
        Mode::Normal => {}
        Mode::PresentPointer => live.with(|a| {
            a.set_present(true);
            a.toggle_pointer();
        }),
        Mode::PresentView => live.with(|a| a.set_present(true)),
    }
    live.buffer(COLS, rows);
    live
}

/// What `J` moves in `mode`: the cursor line, or the first row in view.
fn place(live: &Live, mode: Mode) -> i64 {
    live.with(move |a| match mode {
        Mode::Normal | Mode::PresentPointer => i64::from(a.cursor_line),
        Mode::PresentView => a.scroll as i64,
    })
}

/// Presses `key` and draws a frame of `COLS`×`rows`: the new place.
fn press(live: &Live, mode: Mode, key: char, rows: u16) -> i64 {
    live.keys(&[KeyCode::Char(key)]);
    live.buffer(COLS, rows);
    place(live, mode)
}

#[test]
fn shift_j_and_shift_k_move_half_the_rows_of_the_code_in_every_mode() {
    use Mode::*;
    // (mode, the counter shows, padding, the jump on a terminal of 24 rows).
    // Normal mode: 24 rows less the bar and the padding. Presentation: the
    // counter takes the last row only when no padding is at the bottom.
    let cases = [
        (Normal, false, 0, 11),
        (Normal, false, 1, 10),
        (Normal, false, 2, 9),
        (Normal, true, 2, 9),
        (PresentPointer, true, 0, 11),
        (PresentPointer, true, 1, 11),
        (PresentPointer, true, 2, 10),
        (PresentPointer, false, 0, 12),
        (PresentPointer, false, 2, 10),
        (PresentView, true, 0, 11),
        (PresentView, true, 1, 11),
        (PresentView, true, 2, 10),
        (PresentView, false, 0, 12),
        (PresentView, false, 2, 10),
    ];
    let mut wrong = Vec::new();
    for (i, (mode, counter, pad, jump)) in cases.into_iter().enumerate() {
        let live = opened(&format!("half-{i}"), 200, counter, pad, mode, 24);
        let start = place(&live, mode);
        let down = press(&live, mode, 'J', 24) - start;
        let back = press(&live, mode, 'K', 24) - start;
        if (down, back) != (jump, 0) {
            wrong.push(format!(
                "{mode:?}, counter {counter}, pad {pad}: J moved {down} (want {jump}), K came back to {back} (want 0)"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn shift_j_moves_one_row_on_a_screen_of_one_row_of_code() {
    // Normal mode on 2 rows: one row of code above the bar. Presentation
    // without a counter on 1 row: one row of code. Half of 1 is 0.
    for (mode, rows) in [(Mode::Normal, 2), (Mode::PresentView, 1)] {
        let live = opened(&format!("one-row-{rows}"), 50, false, 0, mode, rows);
        let start = place(&live, mode);
        assert_eq!(
            press(&live, mode, 'J', rows) - start,
            1,
            "{mode:?}, {rows} rows"
        );
        assert_eq!(
            press(&live, mode, 'K', rows) - start,
            0,
            "{mode:?}, {rows} rows"
        );
    }
}

#[test]
fn shift_j_on_a_short_code_stops_on_the_last_line() {
    for mode in [Mode::Normal, Mode::PresentPointer] {
        let live = opened(&format!("short-{mode:?}"), 5, false, 1, mode, 24);
        assert_eq!(place(&live, mode), 1);
        assert_eq!(press(&live, mode, 'J', 24), 5, "{mode:?}");
        assert_eq!(press(&live, mode, 'K', 24), 1, "{mode:?}");
    }
}
