//! `! small terminal` showed on every terminal under 100 columns, also when
//! nothing on the step was cut, so 80 columns (a common width) always
//! warned.
//!
//! Acceptance criteria:
//! - AC1 The warning shows only when the current step really lacks room: a
//!   label or note cut with … or not drawn, or the code column that does not
//!   fit. At 80 columns with nothing cut, no warning.
//! - AC2 Luis chose the words from real captures: `! 3 labels cut`, and
//!   `! code needs 83 cols` for the code (83: the terminal width from
//!   which the step shows every code line, not the width of the code).
//!   `status.warnings` says the same with the details.
//! - AC3 It follows the step and the size; it reads the layout of the frame
//!   (no second layout).
//! - AC4 The tests and snapshots that read the bar are updated.
//!
//! Corner cases: presentation mode shows no bar; labels under the lines
//! are a fact in `status.warnings`, with no badge.

use std::time::{Duration, Instant};

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
    assert!(b.contains("  ! code needs 83 cols"), "{b}");
    assert!(
        warnings(&live)
            .iter()
            .any(|w| w == "code needs 83 columns at 80 columns"),
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

/// `fn main` with a line of `n` characters in a string: 16 more columns of
/// code.
fn wide(n: usize) -> String {
    format!("fn main() {{\n    let s = \"{}\";\n}}\n", "x".repeat(n))
}

#[test]
fn corner_after_a_follow_reload_the_time_gives_way_and_the_badge_is_whole_or_its_sign() {
    // As in Zed's terminal docked on the right (68 columns): code that needs
    // more columns, followed, after one reload.
    let dir = scratch("t246-follow-reload");
    let src = std::path::absolute(dir.join("lib.rs")).unwrap();
    std::fs::write(&src, wide(70)).unwrap();
    let mut doc = tiza_core::Document::from_code(&wide(70), "rs", Some(src.display().to_string()));
    doc.reference_file(src.display().to_string(), None);
    let live = Live::open(doc, dir.join("lib.rs.tiza"), dir.join("live.sock"));
    live.command("follow on");
    let tmp = src.with_extension("rs.tmp");
    std::fs::write(&tmp, wide(71)).unwrap();
    std::fs::rename(&tmp, &src).unwrap();
    let start = Instant::now();
    while live.api_ok("follow.status", json!({}))["reloads"].as_u64() < Some(1) {
        assert!(start.elapsed() < Duration::from_secs(10), "no reload");
        std::thread::sleep(Duration::from_millis(20));
    }
    live.with(|a| a.message = None);
    let needs = live.with(|a| a.doc.code.iter().map(|l| l.chars().count()).max().unwrap());
    assert!(needs > 80, "{needs}");
    let whole = |b: &str| {
        let at = b.find("  ! code needs ")?;
        b[at..].contains(" cols").then_some(())
    };
    for cols in [72u16, 68, 64, 60, 50, 40] {
        let b = bar(&live, cols, 30);
        assert!(b.contains("⟳ following"), "{cols}: {b}");
        // The badge is whole, or its sign alone: never cut in a word.
        let sign = !b.contains("! code") && b.trim_end().ends_with("  !") || b.contains("  !  ");
        assert!(whole(&b).is_some() || sign, "{cols}: {b}");
        // `· last HH:MM:SSZ` goes before the badge does.
        if b.contains("· last ") {
            assert!(whole(&b).is_some(), "{cols}: {b}");
        }
    }
    // Wide enough, the time of the reload shows; narrower, it gives way
    // and the badge stays whole; narrower still, the badge keeps its sign.
    assert!(bar(&live, 72, 30).contains("⟳ following · last "));
    let b68 = bar(&live, 68, 30);
    assert!(!b68.contains("· last") && whole(&b68).is_some(), "{b68}");
    let b40 = bar(&live, 40, 30);
    assert!(whole(&b40).is_none() && b40.contains("  !"), "{b40}");
}

/// The `N` of `! code needs N cols` on the bar, if any.
fn code_needs(bar: &str) -> Option<u16> {
    let rest = &bar[bar.find("! code needs ")? + "! code needs ".len()..];
    rest.split(' ').next()?.parse().ok()
}

#[test]
fn ac2_code_needs_names_the_first_terminal_width_with_no_warning() {
    // N is the terminal width from which the step shows every code line
    // whole: at N there is no warning, at N - 1 it names N. Line numbers,
    // arrow lanes, box rails and the side padding count, not the code alone.
    let open = |name: &str, file: std::path::PathBuf| {
        let dir = scratch(name);
        for f in ["ownership.rs.tiza", "ownership.rs"] {
            std::fs::copy(file.join(f), dir.join(f)).unwrap();
        }
        let doc = tiza_core::fs::load_document(&dir.join("ownership.rs.tiza")).unwrap();
        let live = Live::open(doc, dir.join("ownership.rs.tiza"), dir.join("s.sock"));
        live.with(|a| a.autosave = false);
        live
    };
    let example = || tiza_qa::fixture("shared/ownership");
    let old = || tiza_qa::fixture("t-246");
    let wide = || {
        let code = std::fs::read_to_string(tiza_qa::fixture("t-173/wide.rs")).unwrap();
        Live::from_code(&code, "rs", "t246-needs-wide")
    };
    let cases: Vec<(&str, Live, &str)> = vec![
        (
            "example, step 7",
            open("t246-needs-ex7", example()),
            "step 7",
        ),
        (
            "example, step 1",
            open("t246-needs-ex1", example()),
            "step 1",
        ),
        (
            "old example, step 7",
            open("t246-needs-old7", old()),
            "step 7",
        ),
        (
            "old example, step 3",
            open("t246-needs-old3", old()),
            "step 3",
        ),
        ("padding 1 6", open("t246-needs-pad", old()), "pad 1 6"),
        (
            "layout stable",
            open("t246-needs-stable", old()),
            "layout stable",
        ),
        ("no line numbers", wide(), "numbers off"),
    ];
    for (what, live, command) in cases {
        // A command that fails, or a step that the document does not have
        // (`:step` one past the last opens an empty step without an error),
        // must not pass as another case.
        let run = |c: &str| {
            live.command(c);
            if let Some((m, true)) = live.message() {
                panic!("{what}: `{c}` failed: {m}");
            }
            if let Some(n) = c.strip_prefix("step ") {
                let n: u32 = n.parse().unwrap();
                let (at, last) = live.with(|a| (a.doc.step, a.doc.max_step()));
                assert!(
                    at == n && n <= last,
                    "{what}: `{c}` is at step {at} of {last}"
                );
            }
        };
        if command == "pad 1 6" || command == "layout stable" {
            run("step 7");
        }
        run(command);
        live.with(|a| a.message = None);
        let n = code_needs(&bar(&live, 60, 30)).unwrap_or_else(|| panic!("{what}: no badge at 60"));
        println!("{what}: code needs {n} cols");
        let at = bar(&live, n, 30);
        assert!(!at.contains("code needs"), "{what}: at {n}: {at}");
        assert!(
            warnings(&live).iter().all(|w| !w.starts_with("code needs")),
            "{what}: at {n}: {:?}",
            warnings(&live)
        );
        let before = bar(&live, n - 1, 30);
        assert_eq!(
            code_needs(&before),
            Some(n),
            "{what}: at {}: {before}",
            n - 1
        );
        assert!(
            warnings(&live).contains(&format!("code needs {n} columns at {} columns", n - 1)),
            "{what}: {:?}",
            warnings(&live)
        );
    }
}
