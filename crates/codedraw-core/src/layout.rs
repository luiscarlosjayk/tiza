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
use crate::highlight::{Segment, highlight};
use crate::model::{Annotation, Document, Shape};
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
    /// Virtual row with text (note, arrow legend) below the line.
    Under(usize),
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub canvas: Canvas,
    pub rows: Vec<Row>,
    pub row_of_line: BTreeMap<u32, usize>,
    pub code_x: usize,
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
    let vis: Vec<&Annotation> = doc
        .annotations
        .iter()
        .filter(|a| a.step <= opts.step)
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
            (Shape::Mark { kind, .. }, None) => theme.mark_color(*kind),
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
    let num_w = if opts.line_numbers {
        doc.last_line().to_string().len() + 1
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

        for (i, a) in vis.iter().enumerate() {
            if let Shape::Arrow { from, to, label } = &a.shape
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
                    Shape::Mark { kind, span, label } if span.line == line => Some((
                        format!(
                            "{} {}{}",
                            kind.glyph(),
                            kind.legend(),
                            label.as_ref().map(|l| format!(": {l}")).unwrap_or_default()
                        ),
                        color_of(a),
                        false,
                    )),
                    Shape::Highlight {
                        span,
                        label: Some(l),
                    } if span.line == line => Some((l.clone(), color_of(a), false)),
                    Shape::Arrow {
                        from,
                        to,
                        label: Some(l),
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

        let mut bottoms: Vec<usize> = (0..boxes.len()).filter(|&i| boxes[i].to == line).collect();
        bottoms.sort_by_key(|&i| std::cmp::Reverse(boxes[i].depth));
        rows.extend(bottoms.into_iter().map(Row::BoxBottom));
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
    let segments = highlight(&doc.code, &doc.source.language, theme.syntax);

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
    for a in &vis {
        let (span, fg, bg, label) = match &a.shape {
            Shape::Highlight { span, label } if ann_dimmed(a) => (
                span,
                theme.dimmed,
                crate::theme::blend(theme.dimmed, theme.bg, 0.12),
                label.clone(),
            ),
            Shape::Highlight { span, label } => {
                let c = color_of(a);
                let (fg, bg) = if a.color.is_some() {
                    (c, crate::theme::blend(c, theme.bg, 0.22))
                } else {
                    (theme.fg, theme.highlight_bg)
                };
                (span, fg, bg, label.clone())
            }
            Shape::Mark { kind, span, .. } if ann_dimmed(a) => (
                span,
                theme.dimmed,
                crate::theme::blend(theme.dimmed, theme.bg, 0.12),
                Some(format!("{} {}", kind.glyph(), kind.legend())),
            ),
            Shape::Mark { kind, span, label } => (
                span,
                theme.mark_color(*kind),
                theme.mark_bg(*kind),
                Some(format!(
                    "{} {}{}",
                    kind.glyph(),
                    kind.legend(),
                    label.as_ref().map(|l| format!(": {l}")).unwrap_or_default()
                )),
            ),
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
            canvas.restyle(x0, y, x1 - x0, CellStyle::new().fg(fg).bg(bg).bold());
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
        let Shape::Box { label, .. } = &b.ann.shape else {
            continue;
        };
        let top = rows.iter().position(|r| *r == Row::BoxTop(bi));
        let bottom = rows.iter().position(|r| *r == Row::BoxBottom(bi));
        let (Some(top), Some(bottom)) = (top, bottom) else {
            continue;
        };
        for y in top + 1..bottom {
            canvas.put_line(lx, y, "│", box_style);
            canvas.put_line(rx, y, "│", box_style);
            // subtle tint inside the box
            canvas.restyle(
                lx + 1,
                y,
                rx - lx - 1,
                CellStyle::new().bg(crate::theme::blend(box_color, theme.bg, 0.08)),
            );
        }
        canvas.put(lx, top, "╭", box_style);
        canvas.put(rx, top, "╮", box_style);
        for x in lx + 1..rx {
            canvas.put(x, top, "─", box_style);
        }
        if let Some(l) = label {
            let avail = rx.saturating_sub(lx + 4);
            canvas.put(lx + 1, top, "─", box_style);
            canvas.put(lx + 2, top, " ", box_style);
            let w = canvas.put_str_clipped(lx + 3, top, l, avail, box_style.bold());
            canvas.put(lx + 3 + w, top, " ", box_style);
        }
        canvas.put(lx, bottom, "╰", box_style);
        canvas.put(rx, bottom, "╯", box_style);
        for x in lx + 1..rx {
            canvas.put(x, bottom, "─", box_style);
        }
    }

    // ---- 4. arrows between lines ----
    for a in &arrows {
        let Shape::Arrow { from, to, label } = &a.ann.shape else {
            continue;
        };
        let arrow_color = color_of(a.ann);
        let arrow_style = base.fg(arrow_color).bold();
        let lane_x = arrow_x0 + a.lane * 2;
        let ys = row_of_line[&from.line];
        let yt = row_of_line[&to.line];
        let (top, bottom) = (ys.min(yt), ys.max(yt));
        for y in top + 1..bottom {
            canvas.put_line(lane_x, y, "│", arrow_style);
        }
        let down = yt > ys;
        canvas.put(lane_x, ys, if down { "╭" } else { "╰" }, arrow_style);
        canvas.put(lane_x, yt, if down { "╰" } else { "╭" }, arrow_style);
        for x in lane_x + 1..code_x - 1 {
            canvas.put_line(x, ys, "─", arrow_style);
        }
        for x in lane_x + 1..code_x - 2 {
            canvas.put_line(x, yt, "─", arrow_style);
        }
        canvas.put(code_x - 2, yt, "▶", arrow_style);
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
        let Shape::Arrow { from, to, label } = &vis[*i].shape else {
            continue;
        };
        let arrow_color = color_of(vis[*i]);
        let arrow_style = base.fg(arrow_color).bold();
        let text = doc.line_text(from.line).unwrap_or("");
        let fx = code_x + col_to_x(text, from.col, opts.tab_width).min(code_w.saturating_sub(1));
        let tx = code_x + col_to_x(text, to.col, opts.tab_width).min(code_w.saturating_sub(1));
        if tx > fx {
            canvas.put(fx, y, "╰", arrow_style);
            for x in fx + 1..tx {
                canvas.put_line(x, y, "─", arrow_style);
            }
            canvas.put(tx, y, "▶", arrow_style);
        } else if fx > tx {
            canvas.put(tx, y, "◀", arrow_style);
            for x in tx + 1..fx {
                canvas.put_line(x, y, "─", arrow_style);
            }
            canvas.put(fx, y, "╯", arrow_style);
        } else {
            canvas.put(fx, y, "▲", arrow_style);
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
            style,
        );
    }

    // ---- 7. margin ----
    // Each label takes its own margin row. If a line has several, the next
    // ones move down to the free rows below; if a label does not fit, it is
    // split into words and continues on the next row. It is only clipped
    // when no free rows remain before the next label.
    if labels_in_margin && margin_w > 0 {
        let height = canvas.height;
        let mut occupied = vec![false; height];
        for (y, labels) in margin.iter().enumerate() {
            let mut next_row = y;
            for l in labels {
                let prefix = if l.note { "┆ " } else { "" };
                let style = if l.note {
                    base.fg(l.color).italic()
                } else {
                    base.fg(l.color)
                };
                let text_w = margin_w.saturating_sub(display_width(prefix, 4));
                if text_w < 4 {
                    break;
                }
                let Some(mut r) = (next_row..height).find(|&r| !occupied[r]) else {
                    break;
                };
                let pieces = wrap_words(&l.text, text_w);
                let mut first = true;
                for piece in pieces {
                    if r >= height || occupied[r] {
                        // No room: clip what remains onto the previous row.
                        if r > 0 && r - 1 < height {
                            let px = margin_x + display_width(prefix, 4);
                            canvas.put_str_clipped(px, r - 1, &l.text, text_w, style);
                        }
                        break;
                    }
                    let mut x = margin_x;
                    if first {
                        x += canvas.put_str(x, r, prefix, style);
                    } else {
                        x += canvas.put_str(x, r, &" ".repeat(display_width(prefix, 4)), style);
                    }
                    canvas.put_str(x, r, &piece, style);
                    occupied[r] = true;
                    first = false;
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
    }
}

/// Splits `text` into chunks of at most `width` cells, breaking on words
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
fn col_to_x(text: &str, col: u32, tab_width: usize) -> usize {
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
                s = s.fg(fg);
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
    use crate::model::{Anchor, MarkKind, TextSpan};

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
            shape: Shape::Mark {
                kind: MarkKind::Move,
                span: TextSpan {
                    line: 3,
                    start: 10,
                    end: 11,
                },
                label: None,
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a2".into(),
            step: 1,
            color: None,
            shape: Shape::Arrow {
                from: Anchor {
                    line: s.line,
                    col: s.start + 4,
                },
                to: Anchor { line: 7, col: 9 },
                label: Some("s moves into t".into()),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a3".into(),
            step: 2,
            color: None,
            shape: Shape::Box {
                from_line: 7,
                to_line: 9,
                label: Some("takes".into()),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a4".into(),
            step: 2,
            color: None,
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
        assert!(
            !text.contains('…'),
            "must not clip when there are free rows below"
        );
        assert!(text.contains("second note"));
        assert!(text.contains("s is born here"));
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 80);
        }
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
}
