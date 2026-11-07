//! The signs of a `code.patch` move with `diff` in an export: the `+` in
//! the `diff_add` color of the theme and the `−` of a removed line in its
//! `diff_del` color, each on a cell with a tint of its color, at the step
//! of the move only. A `theme_overrides` sets the colors.

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
    for (theme, add, del) in [
        ("dark", "#3fb950", "#ff7b72"),
        ("light", "#116329", "#a40e26"),
    ] {
        let at = svg(&doc, 1, theme);
        let after = svg(&doc, 3, theme);
        for sign in [add, del] {
            assert!(
                at.contains(&format!("fill=\"{sign}\"")),
                "{theme}: no sign {sign}:\n{at}"
            );
            assert!(!after.contains(sign), "{theme}: a sign at step 3");
        }
        // Two more backgrounds: the cell of the `+` and the cell of the `−`.
        let (a, b) = (rect_fills(&at), rect_fills(&after));
        assert_eq!(a.len(), b.len() + 2, "{theme}: {a:?} vs {b:?}");
    }
}

#[test]
fn theme_overrides_set_the_color_of_the_sign() {
    let text = std::fs::read_to_string(fixture("render/patch_diff/borrow.tiza")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["theme_overrides"] = serde_json::json!({"diff_add": "#00ff00", "diff_del": "#ff8080"});
    let doc = scratch("diff-override").join("doc.tiza");
    std::fs::write(&doc, v.to_string()).unwrap();
    let at = svg(&doc, 1, "dark");
    assert!(at.contains("fill=\"#00ff00\""), "{at}");
    assert!(at.contains("fill=\"#ff8080\""), "{at}");
}

/// The `(text, fill)` of each run of code on the row of the first run that
/// holds `needle`, from the left: the line number and the sign of a diff
/// are left out.
fn row_runs(svg: &str, needle: &str) -> Vec<(String, String)> {
    let runs: Vec<(String, String, String)> = svg
        .split("<text ")
        .skip(1)
        .filter_map(|t| {
            let attr = |k: &str| {
                Some(
                    t.split(&format!("{k}=\""))
                        .nth(1)?
                        .split('"')
                        .next()?
                        .to_string(),
                )
            };
            let text = t.split('>').nth(1)?.split("</text").next()?.to_string();
            Some((attr("y")?, attr("fill")?, text))
        })
        .collect();
    let y = runs
        .iter()
        .find(|(_, _, t)| t.contains(needle))
        .unwrap_or_else(|| panic!("no {needle:?}:\n{svg}"))
        .0
        .clone();
    runs.into_iter()
        .filter(|(ry, _, t)| {
            let number = !t.trim().is_empty() && t.trim().chars().all(|c| c.is_ascii_digit());
            *ry == y && !number && !matches!(t.as_str(), "+" | "\u{2212}")
        })
        .map(|(_, f, t)| (t, f))
        .collect()
}

#[test]
fn a_removed_line_has_the_colors_of_its_own_code() {
    let doc = fixture("render/patch_diff/borrow.tiza");
    // `take(s);` is code at step 0 and a removed line at step 1: the same
    // colors, not those of the line that took its place.
    let before = row_runs(&svg(&doc, 0, "dark"), "take");
    let removed = row_runs(&svg(&doc, 1, "dark"), "take");
    let text = |runs: &[(String, String)]| runs.iter().map(|r| r.0.as_str()).collect::<String>();
    assert_eq!(text(&removed).trim(), "take(s);");
    let colors = |runs: &[(String, String)]| {
        runs.iter()
            .filter(|r| !r.0.trim().is_empty())
            .map(|r| (r.0.trim().to_string(), r.1.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(colors(&removed), colors(&before));
}

#[test]
fn a_removed_line_goes_dim_with_its_owner_and_its_sign_never() {
    let text = std::fs::read_to_string(fixture("render/patch_diff/borrow.tiza")).unwrap();
    let with_spot = |name: &str, from: u32, to: u32| {
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        v["annotations"] = serde_json::json!([{"id": "sp", "step": 1, "type": "spotlight",
            "from_line": from, "to_line": to}]);
        let doc = scratch(name).join("doc.tiza");
        std::fs::write(&doc, v.to_string()).unwrap();
        svg(&doc, 1, "dark")
    };
    let fills = |svg: &str, needle: &str| -> Vec<String> {
        row_runs(svg, needle)
            .into_iter()
            .filter(|r| !r.0.trim().is_empty())
            .map(|r| r.1)
            .collect()
    };
    // Lines 1-2 lit: the removed line over line 3 is dim, as line 3.
    let dim = with_spot("diff-dim-out", 1, 2);
    assert_eq!(fills(&dim, "take"), fills(&dim, "show"), "{dim}");
    assert_ne!(fills(&dim, "take"), fills(&dim, "let"), "{dim}");
    // Its sign keeps its color.
    assert!(dim.contains("fill=\"#ff7b72\""), "{dim}");
    // Line 3 lit: the removed line over it is lit too.
    let lit = with_spot("diff-dim-in", 3, 3);
    let plain = svg(&fixture("render/patch_diff/borrow.tiza"), 1, "dark");
    assert_eq!(fills(&lit, "take"), fills(&plain, "take"), "{lit}");
    assert_ne!(fills(&lit, "take"), fills(&lit, "println"), "{lit}");
}
