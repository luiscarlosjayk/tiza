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
