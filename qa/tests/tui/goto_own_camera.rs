//! A goto to the step on screen does what coming to that step does: a
//! `cursor` or a `code.link` of the step puts the cursor on its line again,
//! and the view goes with it, as a `scroll` of the step puts the view back
//! on its target. In normal mode and in presentation mode. With the live
//! pointer on, the pointer keeps the cursor on its line, as when the
//! presenter comes to the step.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tiza_qa::{Live, scratch};
use tiza_tui::app::App;

const COLS: u16 = 80;
const ROWS: u16 = 24;

/// `main.rs` with 160 lines `let vN = N;` and `other.rs` with 120 lines
/// `let wN = N;`, and a document:
/// - step 1: a note on line 1;
/// - step 2: a `cursor` move to line 10, and a note on line 12;
/// - step 3: a `code.link` to lines 20-80 of `other.rs`;
/// - step 4: a note on line 30 of `other.rs`.
fn open(name: &str, present: bool) -> Live {
    let dir = scratch(name);
    let lines = |prefix: &str, n: u32| -> String {
        (1..=n)
            .map(|i| format!("let {prefix}{i} = {i};\n"))
            .collect()
    };
    std::fs::write(dir.join("main.rs"), lines("v", 160)).unwrap();
    std::fs::write(dir.join("other.rs"), lines("w", 120)).unwrap();
    let doc = serde_json::json!({
        "schema": 1,
        "source": {"language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 1, "text": "start"},
            {"id": "n2", "step": 2, "type": "note", "line": 12, "text": "here"},
            {"id": "n3", "step": 4, "type": "note", "line": 30, "text": "later", "file": "other.rs"}
        ],
        "moves": [
            {"at_step": 2, "action": "cursor", "line": 10},
            {"at_step": 3, "action": "code.link", "file": "other.rs", "lines": "20-80"}
        ]
    });
    std::fs::write(dir.join("doc.tiza"), doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&dir.join("doc.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.command("pad 0 0");
    live.with(move |a| {
        a.autosave = false;
        a.set_present(present);
        draw(a);
    });
    live
}

fn draw(a: &mut App) -> ratatui::buffer::Buffer {
    let mut t = Terminal::new(TestBackend::new(COLS, ROWS)).unwrap();
    t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
    t.backend().buffer().clone()
}

fn key(live: &Live, k: char) {
    live.with(move |a| {
        tiza_tui::on_key(a, KeyCode::Char(k), KeyModifiers::NONE);
        draw(a);
    });
}

/// The first line of code that a new frame shows (`let v10` or `let w20`
/// is line 10 or line 20).
fn first_line(live: &Live) -> u32 {
    live.with(|a| {
        let b = draw(a);
        (0..ROWS)
            .find_map(|y| {
                let row: String = (0..COLS).map(|x| b[(x, y)].symbol()).collect();
                let at = row.find("let ")? + "let ".len() + 1;
                let digits: String = row[at..].chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            })
            .expect("a line of code on screen")
    })
}

/// What a goto leaves: the cursor, the first line in view and the pointer.
fn state(live: &Live) -> (u32, u32, Option<u32>) {
    let first = first_line(live);
    live.with(move |a| (a.cursor_line, first, a.pointer.map(|p| p.line)))
}

fn goto(live: &Live, step: u32) {
    live.command(&format!("goto {step}"));
    live.with(|a| {
        draw(a);
    });
}

#[test]
fn a_goto_to_the_step_on_screen_puts_the_view_back_on_its_cursor_or_link() {
    // At the step, the presenter moves the view far down (`G`), then
    // goes to the step again. The view comes back to the cursor of the
    // step, or to the first line of the window of its link.
    for present in [false, true] {
        for (step, line) in [(2, 10), (3, 20)] {
            let live = open(&format!("goto-own-{present}-{step}"), present);
            goto(&live, step);
            assert_eq!(
                state(&live).0,
                line,
                "present {present}, step {step}: arrival"
            );
            key(&live, 'G');
            assert!(
                first_line(&live) > line + 30,
                "present {present}, step {step}: the view went down"
            );
            goto(&live, step);
            let (cursor, first, _) = state(&live);
            assert_eq!(
                (cursor, first),
                (line, line),
                "present {present}, step {step}: the view is back"
            );
            // The same with `step.goto` over the socket, and with `:step`,
            // which is a goto in presentation mode.
            key(&live, 'G');
            live.api_ok("step.goto", serde_json::json!({"id": step.to_string()}));
            assert_eq!(
                state(&live).1,
                line,
                "present {present}, step {step}: step.goto"
            );
            if present {
                key(&live, 'G');
                live.command(&format!("step {step}"));
                assert_eq!(state(&live).1, line, "present, step {step}: :step");
            }
        }
    }
}

#[test]
fn a_goto_to_the_step_on_screen_is_the_same_as_coming_to_it() {
    // The invariant: after the view goes far down, a goto to the step on
    // screen leaves the cursor, the view and the pointer where a goto to
    // that step from another step of the same file leaves them (step 1 for
    // step 2, step 4 for step 3). With the live pointer on, the
    // pointer stays and the cursor with it, as when the presenter comes to
    // the step.
    for present in [false, true] {
        for pointer in [false, true] {
            for step in [2, 3] {
                let case = format!("present {present}, pointer {pointer}, step {step}");
                let run = |from: u32, tag: &str| {
                    let live = open(
                        &format!("goto-inv-{tag}-{present}-{pointer}-{step}"),
                        present,
                    );
                    goto(&live, from);
                    key(&live, 'G');
                    if pointer {
                        key(&live, 'f');
                        assert!(live.with(|a| a.pointer.is_some()), "{case}: pointer on");
                    }
                    goto(&live, step);
                    state(&live)
                };
                let other = if step == 2 { 1 } else { 4 };
                let here = run(step, "here");
                let there = run(other, "there");
                assert_eq!(here, there, "{case}: here, then from step {other}");
                if pointer {
                    for (what, s) in [("here", here), ("from another step", there)] {
                        assert!(s.2.is_some(), "{case}, {what}: the pointer stays");
                        assert_eq!(
                            Some(s.0),
                            s.2,
                            "{case}, {what}: the cursor is on the pointer"
                        );
                    }
                }
            }
        }
    }
}
