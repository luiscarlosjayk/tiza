//! Backend-independent cell canvas. The TUI copies it into a ratatui `Buffer`;
//! the CLI dumps it as plain text or ANSI.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn hex(v: u32) -> Self {
        Self(
            ((v >> 16) & 0xff) as u8,
            ((v >> 8) & 0xff) as u8,
            (v & 0xff) as u8,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CellStyle {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
}

impl CellStyle {
    pub const fn new() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
            dim: false,
            italic: false,
        }
    }
    pub const fn fg(mut self, c: Rgb) -> Self {
        self.fg = Some(c);
        self
    }
    pub const fn bg(mut self, c: Rgb) -> Self {
        self.bg = Some(c);
        self
    }
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    /// Merges: whatever `over` defines wins; the rest is kept.
    pub fn merge(self, over: CellStyle) -> CellStyle {
        CellStyle {
            fg: over.fg.or(self.fg),
            bg: over.bg.or(self.bg),
            bold: self.bold || over.bold,
            dim: self.dim || over.dim,
            italic: self.italic || over.italic,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Grapheme occupying the cell; empty if the cell is the second half of a wide character.
    pub sym: String,
    pub style: CellStyle,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            sym: " ".to_string(),
            style: CellStyle::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); width * height],
        }
    }

    pub fn new_filled(width: usize, height: usize, style: CellStyle) -> Self {
        let mut c = Self::new(width, height);
        for cell in &mut c.cells {
            cell.style = style;
        }
        c
    }

    fn idx(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        self.idx(x, y).map(|i| &self.cells[i])
    }

    /// Writes a grapheme (width 1 or 2) at (x, y). Returns the width used.
    pub fn put(&mut self, x: usize, y: usize, sym: &str, style: CellStyle) -> usize {
        let w = UnicodeWidthStr::width(sym).max(1);
        let Some(i) = self.idx(x, y) else { return w };
        if w == 2 && x + 1 >= self.width {
            // Does not fit whole: leave a space.
            self.cells[i] = Cell {
                sym: " ".into(),
                style,
            };
            return w;
        }
        self.cells[i] = Cell {
            sym: sym.to_string(),
            style,
        };
        if w == 2 {
            self.cells[i + 1] = Cell {
                sym: String::new(),
                style,
            };
        }
        w
    }

    /// Draws a line stroke (`─`, `│`, corners) respecting what is already there:
    /// if it crosses a perpendicular stroke, it places a junction (`┼`, `╂`, `╋`,
    /// `╪`, `╫`, `╬`… chosen by the two strokes) instead of covering it.
    pub fn put_line(&mut self, x: usize, y: usize, sym: &str, style: CellStyle) {
        let existing = self.get(x, y).map(|c| c.sym.as_str()).unwrap_or(" ");
        let merged = junction(sym, existing).unwrap_or(sym);
        self.put(x, y, merged, style);
    }

    /// Writes a string starting at (x, y), grapheme by grapheme. Returns the total width.
    pub fn put_str(&mut self, x: usize, y: usize, s: &str, style: CellStyle) -> usize {
        let mut cx = x;
        for g in s.graphemes(true) {
            if cx >= self.width {
                break;
            }
            cx += self.put(cx, y, g, style);
        }
        cx - x
    }

    /// Writes `s` clipped to `max_w` cells, with `…` if it does not fit.
    pub fn put_str_clipped(
        &mut self,
        x: usize,
        y: usize,
        s: &str,
        max_w: usize,
        style: CellStyle,
    ) -> usize {
        if max_w == 0 {
            return 0;
        }
        let full = UnicodeWidthStr::width(s);
        if full <= max_w {
            return self.put_str(x, y, s, style);
        }
        let mut cx = x;
        for g in s.graphemes(true) {
            let w = UnicodeWidthStr::width(g).max(1);
            if cx - x + w > max_w.saturating_sub(1) {
                break;
            }
            cx += self.put(cx, y, g, style);
        }
        cx += self.put(cx, y, "…", style);
        cx - x
    }

    /// Applies `style` (merge) to cells x..x+w of row y.
    pub fn restyle(&mut self, x: usize, y: usize, w: usize, style: CellStyle) {
        for cx in x..x + w {
            if let Some(i) = self.idx(cx, y) {
                self.cells[i].style = self.cells[i].style.merge(style);
            }
        }
    }

    pub fn row_text(&self, y: usize) -> String {
        let mut s = String::new();
        for x in 0..self.width {
            if let Some(c) = self.get(x, y) {
                s.push_str(&c.sym);
            }
        }
        s
    }

    /// Plain text, no styles, no trailing spaces, no trailing blank rows
    /// (the padding at the bottom would only add noise in a README).
    pub fn to_plain_text(&self) -> String {
        let mut rows: Vec<String> = (0..self.height)
            .map(|y| self.row_text(y).trim_end().to_string())
            .collect();
        while rows.last().is_some_and(|r| r.is_empty()) {
            rows.pop();
        }
        rows.join("\n")
    }

    /// Plain text inside a fenced code block, ready to paste into a README,
    /// an issue or a chat: `title` becomes a bold line above the block.
    pub fn to_markdown(&self, title: Option<&str>) -> String {
        let mut out = String::new();
        if let Some(t) = title.map(str::trim).filter(|t| !t.is_empty()) {
            out.push_str(&format!("**{t}**\n\n"));
        }
        out.push_str("```text\n");
        out.push_str(&self.to_plain_text());
        out.push_str("\n```\n");
        out
    }

    /// Text with ANSI sequences (24-bit).
    pub fn to_ansi(&self) -> String {
        let mut out = String::new();
        for y in 0..self.height {
            let mut last: Option<CellStyle> = None;
            for x in 0..self.width {
                let Some(c) = self.get(x, y) else { continue };
                if c.sym.is_empty() {
                    continue;
                }
                if last != Some(c.style) {
                    out.push_str("\x1b[0m");
                    if let Some(Rgb(r, g, b)) = c.style.fg {
                        out.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
                    }
                    if let Some(Rgb(r, g, b)) = c.style.bg {
                        out.push_str(&format!("\x1b[48;2;{r};{g};{b}m"));
                    }
                    if c.style.bold {
                        out.push_str("\x1b[1m");
                    }
                    if c.style.dim {
                        out.push_str("\x1b[2m");
                    }
                    if c.style.italic {
                        out.push_str("\x1b[3m");
                    }
                    last = Some(c.style);
                }
                out.push_str(&c.sym);
            }
            out.push_str("\x1b[0m\n");
        }
        out
    }

    pub fn cells(&self) -> impl Iterator<Item = (usize, usize, &Cell)> {
        self.cells
            .iter()
            .enumerate()
            .map(move |(i, c)| (i % self.width, i / self.width, c))
    }
}

/// Width in cells of a string (tabs expanded to `tab_width`).
pub fn display_width(s: &str, tab_width: usize) -> usize {
    let mut w = 0;
    for g in s.graphemes(true) {
        if g == "\t" {
            w += tab_width - (w % tab_width);
        } else {
            w += UnicodeWidthStr::width(g);
        }
    }
    w
}

/// Weight of a line glyph: light, thick or double; `None` for anything else.
fn weight(g: &str) -> Option<(bool, u8)> {
    // (horizontal?, weight) with weight 0 light, 1 thick, 2 double
    Some(match g {
        "─" | "╌" | "┄" | "┈" | "╍" | "┅" => (true, 0),
        "━" => (true, 1),
        "═" => (true, 2),
        "│" | "╎" | "┆" | "┊" | "╏" | "┇" => (false, 0),
        "┃" => (false, 1),
        "║" => (false, 2),
        _ => return None,
    })
}

/// The junction glyph when `new` is drawn over `existing`, if they cross.
fn junction(new: &str, existing: &str) -> Option<&'static str> {
    // An existing junction keeps its shape when a line passes through again.
    if matches!(existing, "┼" | "╂" | "┿" | "╋" | "╪" | "╫" | "╬") && weight(new).is_some()
    {
        return Some(match existing {
            "┼" => "┼",
            "╂" => "╂",
            "┿" => "┿",
            "╋" => "╋",
            "╪" => "╪",
            "╫" => "╫",
            _ => "╬",
        });
    }
    let (nh, nw) = weight(new)?;
    let (eh, ew) = weight(existing)?;
    if nh == eh {
        return None;
    }
    let (hw, vw) = if nh { (nw, ew) } else { (ew, nw) };
    Some(match (hw, vw) {
        (0, 0) => "┼",
        (1, 0) => "┿",
        (0, 1) => "╂",
        (1, 1) => "╋",
        (2, 0) | (2, 1) => "╪",
        (0, 2) | (1, 2) => "╫",
        _ => "╬",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_chars_take_two_cells() {
        let mut c = Canvas::new(6, 1);
        let w = c.put_str(0, 0, "a🦀b", CellStyle::new());
        assert_eq!(w, 4);
        assert_eq!(c.row_text(0), "a🦀b  ");
        assert_eq!(c.get(2, 0).unwrap().sym, "");
    }

    #[test]
    fn put_line_makes_junctions_at_crossings() {
        let mut c = Canvas::new(3, 3);
        for y in 0..3 {
            c.put_line(1, y, "│", CellStyle::new());
        }
        for x in 0..3 {
            c.put_line(x, 1, "─", CellStyle::new());
        }
        assert_eq!(c.row_text(1), "─┼─");
        assert_eq!(c.row_text(0), " │ ");
    }

    #[test]
    fn clipped_adds_ellipsis() {
        let mut c = Canvas::new(5, 1);
        c.put_str_clipped(0, 0, "abcdefgh", 5, CellStyle::new());
        assert_eq!(c.row_text(0), "abcd…");
    }

    #[test]
    fn plain_text_trims() {
        let mut c = Canvas::new(5, 2);
        c.put_str(0, 1, "hi", CellStyle::new());
        assert_eq!(c.to_plain_text(), "\nhi");
    }

    #[test]
    fn junctions_follow_both_strokes() {
        assert_eq!(junction("─", "│"), Some("┼"));
        assert_eq!(junction("━", "│"), Some("┿"));
        assert_eq!(junction("─", "┃"), Some("╂"));
        assert_eq!(junction("━", "┃"), Some("╋"));
        assert_eq!(junction("═", "│"), Some("╪"));
        assert_eq!(junction("─", "║"), Some("╫"));
        assert_eq!(junction("═", "║"), Some("╬"));
        assert_eq!(
            junction("╌", "┆"),
            Some("┼"),
            "dashed and dotted count as light"
        );
        assert_eq!(junction("─", "─"), None, "parallel lines overwrite");
        assert_eq!(junction("│", "┼"), Some("┼"));
        assert_eq!(junction("a", "│"), None);
    }
}
