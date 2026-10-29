//! Enter effects (experimental): each annotation has its own `effect`.
//!
//! - A box is drawn clockwise from its top-left corner, or from the first
//!   cell after its label, so that the border before the label comes
//!   last. An underline, brackets and a strike are drawn from left to
//!   right, and an arrow from its source to its target.
//! - The text of a callout comes in after the longest stroke of the step.
//! - The dim of a step fades only when a spotlight with `fade` comes in or
//!   goes out, on every cell that it dims or lights: code and annotations
//!   of earlier steps alike.
//! - An annotation without an effect is whole from the first frame, and a
//!   `draw` on a shape with no line plays no effect.
//! - The layout that records the effects draws the same frame as the
//!   layout without them, and a document without effects has no frames of
//!   an effect, for every example and fixture at every step.
//! - A screen that paints only the rows it shows, frame after frame while
//!   it scrolls, shows on them the frame of the whole plan.

use std::collections::BTreeMap;

use tiza_core::anim::{AnimOptions, frames};
use tiza_core::canvas::{Canvas, Part, Trace};
use tiza_core::effects::{FADE_MS, Plan};
use tiza_core::layout::{LayoutOptions, layout, layout_traced};
use tiza_core::{Document, Theme};
use tiza_qa::{example, fixture};

fn open(path: &std::path::Path) -> Document {
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
fn reveal(doc: &Document, step: u32) -> (Canvas, BTreeMap<(usize, usize), u64>) {
    let theme = Theme::dark();
    let (l, trace) = layout_traced(doc, &theme, &options(step, 100), None);
    let plan = Plan::new(&l.canvas, &trace, doc).expect("the step has an effect");
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

/// The cells (`x`, `y`) of `part` that annotation `id` painted, in the
/// record of a traced layout `width` cells wide.
fn painted(trace: &Trace, width: usize, id: &str, part: Part) -> Vec<(usize, usize)> {
    let Some(ann) = trace.ids.iter().position(|x| x == id) else {
        return Vec::new();
    };
    trace
        .cells
        .iter()
        .filter(|(_, r)| r.ann == ann && r.part == part)
        .map(|(&i, _)| (i % width, i / width))
        .collect()
}

/// The canvas of `plan` at `t` ms, painted over the final frame `fin`.
fn at_time(plan: &Plan, fin: &Canvas, t: u64) -> Canvas {
    let mut c = fin.clone();
    plan.paint(&mut c, t);
    c
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

/// Every `.tiza` of `examples/` and `qa/fixtures/` that opens.
fn every_document() -> Vec<(std::path::PathBuf, Document)> {
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
    files
        .into_iter()
        // Fixtures of broken documents do not open: nothing to draw.
        .filter_map(|f| {
            let doc = tiza_core::fs::open(f.to_str().unwrap()).ok()?.document;
            Some((f, doc))
        })
        .collect()
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
fn a_border_with_a_label_or_a_title_starts_after_it_and_ends_before_it() {
    // A label on the top border of a box, a title on the one of a callout:
    // the stroke starts at the first cell after the text and goes
    // clockwise, so the cells before the text (`╭─`) come last and never
    // show apart from the rest of the border. The text comes after them.
    let doc = open(&fixture("render/enter_effects/labels.tiza"));
    for (step, id) in [(1, "b1"), (2, "c1")] {
        let (l, trace) = layout_traced(&doc, &Theme::dark(), &options(step, 100), None);
        let w = l.canvas.width;
        let (c, first) = reveal(&doc, step);
        let stroke = painted(&trace, w, id, Part::Stroke);
        let top = stroke.iter().map(|&(_, y)| y).min().unwrap();
        let letters: Vec<(usize, usize)> = painted(&trace, w, id, Part::Text)
            .into_iter()
            .filter(|&(x, y)| {
                y == top && c.get(x, y).is_some_and(|cell| !cell.sym.trim().is_empty())
            })
            .collect();
        let text_x = letters
            .iter()
            .map(|&(x, _)| x)
            .min()
            .expect("a label on the top border");
        let (before, rest): (Vec<_>, Vec<_>) =
            stroke.iter().partition(|&&(x, y)| y == top && x < text_x);
        assert_eq!(before.len(), 2, "{id}: {before:?} before the text");
        let after = rest
            .iter()
            .filter(|&&(x, y)| y == top && x > text_x)
            .map(|&(x, _)| x)
            .min()
            .unwrap();
        assert_eq!(
            at(&first, after, top),
            0,
            "{id}: the stroke starts after the text"
        );
        let rest_end = rest.iter().map(|&(x, y)| at(&first, x, y)).max().unwrap();
        for &(x, y) in &before {
            assert!(
                at(&first, x, y) >= rest_end,
                "{id}: ({x}, {y}) before the text shows at {} ms, before the rest of the border ends at {rest_end} ms",
                at(&first, x, y)
            );
        }
        let before_end = before.iter().map(|&(x, y)| at(&first, x, y)).max().unwrap();
        for &(x, y) in &letters {
            assert!(
                at(&first, x, y) > before_end,
                "{id}: the text comes before the border closes"
            );
        }
    }
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
fn brackets_and_strike_are_drawn_from_left_to_right() {
    let doc = open(&fixture("render/enter_effects/strokes.tiza"));
    let theme = Theme::dark();
    for (step, id) in [(4, "h2"), (5, "h3")] {
        let (l, trace) = layout_traced(&doc, &theme, &options(step, 100), None);
        let mut cells = painted(&trace, l.canvas.width, id, Part::Stroke);
        assert!(cells.len() >= 2, "{id} has no stroke: {cells:?}");
        cells.sort_by_key(|&(x, y)| (x, y));
        let (_, first) = reveal(&doc, step);
        let times: Vec<u64> = cells.iter().map(|&(x, y)| at(&first, x, y)).collect();
        nondecreasing(&times, id);
    }
}

#[test]
fn the_text_of_a_callout_comes_in_after_the_longest_stroke_of_the_step() {
    let doc = open(&fixture("render/enter_effects/strokes.tiza"));
    let (l, trace) = layout_traced(&doc, &Theme::dark(), &options(6, 100), None);
    let w = l.canvas.width;
    let border = painted(&trace, w, "b2", Part::Stroke);
    let callout = painted(&trace, w, "c1", Part::Stroke);
    assert!(
        border.len() > callout.len(),
        "the border of b2 ({}) must have more cells than the one of c1 ({})",
        border.len(),
        callout.len()
    );
    let (c, first) = reveal(&doc, 6);
    let t_box = border.iter().map(|&(x, y)| at(&first, x, y)).max().unwrap();
    let letters: Vec<(usize, usize)> = painted(&trace, w, "c1", Part::Text)
        .into_iter()
        .filter(|&(x, y)| c.get(x, y).is_some_and(|cell| !cell.sym.trim().is_empty()))
        .collect();
    assert!(!letters.is_empty(), "the callout has no text");
    for (x, y) in letters {
        assert!(
            at(&first, x, y) > t_box,
            "the letter at ({x}, {y}) is whole at {} ms, before the border of b2 ends at {t_box} ms",
            at(&first, x, y)
        );
    }
}

#[test]
fn the_dim_fades_in_on_every_cell_that_the_spotlight_dims() {
    // A spotlight with ids at step 2 dims the code and the note and the
    // highlight of step 1.
    let doc = open(&fixture("render/enter_effects/spotlight_ids.tiza"));
    let theme = Theme::dark();
    let (l, trace) = layout_traced(&doc, &theme, &options(2, 100), None);
    let plan = Plan::new(&l.canvas, &trace, &doc).unwrap();
    let mut lit = doc.clone();
    lit.annotations.retain(|a| a.id != "s1");
    let without = layout(&lit, &theme, &options(2, 100)).canvas;
    let mut start = l.canvas.clone();
    plan.paint(&mut start, 0);
    assert_eq!(
        start.to_ansi(),
        without.to_ansi(),
        "at its start, the step shows as without its new spotlight"
    );
    let note = row_of(&l.canvas, "first value");
    let dimmed_note = (0..l.canvas.width)
        .any(|x| l.canvas.get(x, note).map(|c| c.style) != without.get(x, note).map(|c| c.style));
    assert!(dimmed_note, "the spotlight does not dim the note");
    let mut end = l.canvas.clone();
    plan.paint(&mut end, plan.duration_ms());
    assert_eq!(
        end.to_ansi(),
        l.canvas.to_ansi(),
        "at its end, the final frame"
    );
}

#[test]
fn an_annotation_without_an_effect_is_whole_from_the_first_frame() {
    // Step 1 of mixed.tiza: the box b1 has no effect, the arrow a1 has
    // draw and crosses the border of b1. A stable layout keeps the lane of
    // the arrow when the arrow moves to a later step, so the frame without
    // a1 has the same cells as the step.
    let doc = open(&fixture("shared/enter_effects/mixed.tiza"));
    let theme = Theme::dark();
    let opts = LayoutOptions {
        stable: true,
        ..options(1, 100)
    };
    let mut later = doc.clone();
    for a in later.annotations.iter_mut().filter(|a| a.id == "a1") {
        a.step = 9;
    }
    let only_b1 = layout(&later, &theme, &opts).canvas;
    let (l, trace) = layout_traced(&doc, &theme, &opts, None);
    let fin = l.canvas;
    assert_eq!(
        (only_b1.width, only_b1.height),
        (fin.width, fin.height),
        "the step without a1 must have the same size"
    );
    let plan = Plan::new(&fin, &trace, &doc).expect("a1 draws");
    let start = at_time(&plan, &fin, 0);
    let border = |c: &Canvas, x: usize, y: usize| {
        c.get(x, y)
            .is_some_and(|c| "╭╮╰╯─│".contains(c.sym.as_str()))
    };
    let mut crossed_border = false;
    let mut b1_cells = 0;
    for y in 0..fin.height {
        for x in 0..fin.width {
            if only_b1.get(x, y) == fin.get(x, y) {
                // Not a cell of a1: whole from the first frame.
                b1_cells += usize::from(border(&fin, x, y));
                assert_eq!(start.get(x, y), fin.get(x, y), "({x}, {y}) at 0 ms");
            } else if border(&only_b1, x, y) {
                // A cell of a1 on the border of b1: the border shows until
                // a1 comes.
                crossed_border = true;
                assert_eq!(
                    start.get(x, y).map(|c| c.sym.as_str()),
                    only_b1.get(x, y).map(|c| c.sym.as_str()),
                    "({x}, {y}), a cell of a1 on the border of b1, at 0 ms"
                );
            }
        }
    }
    assert!(
        crossed_border,
        "a1 must change a cell of the border of b1:\n{}",
        fin.to_plain_text()
    );
    assert!(b1_cells > 0, "no border of b1 is whole at 0 ms");
}

#[test]
fn a_box_with_fade_is_hidden_at_first_and_whole_at_the_end_of_the_fade() {
    let doc = open(&fixture("shared/enter_effects/mixed.tiza"));
    let (l, trace) = layout_traced(&doc, &Theme::dark(), &options(2, 100), None);
    let plan = Plan::new(&l.canvas, &trace, &doc).expect("b2 fades");
    let border = painted(&trace, l.canvas.width, "b2", Part::Stroke);
    assert!(!border.is_empty(), "b2 has no border");
    let start = at_time(&plan, &l.canvas, 0);
    let end = at_time(&plan, &l.canvas, FADE_MS);
    for (x, y) in border {
        let s = start.get(x, y).unwrap().style;
        assert_eq!(s.fg, s.bg, "({x}, {y}) shows at 0 ms");
        assert_eq!(
            end.get(x, y),
            l.canvas.get(x, y),
            "({x}, {y}) at {FADE_MS} ms"
        );
    }
}

#[test]
fn the_dim_of_a_step_fades_only_when_a_spotlight_that_changes_has_fade() {
    // A (fade) lines 1-3, steps 1-2; B (no effect) lines 6-7, steps 1-4;
    // C (fade) lines 9-10, step 3.
    let doc = open(&fixture("render/enter_effects/spots.tiza"));
    let theme = Theme::dark();
    let shown = |step: u32| layout(&doc, &theme, &options(step, 100)).canvas;
    let row =
        |c: &Canvas, y: usize| -> Vec<_> { (0..c.width).map(|x| c.get(x, y).cloned()).collect() };
    let line_row = |c: &Canvas, line: u32| {
        let text = doc.line_text(line).unwrap().trim().to_string();
        row_of(c, &text)
    };
    // (step, line, whether the line changes): the row at 0 ms is the one of
    // the step before, and at the end the one of the step.
    for (step, checks) in [
        (1, vec![(12, true)]),
        (3, vec![(12, false), (2, true), (9, true)]),
        (4, vec![(9, true)]),
    ] {
        assert!(doc.has_effect_at(step), "no effect at step {step}");
        let (l, trace) = layout_traced(&doc, &theme, &options(step, 100), None);
        let plan =
            Plan::new(&l.canvas, &trace, &doc).unwrap_or_else(|| panic!("no plan at step {step}"));
        let start = at_time(&plan, &l.canvas, 0);
        let (before, after) = (shown(step - 1), shown(step));
        for (line, changes) in checks {
            let y = line_row(&l.canvas, line);
            assert_eq!(
                row(&start, y),
                row(&before, line_row(&before, line)),
                "step {step}, line {line} at 0 ms"
            );
            assert_eq!(
                row(&l.canvas, y),
                row(&after, line_row(&after, line)),
                "step {step}, line {line} at the end"
            );
            assert_eq!(
                row(&start, y) != row(&l.canvas, y),
                changes,
                "step {step}, line {line}: does it change?"
            );
        }
    }
    // B goes out at step 5 with no effect: the dim changes at once.
    assert!(!doc.has_effect_at(5));
    let (l, trace) = layout_traced(&doc, &theme, &options(5, 100), None);
    assert!(
        Plan::new(&l.canvas, &trace, &doc).is_none(),
        "a plan at step 5"
    );
}

#[test]
fn the_record_of_the_effects_does_not_change_the_frame() {
    let theme = Theme::dark();
    let mut checked = 0;
    for (f, doc) in every_document() {
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

#[test]
fn a_document_without_effects_has_no_effect_frames() {
    let theme = Theme::dark();
    let mut checked = 0;
    for (f, doc) in every_document() {
        if doc.annotations.iter().any(|a| a.effect.is_some()) {
            continue;
        }
        for step in 0..=doc.max_step() {
            assert!(
                !doc.has_effect_at(step),
                "{} has an effect at step {step}",
                f.display()
            );
        }
        let fx = frames(&doc, &theme, &AnimOptions::default());
        assert!(
            fx.iter().all(|fr| !fr.effect),
            "{} has frames of an effect",
            f.display()
        );
        checked += 1;
    }
    assert!(checked > 50, "only {checked} documents checked");
}

#[test]
fn wide_characters_stay_whole_in_every_frame() {
    let doc = open(&fixture("render/enter_effects/cjk.tiza"));
    for step in [1, 2] {
        let (l, trace) = layout_traced(&doc, &Theme::dark(), &options(step, 100), None);
        let plan = Plan::new(&l.canvas, &trace, &doc).expect("the step draws");
        for (n, (c, _)) in plan
            .frames(&l.canvas, tiza_core::effects::EXPORT_FRAME_MS)
            .iter()
            .enumerate()
        {
            for y in 0..c.height {
                for x in 0..c.width {
                    let cell = c.get(x, y).unwrap();
                    if tiza_core::canvas::drawn_width(cell.sym.as_str()) == 2 {
                        assert!(
                            c.get(x + 1, y).is_some_and(|next| next.sym.is_empty()),
                            "step {step}, frame {n}: {:?} at ({x}, {y}) is cut",
                            cell.sym
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_typed_move_comes_before_the_effect_in_the_frames() {
    let doc = open(&fixture("render/enter_effects/typed.tiza"));
    let fx = frames(&doc, &Theme::dark(), &AnimOptions::default());
    let kinds: Vec<(bool, bool)> = fx
        .iter()
        .filter(|f| f.step == 2)
        .map(|f| (f.typing, f.effect))
        .collect();
    let last_typed = kinds.iter().rposition(|k| k.0).expect("no typed frame");
    let first_effect = kinds
        .iter()
        .position(|k| k.1)
        .expect("no frame of an effect");
    assert!(last_typed < first_effect, "{kinds:?}");
}

#[test]
fn draw_on_a_shape_without_a_stroke_plays_no_effect() {
    let doc = open(&fixture("shared/enter_effects/mixed.tiza"));
    assert!(!doc.has_effect_at(3));
    let (l, trace) = layout_traced(&doc, &Theme::dark(), &options(3, 100), None);
    assert!(Plan::new(&l.canvas, &trace, &doc).is_none());
}

#[test]
fn the_fill_of_a_box_does_not_take_the_cells_of_a_highlight() {
    // Step 4 of mixed.tiza: the underline h1 has draw, the box b3 (no
    // effect) has a fill over its line.
    let doc = open(&fixture("shared/enter_effects/mixed.tiza"));
    let (c, first) = reveal(&doc, 4);
    let y = (0..c.height)
        .find(|&y| !xs(&c, y, tiza_core::layout::UNDERLINE_GLYPH).is_empty())
        .expect("no underline");
    let times: Vec<u64> = xs(&c, y, tiza_core::layout::UNDERLINE_GLYPH)
        .into_iter()
        .map(|x| at(&first, x, y))
        .collect();
    // The first cell of a stroke shows at 0 ms: every other one later.
    assert!(times[1..].iter().all(|&t| t > 0), "{times:?}");
    nondecreasing(&times, "the underline under the fill");
}

#[test]
fn the_typing_keeps_the_dim_of_the_step_before() {
    // S lights lines 2-3 at step 2 only, with fade. At step 3 its dim
    // fades out after the typing: while the step types, line 1 stays dim.
    let doc = open(&fixture("render/enter_effects/typed.tiza"));
    let theme = Theme::dark();
    assert!(doc.dim_fades_at(3));
    let fx = frames(&doc, &theme, &AnimOptions::default());
    let at_step = |step: u32| {
        fx.iter()
            .rfind(|f| f.step == step && !f.typing && !f.effect)
            .unwrap()
            .canvas
            .clone()
    };
    let style_of_line_1 = |c: &Canvas| {
        let y = row_of(c, "fn main()");
        let x = xs(c, y, "f")[0];
        c.get(x, y).unwrap().style
    };
    let (dim, lit) = (style_of_line_1(&at_step(2)), style_of_line_1(&at_step(3)));
    assert_ne!(dim, lit, "S must dim line 1 at step 2");
    let typed: Vec<&tiza_core::anim::Frame> =
        fx.iter().filter(|f| f.step == 3 && f.typing).collect();
    assert!(!typed.is_empty(), "no typed frame at step 3");
    for f in typed {
        assert_eq!(style_of_line_1(&f.canvas), dim, "line 1 while step 3 types");
    }
}

#[test]
fn a_fade_in_the_terminal_theme_changes_at_the_half() {
    let doc = open(&fixture("shared/enter_effects/mixed.tiza"));
    let theme = Theme::terminal(true);
    let (l, trace) = layout_traced(&doc, &theme, &options(2, 100), None);
    let plan = Plan::new(&l.canvas, &trace, &doc).expect("b2 fades");
    let border = painted(&trace, l.canvas.width, "b2", Part::Stroke);
    assert!(!border.is_empty(), "b2 has no border");
    let before = at_time(&plan, &l.canvas, FADE_MS / 2 - 1);
    let half = at_time(&plan, &l.canvas, FADE_MS / 2);
    let ann = trace.ids.iter().position(|x| x == "b2").unwrap();
    for (x, y) in border {
        let i = y * l.canvas.width + x;
        let r = &trace.cells[&i];
        assert_eq!(r.ann, ann);
        assert_eq!(
            before.get(x, y),
            Some(&r.before),
            "({x}, {y}) just before the half"
        );
        assert_eq!(half.get(x, y), l.canvas.get(x, y), "({x}, {y}) at the half");
    }
}

#[test]
fn the_effect_after_a_typed_move_starts_from_the_dim_of_the_typing() {
    // flash.tiza: the spotlight S1 (no fade) stops at step 2, which types a
    // move and draws a box. The typing shows the spotlights of step 2, so
    // the first frame of the effect must too: no dim that comes back.
    let doc = open(&fixture("render/enter_effects/flash.tiza"));
    assert!(!doc.dim_fades_at(2) && doc.spots_change_at(2));
    let fx = frames(&doc, &Theme::dark(), &AnimOptions::default());
    let at_step: Vec<&tiza_core::anim::Frame> = fx.iter().filter(|f| f.step == 2).collect();
    let typed = at_step
        .iter()
        .rfind(|f| f.typing)
        .expect("no typed frame at step 2");
    let effect = at_step
        .iter()
        .find(|f| f.effect)
        .expect("no frame of an effect at step 2");
    let style_of_e = |c: &Canvas| {
        let y = row_of(c, "let s = String");
        let x = (0..c.width)
            .find(|&x| {
                c.get(x, y).is_some_and(|cell| cell.sym == "l")
                    && c.get(x + 1, y).is_some_and(|cell| cell.sym == "e")
                    && c.get(x + 2, y).is_some_and(|cell| cell.sym == "t")
            })
            .expect("no let on line 2");
        c.get(x + 1, y).unwrap().style
    };
    assert_eq!(
        style_of_e(&effect.canvas),
        style_of_e(&typed.canvas),
        "the e of let on line 2: last typed frame, then the first frame of the effect"
    );
}

#[test]
fn painting_only_the_rows_on_screen_gives_the_frame_of_the_whole_plan() {
    // As the screen does while an effect plays: each frame paints only
    // the rows in view on the same canvas, at a later time. The view
    // scrolls one row each frame, down then up, so rows come into view
    // after the effect started.
    let theme = Theme::dark();
    let (mut checked, mut in_motion) = (0, 0);
    for (f, doc) in every_document() {
        if !doc.annotations.iter().any(|a| a.effect.is_some()) {
            continue;
        }
        for step in 0..=doc.max_step() {
            let (l, trace) = layout_traced(&doc, &theme, &options(step, 100), None);
            let Some(plan) = Plan::new(&l.canvas, &trace, &doc) else {
                continue;
            };
            let fin = l.canvas.clone();
            let mut work = fin.clone();
            let view = (fin.height / 3).max(1);
            let last_top = fin.height - view;
            for (k, t) in (0..=plan.duration_ms() + 16).step_by(16).enumerate() {
                let turn = k % (2 * last_top).max(1);
                let top = turn.min(2 * last_top - turn);
                plan.paint_rows(&mut work, t, top..top + view);
                let want = at_time(&plan, &fin, t);
                for y in top..top + view {
                    for x in 0..fin.width {
                        assert_eq!(
                            work.get(x, y),
                            want.get(x, y),
                            "{} step {step}: cell ({x}, {y}) at {t} ms, rows {top}..{}",
                            f.display(),
                            top + view
                        );
                        if want.get(x, y) != fin.get(x, y) {
                            in_motion += 1;
                        }
                    }
                }
            }
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} steps with an effect checked");
    // The rows in view showed the effect, not only the final frame.
    assert!(in_motion > 1000, "only {in_motion} cells in motion checked");
}
