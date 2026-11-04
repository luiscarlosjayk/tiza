//! `tiza check --width N` takes the width of a terminal, as the TUI shows
//! a document in it: the side padding of the document (`padding`), else of
//! the config file (`pad`), else 1 column, comes off it. A terminal too
//! narrow for the padding keeps less of it, as the screen does. The report
//! says the terminal and its columns of code; `tiza render --width` keeps
//! columns of code (an export has no padding), and so does the `check` of
//! the socket and MCP.
//!
//! The bug: a terminal of 100 columns with a padding of 2 shows 96 columns
//! of code, and `tiza check --width 100` checked 100, so it said
//! «0 warnings» for a label that the recording cut.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, scratch, tiza_with_config};

/// The note of line 1: about 60 columns, so the margin cuts it in a
/// terminal of some 90 columns.
const NOTE: &str = "the label that a recording cuts when check says it is whole";

/// A line of code 64 columns wide: a terminal narrower than about 70
/// columns cuts it.
fn long_line() -> String {
    format!("    let total = {};", "x".repeat(47))
}

/// A document in a scratch folder `name`: `doc.rs` and `doc.tiza`, with
/// `padding` (none: the document leaves it to the config) and a config file
/// `cfg.toml` with `config`. Returns the `.tiza`.
fn document(name: &str, padding: Option<[u16; 2]>, config: &str) -> PathBuf {
    let code = format!("fn main() {{\n{}\n}}\n", long_line());
    document_of(name, &code, NOTE, padding, config)
}

/// [`document`] with the code `code` and the note `note` on line 1.
fn document_of(
    name: &str,
    code: &str,
    note: &str,
    padding: Option<[u16; 2]>,
    config: &str,
) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(dir.join("doc.rs"), code).unwrap();
    let mut doc = json!({
        "schema": 1,
        "source": {"language": "rs"},
        "code": {"file": "doc.rs"},
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 1, "text": note}
        ]
    });
    if let Some(p) = padding {
        doc["padding"] = json!(p);
    }
    let path = dir.join("doc.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    std::fs::write(dir.join("cfg.toml"), config).unwrap();
    path
}

/// `tiza check FILE --json [--width W]` with the config file of its folder.
fn check(file: &Path, width: Option<usize>) -> Value {
    let w = width.map(|w| w.to_string());
    let mut args = vec!["check", file.to_str().unwrap(), "--json"];
    if let Some(w) = &w {
        args.extend(["--width", w]);
    }
    let (_, out, err) = tiza_with_config(file.parent().unwrap(), "cfg.toml", &args);
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}{err}"))
}

/// `tiza check FILE --width W` as text.
fn check_text(file: &Path, width: usize) -> String {
    let w = width.to_string();
    let args = ["check", file.to_str().unwrap(), "--width", &w];
    tiza_with_config(file.parent().unwrap(), "cfg.toml", &args).1
}

/// A name for an assertion message.
fn cut_label(cut: bool) -> &'static str {
    if cut { "a label cut" } else { "no label cut" }
}

fn clipped_labels(v: &Value) -> usize {
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "clipped_label")
        .count()
}

/// The columns of code and the padding that a report names.
fn widths(v: &Value) -> (Value, Value, Value) {
    (
        v["width"].clone(),
        v["terminal"]["width"].clone(),
        v["terminal"]["padding"].clone(),
    )
}

#[test]
fn a_terminal_with_padding_cuts_a_label_that_its_width_in_code_shows() {
    let file = document("tw-bug", Some([1, 2]), "");
    // The narrowest clean width, in columns of code and of a terminal.
    let v = check(&file, None);
    let code = v["min_clean_width"].as_u64().unwrap() as usize;
    assert_eq!(v["terminal"]["min_clean_width"], json!(code + 4), "{v}");
    // A terminal of `code` + 3 columns has `code` - 1 columns of code: the
    // note is cut. (The old check measured `code` + 3 columns of code.)
    let v = check(&file, Some(code + 3));
    assert_eq!(v["width"], json!(code - 1), "{v}");
    assert_eq!(clipped_labels(&v), 1, "{v}");
    let v = check(&file, Some(code + 4));
    assert_eq!(v["width"], json!(code), "{v}");
    assert_eq!(clipped_labels(&v), 0, "{v}");
    // The text says the two widths, and the width for `tiza render`.
    let text = check_text(&file, code + 3);
    let t = code + 3;
    assert!(
        text.contains(&format!(
            "labels at width {t}: {} columns of code, padding 2",
            code - 1
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!("is clipped with «…» at width {t}")),
        "{text}"
    );
    assert!(
        text.trim_end().ends_with(&format!(
            "no label is clipped at {} or wider ({code} columns of code: `tiza render --width {code}`)",
            code + 4
        )),
        "{text}"
    );
}

#[test]
fn the_padding_comes_from_the_document_else_the_config_else_one_column() {
    let pad3 = "[defaults]\npad = \"0 3\"\n";
    for (name, padding, config, code, pad) in [
        ("tw-doc", Some([1, 2]), "", 96, 2),
        // `:pad 0 0` saves a padding of 0: the whole terminal.
        ("tw-zero", Some([0, 0]), "", 100, 0),
        ("tw-config", None, pad3, 94, 3),
        ("tw-both", Some([1, 2]), pad3, 96, 2),
        ("tw-none", None, "", 98, 1),
    ] {
        let file = document(name, padding, config);
        let v = check(&file, Some(100));
        assert_eq!(
            widths(&v),
            (json!(code), json!(100), json!(pad)),
            "{name}: {v}"
        );
    }
}

#[test]
fn the_default_width_is_a_terminal_too() {
    // No `--width`: the config file's `width`, else 100, less the padding.
    let file = document("tw-default", Some([1, 2]), "");
    assert_eq!(
        widths(&check(&file, None)),
        (json!(96), json!(100), json!(2))
    );
    let file = document("tw-default-72", Some([1, 2]), "[defaults]\nwidth = 72\n");
    assert_eq!(
        widths(&check(&file, None)),
        (json!(68), json!(72), json!(2))
    );
}

#[test]
fn a_narrow_terminal_keeps_20_columns_of_code() {
    // As the screen: the padding shrinks so that the code keeps 20 columns.
    let file = document("tw-narrow", Some([0, 5]), "");
    for (terminal, code) in [
        (20, 20),
        (21, 21),
        (22, 20),
        (29, 21),
        (30, 20),
        (31, 21),
        (40, 30),
    ] {
        let v = check(&file, Some(terminal));
        assert_eq!(v["width"], json!(code), "{terminal}: {v}");
        assert_eq!(v["terminal"]["padding"], json!(5), "{terminal}: {v}");
    }
}

#[test]
fn a_document_that_does_not_load_stdin_and_a_source_file() {
    // A document that does not load: the padding of the config file.
    let file = document("tw-broken", None, "[defaults]\npad = \"0 3\"\n");
    std::fs::write(&file, "{ not json").unwrap();
    let v = check(&file, Some(100));
    assert_eq!(widths(&v), (json!(94), json!(100), json!(3)), "{v}");
    assert_eq!(v["ok"], false, "{v}");
    // Standard input and a source file with its `.tiza` next to it read the
    // document's padding.
    let file = document("tw-stdin", Some([1, 2]), "");
    let out = tiza_qa::command()
        .args(["check", "-", "--json", "--width", "100"])
        .current_dir(file.parent().unwrap())
        .stdin(std::fs::File::open(&file).unwrap())
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(widths(&v), (json!(96), json!(100), json!(2)), "{v}");
    std::fs::copy(&file, file.with_file_name("doc.rs.tiza")).unwrap();
    let source = file.with_file_name("doc.rs");
    let out = tiza_qa::command()
        .args([
            "check",
            source.to_str().unwrap(),
            "--json",
            "--width",
            "100",
        ])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(widths(&v), (json!(96), json!(100), json!(2)), "{v}");
}

#[test]
fn render_width_stays_columns_of_code() {
    // An export has no padding: `render --width 66` draws the line of 64
    // columns whole (with the gutter), and a terminal of 66 cuts it.
    let file = document("tw-render", Some([1, 2]), "");
    let out = tiza_qa::tiza_ok(&[
        "render",
        file.to_str().unwrap(),
        "--width",
        "70",
        "--format",
        "txt",
        "--out",
        "-",
    ]);
    assert!(!out.contains('…'), "{out}");
    let v = check(&file, Some(70));
    let cut = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["code"] == "clipped_code");
    assert!(cut, "{v}");
    // The live check and MCP keep columns of code too: no `terminal`.
    let doc = tiza_core::fs::load_document(&file).unwrap();
    let dir = file.parent().unwrap();
    let live = Live::open(doc, dir.join("held.tiza"), dir.join("live.sock"));
    let v = live.api_ok("check", json!({"width": 70}));
    assert_eq!(v["width"], 70, "{v}");
    assert!(v.get("terminal").is_none(), "{v}");
}

/// The TUI at `cols` columns, in normal or presentation mode: the width of
/// its take (`render.text` without a width), its `code needs N columns`, and
/// whether it says that a label is cut.
fn screen(live: &Live, cols: u16, present: bool) -> (u64, Option<u64>, bool) {
    live.with(move |a| {
        a.size = (cols, 30);
        a.set_present(present);
    });
    let r = live.api_ok("render.text", json!({}));
    let status = live.api_ok("status", json!({}));
    // No `warnings` when the screen loses nothing.
    let needs = status["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| w.as_str()?.strip_prefix("code needs "))
        .map(|w| w.split(' ').next().unwrap().parse().unwrap())
        .next();
    let cut = status["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|w| w.as_str().is_some_and(|w| w.contains(" cut at ")));
    (r["width"].as_u64().unwrap(), needs, cut)
}

/// The oracle: at each terminal width, `tiza check` lays the code out at
/// the width of the screen, names the terminal from which every code line
/// fits as the status bar does (`code needs N cols`), and finds a label cut
/// where presentation mode cuts it.
#[test]
fn check_takes_the_width_of_the_screen_at_every_terminal_width() {
    let pad3 = "[defaults]\npad = \"0 3\"\n";
    let widths: Vec<u16> = (20..=36)
        .chain([40, 60, 66, 67, 68, 70, 80, 100, 130])
        .collect();
    // A short document whose first line fits in 21 columns of code and not
    // in 20 (with the gutter): a terminal too narrow for its padding of 5
    // keeps 21 columns at 21, 23, 25… columns, and the status bar must not
    // measure 20 there.
    let short = "let abc = 1234;\nfn f() {}\n";
    let long = format!("fn main() {{\n{}\n}}\n", long_line());
    for (name, code, note, padding, config, config_pad) in [
        ("tw-o-doc", &long[..], NOTE, Some([1, 2]), "", None),
        ("tw-o-wide", &long, NOTE, Some([0, 5]), "", None),
        ("tw-o-short", short, "a short note", Some([0, 5]), "", None),
        ("tw-o-config", &long, NOTE, None, pad3, Some((0, 3))),
        ("tw-o-none", &long, NOTE, None, "", None),
    ] {
        let file = document_of(name, code, note, padding, config);
        let doc = tiza_core::fs::load_document(&file).unwrap();
        let dir = file.parent().unwrap();
        let live = Live::open(doc, dir.join("held.tiza"), dir.join("live.sock"));
        live.with(move |a| a.config_defaults.pad = config_pad);
        live.api_ok("step.goto", json!({"id": "1"}));
        let mut cut = 0;
        for &cols in &widths {
            let v = check(&file, Some(cols as usize));
            let fits = v["findings"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|f| f["code"] == "clipped_code")
                .map(|f| {
                    let m = f["message"].as_str().unwrap();
                    let t = m.split_once("fits from width ").unwrap().1;
                    t.split([',', ' ']).next().unwrap().parse::<u64>().unwrap()
                })
                .next();
            let clipped = clipped_labels(&v) > 0;
            for present in [false, true] {
                let (width, needs, label_cut) = screen(&live, cols, present);
                assert_eq!(
                    v["width"].as_u64(),
                    Some(width),
                    "{name} at {cols} (present: {present}): check {v}"
                );
                // The code is the same at every step: the status bar and
                // `check` name the same terminal, or neither cuts the code.
                assert_eq!(needs, fits, "{name} at {cols} (present: {present}): {v}");
                if needs.is_some() {
                    cut += 1;
                }
                // Presentation mode shows the take, at its last step here.
                if present {
                    assert_eq!(
                        cut_label(label_cut),
                        cut_label(clipped),
                        "{name} at {cols}: {v}"
                    );
                }
            }
        }
        assert!(
            cut > 0,
            "{name}: no width cut the code: the test compared nothing"
        );
    }
}
