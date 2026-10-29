//! The frames of a recording: every step of the document, the
//! intermediate frames of its typed moves and how long each one
//! stays. One geometry for the whole take, so the canvases are what
//! the audience sees; the GIF encoder in `tiza-export`
//! and an MP4 muxer draw them without any further decision.

use crate::Document;
use crate::canvas::{Canvas, Cell, Sym};
use crate::layout::{LayoutOptions, StableGeometry, layout_with, stable_geometry};
use crate::theme::Theme;

/// What a take is made of.
#[derive(Debug, Clone)]
pub struct AnimOptions {
    /// Seconds each step stays on screen (the last one twice as long, so a
    /// loop does not cut the end short). A step with `auto_advance_ms`
    /// stays that long instead: the document's own timing.
    pub interval: f64,
    pub width: usize,
    pub line_numbers: bool,
    /// Start at step 0 (code only) before the first annotated step.
    pub from_zero: bool,
    /// Columns of the right margin (`--margin-cols`); `None` takes the
    /// document's `margin`.
    pub margin_cap: Option<usize>,
}

impl Default for AnimOptions {
    fn default() -> Self {
        Self {
            interval: 2.0,
            width: 100,
            line_numbers: true,
            from_zero: true,
            margin_cap: None,
        }
    }
}

/// One frame: the step it belongs to, its canvas (every frame of a take has
/// the same size) and how long it stays, in milliseconds.
#[derive(Debug, Clone)]
pub struct Frame {
    pub step: u32,
    pub canvas: Canvas,
    pub delay_ms: u64,
    /// An intermediate frame of a typing animation (the step's annotations
    /// are not drawn yet).
    pub typing: bool,
    /// An intermediate frame of an enter effect (the step's annotations
    /// come in, [`crate::effects`]).
    pub effect: bool,
}

/// Validates the seconds each step of a recording stays (`--interval`,
/// `export_image {interval}`): a finite number from 0 to one hour
/// ([`crate::model::MAX_HOLD_MS`]) (`1e308` would overflow, `1e7` would hang
/// the GIF encoder).
pub fn check_interval(secs: f64) -> Result<f64, crate::Error> {
    let max = crate::model::MAX_HOLD_MS as f64 / 1000.0;
    if secs.is_finite() && (0.0..=max).contains(&secs) {
        Ok(secs)
    } else {
        Err(crate::Error::Invalid(format!(
            "interval={secs}: use a number of seconds between 0 and {max}"
        )))
    }
}

/// Steps whose frame holds for their own `auto_advance_ms` instead of the
/// interval, for the export's summary line.
pub fn timed_steps(doc: &Document, opts: &AnimOptions) -> Vec<u32> {
    let first = if opts.from_zero { 0 } else { 1 };
    (first..=doc.max_step().max(first))
        .filter(|s| doc.auto_advance_ms(*s).is_some())
        .collect()
}

/// The frames of the take, in order, padded to the tallest one.
pub fn frames(doc: &Document, theme: &Theme, opts: &AnimOptions) -> Vec<Frame> {
    let take = Take::record(doc, theme, opts);
    let mut out = Vec::with_capacity(take.len());
    let _ = take.each(|f| {
        out.push(Frame {
            step: f.step,
            canvas: f.canvas.clone(),
            delay_ms: f.delay_ms,
            typing: f.typing,
            effect: f.effect,
        });
        Ok::<(), std::convert::Infallible>(())
    });
    out
}

/// Makes every frame of the take, in order, and lends each one to `emit`
/// with its step, how long it stays and whether it is typed or of an
/// effect: the canvases are not the same size ([`Take`] pads them).
fn generate(
    doc: &Document,
    theme: &Theme,
    opts: &AnimOptions,
    mut emit: impl FnMut(u32, &Canvas, u64, bool, bool),
) {
    let first = if opts.from_zero { 0 } else { 1 };
    let last = doc.max_step().max(first);
    // Bounded by the callers (`check_interval`); saturating anyway, so a
    // caller that forgets cannot overflow.
    let max = crate::model::MAX_HOLD_MS;
    let hold = ((opts.interval.max(0.0) * 1000.0).round() as u64).min(max);
    let options = |d: &Document, step: u32, typing: bool| LayoutOptions {
        width: opts.width,
        step,
        focus: None,
        pointer: None,
        line_numbers: opts.line_numbers,
        tab_width: 4,
        typing,
        stable: true,
        margin_cap: opts.margin_cap.or(d.margin.map(usize::from)),
    };
    // The take's geometry, once for every step's own frame. A typed frame
    // shows other code at its step, so it sizes its own.
    let geometry = stable_geometry(doc, &options(doc, first, false));
    let lay = |d: &Document, step: u32, typing: bool, g: Option<&StableGeometry>| {
        layout_with(d, theme, &options(d, step, typing), g).canvas
    };
    for step in first..=last {
        // Typed moves: the code as it is being typed, the annotations of the
        // step waiting (as in the TUI); then the finished step.
        if let Some(plan) = crate::moves::typing_plan(doc, step) {
            let mut d = doc.clone();
            d.step = step;
            // The shown file (and window) at that step, not the current one.
            let first_line = d
                .code_at(step)
                .map(|(_, f)| f)
                .unwrap_or(d.source.first_line);
            d.source.first_line = first_line;
            plan.each_frame(first_line, |code, delay, last| {
                if last {
                    return; // the full code is the step's own frame below
                }
                d.code = code;
                emit(
                    step,
                    &lay(&d, step, true, None),
                    delay.min(max),
                    true,
                    false,
                );
            });
        }
        // The document's own timing first: a named step's `auto_advance_ms` is how long its
        // frame stays, after its typed frames, as in the TUI.
        let delay = match doc.auto_advance_ms(step) {
            Some(ms) => ms.min(max),
            None if step == last => hold.saturating_mul(2),
            None => hold,
        };
        // The enter effects of the step, after its typing: the frames of
        // the effect, then the step's own frame. The first step of the
        // take shows as when the document opens, with no effect.
        let canvas = if step > first && doc.has_effect_at(step) {
            let (l, trace) = crate::layout::layout_traced(
                doc,
                theme,
                &options(doc, step, false),
                Some(&geometry),
            );
            if let Some(plan) = crate::effects::Plan::new(&l.canvas, &trace, doc) {
                plan.each_frame(&l.canvas, crate::effects::EXPORT_FRAME_MS, |c, ms| {
                    emit(step, c, ms, false, true)
                });
            }
            l.canvas
        } else {
            lay(doc, step, false, Some(&geometry))
        };
        emit(step, &canvas, delay, false, false);
    }
}

/// A take recorded once, for the encoders that go over its frames one at
/// a time, more than once (a GIF counts its colors, then writes them).
///
/// The record holds the first frame and, for each frame after it, only
/// the cells that change, at the size of the largest frame so far: a
/// frame of a long take changes about 1 % of its cells, and a canvas of
/// 300 lines is 1.3 MB. [`Take::each`] plays it back on one canvas: each
/// frame is the one of [`frames`], padded as [`same_size`] pads it.
#[derive(Debug, Clone)]
pub struct Take {
    width: usize,
    height: usize,
    base: Cell,
    frames: Vec<Recorded>,
}

#[derive(Debug, Clone)]
struct Recorded {
    step: u32,
    delay_ms: u64,
    typing: bool,
    effect: bool,
    change: Change,
}

/// What a frame changes on the canvas of the frame before.
#[derive(Debug, Clone)]
enum Change {
    /// These cells, at (x, y).
    Cells(Box<[(u32, u32, Cell)]>),
    /// Every cell of the top-left `width` columns, row after row: when
    /// most cells change (a spotlight that fades over a long file), the
    /// cells take less room than a list of changes.
    Whole { width: usize, cells: Vec<Cell> },
}

/// One frame of a [`Take`], lent to the closure of [`Take::each`].
#[derive(Debug)]
pub struct FrameRef<'a> {
    pub step: u32,
    /// Every frame of a take has the same size, [`Take::size`].
    pub canvas: &'a Canvas,
    pub delay_ms: u64,
    pub typing: bool,
    pub effect: bool,
}

impl Take {
    /// Makes the frames of the take once and records what each changes.
    pub fn record(doc: &Document, theme: &Theme, opts: &AnimOptions) -> Take {
        let mut r = Recorder::new(theme);
        generate(
            doc,
            theme,
            opts,
            |step, canvas, delay_ms, typing, effect| r.push(step, canvas, delay_ms, typing, effect),
        );
        r.finish()
    }

    /// The size of every frame: the widest and the tallest of the take.
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Plays the take back: each frame in order, on one canvas, lent to
    /// `f`. Stops at the first error of `f`. Each call gives the same
    /// frames.
    pub fn each<E>(&self, mut f: impl FnMut(FrameRef<'_>) -> Result<(), E>) -> Result<(), E> {
        let mut canvas = Canvas::new_filled(self.width, self.height, self.base.style);
        for r in &self.frames {
            match &r.change {
                Change::Cells(cells) => {
                    for (x, y, cell) in cells {
                        canvas.set_cell(*x as usize, *y as usize, cell.clone());
                    }
                }
                Change::Whole { width, cells } => {
                    for (i, cell) in cells.iter().enumerate() {
                        canvas.set_cell(i % width, i / width, cell.clone());
                    }
                }
            }
            f(FrameRef {
                step: r.step,
                canvas: &canvas,
                delay_ms: r.delay_ms,
                typing: r.typing,
                effect: r.effect,
            })?;
        }
        Ok(())
    }
}

/// Records the frames of a [`Take`], one after the other.
struct Recorder {
    base: Cell,
    /// The frame before, padded with the background to the largest size
    /// so far.
    shown: Canvas,
    frames: Vec<Recorded>,
}

impl Recorder {
    fn new(theme: &Theme) -> Recorder {
        Recorder {
            base: Cell {
                sym: Sym::SPACE,
                style: theme.base(),
            },
            shown: Canvas::new(0, 0),
            frames: Vec::new(),
        }
    }

    /// Records `canvas`. A cell is copied as it is, where [`same_size`]
    /// writes it again with [`Canvas::put`]: the same cell, since every
    /// canvas of a take is written with `put` (a wide grapheme never ends
    /// a row: `put` leaves a space there).
    fn push(&mut self, step: u32, canvas: &Canvas, delay_ms: u64, typing: bool, effect: bool) {
        let (w, h) = (
            self.shown.width.max(canvas.width),
            self.shown.height.max(canvas.height),
        );
        if (w, h) != (self.shown.width, self.shown.height) {
            self.shown = grown(&self.shown, w, h, &self.base);
        }
        let mut cells = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let now = canvas.get(x, y).unwrap_or(&self.base);
                if self.shown.get(x, y) != Some(now) {
                    cells.push((x as u32, y as u32, now.clone()));
                    self.shown.set_cell(x, y, now.clone());
                }
            }
        }
        // A change is a cell and its place (8 bytes more).
        let change = if cells.len() * (size_of::<Cell>() + 8) > w * h * size_of::<Cell>() {
            Change::Whole {
                width: w,
                cells: self.shown.cells().map(|(_, _, c)| c.clone()).collect(),
            }
        } else {
            // With no spare capacity: the record holds only what it says.
            Change::Cells(cells.into_boxed_slice())
        };
        self.frames.push(Recorded {
            step,
            delay_ms,
            typing,
            effect,
            change,
        });
    }

    fn finish(self) -> Take {
        Take {
            width: self.shown.width,
            height: self.shown.height,
            base: self.base,
            frames: self.frames,
        }
    }
}

/// `canvas` at `width` × `height`, the new cells `base`.
fn grown(canvas: &Canvas, width: usize, height: usize, base: &Cell) -> Canvas {
    let mut out = Canvas::new_filled(width, height, base.style);
    for (x, y, cell) in canvas.cells() {
        out.set_cell(x, y, cell.clone());
    }
    out
}

/// Pads every canvas with the theme background to the widest and tallest
/// of them, so the frames of a take (or `render --all-steps`) all
/// have the same pixel size.
pub fn same_size(canvases: &mut [Canvas], theme: &Theme) {
    let width = canvases.iter().map(|c| c.width).max().unwrap_or(1);
    let height = canvases.iter().map(|c| c.height).max().unwrap_or(1);
    let base = theme.base();
    for c in canvases.iter_mut() {
        if c.width < width || c.height < height {
            let mut padded = Canvas::new_filled(width, height, base);
            for (x, y, cell) in c.cells() {
                padded.put(x, y, cell.sym.as_str(), cell.style);
            }
            *c = padded;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Document;
    use serde_json::json;

    /// A frame as the tests compare it: step, canvas, delay, typed, effect.
    type Plain = (u32, Canvas, u64, bool, bool);

    /// The frames as [`frames`] made them before the record: every canvas
    /// of the take kept, then [`same_size`]; and the size of each canvas
    /// before.
    fn reference(
        doc: &Document,
        theme: &Theme,
        opts: &AnimOptions,
    ) -> (Vec<Plain>, Vec<(usize, usize)>) {
        let mut out: Vec<Plain> = Vec::new();
        generate(doc, theme, opts, |s, c, d, t, e| {
            out.push((s, c.clone(), d, t, e))
        });
        let sizes = out.iter().map(|f| (f.1.width, f.1.height)).collect();
        let mut canvases: Vec<Canvas> = out.iter().map(|f| f.1.clone()).collect();
        same_size(&mut canvases, theme);
        let frames = out
            .into_iter()
            .zip(canvases)
            .map(|((s, _, d, t, e), c)| (s, c, d, t, e))
            .collect();
        (frames, sizes)
    }

    fn played(take: &Take) -> Vec<Plain> {
        let mut out = Vec::new();
        take.each(|f| {
            out.push((f.step, f.canvas.clone(), f.delay_ms, f.typing, f.effect));
            Ok::<(), ()>(())
        })
        .unwrap();
        out
    }

    /// Where two canvases differ first: their sizes, or a cell.
    fn first_difference(a: &Canvas, b: &Canvas) -> Option<String> {
        if (a.width, a.height) != (b.width, b.height) {
            return Some(format!(
                "{}×{} against {}×{}",
                a.width, a.height, b.width, b.height
            ));
        }
        a.cells()
            .zip(b.cells())
            .find(|((_, _, x), (_, _, y))| x != y)
            .map(|((x, y, c), (_, _, d))| format!("cell ({x}, {y}): {c:?} against {d:?}"))
    }

    /// Takes with every kind of frame: typed frames narrower, wider and
    /// shorter than the steps, effects (a spotlight that fades over every
    /// line, a box that draws), a step that only waits (frames equal to
    /// the one before), a timed step, wide characters, emoji and a tab.
    fn takes() -> Vec<(&'static str, Document)> {
        let wide = |i: usize| match i % 4 {
            0 => format!("fn 関数{i}(x: &str) -> usize {{ x.len() + {i} }} // 😀"),
            1 => format!("\tlet v{i} = {i};"),
            2 => format!("    let s{i} = \"é{i}\";"),
            _ => format!("    v{i}"),
        };
        let code: Vec<String> = (1..=24).map(wide).collect();
        let typed = json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code,
            "annotations": [
                {"id": "b1", "step": 1, "type": "box", "from_line": 2, "to_line": 4, "label": "a box"},
                {"id": "n2", "step": 2, "type": "note", "line": 6, "text": "a note under line 6"},
                {"id": "c2", "step": 2, "type": "callout", "from_line": 8, "to_line": 9, "text": "a callout of the step"}
            ],
            "moves": [
                {"at_step": 2, "action": "code.patch", "op": "insert", "position": "5",
                 "content": "    let wider_than_the_take = \"中文中文中文中文\";\n\tok", "typing": "char", "speed_ms": 10},
                {"at_step": 3, "action": "code.patch", "op": "delete", "position": "10-20"},
                {"at_step": 4, "action": "code.patch", "op": "replace", "position": "3", "content": "short", "typing": "line"}
            ]
        });
        // Lines as wide as the take: the fade of the spotlight changes
        // almost every cell of a frame.
        let full: Vec<String> = (1..=24)
            .map(|i| format!("let value_{i:02} = compute(alpha, beta, gamma, delta, eps);"))
            .collect();
        let effects = json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": full,
            "steps": [{"step": 3, "auto_advance_ms": 700}],
            "annotations": [
                {"id": "n1", "step": 1, "type": "note", "line": 1, "text": "start"},
                {"id": "s2", "step": 2, "type": "spotlight", "from_line": 3, "to_line": 4, "effect": "fade"},
                {"id": "b2", "step": 2, "type": "box", "from_line": 3, "to_line": 4, "effect": "draw", "label": "drawn"},
                {"id": "h4", "step": 4, "type": "highlight", "span": {"line": 12, "start": 4, "end": 8}, "effect": "fade"}
            ],
            "moves": [{"at_step": 3, "action": "wait", "ms": 300}]
        });
        [("typed", typed), ("effects", effects)]
            .into_iter()
            .map(|(n, v)| (n, crate::fs::parse_document(&v.to_string(), None).unwrap()))
            .collect()
    }

    #[test]
    fn a_take_plays_back_the_frames_padded_as_before() {
        let themes = [
            ("dark", Theme::dark()),
            ("light", Theme::light()),
            ("terminal dark", Theme::terminal(true)),
            ("terminal light", Theme::terminal(false)),
        ];
        let opts = AnimOptions {
            width: 40,
            ..AnimOptions::default()
        };
        let (mut shrinks, mut grows, mut equal) = (false, false, false);
        for (name, doc) in takes() {
            for (theme_name, theme) in &themes {
                let (want, sizes) = reference(&doc, theme, &opts);
                let take = Take::record(&doc, theme, &opts);
                let got = played(&take);
                assert_eq!(got.len(), want.len(), "{name}, {theme_name}");
                assert_eq!(take.size(), (want[0].1.width, want[0].1.height));
                for (i, (g, w)) in got.iter().zip(&want).enumerate() {
                    assert_eq!(
                        (g.0, g.2, g.3, g.4),
                        (w.0, w.2, w.3, w.4),
                        "{name}, {theme_name}, frame {i}: step, delay, typed, effect"
                    );
                    if let Some(d) = first_difference(&g.1, &w.1) {
                        panic!("{name}, {theme_name}, frame {i}: {d}");
                    }
                }
                // Each play gives the same frames.
                let again = played(&take);
                assert!(
                    again
                        .iter()
                        .zip(&got)
                        .all(|(a, b)| first_difference(&a.1, &b.1).is_none())
                );
                // What the takes cover.
                shrinks |= sizes.windows(2).any(|p| p[1].0 < p[0].0 || p[1].1 < p[0].1);
                grows |= sizes.windows(2).any(|p| p[1].0 > p[0].0 || p[1].1 > p[0].1);
                equal |= want
                    .windows(2)
                    .any(|p| first_difference(&p[0].1, &p[1].1).is_none());
            }
        }
        assert!(
            shrinks && grows,
            "a frame smaller and a frame larger than the one before"
        );
        assert!(equal, "a frame equal to the one before");
    }

    #[test]
    fn a_frame_that_changes_most_cells_is_recorded_whole_and_plays_back() {
        let theme = Theme::light();
        let style = crate::canvas::CellStyle::new().fg(crate::canvas::Rgb(9, 9, 9));
        let canvas = |w: usize, h: usize, ch: &str| {
            let mut c = Canvas::new_filled(w, h, theme.base());
            for y in 0..h {
                c.put_str(0, y, &ch.repeat(w), style);
            }
            c
        };
        let frames = [
            canvas(6, 3, "a"),
            canvas(6, 3, "b"),  // every cell: recorded whole
            canvas(4, 2, "b"),  // smaller: the rest goes back to the background
            canvas(8, 4, "中"), // larger
            canvas(8, 4, "中"), // the same
        ];
        let mut r = Recorder::new(&theme);
        for (i, c) in frames.iter().enumerate() {
            r.push(i as u32, c, 10 * i as u64, false, false);
        }
        let take = r.finish();
        assert!(matches!(take.frames[1].change, Change::Whole { .. }));
        assert!(matches!(&take.frames[4].change, Change::Cells(c) if c.is_empty()));
        let mut want = frames.to_vec();
        same_size(&mut want, &theme);
        let got = played(&take);
        assert_eq!(got.len(), want.len());
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!((g.0, g.2), (i as u32, 10 * i as u64), "frame {i}");
            if let Some(d) = first_difference(&g.1, w) {
                panic!("frame {i}: {d}");
            }
        }
    }

    #[test]
    fn one_frame_per_step_plus_the_typed_ones_padded_to_the_tallest() {
        let mut d = Document::from_code("fn a() {}\n", "rs", None);
        d.add_move(
            serde_json::from_value(json!({
                "at_step": 1, "action": "code.patch", "op": "insert",
                "after": "fn a() {}", "content": "fn b() {}", "typing": "line", "speed_ms": 50
            }))
            .unwrap(),
        )
        .unwrap();
        d.add(crate::model::Annotation {
            id: "a1".into(),
            step: 2,
            color: None,
            effect: None,
            markers: None,
            file: None,
            shape: crate::model::Shape::Note {
                line: 1,
                text: "hello".into(),
            },
        })
        .unwrap();
        let f = frames(&d, &Theme::dark(), &AnimOptions::default());
        let steps: Vec<(u32, bool, u64)> =
            f.iter().map(|f| (f.step, f.typing, f.delay_ms)).collect();
        // step 0, one typed frame (blank line, 50 ms), step 1 (2 s), step 2 (4 s: last).
        assert_eq!(
            steps,
            vec![
                (0, false, 2000),
                (1, true, 50),
                (1, false, 2000),
                (2, false, 4000)
            ]
        );
        let h = f[0].canvas.height;
        assert!(f.iter().all(|x| x.canvas.height == h), "heights differ");
        assert!(f[3].canvas.to_plain_text().contains("hello"));
        assert!(!f[1].canvas.to_plain_text().contains("fn b()"));
        assert!(f[2].canvas.to_plain_text().contains("fn b()"));
        // `auto_advance_ms` is the step's duration, after its typing.
        d.steps = vec![crate::steps::NamedStep {
            step: 1,
            id: Some("b".into()),
            title: None,
            auto_advance_ms: Some(3000),
            unknown: Default::default(),
        }];
        let f = frames(&d, &Theme::dark(), &AnimOptions::default());
        let delays: Vec<u64> = f.iter().map(|f| f.delay_ms).collect();
        assert_eq!(delays, vec![2000, 50, 3000, 4000]);
        assert_eq!(timed_steps(&d, &AnimOptions::default()), vec![1]);
    }
}
