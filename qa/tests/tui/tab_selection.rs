//! Tab selects only what the screen draws, and puts the cursor on the line
//! where it is drawn: not an annotation of another file, not a spotlight
//! past its `until`, and not on the line number that a move shifted. The
//! list (`L`, Enter) still reaches every annotation.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::Live;

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    let d = 4;\n    let e = 5;\n}\n";

/// a1 on line 3 (step 1), s1 a spotlight on line 5 (step 1 only), a3 on
/// line 6 (step 2); at step 2 a line goes in at the top, and at step 3
/// another file is shown, with a4 on its line 2.
fn live() -> Live {
    let live = Live::from_code(CODE, "rs", "tab-drawn");
    let dir = live.with(|app| app.save_path.parent().unwrap().to_path_buf());
    std::fs::write(dir.join("small.rs"), "fn x() {}\nfn y() {}\n").unwrap();
    for (method, params) in [
        (
            "annotation.add",
            json!({"type": "note", "line": 3, "text": "one", "step": 1}),
        ),
        (
            "annotation.add",
            json!({"type": "spotlight", "from_line": 5, "to_line": 5, "step": 1}),
        ),
        (
            "annotation.add",
            json!({"type": "note", "line": 6, "text": "three", "step": 2}),
        ),
        (
            "move.add",
            json!({"at_step": 2, "action": "code.patch", "op": "insert", "position": "1", "content": "// top"}),
        ),
        (
            "move.add",
            json!({"at_step": 3, "action": "code.link", "file": "small.rs"}),
        ),
        (
            "annotation.add",
            json!({"type": "note", "line": 2, "text": "four", "step": 3}),
        ),
    ] {
        let r = live.api(method, params.clone());
        assert!(r.error.is_none(), "{method} {params}: {:?}", r.error);
    }
    live
}

/// The ids that Tab visits from no selection, once around, with the cursor
/// line at each.
fn tab_round(live: &Live) -> Vec<(String, u32)> {
    live.with(|app| app.selected = None);
    let mut seen = Vec::new();
    loop {
        live.keys(&[KeyCode::Tab]);
        let at = live.with(|app| (app.selected.clone().unwrap_or_default(), app.cursor_line));
        if seen.first() == Some(&at) || seen.len() > 10 {
            break;
        }
        seen.push(at);
    }
    seen
}

fn ids(round: &[(String, u32)]) -> Vec<&str> {
    round.iter().map(|(id, _)| id.as_str()).collect()
}

#[test]
fn tab_visits_only_what_the_screen_draws() {
    let live = live();
    live.command("step 1");
    assert_eq!(ids(&tab_round(&live)), ["a1", "a2"], "step 1: a1 and s1");
    // Step 2: the spotlight is past, and the inserted line moves a1 to
    // line 4 on the screen; the cursor goes there.
    live.command("step 2");
    let round = tab_round(&live);
    assert_eq!(ids(&round), ["a1", "a3"], "step 2: {round:?}");
    assert_eq!(round[0].1, 4, "the cursor on the line where a1 is drawn");
    // The bar names that line too, not the line that a1 was added on.
    live.with(|app| app.selected = Some("a1".into()));
    let bar = live.with(|app| app.selected_text()).unwrap_or_default();
    assert!(bar.contains("L4"), "{bar}");
    // Shift-Tab goes the other way.
    live.with(|app| app.selected = None);
    live.keys(&[KeyCode::BackTab]);
    assert_eq!(live.with(|app| app.selected.clone()).as_deref(), Some("a3"));
    // Step 3 shows small.rs: only its annotation.
    live.command("step 3");
    let round = tab_round(&live);
    assert_eq!(ids(&round), ["a4"], "step 3: {round:?}");
    assert_eq!(round[0].1, 2);
}

#[test]
fn the_list_still_reaches_an_annotation_that_tab_skips() {
    let live = live();
    live.command("step 3");
    // a1 is of main.rs, which step 3 does not show: Enter on its row goes
    // to its step and selects it, the cursor on its line.
    let item = tiza_tui::list::ListItem::Annotation {
        id: "a1".into(),
        step: 1,
    };
    live.with(move |app| app.go_to_list_item(&item));
    let at = live.with(|app| (app.doc.step, app.selected.clone(), app.cursor_line));
    assert_eq!(at, (1, Some("a1".to_string()), 3));
}

/// Enter in the list selects an annotation that its own step does not
/// draw (its file is not shown there), and leaves the cursor where it is:
/// the line number would be in another file.
#[test]
fn the_list_selects_an_annotation_that_its_step_hides() {
    let live = Live::from_code(CODE, "rs", "tab-hidden");
    let dir = live.with(|app| app.save_path.parent().unwrap().to_path_buf());
    std::fs::write(dir.join("small.rs"), "fn x() {}\nfn y() {}\n").unwrap();
    // a1 on main.rs at step 2, then step 2 shows small.rs.
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 2, "text": "one", "step": 2}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.link", "file": "small.rs"}),
    );
    live.command("step 2");
    live.with(|app| app.cursor_line = 1);
    let item = tiza_tui::list::ListItem::Annotation {
        id: "a1".into(),
        step: 2,
    };
    live.with(move |app| app.go_to_list_item(&item));
    let at = live.with(|app| (app.selected.clone(), app.cursor_line));
    assert_eq!(at, (Some("a1".to_string()), 1));
}

/// A spotlight of ids only has no lines of its own: Tab takes the line of
/// what it names, and never puts the cursor on line 0.
#[test]
fn a_spotlight_of_ids_takes_the_line_of_what_it_names() {
    let live = Live::from_code(CODE, "rs", "tab-spot-ids");
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 4, "text": "one", "step": 1}),
    );
    live.command("step 1");
    live.command("spot a1");
    let round = tab_round(&live);
    assert_eq!(round.len(), 2, "{round:?}");
    assert!(round.iter().all(|(_, line)| *line == 4), "{round:?}");
}

#[test]
fn a_step_that_draws_none_says_where_they_are() {
    let live = Live::from_code(CODE, "rs", "tab-none");
    let dir = live.with(|app| app.save_path.parent().unwrap().to_path_buf());
    std::fs::write(dir.join("small.rs"), "fn x() {}\n").unwrap();
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 3, "text": "one", "step": 1}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.link", "file": "small.rs"}),
    );
    live.command("step 2");
    live.keys(&[KeyCode::Tab]);
    let (message, _) = live.message().unwrap_or_default();
    assert_eq!(
        message,
        "no annotation on the screen at this step: L lists them all"
    );
    assert_eq!(live.with(|app| app.selected.clone()), None);
}

// ---- presentation mode: Tab goes through what the view shows ----

const COLS: u16 = 80;
const ROWS: u16 = 24;

/// A document of 120 lines `let vN = N;` with `annotations` and `moves`,
/// at `step`, with no padding, in presentation mode when `present`.
fn open_lines(
    name: &str,
    annotations: serde_json::Value,
    moves: serde_json::Value,
    step: u32,
    present: bool,
) -> Live {
    let dir = tiza_qa::scratch(name);
    let code: Vec<String> = (1..=120).map(|i| format!("let v{i} = {i};")).collect();
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code,
        "annotations": annotations, "moves": moves, "step": step
    });
    std::fs::write(dir.join("doc.tiza"), doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&dir.join("doc.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.command("pad 0 0");
    live.with(move |a| {
        a.autosave = false;
        a.set_present(present);
        frame(a);
    });
    live
}

fn frame(a: &mut tiza_tui::app::App) -> ratatui::buffer::Buffer {
    let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(COLS, ROWS)).unwrap();
    t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
    t.backend().buffer().clone()
}

/// The code on the first row of a new frame.
fn top_text(live: &Live) -> String {
    live.with(|a| {
        let b = frame(a);
        let row: String = (0..COLS).map(|x| b[(x, 0)].symbol()).collect();
        let code: String = row.chars().skip(a.hit.code_x as usize).collect();
        code.split('│').next().unwrap_or("").trim_end().to_string()
    })
}

/// Presses `keys`, then draws a frame.
fn press(live: &Live, keys: &[KeyCode]) {
    live.keys(keys);
    live.with(|a| {
        frame(a);
    });
}

/// The card's document: notes and boxes of steps 1-4 on lines 5-100, and
/// step 4 holds line 80 at the top (a `scroll`).
fn tab_varias(name: &str, present: bool) -> Live {
    open_lines(
        name,
        json!([
            {"id": "a1", "step": 1, "type": "note", "line": 5, "text": "one"},
            {"id": "a2", "step": 1, "type": "box", "from_line": 30, "to_line": 32, "label": "two"},
            {"id": "a3", "step": 2, "type": "note", "line": 20, "text": "three"},
            {"id": "a4", "step": 3, "type": "box", "from_line": 50, "to_line": 52, "label": "four"},
            {"id": "a5", "step": 3, "type": "note", "line": 8, "text": "five"},
            {"id": "a6", "step": 4, "type": "note", "line": 90, "text": "six"},
            {"id": "a7", "step": 4, "type": "note", "line": 100, "text": "seven"}
        ]),
        json!([{"at_step": 4, "action": "scroll", "line": 80, "align": "top"}]),
        4,
        present,
    )
}

#[test]
fn in_presentation_tab_goes_through_what_the_view_shows_and_the_view_stays() {
    // Step 4 holds line 80 at the top: a6 (line 90) and a7 (line 100) are
    // in the view, a1-a5 are above it. Tab and Shift-Tab go round a6 and
    // a7; the view does not move. With the pointer on, it goes with them.
    for pointer in [false, true] {
        let live = tab_varias(&format!("tab-view-{pointer}"), true);
        assert_eq!(top_text(&live), "let v80 = 80;");
        if pointer {
            press(&live, &[KeyCode::Char('f')]);
        }
        let mut lines = Vec::new();
        for k in [KeyCode::Tab, KeyCode::Tab, KeyCode::Tab, KeyCode::BackTab] {
            press(&live, &[k]);
            lines.push(live.with(|a| a.cursor_line));
            assert_eq!(top_text(&live), "let v80 = 80;", "pointer {pointer}");
        }
        assert_eq!(lines, [90, 100, 90, 100], "pointer {pointer}");
        let at = live.with(|a| a.pointer.map(|p| p.line));
        assert_eq!(at, pointer.then_some(100), "the pointer goes with Tab");
    }
    // Normal mode does not change: Tab takes a1 first, with a selection.
    let live = tab_varias("tab-view-normal", false);
    press(&live, &[KeyCode::Tab]);
    let got = live.with(|a| (a.cursor_line, a.selected.clone()));
    assert_eq!(got, (5, Some("a1".to_string())));
}

#[test]
fn in_presentation_no_key_edits_what_tab_reached() {
    // Tab reaches a6; then the keys that edit a selected annotation in
    // normal mode: nothing changes, and no command line opens.
    let live = tab_varias("tab-no-edit", true);
    press(&live, &[KeyCode::Tab]);
    assert_eq!(
        live.with(|a| (a.cursor_line, a.selected.clone())),
        (90, None)
    );
    let before = live.with(|a| (a.revision, a.doc.annotations.clone()));
    for k in ['x', '+', '-', 'c', 'e'] {
        press(&live, &[KeyCode::Char(k)]);
    }
    press(&live, &[KeyCode::Enter]);
    let after = live.with(|a| (a.revision, a.doc.annotations.clone()));
    assert_eq!(after, before);
    assert!(live.with(|a| a.mode == tiza_tui::app::Mode::Normal));
}

#[test]
fn starting_presentation_clears_a_selection_and_the_list_selects_nothing() {
    // A selection of normal mode goes with `P`: then `x` deletes nothing.
    let live = tab_varias("tab-p-clears", false);
    press(&live, &[KeyCode::Tab]);
    assert!(live.with(|a| a.selected.is_some()));
    press(&live, &[KeyCode::Char('P'), KeyCode::Char('x')]);
    assert!(live.with(|a| a.present && a.selected.is_none()));
    assert_eq!(live.with(|a| a.doc.annotations.len()), 7);
    // Enter on a row of the list goes to the annotation, and selects
    // nothing in presentation mode (in normal mode it selects it).
    for present in [true, false] {
        let live = tab_varias(&format!("tab-list-{present}"), present);
        press(&live, &[KeyCode::Char('L'), KeyCode::Enter]);
        let selected = live.with(|a| a.selected.is_some());
        assert_eq!(selected, !present, "present {present}");
    }
}

#[test]
fn a_tall_box_in_view_takes_its_first_line_in_view_and_tab_stays_on_it() {
    // A box on lines 40-110 fills the view from line 60: Tab goes to line
    // 60, and a second Tab stays there (it is the only one). With the
    // pointer on, the pointer goes there, and the view stays.
    for pointer in [false, true] {
        let live = open_lines(
            &format!("tab-tall-{pointer}"),
            json!([{"id": "b1", "step": 1, "type": "box", "from_line": 40, "to_line": 110, "label": "tall"}]),
            json!([]),
            1,
            true,
        );
        live.with(|a| {
            for _ in 0..60 {
                tiza_tui::on_key(a, KeyCode::Char('j'), crossterm::event::KeyModifiers::NONE);
            }
        });
        let top = top_text(&live);
        let first: u32 = top
            .trim_start_matches("let v")
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        if pointer {
            // The pointer on, then 10 lines down: away from line `first`.
            press(&live, &[KeyCode::Char('f')]);
            live.with(|a| {
                for _ in 0..10 {
                    tiza_tui::on_key(a, KeyCode::Char('j'), crossterm::event::KeyModifiers::NONE);
                }
            });
            assert_eq!(live.with(|a| a.pointer.map(|p| p.line)), Some(first + 10));
        }
        let top = top_text(&live);
        for _ in 0..2 {
            press(&live, &[KeyCode::Tab]);
            let at = live.with(|a| (a.cursor_line, a.pointer.map(|p| p.line)));
            assert_eq!(at, (first, pointer.then_some(first)), "pointer {pointer}");
            assert_eq!(top_text(&live), top, "pointer {pointer}");
        }
    }
}

#[test]
fn with_nothing_in_the_view_tab_does_nothing() {
    let live = open_lines(
        "tab-nothing",
        json!([{"id": "n1", "step": 1, "type": "note", "line": 5, "text": "up"}]),
        json!([]),
        1,
        true,
    );
    live.with(|a| {
        for _ in 0..50 {
            tiza_tui::on_key(a, KeyCode::Char('j'), crossterm::event::KeyModifiers::NONE);
        }
    });
    let top = top_text(&live);
    let cursor = live.with(|a| a.cursor_line);
    press(&live, &[KeyCode::Tab]);
    assert_eq!(live.with(|a| a.cursor_line), cursor);
    assert_eq!(top_text(&live), top);
}

#[test]
fn tab_stops_once_on_each_row_and_goes_on_after_a_deleted_one() {
    // Two notes on line 10, and notes on lines 12 and 14. Each Tab moves
    // the cursor: the two of line 10 are one stop. An agent deletes the
    // note where the cursor is: Tab goes on to the next line.
    let live = open_lines(
        "tab-rows",
        json!([
            {"id": "b2", "step": 1, "type": "note", "line": 10, "text": "two"},
            {"id": "b10", "step": 1, "type": "note", "line": 10, "text": "ten"},
            {"id": "c1", "step": 1, "type": "note", "line": 12, "text": "c"},
            {"id": "d1", "step": 1, "type": "note", "line": 14, "text": "d"}
        ]),
        json!([]),
        1,
        true,
    );
    let mut lines = Vec::new();
    for k in [KeyCode::Tab, KeyCode::Tab, KeyCode::BackTab, KeyCode::Tab] {
        press(&live, &[k]);
        lines.push(live.with(|a| a.cursor_line));
    }
    assert_eq!(lines, [10, 12, 10, 12]);
    live.api_ok("annotation.delete", json!({"id": "c1"}));
    press(&live, &[KeyCode::Tab]);
    assert_eq!(live.with(|a| a.cursor_line), 14);
}

#[test]
fn tab_goes_on_from_where_the_presenter_put_the_pointer() {
    // Notes on lines 5, 12 and 20. With the pointer on, Tab goes to line
    // 5; then `j` moves the pointer to line 15, and Tab goes to line 20,
    // the next one under the pointer, not to line 12.
    let live = open_lines(
        "tab-from-pointer",
        json!([
            {"id": "n5", "step": 1, "type": "note", "line": 5, "text": "five"},
            {"id": "n12", "step": 1, "type": "note", "line": 12, "text": "twelve"},
            {"id": "n20", "step": 1, "type": "note", "line": 20, "text": "twenty"}
        ]),
        json!([]),
        1,
        true,
    );
    press(&live, &[KeyCode::Char('f'), KeyCode::Tab]);
    assert_eq!(live.with(|a| a.pointer.map(|p| p.line)), Some(5));
    live.with(|a| {
        for _ in 0..10 {
            tiza_tui::on_key(a, KeyCode::Char('j'), crossterm::event::KeyModifiers::NONE);
        }
    });
    assert_eq!(live.with(|a| a.pointer.map(|p| p.line)), Some(15));
    press(&live, &[KeyCode::Tab]);
    assert_eq!(live.with(|a| a.pointer.map(|p| p.line)), Some(20));
}

#[test]
fn tab_closes_a_selection_that_presentation_does_not_show() {
    // `V`, `j`, `j` and `f`: the pointer on lines 1-3, in a selection that
    // presentation mode does not show. Tab closes it and goes to the note
    // of line 5; the pointer goes there, on one line.
    let live = open_lines(
        "tab-selection-open",
        json!([{"id": "n5", "step": 1, "type": "note", "line": 5, "text": "five"}]),
        json!([]),
        1,
        true,
    );
    press(
        &live,
        &[
            KeyCode::Char('V'),
            KeyCode::Char('j'),
            KeyCode::Char('j'),
            KeyCode::Char('f'),
        ],
    );
    assert_eq!(
        live.with(|a| a.pointer.map(|p| (p.line, p.to))),
        Some((1, 3))
    );
    press(&live, &[KeyCode::Tab]);
    let got = live.with(|a| {
        (
            a.mode == tiza_tui::app::Mode::Normal,
            a.pointer.map(|p| (p.line, p.to)),
        )
    });
    assert_eq!(got, (true, Some((5, 5))));
}

#[test]
fn tab_to_a_box_whose_title_is_above_the_view_does_not_move_the_view() {
    // A box with a title on lines 61-63, a note on line 70 and one on line
    // 110, under the view; the view starts at line 61, so the title is
    // just above it. Shift-Tab goes to the note of line 70 (the last one
    // in the view), then Tab to line 61, and the view stays: it does not
    // go up a row to show the title.
    let live = open_lines(
        "tab-title",
        json!([
            {"id": "b1", "step": 1, "type": "box", "from_line": 61, "to_line": 63, "label": "title"},
            {"id": "n1", "step": 1, "type": "note", "line": 70, "text": "note"},
            {"id": "n2", "step": 1, "type": "note", "line": 110, "text": "under the view"}
        ]),
        json!([]),
        1,
        true,
    );
    live.with(|a| {
        for _ in 0..61 {
            tiza_tui::on_key(a, KeyCode::Char('j'), crossterm::event::KeyModifiers::NONE);
        }
    });
    assert_eq!(top_text(&live), "let v61 = 61;");
    press(&live, &[KeyCode::BackTab]);
    assert_eq!(live.with(|a| a.cursor_line), 70);
    press(&live, &[KeyCode::Tab]);
    assert_eq!(live.with(|a| a.cursor_line), 61);
    assert_eq!(top_text(&live), "let v61 = 61;");
}

#[test]
fn after_a_step_change_tab_goes_on_from_the_cursor() {
    // a1 on line 10 and b1 on line 15; step 2 inserts three lines at line
    // 2, so they move to lines 13 and 18. Tab at step 1 goes to a1. At
    // step 2 the cursor is on the first line that came in (line 2): Tab
    // goes to a1 on line 13, then to b1.
    let live = open_lines(
        "tab-step",
        json!([
            {"id": "a1", "step": 1, "type": "note", "line": 10, "text": "a"},
            {"id": "b1", "step": 1, "type": "note", "line": 15, "text": "b"}
        ]),
        json!([{"at_step": 2, "action": "code.patch", "op": "insert", "position": "2", "content": "// x\n// y\n// z"}]),
        1,
        true,
    );
    press(&live, &[KeyCode::Tab]);
    assert_eq!(live.with(|a| a.cursor_line), 10);
    press(&live, &[KeyCode::Char('n')]);
    assert_eq!(live.with(|a| (a.doc.step, a.cursor_line)), (2, 2));
    let mut lines = Vec::new();
    for _ in 0..2 {
        press(&live, &[KeyCode::Tab]);
        lines.push(live.with(|a| a.cursor_line));
    }
    assert_eq!(lines, [13, 18]);
}

#[test]
fn tab_right_after_a_step_change_uses_the_view_of_the_next_frame() {
    // Step 2 inserts ten lines at line 2. At step 2 the view starts at
    // `let v21` (line 31); `p` goes back to step 1, where the view keeps
    // `let v21` on top (line 21), and Tab comes before any frame: it goes
    // to c1 on line 30, which that view shows, not by the rows of step 2.
    let live = open_lines(
        "tab-no-frame",
        json!([
            {"id": "a1", "step": 1, "type": "note", "line": 15, "text": "a"},
            {"id": "c1", "step": 1, "type": "note", "line": 30, "text": "c"}
        ]),
        json!([{
            "at_step": 2, "action": "code.patch", "op": "insert", "position": "2",
            "content": "// 1\n// 2\n// 3\n// 4\n// 5\n// 6\n// 7\n// 8\n// 9\n// 10"
        }]),
        2,
        true,
    );
    live.with(|a| {
        for _ in 0..30 {
            tiza_tui::on_key(a, KeyCode::Char('j'), crossterm::event::KeyModifiers::NONE);
        }
    });
    assert_eq!(top_text(&live), "let v21 = 21;");
    live.keys(&[KeyCode::Char('p'), KeyCode::Tab]);
    assert_eq!(live.with(|a| (a.doc.step, a.cursor_line)), (1, 30));
    assert_eq!(top_text(&live), "let v21 = 21;");
}
