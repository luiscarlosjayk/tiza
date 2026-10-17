//! t-068 — Brand typography in SVG/PNG exports: Tiza Serif (a Libron
//! subset, OFL) for notes, labels and legends; monospace for code, boxes and
//! arrows.
//!
//! Acceptance criteria:
//! - AC1 The SVG embeds four Tiza Serif faces (Regular, Bold, Italic,
//!   Bold Italic) next to JetBrains Mono and the symbols font; prose runs
//!   (margin labels, notes, legends, under-line labels) carry class `p`, no
//!   `textLength` (natural width) and the serif family first; code, line
//!   numbers, box borders and titles keep the monospace grid with
//!   `textLength`.
//! - AC2 A mark label's glyphs (`━▶`, `╳`, `✎▶`) stay a monospace run on the
//!   grid; only the words after them are prose (resvg would otherwise set
//!   the whole run in the monospace).
//! - AC3 `--mono-labels` (SvgOptions.brand_font = false) restores the
//!   all-monospace export; the terminal is unaffected (CellStyle.prose is
//!   ignored by the TUI).
//! - AC4 PNG rasterization finds the serif in the embedded font database
//!   (pixels differ from the mono export in the margin, not in the code).
//! - AC5 Fonts README and ADR 0006 record the subset, the OFL renaming
//!   ("Libron" is a Reserved Font Name) and the recommended terminal pairing.

use tiza_qa::{example, fixture, scratch, tiza_ok};

fn svg(args: &[&str]) -> String {
    let f = example("ownership.rs.tiza");
    let mut full = vec![
        "render",
        f.to_str().unwrap(),
        "--out",
        "-",
        "--format",
        "svg",
        "--step",
        "7",
        "--width",
        "100",
    ];
    full.extend_from_slice(args);
    tiza_ok(&full)
}

fn texts(svg: &str) -> Vec<&str> {
    svg.match_indices("<text ")
        .map(|(i, _)| &svg[i..svg[i..].find("</text>").map(|e| i + e + 7).unwrap()])
        .collect()
}

#[test]
fn ac1_serif_faces_and_prose_runs() {
    let s = svg(&[]);
    for face in [
        "font-weight:400;font-style:normal",
        "font-weight:700;font-style:normal",
        "font-weight:400;font-style:italic",
        "font-weight:700;font-style:italic",
    ] {
        assert!(
            s.contains(&format!("@font-face{{font-family:\"Tiza Serif\";{face}")),
            "{face}"
        );
    }
    assert!(s.contains(".p{font-family:\"Tiza Serif\",\"JetBrains Mono\",\"Tiza Symbols\""));
    let runs = texts(&s);
    let born = runs
        .iter()
        .find(|t| t.contains(">s is born<"))
        .expect("note run");
    assert!(born.contains("class=\"p\""), "{born}");
    assert!(!born.contains("textLength"), "natural width: {born}");
    let code = runs
        .iter()
        .find(|t| t.ends_with(">takes</text>") && t.contains("#61afef"))
        .expect("code run (syntax colors split the line into runs)");
    assert!(
        !code.contains("class=\"p") && code.contains("textLength"),
        "{code}"
    );
    let title = runs
        .iter()
        .find(|t| t.contains("takes owns s"))
        .expect("box title");
    assert!(
        !title.contains(" p") && title.contains("textLength"),
        "box titles stay on the grid: {title}"
    );
    let number = runs
        .iter()
        .find(|t| t.contains(">15<") || t.contains("> 15<") || t.contains(">  15<"))
        .expect("line number");
    assert!(!number.contains("class=\"p"), "{number}");
}

#[test]
fn ac2_mark_glyphs_stay_mono_words_go_serif() {
    let s = svg(&[]);
    let runs = texts(&s);
    let glyph = runs
        .iter()
        .find(|t| t.ends_with(">━▶</text>"))
        .expect("glyph run");
    assert!(
        glyph.contains("textLength") && !glyph.contains("class=\"p"),
        "{glyph}"
    );
    let words = runs
        .iter()
        .find(|t| t.contains("> move: s moves"))
        .expect("words run");
    assert!(
        words.contains("class=\"p\"") && !words.contains("textLength"),
        "{words}"
    );
    // The words start right after the glyph cells (2 cells = 2 × 9.6 px at 16 px).
    let x = |t: &str| {
        t.split("x=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse::<f32>()
            .unwrap()
    };
    assert!(
        (x(words) - x(glyph) - 2.0 * 9.6).abs() < 0.01,
        "{} vs {}",
        x(words),
        x(glyph)
    );
    let drop = runs
        .iter()
        .find(|t| t.ends_with(">╳</text>"))
        .expect("drop glyph");
    assert!(!drop.contains("class=\"p"));
}

#[test]
fn ac3_mono_labels_flag_and_terminal_unaffected() {
    let s = svg(&["--mono-labels"]);
    assert!(!s.contains("class=\"p"), "no prose runs");
    assert!(!s.contains(".p{"), "no prose style");
    assert!(
        s.contains("@font-face{font-family:\"Tiza Serif\""),
        "faces still embedded (same binary)"
    );
    let runs = texts(&s);
    let born = runs.iter().find(|t| t.contains(">s is born<")).unwrap();
    assert!(born.contains("textLength"));
    // The text export is byte-identical with or without the brand font.
    let f = example("ownership.rs.tiza");
    let a = tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        "-",
        "--step",
        "7",
        "--width",
        "100",
    ]);
    let b = tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        "-",
        "--step",
        "7",
        "--width",
        "100",
        "--mono-labels",
    ]);
    assert_eq!(a, b);
}

#[test]
fn ac4_png_uses_the_serif() {
    let dir = scratch("t068-png");
    let f = fixture("t-050/export.tiza");
    let serif = dir.join("serif.png");
    let mono = dir.join("mono.png");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        serif.to_str().unwrap(),
        "--width",
        "100",
        "--scale",
        "1",
    ]);
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        mono.to_str().unwrap(),
        "--width",
        "100",
        "--scale",
        "1",
        "--mono-labels",
    ]);
    let a = std::fs::read(&serif).unwrap();
    let b = std::fs::read(&mono).unwrap();
    assert_ne!(a, b, "the margin renders differently");
    assert_eq!(&a[16..24], &b[16..24], "same size");
}

#[test]
fn ac5_docs_record_license_and_pairing() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let fonts = std::fs::read_to_string(root.join("crates/tiza-export/fonts/README.md")).unwrap();
    assert!(
        fonts.contains("Tiza Serif")
            && fonts.contains("Libron")
            && fonts.contains("Reserved Font Name")
    );
    assert!(
        root.join("crates/tiza-export/assets/fonts/LICENSE-TizaSerif-Libron-OFL.txt")
            .exists()
    );
    // ADR 0006 predates the rename to Tiza (ADR 0008): it records the font
    // under its first name and carries the rename note at the top.
    let adr = std::fs::read_to_string(root.join("docs/adr/0006-exportacion-svg-png.md")).unwrap();
    assert!(adr.contains("CodeDraw Serif") && adr.contains("ADR 0008"));
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    assert!(readme.contains("--mono-labels"));
}
