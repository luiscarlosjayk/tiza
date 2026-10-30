//! `tiza check` and code lines that the take cuts with «…» at the width of
//! the check: a warning `clipped_code` for each file that the take shows
//! with a cut line, and one more when a move changes which lines are cut.
//! The warning names the cut lines (five, then «and N more»), the steps and
//! the width from which every code line of the take fits. A warning, so
//! `--strict` fails. Each test also compares the warnings with `tiza render
//! --all-steps --format txt` at the same width: at each step, the lines
//! that the warning names are the code rows that the take draws with «…».

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, scratch};

/// A line of `n` columns: `    let v = xxx…x;`.
fn long_line(n: usize) -> String {
    let fixed = "    let v = ;".len();
    format!("    let v = {};", "x".repeat(n - fixed))
}

fn short_code() -> Vec<String> {
    vec![
        "fn main() {".into(),
        "    let a = 1;".into(),
        "    let b = 2;".into(),
        "}".into(),
    ]
}

fn note(id: &str, step: u32, line: u32) -> Value {
    json!({"id": id, "step": step, "type": "note", "line": line, "text": id})
}

/// A document with `code`, `annotations` and `moves`, next to the files
/// in `files` (name, text).
fn document(
    name: &str,
    code: Vec<String>,
    annotations: Value,
    moves: Value,
    files: &[(&str, String)],
) -> PathBuf {
    let dir = scratch(name);
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code,
        "annotations": annotations, "moves": moves
    });
    let path = dir.join("doc.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    path
}

/// `tiza check FILE --width W [extra]` (with `--json` unless `extra` asks
/// for the text): (exit code, stdout).
fn run_check(file: &Path, width: usize, extra: &[&str]) -> (i32, String) {
    let o = tiza_qa::command()
        .args(["check", file.to_str().unwrap(), "--width"])
        .arg(width.to_string())
        .args(extra)
        .output()
        .unwrap();
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
    )
}

/// `tiza check --json --width W [extra]`: (exit code, report).
fn check(file: &Path, width: usize, extra: &[&str]) -> (i32, Value) {
    let mut args = vec!["--json"];
    args.extend_from_slice(extra);
    let (code, out) = run_check(file, width, &args);
    let v: Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"));
    (code, v)
}

fn with_code(report: &Value, code: &str) -> Vec<Value> {
    report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == code)
        .cloned()
        .collect()
}

/// The code rows of each step that `tiza render --all-steps --format txt`
/// draws with «…» at `width`: (step, line). A row with a line number that
/// has «…» is a cut code line only in a document whose code has no «…» and
/// whose labels are whole at that width: `checked_against_render` makes
/// sure of both.
fn cut_in_render(file: &Path, width: usize) -> Vec<(u64, u64)> {
    let dir = file.with_file_name(format!("takes-{width}"));
    let o = tiza_qa::command()
        .args([
            "render",
            file.to_str().unwrap(),
            "--all-steps",
            "--format",
            "txt",
        ])
        .args(["--width", &width.to_string(), "--out"])
        .arg(format!("{}/", dir.display()))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    // `step-00.txt`, `step-01.txt`…
    paths.sort();
    let mut out = Vec::new();
    for (step, p) in paths.iter().enumerate() {
        for row in std::fs::read_to_string(p).unwrap().lines() {
            // A code row starts with its line number.
            let Some((n, _)) = row.trim_start().split_once(' ') else {
                continue;
            };
            if let Ok(line) = n.parse::<u64>()
                && row.contains('…')
            {
                out.push((step as u64, line));
            }
        }
    }
    out
}

/// The steps of a warning, from its message: «only at step 3», or «first
/// at step 0, then at steps 1-3, 5».
fn steps_of(message: &str) -> Vec<u64> {
    let when = message.split_once(", ").unwrap().1;
    let when = when.split_once(": L").unwrap().0;
    if let Some(one) = when.strip_prefix("only at step ") {
        return vec![one.parse().unwrap()];
    }
    let (first, rest) = when
        .strip_prefix("first at step ")
        .unwrap()
        .split_once(", then at step")
        .unwrap();
    let mut out = vec![first.parse().unwrap()];
    for part in rest.trim_start_matches('s').trim().split(", ") {
        match part.split_once('-') {
            Some((a, b)) => out.extend(a.parse::<u64>().unwrap()..=b.parse().unwrap()),
            None => out.push(part.parse().unwrap()),
        }
    }
    out
}

/// What a warning says of its lines, from its message: the lines it
/// names, how many more it counts, and at how many of its steps the lines
/// have other numbers. «L3, L7» → ([3, 7], 0, 0); «L3, L4, L5, L6, L7 and 2
/// more» → ([3, 4, 5, 6, 7], 2, 0); «L3 at step 0, with other numbers at 5
/// steps» → ([3], 0, 5).
fn lines_of(message: &str) -> (Vec<u64>, usize, usize) {
    let named = message.split_once(": ").unwrap().1;
    let named = named.split_once(" (").unwrap().0;
    let (named, renumbered) = match named.split_once(", with other numbers at ") {
        Some((named, n)) => {
            let n = n.split(' ').next().unwrap().parse().unwrap();
            (named.rsplit_once(" at step ").unwrap().0, n)
        }
        None => (named, 0),
    };
    let (named, more) = match named.split_once(" and ") {
        Some((named, more)) => (named, more.strip_suffix(" more").unwrap().parse().unwrap()),
        None => (named, 0),
    };
    let lines = named
        .split(", ")
        .map(|l| l.trim_start_matches('L').parse().unwrap())
        .collect();
    (lines, more, renumbered)
}

/// Checks `file` at `width` and compares the warnings with the take. At
/// the first step of a warning, the render cuts exactly its lines (the
/// first five, and as many more as it counts). At its other steps, as many
/// lines, with the same numbers except at as many steps as it says. A step
/// is in one warning at most, and a step with no cut line is in none.
/// Returns the warnings.
fn checked_against_render(file: &Path, width: usize) -> Vec<Value> {
    let (_, report) = check(file, width, &[]);
    // Only a cut code line draws «…» on a numbered row.
    assert!(
        with_code(&report, "clipped_label").is_empty(),
        "a label is cut: {report}"
    );
    let dir = file.parent().unwrap();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_file() {
            assert!(
                !std::fs::read_to_string(&p).unwrap().contains('…'),
                "{} has «…»",
                p.display()
            );
        }
    }
    let warnings = with_code(&report, "clipped_code");
    let render = cut_in_render(file, width);
    let last = render.iter().map(|(s, _)| *s).max().unwrap_or(0);
    let mut seen: Vec<u64> = Vec::new();
    for w in &warnings {
        let m = w["message"].as_str().unwrap();
        let steps = steps_of(m);
        let (lines, more, renumbered) = lines_of(m);
        assert_eq!(w["step"], steps[0], "{m}");
        assert_eq!(w["line"], lines[0], "{m}");
        let cut_at = |s: u64| -> Vec<u64> {
            render
                .iter()
                .filter(|(rs, _)| *rs == s)
                .map(|(_, l)| *l)
                .collect()
        };
        let first = cut_at(steps[0]);
        assert_eq!(
            (&first[..first.len().min(5)], first.len()),
            (&lines[..], lines.len() + more),
            "step {}: the render cuts {first:?}; the warning says «{m}»",
            steps[0]
        );
        let mut other_numbers = 0;
        for s in &steps {
            assert!(!seen.contains(s), "two warnings at step {s}: {warnings:?}");
            seen.push(*s);
            let cut = cut_at(*s);
            assert_eq!(
                cut.len(),
                first.len(),
                "step {s}: the render cuts {cut:?}; the warning says «{m}»"
            );
            if cut != first {
                other_numbers += 1;
            }
        }
        assert_eq!(other_numbers, renumbered, "«{m}»");
    }
    for s in 0..=last {
        if render.iter().any(|(rs, _)| *rs == s) {
            assert!(
                seen.contains(&s),
                "the render cuts code at step {s}, but no warning says so: {warnings:?}"
            );
        }
    }
    warnings
}

#[test]
fn a_cut_code_line_is_a_warning_that_strict_fails_on() {
    let mut code = short_code();
    code.insert(2, long_line(112));
    let file = document(
        "cut-code-one",
        code,
        json!([note("n1", 1, 2)]),
        json!([]),
        &[],
    );
    for width in [100, 60, 40] {
        let warnings = checked_against_render(&file, width);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0]["level"], "warning");
        assert_eq!(warnings[0]["line"], 3);
        assert_eq!(warnings[0]["step"], 0);
        assert_eq!(
            warnings[0]["message"],
            format!(
                "code is cut with «…» at width {width}, first at step 0, then at step 1: L3 (every code line of the take fits from width 118, `tiza render --width 118`)"
            )
        );
        assert_eq!(run_check(&file, width, &[]).0, 0);
        assert_eq!(run_check(&file, width, &["--strict"]).0, 1);
    }
    // Wide enough: no warning.
    assert!(checked_against_render(&file, 118).is_empty());
    assert_eq!(run_check(&file, 118, &["--strict"]).0, 0);
}

#[test]
fn a_line_that_fits_exactly_is_not_cut() {
    // Line 3 has 112 columns and line 4 has 111.
    let mut code = short_code();
    code.insert(2, long_line(112));
    code.insert(3, long_line(111));
    let file = document(
        "cut-code-exact",
        code,
        json!([note("n1", 1, 2)]),
        json!([]),
        &[],
    );
    let (_, report) = check(&file, 100, &[]);
    let message = with_code(&report, "clipped_code")[0]["message"]
        .as_str()
        .unwrap()
        .to_string();
    let fits: usize = message
        .split_once("fits from width ")
        .unwrap()
        .1
        .split(',')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    // At that width line 3 fits with no column to spare. One less cuts it,
    // and line 4 fits there with no column to spare.
    assert!(checked_against_render(&file, fits).is_empty());
    assert_eq!(run_check(&file, fits, &["--strict"]).0, 0);
    let warnings = checked_against_render(&file, fits - 1);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let message = warnings[0]["message"].as_str().unwrap();
    assert!(message.contains(": L3 (every"), "{message}");
}

#[test]
fn a_line_wider_than_any_width_says_so() {
    let mut code = short_code();
    code.insert(2, long_line(500));
    let file = document(
        "cut-code-huge",
        code,
        json!([note("n1", 1, 2)]),
        json!([]),
        &[],
    );
    let warnings = checked_against_render(&file, 400);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let message = warnings[0]["message"].as_str().unwrap();
    assert!(
        message.ends_with(": L3 (no width up to 400 shows every code line whole)"),
        "{message}"
    );
}

#[test]
fn only_the_lines_of_the_window_of_a_link_count() {
    let long: String = (1..=20)
        .map(|i| {
            if i == 10 {
                format!("{}\n", long_line(130))
            } else {
                format!("fn f{i}() {{}}\n")
            }
        })
        .collect();
    let link = |lines: &str| json!([{"at_step": 2, "action": "code.link", "file": "long.rs", "lines": lines}]);
    // Line 10 is outside the window 1-5: the take never draws it.
    let outside = document(
        "cut-code-window-out",
        short_code(),
        json!([note("n1", 1, 2)]),
        link("1-5"),
        &[("long.rs", long.clone())],
    );
    assert!(checked_against_render(&outside, 100).is_empty());
    // Inside the window 8-12: cut from step 2, in long.rs.
    let inside = document(
        "cut-code-window-in",
        short_code(),
        json!([note("n1", 1, 2)]),
        link("8-12"),
        &[("long.rs", long)],
    );
    let warnings = checked_against_render(&inside, 100);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0]["file"], "long.rs");
    assert_eq!(warnings[0]["line"], 10);
    assert_eq!(warnings[0]["step"], 2);
}

#[test]
fn a_patch_that_adds_a_long_line_cuts_from_its_step() {
    let file = document(
        "cut-code-patch",
        short_code(),
        json!([note("n1", 1, 2), note("n5", 5, 2)]),
        json!([{"at_step": 3, "action": "code.patch", "op": "insert", "position": "1",
            "content": long_line(120)}]),
        &[],
    );
    let warnings = checked_against_render(&file, 100);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0]["step"], 3);
    let message = warnings[0]["message"].as_str().unwrap();
    assert!(
        message.contains("first at step 3, then at steps 4-5: L1 ("),
        "{message}"
    );
}

#[test]
fn a_patch_that_moves_a_cut_line_names_its_numbers_at_the_first_step() {
    // Line 10 is long. The patch at step 2 removes lines 1-5: the same line
    // is line 5 at steps 2 to 5 (steps 2-4 draw as one).
    let code: Vec<String> = (1..=12)
        .map(|i| {
            if i == 10 {
                long_line(130)
            } else {
                format!("fn f{i}() {{}}")
            }
        })
        .collect();
    let file = document(
        "cut-code-renumbered",
        code,
        json!([note("n1", 1, 11), note("n5", 5, 6)]),
        json!([{"at_step": 2, "action": "code.patch", "op": "delete", "position": "1-5"}]),
        &[],
    );
    let warnings = checked_against_render(&file, 100);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(
        (&warnings[0]["step"], &warnings[0]["line"]),
        (&json!(0), &json!(10))
    );
    let message = warnings[0]["message"].as_str().unwrap();
    assert!(
        message.contains(": L10 at step 0, with other numbers at 4 steps ("),
        "{message}"
    );
}

#[test]
fn a_typing_demo_gives_one_warning_for_its_long_line() {
    // Each patch puts a line on top: the long line 3 goes down one line at
    // steps 2, 3 and 4. At step 5 a patch adds a second long line: from
    // there, other lines are cut, and a second warning tells it.
    let top = |at: u32| {
        json!({"at_step": at, "action": "code.patch", "op": "insert",
        "position": "1", "content": format!("// line {at}")})
    };
    let mut code = short_code();
    code.insert(2, long_line(112));
    let file = document(
        "cut-code-typing",
        code,
        json!([note("n1", 1, 2), note("n6", 6, 2)]),
        json!([top(2), top(3), top(4),
            {"at_step": 5, "action": "code.patch", "op": "insert", "position": "1",
                "content": long_line(120)}]),
        &[],
    );
    let warnings = checked_against_render(&file, 100);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    let message = warnings[0]["message"].as_str().unwrap();
    assert!(
        message.contains(
            "first at step 0, then at steps 1-4: L3 at step 0, with other numbers at 3 steps ("
        ),
        "{message}"
    );
    assert_eq!(warnings[1]["step"], 5);
    let message = warnings[1]["message"].as_str().unwrap();
    assert!(
        message.contains("first at step 5, then at step 6: L1, L7 ("),
        "{message}"
    );
}

#[test]
fn each_file_of_the_take_has_its_own_warning() {
    let mut code = short_code();
    code.insert(2, long_line(112));
    let other = format!("fn other() {{\n{}\n}}\n", long_line(125));
    let file = document(
        "cut-code-files",
        code,
        json!([note("n1", 1, 2)]),
        json!([{"at_step": 2, "action": "code.link", "file": "other.rs"}]),
        &[("other.rs", other)],
    );
    let warnings = checked_against_render(&file, 100);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert_eq!(warnings[0]["file"], Value::Null, "{warnings:?}");
    assert_eq!(warnings[0]["line"], 3);
    assert_eq!(warnings[0]["step"], 0);
    assert_eq!(warnings[1]["file"], "other.rs");
    assert_eq!(warnings[1]["line"], 2);
    assert_eq!(warnings[1]["step"], 2);
    // The text report names the file.
    let (_, text) = run_check(&file, 100, &[]);
    assert!(
        text.contains("warning: step 2, other.rs, L2: code is cut with «…» at width 100"),
        "{text}"
    );
}

#[test]
fn tabs_and_wide_characters_count_as_the_take_draws_them() {
    // 90 characters, but 4 tabs make it 102 columns.
    let tabs = format!("\t\t\t\t{}", "y".repeat(86));
    // 59 characters, but 42 wide ones make it 99 columns.
    let wide = format!("    let s = \"{}\";", "字".repeat(42));
    let mut code = short_code();
    code.insert(2, tabs);
    code.insert(3, wide);
    let file = document(
        "cut-code-tabs-wide",
        code,
        json!([note("n1", 1, 2)]),
        json!([]),
        &[],
    );
    let warnings = checked_against_render(&file, 100);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let message = warnings[0]["message"].as_str().unwrap();
    assert!(message.contains(": L3, L4 (every"), "{message}");
}

#[test]
fn the_warning_names_five_lines_then_counts_the_rest() {
    let many = |name: &str, n: usize| {
        let mut code = short_code();
        for i in 0..n {
            code.insert(2 + i, long_line(110 + i));
        }
        document(name, code, json!([note("n1", 1, 2)]), json!([]), &[])
    };
    let seven = checked_against_render(&many("cut-code-seven", 7), 100);
    let message = seven[0]["message"].as_str().unwrap();
    assert!(
        message.contains(
            ": L3, L4, L5, L6, L7 and 2 more (every code line of the take fits from width"
        ),
        "{message}"
    );
    let five = checked_against_render(&many("cut-code-five", 5), 100);
    let message = five[0]["message"].as_str().unwrap();
    assert!(
        message.contains(": L3, L4, L5, L6, L7 (every code line of the take fits from width"),
        "{message}"
    );
}

#[test]
fn a_cut_line_without_line_numbers_is_a_warning_too() {
    let mut code = short_code();
    code.insert(2, long_line(112));
    let dir = scratch("cut-code-no-numbers");
    let file = dir.join("doc.tiza");
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code,
        "numbers": false, "annotations": [note("n1", 1, 2)]
    });
    std::fs::write(&file, doc.to_string()).unwrap();
    let (_, report) = check(&file, 100, &[]);
    let warnings = with_code(&report, "clipped_code");
    assert_eq!(warnings.len(), 1, "{report}");
    assert_eq!(warnings[0]["line"], 3);
    // The take has no line numbers: the row of line 3 is the one that
    // shows its text.
    let o = tiza_qa::command()
        .args([
            "render",
            file.to_str().unwrap(),
            "--format",
            "txt",
            "--width",
            "100",
        ])
        .output()
        .unwrap();
    let text = String::from_utf8(o.stdout).unwrap();
    let row = text
        .lines()
        .find(|r| r.contains("let v = xxx"))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(row.trim_end().ends_with('…'), "{row}");
}

#[test]
fn the_live_check_gives_the_warning_too() {
    let mut code = short_code();
    code.insert(2, long_line(112));
    let file = document(
        "cut-code-live",
        code,
        json!([note("n1", 1, 2)]),
        json!([]),
        &[],
    );
    let (_, report) = check(&file, 100, &[]);
    let doc = tiza_core::fs::open(file.to_str().unwrap())
        .unwrap()
        .document;
    let dir = file.parent().unwrap();
    let live = Live::open(doc, dir.join("held.tiza"), dir.join("live.sock"));
    let v = live.api_ok("check", json!({"width": 100}));
    assert_eq!(with_code(&v, "clipped_code").len(), 1, "{v}");
    assert_eq!(
        with_code(&v, "clipped_code"),
        with_code(&report, "clipped_code"),
        "{v}"
    );
}
