//! The `+` of a `code.patch` move with `diff` in an export: in the
//! `diff_add` color of the theme, on a cell with a tint of that color, at
//! the step of the move only. A `theme_overrides` sets the color.

use tiza_qa::{fixture, scratch, tiza_ok};

fn svg(doc: &std::path::Path, step: u32, theme: &str) -> String {
    let s = step.to_string();
    tiza_ok(&[
        "render",
        doc.to_str().unwrap(),
        "--step",
        &s,
        "--theme",
        theme,
        "--format",
        "svg",
        "--no-embed-fonts",
        "--out",
        "-",
    ])
}

/// The fill of the background rectangles of an SVG.
fn rect_fills(svg: &str) -> Vec<&str> {
    svg.split("<rect")
        .skip(1)
        .filter_map(|r| r.split("fill=\"").nth(1)?.split('"').next())
        .collect()
}

#[test]
fn the_sign_has_the_diff_color_on_a_tinted_cell_at_its_step() {
    let doc = fixture("render/patch_diff/borrow.tiza");
    for (theme, sign) in [("dark", "#3fb950"), ("light", "#116329")] {
        let at = svg(&doc, 1, theme);
        let after = svg(&doc, 3, theme);
        assert!(
            at.contains(&format!("fill=\"{sign}\"")),
            "{theme}: no sign:\n{at}"
        );
        assert!(!after.contains(sign), "{theme}: a sign at step 3");
        // One more background: the cell of the `+`.
        let (a, b) = (rect_fills(&at), rect_fills(&after));
        assert_eq!(a.len(), b.len() + 1, "{theme}: {a:?} vs {b:?}");
    }
}

#[test]
fn theme_overrides_set_the_color_of_the_sign() {
    let text = std::fs::read_to_string(fixture("render/patch_diff/borrow.tiza")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["theme_overrides"] = serde_json::json!({"diff_add": "#00ff00"});
    let doc = scratch("diff-override").join("doc.tiza");
    std::fs::write(&doc, v.to_string()).unwrap();
    assert!(svg(&doc, 1, "dark").contains("fill=\"#00ff00\""));
}
