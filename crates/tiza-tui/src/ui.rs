//! Drawing with ratatui: copies the core `Canvas` into the buffer and adds the
//! status line, the command line and the overlays.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use tiza_core::canvas::{Canvas, CellStyle, Rgb};
use tiza_core::layout::Layout;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode, Overlay, Selection, ViewEdge, view_anchor_of, view_row_of};

/// A canvas colour for ratatui: real colours as they are, the host-palette
/// sentinels of the `terminal` theme as the terminal's own.
pub(crate) fn rgb(c: Rgb) -> Color {
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

/// Padding that fits a terminal of `size` with `status_h` rows at the
/// bottom that are not code ([`App::status_rows`]): the code keeps at least
/// 20 columns and three rows.
pub fn clamp_padding(pad: (u16, u16), size: (u16, u16), status_h: u16) -> (u16, u16) {
    (
        pad.0.min(size.1.saturating_sub(status_h + 3) / 2),
        pad.1.min(size.0.saturating_sub(20) / 2),
    )
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    // This frame puts the held view of a step change on screen.
    app.held_unresolved = false;
    let area = frame.area();
    app.size = (area.width, area.height);
    app.close_panel_too_small();
    if area.width == 0 || area.height == 0 {
        // A terminal that has not reported its size yet (first frame in a
        // pty, a collapsed pane): nothing to draw, and no arithmetic to fail.
        return;
    }
    let th = app.theme.clone();
    let base = Style::default().fg(rgb(th.fg)).bg(rgb(th.bg));
    frame.buffer_mut().set_style(area, base);

    let status_h = app.status_rows();
    let (pad_v, pad_h) =
        clamp_padding(app.effective_padding(), (area.width, area.height), status_h);
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
    let mut layout = app.screen_layout_with_pointer(canvas_area.width as usize);
    let total = layout.rows.len();
    let visible = canvas_area.height as usize;

    // In presentation mode the view stays on its code: the line on top of
    // the last frame goes back to its row, whatever rows came in or went
    // above it (a step, an agent, another width).
    if app.present
        && let Some(row) = app.view_anchor.and_then(|a| view_row_of(&layout, a))
    {
        app.scroll = row;
    }

    // A view that a `scroll` holds: its target where `align` puts it, on
    // the rows of this frame (the annotations of the step came in, the
    // take changed), until the cursor or the pointer moves.
    if let Some(h) = &app.scroll_held {
        if (app.cursor_line, app.cursor_col) == h.cursor && app.pointer == h.pointer {
            let to_come = app.held_rows_to_come(&layout, canvas_area.width as usize);
            let current = app.scroll;
            let top = app.scroll_held.as_mut().and_then(|h| {
                h.scroll_on(
                    &layout,
                    (visible, canvas_area.width as usize),
                    current,
                    to_come,
                )
            });
            match top {
                Some(top) => app.scroll = top,
                // The target is not on this layout: a patch removed its
                // line, or the window of a `code.link` leaves it out.
                None => app.scroll_held = None,
            }
        } else {
            app.scroll_held = None;
        }
    }

    // Keep the cursor visible — with the virtual rows that belong to its
    // line: the title rows of boxes that open on it (above) and the rows
    // under it (emphasis, labels, bottom borders) when they fit.
    // Otherwise a box whose first line is the first visible row loses its
    // top border and title. A held view already shows the cursor: on the
    // first line of its target, or on a pointer that stayed. In
    // presentation mode the view follows the
    // cursor only when its line changed (an agent, Enter in the list, a
    // `cursor` move), not on every frame: the view stays where the
    // presenter put it. (`Tab` goes only to what the view shows.)
    let follow = !app.present || app.cursor_line != app.view_cursor_line;
    if app.scroll_held.is_none()
        && follow
        && let Some(&row) = layout.row_of_line.get(&app.cursor_line)
    {
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
    // A key of the view in presentation mode, from the view above: it
    // lets a held view go.
    if let Some(r) = app.view_request.take()
        && app.present
    {
        app.scroll_held = None;
        match r.edge {
            Some(ViewEdge::Top) => app.scroll = 0,
            Some(ViewEdge::Bottom) => app.scroll = total.saturating_sub(visible),
            None => {}
        }
        let rows =
            isize::try_from(r.rows).unwrap_or(if r.rows < 0 { isize::MIN } else { isize::MAX });
        app.scroll = app.scroll.saturating_add_signed(rows);
    }
    app.scroll = app.scroll.min(total.saturating_sub(visible));
    // D14 at each frame: a held view keeps its target, also when the
    // terminal gets smaller, and the live pointer stays only where the
    // view shows its lines. So the screen is never dimmed with no lit line.
    if app.scroll_held.is_some()
        && let Some(p) = app.pointer
        && ![p.line, p.to].iter().all(|l| {
            layout
                .row_of_line
                .get(l)
                .is_some_and(|&r| r >= app.scroll && r < app.scroll + visible)
        })
    {
        app.clear_pointer();
        // The held view keeps holding, with the pointer off now; in normal
        // mode the cursor goes to its target, as at a jump (in
        // presentation mode the hidden cursor goes into the view below).
        if let Some(h) = app.scroll_held.as_mut() {
            h.pointer = None;
            if !app.present {
                app.cursor_line = h.lines.0;
                app.cursor_col = 0;
            }
            h.cursor = (app.cursor_line, app.cursor_col);
        }
        // The layout of this frame dims all but the lines of the pointer:
        // lay it out again, the same rows with nothing dimmed.
        layout = app.screen_layout_with_pointer(canvas_area.width as usize);
    }
    if app.present && app.mode == Mode::Normal {
        match app.pointer {
            // The hidden cursor goes into the view; a held view (a reveal)
            // keeps holding with the cursor there.
            None => {
                app.keep_cursor_in_view(&layout, app.scroll, visible);
                let at = (app.cursor_line, app.cursor_col);
                if let Some(h) = app.scroll_held.as_mut() {
                    h.cursor = at;
                }
            }
            // A held view decided the pointer at its jump (D14).
            Some(_) if app.scroll_held.is_some() => {}
            // The pointer stays on its line, and the view shows that row.
            Some(_) => {
                if let Some(&row) = layout.row_of_line.get(&app.cursor_line) {
                    if row < app.scroll {
                        app.scroll = row;
                    } else if row >= app.scroll + visible {
                        app.scroll = row + 1 - visible;
                    }
                }
            }
        }
    }
    // What the next frame keeps.
    app.view_anchor = view_anchor_of(&layout, app.scroll);
    app.view_cursor_line = app.cursor_line;
    let end = (app.scroll + visible).min(total);
    // While an enter effect plays: its frame, on the rows in view only.
    layout.paint_rows(app.scroll..end);
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
        // Discreet step counter in presentation mode, with the count of the
        // status bar: on the new empty step after the last one too, also in
        // a document of one step. It and the chip below are on the last
        // row, which has no code ([`App::status_rows`]).
        if app.present_counter_shown() {
            let total = app.doc.step_total(app.doc.step);
            let txt = format!(" {}/{} ", app.doc.step, total);
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
            // mode (no popup), bottom-left, in the accent color, with the
            // words of the bar's chip.
            let txt = app
                .plain(&format!(" ⏸ {} to review · :review ", app.proposals.len()))
                .into_owned();
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
        // and its result must be seen: the last row, only then.
        draw_command_line(frame, app, input, last_row);
    } else if app.present_message_shown() {
        draw_present_message(frame, app, last_row);
    }
    match app.overlay {
        Overlay::Help => draw_panel(frame, app, |app, w| {
            crate::keys::help_panel(&app.overlay_filter, app.overlay_typing, w)
        }),
        Overlay::List => draw_panel(frame, app, |app, _| crate::list::list(app).0),
        Overlay::Review => draw_panel(frame, app, |app, _| crate::list::review(app).0),
        Overlay::None => {}
    }
    // The next frame of an effect paints on the same layout.
    app.put_back_screen(layout);
}

/// The `:` line being typed, with completions and notices on the right:
/// the status bar's place outside presentation mode, the last row in it.
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
        // Completion candidates and notices: as much as fits, with an
        // ellipsis, in the form this terminal shows ([`App::plain`]).
        let m = app.plain(m).into_owned();
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

/// ` (3 warnings: tiza config)`, at the end of the bar's message about the
/// config file: how many warnings it has, and the command that lists them.
pub fn config_warnings_tail(n: usize) -> String {
    let s = if n == 1 { "" } else { "s" };
    format!(" ({n} warning{s}: tiza config)")
}

/// Where the tail that [`fit_message`] keeps whole starts: a step hint
/// ([`crate::app::step_hint_start`]) or a [`config_warnings_tail`].
fn kept_tail_start(m: &str) -> Option<usize> {
    crate::app::step_hint_start(m).or_else(|| {
        let i = m.rfind(" (")?;
        let count = m[i + 2..].strip_suffix(": tiza config)")?;
        let n = count
            .strip_suffix(" warnings")
            .or_else(|| count.strip_suffix(" warning"))?;
        (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then_some(i)
    })
}

/// `m` cut to `room` cells with an ellipsis, keeping the beginning — the
/// useful part of a message. A message that ends saying where to find what
/// it names — a step hint (`(step 1, press n)`), a file tail
/// (`(src/queue.rs, not shown at step 2)`) or the count of the config
/// warnings (`(3 warnings: tiza config)`) — keeps that tail whole:
/// the middle goes first, never below the message's subject
/// ([`message_subject`]: `a17: box… (step 1, press n)`, down to
/// `a17… (step 1, press n)`); then the subject (`(step 1, press n)`); and
/// with no room for the tail either, the beginning without it. A tail is
/// never cut. The status bar has more to give way in between (the
/// directory of a file, the badge): [`fit_bar_message`].
pub fn fit_message(m: &str, room: usize) -> String {
    if m.width() <= room {
        return m.to_string();
    }
    if room == 0 {
        return String::new();
    }
    // A long path gives up its middle folders first (`/Users/…/nuevo/x.tiza`):
    // the file name is what the message is about.
    let at = kept_tail_start(m).unwrap_or(m.len());
    let (head, tail) = m.split_at(at);
    let shorter = format!(
        "{}{tail}",
        shorten_paths(head, room.saturating_sub(tail.width()))
    );
    let m = shorter.as_str();
    if m.width() <= room {
        return m.to_string();
    }
    let Some(i) = kept_tail_start(m) else {
        return cut_keeping_file_name(m, room);
    };
    let (head, tail) = m.split_at(i);
    keep_subject_and_tail(head, tail, room).unwrap_or_else(|| tail_or_beginning(head, tail, room))
}

/// The byte ranges of the words of `m` that are paths with folders
/// (`/a/b/c.tiza`, `src/x/y.rs`), without the punctuation around them.
fn path_spans(m: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for word in m.split(' ') {
        let end = start + word.len();
        let lead = word.len() - word.trim_start_matches(['(', '«']).len();
        let trail = word.len() - word.trim_end_matches([')', ':', ',', ';', '»', '.']).len();
        if lead + trail < word.len() {
            let path = &word[lead..word.len() - trail];
            if path.matches('/').count() >= 2 {
                out.push((start + lead, end - trail));
            }
        }
        start = end + 1;
    }
    out
}

/// `path` with its middle folders as `…`: the first one, the last folder
/// and the file name stay (`/Users/…/nuevo/x.tiza`); `None` when that is
/// not shorter.
fn shorten_path(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    let (first, from) = match parts.first() {
        Some(&"") => (format!("/{}", parts.get(1)?), 2),
        Some(p) => (p.to_string(), 1),
        None => return None,
    };
    let n = parts.len();
    if n < from + 3 || parts[from..n - 2] == ["…"] {
        return None;
    }
    let short = format!("{first}/…/{}/{}", parts[n - 2], parts[n - 1]);
    (short.width() < path.width()).then_some(short)
}

/// `m` with its longest paths shortened ([`shorten_path`]) until it fits in
/// `room` cells or no path can give more.
fn shorten_paths(m: &str, room: usize) -> String {
    let mut m = m.to_string();
    while m.width() > room {
        let longest = path_spans(&m)
            .into_iter()
            .filter_map(|(a, b)| shorten_path(&m[a..b]).map(|s| (a, b, s)))
            .max_by_key(|(a, b, _)| b - a);
        let Some((a, b, short)) = longest else {
            break;
        };
        m.replace_range(a..b, &short);
    }
    m
}

/// `m` cut at its end with `…`. When that cut would fall in the first
/// path, the end of the path stays instead, so the file name and its
/// extension show: `wrote a copy to …long-name.tiza`.
fn cut_keeping_file_name(m: &str, room: usize) -> String {
    if let Some((a, b)) = path_spans(m).first().copied() {
        let before = m[..a].width();
        if before + m[a..b].width() > room.saturating_sub(1) && room > before + 8 {
            let keep = room - before - 1;
            let tail: String = {
                let mut w = 0;
                let mut kept: Vec<&str> = Vec::new();
                for g in m[a..b].graphemes(true).rev() {
                    if w + g.width() > keep {
                        break;
                    }
                    w += g.width();
                    kept.push(g);
                }
                kept.into_iter().rev().collect()
            };
            // A shortened path already starts its end with `…`.
            return if tail.starts_with('…') {
                format!("{}{tail}", &m[..a])
            } else {
                format!("{}…{tail}", &m[..a])
            };
        }
    }
    format!("{}…", cut_to_width(m, room - 1))
}

/// `head` + `tail` in `room` cells with the subject of `head` and the whole
/// `tail`, the middle cut: `a17: box… (step 1, press n)`; `None`
/// when not even the subject and the tail fit.
fn keep_subject_and_tail(head: &str, tail: &str, room: usize) -> Option<String> {
    let tw = tail.width();
    if head.width() + tw <= room {
        return Some(format!("{head}{tail}"));
    }
    let kept = cut_to_width(head, room.checked_sub(tw + 1)?);
    let kept = kept.trim_end();
    let subject = &head[..message_subject(head)];
    (!kept.is_empty() && kept.starts_with(subject)).then(|| format!("{kept}…{tail}"))
}

/// With no room for the subject: the tail alone, or, with no room for it
/// either, the beginning without it — never a cut tail.
fn tail_or_beginning(head: &str, tail: &str, room: usize) -> String {
    let alone = tail.trim_start();
    if alone.width() <= room {
        alone.to_string()
    } else if room == 0 {
        String::new()
    } else {
        format!("{}…", cut_to_width(head, room - 1))
    }
}

/// The beginning of `s` that fits in `max` cells, by whole graphemes.
pub(crate) fn cut_to_width(s: &str, max: usize) -> String {
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
}

/// What a cut message with a tail keeps of its beginning at the least,
/// as a byte length of `head`: the ids before the first `: `
/// (`a17` of `a17: box 3-5`, `a3, a4`, or `accepted p1` of a supervised
/// acceptance); else, for a count, the count and what it counts (`3 agent
/// annotations` of `3 agent annotations not shown`); else the first word
/// (`a3` of `a3 now appears at step 4`).
fn message_subject(head: &str) -> usize {
    if let Some(c) = head.find(": ") {
        return c;
    }
    let word_end = |from: usize| head[from..].find(' ').map_or(head.len(), |k| from + k);
    let first = word_end(0);
    let is_count = first > 0 && head[..first].bytes().all(|b| b.is_ascii_digit());
    if is_count && let Some(j) = head.find(" annotation") {
        return word_end(j + 1);
    }
    first
}

/// ` (src/queue.rs, not shown at step 2)` → `src/queue.rs` and the same
/// tail with the file's name alone, ` (queue.rs, not shown at step 2)`;
/// `None` for any other tail, or when the file has no directory to drop.
fn short_file_tail(tail: &str) -> Option<(&str, String)> {
    let inner = tail.strip_prefix(" (")?.strip_suffix(')')?;
    let (file, step) = inner.rsplit_once(", not shown at step ")?;
    let (_, name) = file.rsplit_once('/')?;
    (!name.is_empty()).then(|| (file, format!(" ({name}, not shown at step {step})")))
}

/// Whether `file`'s name alone names one file of the document: its main
/// file and every `code.link`, at any step. Two `mod.rs` keep
/// their directories.
fn short_name_is_unique(doc: &tiza_core::model::Document, file: &str) -> bool {
    let name = |f: &str| f.rsplit('/').next().unwrap_or(f).to_string();
    let wanted = name(file);
    let mut files = doc.link_files();
    files.push(doc.file_label(&None));
    files.iter().filter(|f| name(f) == wanted).count() <= 1
}

/// Makes `left[i]` give way: an optional segment empties, and the follow
/// indicator keeps `⟳ following` without the time of the last reload.
fn give_way(left: &mut [Span<'static>], i: usize, follow_time: Option<usize>, bar: Style) {
    left[i] = if Some(i) == follow_time {
        Span::styled("  ⟳ following", left[i].style)
    } else {
        Span::styled("", bar)
    };
}

/// The status bar's message `m` when it ends in a tail and does not fit,
/// as the text for the right of the bar (with its trailing blank),
/// giving way in this order: the optional left segments (`optional`, the
/// time of the last reload in `left[follow_time]` among them), the middle of the
/// message, the directory of a file tail (only when no other file of the
/// document has its name), the `↪ file` view indicator in `left[view]`
/// (it names the other file the message is often about), the
/// terminal-size badge in `left[badge]` (`! labels`, then `!`), the
/// subject; the tail is never cut. Each step is taken only when the one
/// before is not enough, so a wider bar never shows less of the subject,
/// the tail, the directory, the indicator or the badge. `None`: nothing
/// fits on the right.
#[allow(clippy::too_many_arguments)]
fn fit_bar_message(
    app: &App,
    left: &mut [Span<'static>],
    optional: &[usize],
    follow_time: Option<usize>,
    badge: Option<usize>,
    view: Option<usize>,
    m: &str,
    width: usize,
    bar: Style,
) -> Option<String> {
    let lw = |left: &[Span]| left.iter().map(|s| s.content.width()).sum::<usize>();
    // The room after a blank cell on the left and the message's own
    // trailing blank.
    let room = |left: &[Span]| width.saturating_sub(lw(left) + 2);
    for &drop_idx in optional {
        if m.width() <= room(left) {
            break;
        }
        if drop_idx < left.len() && Some(drop_idx) != view {
            give_way(left, drop_idx, follow_time, bar);
        }
    }
    if m.width() <= room(left) {
        return Some(format!("{m} "));
    }
    let i = crate::app::step_hint_start(m)?;
    let (head, tail) = m.split_at(i);
    let short = short_file_tail(tail)
        .filter(|(file, _)| short_name_is_unique(&app.doc, file))
        .map(|(_, short)| short);
    // The labels notice gives way to its sign; `code needs N cols` never
    // does.
    let forms: Vec<String> = match badge.map(|b| left[b].content.to_string()) {
        Some(f) if f.ends_with(" cut") => vec![f, "  !".into()],
        _ => vec![String::new()],
    };
    let layout_badge = badge.filter(|_| forms.len() > 1);
    // The steps: the badge whole with the indicator, then the
    // indicator gone with the badge still whole, then the badge's sign.
    let mut steps: Vec<(String, bool)> = vec![(forms[0].clone(), false)];
    if view.is_some() {
        steps.push((forms[0].clone(), true));
    }
    steps.extend(forms[1..].iter().map(|f| (f.clone(), true)));
    for (k, (form, no_view)) in steps.into_iter().enumerate() {
        if let Some(b) = layout_badge {
            left[b] = Span::styled(form, left[b].style);
        }
        if no_view && let Some(v) = view {
            left[v] = Span::styled("", bar);
        }
        // The directory goes before the indicator and the badge: once they
        // give way, the file is named alone.
        let tails: Vec<&str> = match (&short, k) {
            (Some(s), 0) => vec![tail, s],
            (Some(s), _) => vec![s],
            (None, _) => vec![tail],
        };
        for t in tails {
            if let Some(s) = keep_subject_and_tail(head, t, room(left)) {
                return Some(format!("{s} "));
            }
        }
    }
    // The subject gives way. The badge stays at its sign: one column more
    // brings the subject back with that sign, and must not take a longer
    // badge away.
    let tail = short.as_deref().unwrap_or(tail);
    let r = room(left);
    (r >= 3).then(|| format!("{} ", tail_or_beginning(head, tail, r)))
}

/// The result of the command just run, on the last row in presentation
/// mode, for [`crate::app::PRESENT_MESSAGE_TTL`]: the status bar's
/// text on the command line's row, nothing else of the bar.
fn draw_present_message(frame: &mut Frame, app: &App, area: Rect) {
    let Some((m, is_err)) = &app.present_message else {
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
        fit_message(&app.plain(m), room)
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
                "ARROW"
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
    let file = app.plain(&file).into_owned();
    let mut left = vec![
        Span::styled(app.plain(" ╭▶ ").into_owned(), accent),
        Span::styled(mode, accent),
        Span::styled(
            // `step 3/7 · Shared borrow` on a named step; a long
            // title is cut so the rest of the bar keeps its room.
            match app.doc.step_title(app.doc.step) {
                Some(t) => format!(
                    "  step {}/{} · {}",
                    app.doc.step,
                    app.doc.step_total(app.doc.step),
                    fit_message(&app.plain(&tiza_core::printable::printable_line(&t)), 32)
                ),
                None => format!(
                    "  step {}/{}",
                    app.doc.step,
                    app.doc.step_total(app.doc.step)
                ),
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
    // One indicator for every way of not writing the sidecar, and it says
    // why: `read-only` (the file), `no-save` (`--no-save` keeps the file
    // opened as it is) or `autosave off` (`:autosave off`, or a foreign
    // `scene put` after the `:w` that wrote it over the file opened).
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
        // With `--no-save`, `:w` would write over the file opened: the
        // ways out are `:saveas PATH` and `:q!`.
        let text = match (app.dirty, app.no_save) {
            (true, true) => NO_SAVE_HINTS[0],
            (true, false) => "  unsaved (:w)",
            (false, true) => "  no-save",
            (false, false) => "  autosave off",
        };
        left.push(Span::styled(text, if app.dirty { accent } else { muted }));
    }
    let no_save_hint = (app.dirty && app.no_save && app.save_error.is_none() && !app.autosave)
        .then(|| left.len() - 1);
    // With no changes, `no-save` and `autosave off` lose nothing if they
    // give way: they are the last optional segment. `unsaved …` and
    // `read-only` stay.
    let save_chip =
        (!app.dirty && app.save_error.is_none() && !app.autosave).then(|| left.len() - 1);
    if app.supervised {
        let text = if app.proposals.is_empty() {
            "  ⏸ supervised".to_string()
        } else {
            format!("  ⏸ {} to review · :review", app.proposals.len())
        };
        left.push(Span::styled(app.plain(&text).into_owned(), accent));
    }
    // A notice about stale moves names every one of them: it takes the room
    // of the optional segments, the time of the last reload included.
    let stale = app.stale_message_shown();
    let mut follow_time = None;
    if let Some(f) = &app.follow {
        // Paused at a step with a patch applied: a save waits.
        let text = match (app.follow_paused(), &f.missing, &f.last_reload_clock) {
            (true, true, _) => "  ⟳ paused (file removed)".to_string(),
            (true, false, _) if app.follow_pending() => "  ⟳ paused · changed".to_string(),
            (true, false, _) => "  ⟳ paused".to_string(),
            (false, true, _) => "  ⟳ following (file removed; waiting)".to_string(),
            (false, false, Some(at)) if !stale => {
                follow_time = Some(left.len());
                format!("  ⟳ following · last {at}")
            }
            (false, false, _) => "  ⟳ following".to_string(),
        };
        left.push(Span::styled(text, accent));
    }
    // The optional segments, in the order they give way: `· last …` of
    // follow after them, and a save chip with no changes last.
    let optional: Vec<usize> = [4usize, 6, 7, 5, 3]
        .into_iter()
        .chain(follow_time)
        .chain(save_chip)
        .collect();
    let warnings = app.viewport_warnings_for(layout);
    let badge = left.len();
    // Only what the step on screen loses has a badge: clipped code, or
    // labels cut (`2 labels cut`); the full text is in `status.warnings`.
    // The sign is ASCII `!`, one cell in every terminal: U+26A0 is drawn
    // two cells wide by some, which pushed the bar past the edge.
    let short = layout
        .code_needs
        .map(|w| format!("code needs {} cols", app.columns_for(w)))
        .or_else(|| {
            warnings
                .iter()
                .find_map(|w| w.split_once(" cut at ").map(|(n, _)| format!("{n} cut")))
        });
    if let Some(short) = short {
        left.push(Span::styled(
            format!("  ! {short}"),
            Style::default()
                .fg(rgb(th.error))
                .bg(rgb(th.status_bg))
                .add_modifier(Modifier::BOLD),
        ));
    }
    // What goes on the right, by priority: the last message, then the
    // selection hint, then the key hints. A failed save is not repeated
    // here: it was said once and the left indicator keeps it.
    let err_style = Style::default()
        .fg(rgb(th.error))
        .bg(rgb(th.status_bg))
        .add_modifier(Modifier::BOLD);
    // The message as this terminal shows it ([`App::plain`]): it is
    // measured and cut in that form.
    let plain_message = app
        .message
        .as_ref()
        .map(|(m, is_err)| (app.plain(m).into_owned(), *is_err));
    let message = plain_message.as_ref();
    let (right_text, right_style, essential): (String, Style, bool) =
        match (&app.save_error, message) {
            (_, Some((m, is_err))) => {
                (format!("{m} "), if *is_err { err_style } else { bar }, true)
            }
            (_, None) if app.selected.is_some() => (
                tab_hint(
                    &app.plain(&app.selected_text().unwrap_or_default()),
                    usize::MAX,
                ),
                accent,
                true,
            ),
            // With a selection, its keys are what the author needs next:
            // they take the room of the optional segments, as a message does.
            (_, None) if matches!(app.mode, Mode::Select { .. }) => {
                (SELECT_HINTS[0].to_string(), muted, true)
            }
            (_, None) => (
                "v select · : command · ? help · L list · q quit ".to_string(),
                muted,
                false,
            ),
        };
    // The keys of a `v`/`V` selection and of an annotation selected with
    // Tab: what the author can press next.
    let tab_hint_shown = message.is_none() && app.selected.is_some();
    let select_hint =
        message.is_none() && !tab_hint_shown && matches!(app.mode, Mode::Select { .. });
    let key_hint = select_hint || tab_hint_shown;
    // A message with a tail keeps the `↪ file` view indicator until its
    // middle and the directory have given way.
    let view = (app.shown_view().is_some()
        && message.is_some_and(|(m, _)| crate::app::step_hint_start(m).is_some()))
    .then_some(5);
    // Make room: drop the optional left segments (theme, cursor line,
    // annotation count, file, size, a save chip with no changes) before
    // cutting anything that matters, then truncate the right text with an
    // ellipsis instead of hiding it.
    let width = area.width as usize;
    let lw = |left: &Vec<Span>| left.iter().map(|s| s.content.width()).sum::<usize>();
    let need = if stale || key_hint {
        right_text.width()
    } else if essential {
        right_text.width().min(width / 2)
    } else {
        0
    };
    for &drop_idx in &optional {
        if lw(&left) + need <= width {
            break;
        }
        if drop_idx < left.len() && Some(drop_idx) != view {
            give_way(&mut left, drop_idx, follow_time, bar);
        }
    }
    // `--no-save` with changes: the short form, with both ways out still
    // named, when the long one leaves the right side less than `? help`.
    if let Some(i) = no_save_hint
        && lw(&left) + 1 + need.max(6) > width
    {
        left[i] = Span::styled(NO_SAVE_HINTS[1], accent);
    }
    // An error is about the key just pressed: when it does not fit, it
    // takes the room of the optional segments and of the step, and a
    // warning badge keeps only its sign. They come back with the next key.
    // The mode and the state of the document (unsaved changes, follow,
    // supervised) stay. A message with a tail gives way in its own order below.
    if let Some((m, true)) = message
        && crate::app::step_hint_start(m).is_none()
    {
        let fits = |left: &Vec<Span>| lw(left) + 1 + right_text.width() <= width;
        for &i in &optional {
            if fits(&left) {
                break;
            }
            give_way(&mut left, i, follow_time, bar);
        }
        if !fits(&left) && badge < left.len() {
            left[badge] = Span::styled("  !", err_style);
        }
        if !fits(&left) {
            left[2] = Span::styled("", bar);
        }
    }
    // The keys of a selection are worth more than the text of a warning the
    // author already saw in normal mode: it keeps only its sign.
    if key_hint && badge < left.len() && lw(&left) + 1 + right_text.width() > width {
        left[badge] = Span::styled("  !", err_style);
    }
    // A badge is never cut in the middle of a word: when the left side
    // does not fit whole, it keeps only its sign.
    if badge < left.len() && lw(&left) > width {
        left[badge] = Span::styled("  !", err_style);
    }
    // Hints are optional, but `? help` should find room when giving up only
    // the theme and the size (cursor line and count are worth more).
    if !essential {
        for drop_idx in [3usize, 4] {
            if lw(&left) + "? help ".len() <= width {
                break;
            }
            left[drop_idx] = Span::styled("", bar);
        }
    }
    // A message with a tail that does not fit gives way in its own order;
    // every other message, as before.
    let tailed = match message {
        Some((m, _)) if essential && lw(&left) + right_text.width() + 1 > width => {
            crate::app::step_hint_start(m).map(|_| m.as_str())
        }
        _ => None,
    };
    let badge = (badge < left.len()).then_some(badge);
    let tailed_text = tailed.map(|m| {
        fit_bar_message(
            app,
            &mut left,
            &optional,
            follow_time,
            badge,
            view,
            m,
            width,
            bar,
        )
    });
    // One blank cell between the left segments and the message.
    let room = width.saturating_sub(lw(&left) + 1);
    let mut right: Vec<Span> = Vec::new();
    if let Some(text) = tailed_text {
        if let Some(t) = text {
            right.push(Span::styled(t, right_style));
        }
    } else if essential {
        if right_text.width() <= room {
            right.push(Span::styled(right_text, right_style));
        } else if message.is_none() && app.selected.is_some() {
            // The Tab hint keeps its keys: the label gives way first.
            let hint = tab_hint(&app.plain(&app.selected_text().unwrap_or_default()), room);
            right.push(Span::styled(hint, right_style));
        } else if select_hint {
            // Shorter forms, the keys kept to the last; a form gives up its
            // right margin before a shorter form is taken.
            let form = SELECT_HINTS
                .iter()
                .find_map(|t| [*t, t.trim_end()].into_iter().find(|f| f.width() <= room));
            if let Some(t) = form {
                right.push(Span::styled(t, right_style));
            }
        } else if room >= 4 {
            // Keep the beginning: that is where the useful part of a message is.
            let cut = fit_message(right_text.trim_end_matches(' '), room - 1);
            right.push(Span::styled(format!("{cut} "), right_style));
        }
    } else if right_text.width() <= room {
        right.push(Span::styled(right_text, right_style));
    } else {
        // Shorter hint tiers before giving up on all but `? help`, which
        // gives up its right margin before it goes.
        let tiers: &[&str] = &[
            ": command · ? help · q quit ",
            "? help · q quit ",
            "? help ",
            "? help",
        ];
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

/// The save state with `--no-save` and changes: `:w` would write over the
/// file opened, so it names `:saveas` and `:q!`; the short form for a
/// narrow bar.
const NO_SAVE_HINTS: [&str; 2] = [
    "  unsaved (:saveas PATH · :q! discards)",
    "  unsaved (:saveas · :q!)",
];

/// The hint of a `v`/`V` selection, from the full form to the keys alone.
const SELECT_HINTS: [&str; 4] = [
    "h highlight · m mark · n note · C callout · b box · a arrow · s spotlight ",
    "h highlight · m mark · n note · b box · a arrow ",
    "h highlight · m mark · n note ",
    "h m n C b a s ",
];

/// The hint of an annotation selected with Tab, in `room` cells: `▸`, what
/// it is, its label, then its keys (`enter label · e set …`). The keys come
/// first: when there is no room, the label gives way, then what it is (the
/// id stays), then the keys from the right, so what the presenter can press
/// stays (crates/tiza-tui/UI-GUIDELINES.md, section 5).
fn tab_hint(described: &str, room: usize) -> String {
    let (head, label) = match described.find(" «") {
        Some(i) => (&described[..i], &described[i..]),
        None => (described, ""),
    };
    // `a4 [step 2] note L13`: the id alone is `a4`.
    let id = head.split(' ').next().unwrap_or(head);
    let keys = ["enter label", "e set", "x delete", "c color", "+/- step"];
    let build = |what: &str, n: usize| {
        let mut s = format!("▸ {what}");
        for k in &keys[..n] {
            s.push_str(" · ");
            s.push_str(k);
        }
        s.push(' ');
        s
    };
    // The label goes first, then what the annotation is, then the keys
    // from the right; a form gives up its right margin before a shorter
    // one is taken.
    let forms = [
        build(&format!("{head}{label}"), keys.len()),
        build(head, keys.len()),
    ]
    .into_iter()
    .chain((1..=keys.len()).rev().map(|n| build(id, n)));
    for form in forms {
        for shown in [form.as_str(), form.trim_end()] {
            if shown.width() <= room {
                return shown.to_string();
            }
        }
    }
    format!(
        "{} ",
        cut_to_width(&format!("▸ {id}"), room.saturating_sub(1))
    )
}

/// Draws the panel `make` builds at a content width, and keeps how it was
/// drawn: the page and the largest scroll for the keys, and the scroll
/// itself (moved to show the selected row, or back from past the end).
fn draw_panel(
    frame: &mut Frame,
    app: &mut App,
    make: impl FnOnce(&App, usize) -> crate::panel::Panel,
) {
    let Some(rect) = crate::panel::rect(frame.area()) else {
        return;
    };
    let mut p = make(app, (rect.width as usize).saturating_sub(4));
    if !app.wide.is_empty() {
        p = p.map_text(|s| app.plain(s).into_owned());
    }
    let drawn = crate::panel::draw(frame, rect, &app.theme, &p, app.overlay_scroll);
    app.overlay_drawn.set(drawn);
    app.overlay_scroll = drawn.scroll;
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGENT_BOX: &str = "a1: box 3-5 added by agent (step 1, press n)";
    const FILE_TAIL: &str = "a9: note L17 added by agent (src/queue.rs, not shown at step 2)";
    const GROUPED: &str = "3 agent annotations not shown (step 1, press n)";

    /// Every message shape that reaches the bar with a tail,
    /// with wide characters near the cut, and two without one.
    const SHAPES: &[&str] = &[
        AGENT_BOX,
        FILE_TAIL,
        GROUPED,
        "3 agent annotations not shown (src/queue.rs, not shown at step 2)",
        "a1: box 12-18 updated by agent (src/stack (v2).rs, not shown at step 2)",
        "a17: box 3-5 (step 3, :step 3 shows it)",
        "a4: note L13 updated by agent (step 3, :step 3 shows it)",
        "a3 now appears at step 4 (:step 4 shows it)",
        "a2: spotlight at step 1 (press n)",
        "accepted p1: add box L3-5 (step 1, press n)",
        "a3, a4: box 3-5 (step 2, press n)",
        "注釈1: box 3-5 added by agent (step 1, press n)",
        "a1: 日本語のラベルがここで切れる (step 1, press n)",
        "a2: 🦀🦀🦀 crab note at L4 (step 2, press n)",
        "a1: box 3-5 (hello)",
        "a long message with no tail at all that goes on and on",
        "config.toml:3: colour: unknown key (keys: pad, numbers, mouse) (2 warnings: tiza config)",
        "file.rs:2-99 clamped to 2-4 · cfg.toml:2: co␊lour: unknown key (1 warning: tiza config)",
    ];

    /// Whether `r` ends in a cut form of `tail`: a piece of its beginning
    /// and `…` (`(st…`, as `fit_message` once drew it).
    fn ends_in_cut_tail(r: &str, tail: &str) -> bool {
        let Some(before) = r.strip_suffix('…') else {
            return false;
        };
        let tail = tail.trim_start();
        tail.char_indices()
            .skip(1)
            .any(|(k, _)| before.ends_with(&tail[..k]))
    }

    #[test]
    fn fit_message_keeps_the_tail_whole_at_every_width() {
        for m in SHAPES {
            let tail = kept_tail_start(m).map(|i| &m[i..]);
            let mut last = String::new();
            for room in 0..=130 {
                let r = fit_message(m, room);
                assert!(r.width() <= room, "{m:?} at {room}: {r:?}");
                if m.width() <= room {
                    // What fits is untouched, byte for byte.
                    assert_eq!(r, *m, "at {room}");
                    continue;
                }
                let Some(tail) = tail else {
                    if room > 0 {
                        let kept = r.strip_suffix('…').expect("an ellipsis");
                        assert!(m.starts_with(kept), "{m:?} at {room}: {r:?}");
                    }
                    continue;
                };
                let head = &m[..m.len() - tail.len()];
                let alone = tail.trim_start();
                assert!(!ends_in_cut_tail(&r, tail), "{m:?} at {room}: {r:?}");
                if r.ends_with(alone) {
                    let before = &r[..r.len() - alone.len()];
                    let subject = &head[..message_subject(head)];
                    assert!(
                        before.is_empty() || before.starts_with(subject),
                        "{m:?} at {room}: {r:?} lost {subject:?}"
                    );
                    // A wider room never drops the subject it kept.
                    let had_subject = last.ends_with(alone) && last.len() > alone.len();
                    assert!(!had_subject || !before.is_empty(), "{m:?} at {room}: {r:?}");
                } else {
                    // No room for the tail: the beginning alone.
                    assert!(alone.width() > room, "{m:?} at {room}: {r:?}");
                    if room > 0 {
                        let kept = r.strip_suffix('…').expect("an ellipsis");
                        assert!(head.starts_with(kept), "{m:?} at {room}: {r:?}");
                    }
                }
                last = r;
            }
        }
    }

    #[test]
    fn the_cut_tail_detector_catches_mains_cut() {
        // What `fit_message` once drew at 16 cells.
        assert!(ends_in_cut_tail("a1: box 3-5 (st…", " (step 1, press n)"));
        assert!(ends_in_cut_tail(
            "a1: box (step 1, pr…",
            " (step 1, press n)"
        ));
        assert!(!ends_in_cut_tail("a1: box 3-5 add…", " (step 1, press n)"));
    }

    #[test]
    fn fit_message_cuts_the_middle_then_keeps_the_tail() {
        assert_eq!(fit_message(AGENT_BOX, 120), AGENT_BOX);
        assert_eq!(fit_message(AGENT_BOX, 44), AGENT_BOX);
        assert_eq!(
            fit_message(AGENT_BOX, 43),
            "a1: box 3-5 added by age… (step 1, press n)"
        );
        assert_eq!(fit_message(AGENT_BOX, 27), "a1: box… (step 1, press n)");
        assert_eq!(fit_message(AGENT_BOX, 22), "a1:… (step 1, press n)");
        // The subject and the ellipsis with no blank between, as before.
        assert_eq!(fit_message(AGENT_BOX, 21), "a1… (step 1, press n)");
        // Then the tail wins over the subject …
        assert_eq!(fit_message(AGENT_BOX, 20), "(step 1, press n)");
        assert_eq!(fit_message(AGENT_BOX, 17), "(step 1, press n)");
        // … and a tail that does not fit is not shown cut.
        assert_eq!(fit_message(AGENT_BOX, 16), "a1: box 3-5 add…");
        assert_eq!(fit_message(AGENT_BOX, 1), "…");
        assert_eq!(fit_message(AGENT_BOX, 0), "");
        // The step-hint cases keep their output.
        let m = "a17: box 3-5 a long label (step 1, press n)";
        assert_eq!(fit_message(m, 27), "a17: box… (step 1, press n)");
        assert_eq!(fit_message(m, 10), "a17: box …");
        assert_eq!(fit_message("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn fit_message_keeps_the_directory() {
        // The directory is the status bar's to give way (it knows the
        // document's files): the presentation line keeps it.
        assert_eq!(fit_message(FILE_TAIL, 63), FILE_TAIL);
        assert_eq!(
            fit_message(FILE_TAIL, 44),
            "a9: not… (src/queue.rs, not shown at step 2)"
        );
        assert_eq!(
            fit_message(FILE_TAIL, 35),
            "(src/queue.rs, not shown at step 2)"
        );
        assert_eq!(
            short_file_tail(" (src/queue.rs, not shown at step 2)"),
            Some((
                "src/queue.rs",
                " (queue.rs, not shown at step 2)".to_string()
            ))
        );
        assert_eq!(short_file_tail(" (queue.rs, not shown at step 2)"), None);
        assert_eq!(short_file_tail(" (step 1, press n)"), None);
        // A ` (` in the file's name is part of the file.
        let m = "a1: box 12-18 updated by agent (src/stack (v2).rs, not shown at step 2)";
        assert_eq!(
            fit_message(m, 50),
            "a1: box… (src/stack (v2).rs, not shown at step 2)"
        );
        assert_eq!(
            short_file_tail(" (src/stack (v2).rs, not shown at step 2)").map(|t| t.1),
            Some(" (stack (v2).rs, not shown at step 2)".to_string())
        );
    }

    #[test]
    fn fit_message_grouped_notice_keeps_what_it_counts() {
        assert_eq!(fit_message(GROUPED, 47), GROUPED);
        assert_eq!(
            fit_message(GROUPED, 41),
            "3 agent annotations no… (step 1, press n)"
        );
        assert_eq!(
            fit_message(GROUPED, 38),
            "3 agent annotations… (step 1, press n)"
        );
        assert_eq!(fit_message(GROUPED, 37), "(step 1, press n)");
    }

    #[test]
    fn fit_message_wide_characters_near_the_cut() {
        let m = "注釈1: box 3-5 added by agent (step 1, press n)";
        assert_eq!(fit_message(m, 25), "注釈1:… (step 1, press n)");
        assert_eq!(fit_message(m, 24), "注釈1… (step 1, press n)");
        assert_eq!(fit_message(m, 23), "(step 1, press n)");
        assert_eq!(fit_message(m, 4), "注…");
        // A wide character that would straddle the cut is left out whole.
        let m = "a1: 日本語のラベルがここで切れる (step 1, press n)";
        assert_eq!(fit_message(m, 28), "a1: 日本… (step 1, press n)");
        assert_eq!(fit_message(m, 29), "a1: 日本語… (step 1, press n)");
        assert_eq!(fit_message(m, 30), "a1: 日本語… (step 1, press n)");
        let m = "a2: 🦀🦀🦀 crab note at L4 (step 2, press n)";
        assert_eq!(fit_message(m, 26), "a2: 🦀… (step 2, press n)");
    }

    #[test]
    fn message_subject_for_each_shape() {
        let s = |h: &str| h[..message_subject(h)].to_string();
        assert_eq!(s("a17: box 3-5"), "a17");
        assert_eq!(s("a3, a4: box 3-5"), "a3, a4");
        assert_eq!(s("accepted p1: add box L3-5"), "accepted p1");
        assert_eq!(s("3 agent annotations not shown"), "3 agent annotations");
        assert_eq!(s("3 things"), "3");
        assert_eq!(s("a3 now appears at step 4"), "a3");
        assert_eq!(s("a1: hl 3:takes"), "a1");
    }
}
