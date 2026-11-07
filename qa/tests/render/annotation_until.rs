//! What the take draws with the `until` of an annotation that is not a
//! spotlight. Two arrows whose steps do not meet share a lane, and two
//! boxes whose steps do not meet do not nest, so the code does not go to
//! the right for what is never on the screen together. An arrow without
//! a color keeps its color of the palette when an earlier one goes. A box
//! that goes keeps the columns of the code. Each step draws only what
//! shows at it.

use serde_json::{Value, json};
use tiza_core::Document;
use tiza_core::layout::{LayoutOptions, layout, stable_geometry};
use tiza_qa::scratch;

/// Lines 1 to 8: `fn main() {`, `let v1 = 1;` … `let v6 = 6;`, `}`.
fn code() -> Vec<String> {
    std::iter::once("fn main() {".to_string())
        .chain((1..=6).map(|i| format!("    let v{i} = {i};")))
        .chain(std::iter::once("}".to_string()))
        .collect()
}

fn open(name: &str, annotations: Value) -> Document {
    let path = scratch(name).join("doc.tiza");
    let doc = json!({"schema": 2, "source": {"language": "rs", "first_line": 1},
        "code": code(), "annotations": annotations});
    std::fs::write(&path, doc.to_string()).unwrap();
    tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document
}

fn arrow(id: &str, step: u32, from: u32, to: u32, until: Option<u32>) -> Value {
    let mut a = json!({"id": id, "step": step, "type": "arrow",
        "from": {"line": from, "col": 8}, "to": {"line": to, "col": 8}, "label": id});
    if let Some(u) = until {
        a["until"] = json!(u);
    }
    a
}

fn boxed(id: &str, step: u32, lines: (u32, u32), until: Option<u32>) -> Value {
    let mut a = json!({"id": id, "step": step, "type": "box",
        "from_line": lines.0, "to_line": lines.1});
    if let Some(u) = until {
        a["until"] = json!(u);
    }
    a
}

fn take(doc: &Document, step: u32) -> tiza_core::layout::Layout {
    layout(
        doc,
        &tiza_core::Theme::dark(),
        &LayoutOptions {
            width: 100,
            step,
            stable: true,
            ..Default::default()
        },
    )
}

#[test]
fn arrows_that_never_meet_share_a_lane() {
    let both = open(
        "until-lanes-both",
        json!([arrow("a1", 1, 2, 5, None), arrow("a2", 2, 3, 6, None)]),
    );
    let apart = open(
        "until-lanes",
        json!([arrow("a1", 1, 2, 5, Some(1)), arrow("a2", 2, 3, 6, None)]),
    );
    let lanes = |d: &Document| stable_geometry(d, &LayoutOptions::default()).lane_count;
    assert_eq!((lanes(&both), lanes(&apart)), (2, 1));
    // One lane: the code is to the left of where two lanes put it.
    assert!(take(&apart, 1).code_x < take(&both, 1).code_x);
}

#[test]
fn boxes_that_never_meet_do_not_nest() {
    let both = open(
        "until-boxes-both",
        json!([boxed("b1", 1, (2, 5), None), boxed("b2", 2, (3, 4), None)]),
    );
    let apart = open(
        "until-boxes",
        json!([
            boxed("b1", 1, (2, 5), Some(1)),
            boxed("b2", 2, (3, 4), None)
        ]),
    );
    let depth = |d: &Document| stable_geometry(d, &LayoutOptions::default()).box_depth;
    assert_eq!((depth(&both), depth(&apart)), (2, 1));
}

#[test]
fn a_box_that_goes_keeps_the_columns_of_the_code() {
    let doc = open(
        "until-box-goes",
        json!([
            boxed("b1", 1, (2, 5), Some(2)),
            boxed("b2", 3, (6, 7), None)
        ]),
    );
    let x: Vec<usize> = (1..=3).map(|s| take(&doc, s).code_x).collect();
    assert!(x.iter().all(|c| *c == x[0]), "{x:?}");
    // And the box is not drawn after its `until`.
    let rows = |s| {
        let l = take(&doc, s);
        (0..l.canvas.height)
            .map(|y| l.canvas.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(rows(2).matches('╭').count(), 1, "{}", rows(2));
    assert_eq!(rows(3).matches('╭').count(), 1, "{}", rows(3));
    assert_ne!(rows(2), rows(3));
}

/// The color of the first cell of `text` in the take of `step`.
fn color_of(doc: &Document, step: u32, text: &str) -> Option<tiza_core::canvas::Rgb> {
    let l = take(doc, step);
    (0..l.canvas.height).find_map(|y| {
        let row: Vec<char> = l.canvas.row_text(y).chars().collect();
        let want: Vec<char> = text.chars().collect();
        let x = row.windows(want.len()).position(|w| w == want.as_slice())?;
        l.canvas.get(x, y).and_then(|c| c.style.fg)
    })
}

#[test]
fn an_arrow_keeps_its_color_when_an_earlier_one_goes() {
    let doc = open(
        "until-color",
        json!([
            arrow("one", 1, 2, 3, Some(1)),
            arrow("two", 1, 5, 7, None),
            arrow("three", 2, 4, 5, None)
        ]),
    );
    let at_1 = color_of(&doc, 1, "two").expect("two at step 1");
    let at_2 = color_of(&doc, 2, "two").expect("two at step 2");
    assert_eq!(at_1, at_2);
    assert_ne!(
        color_of(&doc, 2, "three"),
        Some(at_2),
        "three has a color of its own"
    );
}

#[test]
fn a_step_that_types_does_not_draw_one_that_went() {
    // The frames of a typed step (the GIF and the TUI) draw the
    // annotations of the steps before it that still show.
    let doc = open(
        "until-typing",
        json!([
            {"id": "gone", "step": 1, "type": "note", "line": 2, "text": "gone note", "until": 1},
            {"id": "kept", "step": 1, "type": "note", "line": 4, "text": "kept note"}
        ]),
    );
    let l = layout(
        &doc,
        &tiza_core::Theme::dark(),
        &LayoutOptions {
            width: 100,
            step: 2,
            typing: true,
            stable: true,
            ..Default::default()
        },
    );
    let text: String = (0..l.canvas.height)
        .map(|y| l.canvas.row_text(y) + "\n")
        .collect();
    assert!(text.contains("kept note"), "{text}");
    assert!(!text.contains("gone note"), "{text}");
}
