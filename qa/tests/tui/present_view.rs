//! The view in presentation mode, as in a web browser: the keys and the
//! wheel move the view, not the hidden cursor, and the view stays on the
//! same code until something asks to move it.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
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
        Event::GotoHere => {
            let step = live.with(|a| a.doc.step);
            live.command(&format!("goto {step}"));
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
    let dir = scratch(name);
    let code: Vec<String> = (1..=160).map(|i| format!("let v{i} = {i};")).collect();
    let doc = serde_json::json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code,
        "annotations": annotations, "moves": moves, "step": step
    });
    std::fs::write(dir.join("doc.tiza"), doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&dir.join("doc.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.command("pad 0 0");
    live.with(|a| {
        a.autosave = false;
        a.set_present(true);
        draw(a, COLS, ROWS);
    });
    live
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
    // Step 2 types two lines at line 150, one an hour.
    let moves = serde_json::json!([{
        "at_step": 2, "action": "code.patch", "op": "insert", "position": "150",
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
