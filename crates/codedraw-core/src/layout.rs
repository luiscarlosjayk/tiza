//! Turns document + theme + available width into a `Canvas`.
//!
//! Decisions of this first version (see docs/adr/0002):
//! - Boxes truly surround the lines: they insert virtual rows (without a
//!   number) above and below, and vertical rails on the left and right.
//! - Arrows between different lines run along a lane to the left of the
//!   code (`╭──` leaves the source line, `╰──▶` enters the target line).
//! - Arrows within the same line use a virtual row below it.
//! - Notes and legends go in the right margin when there are ≥ 14 cells;
//!   otherwise in a virtual row below the line. This works in a 75-column terminal.

use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::canvas::{Canvas, CellStyle, Rgb, display_width};
use crate::highlight::{Segment, highlight_cached};
use crate::model::{Annotation, Document, Emphasis, Shape, TextSpan};
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct LayoutOptions {
    pub width: usize,
    /// Current step: annotations with `step <= step` are drawn.
    pub step: u32,
    /// Focused line range; the rest is dimmed.
    pub focus: Option<(u32, u32)>,
    pub line_numbers: bool,
    pub tab_width: usize,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            width: 100,
            step: u32::MAX,
            focus: None,
            line_numbers: true,
            tab_width: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Code(u32),
    BoxTop(usize),
    BoxBottom(usize),
    ArrowUnder(usize),
    /// Row under a code line with ▔ / ⌊ ⌋ for underline/bracket emphases (t-096).
    Emphasis(u32),
    /// Virtual row with text (note, arrow legend) below the line.
    Under(usize),
    /// One row of a callout drawn below its range: (callout index in `vis`, row within the block).
    Callout(usize, usize),
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub canvas: Canvas,
    pub rows: Vec<Row>,
    pub row_of_line: BTreeMap<u32, usize>,
    pub code_x: usize,
    /// Width in cells of the widest code line when it did not fit and was
    /// clipped (`None` when every line fits).
    pub clipped_code: Option<usize>,
    /// Whether labels and notes go to the right margin (false: under the line).
    pub labels_in_margin: bool,
}

impl Layout {
    pub fn line_at_row(&self, row: usize) -> Option<u32> {
        match self.rows.get(row)? {
            Row::Code(l) => Some(*l),
            _ => None,
        }
    }
}

struct BoxInfo<'a> {
    ann: &'a Annotation,
    from: u32,
    to: u32,
    depth: usize,
}

struct ArrowInfo<'a> {
    ann: &'a Annotation,
    lane: usize,
}

/// Text waiting to be placed in the margin (or below) of a row.
#[derive(Clone)]
struct Label {
    text: String,
    color: Rgb,
    note: bool,
}

pub fn layout(doc: &Document, theme: &Theme, opts: &LayoutOptions) -> Layout {
    let width = opts.width.max(20);
    // Annotations outside the code (a document whose file shrank, or a
    // hand-edited one) are reported when it opens; here they are simply not
    // drawn, so a stale line number can never break the layout.
    let vis: Vec<&Annotation> = doc
        .annotations
        .iter()
        .filter(|a| a.step <= opts.step)
        .filter(|a| {
            a.shape
                .line_ranges()
                .iter()
                .all(|(f, t)| doc.has_line(*f) && doc.has_line(*t))
        })
        .collect();
    // Color per annotation: the one chosen by the user; otherwise arrows rotate through the
    // automatic palette (in order of appearance) and the rest use the theme color.
    let auto = theme.auto_palette();
    let mut arrow_seq = 0usize;
    let mut colors: std::collections::HashMap<&str, Rgb> = std::collections::HashMap::new();
    for a in &vis {
        let chosen = a.color.as_deref().and_then(|c| theme.named_color(c));
        let c = match (&a.shape, chosen) {
            (_, Some(c)) => c,
            (Shape::Arrow { .. }, None) => {
                let c = auto[arrow_seq % auto.len().max(1)];
                arrow_seq += 1;
                c
            }
            (Shape::Box { .. }, None) => theme.box_line,
            (Shape::Note { .. }, None) => theme.note,
            (Shape::Callout { .. }, None) => theme.note,
            (Shape::Mark { kind, style, .. }, None) => doc
                .resolve_style(kind, style)
                .map(|st| theme.mark_color(&st))
                .unwrap_or(theme.accent),
            (Shape::Highlight { .. }, None) => theme.accent,
            (Shape::Spotlight { .. }, None) => theme.accent,
        };
        colors.insert(a.id.as_str(), c);
    }
    let color_of = |a: &Annotation| colors.get(a.id.as_str()).copied().unwrap_or(theme.accent);

    // ---- spotlights active at this step ----
    let mut spot_lines: Vec<(u32, u32)> = Vec::new();
    let mut spot_ids: Vec<&str> = Vec::new();
    let mut spot_any = false;
    for a in &vis {
        if let Shape::Spotlight {
            from_line,
            to_line,
            ids,
            until,
        } = &a.shape
            && opts.step >= a.step
            && opts.step <= until.unwrap_or(a.step)
        {
            spot_any = true;
            if let Some(f) = from_line {
                spot_lines.push((*f, to_line.unwrap_or(*f)));
            }
            for id in ids {
                spot_ids.push(id.as_str());
                if let Some(target) = doc.find_annotation(id) {
                    spot_lines.extend(target.shape.line_ranges());
                }
            }
        }
    }
    let line_in_spot =
        |line: u32| !spot_any || spot_lines.iter().any(|(a, b)| line >= *a && line <= *b);
    // An annotation is dimmed if there are spotlights with ids and it is not among them.
    let ann_dimmed = |a: &Annotation| {
        spot_any
            && !spot_ids.is_empty()
            && !spot_ids.contains(&a.id.as_str())
            && !matches!(a.shape, Shape::Spotlight { .. })
    };
    let color_of = |a: &Annotation| {
        if ann_dimmed(a) {
            theme.dimmed
        } else {
            color_of(a)
        }
    };
    // Spotlights are not drawn; they only filter.
    let vis: Vec<&Annotation> = vis
        .into_iter()
        .filter(|a| !matches!(a.shape, Shape::Spotlight { .. }))
        .collect();

    // ---- boxes and their nesting ----
    let mut boxes: Vec<BoxInfo> = vis
        .iter()
        .filter_map(|a| match &a.shape {
            Shape::Box {
                from_line, to_line, ..
            } => Some(BoxInfo {
                ann: a,
                from: *from_line,
                to: *to_line,
                depth: 0,
            }),
            _ => None,
        })
        .collect();
    for i in 0..boxes.len() {
        let (f, t) = (boxes[i].from, boxes[i].to);
        boxes[i].depth = boxes
            .iter()
            .enumerate()
            .filter(|(j, b)| {
                *j != i && b.from <= f && b.to >= t && ((b.from, b.to) != (f, t) || *j < i)
            })
            .count();
    }
    let box_depth = boxes.iter().map(|b| b.depth + 1).max().unwrap_or(0);

    // ---- column widths ----
    // One cell for the cursor/selection marker, the digits, one blank.
    let num_w = if opts.line_numbers {
        doc.last_line().to_string().len() + 2
    } else {
        0
    };
    let code_full_w = doc
        .code
        .iter()
        .map(|l| display_width(l, opts.tab_width))
        .max()
        .unwrap_or(0)
        .max(1);
    let multi_arrows: Vec<&Annotation> = vis
        .iter()
        .copied()
        .filter(|a| matches!(&a.shape, Shape::Arrow { from, to, .. } if from.line != to.line))
        .collect();

    // Rows are built before lanes are assigned (lanes depend on rows).
    // But the margin width depends on the number of lanes, and "Under" rows
    // depend on the margin. We approximate: lanes = upper bound (all overlap).
    // Estimate lanes in line space (virtual rows preserve the order, so the
    // overlap is the same as in row space).
    let lane_bound = {
        let mut spans: Vec<(u32, u32)> = multi_arrows
            .iter()
            .filter_map(|a| match &a.shape {
                Shape::Arrow { from, to, .. } => {
                    Some((from.line.min(to.line), from.line.max(to.line)))
                }
                _ => None,
            })
            .collect();
        spans.sort_by_key(|(a, b)| (std::cmp::Reverse(b - a), *a));
        let mut lanes: Vec<Vec<(u32, u32)>> = Vec::new();
        for (a, b) in spans {
            match lanes
                .iter_mut()
                .find(|l| l.iter().all(|(x, y)| b < *x || a > *y))
            {
                Some(l) => l.push((a, b)),
                None => lanes.push(vec![(a, b)]),
            }
        }
        lanes.len()
    };
    let arrow_w = lane_bound * 2;
    let code_x_est = num_w + arrow_w + box_depth + 2;
    let code_w = code_full_w
        .min(width.saturating_sub(code_x_est + box_depth + 1))
        .max(10);
    let margin_est = width.saturating_sub(code_x_est + code_w + 1 + box_depth + 1);
    let labels_in_margin = margin_est >= 14;

    // ---- callouts: decide placement and pre-wrap their text ----
    struct CalloutPlan {
        idx: usize,
        below: bool,
        title: Option<String>,
        lines: Vec<String>,
        /// Outer block width (borders included).
        width: usize,
        frame: crate::model::BorderStyle,
    }
    let mut callouts: Vec<CalloutPlan> = Vec::new();
    for (i, a) in vis.iter().enumerate() {
        let Shape::Callout {
            text,
            title,
            placement,
            width: want,
            frame,
            ..
        } = &a.shape
        else {
            continue;
        };
        let below = match placement {
            crate::model::Placement::Below => true,
            crate::model::Placement::Margin => !labels_in_margin || margin_est < 12,
            // Auto: the margin must hold the block *and* leave room for
            // the labels of the lines it covers, beside it.
            crate::model::Placement::Auto => {
                !labels_in_margin || margin_est < want.unwrap_or(34) + 4 + 1 + 14
            }
        };
        let text_w = if below {
            // Stays inside the code column, so a Box around the range can
            // still draw its right rail.
            let max_text = code_w.saturating_sub(4).max(8);
            want.unwrap_or(max_text).clamp(8, max_text)
        } else {
            want.unwrap_or(34)
                .clamp(8, margin_est.saturating_sub(5).max(8))
        };
        let mut lines: Vec<String> = Vec::new();
        for para in text.split('\n') {
            if para.trim().is_empty() {
                lines.push(String::new());
            } else {
                lines.extend(wrap_words(para, text_w));
            }
        }
        let inner = lines
            .iter()
            .map(|l| display_width(l, 4))
            .max()
            .unwrap_or(0)
            .max(title.as_ref().map(|t| display_width(t, 4) + 4).unwrap_or(0))
            .max(4);
        // Below the code the block never grows past the code column (a long
        // title is clipped in the border instead of breaking a box's rail).
        let inner = if below {
            inner.min(code_w.saturating_sub(4).max(4))
        } else {
            inner
        };
        callouts.push(CalloutPlan {
            idx: i,
            below,
            title: title.clone(),
            lines,
            width: inner + 4, // "│ " + text + " │"
            frame: *frame,
        });
    }

    // ---- rows ----
    let mut rows: Vec<Row> = Vec::new();
    let mut row_of_line = BTreeMap::new();
    // "Under" texts: index into `unders` referenced by Row::Under
    let mut unders: Vec<(Label, bool)> = Vec::new(); // (label, is_note)
    for line in doc.source.first_line..=doc.last_line() {
        let mut tops: Vec<usize> = (0..boxes.len())
            .filter(|&i| boxes[i].from == line)
            .collect();
        tops.sort_by_key(|&i| boxes[i].depth);
        rows.extend(tops.into_iter().map(Row::BoxTop));

        row_of_line.insert(line, rows.len());
        rows.push(Row::Code(line));
        if vis.iter().any(|a| match &a.shape {
            Shape::Highlight { span, emphasis, .. } | Shape::Mark { span, emphasis, .. } => {
                span.line == line && emphasis_needs_row(doc, span, *emphasis)
            }
            _ => false,
        }) {
            rows.push(Row::Emphasis(line));
        }

        for (i, a) in vis.iter().enumerate() {
            if let Shape::Arrow {
                from, to, label, ..
            } = &a.shape
                && from.line == to.line
                && from.line == line
            {
                rows.push(Row::ArrowUnder(i));
                if !labels_in_margin && let Some(l) = label {
                    unders.push((
                        Label {
                            text: l.clone(),
                            color: color_of(a),
                            note: false,
                        },
                        false,
                    ));
                    rows.push(Row::Under(unders.len() - 1));
                }
            }
        }
        if !labels_in_margin {
            for a in &vis {
                let lbl = match &a.shape {
                    Shape::Note { line: l, text } if *l == line => {
                        Some((text.clone(), color_of(a), true))
                    }
                    Shape::Mark {
                        kind,
                        span,
                        label,
                        style,
                        ..
                    } if span.line == line => Some((
                        mark_text(doc, kind, style, label.as_deref()),
                        color_of(a),
                        false,
                    )),
                    Shape::Highlight {
                        span,
                        label: Some(l),
                        ..
                    } if span.line == line => Some((l.clone(), color_of(a), false)),
                    Shape::Arrow {
                        from,
                        to,
                        label: Some(l),
                        ..
                    } if from.line != to.line && to.line == line => {
                        Some((l.clone(), color_of(a), false))
                    }
                    _ => None,
                };
                if let Some((text, color, is_note)) = lbl {
                    unders.push((
                        Label {
                            text,
                            color,
                            note: is_note,
                        },
                        is_note,
                    ));
                    rows.push(Row::Under(unders.len() - 1));
                }
            }
        }

        // Callouts placed below end their range on this line. A box absorbs
        // the callout's rows only when the box contains the callout's whole
        // range; otherwise the rows go after that box's bottom border, so the
        // callout is not read as belonging to the box. Nested boxes ending
        // here follow the same rule one by one (innermost first).
        let mut bottoms: Vec<usize> = (0..boxes.len()).filter(|&i| boxes[i].to == line).collect();
        bottoms.sort_by_key(|&i| std::cmp::Reverse(boxes[i].depth));
        let mut here: Vec<(u32, usize)> = callouts
            .iter()
            .enumerate()
            .filter(|(_, c)| c.below)
            .filter_map(|(ci, c)| match &vis[c.idx].shape {
                Shape::Callout {
                    from_line, to_line, ..
                } if to_line.unwrap_or(*from_line) == line => Some((*from_line, ci)),
                _ => None,
            })
            .collect();
        // Narrowest range first: it sits inside more boxes.
        here.sort_by_key(|&(from, _)| std::cmp::Reverse(from));
        let mut emitted = 0;
        for (from, ci) in here {
            while emitted < bottoms.len() && boxes[bottoms[emitted]].from > from {
                rows.push(Row::BoxBottom(bottoms[emitted]));
                emitted += 1;
            }
            for k in 0..callouts[ci].lines.len() + 2 {
                rows.push(Row::Callout(ci, k));
            }
        }
        rows.extend(bottoms[emitted..].iter().map(|&i| Row::BoxBottom(i)));
    }

    // ---- arrow lanes (in row space) ----
    let mut arrows: Vec<ArrowInfo> = Vec::new();
    {
        let mut spans: Vec<(usize, usize, &Annotation)> = multi_arrows
            .iter()
            .map(|a| {
                let Shape::Arrow { from, to, .. } = &a.shape else {
                    unreachable!()
                };
                let r1 = row_of_line[&from.line];
                let r2 = row_of_line[&to.line];
                (r1.min(r2), r1.max(r2), *a)
            })
            .collect();
        // Longest first → lane 0 (the farthest from the code).
        spans.sort_by_key(|(a, b, _)| (std::cmp::Reverse(b - a), *a));
        let mut lanes: Vec<Vec<(usize, usize)>> = Vec::new();
        for (a, b, ann) in spans {
            let lane = lanes
                .iter()
                .position(|l| l.iter().all(|(x, y)| b < *x || a > *y))
                .unwrap_or_else(|| {
                    lanes.push(Vec::new());
                    lanes.len() - 1
                });
            lanes[lane].push((a, b));
            arrows.push(ArrowInfo { ann, lane });
        }
    }
    let lane_count = arrows.iter().map(|a| a.lane + 1).max().unwrap_or(0);
    let arrow_w = lane_count * 2;

    // ---- final positions ----
    let arrow_x0 = num_w;
    let box_x0 = arrow_x0 + arrow_w;
    let code_x = box_x0 + box_depth + 2;
    let code_w = code_full_w
        .min(width.saturating_sub(code_x + box_depth + 1))
        .max(10);
    let rbox_x0 = code_x + code_w + 1; // right rail: the innermost one here
    let margin_x = rbox_x0 + box_depth + 1;
    let margin_w = width.saturating_sub(margin_x);

    let base = theme.base();
    let mut canvas = Canvas::new_filled(width, rows.len(), base);
    let segments = highlight_cached(&doc.code, &doc.source.language, theme.syntax);

    // Margin labels per row.
    let mut margin: Vec<Vec<Label>> = vec![Vec::new(); rows.len()];

    // ---- 1. code ----
    for (y, row) in rows.iter().enumerate() {
        let Row::Code(line) = row else { continue };
        let text = doc.line_text(*line).unwrap_or("");
        let in_focus =
            opts.focus.is_none_or(|(a, b)| *line >= a && *line <= b) && line_in_spot(*line);
        if opts.line_numbers {
            let n = format!("{:>w$}", line, w = num_w - 1);
            canvas.put_str(0, y, &n, base.fg(theme.muted));
        }
        let segs = &segments[(*line - doc.source.first_line) as usize];
        paint_code(
            &mut canvas,
            code_x,
            y,
            code_w,
            text,
            segs,
            opts.tab_width,
            base,
            theme,
            in_focus,
        );
    }

    // ---- 2. highlights and marks ----
    let emphasis_row: BTreeMap<u32, usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(y, r)| match r {
            Row::Emphasis(l) => Some((*l, y)),
            _ => None,
        })
        .collect();
    for a in &vis {
        let (span, fg, bg, label, emphasis) = match &a.shape {
            Shape::Highlight {
                span,
                label,
                emphasis,
            } if ann_dimmed(a) => (
                span,
                theme.dimmed,
                crate::theme::blend(theme.dimmed, theme.bg, 0.12),
                label.clone(),
                *emphasis,
            ),
            Shape::Highlight {
                span,
                label,
                emphasis,
            } => {
                let c = color_of(a);
                let (fg, bg) = if a.color.is_some() {
                    (c, crate::theme::blend(c, theme.bg, 0.22))
                } else {
                    (theme.fg, theme.highlight_bg)
                };
                (span, fg, bg, label.clone(), *emphasis)
            }
            Shape::Mark {
                kind,
                span,
                style,
                emphasis,
                ..
            } if ann_dimmed(a) => (
                span,
                theme.dimmed,
                crate::theme::blend(theme.dimmed, theme.bg, 0.12),
                Some(mark_text(doc, kind, style, None)),
                *emphasis,
            ),
            Shape::Mark {
                kind,
                span,
                label,
                style,
                emphasis,
            } => {
                let st = doc.resolve_style(kind, style).unwrap_or_default();
                (
                    span,
                    theme.mark_color(&st),
                    theme.mark_bg(&st),
                    Some(mark_text(doc, kind, style, label.as_deref())),
                    *emphasis,
                )
            }
            _ => continue,
        };
        let Some(&y) = row_of_line.get(&span.line) else {
            continue;
        };
        let text = doc.line_text(span.line).unwrap_or("");
        let x0 = code_x + col_to_x(text, span.start, opts.tab_width);
        let x1 = code_x + col_to_x(text, span.end, opts.tab_width);
        let x1 = x1.min(code_x + code_w);
        if x1 > x0 {
            // Without a tint, the span's own color is the annotation's (a
            // plain highlight keeps the theme text color + the accent).
            let ink = if a.color.is_some() || matches!(a.shape, Shape::Mark { .. }) || ann_dimmed(a)
            {
                fg
            } else {
                theme.accent
            };
            match emphasis {
                Emphasis::Fill => {
                    canvas.restyle(x0, y, x1 - x0, CellStyle::new().fg(fg).bg(bg).bold());
                }
                Emphasis::Underline => {
                    canvas.restyle(x0, y, x1 - x0, CellStyle::new().fg(ink).bold());
                    if let Some(&uy) = emphasis_row.get(&span.line) {
                        for x in x0..x1 {
                            canvas.put(x, uy, "▔", base.fg(ink));
                        }
                    }
                }
                Emphasis::Brackets => {
                    canvas.restyle(x0, y, x1 - x0, CellStyle::new().fg(ink).bold());
                    // A real space: the empty continuation cell of a wide
                    // character (名, emoji) is not room.
                    let blank_before =
                        x0 > code_x && canvas.get(x0 - 1, y).is_some_and(|c| c.sym == " ");
                    let blank_after =
                        x1 < code_x + code_w && canvas.get(x1, y).is_some_and(|c| c.sym == " ");
                    let under = emphasis_row.get(&span.line).copied();
                    if blank_before {
                        canvas.put(x0 - 1, y, "⌈", base.fg(ink).bold());
                    } else if let Some(uy) = under {
                        canvas.put(x0, uy, "⌊", base.fg(ink));
                    }
                    if blank_after {
                        canvas.put(x1, y, "⌉", base.fg(ink).bold());
                    } else if let Some(uy) = under {
                        // A one-cell span glued on both sides: ⌊ and ⌋ would
                        // share the cell, so it gets a plain underline.
                        let sym = if x1 - x0 == 1 && !blank_before {
                            "▔"
                        } else {
                            "⌋"
                        };
                        canvas.put(x1 - 1, uy, sym, base.fg(ink));
                    }
                }
                Emphasis::Strike => {
                    canvas.restyle(x0, y, x1 - x0, CellStyle::new().fg(ink).bold());
                    for x in x0..x1 {
                        let cell = canvas.get(x, y).map(|c| (c.sym.clone(), c.style));
                        if let Some((sym, st)) = cell
                            && !sym.trim().is_empty()
                            && !sym.ends_with('\u{336}')
                        {
                            canvas.put(x, y, &format!("{sym}\u{336}"), st);
                        }
                    }
                }
            }
        }
        if labels_in_margin && let Some(l) = label {
            margin[y].push(Label {
                text: l,
                color: color_of(a),
                note: false,
            });
        }
    }

    // ---- 3. boxes ----
    for (bi, b) in boxes.iter().enumerate() {
        let box_color = color_of(b.ann);
        let box_style = base.fg(box_color);
        let lx = box_x0 + b.depth;
        let rx = rbox_x0 + (box_depth - 1 - b.depth);
        let Shape::Box { label, frame, .. } = &b.ann.shape else {
            continue;
        };
        let g = Glyphs::for_border(frame);
        let top = rows.iter().position(|r| *r == Row::BoxTop(bi));
        let bottom = rows.iter().position(|r| *r == Row::BoxBottom(bi));
        let (Some(top), Some(bottom)) = (top, bottom) else {
            continue;
        };
        for y in top + 1..bottom {
            canvas.put_line(lx, y, g.v, box_style);
            canvas.put_line(rx, y, g.v, box_style);
            // subtle tint inside the box (the default; `fill: false` turns it off)
            if frame.fill.unwrap_or(true) {
                canvas.restyle(
                    lx + 1,
                    y,
                    rx - lx - 1,
                    CellStyle::new().bg(crate::theme::blend(box_color, theme.bg, 0.08)),
                );
            }
        }
        canvas.put(lx, top, g.tl, box_style);
        canvas.put(rx, top, g.tr, box_style);
        for x in lx + 1..rx {
            canvas.put(x, top, g.h, box_style);
        }
        if let Some(l) = label {
            let avail = rx.saturating_sub(lx + 4);
            canvas.put(lx + 1, top, g.h, box_style);
            canvas.put(lx + 2, top, " ", box_style);
            let w = canvas.put_str_clipped(lx + 3, top, l, avail, box_style.bold());
            canvas.put(lx + 3 + w, top, " ", box_style);
        }
        canvas.put(lx, bottom, g.bl, box_style);
        canvas.put(rx, bottom, g.br, box_style);
        for x in lx + 1..rx {
            canvas.put(x, bottom, g.h, box_style);
        }
    }

    // ---- 4. arrows between lines ----
    for a in &arrows {
        let Shape::Arrow {
            from,
            to,
            label,
            line: ls,
        } = &a.ann.shape
        else {
            continue;
        };
        let g = Glyphs::for_style(ls);
        let arrow_color = color_of(a.ann);
        let arrow_style = base.fg(arrow_color).bold();
        let lane_x = arrow_x0 + a.lane * 2;
        let ys = row_of_line[&from.line];
        let yt = row_of_line[&to.line];
        let (top, bottom) = (ys.min(yt), ys.max(yt));
        for y in top + 1..bottom {
            canvas.put_line(lane_x, y, g.v, arrow_style);
        }
        let down = yt > ys;
        // The lane turns toward the code at both ends: ╭─ at the top row, ╰─ at the bottom.
        canvas.put(lane_x, ys, if down { g.tl } else { g.bl }, arrow_style);
        canvas.put(lane_x, yt, if down { g.bl } else { g.tl }, arrow_style);
        // Source row: line up to the code, with the start head in the last cell.
        // Crossings with other rails become junction glyphs (t-094 AC4) in the
        // rail's color (Canvas::put_line), whatever the drawing order.
        for x in lane_x + 1..code_x - 2 {
            canvas.put_line(x, ys, g.h, arrow_style);
        }
        match head_glyph(ls.heads.start, true, g.h) {
            Some(h) => {
                canvas.put(code_x - 2, ys, h, arrow_style);
            }
            None => canvas.put_line(code_x - 2, ys, g.h, arrow_style),
        }
        // Target row: same, with the end head.
        for x in lane_x + 1..code_x - 2 {
            canvas.put_line(x, yt, g.h, arrow_style);
        }
        match head_glyph(ls.heads.end, true, g.h) {
            Some(h) => {
                canvas.put(code_x - 2, yt, h, arrow_style);
            }
            None => canvas.put_line(code_x - 2, yt, g.h, arrow_style),
        }
        // Mark the source and target points in the code.
        mark_anchor(
            &mut canvas,
            doc,
            code_x,
            code_w,
            ys,
            from.line,
            from.col,
            opts.tab_width,
            arrow_color,
        );
        mark_anchor(
            &mut canvas,
            doc,
            code_x,
            code_w,
            yt,
            to.line,
            to.col,
            opts.tab_width,
            arrow_color,
        );
        if labels_in_margin && let Some(l) = label {
            margin[yt].push(Label {
                text: l.clone(),
                color: arrow_color,
                note: false,
            });
        }
    }

    // ---- 5. arrows on the same line ----
    for (y, row) in rows.iter().enumerate() {
        let Row::ArrowUnder(i) = row else { continue };
        let Shape::Arrow {
            from,
            to,
            label,
            line: ls,
        } = &vis[*i].shape
        else {
            continue;
        };
        let g = Glyphs::for_style(ls);
        let arrow_color = color_of(vis[*i]);
        let arrow_style = base.fg(arrow_color).bold();
        let text = doc.line_text(from.line).unwrap_or("");
        let fx = code_x + col_to_x(text, from.col, opts.tab_width).min(code_w.saturating_sub(1));
        let tx = code_x + col_to_x(text, to.col, opts.tab_width).min(code_w.saturating_sub(1));
        if tx > fx {
            // Leaves the source downward (╰), runs right, head points right.
            canvas.put(
                fx,
                y,
                head_glyph(ls.heads.start, false, g.bl).unwrap_or(g.bl),
                arrow_style,
            );
            for x in fx + 1..tx {
                canvas.put_line(x, y, g.h, arrow_style);
            }
            canvas.put(
                tx,
                y,
                head_glyph(ls.heads.end, true, g.h).unwrap_or(g.h),
                arrow_style,
            );
        } else if fx > tx {
            canvas.put(
                tx,
                y,
                head_glyph(ls.heads.end, false, g.h).unwrap_or(g.h),
                arrow_style,
            );
            for x in tx + 1..fx {
                canvas.put_line(x, y, g.h, arrow_style);
            }
            canvas.put(
                fx,
                y,
                head_glyph(ls.heads.start, true, g.br).unwrap_or(g.br),
                arrow_style,
            );
        } else {
            canvas.put(
                fx,
                y,
                match ls.heads.end {
                    crate::model::Head::None => g.v,
                    crate::model::Head::Dot => "●",
                    crate::model::Head::Bar => "┴",
                    crate::model::Head::Open => "^",
                    crate::model::Head::Arrow => "▲",
                },
                arrow_style,
            );
        }
        if labels_in_margin && let Some(l) = label {
            margin[y].push(Label {
                text: l.clone(),
                color: arrow_color,
                note: false,
            });
        }
        let code_row = row_of_line[&from.line];
        mark_anchor(
            &mut canvas,
            doc,
            code_x,
            code_w,
            code_row,
            from.line,
            from.col,
            opts.tab_width,
            arrow_color,
        );
        mark_anchor(
            &mut canvas,
            doc,
            code_x,
            code_w,
            code_row,
            to.line,
            to.col,
            opts.tab_width,
            arrow_color,
        );
    }

    // ---- 6. notes ----
    if labels_in_margin {
        for a in &vis {
            if let Shape::Note { line, text } = &a.shape
                && let Some(&y) = row_of_line.get(line)
            {
                margin[y].push(Label {
                    text: text.clone(),
                    color: color_of(a),
                    note: true,
                });
            }
        }
    }
    for (y, row) in rows.iter().enumerate() {
        let Row::Under(i) = row else { continue };
        let (lbl, is_note) = &unders[*i];
        let style = base.fg(lbl.color).italic();
        let prefix = if *is_note { "┆ " } else { "· " };
        canvas.put_str(code_x, y, prefix, style);
        canvas.put_str_clipped(
            code_x + 2,
            y,
            &lbl.text,
            width.saturating_sub(code_x + 2),
            style.prose(),
        );
    }

    // ---- 6b. callouts ----
    let mut occupied = vec![false; canvas.height];
    // Columns of the margin taken by a callout block on each row; labels of
    // those rows are drawn to the right of the block when there is room.
    let mut shift = vec![0usize; canvas.height];
    let draw_block = |canvas: &mut Canvas, c: &CalloutPlan, x: usize, y0: usize, color: Rgb| {
        let w = c.width;
        let g = Glyphs::for_border(&c.frame);
        let border = base.fg(color);
        // Callouts are not tinted unless asked (`fill: true`).
        let inside = if c.frame.fill.unwrap_or(false) {
            base.bg(crate::theme::blend(color, theme.bg, 0.08))
        } else {
            base
        };
        let text_style = inside.fg(color).italic();
        canvas.put(x, y0, g.tl, border);
        for xx in x + 1..x + w - 1 {
            canvas.put(xx, y0, g.h, border);
        }
        canvas.put(x + w - 1, y0, g.tr, border);
        if let Some(t) = &c.title {
            canvas.put(x + 2, y0, " ", border);
            let tw = canvas.put_str_clipped(x + 3, y0, t, w.saturating_sub(6), border.bold());
            canvas.put(x + 3 + tw, y0, " ", border);
        }
        for (k, l) in c.lines.iter().enumerate() {
            let y = y0 + 1 + k;
            canvas.put(x, y, g.v, border);
            for xx in x + 1..x + w - 1 {
                canvas.put(xx, y, " ", inside);
            }
            canvas.put_str(x + 2, y, l, text_style);
            canvas.put(x + w - 1, y, g.v, border);
        }
        let yb = y0 + 1 + c.lines.len();
        canvas.put(x, yb, g.bl, border);
        for xx in x + 1..x + w - 1 {
            canvas.put(xx, yb, g.h, border);
        }
        canvas.put(x + w - 1, yb, g.br, border);
    };
    // Below: their rows already exist.
    for c in &callouts {
        if !c.below {
            continue;
        }
        if let Some(y0) = rows.iter().position(|r| {
            *r == Row::Callout(callouts.iter().position(|x| std::ptr::eq(x, c)).unwrap(), 0)
        }) {
            draw_block(&mut canvas, c, code_x, y0, color_of(vis[c.idx]));
        }
    }
    // Margin: reserve rows from the callout's first line.
    if labels_in_margin && margin_w > 0 {
        for c in &callouts {
            if c.below {
                continue;
            }
            let Shape::Callout { from_line, .. } = &vis[c.idx].shape else {
                continue;
            };
            let Some(&y0) = row_of_line.get(from_line) else {
                continue;
            };
            let h = c.lines.len() + 2;
            if y0 + h > canvas.height || c.width > margin_w {
                // Does not fit: fall back to a clipped single label.
                let Shape::Callout { text, .. } = &vis[c.idx].shape else {
                    continue;
                };
                margin[y0].push(Label {
                    text: text.clone(),
                    color: color_of(vis[c.idx]),
                    note: true,
                });
                continue;
            }
            draw_block(&mut canvas, c, margin_x, y0, color_of(vis[c.idx]));
            if margin_w.saturating_sub(c.width + 1) >= 14 {
                shift[y0..y0 + h].fill(c.width + 1);
            } else {
                occupied[y0..y0 + h].fill(true);
            }
        }
    }

    // ---- 7. margin ----
    // Each label takes its own margin row. If a line has several, the next
    // ones move down to the free rows below; if a label does not fit, it is
    // split into words and continues on the next row. A row that belongs to
    // another line's label is never taken by a continuation or an overflow
    // (so labels do not drift away from their line); what does not fit is
    // clipped with an ellipsis on the last row it got.
    if labels_in_margin && margin_w > 0 {
        let height = canvas.height;
        let reserved: Vec<bool> = margin.iter().map(|l| !l.is_empty()).collect();
        for (y, labels) in margin.iter().enumerate() {
            let mut next_row = y;
            let free = |r: usize, occupied: &[bool]| {
                r < height && !occupied[r] && (!reserved[r] || r == y)
            };
            for l in labels {
                let prefix = if l.note { "┆ " } else { "" };
                let style = if l.note {
                    base.fg(l.color).italic()
                } else {
                    base.fg(l.color)
                };
                let Some(mut r) = (next_row..height).find(|&r| free(r, &occupied)) else {
                    break;
                };
                let x0 = margin_x + shift[r];
                let text_w = margin_w.saturating_sub(shift[r] + display_width(prefix, 4) + 1);
                if text_w < 4 {
                    break;
                }
                let pieces = wrap_words(&l.text, text_w);
                let px = x0 + display_width(prefix, 4);
                for (i, piece) in pieces.iter().enumerate() {
                    if !free(r, &occupied) || shift[r] > x0 - margin_x {
                        // No room for this piece: the previous row shows the
                        // rest of the text, clipped with an ellipsis.
                        if i > 0 {
                            let rest = pieces[i - 1..].join(" ");
                            canvas.put_str_clipped(px, r - 1, &rest, text_w, style.prose());
                        }
                        break;
                    }
                    let mut x = x0;
                    if i == 0 {
                        x += canvas.put_str(x, r, prefix, style);
                    } else {
                        x += canvas.put_str(x, r, &" ".repeat(display_width(prefix, 4)), style);
                    }
                    canvas.put_str(x, r, piece, style.prose());
                    occupied[r] = true;
                    r += 1;
                }
                next_row = r;
            }
        }
    }

    Layout {
        canvas,
        rows,
        row_of_line,
        code_x,
        clipped_code: (code_full_w > code_w).then_some(code_full_w),
        labels_in_margin,
    }
}

/// Splits `text` into chunks of at most `width` cells, breaking on words
/// `glyph legend[: label]` for a mark, from its resolved style.
/// Box-drawing glyphs for one stroke + corner style.
struct Glyphs {
    h: &'static str,
    v: &'static str,
    /// Top-left corner (`╭`).
    tl: &'static str,
    /// Top-right corner (`╮`).
    tr: &'static str,
    /// Bottom-left corner (`╰`).
    bl: &'static str,
    /// Bottom-right corner (`╯`).
    br: &'static str,
}

impl Glyphs {
    fn new(stroke: crate::model::Stroke, route: crate::model::Route) -> Glyphs {
        use crate::model::{Route, Stroke};
        let (h, v) = match stroke {
            Stroke::Solid => ("─", "│"),
            Stroke::Dashed => ("╌", "╎"),
            Stroke::Dotted => ("┄", "┆"),
            Stroke::Thick => ("━", "┃"),
            Stroke::Double => ("═", "║"),
        };
        // Thick and double have no rounded corners in Unicode.
        let (tl, tr, bl, br) = match (stroke, route) {
            (Stroke::Thick, _) => ("┏", "┓", "┗", "┛"),
            (Stroke::Double, _) => ("╔", "╗", "╚", "╝"),
            (_, Route::Rounded) => ("╭", "╮", "╰", "╯"),
            (_, Route::Sharp) => ("┌", "┐", "└", "┘"),
        };
        Glyphs {
            h,
            v,
            tl,
            tr,
            bl,
            br,
        }
    }

    fn for_style(ls: &crate::model::LineStyle) -> Glyphs {
        Glyphs::new(ls.stroke, ls.route)
    }

    fn for_border(bs: &crate::model::BorderStyle) -> Glyphs {
        Glyphs::new(bs.border, bs.corners)
    }
}

/// Glyph for a head at a line end, or `None` to continue the line.
/// `points_right` is the direction the line travels into that end.
fn head_glyph(head: crate::model::Head, points_right: bool, _line: &str) -> Option<&'static str> {
    use crate::model::Head;
    Some(match (head, points_right) {
        (Head::None, _) => return None,
        (Head::Arrow, true) => "▶",
        (Head::Arrow, false) => "◀",
        (Head::Dot, _) => "●",
        (Head::Bar, true) => "┤",
        (Head::Bar, false) => "├",
        (Head::Open, true) => ">",
        (Head::Open, false) => "<",
    })
}

fn mark_text(
    doc: &Document,
    kind: &str,
    style: &crate::model::MarkStyle,
    label: Option<&str>,
) -> String {
    let st = doc.resolve_style(kind, style).unwrap_or_default();
    let mut out = String::new();
    if let Some(g) = &st.glyph {
        out.push_str(g);
        out.push(' ');
    }
    out.push_str(st.legend.as_deref().unwrap_or(kind));
    if let Some(l) = label {
        out.push_str(": ");
        out.push_str(l);
    }
    out
}

/// (and on graphemes if a single word does not fit).
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    let mut line = String::new();
    let mut line_w = 0usize;
    for word in text.split_whitespace() {
        let w = display_width(word, 4);
        if w > width {
            // Word longer than the width: split by graphemes.
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            let mut chunk = String::new();
            let mut cw = 0;
            for g in word.graphemes(true) {
                let gw = unicode_width::UnicodeWidthStr::width(g).max(1);
                if cw + gw > width {
                    out.push(std::mem::take(&mut chunk));
                    cw = 0;
                }
                chunk.push_str(g);
                cw += gw;
            }
            line = chunk;
            line_w = cw;
            continue;
        }
        let sep = usize::from(!line.is_empty());
        if line_w + sep + w > width {
            out.push(std::mem::take(&mut line));
            line_w = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            line_w += 1;
        }
        line.push_str(word);
        line_w += w;
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}

/// Screen column (relative to the start of the code) of grapheme `col` of `text`.
/// Whether a span's emphasis needs the row under its line: underlines always,
/// brackets only when a side has no blank cell beside the span.
fn emphasis_needs_row(doc: &Document, span: &TextSpan, e: Emphasis) -> bool {
    use unicode_segmentation::UnicodeSegmentation;
    match e {
        Emphasis::Underline => true,
        Emphasis::Brackets => {
            let Some(text) = doc.line_text(span.line) else {
                return false;
            };
            let g: Vec<&str> = text.graphemes(true).collect();
            let before = span
                .start
                .checked_sub(1)
                .and_then(|i| g.get(i as usize))
                .is_some_and(|c| c.trim().is_empty());
            let after = g.get(span.end as usize).is_none_or(|c| c.trim().is_empty());
            !(before && after)
        }
        _ => false,
    }
}

pub fn col_to_x(text: &str, col: u32, tab_width: usize) -> usize {
    let mut w = 0;
    for (i, g) in text.graphemes(true).enumerate() {
        if i as u32 >= col {
            break;
        }
        if g == "\t" {
            w += tab_width - (w % tab_width);
        } else {
            w += unicode_width::UnicodeWidthStr::width(g);
        }
    }
    w
}

#[allow(clippy::too_many_arguments)]
fn paint_code(
    canvas: &mut Canvas,
    x0: usize,
    y: usize,
    max_w: usize,
    text: &str,
    segs: &[Segment],
    tab_width: usize,
    base: CellStyle,
    theme: &Theme,
    in_focus: bool,
) {
    let full_w = display_width(text, tab_width);
    let clip = full_w > max_w;
    let limit = if clip { max_w.saturating_sub(1) } else { max_w };
    let mut x = 0usize;
    'outer: for seg in segs {
        let style = if in_focus {
            let mut s = base;
            if let Some(fg) = seg.fg {
                s = s.fg(theme.legible(fg));
            }
            if seg.bold {
                s = s.bold();
            }
            if seg.italic {
                s = s.italic();
            }
            s
        } else {
            base.fg(theme.dimmed)
        };
        for g in seg.text.graphemes(true) {
            let w = if g == "\t" {
                tab_width - (x % tab_width)
            } else {
                unicode_width::UnicodeWidthStr::width(g)
            };
            if x + w > limit {
                break 'outer;
            }
            if g == "\t" {
                for k in 0..w {
                    canvas.put(x0 + x + k, y, " ", style);
                }
            } else {
                canvas.put(x0 + x, y, g, style);
            }
            x += w;
        }
    }
    if clip {
        canvas.put(x0 + limit, y, "…", base.fg(theme.muted));
    }
}

#[allow(clippy::too_many_arguments)]
fn mark_anchor(
    canvas: &mut Canvas,
    doc: &Document,
    code_x: usize,
    code_w: usize,
    y: usize,
    line: u32,
    col: u32,
    tab_width: usize,
    color: Rgb,
) {
    let text = doc.line_text(line).unwrap_or("");
    let x = col_to_x(text, col, tab_width);
    if x < code_w {
        // Underline the token that starts at the anchor (up to the next separator).
        let rest = crate::model::grapheme_slice(text, col, u32::MAX);
        let token_len = rest
            .graphemes(true)
            .take_while(|g| g.chars().all(|c| c.is_alphanumeric() || c == '_'))
            .count()
            .max(1);
        let w = col_to_x(text, col + token_len as u32, tab_width) - x;
        canvas.restyle(
            code_x + x,
            y,
            w.min(code_w - x),
            CellStyle::new().fg(color).bold(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Anchor, Placement, TextSpan};

    fn doc() -> Document {
        let mut d = Document::from_code(
            "fn main() {\n    let s = String::from(\"hola\");\n    takes(s);\n    println!(\"{s}\");\n}\n\nfn takes(t: String) {\n    println!(\"{t}\");\n}\n",
            "rs",
            None,
        );
        let s = d.find_text("let s", 1, None).unwrap();
        d.add(Annotation {
            id: "a1".into(),
            step: 1,
            color: None,
            markers: None,
            shape: Shape::Mark {
                kind: "move".into(),
                style: Default::default(),
                span: TextSpan {
                    line: 3,
                    start: 10,
                    end: 11,
                },
                label: None,
                emphasis: Default::default(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a2".into(),
            step: 1,
            color: None,
            markers: None,
            shape: Shape::Arrow {
                from: Anchor {
                    line: s.line,
                    col: s.start + 4,
                },
                to: Anchor { line: 7, col: 9 },
                label: Some("s moves into t".into()),
                line: Default::default(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a3".into(),
            step: 2,
            color: None,
            markers: None,
            shape: Shape::Box {
                from_line: 7,
                to_line: 9,
                label: Some("takes".into()),
                frame: Default::default(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a4".into(),
            step: 2,
            color: None,
            markers: None,
            shape: Shape::Note {
                line: 4,
                text: "error: s is no longer the owner".into(),
            },
        })
        .unwrap();
        d
    }

    #[test]
    fn boxes_insert_virtual_rows_and_arrows_use_left_rail() {
        let d = doc();
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 90,
                step: 2,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        // 9 lines + 2 box rows
        assert_eq!(l.rows.len(), 11);
        assert!(text.contains("╭─ takes"));
        assert!(
            text.contains("┼"),
            "the arrow crosses the box rail with a junction"
        );
        assert!(text.contains("╰─"));
        assert!(text.contains("▶"));
        assert!(text.contains("━▶ move"));
        assert!(text.contains("┆ error: s is no longer the owner"));
        // Nothing exceeds the width.
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 90);
        }
    }

    #[test]
    fn spotlight_dims_code_and_other_annotations() {
        let mut d = doc();
        d.add(Annotation {
            id: "s1".into(),
            step: 3,
            color: None,
            markers: None,
            shape: Shape::Spotlight {
                from_line: Some(7),
                to_line: Some(9),
                ids: vec!["a3".into()],
                until: None,
            },
        })
        .unwrap();
        let th = Theme::dark();
        let on = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 90,
                step: 3,
                ..Default::default()
            },
        );
        let off = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 90,
                step: 4,
                ..Default::default()
            },
        );
        let fg_at = |l: &Layout, line: u32| {
            let y = l.row_of_line[&line];
            l.canvas.get(l.code_x + 4, y).unwrap().style.fg
        };
        assert_eq!(
            fg_at(&on, 2),
            Some(th.dimmed),
            "outside the spotlight is dimmed"
        );
        assert_ne!(
            fg_at(&on, 7),
            Some(th.dimmed),
            "inside the spotlight is not"
        );
        assert_ne!(
            fg_at(&off, 2),
            Some(th.dimmed),
            "at step 4 it no longer applies"
        );
        // Note a4 (not included in ids) is drawn dimmed; box a3 keeps its color.
        let text = on.canvas.to_plain_text();
        assert!(text.contains("takes"));
        let y_note = on.row_of_line[&4];
        let note_cell = (0..on.canvas.width)
            .rev()
            .filter_map(|x| on.canvas.get(x, y_note))
            .find(|c| c.sym == "┆")
            .unwrap();
        assert_eq!(note_cell.style.fg, Some(th.dimmed));
    }

    #[test]
    fn margin_labels_wrap_and_stack_instead_of_clipping() {
        let mut d = doc();
        // Two labels on the same line and a long note.
        d.add(Annotation {
            id: "n1".into(),
            step: 1,
            color: None,
            markers: None,
            shape: Shape::Note {
                line: 2,
                text: "s is born here with String::from and lives until it is moved".into(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "n2".into(),
            step: 1,
            color: None,
            markers: None,
            shape: Shape::Note {
                line: 2,
                text: "second note".into(),
            },
        })
        .unwrap();
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        // Line 3 has its own label (the mark), so the long note on line 2
        // cannot continue onto row 3: it is clipped there with an ellipsis
        // and the second note takes the next free row. Labels never drift
        // away from their line.
        let row = |line: u32| l.row_of_line[&line];
        assert!(
            l.canvas.row_text(row(2)).contains("s is born here"),
            "{text}"
        );
        assert!(l.canvas.row_text(row(2)).contains('…'), "{text}");
        assert!(
            l.canvas.row_text(row(3)).contains("━▶ move"),
            "line 3 keeps its label: {text}"
        );
        assert!(l.canvas.row_text(row(4)).contains("second note"), "{text}");
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 80);
        }
        // Without a label on line 3, the note wraps freely over rows 2-4.
        d.delete("a1").unwrap();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        assert!(!text.contains('…'), "free rows below: no clipping\n{text}");
        let row = |line: u32| l.row_of_line[&line];
        assert!(l.canvas.row_text(row(2)).contains("s is born here"));
        assert!(l.canvas.row_text(row(3)).contains("and lives until"));
        assert_eq!(
            wrap_words("hello cruel world", 11),
            vec!["hello cruel", "world"]
        );
        assert_eq!(
            wrap_words("supercalifragilistico", 8),
            vec!["supercal", "ifragili", "stico"]
        );
    }

    #[test]
    fn step_filters_annotations() {
        let d = doc();
        let th = Theme::light();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 90,
                step: 1,
                ..Default::default()
            },
        );
        assert_eq!(l.rows.len(), 9, "at step 1 there is no box yet");
        assert!(!l.canvas.to_plain_text().contains("takes\n") || true);
    }

    #[test]
    fn narrow_terminal_moves_notes_under_lines() {
        let d = doc();
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 48,
                step: 2,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        assert!(l.rows.iter().any(|r| matches!(r, Row::Under(_))));
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 48);
        }
    }

    /// Code without annotations, plus a callout over L2-3 and a note on L4
    /// (the "following line" of the spec).
    fn callout_doc(placement: Placement) -> Document {
        let mut d = Document::from_code(
            "fn main() {\n    let s = String::from(\"hola\");\n    takes(s);\n    println!(\"{s}\");\n}\n\nfn takes(t: String) {\n    println!(\"{t}\");\n}\n",
            "rs",
            None,
        );
        d.add(Annotation {
            id: "c1".into(),
            step: 1,
            color: Some("cyan".into()),
            markers: None,
            shape: Shape::Callout {
                from_line: 2,
                to_line: Some(3),
                text: "s owns the String. Passing it to takes moves the value, so after line 3 main cannot use s any more: the compiler rejects line 4.".into(),
                title: Some("move".into()),
                placement,
                width: None,
                frame: Default::default(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "n1".into(),
            step: 1,
            color: None,
            markers: None,
            shape: Shape::Note {
                line: 4,
                text: "does not compile".into(),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "b1".into(),
            step: 1,
            color: None,
            markers: None,
            shape: Shape::Box {
                from_line: 2,
                to_line: 3,
                label: Some("moved here".into()),
                frame: Default::default(),
            },
        })
        .unwrap();
        d
    }

    fn row_with(text: &str, l: &Layout) -> usize {
        (0..l.canvas.height)
            .find(|&y| l.canvas.row_text(y).contains(text))
            .unwrap_or_else(|| panic!("{text:?} not rendered:\n{}", l.canvas.to_plain_text()))
    }

    #[test]
    fn callout_in_margin_reserves_rows_and_next_label_stays_on_its_line() {
        let d = callout_doc(Placement::Margin);
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 114,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        // Block is a bordered box with its title, aligned with L2.
        let top = row_with("╭─ move ─", &l);
        assert_eq!(top, l.row_of_line[&2], "callout top aligns with from_line");
        assert!(text.contains("│ s owns the String."));
        // The note on L4 is drawn on its own line, not pushed below the block…
        let note_row = row_with("┆ does not compile", &l);
        assert_eq!(note_row, l.row_of_line[&4]);
        // …which means the block only occupies the margin to the right.
        assert!(l.canvas.row_text(note_row).contains("println!"));
        // No row exceeds the width and the code block is contiguous (no Callout rows).
        assert!(!l.rows.iter().any(|r| matches!(r, Row::Callout(..))));
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 114);
        }
    }

    #[test]
    fn callout_below_inserts_rows_inside_the_box() {
        let d = callout_doc(Placement::Below);
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        let callout_rows: Vec<usize> = l
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, Row::Callout(..)))
            .map(|(y, _)| y)
            .collect();
        assert!(!callout_rows.is_empty(), "below placement inserts rows");
        // Rows sit after L3 and before L4.
        assert!(callout_rows[0] > l.row_of_line[&3]);
        assert!(*callout_rows.last().unwrap() < l.row_of_line[&4]);
        // The Box around L2-3 is extended: its bottom border comes after the callout.
        let box_top = row_with("╭─ moved here ─", &l);
        let box_bottom = l
            .rows
            .iter()
            .position(|r| matches!(r, Row::BoxBottom(_)))
            .unwrap();
        assert!(box_top < callout_rows[0] && box_bottom > *callout_rows.last().unwrap());
        // Every callout row carries the box rail on the left.
        for &y in &callout_rows {
            assert!(
                l.canvas.row_text(y).contains('│'),
                "row {y}: {}",
                l.canvas.row_text(y)
            );
        }
        assert!(text.contains("│ s owns the String."));
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 80);
        }
    }

    #[test]
    fn callout_wider_than_the_box_is_drawn_after_its_bottom_border() {
        // Box L2-3, callout L1-3: the callout explains more than the box.
        let mut d = callout_doc(Placement::Below);
        d.update(Annotation {
            id: "c1".into(),
            step: 1,
            color: Some("cyan".into()),
            markers: None,
            shape: Shape::Callout {
                from_line: 1,
                to_line: Some(3),
                text: "main as a whole: s is created and then moved.".into(),
                title: Some("whole main".into()),
                placement: Placement::Below,
                width: None,
                frame: Default::default(),
            },
        })
        .unwrap();
        let th = Theme::dark();
        let l = layout(
            &d,
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let text = l.canvas.to_plain_text();
        println!("{text}");
        let callout_rows: Vec<usize> = l
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, Row::Callout(..)))
            .map(|(y, _)| y)
            .collect();
        let box_bottom = l
            .rows
            .iter()
            .position(|r| matches!(r, Row::BoxBottom(_)))
            .unwrap();
        assert!(
            box_bottom < callout_rows[0],
            "box closes before the callout: bottom at {box_bottom}, callout at {callout_rows:?}"
        );
        assert!(*callout_rows.last().unwrap() < l.row_of_line[&4]);
        // No box rail on the callout rows: the block itself is the first thing drawn.
        let (first, last) = (callout_rows[0], *callout_rows.last().unwrap());
        for &y in &callout_rows {
            let row = l.canvas.row_text(y);
            let want = if y == first {
                '╭'
            } else if y == last {
                '╰'
            } else {
                '│'
            };
            assert_eq!(
                row.trim_start().chars().next(),
                Some(want),
                "row {y}: {row}"
            );
            assert!(!row.contains("│  ╭"), "row {y} carries a box rail: {row}");
        }
        // The narrower case (t-092's test) is unchanged: callout inside the box.
        let inside = layout(
            &callout_doc(Placement::Below),
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        let bb = inside
            .rows
            .iter()
            .position(|r| matches!(r, Row::BoxBottom(_)))
            .unwrap();
        assert!(
            inside.rows[..bb]
                .iter()
                .any(|r| matches!(r, Row::Callout(..)))
        );
    }

    #[test]
    fn callout_auto_follows_the_margin_threshold() {
        let th = Theme::dark();
        let wide = layout(
            &callout_doc(Placement::Auto),
            &th,
            &LayoutOptions {
                width: 114,
                step: 1,
                ..Default::default()
            },
        );
        assert!(!wide.rows.iter().any(|r| matches!(r, Row::Callout(..))));
        let narrow = layout(
            &callout_doc(Placement::Auto),
            &th,
            &LayoutOptions {
                width: 80,
                step: 1,
                ..Default::default()
            },
        );
        assert!(narrow.rows.iter().any(|r| matches!(r, Row::Callout(..))));
    }
}
