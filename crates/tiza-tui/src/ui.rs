//! Drawing with ratatui: copies the core `Canvas` into the buffer and adds the
//! status line, the command line and the overlays.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use tiza_core::canvas::{Canvas, CellStyle, Rgb};
use tiza_core::layout::Layout;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode, Overlay, Selection};

/// A canvas colour for ratatui: real colours as they are, the host-palette
/// sentinels of the `terminal` theme (t-119) as the terminal's own.
fn rgb(c: Rgb) -> Color {
    match tiza_core::theme::host_color(c) {
        Some(tiza_core::theme::HostColor::Default) => Color::Reset,
        Some(tiza_core::theme::HostColor::Indexed(n)) => Color::Indexed(n),
        None => Color::Rgb(c.0, c.1, c.2),
    }
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

/// The geometry a mouse click needs: screen origin, the code line of each
/// visible row and where the code text starts.
fn tiza_tui_hitmap(area: Rect, layout: &Layout, scroll: usize, end: usize) -> crate::app::HitMap {
    crate::app::HitMap {
        origin: (area.x, area.y),
        rows: layout.rows[scroll.min(end)..end]
            .iter()
            .map(|r| match r {
                tiza_core::layout::Row::Code(l) => Some(*l),
                _ => None,
            })
            .collect(),
        code_x: area.x + layout.code_x as u16,
        tab_width: 4,
    }
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

/// Padding that fits a terminal of `size`: the code keeps at least 20 columns
/// and three rows plus the status line.
pub fn clamp_padding(pad: (u16, u16), size: (u16, u16), present: bool) -> (u16, u16) {
    let status_h = if present { 0 } else { 1 };
    (
        pad.0.min(size.1.saturating_sub(status_h + 3) / 2),
        pad.1.min(size.0.saturating_sub(20) / 2),
    )
}

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
    let (pad_v, pad_h) = clamp_padding(
        app.effective_padding(),
        (area.width, area.height),
        app.present,
    );
    let canvas_area = Rect {
        x: area.x + pad_h,
        y: area.y + pad_v,
        width: area.width.saturating_sub(pad_h * 2),
        height: area
            .height
            .saturating_sub(status_h)
            .saturating_sub(pad_v * 2),
    };

    // A motion the pointer has not seen yet (mouse, agent): follow it.
    app.sync_pointer();
    let layout = app.screen_layout_with_pointer(canvas_area.width as usize);
    let total = layout.rows.len();
    let visible = canvas_area.height as usize;

    // Keep the cursor visible — with the virtual rows that belong to its
    // line: the title rows of boxes that open on it (above) and the rows
    // under it (emphasis, labels, bottom borders) when they fit (t-114).
    // Otherwise a box whose first line is the first visible row loses its
    // top border and title.
    if let Some(&row) = layout.row_of_line.get(&app.cursor_line) {
        let mut top = row;
        while top > 0 && matches!(layout.rows[top - 1], tiza_core::layout::Row::BoxTop(_)) {
            top -= 1;
        }
        let mut bottom = row;
        while bottom + 1 < layout.rows.len()
            && !matches!(layout.rows[bottom + 1], tiza_core::layout::Row::Code(_))
            && bottom + 1 - top < visible
        {
            bottom += 1;
        }
        if top < app.scroll {
            app.scroll = top;
        } else if visible > 0 && bottom >= app.scroll + visible {
            app.scroll = bottom + 1 - visible;
        }
        // The cursor row itself always wins over its trailing rows.
        if row < app.scroll {
            app.scroll = row;
        } else if visible > 0 && row >= app.scroll + visible {
            app.scroll = row + 1 - visible;
        }
    }
    app.scroll = app.scroll.min(total.saturating_sub(visible));
    let end = (app.scroll + visible).min(total);
    app.hit = tiza_tui_hitmap(canvas_area, &layout, app.scroll, end);
    blit(
        &layout.canvas,
        app.scroll..end,
        canvas_area,
        frame.buffer_mut(),
    );

    // Cursor line indicator, selection and cursor cell (only outside presentation mode).
    if !app.present {
        let sel = app.selection();
        let sel_bg = rgb(th.tint(th.accent, 0.35));
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
            // The selected annotation (Tab): ▸ in the gutter of its first line, so
            // the selection is visible even when the status line has no room for it.
            if let Some(sel_id) = &app.selected
                && let Some(line) = app
                    .doc
                    .find_annotation(sel_id)
                    .and_then(|a| a.shape.line_ranges().first().map(|r| r.0))
                && let Some(&srow) = layout.row_of_line.get(&line)
                && srow >= app.scroll
                && srow < end
            {
                frame.buffer_mut().set_string(
                    canvas_area.x,
                    canvas_area.y + (srow - app.scroll) as u16,
                    "▸",
                    Style::default()
                        .fg(rgb(th.accent))
                        .bg(rgb(th.bg))
                        .add_modifier(Modifier::BOLD),
                );
            }
            // The cursor line: its number in the accent color, bold, with no
            // glyph of its own so the gutter keeps every digit.
            let gutter_w = (layout.code_x as u16).min(canvas_area.width);
            frame.buffer_mut().set_style(
                Rect {
                    x: canvas_area.x,
                    y,
                    width: gutter_w,
                    height: 1,
                },
                Style::default()
                    .fg(rgb(th.accent))
                    .add_modifier(Modifier::BOLD),
            );
            let text = app.doc.line_text(app.cursor_line).unwrap_or("");
            if let Some(Selection::Span(sp)) = sel {
                let x0 = layout.code_x + tiza_core::layout::col_to_x(text, sp.start, 4);
                let x1 = layout.code_x + tiza_core::layout::col_to_x(text, sp.end, 4);
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
            let cx = layout.code_x + tiza_core::layout::col_to_x(text, col, 4);
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
    } else {
        if app.doc.max_step() > 1 || app.doc.step == 0 {
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
        if !app.proposals.is_empty() {
            // An agent is waiting: the chip is the only sign in presentation
            // mode (no popup), bottom-left, in the accent color.
            let txt = format!(
                " ⏸ {} to review (:review, or P then y/n) ",
                app.proposals.len()
            );
            let w = txt.width() as u16;
            if area.width > w {
                frame.buffer_mut().set_string(
                    area.x,
                    area.y + area.height - 1,
                    &txt,
                    Style::default()
                        .fg(rgb(th.accent))
                        .bg(rgb(th.bg))
                        .add_modifier(Modifier::BOLD),
                );
            }
        }
    }

    let last_row = Rect {
        x: area.x,
        y: area.y + area.height - 1,
        width: area.width,
        height: 1,
    };
    if !app.present {
        draw_status(frame, app, &layout, last_row);
    } else if let Mode::Command(input) = &app.mode {
        // Presentation mode has no status bar, but what is typed after `:`
        // and its result must be seen (t-107): the last row, only then.
        draw_command_line(frame, app, input, last_row);
    } else if app.present_message_shown() {
        draw_present_message(frame, app, last_row);
    }
    match app.overlay {
        Overlay::Help => draw_overlay(frame, app, "╭─ Tiza ─▶  keys and commands", help_lines()),
        Overlay::List => {
            let mut lines: Vec<String> = app.list_lines();
            if lines.is_empty() {
                lines.push("(no annotations yet; try :mark move 3:s)".into());
            }
            if !app.doc.moves.is_empty() {
                lines.push(String::new());
                lines.push("Moves (code changes between steps):".into());
                lines.extend(
                    app.doc
                        .moves
                        .iter()
                        .enumerate()
                        .map(|(i, m)| tiza_core::protocol::describe_move(i + 1, m)),
                );
            }
            draw_overlay(frame, app, "Annotations", lines)
        }
        Overlay::Review => {
            let mut lines: Vec<String> = app
                .proposals
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    format!(
                        "{} p{}  {}  ({}s ago)",
                        if i == 0 { "▶" } else { " " },
                        p.id,
                        p.summary,
                        p.since.elapsed().as_secs()
                    )
                })
                .collect();
            lines.push(String::new());
            lines.push(
                "y accept · n reject · a accept all · d reject all · Esc hide (:review reopens)"
                    .into(),
            );
            draw_overlay(frame, app, "Agent proposals", lines)
        }
        Overlay::None => {}
    }
}

/// The `:` line being typed, with completions and notices on the right:
/// the status bar's place outside presentation mode, the last row in it
/// (t-107).
fn draw_command_line(frame: &mut Frame, app: &App, input: &str, area: Rect) {
    let th = &app.theme;
    let cmd = Style::default().fg(rgb(th.fg)).bg(rgb(th.cmd_bg));
    // In presentation mode the row already holds code or the step counter.
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.buffer_mut().set_style(area, cmd);
    let line = Line::from(vec![
        Span::styled(":", Style::default().fg(rgb(th.accent)).bg(rgb(th.cmd_bg))),
        Span::styled(input, cmd),
    ]);
    frame.render_widget(Paragraph::new(line), area);
    if let Some((m, _)) = &app.message {
        // Completion candidates and notices: as much as fits, with an ellipsis.
        let room = (area.width as usize).saturating_sub(input.width() + 4);
        let text = if m.width() <= room {
            m.clone()
        } else if room >= 4 {
            let mut cut = String::new();
            let mut w = 0;
            for g in m.graphemes(true) {
                if w + g.width() > room - 1 {
                    break;
                }
                cut.push_str(g);
                w += g.width();
            }
            format!("{cut}…")
        } else {
            String::new()
        };
        let w = text.width() as u16;
        if w > 0 {
            frame.buffer_mut().set_string(
                area.x + area.width - w - 1,
                area.y,
                &text,
                Style::default().fg(rgb(th.muted)).bg(rgb(th.cmd_bg)),
            );
        }
    }
    // A long input on a narrow terminal keeps the cursor on the row.
    let cx = (1 + input.width()).min(area.width.saturating_sub(1) as usize) as u16;
    frame.set_cursor_position((area.x + cx, area.y));
}

/// `m` cut to `room` cells with an ellipsis, keeping the beginning — the
/// useful part of a message. A step hint at the end (`(step 1, press n)`,
/// t-132) survives the cut when it fits with at least one cell of the rest:
/// `a17: box 3-… (step 1, press n)`.
pub fn fit_message(m: &str, room: usize) -> String {
    if m.width() <= room {
        return m.to_string();
    }
    if room == 0 {
        return String::new();
    }
    let cut = |s: &str, max: usize| {
        let mut out = String::new();
        let mut w = 0;
        for g in s.graphemes(true) {
            if w + g.width() > max {
                break;
            }
            out.push_str(g);
            w += g.width();
        }
        out
    };
    if let Some(i) = crate::app::step_hint_start(m) {
        let (head, tail) = m.split_at(i);
        let tw = tail.width();
        if tw + 2 <= room {
            return format!("{}…{tail}", cut(head, room - tw - 1).trim_end());
        }
    }
    format!("{}…", cut(m, room - 1))
}

/// The result of the command just run, on the last row in presentation
/// mode, for [`crate::app::PRESENT_MESSAGE_TTL`] (t-107): the status bar's
/// text on the command line's row, nothing else of the bar.
fn draw_present_message(frame: &mut Frame, app: &App, area: Rect) {
    let Some((m, is_err)) = &app.message else {
        return;
    };
    let th = &app.theme;
    let style = if *is_err {
        Style::default()
            .fg(rgb(th.error))
            .bg(rgb(th.cmd_bg))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(rgb(th.fg)).bg(rgb(th.cmd_bg))
    };
    frame.render_widget(ratatui::widgets::Clear, area);
    frame
        .buffer_mut()
        .set_style(area, Style::default().fg(rgb(th.fg)).bg(rgb(th.cmd_bg)));
    // Keep the beginning, with an ellipsis, as the status bar does.
    let room = (area.width as usize).saturating_sub(2);
    let text = if room >= 2 {
        fit_message(m, room)
    } else {
        String::new()
    };
    frame
        .buffer_mut()
        .set_string(area.x + 1.min(area.width), area.y, &text, style);
}

fn draw_status(frame: &mut Frame, app: &App, layout: &Layout, area: Rect) {
    let th = &app.theme;
    let bar = Style::default().fg(rgb(th.status_fg)).bg(rgb(th.status_bg));
    let accent = Style::default()
        .fg(rgb(th.accent))
        .bg(rgb(th.status_bg))
        .add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(rgb(th.muted)).bg(rgb(th.status_bg));
    frame.buffer_mut().set_style(area, bar);

    if let Mode::Command(input) = &app.mode {
        draw_command_line(frame, app, input, area);
        return;
    }

    let mode = match app.mode {
        Mode::Select { linewise: true, .. } => "V-LINE",
        Mode::Select { .. } => "SELECT",
        _ => {
            if app.arrow_from.is_some() {
                "ARROW→"
            } else if app.pointer.is_some() {
                "POINTER"
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
        .or_else(|| {
            // Embedded code: name the document instead of "(stdin)".
            app.save_path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "(stdin)".into());
    // While a `code.link` shows another file, name it (with its window).
    let file = app.shown_view().map(|l| format!("↪ {l}")).unwrap_or(file);
    let mut left = vec![
        Span::styled(" ╭▶ ", accent),
        Span::styled(mode, accent),
        Span::styled(
            // `step 3/7 · Shared borrow` on a named step (t-099); a long
            // title is cut so the rest of the bar keeps its room.
            match app.doc.step_title(app.doc.step) {
                Some(t) => format!(
                    "  step {}/{} · {}",
                    app.doc.step,
                    app.doc.max_step(),
                    fit_message(&t, 32)
                ),
                None => format!("  step {}/{}", app.doc.step, app.doc.max_step()),
            },
            bar,
        ),
        Span::styled(format!("  {}×{}", app.size.0, app.size.1), muted),
        Span::styled(format!("  {}", app.theme_status()), muted),
        Span::styled(format!("  {file}"), bar),
        Span::styled(format!("  L{}", app.cursor_line), muted),
        Span::styled(format!("  {} ann.", app.doc.annotations.len()), muted),
    ];
    if app.dirty {
        left.push(Span::styled(" •", accent));
    }
    if app.layout_stable && !app.present {
        left.push(Span::styled("  ⊞ stable", muted));
    }
    // One indicator for every way of not writing the sidecar (t-110): a
    // read-only file, `--no-save`, `:autosave off`, a foreign `scene put`.
    if app.save_error.is_some() {
        left.push(Span::styled(
            if app.dirty {
                "  read-only • unsaved (:w)"
            } else {
                "  read-only"
            },
            Style::default()
                .fg(rgb(th.error))
                .bg(rgb(th.status_bg))
                .add_modifier(Modifier::BOLD),
        ));
    } else if !app.autosave {
        let text = if app.dirty {
            "  unsaved (:w)"
        } else {
            "  no-save"
        };
        left.push(Span::styled(text, if app.dirty { accent } else { muted }));
    }
    if app.supervised {
        let text = if app.proposals.is_empty() {
            "  ⏸ supervised".to_string()
        } else {
            format!("  ⏸ {} to review (y/n)", app.proposals.len())
        };
        left.push(Span::styled(text, accent));
    }
    // A notice about stale moves names every one of them: it takes the room
    // of the optional segments, the time of the last reload included.
    let stale = app.stale_message_shown();
    if let Some(f) = &app.follow {
        let text = match (&f.missing, &f.last_reload_clock) {
            (true, _) => "  ⟳ following (file removed; waiting)".to_string(),
            (false, Some(at)) if !stale => format!("  ⟳ following · last {at}"),
            (false, _) => "  ⟳ following".to_string(),
        };
        left.push(Span::styled(text, accent));
    }
    let warnings = app.viewport_warnings_for(layout);
    if let Some(w) = warnings.first() {
        // Short form on the bar; the full text is in `status.warnings`.
        let short = if w.starts_with("code clipped") {
            w.clone()
        } else if w.contains("under the lines") {
            "labels under the lines".to_string()
        } else {
            "small terminal".to_string()
        };
        left.push(Span::styled(
            format!("  ⚠ {short}"),
            Style::default()
                .fg(rgb(th.error))
                .bg(rgb(th.status_bg))
                .add_modifier(Modifier::BOLD),
        ));
    }
    // What goes on the right, by priority: the last message, then the
    // selection hint, then the key hints. A failed save is not repeated
    // here: it was said once and the left indicator keeps it (t-110).
    let err_style = Style::default()
        .fg(rgb(th.error))
        .bg(rgb(th.status_bg))
        .add_modifier(Modifier::BOLD);
    let message = app.message.as_ref();
    let (right_text, right_style, essential): (String, Style, bool) =
        match (&app.save_error, message) {
            (_, Some((m, is_err))) => {
                (format!("{m} "), if *is_err { err_style } else { bar }, true)
            }
            (_, None) if app.selected.is_some() => (
                format!(
                    "▸ {}   Enter label  e set  x delete  c color  +/- step  Tab next  Esc ",
                    app.selected_text().unwrap_or_default()
                ),
                accent,
                true,
            ),
            (_, None) => (
                if matches!(app.mode, Mode::Select { .. }) {
                    "h hl  m mark  n note  C callout  b box  a arrow  s spot  Esc "
                } else {
                    "v select  :cmd  ?help  L list  n/p steps  t theme  P present  q quit "
                }
                .to_string(),
                muted,
                false,
            ),
        };
    // Make room: drop the optional left segments (theme, cursor line,
    // annotation count, file) before cutting anything that matters, then
    // truncate the right text with an ellipsis instead of hiding it.
    let width = area.width as usize;
    let lw = |left: &Vec<Span>| left.iter().map(|s| s.content.width()).sum::<usize>();
    let need = if stale {
        right_text.width()
    } else if essential {
        right_text.width().min(width / 2)
    } else {
        0
    };
    for drop_idx in [4usize, 6, 7, 5, 3] {
        if lw(&left) + need <= width {
            break;
        }
        if drop_idx < left.len() {
            left[drop_idx] = Span::styled("", bar);
        }
    }
    // Hints are optional, but `?help` should find room when giving up only
    // the theme and the size (cursor line and count are worth more).
    if !essential {
        for drop_idx in [3usize, 4] {
            if lw(&left) + "?help ".len() <= width {
                break;
            }
            left[drop_idx] = Span::styled("", bar);
        }
    }
    // One blank cell between the left segments and the message.
    let room = width.saturating_sub(lw(&left) + 1);
    let mut right: Vec<Span> = Vec::new();
    if essential {
        if right_text.width() <= room {
            right.push(Span::styled(right_text, right_style));
        } else if room >= 4 {
            // Keep the beginning: that is where the useful part of a message is.
            let cut = fit_message(right_text.trim_end_matches(' '), room - 1);
            right.push(Span::styled(format!("{cut} "), right_style));
        }
    } else if right_text.width() <= room {
        right.push(Span::styled(right_text, right_style));
    } else {
        // Shorter hint tiers before giving up on all but `?help`.
        let tiers: &[&str] = if matches!(app.mode, Mode::Select { .. }) {
            &[
                "h hl  m mark  n note  b box  a arrow  Esc ",
                "h m n b a  Esc ",
                "?help ",
            ]
        } else {
            &[
                ":cmd  ?help  n/p steps  q quit ",
                "?help  q quit ",
                "?help ",
            ]
        };
        if let Some(t) = tiers.iter().find(|t| t.width() <= room) {
            right.push(Span::styled(*t, muted));
        }
    }
    let rw: usize = right.iter().map(|s| s.content.width()).sum();
    let gap = width.saturating_sub(lw(&left) + rw);
    let mut spans = left;
    spans.push(Span::styled(" ".repeat(gap), bar));
    spans.extend(right);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Number of overlay rows a set of lines takes at `inner_w` columns (wrapped).
pub fn overlay_rows(lines: &[String], inner_w: usize) -> usize {
    lines
        .iter()
        .map(|l| {
            let w = l.width();
            if w == 0 {
                1
            } else {
                w.div_ceil(inner_w.max(1))
            }
        })
        .sum()
}

fn draw_overlay(frame: &mut Frame, app: &App, title: &str, lines: Vec<String>) {
    let th = &app.theme;
    let area = frame.area();
    if area.width < 8 || area.height < 4 {
        // Too small for a bordered box: nothing to draw (and no arithmetic to fail).
        return;
    }
    let w = (area.width.saturating_sub(4)).clamp(4, 96);
    let inner_w = w.saturating_sub(2) as usize;
    let total = overlay_rows(&lines, inner_w);
    let h = (total as u16 + 2).min(area.height.saturating_sub(2)).max(3);
    let inner_h = h.saturating_sub(2) as usize;
    // Scroll in wrapped rows; j/k, ↑/↓ and PgDn/PgUp move it.
    let max_scroll = total.saturating_sub(inner_h);
    let scroll = app.overlay_scroll.min(max_scroll);
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
                if max_scroll > 0 {
                    format!(
                        " j/k scroll ({}/{})  Esc closes ",
                        scroll + inner_h.min(total),
                        total
                    )
                } else {
                    " Esc closes ".to_string()
                },
                Style::default().fg(rgb(th.muted)),
            ))
            .right_aligned(),
        )
        .style(Style::default().fg(rgb(th.fg)).bg(rgb(th.cmd_bg)));
    let text: Vec<Line> = lines.into_iter().map(Line::from).collect();
    frame.render_widget(
        Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll as u16, 0)),
        rect,
    );
}

fn help_lines() -> Vec<String> {
    [
        "j/k ↑/↓ line   h/l column   w/b/e words   0/$ line start/end   g/G   J/K page",
        "n/p →/← PgDn/PgUp  next/previous step (0 = code only)",
        "v  select text   V  select lines   i  select the word under the cursor   Esc  cancel",
        "Tab/Shift-Tab  select an annotation: Enter edit label  x delete  c color  +/- move step  Esc",
        "  with a selection:  h highlight   m mark (then type KIND [label])   n note   b box",
        "                     a arrow (select source, a, select target, a)   s spotlight",
        "u / U    undo / redo                 t      dark/light theme",
        "P        presentation mode           L      annotation list      :  command line",
        "f        pointer: the cursor line (or the selection) stays lit, the rest dims; j/k move it, f or Esc ends it (live only: never saved nor exported)",
        "         (presenting, : shows the command line on the last row and its result for 2 s)",
        "Ctrl-C   cancels, or quits: closes the command line, a selection, an arrow or this window like Esc; with nothing open it quits as :q!",
        "",
        "Commands (L = line number, text = fragment to find on that line):",
        "  :mark KIND[@COLOR] L:text [label]   KIND: info warn error ok question important read write move new drop scope flow todo",
        "        …or a :style name, or a bare glyph/emoji for a one-off mark (:mark 🔥 7:&t hot path)",
        "  :style NAME glyph=G legend=\"text\" color=C   define a reusable style   :style  lists them",
        "  :arrow L:text L:text [label]             :hl L:text [label] [emphasis=fill|underline|brackets|strike]",
        "  :arrow … [label] stroke=dashed|dotted|thick|double heads=START,END route=sharp   (heads: none arrow dot bar open)",
        "  :set ID key=value…   label, color, step; arrows: stroke, route, heads, start, end; boxes/callouts: border, corners, fill; highlights/marks: emphasis   (selection: e)",
        "  :box … border=double corners=sharp fill=no   box/callout borders (solid dashed dotted thick double; rounded sharp)",
        "  :box A-B [label]                         :note L text   (L may be .)",
        "  :spot A-B [ID…] | :spot ID…   dims everything else in this step   :until ID N",
        "  :callout A[-B] [@below|@margin|@auto] [title=\"…\"] text   bordered paragraph (selection: C)",
        "  :step N | :step ID N | :next | :prev     :del ID…  :clear [N|.]  :label ID text",
        "  :goto ID   jump to a named step (`steps` in the .tiza; `tiza goto ID` from a script)",
        "  :color ID blue|verde|#ff8800|auto   (orange blue green purple pink cyan yellow red gray)",
        "  :focus A-B | :focus     :theme dark|light     :lang rs | :lang auto     :title text",
        "  :pad V H | :pad 0 0 | :pad default   blank cells around the drawing (saved in the document)",
        "  :embed  freezes the code (and every linked file) inside the .tiza   :ref [path[:A-B]]  references a file (:link at step 0 does the same)",
        "  :link path[:A-B]   at step N ≥ 1: from this step the demo shows that file (A-B only crops it); each file keeps its own annotations",
        "  :export [file.txt|.ansi|.md|.svg|.png] [step=N] [width=N] [theme=dark|light]   what this step shows, as a file (default <doc>-stepN.txt)",
        "  :follow on|off   reload the code when the file is saved (annotations follow their text; u undoes a reload)",
        "  :supervise on|off   agent changes wait for your y/n (a all, d none; :review lists them; 60 s then rejected)",
        "  :mouse on|off   click moves the cursor, drag selects, wheel scrolls (off by default: the terminal keeps its own selection)",
        "  :layout stable|dynamic   rehearse with the take's geometry (presentation mode and exports always use it)",
        "  :theme dark|light|terminal|auto   :theme file PATH   fix a theme, use the terminal's palette, follow the terminal, load token overrides",
        "  :numbers on|off   show or hide the line-number gutter (saved in the document; exports follow it)",
        "  :margin N | :margin auto   cap the right margin so long notes wrap earlier (12-200 cells; saved in the document)",
        "  :w [path]   :wq   :q   :q! (quits without saving; also over stale moves)   :autosave on|off (--no-save at start)   :redo   :help",
        "",
        "New annotations appear at the current step. The local API (socket) and the",
        "MCP server (`tiza mcp`) do exactly the same thing as these commands.",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
