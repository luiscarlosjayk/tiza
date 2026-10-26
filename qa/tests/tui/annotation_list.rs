//! The annotation list (`L`) was a block of text: one line per annotation
//! that wrapped into the next, no columns and nothing to act on.
//!
//! Acceptance criteria (option A «Marco», crates/tiza-tui/UI-GUIDELINES.md):
//! - AC1 Grouped by step (with its title), and inside a step by file when
//!   the document shows more than one.
//! - AC2 Aligned columns: id, type, lines, label (cut with … in its own
//!   column, never wrapped).
//! - AC3 A selected row: j/k move it, Enter goes to that step and selects
//!   the annotation as Tab does, `x` deletes it, `u` undoes and `U` or
//!   Ctrl-R redo with the list open, `/` filters.
//! - AC4 Moves in their own section, with columns too.
//! - AC5 The panel of the help: title, Esc chip, footer with the keys.
//! - The agent review is the same panel, and it no longer opens by
//!   itself: the bar and presentation mode show `⏸ N to review · :review`.
//!
//! Corner cases: an unanchored annotation and a stale move have their own
//! mark; 200 annotations scroll with the selected row in view; a label
//! with wide characters keeps the columns aligned.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};
use serde_json::json;
use tiza_qa::Live;
use tiza_tui::app::Overlay;

/// `examples/stack-and-queue.tiza` with its code, in a folder of its own.
fn stack_and_queue(name: &str) -> Live {
    let dir = tiza_qa::scratch(name);
    let examples = tiza_qa::example("");
    std::fs::copy(
        examples.join("stack-and-queue.tiza"),
        dir.join("stack-and-queue.tiza"),
    )
    .unwrap();
    copy_dir(&examples.join("src"), &dir.join("src"));
    let doc = tiza_core::fs::load_document(&dir.join("stack-and-queue.tiza")).unwrap();
    let live = Live::open(doc, dir.join("stack-and-queue.tiza"), dir.join("s.sock"));
    live.with(|a| a.autosave = false);
    live
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_file() {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

fn open_list(live: &Live, cols: u16, rows: u16) -> String {
    live.with(move |a| a.size = (cols, rows));
    let _ = live.screen(cols, rows);
    live.keys(&[KeyCode::Char('L')]);
    live.screen(cols, rows)
}

/// The rows of the panel on `screen`, drawn at `cols×rows`: the text
/// between its side borders, without the frame rows.
fn panel_rows(screen: &str, cols: u16, rows: u16) -> Vec<String> {
    let r = tiza_tui::panel::rect(ratatui::layout::Rect::new(0, 0, cols, rows)).unwrap();
    screen
        .lines()
        .skip(r.y as usize + 1)
        .take(r.height as usize - 2)
        .map(|l| {
            let cells: String = l
                .chars()
                .skip(r.x as usize + 2)
                .take(r.width as usize - 4)
                .collect();
            cells.trim_end().to_string()
        })
        .collect()
}

#[test]
fn steps_files_columns_and_moves() {
    let live = stack_and_queue("groups");
    live.command("step 2");
    let s = open_list(&live, 120, 46);
    assert!(s.contains("╭─ Annotations · 7 · Moves · 6 "), "{s}");
    assert!(s.contains(" esc close ─╮"), "{s}");
    assert!(
        s.contains("╰─ j/k select · enter go · x delete · u undo · U redo "),
        "{s}"
    );
    // It opens at the first row of the step on screen: a3 at step 2.
    assert!(s.contains(" 3 of 13 ─╯"), "{s}");
    let rows = panel_rows(&s, 120, 46);
    let at = |text: &str| {
        rows.iter()
            .position(|r| r.starts_with(text))
            .unwrap_or_else(|| panic!("no row «{text}»:\n{}", rows.join("\n")))
    };
    assert_eq!(rows[0], "/ type to filter annotations and moves");
    // Steps in order, the one on screen marked, a step with nothing said.
    let step1 = at("Step 1");
    let step2 = at("Step 2 · current");
    let step3 = at("Step 3");
    assert!(step1 < step2 && step2 < step3);
    assert_eq!(rows[step3 + 1], "  no annotations");
    // Files under their step: queue.rs at step 2.
    assert_eq!(rows[step2 + 1], "  src/queue.rs");
    // Aligned columns: the label of every annotation starts at one column.
    let labels: Vec<usize> = rows
        .iter()
        .filter(|r| r.len() > 4 && (r.starts_with("▸ a") || r.starts_with("  a")))
        .map(|r| {
            let cells: Vec<char> = r.chars().collect();
            // id (4), type, lines: the label starts after the third run of
            // two or more blanks.
            let mut runs = 0;
            let mut i = 2;
            while i + 1 < cells.len() {
                if cells[i] == ' ' && cells[i + 1] == ' ' {
                    while i < cells.len() && cells[i] == ' ' {
                        i += 1;
                    }
                    runs += 1;
                    if runs == 3 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            i
        })
        .collect();
    assert_eq!(labels.len(), 7, "{}", rows.join("\n"));
    assert!(
        labels.iter().all(|l| *l == labels[0]),
        "{labels:?}\n{}",
        rows.join("\n")
    );
    // Moves in their own section, with columns.
    let moves = at("Moves");
    assert!(
        rows[moves + 1].starts_with("  #1  step 2  code.link"),
        "{}",
        rows[moves + 1]
    );
    assert_eq!(rows.iter().filter(|r| r.starts_with("  #")).count(), 6);
}

#[test]
fn a_long_label_is_cut_in_its_column_never_wrapped() {
    let live = stack_and_queue("cut");
    let s = open_list(&live, 80, 46);
    let rows = panel_rows(&s, 80, 46);
    let a6 = rows.iter().find(|r| r.contains("a6  note")).unwrap();
    assert!(a6.ends_with('…'), "{a6}");
    // The row after it is not the rest of its label.
    let i = rows.iter().position(|r| r.contains("a6  note")).unwrap();
    assert!(
        rows[i + 1].is_empty() || rows[i + 1].starts_with("Step"),
        "{}",
        rows[i + 1]
    );
}

#[test]
fn j_k_and_enter_go_to_the_step_with_the_annotation_selected() {
    let live = stack_and_queue("enter");
    open_list(&live, 120, 46);
    live.keys(&[KeyCode::Char('j'), KeyCode::Char('j'), KeyCode::Char('j')]);
    let s = live.screen(120, 46);
    assert!(s.contains(" 4 of 13 ─╯"), "{s}");
    assert!(
        panel_rows(&s, 120, 46)
            .iter()
            .any(|r| r.starts_with("▸ a4  note")),
        "{s}"
    );
    live.keys(&[KeyCode::Enter]);
    assert_eq!(
        live.with(|a| (a.overlay, a.doc.step, a.selected.clone())),
        (Overlay::None, 2, Some("a4".to_string()))
    );
    // On a move, Enter goes to its step.
    live.keys(&[KeyCode::Char('L'), KeyCode::Char('G')]);
    live.keys(&[KeyCode::Enter]);
    assert_eq!(live.with(|a| a.doc.step), 5);
}

#[test]
fn x_deletes_then_u_undoes_and_u_or_ctrl_r_redo_with_the_list_open() {
    let live = stack_and_queue("delete");
    open_list(&live, 120, 46);
    live.keys(&[KeyCode::Char('j'), KeyCode::Char('x')]);
    let count = || live.with(|a| a.doc.annotations.len());
    assert_eq!(count(), 6);
    assert_eq!(live.with(|a| a.overlay), Overlay::List);
    assert_eq!(live.message().unwrap().0, "a2 deleted (u to undo)");
    let s = live.screen(120, 46);
    assert!(s.contains("╭─ Annotations · 6 · Moves · 6 "), "{s}");
    // The selection stays at the same place: on the row that came up.
    assert!(
        panel_rows(&s, 120, 46)
            .iter()
            .any(|r| r.starts_with("▸ a3  box")),
        "{s}"
    );
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!((count(), live.with(|a| a.overlay)), (7, Overlay::List));
    live.keys(&[KeyCode::Char('U')]);
    assert_eq!(count(), 6);
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(count(), 7);
    live.with(|a| tiza_tui::on_key(a, KeyCode::Char('r'), KeyModifiers::CONTROL));
    assert_eq!((count(), live.with(|a| a.overlay)), (6, Overlay::List));
    // A move goes too.
    live.keys(&[KeyCode::Char('G'), KeyCode::Char('x')]);
    assert_eq!(live.with(|a| a.doc.moves.len()), 5);
    assert!(
        live.message()
            .unwrap()
            .0
            .starts_with("#6 deleted (u to undo)")
    );
}

#[test]
fn the_filter_keeps_the_queue_rows_then_esc_clears_then_closes() {
    let live = stack_and_queue("filter");
    open_list(&live, 120, 46);
    live.keys(&"/queue".chars().map(KeyCode::Char).collect::<Vec<_>>());
    let s = live.screen(120, 46);
    assert!(s.contains("│ / queue▏"), "{s}");
    assert!(s.contains(" esc clear ─╮"), "{s}");
    assert!(s.contains(" matches ─╯"), "{s}");
    let rows = panel_rows(&s, 120, 46);
    assert!(!rows.iter().any(|r| r.contains("a1  box")), "{s}");
    assert!(rows.iter().any(|r| r.contains("a3  box")), "{s}");
    assert!(
        rows.iter().any(|r| r.contains("#1  step 2  code.link")),
        "{s}"
    );
    assert!(!rows.iter().any(|r| r.contains("#2 ")), "{s}");
    // ↓ moves the selection while typing; Enter keeps the filter.
    live.keys(&[KeyCode::Down, KeyCode::Enter]);
    assert_eq!(
        live.with(|a| (a.overlay_filter.clone(), a.overlay_typing)),
        ("queue".into(), false)
    );
    live.keys(&[KeyCode::Esc]);
    assert_eq!(
        live.with(|a| (a.overlay, a.overlay_filter.clone())),
        (Overlay::List, String::new())
    );
    live.keys(&[KeyCode::Esc]);
    assert_eq!(live.with(|a| a.overlay), Overlay::None);
}

#[test]
fn corner_200_annotations_scroll_with_the_selected_row_in_view() {
    let live = Live::from_code(&"let x = 1;\n".repeat(20), "rs", "many");
    for i in 0..200 {
        live.command(&format!("note {} n{i}", i % 20 + 1));
    }
    open_list(&live, 80, 24);
    live.keys(&[KeyCode::Char('G')]);
    let s = live.screen(80, 24);
    assert!(s.contains(" 200 of 200 ─╯"), "{s}");
    assert!(
        panel_rows(&s, 80, 24)
            .iter()
            .any(|r| r.starts_with("▸ a200  note")),
        "{s}"
    );
    for _ in 0..120 {
        live.keys(&[KeyCode::Char('k')]);
    }
    let s = live.screen(80, 24);
    assert!(s.contains(" 80 of 200 ─╯"), "{s}");
    assert!(
        panel_rows(&s, 80, 24)
            .iter()
            .any(|r| r.starts_with("▸ a80   note")),
        "{s}"
    );
}

#[test]
fn a_label_with_wide_characters_keeps_the_frame_and_the_columns() {
    let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", "wide");
    live.command("note 2 名前名前名前名前名前名前名前名前名前名前名前名前名前名前名前名前名前名前");
    live.command("note 1 ascii");
    open_list(&live, 80, 24);
    let panel = tiza_tui::panel::rect(ratatui::layout::Rect::new(0, 0, 80, 24)).unwrap();
    // The cell at the right edge of each row of the panel is its frame: a
    // wide character never pushes it out.
    let edges = live.with(move |a| {
        use ratatui::{Terminal, backend::TestBackend};
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
        let buf = t.backend().buffer().clone();
        let right = panel.x + panel.width - 1;
        (panel.y..panel.y + panel.height)
            .map(|y| buf[(right, y)].symbol().to_string())
            .collect::<Vec<_>>()
    });
    for e in &edges {
        assert!(["│", "╮", "╯"].contains(&e.as_str()), "{edges:?}");
    }
    let rows = panel_rows(&live.screen(80, 24), 80, 24);
    let wide = rows
        .iter()
        .find(|r| r.contains("note") && r.contains('名'))
        .unwrap();
    assert!(wide.ends_with('…'), "{wide}");
    // The label starts at the same column as the label of an ASCII row:
    // everything before it is ASCII, so characters count cells.
    let ascii = rows.iter().find(|r| r.contains("ascii")).unwrap();
    let start = ascii.find("ascii").unwrap();
    assert_eq!(wide.chars().nth(start), Some('名'), "{wide}\n{ascii}");
}

#[test]
fn an_unanchored_annotation_and_a_stale_move_have_their_mark() {
    let dir = tiza_qa::scratch("marks");
    let src = dir.join("main.rs");
    std::fs::write(&src, "fn main() {\n    let x = 1;\n    let y = 2;\n}\n").unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [
            {"id": "a1", "step": 1, "type": "note", "line": 3, "text": "y here"}
        ],
        "moves": [
            {"at_step": 2, "action": "code.patch", "op": "delete", "find": "let y = 2;"},
            {"at_step": 3, "action": "code.patch", "op": "replace",
             "find": "let x = 1;", "content": "let x = 3;"}
        ]
    });
    let tiza = dir.join("doc.tiza");
    std::fs::write(&tiza, doc.to_string()).unwrap();
    let loaded = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(loaded, tiza, dir.join("s.sock"));
    live.with(|a| a.autosave = false);
    live.command("step 2");
    // The code changes on disk under move #2: it goes stale.
    live.command("follow on");
    std::fs::write(&src, "fn main() {\n    let x = 9;\n    let y = 2;\n}\n").unwrap();
    // A save at a step with moves waits for step 0.
    let start = Instant::now();
    while live.api_ok("follow.status", json!({}))["reason"].is_null() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the save was not seen"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    live.command("step 0");
    let start = Instant::now();
    while !live.with(|a| a.doc.moves.iter().any(|m| m.stale.is_some())) {
        assert!(start.elapsed() < Duration::from_secs(10), "no stale move");
        std::thread::sleep(Duration::from_millis(20));
    }
    live.command("follow off");
    live.command("step 2");
    let rows = panel_rows(&open_list(&live, 100, 30), 100, 30);
    let a1 = rows.iter().find(|r| r.contains("a1  note")).unwrap();
    assert!(a1.ends_with("! unanchored"), "{rows:?}");
    let stale = rows.iter().find(|r| r.contains("#2 ")).unwrap();
    assert!(stale.ends_with("! stale"), "{rows:?}");
    // Selected, the stale move says why on the bar.
    live.keys(&[KeyCode::Char('G')]);
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.starts_with("#2 is stale: "), "{msg}");
    assert!(msg.ends_with(". x deletes it"), "{msg}");
}

#[test]
fn below_40x12_the_list_does_not_open() {
    let live = Live::from_code("fn main() {}\n", "rs", "small");
    live.with(|a| a.size = (39, 11));
    live.keys(&[KeyCode::Char('L')]);
    assert_eq!(live.with(|a| a.overlay), Overlay::None);
    assert_eq!(live.message().unwrap().0, "needs 40×12");
}

/// A supervised instance with two agent changes waiting, each from a
/// thread of its own (the agent waits for the decision).
fn two_waiting(name: &str) -> (Live, Vec<std::thread::JoinHandle<serde_json::Value>>) {
    let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", name);
    live.command("supervise on");
    let mut agents = Vec::new();
    for (line, text) in [(1, "first"), (2, "second")] {
        let socket: PathBuf = live.socket.clone();
        agents.push(std::thread::spawn(move || {
            let r = tiza_tui::ipc::call(
                &socket,
                "annotation.add",
                json!({"type": "note", "line": line, "text": text}),
            )
            .unwrap();
            serde_json::to_value(&r).unwrap()
        }));
        let want = agents.len();
        let start = Instant::now();
        while live.with(|a| a.proposals.len()) < want {
            assert!(start.elapsed() < Duration::from_secs(10), "no proposal");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    (live, agents)
}

#[test]
fn does_not_open_by_itself_and_the_chip_says_review() {
    let (live, agents) = two_waiting("chip");
    assert_eq!(live.with(|a| a.overlay), Overlay::None);
    let bar = live.screen(120, 30).lines().last().unwrap().to_string();
    assert!(bar.contains("⏸ 2 to review · :review"), "{bar}");
    live.keys(&[KeyCode::Char('P')]);
    let last = live.screen(120, 30).lines().last().unwrap().to_string();
    assert!(last.starts_with(" ⏸ 2 to review · :review"), "{last}");
    // Leaving presentation mode does not open it either.
    live.keys(&[KeyCode::Char('P')]);
    assert_eq!(live.with(|a| a.overlay), Overlay::None);
    live.command("review");
    live.keys(&[KeyCode::Char('d')]);
    for a in agents {
        a.join().unwrap();
    }
}

#[test]
fn is_the_panel_and_y_accepts_the_selected_change() {
    let (live, agents) = two_waiting("review");
    live.with(|a| a.size = (100, 30));
    live.command("review");
    let s = live.screen(100, 30);
    assert!(s.contains("╭─ Agent changes · 2 waiting "), "{s}");
    assert!(s.contains(" esc hide ─╮"), "{s}");
    assert!(
        s.contains("╰─ y accept · n reject · a accept all · d reject all · j/k select "),
        "{s}"
    );
    assert!(s.contains(" 1 of 2 ─╯"), "{s}");
    let rows = panel_rows(&s, 100, 30);
    assert!(rows.iter().any(|r| r.starts_with("▸ p1  ")), "{rows:?}");
    assert!(
        rows.iter()
            .any(|r| r == "Tiza rejects an agent change 60 s after it arrives."),
        "{rows:?}"
    );
    // The second one, out of turn.
    live.keys(&[KeyCode::Char('j'), KeyCode::Char('y')]);
    assert!(
        live.message().unwrap().0.starts_with("accepted p2: "),
        "{:?}",
        live.message()
    );
    assert_eq!(
        live.with(|a| a.proposals.iter().map(|p| p.id).collect::<Vec<_>>()),
        [1]
    );
    // Esc hides it; the change still waits.
    live.keys(&[KeyCode::Esc]);
    assert_eq!(
        live.with(|a| (a.overlay, a.proposals.len())),
        (Overlay::None, 1)
    );
    live.command("review");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(
        live.with(|a| (a.overlay, a.proposals.len())),
        (Overlay::None, 0)
    );
    let replies: Vec<serde_json::Value> = agents.into_iter().map(|a| a.join().unwrap()).collect();
    assert!(replies[1]["error"].is_null(), "{replies:?}");
    assert_eq!(replies[0]["error"]["code"], "rejected", "{replies:?}");
}

#[test]
fn the_list_opens_at_the_step_on_screen_and_enter_on_nothing_closes() {
    let live = stack_and_queue("start");
    live.command("step 4");
    let s = open_list(&live, 120, 46);
    assert!(
        panel_rows(&s, 120, 46)
            .iter()
            .any(|r| r.starts_with("▸ a6  note")),
        "{s}"
    );
    live.keys(&[KeyCode::Esc]);
    // No row: Enter closes, as on the help.
    let empty = Live::from_code("fn main() {}\n", "rs", "enter-empty");
    open_list(&empty, 80, 30);
    empty.keys(&[KeyCode::Enter]);
    assert_eq!(empty.with(|a| a.overlay), Overlay::None);
}

#[test]
fn the_selection_stays_on_its_row_when_an_agent_adds_one_above() {
    let live = stack_and_queue("identity");
    live.command("step 2");
    open_list(&live, 120, 46);
    // ▸ a3 at step 2; an agent adds a note at step 1, above it.
    assert!(
        panel_rows(&live.screen(120, 46), 120, 46)
            .iter()
            .any(|r| r.starts_with("▸ a3  box"))
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 9, "text": "agent", "step": 1}),
    );
    let s = live.screen(120, 46);
    assert!(
        panel_rows(&s, 120, 46)
            .iter()
            .any(|r| r.starts_with("▸ a3  box")),
        "{s}"
    );
    live.keys(&[KeyCode::Char('x')]);
    assert_eq!(live.message().unwrap().0, "a3 deleted (u to undo)");
}

#[test]
fn ctrl_with_a_letter_does_not_act_in_the_list() {
    let live = stack_and_queue("ctrl");
    open_list(&live, 120, 46);
    for c in ['x', 'd', 'u', 'j'] {
        live.with(move |a| tiza_tui::on_key(a, KeyCode::Char(c), KeyModifiers::CONTROL));
    }
    assert_eq!(
        live.with(|a| (a.overlay, a.doc.annotations.len(), a.overlay_sel)),
        (Overlay::List, 7, 0)
    );
    // A plain `r` is no redo either.
    live.keys(&[KeyCode::Char('x'), KeyCode::Char('u'), KeyCode::Char('r')]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 7);
}

#[test]
fn the_step_on_screen_stays_after_its_last_annotation_goes() {
    let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", "last-step");
    live.command("note 2 first");
    live.command("step 2");
    live.command("note 1 second");
    let s = open_list(&live, 100, 30);
    assert!(
        panel_rows(&s, 100, 30)
            .iter()
            .any(|r| r.starts_with("▸ a2  note")),
        "{s}"
    );
    live.keys(&[KeyCode::Char('x')]);
    let rows = panel_rows(&live.screen(100, 30), 100, 30);
    let at = rows
        .iter()
        .position(|r| r == "Step 2 · current")
        .unwrap_or_else(|| panic!("{rows:?}"));
    assert_eq!(rows[at + 1], "  no annotations", "{rows:?}");
}

#[test]
fn a_narrow_list_stacks_its_rows_inside_the_frame() {
    let live = stack_and_queue("narrow");
    open_list(&live, 44, 30);
    let panel = tiza_tui::panel::rect(ratatui::layout::Rect::new(0, 0, 44, 30)).unwrap();
    let edges = live.with(move |a| {
        use ratatui::{Terminal, backend::TestBackend};
        let mut t = Terminal::new(TestBackend::new(44, 30)).unwrap();
        t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
        let buf = t.backend().buffer().clone();
        let right = panel.x + panel.width - 1;
        (panel.y..panel.y + panel.height)
            .map(|y| buf[(right, y)].symbol().to_string())
            .collect::<Vec<_>>()
    });
    assert!(
        edges.iter().all(|e| ["│", "╮", "╯"].contains(&e.as_str())),
        "{edges:?}"
    );
    let rows = panel_rows(&live.screen(44, 30), 44, 30);
    let i = rows
        .iter()
        .position(|r| r.starts_with("▸ a1  box"))
        .unwrap();
    assert!(rows[i + 1].starts_with("      LIFO"), "{rows:?}");
}

#[test]
fn a_filter_decides_only_the_changes_shown() {
    let (live, agents) = two_waiting("review-filter");
    live.with(|a| a.size = (100, 30));
    live.command("review");
    live.keys(&"/second".chars().map(KeyCode::Char).collect::<Vec<_>>());
    live.keys(&[KeyCode::Enter, KeyCode::Char('a')]);
    assert_eq!(live.message().unwrap().0, "accepted 1 agent change");
    assert_eq!(
        live.with(|a| a.proposals.iter().map(|p| p.id).collect::<Vec<_>>()),
        [1]
    );
    live.keys(&[KeyCode::Esc, KeyCode::Char('G'), KeyCode::Char('d')]);
    let replies: Vec<serde_json::Value> = agents.into_iter().map(|a| a.join().unwrap()).collect();
    assert!(replies[1]["error"].is_null(), "{replies:?}");
    assert_eq!(replies[0]["error"]["code"], "rejected", "{replies:?}");
}

#[test]
fn an_id_too_long_for_its_column_is_cut_and_keeps_its_gap() {
    let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", "long-id");
    live.api_ok(
        "annotation.add",
        json!({"id": "push-and-pop-long", "type": "note", "line": 2, "text": "long id"}),
    );
    let rows = panel_rows(&open_list(&live, 100, 30), 100, 30);
    let row = rows.iter().find(|r| r.contains("long id")).unwrap();
    // The id column is 12 cells at most: 10 for the text, 2 blank.
    assert!(row.starts_with("▸ push-and-…  note"), "{row}");
}

#[test]
fn bar_the_tab_hint_keeps_its_keys_and_the_id_after_enter() {
    let live = stack_and_queue("tab-hint");
    live.command("step 2");
    open_list(&live, 80, 24);
    live.keys(&[KeyCode::Char('j'), KeyCode::Enter]);
    assert_eq!(live.with(|a| a.selected.clone()), Some("a4".to_string()));
    // 80 columns: the label and what the annotation is give way, the keys
    // stay from the left (nothing is cut at this step: no warning), and
    // `autosave off` with no changes gives way to them.
    let b80 = live.screen(80, 24).lines().last().unwrap().to_string();
    assert!(!b80.contains("  ! "), "{b80}");
    assert!(!b80.contains("autosave off"), "{b80}");
    assert!(
        b80.ends_with("▸ a4 · enter label · e set · x delete · c color · +/- step"),
        "{b80}"
    );
    // 120 columns: what it is and every key.
    let b120 = live.screen(120, 40).lines().last().unwrap().to_string();
    assert!(
        b120.ends_with(
            "▸ a4 [step 2] note L13 · enter label · e set · x delete · c color · +/- step"
        ),
        "{b120}"
    );
}
