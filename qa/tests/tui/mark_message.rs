//! The message of `:mark`, `ID: LEGEND at LN`. With a style whose legend
//! is empty or blank, it named nothing (`a1:  at L2`); it names the kind.

use serde_json::json;
use tiza_qa::Live;

#[test]
fn a_mark_with_an_empty_legend_names_its_kind() {
    let live = Live::from_code(
        "fn main() {\n    let s = 1;\n}\n",
        "rs",
        "mark-empty-legend",
    );
    // A style with an empty legend, and one with a legend of spaces.
    live.with(|a| {
        a.doc.styles = serde_json::from_value(json!({
            "plus": {"glyph": "+", "legend": ""},
            "blank": {"glyph": "~", "legend": "   "},
        }))
        .unwrap();
    });
    for (kind, id) in [("plus", "a1"), ("blank", "a2")] {
        live.command(&format!("mark {kind} 2:let s"));
        let (msg, err) = live.message().unwrap();
        assert!(!err, "{msg}");
        assert_eq!(msg, format!("{id}: {kind} at L2"));
    }
    // A legend is still the legend: `warning` for the kind `warn`.
    live.command("mark warn 2:let s");
    assert_eq!(live.message().unwrap().0, "a3: warning at L2");
}
