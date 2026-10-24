//! The panel: a box that opens over the code, with the title and a close
//! chip on its top border, a filter line, rows, and the panel's keys and a
//! position on its bottom border (docs/ui-guidelines.md, section 1). The
//! help uses it. Colors come from the theme's roles only.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use tiza_core::theme::Theme;

use crate::ui::cut_to_width as cut;

/// The smallest terminal a panel opens in.
pub const MIN_SIZE: (u16, u16) = (40, 12);

/// Below this content width, a row stacks: the key above its description.
pub const STACK_BELOW: usize = 64;

/// The width of the key column, and of the command column at most.
const KEY_COL: usize = 12;
const COMMAND_COL: (usize, usize) = (24, 38);

/// One row of a panel, before it is laid out at a width.
#[derive(Clone, Debug, PartialEq)]
pub enum PanelRow {
    /// Section header: accent, bold.
    Section(String),
    /// Sub-header: two cells of indent, muted, bold.
    Sub(String),
    /// A line of muted text (a legend).
    Note(String),
    Blank,
    /// A key or a command and its description. `key` is in parts: (text,
    /// is a placeholder). `wide`: the command column.
    Pair {
        key: Vec<(String, bool)>,
        desc: String,
        wide: bool,
    },
    /// A line of text (an empty state).
    Text(String),
}

/// What a panel shows.
pub struct Panel {
    pub title: String,
    /// The close chip: (key, word), as `esc close`.
    pub chip: (&'static str, &'static str),
    /// The filter text, and whether the user types it now.
    pub filter: String,
    pub typing: bool,
    /// The filter line when it is empty and not typed (after `/ `).
    pub placeholder: &'static str,
    pub rows: Vec<PanelRow>,
    /// The panel's keys: (key, word); a key of `""` is a word alone.
    pub footer: Vec<(&'static str, &'static str)>,
    /// The right of the footer (`14 matches`); `None`: the rows shown,
    /// `1-30 of 124`.
    pub position: Option<String>,
}

/// Where a panel goes in `area`, or `None` when the terminal is smaller
/// than [`MIN_SIZE`].
pub fn rect(area: Rect) -> Option<Rect> {
    if area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1 {
        return None;
    }
    let w = area.width.saturating_sub(4).clamp(MIN_SIZE.0, 100);
    let h = area.height.saturating_sub(6).max(6);
    Some(Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    })
}

/// The cells of `s`, as the terminal counts them.
fn cells(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// `s` cut to `max` cells with `…` when it does not fit.
fn fit(s: &str, max: usize) -> String {
    if cells(s) <= max {
        s.to_string()
    } else if max == 0 {
        String::new()
    } else {
        format!("{}…", cut(s, max - 1))
    }
}

/// `text` in lines of at most `width` cells, broken between words (a word
/// wider than the line is broken by graphemes).
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        let sep = usize::from(!line.is_empty());
        if cells(&line) + sep + cells(word) <= width {
            if sep == 1 {
                line.push(' ');
            }
            line.push_str(word);
            continue;
        }
        if !line.is_empty() {
            out.push(std::mem::take(&mut line));
        }
        let mut rest = word.to_string();
        while cells(&rest) > width {
            let mut head = cut(&rest, width);
            if head.is_empty() {
                // One grapheme wider than the line: it takes a line alone.
                head = rest.graphemes(true).next().unwrap_or_default().to_string();
            }
            rest = rest[head.len()..].to_string();
            out.push(head);
        }
        line = rest;
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}

struct Styles {
    base: Style,
    accent: Style,
    accent_bold: Style,
    muted: Style,
    muted_bold: Style,
    key: Style,
    note: Style,
}

impl Styles {
    fn of(th: &Theme) -> Styles {
        let rgb = crate::ui::rgb;
        let base = Style::default().fg(rgb(th.fg)).bg(rgb(th.cmd_bg));
        Styles {
            base,
            accent: base.fg(rgb(th.accent)),
            accent_bold: base.fg(rgb(th.accent)).add_modifier(Modifier::BOLD),
            muted: base.fg(rgb(th.muted)),
            muted_bold: base.fg(rgb(th.muted)).add_modifier(Modifier::BOLD),
            key: base.add_modifier(Modifier::BOLD),
            note: base.fg(rgb(th.note)),
        }
    }
}

/// The width of the column a pair's key takes at `content_w`.
fn key_col(wide: bool, content_w: usize) -> usize {
    if wide {
        (content_w * 5 / 12).clamp(COMMAND_COL.0, COMMAND_COL.1)
    } else {
        KEY_COL
    }
}

/// The key spans of a pair, cut to `max` cells (a key never wraps).
fn key_spans(key: &[(String, bool)], max: usize, st: &Styles) -> (Vec<Span<'static>>, usize) {
    let mut out = Vec::new();
    let mut used = 0;
    let total: usize = key.iter().map(|(t, _)| cells(t)).sum();
    let room = if total > max {
        max.saturating_sub(1)
    } else {
        max
    };
    for (text, placeholder) in key {
        let left = room.saturating_sub(used);
        let part = cut(text, left);
        used += cells(&part);
        out.push(Span::styled(
            part,
            if *placeholder { st.note } else { st.key },
        ));
        if used >= room {
            break;
        }
    }
    if total > max && max > 0 {
        out.push(Span::styled("…", st.key));
        used += 1;
    }
    (out, used)
}

/// The rows laid out at `content_w` cells: one `Line` per screen row.
pub fn lines(rows: &[PanelRow], content_w: usize, th: &Theme) -> Vec<Line<'static>> {
    let st = Styles::of(th);
    let stacked = content_w < STACK_BELOW;
    let mut out: Vec<Line<'static>> = Vec::new();
    for row in rows {
        match row {
            PanelRow::Section(t) => {
                out.push(Line::from(Span::styled(fit(t, content_w), st.accent_bold)))
            }
            PanelRow::Sub(t) => out.push(Line::from(vec![
                Span::styled("  ", st.base),
                Span::styled(fit(t, content_w.saturating_sub(2)), st.muted_bold),
            ])),
            PanelRow::Note(t) => {
                for l in wrap(t, content_w) {
                    out.push(Line::from(Span::styled(l, st.muted)));
                }
            }
            PanelRow::Text(t) => {
                for l in wrap(t, content_w) {
                    out.push(Line::from(Span::styled(l, st.base)));
                }
            }
            PanelRow::Blank => out.push(Line::from("")),
            PanelRow::Pair { key, desc, wide } => {
                if stacked {
                    let (spans, _) = key_spans(key, content_w, &st);
                    out.push(Line::from(spans));
                    for l in wrap(desc, content_w.saturating_sub(4)) {
                        out.push(Line::from(vec![
                            Span::styled("    ", st.base),
                            Span::styled(l, st.base),
                        ]));
                    }
                    continue;
                }
                let col = key_col(*wide, content_w);
                let indent = col + 2;
                let desc_lines = wrap(desc, content_w.saturating_sub(indent));
                let key_w: usize = key.iter().map(|(t, _)| cells(t)).sum();
                let (spans, used) = key_spans(key, content_w, &st);
                let mut first = spans;
                let mut rest = desc_lines.iter();
                if key_w <= col {
                    first.push(Span::styled(" ".repeat(indent - used), st.base));
                    if let Some(d) = rest.next() {
                        first.push(Span::styled(d.clone(), st.base));
                    }
                }
                out.push(Line::from(first));
                for d in rest {
                    out.push(Line::from(vec![
                        Span::styled(" ".repeat(indent), st.base),
                        Span::styled(d.clone(), st.base),
                    ]));
                }
            }
        }
    }
    out
}

/// How the panel was drawn: what the keys that scroll it need.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drawn {
    /// The last scroll offset that still fills the panel.
    pub max_scroll: usize,
    /// The rows of content the panel shows at once.
    pub page: usize,
}

/// The footer's keys that fit in `room` cells, from the left.
fn footer_spans(items: &[(&str, &str)], room: usize, st: &Styles) -> Vec<Span<'static>> {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for (k, w) in items {
        let item_w = cells(k) + usize::from(!k.is_empty()) + cells(w);
        let sep = if out.is_empty() { 0 } else { 3 };
        if used + sep + item_w > room {
            break;
        }
        if sep > 0 {
            out.push(Span::styled(" · ", st.muted));
        }
        if !k.is_empty() {
            out.push(Span::styled(k.to_string(), st.key));
            out.push(Span::styled(" ", st.muted));
        }
        out.push(Span::styled(w.to_string(), st.muted));
        used += sep + item_w;
    }
    out
}

fn width_of(spans: &[Span]) -> usize {
    spans.iter().map(|s| cells(&s.content)).sum()
}

/// Draws `p` in `rect`, scrolled down by `scroll` rows (kept within the
/// rows), and says how it was drawn.
pub fn draw(frame: &mut Frame, rect: Rect, th: &Theme, p: &Panel, scroll: usize) -> Drawn {
    let st = Styles::of(th);
    let w = rect.width as usize;
    let h = rect.height as usize;
    let content_w = w.saturating_sub(4);
    let body = lines(&p.rows, content_w, th);
    // Two rows of frame, the filter line and the blank row after it.
    let page = h.saturating_sub(4).max(1);
    let max_scroll = body.len().saturating_sub(page);
    let scroll = scroll.min(max_scroll);
    frame.render_widget(Clear, rect);
    frame.buffer_mut().set_style(rect, st.base);

    let mut rows: Vec<Line> = Vec::with_capacity(h);
    // ---- top border: title and chip ----
    let chip = vec![
        Span::styled(p.chip.0, st.key),
        Span::styled(format!(" {}", p.chip.1), st.muted),
    ];
    let chip_w = width_of(&chip);
    let title_room = w.saturating_sub(4 + 1 + chip_w + 3);
    let title = fit(&p.title, title_room);
    let fill = w.saturating_sub(3 + cells(&title) + 1 + 1 + chip_w + 3);
    let mut top = vec![
        Span::styled("╭─ ", st.accent),
        Span::styled(title, st.accent_bold),
        Span::styled(format!(" {} ", "─".repeat(fill)), st.accent),
    ];
    top.extend(chip);
    top.push(Span::styled(" ─╮", st.accent));
    rows.push(Line::from(top));
    // ---- the filter line and a blank row ----
    let side = |inner: Vec<Span<'static>>| {
        let used = width_of(&inner);
        let mut l = vec![Span::styled("│ ", st.accent)];
        l.extend(inner);
        l.push(Span::styled(
            " ".repeat(content_w.saturating_sub(used)),
            st.base,
        ));
        l.push(Span::styled(" │", st.accent));
        Line::from(l)
    };
    let mut filter = vec![Span::styled("/ ", st.accent)];
    if p.typing || !p.filter.is_empty() {
        let room = content_w.saturating_sub(3);
        let shown = if cells(&p.filter) > room {
            // The end of a long filter: what is being typed.
            let skip = cells(&p.filter) - room;
            let mut acc = 0;
            // At least `skip` cells go, also when a wide grapheme is cut.
            p.filter
                .graphemes(true)
                .skip_while(|g| {
                    let drop = acc < skip;
                    acc += cells(g);
                    drop
                })
                .collect::<String>()
        } else {
            p.filter.clone()
        };
        filter.push(Span::styled(shown, st.key));
        if p.typing {
            filter.push(Span::styled("▏", st.accent));
        }
    } else {
        filter.push(Span::styled(
            fit(p.placeholder, content_w.saturating_sub(2)),
            st.muted,
        ));
    }
    rows.push(side(filter));
    rows.push(side(Vec::new()));
    // ---- the rows ----
    for i in 0..page {
        let inner = body
            .get(scroll + i)
            .map(|l| l.spans.clone())
            .unwrap_or_default();
        rows.push(side(inner));
    }
    // ---- bottom border: keys and position ----
    let position = p.position.clone().unwrap_or_else(|| {
        if body.is_empty() {
            "0 rows".to_string()
        } else {
            format!(
                "{}-{} of {}",
                scroll + 1,
                (scroll + page).min(body.len()),
                body.len()
            )
        }
    });
    let pos = fit(&position, w.saturating_sub(8));
    let room = w.saturating_sub(3 + 1 + 1 + cells(&pos) + 3);
    let keys = footer_spans(&p.footer, room, &st);
    let fill = w.saturating_sub(3 + width_of(&keys) + 1 + 1 + cells(&pos) + 3);
    let mut bottom = vec![Span::styled("╰─ ", st.accent)];
    bottom.extend(keys);
    bottom.push(Span::styled(format!(" {} ", "─".repeat(fill)), st.accent));
    bottom.push(Span::styled(pos, st.muted));
    bottom.push(Span::styled(" ─╯", st.accent));
    rows.push(Line::from(bottom));
    frame.render_widget(Paragraph::new(rows).style(st.base), rect);
    Drawn { max_scroll, page }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_keeps_words_and_breaks_long_ones() {
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("", 4), [""]);
        // Wide characters count two cells and are never cut in half.
        assert_eq!(wrap("名前名前", 5), ["名前", "名前"]);
    }

    #[test]
    fn a_long_key_is_cut_with_an_ellipsis_never_split() {
        let th = Theme::dark();
        let st = Styles::of(&th);
        let key = vec![(":style ".to_string(), false), ("NAME".to_string(), true)];
        let (spans, used) = key_spans(&key, 8, &st);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(text, ":style …");
        assert_eq!(used, 8);
    }
}
