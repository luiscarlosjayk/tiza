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
