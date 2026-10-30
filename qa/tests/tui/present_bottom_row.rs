//! Presentation mode at a small height: the step counter («5/5») covered
//! the end of the last row of code, and the review chip («⏸ N to review ·
//! :review») its start.
//!
//! Presentation mode has no status bar, so the code had every row, and the
//! counter and the chip were drawn over the last one. `render` and the
//! exports have no counter, so on that row the screen and the take were not
//! the same.
//!
//! Acceptance criteria:
//! - AC1 While the counter or the chip is drawn, the last row of the
//!   terminal is theirs, as the status bar is in normal mode: the code has
//!   one row less, and none of it is under them.
//! - AC2 The view (`j`, `G`, `J`) counts the same rows as the screen: at
//!   the end of the code, its last line is on the row above the counter.
//! - AC3 With neither of them (a document of one step, at step 1), the code
//!   has every row, as before.
//! - AC4 A padding at the bottom keeps the last row free of code already:
//!   the counter goes there, and the code keeps the rows it had.
//! - AC5 When a change of an agent comes in for review, or goes, the top
//!   line of the view stays: the last line of code goes under the chip and
//!   comes back.
//!
//! Corner cases: a line that fits only with the cells of the counter; step
//! 0 of a document of one step (the counter `0/1` takes the row, and at
//! step 1 the row is code again); the chip with no counter.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::{Value, json};
use tiza_qa::{Live, scratch};
use tiza_tui::app::App;

/// A low panel: 10 rows of code and the row of the counter.
const COLS: u16 = 50;
const ROWS: u16 = 11;
/// Lines of code in the documents: more than the rows of the panel.
const LINES: u32 = 20;

/// Line `n` of the code: 43 cells, the most that the take at 50 columns
/// shows with the gutter of 20 lines, so its end is in the cells where the
/// counter is drawn.
fn line(n: u32) -> String {
    let head = format!("let v{n:02} = ");
    format!("{head}{};", "x".repeat(43 - head.len() - 1))
}

/// The document of [`LINES`] lines with one highlight at each step from 1
/// to `steps` (a highlight adds no row), at `step`, in presentation mode
/// with no padding, after a first frame.
fn open(name: &str, steps: u32, step: u32) -> Live {
    open_padded(name, steps, step, "0 0")
}

/// [`open`] with the padding `pad` (`V H`).
fn open_padded(name: &str, steps: u32, step: u32, pad: &str) -> Live {
    let annotations: Vec<Value> = (1..=steps)
        .map(|s| {
            json!({"id": format!("h{s}"), "step": s, "type": "highlight",
                "span": {"line": s, "start": 4, "end": 7}})
        })
        .collect();
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1},
        "code": (1..=LINES).map(line).collect::<Vec<_>>(),
        "annotations": annotations, "step": step
    });
    let dir = scratch(name);
    std::fs::write(dir.join("doc.tiza"), doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&dir.join("doc.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.command(&format!("pad {pad}"));
    live.with(|a| {
        a.autosave = false;
        a.set_present(true);
        rows(a);
    });
    live
}

/// The rows of a new frame of [`COLS`]×[`ROWS`], trailing blanks trimmed.
fn rows(a: &mut App) -> Vec<String> {
    let mut t = Terminal::new(TestBackend::new(COLS, ROWS)).unwrap();
    t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
    let buf = t.backend().buffer().clone();
    (0..ROWS)
        .map(|y| {
            (0..COLS)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Presses `keys` with no frame between them, then draws one.
fn after_keys(live: &Live, keys: &[KeyCode]) -> Vec<String> {
    let keys = keys.to_vec();
    live.with(move |a| {
        for k in keys {
            tiza_tui::on_key(a, k, KeyModifiers::NONE);
        }
        rows(a)
    })
}

/// The row of line `n` in the take of `render.text` at [`COLS`]: what the
/// screen must show for it.
fn rendered(live: &Live, n: u32) -> String {
    let text = live.api_ok("render.text", json!({"width": COLS}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    let row = text
        .lines()
        .find(|r| r.contains(&format!("let v{n:02} ")))
        .unwrap_or_else(|| panic!("line {n} is not in the render:\n{text}"))
        .trim_end()
        .to_string();
    // The whole line, and its end is in the cells of the counter (` 5/5 `).
    assert!(!row.contains('…'), "{row}");
    assert!(row.chars().count() > usize::from(COLS) - 5, "{row}");
    row
}

/// AC1, AC2: at step 5 of 5, the view at the end of the code (`G`, then
/// `j` past it): the last row is the counter alone, and the last line of
/// code is whole on the row above it, as `render` gives it.
#[test]
fn the_counter_has_a_row_of_its_own_and_covers_no_code() {
    let live = open("counter-row", 5, 5);
    let last = rendered(&live, LINES);
    for keys in [&[KeyCode::Char('G')][..], &[KeyCode::Char('j')]] {
        let screen = after_keys(&live, keys);
        let all = screen.join("\n");
        assert_eq!(
            screen[usize::from(ROWS) - 1].trim(),
            "5/5",
            "{keys:?}:\n{all}"
        );
        assert_eq!(screen[usize::from(ROWS) - 2], last, "{keys:?}:\n{all}");
    }
    assert_eq!(live.with(|a| a.screen_height()), usize::from(ROWS) - 1);
}

/// AC3 and the step 0 of a document of one step: at step 0 the counter
/// `0/1` takes the last row; at step 1 there is no counter, and the last
/// line of code is on the last row.
#[test]
fn with_the_counter_off_the_code_has_the_last_row() {
    let live = open("counter-off", 1, 0);
    let last = rendered(&live, LINES);
    let screen = after_keys(&live, &[KeyCode::Char('G')]);
    let all = screen.join("\n");
    assert_eq!(
        screen[usize::from(ROWS) - 1].trim(),
        "0/1",
        "step 0:\n{all}"
    );
    assert_eq!(screen[usize::from(ROWS) - 2], last, "step 0:\n{all}");
    live.with(|a| a.set_step(1));
    let screen = after_keys(&live, &[KeyCode::Char('G')]);
    let all = screen.join("\n");
    assert_eq!(screen[usize::from(ROWS) - 1], last, "step 1:\n{all}");
    assert_eq!(live.with(|a| a.screen_height()), usize::from(ROWS));
}

/// Supervised mode on, and a change of an agent parked for review: on the
/// TUI thread at once, with its reply waiting on the receiver.
fn propose(live: &Live) -> std::sync::mpsc::Receiver<tiza_core::protocol::Response> {
    live.command("supervise on");
    let (tx, reply) = std::sync::mpsc::channel();
    live.with(move |a| {
        a.handle_ipc(
            tiza_core::protocol::Request {
                id: json!(1),
                method: "annotation.add".into(),
                params: json!({"step": 1, "type": "note", "line": 2, "text": "while presenting"}),
            },
            tx,
        )
    });
    assert_eq!(live.with(|a| a.proposals.len()), 1);
    reply
}

const CHIP: &str = "⏸ 1 to review · :review";

/// AC1 for the chip, with a change of an agent waiting for review: in a
/// document of one step at step 1 (no counter), the chip is alone on the
/// last row; at step 5 of 5, the counter is on the same row, at the right.
/// The last line of code is whole on the row above it.
#[test]
fn the_review_chip_has_a_row_of_its_own() {
    for (steps, counter) in [(1, None), (5, Some("5/5"))] {
        let live = open(&format!("chip-row-{steps}"), steps, steps);
        let last = rendered(&live, LINES);
        let _reply = propose(&live);
        let screen = after_keys(&live, &[KeyCode::Char('G')]);
        let all = screen.join("\n");
        let bottom = screen[usize::from(ROWS) - 1].trim();
        match counter {
            None => assert_eq!(bottom, CHIP, "{all}"),
            Some(c) => assert!(bottom.starts_with(CHIP) && bottom.ends_with(c), "{all}"),
        }
        assert_eq!(screen[usize::from(ROWS) - 2], last, "{all}");
    }
}

/// AC5: a document of one step at step 1, the view at the end. A change
/// that comes in for review takes the last row for its chip, and the view
/// keeps its top line: line 20 goes under the chip, line 19 is on the row
/// above it. When supervised mode goes off, the change is rejected, and
/// line 20 is on the last row again.
#[test]
fn a_change_for_review_that_comes_and_goes_keeps_the_top_of_the_view() {
    let live = open("chip-comes", 1, 1);
    let (last, before_last) = (rendered(&live, LINES), rendered(&live, LINES - 1));
    let before = after_keys(&live, &[KeyCode::Char('G')]);
    assert_eq!(before[usize::from(ROWS) - 1], last, "{}", before.join("\n"));
    let reply = propose(&live);
    let with_chip = after_keys(&live, &[]);
    let all = with_chip.join("\n");
    assert_eq!(with_chip[0], before[0], "the top line:\n{all}");
    assert_eq!(with_chip[usize::from(ROWS) - 1].trim(), CHIP, "{all}");
    assert_eq!(with_chip[usize::from(ROWS) - 2], before_last, "{all}");
    live.command("supervise off");
    assert!(reply.try_recv().is_ok_and(|r| r.error.is_some()));
    let after = after_keys(&live, &[]);
    assert_eq!(after, before, "{}", after.join("\n"));
}

/// AC4: with one row of padding at the top and at the bottom, the counter
/// is on the row of the bottom padding, and the code has the 9 rows
/// between the two, as before. On a terminal of 4 rows the padding does
/// not fit (the code keeps 3 rows): it is 0, and the counter has the 4th
/// row, under 3 rows of code.
#[test]
fn a_padding_at_the_bottom_holds_the_counter_and_the_code_keeps_its_rows() {
    let live = open_padded("counter-pad", 5, 5, "1 0");
    let last = rendered(&live, LINES);
    let screen = after_keys(&live, &[KeyCode::Char('G')]);
    let all = screen.join("\n");
    assert_eq!(screen[usize::from(ROWS) - 1].trim(), "5/5", "{all}");
    assert_eq!(screen[usize::from(ROWS) - 2], last, "{all}");
    assert_eq!(live.with(|a| a.screen_height()), usize::from(ROWS) - 2);
    let low = live.with(|a| {
        let mut t = Terminal::new(TestBackend::new(COLS, 4)).unwrap();
        t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
        let buf = t.backend().buffer().clone();
        let row = |y| (0..COLS).map(|x| buf[(x, y)].symbol()).collect::<String>();
        (row(2), row(3).trim().to_string(), a.screen_height())
    });
    // The 3rd row is a whole line of code (the view keeps its top line).
    assert!(
        low.0.contains("let v") && low.0.trim_end().ends_with(';'),
        "{low:?}"
    );
    assert_eq!((low.1.as_str(), low.2), ("5/5", 3), "{low:?}");
}

/// AC2: `J` moves the view half the rows of the code that the screen
/// shows, the row of the counter not counted: (12 − 1) / 2 = 5 on a
/// terminal of 12 rows. Not on 11 rows: 11 / 2 and (11 − 1) / 2 are both 5.
#[test]
fn half_a_screen_is_half_the_rows_above_the_counter() {
    let live = open("counter-half", 5, 5);
    let scroll = live.with(|a| {
        let mut t = Terminal::new(TestBackend::new(COLS, 12)).unwrap();
        t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
        tiza_tui::on_key(a, KeyCode::Char('J'), KeyModifiers::NONE);
        t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
        a.scroll
    });
    assert_eq!(scroll, 5);
}
