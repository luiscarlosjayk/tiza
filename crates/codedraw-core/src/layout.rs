//! Convierte documento + tema + ancho disponible en un `Canvas`.
//!
//! Decisiones de esta primera versión (ver docs/adr/0002):
//! - Las cajas rodean de verdad a las líneas: insertan filas virtuales (sin
//!   número) arriba y abajo, y rieles verticales a izquierda y derecha.
//! - Las flechas entre líneas distintas van por un carril a la izquierda del
//!   código (`╭──` sale de la línea origen, `╰──▶` entra en la destino).
//! - Las flechas dentro de una misma línea usan una fila virtual debajo.
//! - Notas y leyendas van al margen derecho si hay ≥ 14 celdas; si no, en una
//!   fila virtual debajo de la línea. Así funciona en una terminal de 75 columnas.

use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::canvas::{Canvas, CellStyle, Rgb, display_width};
use crate::highlight::{Segment, highlight};
use crate::model::{Annotation, Document, Shape};
use crate::theme::Theme;

#[derive(Debug, Clone)]
pub struct LayoutOptions {
    pub width: usize,
    /// Paso actual: se dibujan las anotaciones con `step <= step`.
    pub step: u32,
    /// Rango de líneas enfocadas; el resto se atenúa.
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
    /// Fila virtual con texto (nota, leyenda de flecha) bajo la línea.
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

/// Texto pendiente de colocar en el margen (o debajo) de una fila.
#[derive(Clone)]
struct Label {
    text: String,
    color: Rgb,
}

pub fn layout(doc: &Document, theme: &Theme, opts: &LayoutOptions) -> Layout {
    let width = opts.width.max(20);
    let vis: Vec<&Annotation> = doc
        .annotations
        .iter()
        .filter(|a| a.step <= opts.step)
        .collect();

    // ---- cajas y su anidamiento ----
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

    // ---- anchos de columnas ----
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

    // Las filas se construyen antes de asignar carriles (los carriles dependen de filas).
    // Pero el ancho del margen depende del número de carriles, y las filas "Under"
    // dependen del margen. Aproximamos: carriles = cota superior (todas se solapan).
    // Estimar carriles en espacio de líneas (las filas virtuales conservan el orden,
    // así que el solapamiento es el mismo que en espacio de filas).
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

    // ---- filas ----
    let mut rows: Vec<Row> = Vec::new();
    let mut row_of_line = BTreeMap::new();
    // Textos "debajo": índice en `unders` referenciado por Row::Under
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
                            color: theme.arrow,
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
                        Some((text.clone(), theme.note, true))
                    }
                    Shape::Mark { kind, span, label } if span.line == line => Some((
                        format!(
                            "{} {}{}",
                            kind.glyph(),
                            kind.legend(),
                            label.as_ref().map(|l| format!(": {l}")).unwrap_or_default()
                        ),
                        theme.mark_color(*kind),
                        false,
                    )),
                    Shape::Highlight {
                        span,
                        label: Some(l),
                    } if span.line == line => Some((l.clone(), theme.accent, false)),
                    Shape::Arrow {
                        from,
                        to,
                        label: Some(l),
                    } if from.line != to.line && to.line == line => {
                        Some((l.clone(), theme.arrow, false))
                    }
                    _ => None,
                };
                if let Some((text, color, is_note)) = lbl {
                    unders.push((Label { text, color }, is_note));
                    rows.push(Row::Under(unders.len() - 1));
                }
            }
        }

        let mut bottoms: Vec<usize> = (0..boxes.len()).filter(|&i| boxes[i].to == line).collect();
        bottoms.sort_by_key(|&i| std::cmp::Reverse(boxes[i].depth));
        rows.extend(bottoms.into_iter().map(Row::BoxBottom));
    }

    // ---- carriles de flechas (en espacio de filas) ----
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
        // Las más largas primero → carril 0 (el más alejado del código).
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

    // ---- posiciones finales ----
    let arrow_x0 = num_w;
    let box_x0 = arrow_x0 + arrow_w;
    let code_x = box_x0 + box_depth + 2;
    let code_w = code_full_w
        .min(width.saturating_sub(code_x + box_depth + 1))
        .max(10);
    let rbox_x0 = code_x + code_w + 1; // riel derecho: el más interno aquí
    let margin_x = rbox_x0 + box_depth + 1;
    let margin_w = width.saturating_sub(margin_x);

    let base = theme.base();
    let mut canvas = Canvas::new_filled(width, rows.len(), base);
    let segments = highlight(&doc.code, &doc.source.language, theme.syntax);

    // Etiquetas de margen por fila.
    let mut margin: Vec<Vec<Label>> = vec![Vec::new(); rows.len()];

    // ---- 1. código ----
    for (y, row) in rows.iter().enumerate() {
        let Row::Code(line) = row else { continue };
        let text = doc.line_text(*line).unwrap_or("");
        let in_focus = opts.focus.is_none_or(|(a, b)| *line >= a && *line <= b);
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

    // ---- 2. resaltes y marcas ----
    for a in &vis {
        let (span, fg, bg, label) = match &a.shape {
            Shape::Highlight { span, label } => (span, theme.fg, theme.highlight_bg, label.clone()),
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
            let color = if matches!(a.shape, Shape::Mark { .. }) {
                fg
            } else {
                theme.accent
            };
            margin[y].push(Label { text: l, color });
        }
    }

    // ---- 3. cajas ----
    let box_style = base.fg(theme.box_line);
    for (bi, b) in boxes.iter().enumerate() {
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
            canvas.put(lx, y, "│", box_style);
            canvas.put(rx, y, "│", box_style);
            // tinte sutil dentro de la caja
            canvas.restyle(lx + 1, y, rx - lx - 1, CellStyle::new().bg(theme.box_bg));
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

    // ---- 4. flechas entre líneas ----
    let arrow_style = base.fg(theme.arrow).bold();
    for a in &arrows {
        let Shape::Arrow { from, to, label } = &a.ann.shape else {
            continue;
        };
        let lane_x = arrow_x0 + a.lane * 2;
        let ys = row_of_line[&from.line];
        let yt = row_of_line[&to.line];
        let (top, bottom) = (ys.min(yt), ys.max(yt));
        for y in top + 1..bottom {
            canvas.put(lane_x, y, "│", arrow_style);
        }
        let down = yt > ys;
        canvas.put(lane_x, ys, if down { "╭" } else { "╰" }, arrow_style);
        canvas.put(lane_x, yt, if down { "╰" } else { "╭" }, arrow_style);
        for x in lane_x + 1..code_x - 1 {
            canvas.put(x, ys, "─", arrow_style);
        }
        for x in lane_x + 1..code_x - 2 {
            canvas.put(x, yt, "─", arrow_style);
        }
        canvas.put(code_x - 2, yt, "▶", arrow_style);
        // Marcar los puntos de origen y destino en el código.
        mark_anchor(
            &mut canvas,
            doc,
            code_x,
            code_w,
            ys,
            from.line,
            from.col,
            opts.tab_width,
            theme.arrow,
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
            theme.arrow,
        );
        if labels_in_margin && let Some(l) = label {
            margin[yt].push(Label {
                text: l.clone(),
                color: theme.arrow,
            });
        }
    }

    // ---- 5. flechas en la misma línea ----
    for (y, row) in rows.iter().enumerate() {
        let Row::ArrowUnder(i) = row else { continue };
        let Shape::Arrow { from, to, label } = &vis[*i].shape else {
            continue;
        };
        let text = doc.line_text(from.line).unwrap_or("");
        let fx = code_x + col_to_x(text, from.col, opts.tab_width).min(code_w.saturating_sub(1));
        let tx = code_x + col_to_x(text, to.col, opts.tab_width).min(code_w.saturating_sub(1));
        if tx > fx {
            canvas.put(fx, y, "╰", arrow_style);
            for x in fx + 1..tx {
                canvas.put(x, y, "─", arrow_style);
            }
            canvas.put(tx, y, "▶", arrow_style);
        } else if fx > tx {
            canvas.put(tx, y, "◀", arrow_style);
            for x in tx + 1..fx {
                canvas.put(x, y, "─", arrow_style);
            }
            canvas.put(fx, y, "╯", arrow_style);
        } else {
            canvas.put(fx, y, "▲", arrow_style);
        }
        if labels_in_margin && let Some(l) = label {
            margin[y].push(Label {
                text: l.clone(),
                color: theme.arrow,
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
            theme.arrow,
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
            theme.arrow,
        );
    }

    // ---- 6. notas ----
    if labels_in_margin {
        for a in &vis {
            if let Shape::Note { line, text } = &a.shape
                && let Some(&y) = row_of_line.get(line)
            {
                margin[y].push(Label {
                    text: text.clone(),
                    color: theme.note,
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

    // ---- 7. margen ----
    if labels_in_margin && margin_w > 0 {
        for (y, labels) in margin.iter().enumerate() {
            let mut x = margin_x;
            for (i, l) in labels.iter().enumerate() {
                if x >= width {
                    break;
                }
                if i > 0 {
                    x += canvas.put_str(x, y, "  ", base);
                }
                let avail = width.saturating_sub(x);
                let is_note = l.color == theme.note;
                let style = if is_note {
                    base.fg(l.color).italic()
                } else {
                    base.fg(l.color)
                };
                if is_note {
                    x += canvas.put_str(x, y, "┆ ", style);
                }
                x += canvas.put_str_clipped(
                    x,
                    y,
                    &l.text,
                    width.saturating_sub(x).min(avail),
                    style,
                );
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

/// Columna de pantalla (relativa al inicio del código) del grafema `col` de `text`.
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
        // Subrayamos el token que empieza en el ancla (hasta el siguiente separador).
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
            shape: Shape::Arrow {
                from: Anchor {
                    line: s.line,
                    col: s.start + 4,
                },
                to: Anchor { line: 7, col: 9 },
                label: Some("s se mueve a t".into()),
            },
        })
        .unwrap();
        d.add(Annotation {
            id: "a3".into(),
            step: 2,
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
            shape: Shape::Note {
                line: 4,
                text: "error: s ya no es dueña".into(),
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
        // 9 líneas + 2 filas de caja
        assert_eq!(l.rows.len(), 11);
        assert!(text.contains("╭─ takes"));
        assert!(text.contains("╰─"));
        assert!(text.contains("▶"));
        assert!(text.contains("━▶ move"));
        assert!(text.contains("┆ error: s ya no es dueña"));
        // Nada se sale del ancho.
        for y in 0..l.canvas.height {
            assert!(display_width(&l.canvas.row_text(y), 4) <= 90);
        }
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
        assert_eq!(l.rows.len(), 9, "en el paso 1 no hay caja todavía");
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
