//! Enter effects (experimental): the order in which the stroke of an
//! annotation is drawn, and the same frame with the record of the effects
//! on or off.
//!
//! - A box is drawn clockwise from its top-left corner.
//! - An underline is drawn from left to right.
//! - An arrow is drawn from its source to its target.
//! - The layout that records the effects draws the same frame as the
//!   layout without them, for every example and fixture at every step.

use std::collections::BTreeMap;

use tiza_core::canvas::Canvas;
use tiza_core::effects::{Effects, Plan};
use tiza_core::layout::{LayoutOptions, layout, layout_traced};
use tiza_qa::{example, fixture};

fn open(path: &std::path::Path) -> tiza_core::Document {
    tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document
}

fn options(step: u32, width: usize) -> LayoutOptions {
    LayoutOptions {
        width,
        step,
        ..Default::default()
    }
}

/// The final frame of `step` and, for each cell that the effect hides at
/// first, the moment (ms) from which it shows as in the final frame.
fn reveal(doc: &tiza_core::Document, step: u32) -> (Canvas, BTreeMap<(usize, usize), u64>) {
    let theme = tiza_core::Theme::dark();
    let (l, trace) = layout_traced(doc, &theme, &options(step, 100), None);
    let fx = Effects::parse("draw").unwrap();
    let plan = Plan::new(&l.canvas, &trace, fx).expect("the step has an effect");
    let fin = l.canvas.clone();
    let mut work = l.canvas.clone();
    let mut first: BTreeMap<(usize, usize), u64> = BTreeMap::new();
    for t in (0..=plan.duration_ms()).rev() {
        plan.paint(&mut work, t);
        for y in 0..fin.height {
            for x in 0..fin.width {
                if work.get(x, y) != fin.get(x, y) {
                    first.entry((x, y)).or_insert(t + 1);
                }
            }
        }
    }
    (fin, first)
}

/// Where `sym` is on row `y`.
fn xs(c: &Canvas, y: usize, sym: &str) -> Vec<usize> {
    (0..c.width)
        .filter(|&x| c.get(x, y).is_some_and(|cell| cell.sym == sym))
        .collect()
}

fn row_of(c: &Canvas, text: &str) -> usize {
    (0..c.height)
        .find(|&y| c.row_text(y).contains(text))
        .unwrap_or_else(|| panic!("no row with {text:?}:\n{}", c.to_plain_text()))
}

/// When cell (`x`, `y`) shows as in the final frame: 0 when it shows from
/// the start.
fn at(first: &BTreeMap<(usize, usize), u64>, x: usize, y: usize) -> u64 {
    first.get(&(x, y)).copied().unwrap_or(0)
}

fn nondecreasing(times: &[u64], what: &str) {
    assert!(
        times.windows(2).all(|w| w[0] <= w[1]),
        "{what} is not drawn in order: {times:?}"
    );
    assert!(
        times.first() < times.last(),
        "{what} is drawn at once: {times:?}"
    );
}

#[test]
fn a_box_is_drawn_clockwise_from_its_top_left_corner() {
    let doc = open(&fixture("render/enter_effects/strokes.tiza"));
    let (c, first) = reveal(&doc, 1);
    let top = (0..c.height).find(|&y| !xs(&c, y, "╭").is_empty()).unwrap();
    let bottom = (0..c.height).find(|&y| !xs(&c, y, "╰").is_empty()).unwrap();
    let (lx, rx) = (xs(&c, top, "╭")[0], xs(&c, top, "╮")[0]);
    assert_eq!(at(&first, lx, top), 0, "the top-left corner comes first");
    let top_row: Vec<u64> = (lx..=rx).map(|x| at(&first, x, top)).collect();
    nondecreasing(&top_row, "the top border, left to right");
    let right: Vec<u64> = (top..=bottom).map(|y| at(&first, rx, y)).collect();
    nondecreasing(&right, "the right side, top to bottom");
    let bottom_row: Vec<u64> = (lx..=rx).rev().map(|x| at(&first, x, bottom)).collect();
    nondecreasing(&bottom_row, "the bottom border, right to left");
    let left: Vec<u64> = (top + 1..=bottom)
        .rev()
        .map(|y| at(&first, lx, y))
        .collect();
    nondecreasing(&left, "the left side, bottom to top");
    assert!(top_row.last() <= right.first() && right.last() <= bottom_row.first());
    assert!(bottom_row.last() <= left.first());
}

#[test]
fn an_underline_is_drawn_from_left_to_right() {
    let doc = open(&fixture("render/enter_effects/strokes.tiza"));
    let (c, first) = reveal(&doc, 2);
    let y = (0..c.height)
        .find(|&y| !xs(&c, y, tiza_core::layout::UNDERLINE_GLYPH).is_empty())
        .unwrap();
    let times: Vec<u64> = xs(&c, y, tiza_core::layout::UNDERLINE_GLYPH)
        .into_iter()
        .map(|x| at(&first, x, y))
        .collect();
    nondecreasing(&times, "the underline");
}

#[test]
fn an_arrow_is_drawn_from_its_source_to_its_target() {
    let doc = open(&fixture("render/enter_effects/strokes.tiza"));
    let (c, first) = reveal(&doc, 3);
    // The arrow goes from line 5 up to line 2: its stub out of line 5, its
    // rail up the lane, and its head before line 2.
    let source = row_of(&c, "println!");
    let target = row_of(&c, "String::new");
    let lane = xs(&c, source, "╰")[0];
    let head = xs(&c, target, "▶")[0];
    // The source stub ends in the column of the head.
    let out: Vec<u64> = (lane..=head).rev().map(|x| at(&first, x, source)).collect();
    nondecreasing(&out, "the stub out of the source");
    let rail: Vec<u64> = (target..=source)
        .rev()
        .map(|y| at(&first, lane, y))
        .collect();
    nondecreasing(&rail, "the rail, from the source row to the target row");
    assert!(
        out.last() <= rail.first() && rail.last() <= Some(&at(&first, head, target)),
        "source {out:?}, rail {rail:?}, head {}",
        at(&first, head, target)
    );
}

#[test]
fn the_record_of_the_effects_does_not_change_the_frame() {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in [example(""), fixture("")] {
        let mut stack = vec![dir];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "tiza") {
                    files.push(p);
                }
            }
        }
    }
    files.sort();
    let theme = tiza_core::Theme::dark();
    let mut checked = 0;
    for f in &files {
        // Fixtures of broken documents do not open: nothing to draw.
        let Ok(opened) = tiza_core::fs::open(f.to_str().unwrap()) else {
            continue;
        };
        let doc = opened.document;
        for step in 0..=doc.max_step() {
            for width in [100, 60] {
                let plain = layout(&doc, &theme, &options(step, width)).canvas;
                let (traced, _) = layout_traced(&doc, &theme, &options(step, width), None);
                assert_eq!(
                    plain.to_ansi(),
                    traced.canvas.to_ansi(),
                    "{} at step {step}, width {width}",
                    f.display()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "only {checked} frames checked");
}
