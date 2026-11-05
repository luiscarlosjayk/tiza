//! The text of a mark, `glyph legend: label`, when its style has an empty
//! legend. A style `{"glyph": "+", "legend": ""}` drew `+ : label`, with a
//! stray `: `. Now the label follows the glyph, `+ label`; without a label
//! the mark shows its glyph only. A legend of spaces or of zero-width
//! characters is empty too, as a blank label is. A style without a legend
//! still shows the legend of its kind.
//!
//! The fixture `qa/fixtures/render/mark_legends/legends.tiza` has one mark
//! on each of lines 2 to 9, and a spotlight at step 2 that dims them.

use tiza_qa::{fixture, tiza_ok};

/// The take at `width` columns of code: the labels go in the right margin
/// at 140 and under their lines at 40.
const WIDTHS: [u32; 2] = [140, 40];

/// `tiza render legends.tiza --step STEP --width WIDTH --format txt`.
fn render(step: u32, width: u32) -> String {
    let path = fixture("render/mark_legends/legends.tiza");
    let (step, width) = (step.to_string(), width.to_string());
    tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--step",
        &step,
        "--width",
        &width,
        "--format",
        "txt",
    ])
}

/// The label text of the mark on line `n`: after the code on its row in
/// the margin, or on the row under it, after its `·`. Under the line,
/// `no row` when the next row is the code of the next line.
fn label_of(text: &str, n: u32, width: u32) -> String {
    let path = fixture("render/mark_legends/legends.tiza");
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let code = doc["code"][n as usize - 1].as_str().unwrap();
    let rows: Vec<&str> = text.lines().collect();
    let start = format!("{n:>3} ");
    let i = rows
        .iter()
        .position(|r| r.starts_with(&start))
        .unwrap_or_else(|| panic!("no row for L{n}\n{text}"));
    if width >= 100 {
        let row = rows[i];
        let at = row
            .find(code)
            .unwrap_or_else(|| panic!("no code of L{n} in «{row}»"));
        row[at + code.len()..].trim().to_string()
    } else {
        // The next row is the label's, or the code of the next line.
        let under = rows.get(i + 1).copied().unwrap_or("");
        if under.starts_with(&format!("{:>3} ", n + 1)) {
            return "no row".to_string();
        }
        under.trim().trim_start_matches('·').trim().to_string()
    }
}

#[test]
fn an_empty_legend_puts_the_label_right_after_the_glyph() {
    for step in [1, 2] {
        for width in WIDTHS {
            let text = render(step, width);
            assert_eq!(
                label_of(&text, 2, width),
                "+ s asks for memory on the heap",
                "step {step}, width {width}:\n{text}"
            );
            assert!(
                !text.contains(": s asks"),
                "step {step}, width {width}:\n{text}"
            );
        }
    }
}

#[test]
fn an_empty_legend_without_a_label_shows_only_the_glyph() {
    for width in WIDTHS {
        let text = render(1, width);
        assert_eq!(label_of(&text, 3, width), "+", "width {width}:\n{text}");
    }
}

#[test]
fn a_blank_legend_is_an_empty_legend() {
    for width in WIDTHS {
        let text = render(1, width);
        // A legend of three spaces, and one of a zero-width space.
        assert_eq!(label_of(&text, 4, width), "~ a legend of spaces", "{text}");
        assert_eq!(label_of(&text, 5, width), "^ a zero-width legend", "{text}");
        assert!(!text.contains('\u{200b}'), "{text}");
    }
}

#[test]
fn a_style_without_a_legend_shows_the_legend_of_its_kind() {
    for width in WIDTHS {
        let text = render(1, width);
        // A style of the document with no legend: its name is the legend
        // (`"legend": null` in the report).
        assert_eq!(
            label_of(&text, 6, width),
            "* kept: the name of the style",
            "width {width}:\n{text}"
        );
        // `ok` is a built-in kind: its legend is `ok`.
        assert_eq!(
            label_of(&text, 9, width),
            "✓ ok: the legend of the kind",
            "width {width}:\n{text}"
        );
    }
}

#[test]
fn a_mark_without_a_glyph_shows_its_label_alone() {
    for width in WIDTHS {
        let text = render(1, width);
        // A style with no glyph and an empty legend: the label alone, and
        // without a label no row of its own under the line.
        assert_eq!(label_of(&text, 8, width), "only the label", "{text}");
        let none = if width >= 100 { "" } else { "no row" };
        assert_eq!(label_of(&text, 7, width), none, "width {width}:\n{text}");
    }
}
