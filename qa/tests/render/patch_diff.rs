//! A `code.patch` move with `diff` shows a `+` beside the number of each
//! line that it writes, at its step only. The column of the `+` is there
//! at every step of the document, also with no line numbers, so the code
//! does not move when the `+` comes or goes.

use serde_json::json;
use tiza_qa::{Live, fixture, scratch, tiza_ok};

fn render(step: u32, extra: &[&str]) -> String {
    render_doc(&fixture("render/patch_diff/borrow.tiza"), step, extra)
}

fn render_doc(p: &std::path::Path, step: u32, extra: &[&str]) -> String {
    let s = step.to_string();
    let mut args = vec![
        "render",
        p.to_str().unwrap(),
        "--width",
        "60",
        "--step",
        &s,
        "--format",
        "txt",
        "--out",
        "-",
    ];
    args.extend_from_slice(extra);
    tiza_ok(&args)
}

/// The column of the code: where `fn main()` starts.
fn code_x(text: &str) -> usize {
    text.lines()
        .find_map(|l| l.find("fn main()"))
        .unwrap_or_else(|| panic!("no fn main():\n{text}"))
}

/// The character at column `x` of each row that holds `needle`, in the
/// order of the rows.
fn signs(text: &str, needle: &str, x: usize) -> Vec<char> {
    text.lines()
        .filter(|l| l.contains(needle))
        .map(|l| l.chars().nth(x).unwrap_or(' '))
        .collect()
}

/// The same document with no `diff`.
fn plain() -> std::path::PathBuf {
    let text = std::fs::read_to_string(fixture("render/patch_diff/borrow.tiza")).unwrap();
    let path = scratch("diff-plain").join("plain.tiza");
    let text = text
        .replace(",\n     \"diff\": true", "")
        .replace(", \"diff\": true", "");
    assert!(!text.contains("diff"), "{text}");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn a_diff_marks_the_lines_that_its_step_writes_and_the_code_stays() {
    for extra in [&[][..], &["--no-numbers"][..]] {
        let frames: Vec<String> = (0..=3).map(|s| render(s, extra)).collect();
        let x = code_x(&frames[0]);
        for (s, f) in frames.iter().enumerate() {
            assert_eq!(code_x(f), x, "{extra:?}: the code moved at step {s}:\n{f}");
        }
        // The column of the sign: right of the line numbers, or with no
        // numbers right of the cell of the cursor.
        let row = |f: &str, line: &str| f.lines().find(|l| l.contains(line)).unwrap().to_string();
        let sign_x = row(&frames[1], "show(&s)")
            .find('+')
            .expect("a sign at step 1");
        let gutter = if extra.is_empty() { 3 } else { 1 };
        assert_eq!(sign_x, gutter, "{extra:?}:\n{}", frames[1]);
        // The column takes two cells of its own, and with no numbers also
        // the cell of the cursor, so that the cursor never covers a sign.
        let more = if extra.is_empty() { 2 } else { 3 };
        assert_eq!(
            x,
            code_x(&render_doc(&plain(), 1, extra)) + more,
            "{extra:?}"
        );
        // Step 0: no sign. Step 1: only the line that the replace changed,
        // not the lines that it wrote again.
        assert_eq!(signs(&frames[0], "take(s)", sign_x), [' ']);
        for same in ["fn main()", "String::from", "println!"] {
            assert_eq!(
                signs(&frames[1], same, sign_x),
                [' '],
                "{same}:\n{}",
                frames[1]
            );
        }
        // Step 2: the `+` of step 1 went; the insert has its own.
        assert_eq!(
            signs(&frames[2], "show(&s)", sign_x),
            [' ', '+'],
            "{}",
            frames[2]
        );
        // Step 3: no patch, no sign.
        assert_eq!(
            signs(&frames[3], "show(&s)", sign_x),
            [' ', ' '],
            "{}",
            frames[3]
        );
    }
}

/// In normal mode (no take), the code of the TUI stays too: every step
/// keeps the column, also a step that shows no `+`.
#[test]
fn the_code_of_the_tui_stays_when_the_sign_comes() {
    let dir = scratch("diff-tui");
    let doc = tiza_core::fs::open(fixture("render/patch_diff/borrow.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let mut xs = Vec::new();
    for step in 0..=3 {
        live.api_ok("step.set", json!({"step": step}));
        let screen = live.screen(80, 20);
        xs.push(code_x(&screen));
        let show = screen.lines().find(|l| l.contains("show(&s)"));
        if step == 1 {
            assert!(show.unwrap().contains(" + "), "{screen}");
        }
    }
    assert!(xs.iter().all(|x| *x == xs[0]), "{xs:?}");
}

// ---- the lines that a move with `diff` removes ----

/// A document in a scratch folder with `code`, `annotations` and `moves`.
fn doc(
    name: &str,
    code: &[&str],
    annotations: serde_json::Value,
    moves: serde_json::Value,
) -> std::path::PathBuf {
    let path = scratch(name).join("doc.tiza");
    let d = json!({
        "schema": 2,
        "source": {"language": "rust", "first_line": 1},
        "code": code,
        "annotations": annotations,
        "moves": moves,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&d).unwrap()).unwrap();
    path
}

/// The index of the first row that holds `needle`.
fn row_of(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle:?}:\n{text}"))
}

/// The rows that hold the sign `−` of a removed line.
fn removed_rows(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.contains('\u{2212}'))
        .map(str::to_string)
        .collect()
}

#[test]
fn a_removed_line_shows_at_its_step_with_no_number_and_goes_after_it() {
    for extra in [&[][..], &["--no-numbers"][..]] {
        let frames: Vec<String> = (0..=2).map(|s| render(s, extra)).collect();
        let x = code_x(&frames[0]);
        let sign_x = if extra.is_empty() { 3 } else { 1 };
        // Step 1: the line that the replace took away, over the line that
        // it wrote in its place, with a `−` in the column of the sign.
        let take = row_of(&frames[1], "take(s)");
        assert_eq!(take + 1, row_of(&frames[1], "show(&s)"), "{}", frames[1]);
        let row: Vec<char> = frames[1].lines().nth(take).unwrap().chars().collect();
        assert_eq!(row[sign_x], '\u{2212}', "{extra:?}:\n{}", frames[1]);
        // No number: the numbers are the ones of the code at the end of the
        // step, 1 to 5.
        assert!(
            row[..sign_x].iter().all(|c| *c == ' '),
            "{extra:?}:\n{}",
            frames[1]
        );
        if extra.is_empty() {
            let numbers: Vec<&str> = frames[1]
                .lines()
                .filter_map(|l| l.split_whitespace().next())
                .filter(|w| w.parse::<u32>().is_ok())
                .collect();
            assert_eq!(numbers, ["1", "2", "3", "4", "5"], "{}", frames[1]);
        }
        // The text of the removed line starts in the column of the code.
        let at = |row: &str, text: &str| row.find(text).map(|b| row[..b].chars().count());
        assert_eq!(
            at(frames[1].lines().nth(take).unwrap(), "take(s)"),
            Some(x + 4),
            "{}",
            frames[1]
        );
        // Step 0 has the line as code, step 2 has it no more.
        assert!(removed_rows(&frames[0]).is_empty(), "{}", frames[0]);
        assert!(!frames[2].contains("take(s)"), "{}", frames[2]);
        assert!(removed_rows(&frames[2]).is_empty(), "{}", frames[2]);
    }
}

#[test]
fn a_delete_shows_its_line_over_the_line_that_is_now_under_it() {
    let p = doc(
        "diff-delete-render",
        &["a();", "b();", "c();"],
        json!([]),
        json!([{"at_step": 1, "action": "code.patch", "op": "delete", "find": "b();", "diff": true}]),
    );
    let f = render_doc(&p, 1, &[]);
    assert_eq!(removed_rows(&f).len(), 1, "{f}");
    assert_eq!(row_of(&f, "a();") + 1, row_of(&f, "b();"), "{f}");
    assert_eq!(row_of(&f, "b();") + 1, row_of(&f, "c();"), "{f}");
    // c is line 2 now.
    assert!(
        f.lines()
            .nth(row_of(&f, "c();"))
            .unwrap()
            .trim_start()
            .starts_with("2 "),
        "{f}"
    );
}

#[test]
fn the_lines_removed_after_the_last_line_go_under_it() {
    let p = doc(
        "diff-delete-last",
        &["a();", "b();", "c();"],
        json!([]),
        json!([{"at_step": 1, "action": "code.patch", "op": "delete", "find": "c();", "diff": true}]),
    );
    let f = render_doc(&p, 1, &[]);
    assert_eq!(row_of(&f, "b();") + 1, row_of(&f, "c();"), "{f}");
    assert_eq!(
        removed_rows(&f),
        [f.lines().nth(row_of(&f, "c();")).unwrap()],
        "{f}"
    );
}

#[test]
fn a_box_that_starts_on_a_line_holds_the_lines_removed_over_it() {
    let p = doc(
        "diff-box-owner",
        &["a();", "b();", "c();", "d();"],
        json!([{"id": "bx", "step": 1, "type": "box", "from_line": 2, "to_line": 3, "label": "here"}]),
        json!([{"at_step": 1, "action": "code.patch", "op": "delete", "find": "b();", "diff": true}]),
    );
    let f = render_doc(&p, 1, &[]);
    let top = row_of(&f, "here");
    let gone = row_of(&f, "b();");
    // The title of the box, then the removed line, then line 2 (c).
    assert!(top < gone, "{f}");
    assert_eq!(gone + 1, row_of(&f, "c();"), "{f}");
    // The rail of the box goes down beside the removed line.
    let rail = f
        .lines()
        .nth(row_of(&f, "c();"))
        .unwrap()
        .find('│')
        .expect("a rail");
    assert_eq!(
        f.lines().nth(gone).unwrap().chars().nth(rail),
        Some('│'),
        "{f}"
    );
}

#[test]
fn a_bottom_border_does_not_sit_on_a_blank_line_with_removed_lines_over_it() {
    let p = doc(
        "diff-box-blank",
        &["x();", "y();", "q();", "", "z();"],
        json!([{"id": "bx", "step": 1, "type": "box", "from_line": 1, "to_line": 2}]),
        json!([{"at_step": 1, "action": "code.patch", "op": "delete", "find": "q();", "diff": true}]),
    );
    let f = render_doc(&p, 1, &[]);
    // The border closes the box under y, before the removed line q: on
    // the blank line it would hold q inside the box.
    let bottom = row_of(&f, "╰");
    assert_eq!(row_of(&f, "y();") + 1, bottom, "{f}");
    assert!(bottom < row_of(&f, "q();"), "{f}");
    // With no removed line there, the border sits on the blank line: the
    // row under y is the row of the blank line 3.
    let p = doc(
        "diff-box-blank-none",
        &["x();", "y();", "", "z();"],
        json!([{"id": "bx", "step": 1, "type": "box", "from_line": 1, "to_line": 2}]),
        json!([{"at_step": 1, "action": "code.patch", "op": "replace", "find": "z();",
            "content": "w();", "diff": true}]),
    );
    let f = render_doc(&p, 1, &[]);
    let bottom = row_of(&f, "╰");
    assert_eq!(row_of(&f, "y();") + 1, bottom, "{f}");
    assert!(
        f.lines()
            .nth(bottom)
            .unwrap()
            .trim_start()
            .starts_with("3 "),
        "{f}"
    );
}

#[test]
fn an_arrow_goes_on_across_the_lines_removed_inside_its_span() {
    let p = doc(
        "diff-arrow",
        &["let a = 1;", "let b = 2;", "let c = a;"],
        json!([{"id": "ar", "step": 1, "type": "arrow",
            "from": {"line": 1, "col": 4}, "to": {"line": 2, "col": 8}}]),
        json!([{"at_step": 1, "action": "code.patch", "op": "delete", "find": "let b = 2;", "diff": true}]),
    );
    let f = render_doc(&p, 1, &[]);
    let gone = row_of(&f, "let b");
    let above = f.lines().nth(gone - 1).unwrap();
    let rail = above
        .char_indices()
        .take_while(|(i, _)| *i < code_x_of(&f, "let a"))
        .find(|(_, c)| matches!(c, '│' | '╭' | '┌' | '├'))
        .map(|(i, _)| above[..i].chars().count())
        .expect("a lane");
    assert_eq!(
        f.lines().nth(gone).unwrap().chars().nth(rail),
        Some('│'),
        "{f}"
    );
}

/// The column where `needle` starts on its row.
fn code_x_of(text: &str, needle: &str) -> usize {
    text.lines().find_map(|l| l.find(needle)).unwrap()
}

#[test]
fn a_margin_label_goes_on_beside_a_removed_line_as_beside_code() {
    let p = doc(
        "diff-margin-label",
        &["let a = 1;", "let b = 2;", "let c = 3;", "let d = 4;"],
        json!([{"id": "n", "step": 0, "type": "note", "line": 2,
            "text": "a long note that has many words so that it goes on over more than one row in the margin of the take"}]),
        json!([{"at_step": 1, "action": "code.patch", "op": "delete", "find": "let c = 3;", "diff": true}]),
    );
    // Step 0: the note goes on beside line 3. Step 1: beside the removed
    // line 3, whole, not cut.
    for step in [0, 1] {
        let f = render_doc(&p, step, &[]);
        assert!(f.contains("in the margin of the take"), "step {step}:\n{f}");
        assert!(!f.contains('…'), "step {step}:\n{f}");
    }
    let f = render_doc(&p, 1, &[]);
    let removed = f.lines().nth(row_of(&f, "let c = 3;")).unwrap();
    assert!(removed.contains("that it goes on"), "{f}");
}
