//! Enter effects (experimental): how the annotations of a step come in
//! when the step goes forward. The layout never changes: an effect only
//! decides which cells of the final drawing show at each moment and how
//! their colours mix.
//!
//! - `draw`: the stroke of each new annotation (a box or callout border,
//!   an underline, an arrow) is drawn cell by cell along its path; the code
//!   that it recolors fades in with it; the text of a callout or a label
//!   fades in after every stroke of the step.
//! - `dim` (`fade`): every cell that a new spotlight dims (code, or an
//!   annotation that the spotlight dims) goes from its colour to the
//!   dimmed colour. The layout finds these cells by itself: they are the
//!   cells with another style without the spotlights of the step.
//!
//! A traced layout ([`crate::layout::layout_traced`]) records what each
//! annotation of the step paints. [`Plan::new`] turns that record into a
//! plan once per step; [`Plan::paint`] then only copies or mixes cells.
//! The annotations of one step come in at the same time.

use crate::canvas::{Canvas, Cell, CellStyle, Part, Trace};
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
/// Time between two frames of an effect on the screen (about 30 fps).
pub const SCREEN_FRAME_MS: u64 = 33;
/// Time between two frames of an effect in a GIF, WebP or video (a
/// multiple of the 10 ms unit of a GIF).
pub const EXPORT_FRAME_MS: u64 = 40;

/// How the annotations of a step come in.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Enter {
    /// At once, as without effects.
    None,
    /// Strokes drawn along their path, then the text fades in.
    Draw,
}

/// How the dim of a spotlight comes in.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Dim {
    /// At once, as without effects.
    None,
    /// The dimmed code fades to the dimmed colour.
    Fade,
}

/// The `effects` of a document (experimental), and of the `--effects`
/// flag, `:effects` and `effects` in `[defaults]`. A key that is not set
/// takes the value of the next level (flag, document, config file); with
/// none, there is no effect.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub struct Effects {
    /// How the annotations of a step come in: `draw` or `none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enter: Option<Enter>,
    /// How the dim of a spotlight comes in: `fade` or `none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dim: Option<Dim>,
}

impl Effects {
    /// No effect, with both keys set (it hides the next level).
    pub const OFF: Effects = Effects {
        enter: Some(Enter::None),
        dim: Some(Dim::None),
    };

    /// Each key not set here takes the value of `other`.
    pub fn or(self, other: Effects) -> Effects {
        Effects {
            enter: self.enter.or(other.enter),
            dim: self.dim.or(other.dim),
        }
    }

    pub fn draw(&self) -> bool {
        self.enter == Some(Enter::Draw)
    }

    pub fn fade(&self) -> bool {
        self.dim == Some(Dim::Fade)
    }

    /// Whether any effect is on.
    pub fn any(&self) -> bool {
        self.draw() || self.fade()
    }

    /// The words of `--effects`, `:effects` and `[defaults] effects`:
    /// `draw`, `dim`, `draw,dim`, `on` (both) or `off` (none). The keys
    /// that a list does not name are off.
    pub fn parse(text: &str) -> Result<Effects, String> {
        let text = text.trim();
        match text {
            "on" | "all" => {
                return Ok(Effects {
                    enter: Some(Enter::Draw),
                    dim: Some(Dim::Fade),
                });
            }
            "off" | "none" => return Ok(Effects::OFF),
            _ => {}
        }
        let mut fx = Effects::OFF;
        for word in text.split(',').map(str::trim) {
            match word {
                "draw" => fx.enter = Some(Enter::Draw),
                "dim" | "fade" => fx.dim = Some(Dim::Fade),
                other => {
                    return Err(format!(
                        "unknown effect «{other}»: use draw, dim, draw,dim, on or off"
                    ));
                }
            }
        }
        Ok(fx)
    }

    /// The words of [`Effects::parse`] that give these effects.
    pub fn label(&self) -> &'static str {
        match (self.draw(), self.fade()) {
            (true, true) => "draw,dim",
            (true, false) => "draw",
            (false, true) => "dim",
            (false, false) => "off",
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
    /// The plan of the step that `canvas` (a traced layout) draws, or
    /// `None` when nothing of it comes in with an effect.
    pub fn new(canvas: &Canvas, trace: &Trace, fx: Effects) -> Option<Plan> {
        let width = canvas.width;
        let mut changes: Vec<Change> = Vec::new();
        let xy = |i: usize| (i % width.max(1), i / width.max(1));
        if fx.draw() {
            // Strokes: in the order of their path, each annotation at its
            // own speed, all of them at once.
            let mut stroke_end = 0;
            let mut timed: Vec<Option<u64>> = vec![None; trace.ids.len()];
            for (ann, slot) in timed.iter_mut().enumerate() {
                let mut cells: Vec<(usize, usize)> = trace
                    .cells
                    .iter()
                    .filter(|(_, r)| r.ann == ann && r.part == Part::Stroke)
                    .map(|(&i, _)| {
                        let (x, y) = xy(i);
                        (path_position(trace, ann, x, y).unwrap_or(i), i)
                    })
                    .collect();
                if cells.is_empty() {
                    continue;
                }
                cells.sort_unstable();
                let n = cells.len() as u64;
                let ms = (n * STROKE_MS_PER_CELL).clamp(STROKE_MIN_MS, STROKE_MAX_MS);
                *slot = Some(ms);
                stroke_end = stroke_end.max(ms);
                for (k, (_, i)) in cells.into_iter().enumerate() {
                    let at = k as u64 * ms / n;
                    let r = &trace.cells[&i];
                    changes.push(Change {
                        i,
                        before: r.before.clone(),
                        after: canvas.cell_at(i).clone(),
                        start: at,
                        end: at,
                        from: None,
                    });
                }
            }
            for (&i, r) in &trace.cells {
                let after = canvas.cell_at(i).clone();
                match r.part {
                    Part::Stroke => {}
                    // Code it recolors: with its stroke, or a fade.
                    Part::Recolor => {
                        let from = match (fx.fade(), trace.undimmed.get(&i)) {
                            (true, Some(s)) => *s,
                            _ => r.before.style,
                        };
                        changes.push(Change {
                            i,
                            before: Cell {
                                sym: after.sym.clone(),
                                style: from,
                            },
                            after,
                            start: 0,
                            end: timed[r.ann].unwrap_or(FADE_MS),
                            from: Some(from),
                        });
                    }
                    // Text: after every stroke, from its own background.
                    Part::Text => {
                        let mut from = after.style;
                        from.fg = after.style.bg.or(r.before.style.bg);
                        changes.push(Change {
                            i,
                            before: r.before.clone(),
                            after,
                            start: stroke_end,
                            end: stroke_end + TEXT_FADE_MS,
                            from: Some(from),
                        });
                    }
                }
            }
        }
        if fx.fade() {
            for (&i, &from) in &trace.undimmed {
                if fx.draw() && trace.cells.contains_key(&i) {
                    continue;
                }
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
                });
            }
        }
        if changes.is_empty() {
            return None;
        }
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
        if canvas.width != self.width || canvas.height != self.height {
            return;
        }
        for c in &self.changes {
            let cell = if t_ms >= c.end {
                c.after.clone()
            } else if t_ms < c.start {
                c.before.clone()
            } else {
                match c.from {
                    None => c.after.clone(),
                    Some(from) => {
                        let t = (t_ms - c.start) as f32 / (c.end - c.start).max(1) as f32;
                        Cell {
                            sym: c.after.sym.clone(),
                            style: mix(from, c.after.style, t),
                        }
                    }
                }
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

/// `a` mixed into `b` at `t` (0: `a`, 1: `b`). A colour of the terminal's
/// own palette (the `terminal` theme) cannot be mixed: the cell changes
/// at half of the fade instead.
fn mix(a: CellStyle, b: CellStyle, t: f32) -> CellStyle {
    let t = t.clamp(0.0, 1.0);
    let mut s = if t < 0.5 { a } else { b };
    let colour = |x: Option<crate::canvas::Rgb>, y: Option<crate::canvas::Rgb>| match (x, y) {
        (Some(x), Some(y)) if host_color(x).is_none() && host_color(y).is_none() => {
            Some(blend(y, x, t))
        }
        _ if t < 0.5 => x,
        _ => y,
    };
    s.fg = colour(a.fg, b.fg);
    s.bg = colour(a.bg, b.bg);
    s
}

/// The effects that apply: `flag` (the `--effects` flag or `:effects`),
/// else the document's, else the config file's `[defaults] effects`.
pub fn resolve(flag: Option<Effects>, doc: Option<Effects>, config: Option<Effects>) -> Effects {
    flag.unwrap_or_default()
        .or(doc.unwrap_or_default())
        .or(config.unwrap_or_default())
}
