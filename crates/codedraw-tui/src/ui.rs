//! Drawing with ratatui: copies the core `Canvas` into the buffer and adds the
//! status line, the command line and the overlays.

use codedraw_core::canvas::{Canvas, CellStyle, Rgb};
use codedraw_core::protocol::describe;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode, Overlay, Selection, theme_label};

fn rgb(c: Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

fn style(s: CellStyle) -> Style {
    let mut st = Style::default();
    if let Some(fg) = s.fg {
        st = st.fg(rgb(fg));
    }
    if let Some(bg) = s.bg {
        st = st.bg(rgb(bg));
    }
    if s.bold {
        st = st.add_modifier(Modifier::BOLD);
    }
    if s.dim {
        st = st.add_modifier(Modifier::DIM);
    }
    if s.italic {
        st = st.add_modifier(Modifier::ITALIC);
    }
    st
}

/// Copies the canvas rows `rows` into `area` of the buffer.
fn blit(canvas: &Canvas, rows: std::ops::Range<usize>, area: Rect, buf: &mut Buffer) {
    for (dy, y) in rows.enumerate() {
        if dy as u16 >= area.height || y >= canvas.height {
            break;
        }
        let mut x = 0usize;
        while x < canvas.width && (x as u16) < area.width {
            let Some(cell) = canvas.get(x, y) else { break };
            if cell.sym.is_empty() {
                x += 1;
                continue;
            }
            // Group consecutive cells with the same style.
            let st = cell.style;
            let mut run = String::new();
            let start = x;
            while x < canvas.width && (x as u16) < area.width {
                let Some(c) = canvas.get(x, y) else { break };
                if c.style != st {
                    break;
                }
                if !c.sym.is_empty() {
                    run.push_str(&c.sym);
                }
                x += 1;
            }
            buf.set_stringn(
                area.x + start as u16,
                area.y + dy as u16,
                &run,
                (area.width as usize).saturating_sub(start),
                style(st),
            );
        }
    }
}

/// Blank cells kept between the terminal edge and the drawing when the
/// document does not set `padding` (`:pad V H`).
pub const DEFAULT_PADDING: (u16, u16) = (0, 1);

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.size = (area.width, area.height);
    if area.width == 0 || area.height == 0 {
        // A terminal that has not reported its size yet (first frame in a
        // pty, a collapsed pane): nothing to draw, and no arithmetic to fail.
        return;
    }
    let th = app.theme.clone();
    let base = Style::default().fg(rgb(th.fg)).bg(rgb(th.bg));
    frame.buffer_mut().set_style(area, base);

    let status_h = if app.present { 0 } else { 1 };
    let (pad_v, pad_h) = app.doc.padding.unwrap_or(DEFAULT_PADDING);
    let pad_h = pad_h.min(area.width.saturating_sub(20) / 2);
    let pad_v = pad_v.min(area.height.saturating_sub(status_h + 3) / 2);
    let canvas_area = Rect {
        x: area.x + pad_h,
        y: area.y + pad_v,
        width: area.width.saturating_sub(pad_h * 2),
        height: area
            .height
            .saturating_sub(status_h)
            .saturating_sub(pad_v * 2),
    };

    let layout = app.layout(canvas_area.width as usize);
    let total = layout.rows.len();
    let visible = canvas_area.height as usize;

    // Keep the cursor visible.
    if let Some(&row) = layout.row_of_line.get(&app.cursor_line) {
        if row < app.scroll {
            app.scroll = row;
        } else if visible > 0 && row >= app.scroll + visible {
            app.scroll = row + 1 - visible;
        }
    }
    app.scroll = app.scroll.min(total.saturating_sub(visible));
    let end = (app.scroll + visible).min(total);
    blit(
        &layout.canvas,
        app.scroll..end,
        canvas_area,
        frame.buffer_mut(),
    );

    // Cursor line indicator, selection and cursor cell (only outside presentation mode).
    if !app.present {
        let sel = app.selection();
        let sel_bg = rgb(codedraw_core::theme::blend(th.accent, th.bg, 0.35));
        if let Some(Selection::Lines(a, b)) = sel {
            for line in a..=b {
                if let Some(&row) = layout.row_of_line.get(&line)
                    && row >= app.scroll
                    && row < end
                {
                    let y = canvas_area.y + (row - app.scroll) as u16;
                    let x0 = canvas_area.x + layout.code_x as u16;
                    let w = canvas_area.width.saturating_sub(layout.code_x as u16);
                    frame.buffer_mut().set_style(
                        Rect {
                            x: x0,
                            y,
                            width: w,
                            height: 1,
                        },
                        Style::default().bg(sel_bg),
                    );
                }
            }
        }
        if let Some(&row) = layout.row_of_line.get(&app.cursor_line)
            && row >= app.scroll
            && row < end
        {
            let y = canvas_area.y + (row - app.scroll) as u16;
            frame.buffer_mut().set_string(
                canvas_area.x,
                y,
                "▍",
                Style::default().fg(rgb(th.accent)).bg(rgb(th.bg)),
            );
            let text = app.doc.line_text(app.cursor_line).unwrap_or("");
            if let Some(Selection::Span(sp)) = sel {
                let x0 = layout.code_x + codedraw_core::layout::col_to_x(text, sp.start, 4);
                let x1 = layout.code_x + codedraw_core::layout::col_to_x(text, sp.end, 4);
                if x1 > x0 && (x0 as u16) < canvas_area.width {
                    let w = (x1 - x0).min(canvas_area.width as usize - x0) as u16;
                    frame.buffer_mut().set_style(
                        Rect {
                            x: canvas_area.x + x0 as u16,
                            y,
                            width: w,
                            height: 1,
                        },
                        Style::default().bg(sel_bg),
                    );
                }
            }
            let col = app
                .cursor_col
                .min(app.doc.line_len(app.cursor_line).saturating_sub(1));
            let cx = layout.code_x + codedraw_core::layout::col_to_x(text, col, 4);
            if (cx as u16) < canvas_area.width {
                frame.buffer_mut().set_style(
                    Rect {
                        x: canvas_area.x + cx as u16,
                        y,
                        width: 1,
                        height: 1,
                    },
                    Style::default().add_modifier(Modifier::REVERSED),
                );
            }
        }
    } else if app.doc.max_step() > 1 || app.doc.step == 0 {
        // Discreet step counter in presentation mode.
        let txt = format!(" {}/{} ", app.doc.step, app.doc.max_step());
        let w = txt.width() as u16;
        if area.width > w {
            frame.buffer_mut().set_string(
                area.x + area.width - w,
                area.y + area.height - 1,
                &txt,
                Style::default().fg(rgb(th.muted)).bg(rgb(th.bg)),
            );
        }
    }

    if !app.present {
        draw_status(
            frame,
            app,
            Rect {
                x: area.x,
                y: area.y + area.height - 1,
                width: area.width,
                height: 1,
            },
        );
    }

    match app.overlay {
        Overlay::Help => draw_overlay(frame, app, "Keys and commands", help_lines()),
        Overlay::List => {
            let mut lines: Vec<String> = app.doc.annotations.iter().map(describe).collect();
            if lines.is_empty() {
                lines.push("(no annotations yet; try :mark move 3:s)".into());
            }
            draw_overlay(frame, app, "Annotations", lines)
        }
        Overlay::None => {}
    }
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let bar = Style::default().fg(rgb(th.status_fg)).bg(rgb(th.status_bg));
    let accent = Style::default()
        .fg(rgb(th.accent))
        .bg(rgb(th.status_bg))
        .add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(rgb(th.muted)).bg(rgb(th.status_bg));
    frame.buffer_mut().set_style(area, bar);

    if let Mode::Command(input) = &app.mode {
        let cmd = Style::default().fg(rgb(th.fg)).bg(rgb(th.cmd_bg));
        frame.buffer_mut().set_style(area, cmd);
        let line = Line::from(vec![
            Span::styled(":", Style::default().fg(rgb(th.accent)).bg(rgb(th.cmd_bg))),
            Span::styled(input.as_str(), cmd),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        if let Some((m, _)) = &app.message {
            let w = m.width() as u16;
            if area.width > input.width() as u16 + 4 + w {
                frame.buffer_mut().set_string(
                    area.x + area.width - w - 1,
                    area.y,
                    m,
                    Style::default().fg(rgb(th.muted)).bg(rgb(th.cmd_bg)),
                );
            }
        }
        frame.set_cursor_position((area.x + 1 + input.width() as u16, area.y));
        return;
    }

    let mode = match app.mode {
        Mode::Select { linewise: true, .. } => "V-LINE",
        Mode::Select { .. } => "SELECT",
        _ => {
            if app.arrow_from.is_some() {
                "ARROW→"
            } else {
                "NORMAL"
            }
        }
    };
    let file = app
        .doc
        .source
        .path
        .as_deref()
        .map(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string())
        })
        .unwrap_or_else(|| "(stdin)".into());
    let mut left = vec![
        Span::styled(" ╭▶ ", accent),
        Span::styled(mode, accent),
        Span::styled(
            format!("  step {}/{}", app.doc.step, app.doc.max_step()),
            bar,
        ),
        Span::styled(format!("  {}×{}", app.size.0, app.size.1), muted),
        Span::styled(format!("  {}", theme_label(th.name)), muted),
        Span::styled(format!("  {file}"), bar),
        Span::styled(format!("  L{}", app.cursor_line), muted),
        Span::styled(format!("  {} ann.", app.doc.annotations.len()), muted),
    ];
    if app.dirty {
        left.push(Span::styled(" •", accent));
    }
    let warnings = app.viewport_warnings();
    if let Some(w) = warnings.first() {
        left.push(Span::styled(
            format!("  ⚠ {w}"),
            Style::default()
                .fg(rgb(th.error))
                .bg(rgb(th.status_bg))
                .add_modifier(Modifier::BOLD),
        ));
    }
    let right: Vec<Span> = match &app.message {
        Some((m, is_err)) => vec![Span::styled(
            format!("{m} "),
            if *is_err {
                Style::default()
                    .fg(rgb(th.error))
                    .bg(rgb(th.status_bg))
                    .add_modifier(Modifier::BOLD)
            } else {
                bar
            },
        )],
        None if app.selected.is_some() => vec![Span::styled(
            format!(
                "▸ {}   Enter label  e set  x delete  c color  +/- step  Tab next  Esc ",
                app.selected_text().unwrap_or_default()
            ),
            accent,
        )],
        None => vec![Span::styled(
            if matches!(app.mode, Mode::Select { .. }) {
                "h highlight  m mark  n note  C callout  b box  a arrow  s spotlight  Esc "
            } else {
                "v select  i word  :cmd  ?help  L list  n/p steps  t theme  P present  q quit "
            },
            muted,
        )],
    };
    // On a narrow bar, drop the least useful segments (theme, cursor line,
    // annotation count, file) so a warning never gets cut off.
    if !warnings.is_empty() {
        for drop_idx in [4usize, 6, 7, 5] {
            let lw: usize = left.iter().map(|s| s.content.width()).sum();
            if lw <= area.width as usize {
                break;
            }
            if drop_idx < left.len() {
                left[drop_idx] = Span::styled("", bar);
            }
        }
    }
    let lw: usize = left.iter().map(|s| s.content.width()).sum();
    let mut right = right;
    let mut rw: usize = right.iter().map(|s| s.content.width()).sum();
    if lw + rw > area.width as usize && app.message.is_none() {
        right = vec![Span::styled("?help ", muted)];
        rw = right.iter().map(|s| s.content.width()).sum();
    }
    if lw + rw > area.width as usize {
        right.clear();
        rw = 0;
    }
    let gap = (area.width as usize).saturating_sub(lw + rw);
    let mut spans = left;
    spans.push(Span::styled(" ".repeat(gap), bar));
    spans.extend(right);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_overlay(frame: &mut Frame, app: &App, title: &str, lines: Vec<String>) {
    let th = &app.theme;
    let area = frame.area();
    let w = (area.width.saturating_sub(4)).clamp(20, 96);
    let h = (lines.len() as u16 + 2)
        .min(area.height.saturating_sub(2))
        .max(3);
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(rgb(th.accent)))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(rgb(th.accent))
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(
            Line::from(Span::styled(
                " Esc closes ",
                Style::default().fg(rgb(th.muted)),
            ))
            .right_aligned(),
        )
        .style(Style::default().fg(rgb(th.fg)).bg(rgb(th.cmd_bg)));
    let text: Vec<Line> = lines.into_iter().map(Line::from).collect();
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        rect,
    );
}

fn help_lines() -> Vec<String> {
    [
        "j/k ↑/↓ line   h/l ←/→ column   w/b/e words   0/$ line start/end   g/G   J/K page",
        "v  select text   V  select lines   i  select the word under the cursor   Esc  cancel",
        "Tab/Shift-Tab  select an annotation: Enter edit label  x delete  c color  +/- move step  Esc",
        "  with a selection:  h highlight   m mark (then type KIND [label])   n note   b box",
        "                     a arrow (select source, a, select target, a)   s spotlight",
        "n/p      next/previous step (0 = code only)   u / U  undo / redo",
        "t        dark/light theme            P      presentation mode",
        "l        annotation list             :      command line",
        "",
        "Commands (L = line number, text = fragment to find on that line):",
        "  :mark KIND[@COLOR] L:text [label]   KIND: info warn error ok question important read write move new drop scope flow todo",
        "        …or a :style name, or a bare glyph/emoji for a one-off mark (:mark 🔥 7:&t hot path)",
        "  :style NAME glyph=G legend=\"text\" color=C   define a reusable style   :style  lists them",
        "  :arrow L:text L:text [label]             :hl L:text [label]",
        "  :arrow … [label] stroke=dashed|dotted|thick|double heads=START,END route=sharp   (heads: none arrow dot bar open)",
        "  :set ID key=value…   label, color, step; arrows: stroke, route, heads, start, end   (selection: e)",
        "  :box A-B [label]                         :note L text   (L may be .)",
        "  :spot A-B [ID…] | :spot ID…   dims everything else in this step   :until ID N",
        "  :callout A[-B] [@below|@margin|@auto] [title=\"…\"] text   bordered paragraph (selection: C)",
        "  :step N | :step ID N | :next | :prev     :del ID…  :clear [N|.]  :label ID text",
        "  :color ID blue|verde|#ff8800|auto   (orange blue green purple pink cyan yellow red gray)",
        "  :focus A-B | :focus     :theme dark|light     :lang rs     :w [path]   :q",
        "  :embed  freezes the code inside the .codedraw   :link [path]  references a file",
        "",
        "New annotations appear at the current step. The local API (socket) and the",
        "MCP server (`codedraw mcp`) do exactly the same thing as these commands.",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
