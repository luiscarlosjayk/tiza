//! The frames of a recording (t-120): every step of the document, the
//! intermediate frames of its typed moves (t-098) and how long each one
//! stays. One geometry for the whole take (t-111) and masks on (t-117), so
//! the canvases are what the audience sees; the GIF encoder in `tiza-export`
//! and an MP4 muxer draw them without any further decision.

use crate::Document;
use crate::canvas::Canvas;
use crate::layout::{LayoutOptions, layout};
use crate::theme::Theme;

/// What a take is made of.
#[derive(Debug, Clone)]
pub struct AnimOptions {
    /// Seconds each step stays on screen (the last one twice as long, so a
    /// loop does not cut the end short). A step with `auto_advance_ms`
    /// (t-099) stays that long instead: the document's own timing.
    pub interval: f64,
    pub width: usize,
    pub line_numbers: bool,
    /// Start at step 0 (code only) before the first annotated step.
    pub from_zero: bool,
}

impl Default for AnimOptions {
    fn default() -> Self {
        Self {
            interval: 2.0,
            width: 100,
            line_numbers: true,
            from_zero: true,
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
}

/// Steps whose frame holds for their own `auto_advance_ms` instead of the
/// interval (t-099), for the export's summary line.
pub fn timed_steps(doc: &Document, opts: &AnimOptions) -> Vec<u32> {
    let first = if opts.from_zero { 0 } else { 1 };
    (first..=doc.max_step().max(first))
        .filter(|s| doc.auto_advance_ms(*s).is_some())
        .collect()
}

/// The frames of the take, in order, padded to the tallest one.
pub fn frames(doc: &Document, theme: &Theme, opts: &AnimOptions) -> Vec<Frame> {
    let first = if opts.from_zero { 0 } else { 1 };
    let last = doc.max_step().max(first);
    let hold = (opts.interval.max(0.0) * 1000.0).round() as u64;
    let lay = |d: &Document, step: u32, typing: bool| {
        layout(
            d,
            theme,
            &LayoutOptions {
                width: opts.width,
                step,
                focus: None,
                pointer: None,
                line_numbers: opts.line_numbers,
                tab_width: 4,
                typing,
                redact: true,
                stable: true,
                margin_cap: d.margin.map(usize::from),
            },
        )
        .canvas
    };
    let mut out: Vec<Frame> = Vec::new();
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
            let typed = plan.frames(first_line);
            let n = typed.len();
            for (i, (code, delay)) in typed.into_iter().enumerate() {
                if i + 1 == n {
                    break; // the full code is the step's own frame below
                }
                d.code = code;
                out.push(Frame {
                    step,
                    canvas: lay(&d, step, true),
                    delay_ms: delay,
                    typing: true,
                });
            }
        }
        // The document's own timing first (t-099, decided by Luis
        // 2026-10-18): a named step's `auto_advance_ms` is how long its
        // frame stays, after its typed frames, as in the TUI.
        let delay = match doc.auto_advance_ms(step) {
            Some(ms) => ms,
            None if step == last => hold * 2,
            None => hold,
        };
        out.push(Frame {
            step,
            canvas: lay(doc, step, false),
            delay_ms: delay,
            typing: false,
        });
    }
    // Same size for every frame: pad with the theme background.
    let mut canvases: Vec<Canvas> = out.iter().map(|f| f.canvas.clone()).collect();
    same_size(&mut canvases, theme);
    for (f, c) in out.iter_mut().zip(canvases) {
        f.canvas = c;
    }
    out
}

/// Pads every canvas with the theme background to the widest and tallest
/// of them, so the frames of a take (or `render --all-steps`, t-101) all
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
        // t-099: `auto_advance_ms` is the step's duration, after its typing.
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
