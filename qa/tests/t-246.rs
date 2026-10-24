//! `! small terminal` showed on every terminal under 100 columns, also when
//! nothing on the step was cut, so 80 columns (a common width) always
//! warned.
//!
//! Acceptance criteria:
//! - AC1 The warning shows only when the current step really lacks room: a
//!   label or note cut with … or not drawn, or the code column that does not
//!   fit. At 80 columns with nothing cut, no warning.
//! - AC2 Luis chose the words from real captures: `! 3 labels cut`, and
//!   `! code needs 64 cols` for the code (64: what the code needs).
//!   `status.warnings` says the same with the details.
//! - AC3 It follows the step and the size; it reads the layout of the frame
//!   (no second layout).
//! - AC4 The tests and snapshots that read the bar are updated.
//!
//! Corner cases: presentation mode shows no bar; labels under the lines
//! are a fact in `status.warnings`, with no badge.

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, scratch};

/// A copy of `examples/ownership.rs.tiza` as it was when Luis chose the
/// words (labels cut at 100 columns, code of 64 columns), with its code, in
/// a folder of its own: the example itself may change.
fn ownership(name: &str) -> Live {
    let dir = scratch(name);
    for f in ["ownership.rs.tiza", "ownership.rs"] {
        std::fs::copy(tiza_qa::fixture("t-246").join(f), dir.join(f)).unwrap();
    }
    let doc = tiza_core::fs::load_document(&dir.join("ownership.rs.tiza")).unwrap();
    let live = Live::open(doc, dir.join("ownership.rs.tiza"), dir.join("s.sock"));
    live.with(|a| a.autosave = false);
    live
}

fn bar(live: &Live, cols: u16, rows: u16) -> String {
    live.screen(cols, rows).lines().last().unwrap().to_string()
}

fn warnings(live: &Live) -> Vec<String> {
    match live.api_ok("status", json!({})).get("warnings") {
        Some(Value::Array(w)) => w.iter().map(|w| w.as_str().unwrap().to_string()).collect(),
        _ => Vec::new(),
    }
}

#[test]
fn step1_ac1_at_80_columns_with_nothing_cut_there_is_no_warning() {
    let live = ownership("t246-step1");
    live.command("step 1");
    let b = bar(&live, 80, 24);
    assert!(!b.contains("  ! "), "{b}");
    assert!(!b.contains("small terminal"), "{b}");
    assert!(
        warnings(&live).iter().all(|w| !w.contains(" cut at ")),
        "{:?}",
        warnings(&live)
    );
}

#[test]
fn step2_ac2_step_7_names_what_is_cut_and_the_code_it_needs() {
    let live = ownership("t246-step7");
    live.command("step 7");
    // 100 columns: three labels end in `…`.
    let b = bar(&live, 100, 30);
    assert!(b.contains("  ! 3 labels cut"), "{b}");
    let w = warnings(&live);
    println!("WARNINGS at 100: {w:?}");
    let cut = w
        .iter()
        .find(|w| w.starts_with("3 labels cut at 100 columns: "))
        .unwrap_or_else(|| panic!("{w:?}"));
    // Each id it names is an annotation of the document.
    let ids: Vec<String> = live.with(|a| a.doc.annotations.iter().map(|x| x.id.clone()).collect());
    for id in cut["3 labels cut at 100 columns: ".len()..].split(", ") {
        let id = id.trim_end_matches(" (not drawn)");
        assert!(ids.iter().any(|x| x == id), "{id} in {cut}");
    }
    // 80 columns: the code column does not fit; that comes first.
    let b = bar(&live, 80, 24);
    assert!(b.contains("  ! code needs 64 cols"), "{b}");
    assert!(
        warnings(&live)
            .iter()
            .any(|w| w == "code needs 64 columns at 80 columns"),
        "{:?}",
        warnings(&live)
    );
}

#[test]
fn ac3_it_follows_the_step_and_the_size() {
    let live = ownership("t246-follow");
    live.command("step 7");
    assert!(bar(&live, 100, 30).contains("! 3 labels cut"));
    live.command("step 1");
    assert!(!bar(&live, 100, 30).contains("  ! "));
    live.command("step 7");
    // Wider, the labels fit: the warning goes.
    let wide = (100..=240u16)
        .find(|c| {
            live.screen(*c, 30);
            warnings(&live).iter().all(|w| !w.contains(" cut at "))
        })
        .expect("some width shows every label whole");
    assert!(!bar(&live, wide, 30).contains("  ! "), "{wide}");
    assert!(bar(&live, wide - 1, 30).contains("label"), "{}", wide - 1);
}

#[test]
fn corner_presentation_mode_has_no_bar_and_the_warning_is_back_after() {
    let live = ownership("t246-present");
    live.command("step 7");
    live.keys(&[KeyCode::Char('P')]);
    let s = live.screen(100, 30);
    assert!(!s.contains("labels cut") && !s.contains("NORMAL"), "{s}");
    live.keys(&[KeyCode::Char('P')]);
    assert!(bar(&live, 100, 30).contains("! 3 labels cut"));
}

#[test]
fn corner_labels_under_the_lines_are_a_fact_without_a_badge() {
    let live = ownership("t246-under");
    live.command("step 1");
    let b = bar(&live, 60, 30);
    let w = warnings(&live);
    assert!(
        w.iter()
            .any(|w| w == "labels go under the lines at 60 columns"),
        "{w:?}"
    );
    assert!(!b.contains("under the lines"), "{b}");
    // A document with nothing to draw says nothing about labels.
    let plain = Live::from_code("fn main() {}\n", "rs", "t246-plain");
    plain.screen(40, 20);
    assert!(warnings(&plain).is_empty(), "{:?}", warnings(&plain));
}

#[test]
fn ac2_the_readme_the_guide_and_the_changelog_say_it() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |f: &str| std::fs::read_to_string(root.join(f)).unwrap();
    for f in ["README.md", "CHANGELOG.md"] {
        let t = read(f);
        assert!(
            t.contains("! 3 labels cut") && t.contains("! code needs 64 cols"),
            "{f}"
        );
    }
    assert!(!read("README.md").contains("! small terminal"));
    assert!(read("docs/agent-guide.md").contains("code needs N columns at C columns"));
}
