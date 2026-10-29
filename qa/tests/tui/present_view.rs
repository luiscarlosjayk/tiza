//! The view in presentation mode, as in a web browser: the keys and the
//! wheel move the view, not the hidden cursor, and the view stays on the
//! same code until something asks to move it.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tiza_core::moves::MoveAction;
use tiza_qa::{Live, fixture, scratch};
use tiza_tui::app::App;

const COLS: u16 = 80;
const ROWS: u16 = 24;

/// `tui/present_view/anchor.tiza` and its code file in a scratch folder,
/// in presentation mode, with no padding (every row of the terminal shows
/// code), at step 1. The code is 160 lines `let vN = N;`. Step 1 has a box
/// with a title on lines 60-62 and one on lines 83-85; steps 2 to 4 add
/// annotations on lines 65-70, and step 3 auto-advances after one hour.
/// Also returns the folder.
fn open_anchor(name: &str) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    for file in ["anchor.tiza", "anchor.rs"] {
        std::fs::copy(fixture(&format!("tui/present_view/{file}")), dir.join(file)).unwrap();
    }
    let doc = tiza_core::fs::load_document(&dir.join("anchor.tiza")).unwrap();
    let live = Live::open(doc, dir.join("anchor.tiza"), dir.join("s.sock"));
    live.command("pad 0 0");
    live.with(|a| {
        a.autosave = false;
        a.set_present(true);
        draw(a, COLS, ROWS);
    });
    (live, dir)
}

fn draw(a: &mut App, cols: u16, rows: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(cols, rows)).unwrap();
    t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
    t.backend().buffer().clone()
}

/// The line of code on the first row of a new frame at `size`, or `None`
/// when that row is not a row of code (the title of a box).
fn first_line(live: &Live, size: (u16, u16)) -> Option<u32> {
    live.with(move |a| {
        let b = draw(a, size.0, size.1);
        let row: String = (0..size.0).map(|x| b[(x, 0)].symbol()).collect();
        let at = row.find("let v")? + "let v".len();
        let digits: String = row[at..].chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

fn key(a: &mut App, k: KeyCode) {
    tiza_tui::on_key(a, k, KeyModifiers::NONE);
}

/// An event that must not move the view. Each one runs in one call, with
/// no wait: the effects and the auto-advance go by the times the test
/// gives them, not by the clock, and an effect ends after its frames.
#[derive(Debug, Clone, Copy)]
enum Event {
    Next,
    Back,
    AutoAdvance,
    Column(char),
    Pointer,
    EffectFrames,
    Resize(u16, u16),
    UpdateLabel,
    GotoHere,
}

/// Runs `event`; returns the size of the terminal after it.
fn run(live: &Live, event: Event, size: (u16, u16)) -> (u16, u16) {
    match event {
        // Steps 1 to 4 only: at step 0 the boxes of step 1 go, and a step
        // forward from 0 shows the title of the box over the view. A `n`
        // at the last step of a looping document goes to step 0.
        Event::Next => live.with(|a| {
            if a.doc.step < a.doc.max_step() {
                key(a, KeyCode::Char('n'));
            }
        }),
        Event::Back => live.with(|a| {
            if a.doc.step > 1 {
                key(a, KeyCode::Char('p'));
            }
        }),
        Event::AutoAdvance => live.with(|a| {
            a.finish_effect();
            a.auto_advance_tick(Instant::now() + Duration::from_secs(2 * 3600));
        }),
        Event::Column(c) => live.with(move |a| key(a, KeyCode::Char(c))),
        Event::Pointer => live.with(|a| key(a, KeyCode::Char('f'))),
        Event::EffectFrames => live.with(move |a| {
            for ms in [0, 60, 120, 240] {
                a.hold_effect_at(ms);
                draw(a, size.0, size.1);
            }
            a.finish_effect();
        }),
        Event::Resize(cols, rows) => return (cols, rows),
        Event::UpdateLabel => {
            live.api_ok(
                "annotation.update",
                serde_json::json!({"id": "top", "label": "a box with a longer title"}),
            );
        }
        // At step 1 the box under the view is of that step: a goto there
        // shows it, as the presenter asks.
        Event::GotoHere => {
            let step = live.with(|a| a.doc.step);
            if step >= 2 {
                live.command(&format!("goto {step}"));
            }
        }
    }
    size
}

/// The fixed list: each event once, in an order that reaches every step.
const EVENTS: [Event; 16] = [
    Event::Next,
    Event::Back,
    Event::Next,
    Event::Next,
    Event::EffectFrames,
    Event::AutoAdvance,
    Event::Column('l'),
    Event::Column('w'),
    Event::Column('0'),
    Event::Column('$'),
    Event::Pointer,
    Event::Pointer,
    Event::Resize(COLS, ROWS),
    Event::Resize(100, 30),
    Event::Resize(80, 20),
    Event::GotoHere,
];

/// A small linear congruential generator: the same sequence on every run.
struct Seeded(u64);

impl Seeded {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % n
    }
}

/// Moves the view with the keys until line 60 is on the first row: the
/// title of its box is on the row above, out of view, and the box of line
/// 83 starts on the row after the last one.
fn view_on_line_60(live: &Live) {
    let rows = live.with(|a| a.layout(usize::from(COLS)).row_of_line[&60]);
    live.with(move |a| {
        for _ in 0..rows {
            key(a, KeyCode::Char('j'));
        }
    });
    assert_eq!(first_line(live, (COLS, ROWS)), Some(60));
    let last = live.with(|a| {
        draw(a, COLS, ROWS);
        a.hit.rows.iter().rev().find_map(|r| *r)
    });
    assert_eq!(last, Some(82), "the box of line 83 starts under the view");
}

#[test]
fn with_no_camera_only_the_presenter_and_new_annotations_move_the_view() {
    // The events in a fixed order, then three sequences from fixed seeds.
    let (live, _) = open_anchor("view-invariant");
    view_on_line_60(&live);
    let mut size = (COLS, ROWS);
    for event in EVENTS {
        size = run(&live, event, size);
        assert_eq!(first_line(&live, size), Some(60), "after {event:?}");
    }
    for seed in [1, 7, 42] {
        let (live, _) = open_anchor("view-invariant-seed");
        view_on_line_60(&live);
        let mut size = (COLS, ROWS);
        let mut pick = Seeded(seed);
        for _ in 0..40 {
            let event = EVENTS[pick.below(EVENTS.len())];
            size = run(&live, event, size);
            assert_eq!(
                first_line(&live, size),
                Some(60),
                "seed {seed}, after {event:?}"
            );
        }
    }
    // An agent edits an annotation in view, and the code file changes
    // under the view: the reload keeps the view too.
    let (live, dir) = open_anchor("view-invariant-reload");
    view_on_line_60(&live);
    run(&live, Event::UpdateLabel, (COLS, ROWS));
    assert_eq!(first_line(&live, (COLS, ROWS)), Some(60), "after an update");
    live.command("follow on");
    live.wait_follow_started();
    let file = dir.join("anchor.rs");
    let code = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, code.replace("let v150 = 150;", "let v150 = 1500;")).unwrap();
    // The watcher can read the save first: wait for the new code.
    let end = Instant::now() + Duration::from_secs(10);
    while !live.with(|a| {
        a.read_save();
        a.doc.code.iter().any(|l| l == "let v150 = 1500;")
    }) {
        assert!(Instant::now() < end, "the reload did not happen in 10 s");
    }
    assert_eq!(first_line(&live, (COLS, ROWS)), Some(60), "after a reload");
}

/// `lines` lines `line N` in presentation mode, with the padding `pad`
/// (`V H`), after a first frame.
fn open_lines(name: &str, lines: u32, pad: &str) -> Live {
    let code: String = (1..=lines).map(|i| format!("line {i}\n")).collect();
    let live = Live::from_code(&code, "txt", name);
    live.command(&format!("pad {pad}"));
    live.with(|a| {
        a.autosave = false;
        a.set_present(true);
        draw(a, COLS, ROWS);
    });
    live
}

/// Presses `keys` with no frame between them, then draws one: the first
/// row in view after it.
fn scroll_after(live: &Live, keys: &[KeyCode]) -> usize {
    let keys = keys.to_vec();
    live.with(move |a| {
        for k in keys {
            key(a, k);
        }
        draw(a, COLS, ROWS);
        a.scroll
    })
}

/// A document of 160 lines `let vN = N;` with `moves`, in presentation
/// mode with no padding, at step 1.
fn open_moves(name: &str, moves: serde_json::Value) -> Live {
    let n1 =
        serde_json::json!([{"id": "n1", "step": 1, "type": "note", "line": 5, "text": "start"}]);
    open_json(name, n1, moves, 1)
}

/// [`open_moves`] with `annotations`, at `step`.
fn open_json(
    name: &str,
    annotations: serde_json::Value,
    moves: serde_json::Value,
    step: u32,
) -> Live {
    let doc = serde_json::json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
        "annotations": annotations, "moves": moves, "step": step
    });
    open_doc(name, doc, true)
}

/// `n` lines `let {name}N = N;`.
fn code_of(n: u32, name: &str) -> Vec<String> {
    (1..=n).map(|i| format!("let {name}{i} = {i};")).collect()
}

/// The document `doc` in a scratch folder, with no padding, in
/// presentation mode when `present`.
fn open_doc(name: &str, doc: serde_json::Value, present: bool) -> Live {
    let dir = scratch(name);
    std::fs::write(dir.join("doc.tiza"), doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&dir.join("doc.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.command("pad 0 0");
    live.with(move |a| {
        a.autosave = false;
        a.set_present(present);
        draw(a, COLS, ROWS);
    });
    live
}

/// The row of line `line` in a new frame, if it is in view.
fn row_of(live: &Live, line: u32) -> Option<usize> {
    live.with(move |a| {
        draw(a, COLS, ROWS);
        a.hit.rows.iter().position(|r| *r == Some(line))
    })
}

#[test]
fn the_keys_move_the_view_one_row_and_the_cursor_stays_in_it() {
    let live = open_lines("view-keys", 200, "0 0");
    assert_eq!(scroll_after(&live, &[KeyCode::Char('j')]), 1, "j");
    assert_eq!(scroll_after(&live, &[KeyCode::Down]), 2, "↓");
    assert_eq!(scroll_after(&live, &[KeyCode::Char('k')]), 1, "k");
    assert_eq!(scroll_after(&live, &[KeyCode::Up]), 0, "↑");
    // Far down, the hidden cursor is in the view, and `f` points there.
    assert_eq!(scroll_after(&live, &[KeyCode::Char('j'); 40]), 40);
    let (cursor, pointer) = live.with(|a| {
        key(a, KeyCode::Char('f'));
        (a.cursor_line, a.pointer.map(|p| p.line))
    });
    assert!((41..=64).contains(&cursor), "the cursor on line {cursor}");
    assert_eq!(pointer, Some(cursor));
    // From below, the cursor goes to the last line in view.
    let cursor = live.with(|a| {
        key(a, KeyCode::Char('f'));
        key(a, KeyCode::Char('G'));
        draw(a, COLS, ROWS);
        key(a, KeyCode::Char('g'));
        draw(a, COLS, ROWS);
        a.cursor_line
    });
    assert_eq!(cursor, 24);
}

#[test]
fn half_a_screen_is_half_the_rows_of_the_code() {
    // A padding of one row: 22 rows of code on a terminal of 24.
    let live = open_lines("view-half", 200, "1 1");
    assert_eq!(scroll_after(&live, &[KeyCode::Char('J')]), 11, "J");
    assert_eq!(scroll_after(&live, &[KeyCode::Char('K')]), 0, "K");
}

#[test]
fn g_and_shift_g_go_to_the_ends_and_the_view_stops_there() {
    let live = open_lines("view-ends", 200, "0 0");
    let end = 200 - usize::from(ROWS);
    assert_eq!(scroll_after(&live, &[KeyCode::Char('G')]), end, "G");
    assert_eq!(
        scroll_after(&live, &[KeyCode::Char('j')]),
        end,
        "past the end"
    );
    assert_eq!(scroll_after(&live, &[KeyCode::Char('g')]), 0, "g");
    assert_eq!(
        scroll_after(&live, &[KeyCode::Char('k')]),
        0,
        "before the start"
    );
    assert_eq!(scroll_after(&live, &[KeyCode::End]), end, "End");
    assert_eq!(scroll_after(&live, &[KeyCode::Home]), 0, "Home");
    // Keys with no frame between them add up, from the end they name.
    let keys = [KeyCode::Char('G'), KeyCode::Char('k'), KeyCode::Char('k')];
    assert_eq!(scroll_after(&live, &keys), end - 2, "G k k");
    let keys = [KeyCode::Char('g'), KeyCode::Char('j'), KeyCode::Char('j')];
    assert_eq!(scroll_after(&live, &keys), 2, "g j j");
}

#[test]
fn in_a_short_document_the_keys_move_nothing() {
    let live = open_lines("view-short", 5, "0 0");
    for k in [KeyCode::Char('j'), KeyCode::Char('J'), KeyCode::Char('G')] {
        assert_eq!(scroll_after(&live, &[k]), 0, "{k:?}");
    }
}

#[test]
fn a_key_lets_a_held_view_go_and_starts_from_its_target() {
    // Step 2 scrolls to line 100, at the top.
    let moves =
        serde_json::json!([{"at_step": 2, "action": "scroll", "line": 100, "align": "top"}]);
    let live = open_moves("view-held", moves.clone());
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(first_line(&live, (COLS, ROWS)), Some(100));
    live.with(|a| key(a, KeyCode::Char('j')));
    assert_eq!(
        first_line(&live, (COLS, ROWS)),
        Some(101),
        "the key moves it"
    );
    assert_eq!(
        first_line(&live, (COLS, ROWS)),
        Some(101),
        "the target does not hold it"
    );
    // The step and the key before the next frame: the key starts from the
    // target of the step.
    let live = open_moves("view-held-key", moves);
    live.with(|a| {
        key(a, KeyCode::Char('n'));
        key(a, KeyCode::Char('j'));
    });
    assert_eq!(first_line(&live, (COLS, ROWS)), Some(101));
}

#[test]
fn leaving_presentation_mode_drops_a_key_of_the_view() {
    let live = open_lines("view-leave", 200, "0 0");
    let scroll = live.with(|a| {
        key(a, KeyCode::Char('j'));
        key(a, KeyCode::Char('P'));
        draw(a, COLS, ROWS);
        (a.view_request(), a.scroll)
    });
    assert_eq!(scroll, (None, 0));
}

#[test]
fn in_a_selection_the_keys_move_the_selection_and_a_resize_leaves_it() {
    let live = open_lines("view-select", 200, "0 0");
    let (cursor, scroll) = live.with(|a| {
        for _ in 0..20 {
            key(a, KeyCode::Char('j'));
        }
        draw(a, COLS, ROWS);
        // Line 40 is the last line in view: the cursor goes there, and `V`
        // selects it.
        a.cursor_line = 40;
        draw(a, COLS, ROWS);
        key(a, KeyCode::Char('V'));
        key(a, KeyCode::Char('k'));
        draw(a, COLS, ROWS);
        (a.cursor_line, a.scroll)
    });
    assert_eq!((cursor, scroll), (39, 20), "k moves the selection");
    // A smaller terminal leaves line 39 out of the view: the selection stays.
    let selection = live.with(|a| {
        draw(a, COLS, 10);
        (a.cursor_line, a.selection())
    });
    assert_eq!(selection.0, 39, "{selection:?}");
}

#[test]
fn while_a_step_types_the_keys_of_the_view_let_it_type() {
    // Step 2 types two lines at line 10, in view, one an hour.
    let moves = serde_json::json!([{
        "at_step": 2, "action": "code.patch", "op": "insert", "position": "10",
        "content": "// typed\n// more", "typing": "line", "speed_ms": 3_600_000
    }]);
    let live = open_moves("view-typing", moves.clone());
    live.with(|a| key(a, KeyCode::Char('n')));
    assert!(live.with(|a| a.is_typing()));
    assert_eq!(scroll_after(&live, &[KeyCode::Char('j')]), 1);
    assert!(live.with(|a| a.is_typing()), "j ended the typing");
    live.with(|a| key(a, KeyCode::Char('x')));
    assert!(!live.with(|a| a.is_typing()), "x did not end the typing");
    // With the live pointer on, the key moves the pointer: it ends the
    // typing, as any key does.
    let live = open_moves("view-typing-pointer", moves);
    live.with(|a| {
        key(a, KeyCode::Char('f'));
        key(a, KeyCode::Char('n'));
    });
    assert!(live.with(|a| a.is_typing()));
    live.with(|a| key(a, KeyCode::Char('j')));
    assert!(!live.with(|a| a.is_typing()), "j did not end the typing");
}

/// `tui/present_view/above.tiza` at step 2, in presentation mode with no
/// padding: 160 lines `let vN = N;`. At step 2 a box with a title comes in
/// on lines 30-31 (two rows more), and a patch inserts five lines
/// `// new N` at line 10.
fn open_above(name: &str) -> Live {
    let dir = scratch(name);
    let doc = tiza_core::fs::load_document(&fixture("tui/present_view/above.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.command("pad 0 0");
    live.with(|a| {
        a.autosave = false;
        a.set_present(true);
        draw(a, COLS, ROWS);
    });
    live
}

/// Moves the view with `j` until the line with `text` is on the first row.
fn view_on(live: &Live, text: &'static str) {
    let rows = live.with(move |a| {
        let line = (1..=a.doc.last_line())
            .find(|l| a.doc.line_text(*l) == Some(text))
            .unwrap();
        a.layout(usize::from(COLS)).row_of_line[&line]
    });
    live.with(move |a| {
        for _ in 0..rows {
            key(a, KeyCode::Char('j'));
        }
    });
    assert_eq!(top_text(live), text);
}

/// The code on the first row of a new frame, without the right border of
/// a box around it.
fn top_text(live: &Live) -> String {
    live.with(|a| {
        let b = draw(a, COLS, ROWS);
        let row: String = (0..COLS).map(|x| b[(x, 0)].symbol()).collect();
        let code: String = row.chars().skip(a.hit.code_x as usize).collect();
        code.split('│').next().unwrap_or("").trim_end().to_string()
    })
}

#[test]
fn a_step_back_over_rows_and_lines_above_the_view_keeps_its_code() {
    // At step 2 `let v60` is line 65, under a box and five new lines.
    let live = open_above("view-above");
    view_on(&live, "let v60 = 60;");
    live.with(|a| key(a, KeyCode::Char('p')));
    assert_eq!(live.with(|a| a.doc.step), 1);
    assert_eq!(top_text(&live), "let v60 = 60;");
    // The hidden cursor was on that line, and went with it: `f` points
    // at it.
    let pointed = live.with(|a| {
        key(a, KeyCode::Char('f'));
        a.pointer
            .and_then(|p| a.doc.line_text(p.line))
            .map(str::to_string)
    });
    assert_eq!(pointed.as_deref(), Some("let v60 = 60;"));
}

#[test]
fn a_line_on_top_that_the_step_removes_gives_its_place_to_the_line_above() {
    let live = open_above("view-removed");
    let above = live.with(|a| {
        let first_new = (1..=a.doc.last_line())
            .find(|l| a.doc.line_text(*l) == Some("// new 1"))
            .unwrap();
        a.doc.line_text(first_new - 1).unwrap().to_string()
    });
    view_on(&live, "// new 3");
    live.with(|a| key(a, KeyCode::Char('p')));
    assert_eq!(top_text(&live), above);
}

#[test]
fn an_agent_annotation_above_the_view_keeps_its_code() {
    let (live, _) = open_anchor("view-agent-above");
    view_on_line_60(&live);
    live.api_ok(
        "annotation.add",
        serde_json::json!({"type": "box", "from_line": 20, "to_line": 21, "step": 1, "label": "an agent"}),
    );
    assert_eq!(first_line(&live, (COLS, ROWS)), Some(60));
}

#[test]
fn the_wheel_moves_the_view_three_rows_and_a_click_does_nothing() {
    use tiza_tui::app::{Mode, MouseKind, MouseOutcome};
    let live = open_lines("view-wheel", 200, "0 0");
    let wheel = |kind: MouseKind| {
        live.with(move |a| {
            a.mouse = true;
            let out = a.on_mouse(kind, 10, 5);
            draw(a, COLS, ROWS);
            (out, a.scroll)
        })
    };
    assert_eq!(wheel(MouseKind::ScrollDown), (MouseOutcome::Scrolled, 3));
    assert_eq!(wheel(MouseKind::ScrollDown), (MouseOutcome::Scrolled, 6));
    assert_eq!(wheel(MouseKind::ScrollUp), (MouseOutcome::Scrolled, 3));
    // A click or a drag does nothing.
    let cursor = live.with(|a| (a.cursor_line, a.cursor_col));
    for kind in [MouseKind::Down, MouseKind::Drag, MouseKind::Up] {
        assert_eq!(wheel(kind), (MouseOutcome::Ignored, 3), "{kind:?}");
    }
    assert_eq!(live.with(|a| (a.cursor_line, a.cursor_col)), cursor);
    // With the command line or a selection open, the wheel does nothing.
    live.with(|a| key(a, KeyCode::Char(':')));
    assert_eq!(wheel(MouseKind::ScrollDown), (MouseOutcome::Ignored, 3));
    live.with(|a| {
        key(a, KeyCode::Esc);
        key(a, KeyCode::Char('V'));
    });
    assert!(matches!(live.with(|a| a.mode.clone()), Mode::Select { .. }));
    assert_eq!(wheel(MouseKind::ScrollDown), (MouseOutcome::Ignored, 3));
    // With the live pointer on, the wheel moves the cursor and the pointer
    // three lines, as ↓ does.
    let line = live.with(|a| {
        key(a, KeyCode::Esc);
        key(a, KeyCode::Char('f'));
        a.cursor_line
    });
    assert_eq!(wheel(MouseKind::ScrollDown).0, MouseOutcome::Scrolled);
    let pointer = live.with(|a| (a.cursor_line, a.pointer.map(|p| p.line)));
    assert_eq!(pointer, (line + 3, Some(line + 3)));
}

#[test]
fn the_wheel_lets_an_effect_play_and_a_step_type() {
    use tiza_tui::app::{MouseKind, MouseOutcome};
    // Step 3 of anchor.tiza has a box with `draw`.
    let (live, _) = open_anchor("view-wheel-effect");
    let playing = live.with(|a| {
        key(a, KeyCode::Char('n'));
        key(a, KeyCode::Char('n'));
        a.mouse = true;
        let out = a.on_mouse(MouseKind::ScrollDown, 10, 5);
        (out, a.is_playing_effect())
    });
    assert_eq!(playing, (MouseOutcome::Scrolled, true));
    let moves = serde_json::json!([{
        "at_step": 2, "action": "code.patch", "op": "insert", "position": "10",
        "content": "// typed\n// more", "typing": "line", "speed_ms": 3_600_000
    }]);
    let live = open_moves("view-wheel-typing", moves);
    let typing = live.with(|a| {
        key(a, KeyCode::Char('n'));
        a.mouse = true;
        a.on_mouse(MouseKind::ScrollDown, 10, 5);
        draw(a, COLS, ROWS);
        (a.is_typing(), a.scroll)
    });
    assert_eq!(typing, (true, 3));
    // With the live pointer on, the wheel moves the pointer: it ends the
    // typing, as ↓ does.
    let typing = live.with(|a| {
        // The first key ends the typing, the second goes back a step.
        key(a, KeyCode::Char('p'));
        key(a, KeyCode::Char('p'));
        key(a, KeyCode::Char('f'));
        key(a, KeyCode::Char('n'));
        let before = a.is_typing();
        a.on_mouse(MouseKind::ScrollDown, 10, 5);
        (before, a.is_typing())
    });
    assert_eq!(typing, (true, false));
}

/// The real TUI with the mouse on, in presentation mode: a notch of the
/// wheel (SGR 65) moves the view three rows, and the hidden cursor, on
/// line 1, goes into the view. `f` then points at line 4.
#[test]
fn the_wheel_moves_the_view_in_the_real_tui() {
    let dir = scratch("view-wheel-tui");
    let code: String = (1..=100).map(|i| format!("line {i}\n")).collect();
    std::fs::write(dir.join("lines.txt"), code).unwrap();
    let body = r##"
import json, subprocess
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--no-save", "--present", "--mouse", "--socket", "s.sock", "lines.txt"], 30, 80, env=env)

def pointer():
    r = subprocess.run([binary, "--socket", "s.sock", "api", "status"], capture_output=True, text=True, timeout=10)
    return (json.loads(r.stdout).get("pointer") or {}).get("line") if r.returncode == 0 else None

if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    s.send(b"\x1b[<65;20;5M")
    s.send(b"f")
    end = time.monotonic() + 10
    line = None
    while time.monotonic() < end and line is None:
        line = pointer()
    print("POINTER", line, flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "wheel.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    assert!(out.contains("POINTER 4"), "{out}");
}

#[test]
fn with_the_pointer_on_the_view_keeps_the_pointer_in_it() {
    let live = open_lines("view-pointer", 200, "0 0");
    // The pointer on line 24, the last row: rows that come in above it
    // push it down, and the view goes with it.
    let line = live.with(|a| {
        key(a, KeyCode::Char('f'));
        for _ in 0..23 {
            key(a, KeyCode::Char('j'));
            draw(a, COLS, ROWS);
        }
        a.cursor_line
    });
    assert_eq!(line, 24);
    live.api_ok(
        "annotation.add",
        serde_json::json!({"type": "box", "from_line": 5, "to_line": 6, "label": "a title"}),
    );
    let shown = |live: &Live, rows: u16| {
        live.with(move |a| {
            draw(a, COLS, rows);
            let visible = a.hit.rows.iter().flatten().copied().collect::<Vec<u32>>();
            (
                a.cursor_line,
                a.pointer.map(|p| p.line),
                visible.contains(&a.cursor_line),
            )
        })
    };
    assert_eq!(shown(&live, ROWS), (24, Some(24), true), "rows above");
    // A smaller terminal: the view shows the pointer, and the cursor stays
    // on it.
    assert_eq!(shown(&live, 10), (24, Some(24), true), "a smaller terminal");
    live.with(|a| key(a, KeyCode::Char('j')));
    assert_eq!(shown(&live, 10), (25, Some(25), true), "j after it");
}

#[test]
fn a_camera_of_the_step_wins_also_on_the_line_number_of_the_cursor() {
    // Step 2 inserts five lines at line 10, then puts the view or the
    // cursor on line 100 of its code. The hidden cursor was on line 100.
    let insert = serde_json::json!({
        "at_step": 2, "action": "code.patch", "op": "insert", "position": "10",
        "content": "// 1\n// 2\n// 3\n// 4\n// 5"
    });
    let camera = [
        serde_json::json!({"at_step": 2, "action": "scroll", "line": 100, "align": "top"}),
        serde_json::json!({"at_step": 2, "action": "cursor", "line": 100}),
    ];
    for (i, cam) in camera.into_iter().enumerate() {
        let live = open_moves(
            &format!("view-camera-{i}"),
            serde_json::json!([insert, cam]),
        );
        view_on(&live, "let v100 = 100;");
        assert_eq!(live.with(|a| a.cursor_line), 100);
        let (target, pointed) = live.with(|a| {
            key(a, KeyCode::Char('n'));
            draw(a, COLS, ROWS);
            key(a, KeyCode::Char('f'));
            let target = a.doc.line_text(100).unwrap().to_string();
            (target, a.pointer.map(|p| p.line))
        });
        assert_eq!(target, "let v95 = 95;");
        assert_eq!(pointed, Some(100), "{cam}: the cursor of the step");
        if i == 0 {
            assert_eq!(top_text(&live), target, "the view of the step");
        }
    }
}

#[test]
fn with_or_without_the_pointer_a_show_starts_from_the_same_view() {
    // Step 2 adds a box with a title above the view (two rows) and shows
    // line 70, which is in view: the view does not move. A note adds no
    // row: it is on the row of its line.
    let notes = serde_json::json!([
        {"id": "n1", "step": 1, "type": "note", "line": 5, "text": "start"},
        {"id": "b2", "step": 2, "type": "box", "from_line": 30, "to_line": 31, "label": "above"}
    ]);
    let show = serde_json::json!([{"at_step": 2, "action": "scroll", "line": 70, "align": "show"}]);
    for pointer in [false, true] {
        let live = open_json(
            &format!("view-show-{pointer}"),
            notes.clone(),
            show.clone(),
            1,
        );
        view_on(&live, "let v60 = 60;");
        live.with(move |a| {
            if pointer {
                key(a, KeyCode::Char('f'));
            }
            key(a, KeyCode::Char('n'));
        });
        assert_eq!(top_text(&live), "let v60 = 60;", "pointer {pointer}");
    }
}

#[test]
fn a_step_carries_the_view_over_a_delete_a_replace_and_two_patches() {
    let at = |op: &str, position: &str, content: &str| {
        serde_json::json!({
            "at_step": 2, "action": "code.patch", "op": op, "position": position,
            "content": content
        })
    };
    let n1 =
        serde_json::json!([{"id": "n1", "step": 1, "type": "note", "line": 5, "text": "start"}]);
    // Forward over a line that a delete removes above the view.
    let delete = serde_json::json!([{"at_step": 2, "action": "code.patch", "op": "delete", "position": "10"}]);
    let live = open_json("view-delete", n1.clone(), delete, 1);
    view_on(&live, "let v60 = 60;");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v60 = 60;", "a delete");
    // Back over a replace of one line by three, and over two inserts of
    // one step.
    let cases = [
        (
            "replace",
            serde_json::json!([at("replace", "10", "// a\n// b\n// c")]),
        ),
        (
            "two patches",
            serde_json::json!([at("insert", "10", "// a\n// b"), at("insert", "20", "// c")]),
        ),
    ];
    for (what, moves) in cases {
        let live = open_json(
            &format!("view-{}", what.replace(' ', "-")),
            n1.clone(),
            moves,
            2,
        );
        view_on(&live, "let v60 = 60;");
        live.with(|a| key(a, KeyCode::Char('p')));
        assert_eq!(top_text(&live), "let v60 = 60;", "{what}");
    }
}

/// Notes `n1` (step 1, line 5) and those of `more`, with `moves`, at step 1.
fn open_steps(name: &str, more: serde_json::Value, moves: serde_json::Value) -> Live {
    let mut notes = vec![
        serde_json::json!({"id": "n1", "step": 1, "type": "note", "line": 5, "text": "start"}),
    ];
    notes.extend(more.as_array().unwrap().iter().cloned());
    open_json(name, serde_json::Value::Array(notes), moves, 1)
}

#[test]
fn a_step_shows_what_comes_in_with_the_least_move() {
    // Step 2 brings a box on lines 100-102, far under the view: the view
    // goes down just enough to show it whole, its border on the last row.
    // Step 3 brings a note that is in view: the view does not move.
    let live = open_steps(
        "reveal-least",
        serde_json::json!([
            {"id": "b2", "step": 2, "type": "box", "from_line": 100, "to_line": 102, "label": "the box"},
            {"id": "n3", "step": 3, "type": "note", "line": 95, "text": "in view"}
        ]),
        serde_json::json!([]),
    );
    assert_eq!(top_text(&live), "let v1 = 1;");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v81 = 81;", "step 2: the least move");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v81 = 81;", "step 3: in view already");
}

#[test]
fn the_scroll_of_the_step_wins_and_one_that_does_not_resolve_lets_it_reveal() {
    let live = open_steps(
        "reveal-scroll",
        serde_json::json!([
            {"id": "n2", "step": 2, "type": "note", "line": 120, "text": "under"},
            {"id": "n3", "step": 3, "type": "note", "line": 140, "text": "under too"}
        ]),
        serde_json::json!([
            {"at_step": 2, "action": "scroll", "line": 50, "align": "top"},
            {"at_step": 3, "action": "scroll", "line": 999}
        ]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v50 = 50;", "step 2: its scroll");
    live.with(|a| key(a, KeyCode::Char('n')));
    let shown = live.with(|a| {
        draw(a, COLS, ROWS);
        a.hit.rows.contains(&Some(140))
    });
    assert!(
        shown,
        "step 3: its scroll does not resolve, the note comes in"
    );
}

#[test]
fn a_step_back_reveals_nothing_and_a_goto_of_some_steps_does() {
    let live = open_steps(
        "reveal-back-goto",
        serde_json::json!([
            {"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"},
            {"id": "n3", "step": 3, "type": "note", "line": 140, "text": "three"}
        ]),
        serde_json::json!([]),
    );
    live.command("goto 3");
    assert!(
        live.with(|a| {
            draw(a, COLS, ROWS);
            a.hit.rows.contains(&Some(140))
        }),
        "goto 3"
    );
    // Back to the top with the keys, then one step back: nothing moves.
    live.with(|a| key(a, KeyCode::Char('g')));
    assert_eq!(top_text(&live), "let v1 = 1;");
    live.with(|a| key(a, KeyCode::Char('p')));
    assert_eq!(top_text(&live), "let v1 = 1;", "p");
}

#[test]
fn in_normal_mode_nothing_comes_in_by_itself() {
    let live = open_steps(
        "reveal-normal",
        serde_json::json!([{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}]),
        serde_json::json!([]),
    );
    live.with(|a| a.set_present(false));
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v1 = 1;");
}

#[test]
fn what_does_not_fit_shows_its_start() {
    // A box on lines 60-110, taller than the view: its title on the first
    // row, line 60 under it.
    let live = open_steps(
        "reveal-tall",
        serde_json::json!([{"id": "b2", "step": 2, "type": "box", "from_line": 60, "to_line": 110, "label": "tall"}]),
        serde_json::json!([]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(first_line(&live, (COLS, ROWS)), None, "the title on top");
    let row = live.with(|a| {
        draw(a, COLS, ROWS);
        a.hit.rows.iter().position(|r| *r == Some(60))
    });
    assert_eq!(row, Some(1));
}

#[test]
fn a_step_that_types_shows_its_lines_and_they_keep_their_rows() {
    // Step 2 types three lines at line 120, one an hour, and a box comes in
    // around them when the typing ends: they come into view at the start,
    // and keep their rows when the box comes in.
    let live = open_steps(
        "reveal-typed",
        serde_json::json!([{"id": "b2", "step": 2, "type": "box", "from_line": 120, "to_line": 122, "label": "typed"}]),
        serde_json::json!([{
            "at_step": 2, "action": "code.patch", "op": "insert", "position": "120",
            "content": "// typed 1\n// typed 2\n// typed 3", "typing": "line", "speed_ms": 3_600_000
        }]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    assert!(live.with(|a| a.is_typing()));
    let rows = |live: &Live| {
        live.with(|a| {
            draw(a, COLS, ROWS);
            [119, 122].map(|l| a.hit.rows.iter().position(|r| *r == Some(l)))
        })
    };
    let typing = rows(&live);
    assert!(typing.iter().all(Option::is_some), "in view: {typing:?}");
    live.with(|a| key(a, KeyCode::Char('x')));
    assert!(!live.with(|a| a.is_typing()));
    let done = rows(&live);
    assert_eq!(done[1], typing[1], "the last typed line keeps its row");
    assert_eq!(
        done[0],
        typing[0].map(|r| r - 1),
        "the line above goes up for the title"
    );
}

#[test]
fn the_pointer_stays_only_where_the_new_view_shows_it() {
    let live = open_steps(
        "reveal-pointer",
        serde_json::json!([{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}]),
        serde_json::json!([]),
    );
    live.api_ok("pointer.set", serde_json::json!({ "line": 5 }));
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(live.with(|a| a.pointer), None, "line 5 is out of view");
}

#[test]
fn back_at_the_top_the_pointer_stays_only_where_the_top_shows_it() {
    // A `scroll` to line 50 at the top at step 2; the pointer on line 60.
    // Back at step 1, the view goes to the top, where line 60 is not: the
    // pointer goes off and does not pull the view down.
    let live = open_steps(
        "reveal-top-pointer",
        serde_json::json!([]),
        serde_json::json!([{"at_step": 2, "action": "scroll", "line": 50, "align": "top"}]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v50 = 50;");
    live.api_ok("pointer.set", serde_json::json!({ "line": 60 }));
    live.with(|a| key(a, KeyCode::Char('p')));
    assert_eq!(live.with(|a| a.pointer), None);
    assert_eq!(top_text(&live), "let v1 = 1;");
}

#[test]
fn an_older_cursor_does_not_pull_back_a_view_the_presenter_moved() {
    // Step 2 puts the cursor on line 10; step 3 brings a note in view. The
    // presenter moves the view down at step 2: at step 3 the `cursor` of
    // step 2 does not take the view back (N (a)).
    let live = open_steps(
        "reveal-older-cursor",
        serde_json::json!([{"id": "n3", "step": 3, "type": "note", "line": 70, "text": "three"}]),
        serde_json::json!([{"at_step": 2, "action": "cursor", "line": 10}]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    live.with(|a| {
        for _ in 0..60 {
            key(a, KeyCode::Char('j'));
        }
    });
    assert_eq!(top_text(&live), "let v61 = 61;");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v61 = 61;", "step 3");
}

#[test]
fn the_lines_that_a_step_writes_come_in_with_no_annotation() {
    // Step 2 only inserts two lines at line 140, with no typing (D (a)).
    let live = open_steps(
        "reveal-written",
        serde_json::json!([]),
        serde_json::json!([{
            "at_step": 2, "action": "code.patch", "op": "insert", "position": "140",
            "content": "// new 1\n// new 2"
        }]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(live.with(|a| a.doc.step), 2);
    let shown = live.with(|a| {
        draw(a, COLS, ROWS);
        a.hit.rows.contains(&Some(140)) && a.hit.rows.contains(&Some(141))
    });
    assert!(shown, "the new lines in view");
}

#[test]
fn an_agent_edit_keeps_a_reveal_over_the_view_of_an_older_scroll() {
    // Step 2 scrolls to line 50 at the top; the presenter moves the view;
    // step 3 brings a note on line 120. Its reveal holds the view: an agent
    // that edits the note does not bring back the view of the `scroll`.
    let live = open_steps(
        "reveal-agent",
        serde_json::json!([{"id": "n3", "step": 3, "type": "note", "line": 120, "text": "three"}]),
        serde_json::json!([{"at_step": 2, "action": "scroll", "line": 50, "align": "top"}]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v50 = 50;");
    live.with(|a| key(a, KeyCode::Char('j')));
    assert_eq!(top_text(&live), "let v51 = 51;");
    live.with(|a| key(a, KeyCode::Char('n')));
    let before = top_text(&live);
    assert_ne!(before, "let v51 = 51;", "the note came in");
    live.api_ok(
        "annotation.update",
        serde_json::json!({"id": "n3", "label": "edited"}),
    );
    assert_eq!(top_text(&live), before, "after the agent");
}

#[test]
fn a_goto_to_the_step_on_screen_shows_what_came_in_again() {
    let live = open_steps(
        "reveal-goto-here",
        serde_json::json!([{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}]),
        serde_json::json!([]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    live.with(|a| key(a, KeyCode::Char('g')));
    assert_eq!(top_text(&live), "let v1 = 1;");
    live.command("goto 2");
    assert!(live.with(|a| {
        draw(a, COLS, ROWS);
        a.hit.rows.contains(&Some(100))
    }));
}

#[test]
fn an_older_cursor_does_not_take_the_view_at_a_step_with_nothing_new() {
    // Step 2 puts the cursor on line 10; step 3 only waits. The presenter
    // moves the view down at step 2: at step 3 the view stays (N (a)).
    let live = open_steps(
        "reveal-older-cursor-wait",
        serde_json::json!([]),
        serde_json::json!([
            {"at_step": 2, "action": "cursor", "line": 10},
            {"at_step": 3, "action": "wait", "ms": 3_600_000}
        ]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    live.with(|a| {
        for _ in 0..60 {
            key(a, KeyCode::Char('j'));
        }
    });
    assert_eq!(top_text(&live), "let v61 = 61;");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(live.with(|a| a.doc.step), 3);
    assert_eq!(top_text(&live), "let v61 = 61;");
}

#[test]
fn rows_that_an_agent_adds_above_what_came_in_do_not_move_it() {
    // Step 2 brings a note on line 120: the view holds it where it came
    // in. An agent adds a box with a title on lines 115-116, above it and
    // in view: the line of the note keeps its row.
    let live = open_steps(
        "reveal-agent-rows",
        serde_json::json!([{"id": "n2", "step": 2, "type": "note", "line": 120, "text": "two"}]),
        serde_json::json!([]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    let row_120 = |live: &Live| {
        live.with(|a| {
            draw(a, COLS, ROWS);
            a.hit.rows.iter().position(|r| *r == Some(120))
        })
    };
    let before = row_120(&live);
    assert!(before.is_some());
    live.api_ok(
        "annotation.add",
        serde_json::json!({"type": "box", "from_line": 115, "to_line": 116, "step": 2, "label": "agent"}),
    );
    assert_eq!(row_120(&live), before);
}

#[test]
fn what_does_not_fit_under_an_older_scroll_shows_its_start() {
    // Step 2 holds line 50 at the top; step 3 brings a box on lines
    // 100-150, taller than the view: its title on the first row.
    let live = open_steps(
        "reveal-tall-camera",
        serde_json::json!([{"id": "b3", "step": 3, "type": "box", "from_line": 100, "to_line": 150, "label": "tall"}]),
        serde_json::json!([{"at_step": 2, "action": "scroll", "line": 50, "align": "top"}]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v50 = 50;");
    live.with(|a| key(a, KeyCode::Char('n')));
    let row = live.with(|a| {
        draw(a, COLS, ROWS);
        a.hit.rows.iter().position(|r| *r == Some(100))
    });
    assert_eq!(row, Some(1), "the title, then line 100");
}

#[test]
fn a_reveal_keeps_a_pointer_that_it_shows_with_n_and_with_a_goto() {
    // A `scroll` holds line 50 at the bottom from step 4; step 5 brings a
    // note on line 52. The presenter moves the view and points at line 52.
    // With `n`, with a goto that applies the `scroll` again, and with a
    // goto to step 5 on screen, the view shows line 52: the pointer stays,
    // and the cursor with it. The pointer is decided once, after the
    // reveal (D14).
    for how in ["n", "goto", "here"] {
        let live = open_json(
            &format!("reveal-pointer-{how}"),
            serde_json::json!([{"id": "n5", "step": 5, "type": "note", "line": 52, "text": "five"}]),
            serde_json::json!([{"at_step": 4, "action": "scroll", "line": 50, "align": "bottom"}]),
            if how == "here" { 5 } else { 4 },
        );
        live.with(|a| key(a, KeyCode::Char('G')));
        top_text(&live);
        live.api_ok("pointer.set", serde_json::json!({ "line": 52 }));
        top_text(&live);
        if how == "n" {
            live.with(|a| key(a, KeyCode::Char('n')));
        } else {
            live.command("goto 5");
        }
        assert_eq!(live.with(|a| a.doc.step), 5, "{how}");
        assert!(row_of(&live, 52).is_some(), "{how}: line 52 in view");
        let at = live.with(|a| (a.pointer.map(|p| p.line), a.cursor_line));
        assert_eq!(at, (Some(52), 52), "{how}");
    }
}

#[test]
fn back_at_the_top_a_reveal_keeps_a_pointer_that_it_shows() {
    // A `scroll` holds line 100 at the top from step 2. At step 3 the
    // pointer is on line 40, where step 1 has a note. A goto to step 1
    // goes back to the top and shows the note: the pointer stays.
    let live = open_json(
        "reveal-top-goto-pointer",
        serde_json::json!([
            {"id": "n1", "step": 1, "type": "note", "line": 40, "text": "one"},
            {"id": "n3", "step": 3, "type": "note", "line": 105, "text": "three"}
        ]),
        serde_json::json!([{"at_step": 2, "action": "scroll", "line": 100, "align": "top"}]),
        3,
    );
    top_text(&live);
    live.api_ok("pointer.set", serde_json::json!({ "line": 40 }));
    top_text(&live);
    live.command("goto 1");
    assert!(row_of(&live, 40).is_some(), "line 40 in view");
    let at = live.with(|a| (a.pointer.map(|p| p.line), a.cursor_line));
    assert_eq!(at, (Some(40), 40));
}

#[test]
fn a_step_back_after_a_reveal_keeps_the_code_on_top() {
    // Step 2 brings a box with a title on lines 30-32, in view: nothing
    // moves. Back at step 1 its title goes: the code on top stays, with no
    // move in the document and with one.
    let moves = [
        serde_json::json!([]),
        serde_json::json!([{"at_step": 3, "action": "wait", "ms": 3_600_000}]),
    ];
    for (k, moves) in moves.into_iter().enumerate() {
        let live = open_steps(
            &format!("reveal-back-top-{k}"),
            serde_json::json!([{"id": "b2", "step": 2, "type": "box", "from_line": 30, "to_line": 32, "label": "title"}]),
            moves,
        );
        live.with(|a| {
            for _ in 0..20 {
                key(a, KeyCode::Char('j'));
            }
        });
        assert_eq!(top_text(&live), "let v21 = 21;");
        live.with(|a| key(a, KeyCode::Char('n')));
        assert_eq!(top_text(&live), "let v21 = 21;", "{k}: step 2");
        live.with(|a| key(a, KeyCode::Char('p')));
        assert_eq!(top_text(&live), "let v21 = 21;", "{k}: step 1");
    }
}

#[test]
fn a_reveal_under_an_older_scroll_starts_from_the_view_of_the_step_before() {
    // A `scroll` holds line 50 at the top from step 4. Step 5 brings a note
    // on line 80: the view goes down to it. Step 6 brings a note on line
    // 70, in view: the view stays, and does not go back to the `scroll`.
    let live = open_json(
        "reveal-second",
        serde_json::json!([
            {"id": "n5", "step": 5, "type": "note", "line": 80, "text": "five"},
            {"id": "n6", "step": 6, "type": "note", "line": 70, "text": "six"}
        ]),
        serde_json::json!([{"at_step": 4, "action": "scroll", "line": 50, "align": "top"}]),
        4,
    );
    assert_eq!(top_text(&live), "let v50 = 50;");
    live.with(|a| key(a, KeyCode::Char('n')));
    let five = top_text(&live);
    assert_ne!(five, "let v50 = 50;", "step 5: the note came in");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), five, "step 6");
}

#[test]
fn a_line_written_out_of_the_window_of_a_link_does_not_keep_a_box_out() {
    // A `code.link` shows lines 1-130 of a.rs. Step 2 inserts a line at
    // line 140, out of the window, and brings a box on lines 118-120,
    // under the view: the box comes in whole.
    let doc = serde_json::json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
        "views": {"a.rs": code_of(160, "a")},
        "annotations": [
            {"id": "b2", "step": 2, "type": "box", "from_line": 118, "to_line": 120, "label": "two", "file": "a.rs"}
        ],
        "moves": [
            {"at_step": 1, "action": "code.link", "file": "a.rs", "lines": "1-130"},
            {"at_step": 2, "action": "code.patch", "op": "insert", "position": "140", "content": "// out"}
        ],
        "step": 1
    });
    let live = open_doc("reveal-window", doc, true);
    assert_eq!(top_text(&live), "let a1 = 1;");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert!(row_of(&live, 118).is_some(), "the first line of the box");
    assert!(row_of(&live, 120).is_some(), "the last line of the box");
}

#[test]
fn a_step_back_to_code_that_is_not_on_screen_shows_it_from_its_top() {
    // a.rs from step 1; from step 3, b.rs or lines 100-160 of a.rs. The
    // view goes down at step 3; a step back shows a.rs from its top, as in
    // normal mode: the code of the view is not on screen there (N (a)
    // keeps only a view on the same code).
    let links = [
        serde_json::json!({"at_step": 3, "action": "code.link", "file": "b.rs"}),
        serde_json::json!({"at_step": 3, "action": "code.link", "file": "a.rs", "lines": "100-160"}),
    ];
    for (k, link) in links.into_iter().enumerate() {
        let doc = serde_json::json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
            "views": {"a.rs": code_of(160, "a"), "b.rs": code_of(160, "b")},
            "annotations": [],
            "moves": [
                {"at_step": 1, "action": "code.link", "file": "a.rs", "lines": "1-80"},
                link
            ],
            "step": 3
        });
        for present in [true, false] {
            let live = open_doc(
                &format!("reveal-off-screen-{k}-{present}"),
                doc.clone(),
                present,
            );
            live.with(|a| {
                for _ in 0..20 {
                    key(a, KeyCode::Char('j'));
                }
            });
            top_text(&live);
            live.with(|a| key(a, KeyCode::Char('p')));
            assert_eq!(top_text(&live), "let a1 = 1;", "{k}, present {present}");
        }
    }
}

#[test]
fn an_agent_step_change_lets_the_reveal_of_the_step_before_go() {
    // Step 2 brings a box with a title on lines 30-32, in view. An agent
    // sets step 1: the title goes, and the code on top stays.
    let live = open_steps(
        "reveal-agent-step",
        serde_json::json!([{"id": "b2", "step": 2, "type": "box", "from_line": 30, "to_line": 32, "label": "title"}]),
        serde_json::json!([]),
    );
    live.with(|a| {
        for _ in 0..20 {
            key(a, KeyCode::Char('j'));
        }
    });
    assert_eq!(top_text(&live), "let v21 = 21;");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(top_text(&live), "let v21 = 21;", "step 2");
    live.api_ok("scene.patch", serde_json::json!({ "step": 1 }));
    assert_eq!(live.with(|a| a.doc.step), 1);
    assert_eq!(top_text(&live), "let v21 = 21;");
}

#[test]
fn leaving_presentation_mode_lets_a_reveal_go() {
    let live = open_steps(
        "reveal-leave",
        serde_json::json!([{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}]),
        serde_json::json!([]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    top_text(&live);
    assert!(
        live.with(|a| a.scroll_held.is_some()),
        "the reveal holds the view"
    );
    live.with(|a| a.set_present(false));
    assert!(live.with(|a| a.scroll_held.is_none()));
}

#[test]
fn back_at_the_top_with_the_pointer_off_the_cursor_goes_where_the_step_puts_it() {
    // A `cursor` on line 10 at step 1, a `scroll` to line 100 at the top at
    // step 2, and the pointer on line 110. Back at step 1 the view goes to
    // the top, where line 110 is not: the pointer goes off, and the cursor
    // goes to line 10, as with no pointer. In both modes.
    let doc = serde_json::json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
        "annotations": [],
        "moves": [
            {"at_step": 1, "action": "cursor", "line": 10},
            {"at_step": 2, "action": "scroll", "line": 100, "align": "top"}
        ],
        "step": 2
    });
    for present in [false, true] {
        let live = open_doc(
            &format!("reveal-top-cursor-{present}"),
            doc.clone(),
            present,
        );
        live.api_ok("pointer.set", serde_json::json!({ "line": 110 }));
        top_text(&live);
        live.with(|a| key(a, KeyCode::Char('p')));
        top_text(&live);
        let at = live.with(|a| (a.doc.step, a.pointer.is_none(), a.cursor_line));
        assert_eq!(at, (1, true, 10), "present {present}");
    }
}

#[test]
fn an_agent_that_goes_back_to_the_top_keeps_the_pointer_only_where_the_top_shows_it() {
    // A `scroll` to line 100 at the top at step 2. An agent sets step 1:
    // the view goes to the top. The pointer on line 110 goes off; the one
    // on line 5 stays, and the cursor with it (D14).
    for (line, kept) in [(110, None), (5, Some(5))] {
        let live = open_json(
            &format!("reveal-agent-top-{line}"),
            serde_json::json!([]),
            serde_json::json!([{"at_step": 2, "action": "scroll", "line": 100, "align": "top"}]),
            2,
        );
        live.api_ok("pointer.set", serde_json::json!({ "line": line }));
        top_text(&live);
        live.api_ok("scene.patch", serde_json::json!({ "step": 1 }));
        assert_eq!(top_text(&live), "let v1 = 1;", "{line}");
        let at = live.with(|a| (a.doc.step, a.pointer.map(|p| p.line), a.cursor_line));
        assert_eq!(at, (1, kept, kept.unwrap_or(1)), "{line}");
    }
}

#[test]
fn after_a_reveal_f_points_at_what_came_in() {
    let live = open_steps(
        "reveal-f",
        serde_json::json!([{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}]),
        serde_json::json!([]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    top_text(&live);
    live.with(|a| key(a, KeyCode::Char('f')));
    assert_eq!(live.with(|a| a.pointer.map(|p| p.line)), Some(100));
}

#[test]
fn the_annotations_of_a_file_not_on_screen_do_not_come_in() {
    // a.rs is on screen from step 1. Step 2 brings a note on line 100 of
    // the main file: the view of a.rs does not move.
    let doc = serde_json::json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
        "views": {"a.rs": code_of(160, "a")},
        "annotations": [
            {"id": "n2", "step": 2, "type": "note", "line": 100, "text": "main"}
        ],
        "moves": [{"at_step": 1, "action": "code.link", "file": "a.rs"}],
        "step": 1
    });
    let live = open_doc("reveal-other-file-note", doc, true);
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(live.with(|a| a.doc.step), 2);
    assert_eq!(top_text(&live), "let a1 = 1;");
}

#[test]
fn the_auto_advance_shows_what_comes_in() {
    let doc = serde_json::json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 5, "text": "start"},
            {"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}
        ],
        "steps": [{"step": 1, "auto_advance_ms": 3_600_000}],
        "step": 1
    });
    let live = open_doc("reveal-auto", doc, true);
    live.with(|a| {
        a.auto_advance_tick(Instant::now() + Duration::from_secs(2 * 3600));
    });
    assert_eq!(live.with(|a| a.doc.step), 2);
    assert!(row_of(&live, 100).is_some());
}

#[test]
fn a_move_that_does_not_apply_shows_nothing_new() {
    // Step 2 inserts a line at line 140. Then the move changes in the
    // document, as an edit of the file does, to line 500 of a code of 160
    // lines: it does not apply, and the note on line 100 does not take the
    // view.
    let live = open_steps(
        "reveal-loud",
        serde_json::json!([{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}]),
        serde_json::json!([{
            "at_step": 2, "action": "code.patch", "op": "insert", "position": "140", "content": "// no"
        }]),
    );
    live.with(|a| {
        let MoveAction::CodePatch(p) = &mut a.doc.moves[0].action else {
            unreachable!()
        };
        p.position = Some("500".into());
        key(a, KeyCode::Char('n'));
    });
    assert_eq!(live.with(|a| a.doc.step), 2);
    assert!(
        live.with(|a| a
            .message
            .as_ref()
            .is_some_and(|m| m.0.starts_with("moves:"))),
        "the move does not apply"
    );
    assert_eq!(top_text(&live), "let v1 = 1;");
    // Nor a goto or `:step` to the step on screen, nor the open there.
    for how in ["goto 2", "step 2"] {
        live.command(how);
        assert_eq!(top_text(&live), "let v1 = 1;", "{how}");
    }
    live.with(|a| a.reveal_at_open());
    assert_eq!(top_text(&live), "let v1 = 1;", "the open");
}

#[test]
fn a_reveal_gives_the_same_view_with_n_or_a_goto_with_or_without_the_pointer_or_a_scroll() {
    // The matrix of the reveal: `n` or a goto to the next step, the
    // pointer off, on a line that stays in view or on one that goes out, a
    // `scroll` of step 1 or none, a step that types or not (a goto lands
    // with the typing done). Each case starts with line 40 on top, and
    // step 2 brings a box on lines 70-72 (and types three lines at line
    // 73). In each case the box comes in, the view is the same, and the
    // pointer stays only where it is in view, with the cursor; with no
    // pointer the cursor goes to line 70.
    let mut views: Vec<(bool, String)> = Vec::new();
    for typing in [false, true] {
        for how in ["n", "goto"] {
            if typing && how == "goto" {
                continue;
            }
            for scroll in [false, true] {
                for pointer in [None, Some(55), Some(45)] {
                    let what =
                        format!("typing {typing}, {how}, scroll {scroll}, pointer {pointer:?}");
                    let mut moves = Vec::new();
                    if scroll {
                        moves.push(serde_json::json!({"at_step": 1, "action": "scroll", "line": 40, "align": "top"}));
                    }
                    if typing {
                        moves.push(serde_json::json!({
                            "at_step": 2, "action": "code.patch", "op": "insert", "position": "73",
                            "content": "// t1\n// t2\n// t3", "typing": "line", "speed_ms": 3_600_000
                        }));
                    }
                    let live = open_json(
                        &format!(
                            "rm-{}{how}{}{}",
                            u8::from(typing),
                            u8::from(scroll),
                            pointer.unwrap_or(0)
                        ),
                        serde_json::json!([
                            {"id": "n1", "step": 1, "type": "note", "line": 5, "text": "start"},
                            {"id": "b2", "step": 2, "type": "box", "from_line": 70, "to_line": 72, "label": "two"}
                        ]),
                        serde_json::Value::Array(moves),
                        1,
                    );
                    if !scroll {
                        live.with(|a| {
                            for _ in 0..39 {
                                key(a, KeyCode::Char('j'));
                            }
                        });
                    }
                    assert_eq!(top_text(&live), "let v40 = 40;", "{what}: before");
                    if let Some(line) = pointer {
                        live.api_ok("pointer.set", serde_json::json!({ "line": line }));
                        assert_eq!(top_text(&live), "let v40 = 40;", "{what}: pointer");
                    }
                    if how == "n" {
                        live.with(|a| key(a, KeyCode::Char('n')));
                    } else {
                        live.command("goto 2");
                    }
                    assert_eq!(
                        live.with(|a| (a.doc.step, a.is_typing())),
                        (2, typing),
                        "{what}"
                    );
                    let last = if typing { 75 } else { 72 };
                    for line in [70, last] {
                        assert!(row_of(&live, line).is_some(), "{what}: line {line} in view");
                    }
                    let kept = pointer.filter(|l| row_of(&live, *l).is_some());
                    assert_eq!(
                        kept,
                        pointer.filter(|l| *l == 55),
                        "{what}: 55 stays, 45 goes out"
                    );
                    let at = live.with(|a| (a.pointer.map(|p| p.line), a.cursor_line));
                    assert_eq!(at, (kept, kept.unwrap_or(70)), "{what}: pointer and cursor");
                    views.push((typing, top_text(&live)));
                }
            }
        }
    }
    for typing in [false, true] {
        let mut tops: Vec<&String> = views
            .iter()
            .filter(|v| v.0 == typing)
            .map(|v| &v.1)
            .collect();
        tops.dedup();
        assert_eq!(tops.len(), 1, "typing {typing}: one view, {tops:?}");
    }
}

#[test]
fn a_box_that_comes_in_plays_its_effect_where_it_comes_in() {
    // Step 2 brings a box with `draw` on lines 100-102, under the view: the
    // view moves before the effect starts, and keeps its place while the
    // effect plays and after it.
    let live = open_steps(
        "reveal-effect",
        serde_json::json!([{"id": "b2", "step": 2, "type": "box", "from_line": 100, "to_line": 102, "label": "drawn", "effect": "draw"}]),
        serde_json::json!([]),
    );
    live.with(|a| key(a, KeyCode::Char('n')));
    assert!(live.with(|a| a.is_playing_effect()), "the effect plays");
    let start = top_text(&live);
    assert!(row_of(&live, 102).is_some(), "in view at the start");
    for ms in [60, 240] {
        live.with(move |a| a.hold_effect_at(ms));
        assert_eq!(top_text(&live), start, "at {ms} ms");
    }
    live.with(|a| a.finish_effect());
    assert_eq!(top_text(&live), start, "at the end");
    assert!(row_of(&live, 102).is_some());
}

#[test]
fn a_smaller_terminal_still_shows_what_came_in() {
    // Step 2 brings a box on lines 100-102 under the view: it comes in at
    // the bottom. The terminal then gets 8 rows shorter: the box stays in
    // view whole. The same while step 2 types three lines at line 100.
    let typed = serde_json::json!([{
        "at_step": 2, "action": "code.patch", "op": "insert", "position": "100",
        "content": "// t1\n// t2\n// t3", "typing": "line", "speed_ms": 3_600_000
    }]);
    for (k, moves) in [serde_json::json!([]), typed].into_iter().enumerate() {
        let live = open_steps(
            &format!("reveal-resize-{k}"),
            serde_json::json!([{"id": "b2", "step": 2, "type": "box", "from_line": 100, "to_line": 102, "label": "two"}]),
            moves,
        );
        live.with(|a| key(a, KeyCode::Char('n')));
        assert_eq!(live.with(|a| a.is_typing()), k == 1);
        assert!(row_of(&live, 102).is_some(), "{k}: in view");
        let rows = live.with(|a| {
            draw(a, COLS, ROWS - 8);
            [100, 102].map(|l| a.hit.rows.iter().position(|r| *r == Some(l)))
        });
        assert!(rows.iter().all(Option::is_some), "{k}: {rows:?}");
    }
}

#[test]
fn step_to_the_step_on_screen_shows_what_came_in_again() {
    // As a goto there: `:step 2` and `step.set` at step 2, after the view
    // went to the top.
    for how in [":step", "step.set"] {
        let live = open_steps(
            &format!("reveal-step-here{}", how.len()),
            serde_json::json!([{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}]),
            serde_json::json!([]),
        );
        live.with(|a| key(a, KeyCode::Char('n')));
        live.with(|a| key(a, KeyCode::Char('g')));
        assert_eq!(top_text(&live), "let v1 = 1;");
        if how == ":step" {
            live.command("step 2");
        } else {
            live.api_ok("step.set", serde_json::json!({ "step": 2 }));
        }
        assert!(row_of(&live, 100).is_some(), "{how}");
    }
}

#[test]
fn starting_presentation_mode_shows_nothing_new() {
    // Not a step change: the view stays at the top, under the note of
    // step 2 on line 100.
    let doc = serde_json::json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
        "annotations": [{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}],
        "step": 2
    });
    let live = open_doc("reveal-start-p", doc, false);
    live.with(|a| key(a, KeyCode::Char('P')));
    assert!(live.with(|a| a.present));
    assert_eq!(top_text(&live), "let v1 = 1;");
}

/// The real TUI in presentation mode, at step 2 by `--step 2` and by the
/// step that the document keeps: the note of step 2 on line 100 comes into
/// view, and the hidden cursor goes to it, so `f` points at line 100. In
/// normal mode nothing comes in: `f` points at line 1.
#[test]
fn the_tui_that_opens_at_a_step_shows_what_comes_in() {
    let dir = scratch("reveal-open-tui");
    for (name, step) in [("one.tiza", 1), ("two.tiza", 2)] {
        let doc = serde_json::json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
            "annotations": [{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}],
            "step": step
        });
        std::fs::write(dir.join(name), doc.to_string()).unwrap();
    }
    let body = r##"
import json, subprocess
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
runs = [
    ("step", ["--present", "one.tiza", "--step", "2"]),
    ("kept", ["--present", "two.tiza"]),
    ("normal", ["two.tiza"]),
]
for name, args in runs:
    sock = name + ".sock"
    s = Pty([binary, "--no-save", "--socket", sock] + args, 30, 80, env=env)

    def pointer():
        r = subprocess.run([binary, "--socket", sock, "api", "status"], capture_output=True, text=True, timeout=10)
        return (json.loads(r.stdout).get("pointer") or {}).get("line") if r.returncode == 0 else None

    if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 10, "first frame"):
        s.send(b"f")
        end = time.monotonic() + 10
        line = None
        while time.monotonic() < end and line is None:
            line = pointer()
        print("POINTER", name, line, flush=True)
    s.send(b"\x1b:q!\r")
    print("EXIT", name, s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "open.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    for line in ["POINTER step 100", "POINTER kept 100", "POINTER normal 1"] {
        assert!(out.lines().any(|l| l == line), "{line}: {out}");
    }
}

#[test]
fn with_the_pointer_on_the_wheel_ends_an_effect_as_a_key_does() {
    use tiza_tui::app::MouseKind;
    // Step 2 brings a box with `draw` on lines 10-12, in view; the pointer
    // is on line 1. A notch of the wheel while the box draws ends the
    // effect and does nothing else, as `j` does.
    for wheel in [false, true] {
        let live = open_steps(
            &format!("reveal-wheel-effect-{wheel}"),
            serde_json::json!([{"id": "b2", "step": 2, "type": "box", "from_line": 10, "to_line": 12, "label": "drawn", "effect": "draw"}]),
            serde_json::json!([]),
        );
        live.with(|a| key(a, KeyCode::Char('f')));
        live.with(|a| key(a, KeyCode::Char('n')));
        top_text(&live);
        assert!(live.with(|a| a.is_playing_effect()), "wheel {wheel}");
        live.with(move |a| {
            if wheel {
                a.mouse = true;
                a.on_mouse(MouseKind::ScrollDown, 10, 5);
            } else {
                key(a, KeyCode::Char('j'));
            }
        });
        top_text(&live);
        let at = live.with(|a| (a.is_playing_effect(), a.pointer.map(|p| p.line)));
        assert_eq!(at, (false, Some(1)), "wheel {wheel}");
    }
}

#[test]
fn at_the_open_a_camera_of_the_step_wins() {
    // Step 2 holds line 40 at the top and brings a note on line 100: when
    // the TUI opens there, the `scroll` places the view, and the note does
    // not take it (B (a)). So does a `cursor` of the step on line 10. With
    // neither, the note comes in.
    for camera in ["scroll", "cursor", "none"] {
        let moves = match camera {
            "scroll" => {
                serde_json::json!([{"at_step": 2, "action": "scroll", "line": 40, "align": "top"}])
            }
            "cursor" => serde_json::json!([{"at_step": 2, "action": "cursor", "line": 10}]),
            _ => serde_json::json!([]),
        };
        let doc = serde_json::json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
            "annotations": [{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}],
            "moves": moves, "step": 2
        });
        let live = open_doc(&format!("reveal-open-{camera}"), doc, true);
        live.with(|a| a.reveal_at_open());
        match camera {
            "scroll" => assert_eq!(top_text(&live), "let v40 = 40;"),
            "cursor" => assert_eq!(row_of(&live, 100), None, "cursor"),
            _ => assert!(row_of(&live, 100).is_some()),
        }
    }
}

#[test]
fn in_normal_mode_step_to_the_step_on_screen_moves_nothing() {
    // Step 2 holds line 60 at the top. The cursor goes to the end with
    // `G`: `step.set` and `:step` to step 2 leave the view there (normal
    // mode does not change).
    for how in [":step", "step.set"] {
        let doc = serde_json::json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
            "annotations": [{"id": "n2", "step": 2, "type": "note", "line": 100, "text": "two"}],
            "moves": [{"at_step": 2, "action": "scroll", "line": 60, "align": "top"}],
            "step": 1
        });
        let live = open_doc(&format!("step-here-normal{}", how.len()), doc, false);
        live.with(|a| key(a, KeyCode::Char('n')));
        assert_eq!(top_text(&live), "let v60 = 60;");
        live.with(|a| key(a, KeyCode::Char('G')));
        let end = top_text(&live);
        assert_ne!(end, "let v60 = 60;");
        if how == ":step" {
            live.command("step 2");
        } else {
            live.api_ok("step.set", serde_json::json!({ "step": 2 }));
        }
        assert_eq!(top_text(&live), end, "{how}");
    }
}

#[test]
fn in_normal_mode_with_a_margin_the_lines_of_a_later_box_keep_their_rows() {
    // A `scroll` of step 2 shows the box of step 3 (lines 80-82), in normal
    // mode with a `margin` of 20. The box that comes in at step 3 makes the
    // drawing wider; the screen keeps its size, so the lines of the box
    // keep their rows.
    let doc = serde_json::json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code_of(160, "v"),
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 5, "text": "start"},
            {"id": "b3", "step": 3, "type": "box", "from_line": 80, "to_line": 82, "label": "the box"}
        ],
        "moves": [{"at_step": 2, "action": "scroll", "annotation": "b3", "align": "show"}],
        "margin": 20,
        "step": 1
    });
    let live = open_doc("later-box-margin", doc, false);
    live.with(|a| {
        for _ in 0..96 {
            key(a, KeyCode::Char('j'));
        }
    });
    top_text(&live);
    let rows = |live: &Live| {
        live.with(|a| {
            draw(a, COLS, ROWS);
            [80, 82].map(|l| a.hit.rows.iter().position(|r| *r == Some(l)))
        })
    };
    live.with(|a| key(a, KeyCode::Char('n')));
    let step2 = rows(&live);
    assert!(step2.iter().all(Option::is_some), "{step2:?}");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(rows(&live), step2);
}
