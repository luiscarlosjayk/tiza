//! Enter effects (experimental): how an annotation comes in when the step
//! goes forward. The author sets the effect of each annotation with its
//! `effect` field. The layout never changes: an effect only decides which
//! cells of the final drawing show at each moment and how their colours
//! mix.
//!
//! - `draw`: the stroke of the annotation (a box or callout border, an
//!   underline, brackets, a strike, an arrow) is drawn cell by cell along
//!   its path. The code that it recolors fades in with it. Its text fades
//!   in after the longest stroke of the step.
//! - `fade`: every cell of the annotation fades in, its text too. On a
//!   spotlight, the dim of the step fades in and out: every cell that
//!   keeps its symbol and gets another style when the spotlights of the
//!   step change, code or annotation alike.
//!
//! A traced layout ([`crate::layout::layout_traced`]) records what each
//! annotation of the step paints, over the dim of the step before.
//! [`Plan::new`] turns that record into a plan once per step;
//! [`Plan::paint`] then only copies or mixes cells, and
//! [`Plan::paint_rows`] does it only on the rows that a screen shows. The
//! annotations of one step come in at the same time; one without an effect
//! is whole from the first frame.

use crate::canvas::{Canvas, Cell, CellStyle, Part, Trace, Traced, is_crossing};
use crate::model::Document;
use crate::theme::{blend, host_color};

/// Shortest time a stroke takes to draw, in milliseconds.
pub const STROKE_MIN_MS: u64 = 250;
/// Longest time a stroke takes to draw, in milliseconds.
pub const STROKE_MAX_MS: u64 = 500;
/// Time for each cell of a stroke, between the two bounds above.
pub const STROKE_MS_PER_CELL: u64 = 12;
/// Time of a fade: the dim of a spotlight, a recolor with no stroke.
pub const FADE_MS: u64 = 240;
/// Time the text of a callout or a label takes to fade in.
pub const TEXT_FADE_MS: u64 = 200;
/// The frames a second of an effect on the screen when `effect_fps` in
/// `[defaults]` of config.toml does not set them.
pub const SCREEN_FPS: u16 = 60;
/// The frames a second that `effect_fps` takes.
pub const SCREEN_FPS_VALUES: [u16; 2] = [30, 60];

/// The time between two frames of an effect on the screen at `fps`
/// frames a second, to the microsecond: 33,333 µs at 30 and 16,667 µs at
/// 60. A whole number of milliseconds would not give 30 or 60.
pub fn screen_frame_period(fps: u16) -> std::time::Duration {
    let fps = u64::from(fps.max(1));
    std::time::Duration::from_micros((1_000_000 + fps / 2) / fps)
}
/// Time between two frames of an effect in a GIF, WebP or video (a
/// multiple of the 10 ms unit of a GIF).
pub const EXPORT_FRAME_MS: u64 = 40;

/// The enter effect of an annotation (experimental).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    /// The stroke is drawn along its path, then the text fades in. Only
    /// a box, a callout, an arrow, or a highlight or mark with emphasis
    /// underline, brackets or strike has a stroke.
    Draw,
    /// The annotation fades in. On a spotlight, the dim fades in and out.
    Fade,
}

impl Effect {
    /// The value of `effect=` in a command, as the other keys take theirs:
    /// in any case, with blanks around it. `Some(None)` for `none`, which
    /// removes the effect; `None` for an unknown value.
    pub fn parse(s: &str) -> Option<Option<Effect>> {
        match s.trim().to_ascii_lowercase().as_str() {
            "draw" => Some(Some(Effect::Draw)),
            "fade" => Some(Some(Effect::Fade)),
            "none" => Some(None),
            _ => None,
        }
    }
}

/// How one cell changes during an effect.
#[derive(Debug, Clone)]
struct Change {
    /// Index of the cell (`y * width + x`).
    i: usize,
    /// The cell before `start`.
    before: Cell,
    /// The cell from `end` on.
    after: Cell,
    start: u64,
    end: u64,
    /// The style the cell fades from, between `start` and `end` (with the
    /// symbol of `after`). `None`: it changes at once, at `start`.
    from: Option<CellStyle>,
    /// When the symbol of `after` comes, if later than `start`: until then
    /// the cell keeps the symbol of `before`. The text of a strike fades in
    /// along the whole stroke, and its mark comes when the line comes.
    sym_at: Option<u64>,
}

/// The effect of one step: what each changed cell shows at each moment.
#[derive(Debug, Clone)]
pub struct Plan {
    width: usize,
    height: usize,
    changes: Vec<Change>,
    duration_ms: u64,
}

impl Plan {
    /// The plan of the step that `canvas` (a traced layout of `doc`)
    /// draws, or `None` when nothing of it comes in with an effect.
    pub fn new(canvas: &Canvas, trace: &Trace, doc: &Document) -> Option<Plan> {
        let width = canvas.width;
        let xy = |i: usize| (i % width.max(1), i / width.max(1));
        // The effect of each annotation that painted something: a `draw`
        // on a shape with no line is no effect.
        let effects: Vec<Option<Effect>> = trace
            .ids
            .iter()
            .map(|id| {
                let a = doc.find_annotation(id)?;
                match a.effect? {
                    Effect::Draw if !a.shape.has_stroke() => None,
                    e => Some(e),
                }
            })
            .collect();
        let mut changes: Vec<Change> = Vec::new();
        // Strokes: in the order of their path, each annotation at its own
        // speed, all of them at once.
        let mut stroke_end = 0;
        let mut timed: Vec<Option<u64>> = vec![None; trace.ids.len()];
        for (ann, slot) in timed.iter_mut().enumerate() {
            if effects[ann] != Some(Effect::Draw) {
                continue;
            }
            // Every stroke records its path ([`Trace::paths`]). A stroke
            // with no path would go in the order of its cells.
            let has_path = trace.paths.iter().any(|(a, _)| *a == ann);
            // The cells of its stroke, also those that another annotation
            // of the step painted over after it (`own` false): they keep
            // the times of the cells that it keeps.
            let mut cells: Vec<(usize, usize, bool)> = trace
                .cells
                .iter()
                .filter_map(|(&i, r)| {
                    let own = r.ann == ann && r.part == Part::Stroke;
                    let lost = r
                        .under
                        .iter()
                        .any(|(p, part, _)| *p == ann && *part == Part::Stroke);
                    (own || lost).then_some((i, own))
                })
                .map(|(i, own)| {
                    let (x, y) = xy(i);
                    match path_position(trace, ann, x, y) {
                        Some(along) if has_path => (along, i, own),
                        _ => (i, i, own),
                    }
                })
                .collect();
            debug_assert!(
                cells.is_empty() || has_path,
                "the stroke of {} has no path",
                trace.ids[ann]
            );
            // A stroke that other annotations covered whole shows nothing:
            // it does not time the text of the step.
            if !cells.iter().any(|&(_, _, own)| own) {
                continue;
            }
            cells.sort_unstable();
            let n = cells.len() as u64;
            let ms = (n * STROKE_MS_PER_CELL).clamp(STROKE_MIN_MS, STROKE_MAX_MS);
            *slot = Some(ms);
            stroke_end = stroke_end.max(ms);
            for (k, (_, i, own)) in cells.into_iter().enumerate() {
                if !own {
                    continue;
                }
                let at = k as u64 * ms / n;
                let before = shown_before(&trace.cells[&i], &effects).clone();
                let after = canvas.cell_at(i).clone();
                // The text of a strike (its symbol with the mark): its style
                // fades in along the whole stroke, as the code that an
                // underline recolors, and the mark comes when the line comes.
                let marked = is_marked(&before, &after);
                changes.push(Change {
                    i,
                    start: if marked { 0 } else { at },
                    end: if marked { ms } else { at },
                    from: marked.then_some(before.style),
                    sym_at: marked.then_some(at),
                    before,
                    after,
                });
            }
        }
        for (&i, r) in &trace.cells {
            let after = canvas.cell_at(i).clone();
            let before = shown_before(r, &effects);
            let change = match (effects[r.ann], r.part) {
                (None, _) | (Some(Effect::Draw), Part::Stroke) => continue,
                // Code it recolors: with its stroke.
                (Some(Effect::Draw), Part::Recolor) => Change {
                    i,
                    before: Cell {
                        sym: after.sym.clone(),
                        style: before.style,
                    },
                    start: 0,
                    end: timed[r.ann].unwrap_or(FADE_MS),
                    from: Some(before.style),
                    sym_at: None,
                    after,
                },
                // Text: after the longest stroke, from its own background.
                (Some(Effect::Draw), Part::Text) => Change {
                    i,
                    before: before.clone(),
                    start: stroke_end,
                    end: stroke_end + TEXT_FADE_MS,
                    from: Some(hidden(&after, before)),
                    sym_at: None,
                    after,
                },
                // An older line that crosses the new stroke: a fade cannot
                // mix a line into a junction, so the line shows until half
                // of the fade, then the junction.
                (Some(Effect::Fade), _) if is_crossing(&before.sym, &after.sym) => Change {
                    i,
                    before: before.clone(),
                    start: FADE_MS / 2,
                    end: FADE_MS / 2,
                    from: None,
                    sym_at: None,
                    after,
                },
                // The text of a strike: it fades from the code, and its mark
                // comes at half of the fade, so the letter never goes away.
                (Some(Effect::Fade), _) if is_marked(before, &after) => Change {
                    i,
                    before: before.clone(),
                    start: 0,
                    end: FADE_MS,
                    from: Some(before.style),
                    sym_at: Some(FADE_MS / 2),
                    after,
                },
                // Every part at once: from the cell before when it keeps
                // its symbol, else from its own background.
                (Some(Effect::Fade), _) => Change {
                    i,
                    before: before.clone(),
                    start: 0,
                    end: FADE_MS,
                    from: Some(if before.sym == after.sym {
                        before.style
                    } else {
                        hidden(&after, before)
                    }),
                    sym_at: None,
                    after,
                },
            };
            changes.push(change);
        }
        // The dim of the step: the cells that no annotation of the step
        // painted.
        for (&i, &from) in &trace.dim_from {
            let after = canvas.cell_at(i).clone();
            changes.push(Change {
                i,
                before: Cell {
                    sym: after.sym.clone(),
                    style: from,
                },
                after,
                start: 0,
                end: FADE_MS,
                from: Some(from),
                sym_at: None,
            });
        }
        if changes.is_empty() {
            return None;
        }
        // By cell, so that a frame can paint only the rows it shows
        // ([`Plan::paint_rows`]). The sort is stable: two changes of one
        // cell would keep their order. The record gives a cell one change
        // at most: `trace.cells` has one entry for each cell, and
        // `dim_from` has none of them.
        changes.sort_by_key(|c| c.i);
        debug_assert!(
            changes.windows(2).all(|w| w[0].i != w[1].i),
            "a cell has two changes"
        );
        let duration_ms = changes.iter().map(|c| c.end).max().unwrap_or(0).max(1);
        Some(Plan {
            width,
            height: canvas.height,
            changes,
            duration_ms,
        })
    }

    /// How long the effect lasts, in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    /// Paints the effect at `t_ms` from its start over `canvas`, the
    /// final drawing of the step (or a canvas that an earlier call
    /// painted): only the cells that the effect changes are written.
    /// From [`Plan::duration_ms`] on, `canvas` is the final drawing.
    pub fn paint(&self, canvas: &mut Canvas, t_ms: u64) {
        self.paint_changes(&self.changes, canvas, t_ms);
    }

    /// [`Plan::paint`] on the rows `rows` of `canvas` only: the other rows
    /// keep what they show. Each change writes its cell as it is at
    /// `t_ms`, whatever an earlier call wrote there, so a row painted at
    /// `t_ms` is the same as with `paint` at `t_ms`. A screen can thus
    /// paint only the rows it shows at each frame, also a row that comes
    /// into view while the effect plays.
    pub fn paint_rows(&self, canvas: &mut Canvas, t_ms: u64, rows: std::ops::Range<usize>) {
        let first = self
            .changes
            .partition_point(|c| c.i < rows.start.saturating_mul(self.width));
        let end = self
            .changes
            .partition_point(|c| c.i < rows.end.saturating_mul(self.width));
        self.paint_changes(&self.changes[first..end.max(first)], canvas, t_ms);
    }

    fn paint_changes(&self, changes: &[Change], canvas: &mut Canvas, t_ms: u64) {
        if canvas.width != self.width || canvas.height != self.height {
            return;
        }
        for c in changes {
            let cell = if t_ms >= c.end {
                c.after.clone()
            } else if t_ms < c.start {
                c.before.clone()
            } else {
                match c.from {
                    None => c.after.clone(),
                    // A colour of the terminal's own palette (the
                    // `terminal` theme) cannot be mixed: the cell before
                    // shows until half of the fade, then the final one.
                    Some(from) if uses_host_color(from) || uses_host_color(c.after.style) => {
                        if t_ms < c.start + (c.end - c.start) / 2 {
                            c.before.clone()
                        } else {
                            c.after.clone()
                        }
                    }
                    Some(from) => {
                        let t = (t_ms - c.start) as f32 / (c.end - c.start).max(1) as f32;
                        Cell {
                            sym: c.after.sym.clone(),
                            style: mix(from, c.after.style, t),
                        }
                    }
                }
            };
            let cell = match c.sym_at {
                Some(at) if t_ms < at && t_ms < c.end => Cell {
                    sym: c.before.sym.clone(),
                    ..cell
                },
                _ => cell,
            };
            canvas.set_cell_at(c.i, cell);
        }
    }

    /// The frames of the effect for an export, every `frame_ms` from its
    /// start: (canvas, how long it stays). The final drawing is not one
    /// of them.
    pub fn frames(&self, canvas: &Canvas, frame_ms: u64) -> Vec<(Canvas, u64)> {
        let frame_ms = frame_ms.max(1);
        let mut out = Vec::new();
        let mut work = canvas.clone();
        let mut t = 0;
        while t < self.duration_ms {
            self.paint(&mut work, t);
            let next = (t + frame_ms).min(self.duration_ms);
            out.push((work.clone(), next - t));
            t = next;
        }
        out
    }
}

/// Whether `after` is `before` with a mark over it: the text of a strike
/// (U+0336 after the symbol), a symbol or the second half of a wide one.
fn is_marked(before: &Cell, after: &Cell) -> bool {
    let (b, a) = (before.sym.as_str(), after.sym.as_str());
    (!b.is_empty() && a.len() > b.len() && a.starts_with(b)) || (b.is_empty() && a.is_empty())
}

/// The cell that the first frame shows where `r` changes: the cell as
/// its annotation found it. When another annotation of the step painted
/// it before and comes in with an effect, the first frame does not show
/// that one either: then the cell as that one found it, and so on down
/// to an annotation with no effect, or to the cell before the step.
fn shown_before<'a>(r: &'a Traced, effects: &[Option<Effect>]) -> &'a Cell {
    let mut shown = &r.before;
    for (painter, _, found) in r.under.iter().rev() {
        if effects[*painter].is_none() {
            break;
        }
        shown = found;
    }
    shown
}

/// The position of cell (`x`, `y`) along the stroke paths of annotation
/// `ann`: the place on the nearest path, counted in cells from its start.
fn path_position(trace: &Trace, ann: usize, x: usize, y: usize) -> Option<usize> {
    trace
        .paths
        .iter()
        .filter(|(a, _)| *a == ann)
        .map(|(_, corners)| along(corners, x, y))
        .min()
        .map(|(_, pos)| pos)
}

/// (distance, position) of a cell to a polyline of straight segments:
/// the distance to its nearest segment, the earlier segment on a tie
/// (so a closed path starts at its first corner), and the position of
/// the nearest point of that segment from the start of the polyline.
fn along(corners: &[(usize, usize)], x: usize, y: usize) -> (usize, usize) {
    if let [(cx, cy)] = corners {
        return (x.abs_diff(*cx) + y.abs_diff(*cy), 0);
    }
    let mut best = (usize::MAX, 0);
    let mut walked = 0;
    for w in corners.windows(2) {
        let ((ax, ay), (bx, by)) = (w[0], w[1]);
        let px = x.clamp(ax.min(bx), ax.max(bx));
        let py = y.clamp(ay.min(by), ay.max(by));
        let dist = x.abs_diff(px) + y.abs_diff(py);
        if dist < best.0 {
            best = (dist, walked + ax.abs_diff(px) + ay.abs_diff(py));
        }
        walked += ax.abs_diff(bx) + ay.abs_diff(by);
    }
    best
}

/// The style of `after` with its text hidden: the foreground is its own
/// background (or the one of `before`).
fn hidden(after: &Cell, before: &Cell) -> CellStyle {
    let mut from = after.style;
    from.fg = after.style.bg.or(before.style.bg);
    from
}

/// Whether `s` has a colour of the terminal's own palette.
fn uses_host_color(s: CellStyle) -> bool {
    s.fg.is_some_and(|c| host_color(c).is_some()) || s.bg.is_some_and(|c| host_color(c).is_some())
}

/// `a` mixed into `b` at `t` (0: `a`, 1: `b`). A colour that one of them
/// does not set changes at half of the fade.
fn mix(a: CellStyle, b: CellStyle, t: f32) -> CellStyle {
    let t = t.clamp(0.0, 1.0);
    let mut s = if t < 0.5 { a } else { b };
    let colour = |x: Option<crate::canvas::Rgb>, y: Option<crate::canvas::Rgb>| match (x, y) {
        (Some(x), Some(y)) => Some(blend(y, x, t)),
        _ if t < 0.5 => x,
        _ => y,
    };
    s.fg = colour(a.fg, b.fg);
    s.bg = colour(a.bg, b.bg);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(sym: &str) -> Cell {
        Cell {
            sym: sym.into(),
            ..Cell::default()
        }
    }

    fn change(i: usize, after: &str) -> Change {
        Change {
            i,
            before: cell("."),
            after: cell(after),
            start: 0,
            end: 10,
            from: None,
            sym_at: None,
        }
    }

    #[test]
    fn painting_some_rows_keeps_the_order_of_two_changes_of_one_cell() {
        // A plan made by hand: `Plan::new` never gives a cell two changes,
        // but if it did, the last one would win in `paint_rows` as in
        // `paint`.
        let plan = Plan {
            width: 2,
            height: 3,
            changes: vec![
                change(1, "a"),
                change(2, "b"),
                change(2, "c"),
                change(5, "d"),
            ],
            duration_ms: 10,
        };
        let mut whole = Canvas::new(2, 3);
        plan.paint(&mut whole, 5);
        let mut rows = Canvas::new(2, 3);
        plan.paint_rows(&mut rows, 5, 1..2);
        assert_eq!(rows.get(0, 1), whole.get(0, 1));
        assert_eq!(rows.get(0, 1).map(|c| c.sym.as_str()), Some("c"));
        // The rows out of the range keep what they had.
        assert_eq!(rows.get(1, 0), Canvas::new(2, 3).get(1, 0));
        assert_eq!(rows.get(1, 2), Canvas::new(2, 3).get(1, 2));
        // A range past the last row paints up to it; an empty or an
        // inverted range paints nothing.
        let mut past = Canvas::new(2, 3);
        plan.paint_rows(&mut past, 5, 2..10);
        assert_eq!(past.get(1, 2).map(|c| c.sym.as_str()), Some("d"));
        assert_eq!(past.get(0, 1), Canvas::new(2, 3).get(0, 1));
        for empty in [1..1, std::ops::Range { start: 2, end: 1 }] {
            let mut none = Canvas::new(2, 3);
            plan.paint_rows(&mut none, 5, empty);
            assert_eq!(none.to_ansi(), Canvas::new(2, 3).to_ansi());
        }
    }
}
