//! Dibujo con ratatui: copia el `Canvas` del núcleo al buffer, y añade la
//! línea de estado, la línea de comandos y las superposiciones.

use codedraw_core::canvas::{Canvas, CellStyle, Rgb};
use codedraw_core::protocol::describe;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode, Overlay, theme_label};

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

/// Copia las filas `rows` del lienzo a `area` del buffer.
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
            // Agrupar celdas consecutivas con el mismo estilo.
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

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.size = (area.width, area.height);
    let th = app.theme.clone();
    let base = Style::default().fg(rgb(th.fg)).bg(rgb(th.bg));
    frame.buffer_mut().set_style(area, base);

    let status_h = if app.present { 0 } else { 1 };
    let canvas_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(status_h),
    };

    let layout = app.layout(canvas_area.width as usize);
    let total = layout.rows.len();
    let visible = canvas_area.height as usize;

    // Mantener el cursor visible.
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

    // Indicador de línea del cursor (solo fuera de presentación).
    if !app.present {
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
        }
    } else if app.doc.max_step() > 1 || app.doc.step == 0 {
        // Contador de paso discreto en presentación.
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
        Overlay::Help => draw_overlay(frame, app, "Atajos y comandos", help_lines()),
        Overlay::List => {
            let mut lines: Vec<String> = app.doc.annotations.iter().map(describe).collect();
            if lines.is_empty() {
                lines.push("(sin anotaciones todavía; prueba :mark move 3:s)".into());
            }
            draw_overlay(frame, app, "Anotaciones", lines)
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
        frame.set_cursor_position((area.x + 1 + input.width() as u16, area.y));
        return;
    }

    let mode = "NORMAL";
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
            format!("  paso {}/{}", app.doc.step, app.doc.max_step()),
            bar,
        ),
        Span::styled(format!("  {}×{}", app.size.0, app.size.1), muted),
        Span::styled(format!("  {}", theme_label(th.name)), muted),
        Span::styled(format!("  {file}"), bar),
        Span::styled(format!("  L{}", app.cursor_line), muted),
        Span::styled(format!("  {} anot.", app.doc.annotations.len()), muted),
    ];
    if app.dirty {
        left.push(Span::styled(" •", accent));
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
        None => vec![Span::styled(
            ":cmd  ?ayuda  l lista  n/p pasos  t tema  P presentar  q salir ",
            muted,
        )],
    };
    let lw: usize = left.iter().map(|s| s.content.width()).sum();
    let mut right = right;
    let mut rw: usize = right.iter().map(|s| s.content.width()).sum();
    if lw + rw > area.width as usize && app.message.is_none() {
        right = vec![Span::styled("?ayuda ", muted)];
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
                " Esc cierra ",
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
        "j/k ↑/↓  mover cursor      g/G  inicio/fin      J/K  página",
        "n/p      paso siguiente/anterior (0 = solo código)   u / U  deshacer / rehacer",
        "t        tema oscuro/claro           P      modo presentación",
        "l        lista de anotaciones        :      línea de comandos",
        "",
        "Comandos (L = número de línea, texto = fragmento a buscar en esa línea):",
        "  :mark move|borrow|mut|drop|life L:texto [etiqueta]",
        "  :arrow L:texto L:texto [etiqueta]        :hl L:texto [etiqueta]",
        "  :box A-B [etiqueta]                      :note L texto   (L puede ser .)",
        "  :step N | :step ID N | :next | :prev     :del ID…  :clear [N|.]  :label ID texto",
        "  :color ID blue|verde|#ff8800|auto   (orange blue green purple pink cyan yellow red gray)",
        "  :focus A-B | :focus     :theme dark|light     :lang rs     :w [ruta]   :q",
        "  :embed  congela el código en el .codedraw   :link [ruta]  referencia un archivo",
        "",
        "Las anotaciones nuevas aparecen en el paso actual. La API local (socket) y",
        "el servidor MCP (`codedraw mcp`) hacen exactamente lo mismo que estos comandos.",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
