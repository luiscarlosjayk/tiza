//! Canvas → SVG.

use std::fmt::Write as _;

use base64::Engine as _;
use tiza_core::canvas::{Canvas, CellStyle, Rgb};
use tiza_core::theme::Theme;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::fonts;

#[derive(Debug, Clone)]
pub struct SvgOptions {
    /// Font size in px. Cells are 0.6 × 1.32 em.
    pub font_size: f32,
    /// Blank margin around the drawing, in cells.
    pub margin_cells: f32,
    /// Corner radius of the background, in px (0 for a plain rectangle).
    pub radius: f32,
    /// Embed the fonts as `@font-face` data URIs so the SVG renders the same
    /// everywhere: the regular face always, the bold/italic faces and the
    /// symbols fallback only when the frame uses them (~0.37 MB each in
    /// base64). Off, the SVG relies on the viewer's fonts.
    pub embed_fonts: bool,
    /// Optional `<title>`.
    pub title: Option<String>,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            margin_cells: 1.5,
            radius: 12.0,
            embed_fonts: true,
            title: None,
        }
    }
}

/// Pixel geometry of an export, derived from the canvas and the options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub cell_w: f32,
    pub cell_h: f32,
    pub margin: f32,
    pub width: f32,
    pub height: f32,
}

pub fn geometry(canvas: &Canvas, o: &SvgOptions) -> Geometry {
    geometry_for(canvas.width, canvas.height, o)
}

/// [`geometry`] of a canvas of `cols` × `rows` cells, without the canvas.
pub fn geometry_for(cols: usize, rows: usize, o: &SvgOptions) -> Geometry {
    let cell_w = o.font_size * fonts::CELL_WIDTH_EM;
    let cell_h = o.font_size * fonts::CELL_HEIGHT_EM;
    let margin = (o.margin_cells * cell_h).round();
    // Round to 1/1000 px before ceil so 50 × 9.6 is 480, not 481.
    let px = |v: f32| ((v * 1000.0).round() / 1000.0).ceil();
    Geometry {
        cell_w,
        cell_h,
        margin,
        width: px(cols as f32 * cell_w + 2.0 * margin),
        height: px(rows as f32 * cell_h + 2.0 * margin),
    }
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

/// XML text: the markup characters as entities, and a character XML 1.0
/// does not allow written as its picture or `�`. The cells never
/// hold a control character (they come out of `Canvas::put`), but a title
/// can, and one stray byte would make the whole file invalid.
fn escape(s: &str, out: &mut String) {
    for ch in tiza_core::printable::printable_line(s).chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\u{FFFE}' | '\u{FFFF}' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

/// A horizontal run of cells sharing one style.
struct Run {
    x: usize,
    cells: usize,
    text: String,
    style: CellStyle,
}

fn runs(canvas: &Canvas, y: usize) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut x = 0;
    while x < canvas.width {
        let Some(c) = canvas.get(x, y) else {
            x += 1;
            continue;
        };
        // Width of this grapheme in cells: following cells with an empty sym belong to it.
        let mut w = 1;
        while canvas
            .get(x + w, y)
            .is_some_and(|n| n.sym.is_empty() && n.style == c.style)
        {
            w += 1;
        }
        let sym = if c.sym.is_empty() {
            " "
        } else {
            c.sym.as_str()
        };
        match out.last_mut() {
            Some(r) if r.style == c.style && r.x + r.cells == x => {
                r.text.push_str(sym);
                r.cells += w;
            }
            _ => out.push(Run {
                x,
                cells: w,
                text: sym.to_string(),
                style: c.style,
            }),
        }
        x += w;
    }
    out
}

/// A label run (`CellStyle.prose`: notes, labels, legends) that starts with
/// symbol glyphs (a mark's `━▶`, `╳`, `✎▶`) is split in two: the glyphs get
/// their own run pinned to their cells and the words start on the next cell.
/// Each run is stretched to its cells with `textLength`, and a glyph from the
/// fallback font is not exactly one cell wide; in one run, the difference
/// would be spread over the words and shift them off the grid. (The words
/// could once also be set in the brand serif; the split stays because
/// the exported image is the same as with it.)
fn split_glyph_prefix(runs: Vec<Run>) -> Vec<Run> {
    let mut out = Vec::with_capacity(runs.len());
    for r in runs {
        if !r.style.prose {
            out.push(r);
            continue;
        }
        let prefix_len: usize = r
            .text
            .graphemes(true)
            .take_while(|g| !g.chars().next().is_some_and(is_prose_char))
            .map(|g| UnicodeWidthStr::width(g).max(1))
            .sum();
        let prefix_graphemes: usize = r
            .text
            .graphemes(true)
            .take_while(|g| !g.chars().next().is_some_and(is_prose_char))
            .count();
        if prefix_len == 0 || prefix_len >= r.cells {
            out.push(r);
            continue;
        }
        let split_at: usize = r
            .text
            .graphemes(true)
            .take(prefix_graphemes)
            .map(str::len)
            .sum();
        let (glyphs, words) = r.text.split_at(split_at);
        let mono = CellStyle {
            prose: false,
            ..r.style
        };
        out.push(Run {
            x: r.x,
            cells: prefix_len,
            text: glyphs.to_string(),
            style: mono,
        });
        out.push(Run {
            x: r.x + prefix_len,
            cells: r.cells - prefix_len,
            text: words.to_string(),
            style: r.style,
        });
    }
    out
}

/// Letters, digits, spaces and ASCII punctuation: the words of a label.
fn is_prose_char(c: char) -> bool {
    c.is_alphanumeric() || c == ' ' || (c.is_ascii_punctuation() && c != '|')
}

/// Which of [`fonts::FACES`] a frame draws with. The regular face is
/// always in; bold, italic and bold italic only when a run with that style
/// has text; the symbols fallback only when a character is missing from the
/// run's JetBrains Mono face and the symbols font has it (the browser falls
/// back per character, in the order of the `font-family` list).
#[derive(Debug)]
struct FacesUsed([bool; 5]);

impl FacesUsed {
    fn new() -> Self {
        let mut used = [false; 5];
        used[fonts::mono_face(false, false)] = true;
        Self(used)
    }

    fn add(&mut self, coverage: &fonts::Coverage, r: &Run) {
        let face = fonts::mono_face(r.style.bold, r.style.italic);
        self.0[face] = true;
        if !self.0[fonts::SYMBOLS_FACE]
            && r.text
                .chars()
                .any(|ch| !coverage.has(face, ch) && coverage.has(fonts::SYMBOLS_FACE, ch))
        {
            self.0[fonts::SYMBOLS_FACE] = true;
        }
    }
}

/// Renders the canvas as a standalone SVG document.
pub fn to_svg(canvas: &Canvas, theme: &Theme, o: &SvgOptions) -> String {
    let g = geometry(canvas, o);
    // Text runs first: the `<style>` above them embeds only the faces they use.
    let mut body = String::with_capacity(64 * 1024);
    let coverage = o.embed_fonts.then(fonts::Coverage::new);
    let mut used = FacesUsed::new();
    // Cell backgrounds, merged per run.
    for y in 0..canvas.height {
        for r in runs(canvas, y) {
            if let Some(bg) = r.style.bg
                && bg != theme.bg
            {
                let _ = writeln!(
                    body,
                    r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}"/>"#,
                    g.margin + r.x as f32 * g.cell_w,
                    g.margin + y as f32 * g.cell_h,
                    r.cells as f32 * g.cell_w,
                    g.cell_h,
                    hex(bg)
                );
            }
        }
    }
    // Text runs.
    let ascent = o.font_size * fonts::ASCENT_EM;
    for y in 0..canvas.height {
        let base_y = g.margin + y as f32 * g.cell_h + ascent;
        for r in split_glyph_prefix(runs(canvas, y)) {
            if r.text.trim().is_empty() {
                continue;
            }
            if let Some(c) = &coverage {
                used.add(c, &r);
            }
            let mut class = String::new();
            if r.style.bold {
                class.push_str("b ");
            }
            if r.style.italic {
                class.push_str("i ");
            }
            if r.style.dim {
                class.push_str("d ");
            }
            let _ = write!(
                body,
                r#"<text x="{:.2}" y="{:.2}" textLength="{:.2}" lengthAdjust="spacing" fill="{}""#,
                g.margin + r.x as f32 * g.cell_w,
                base_y,
                r.cells as f32 * g.cell_w,
                hex(r.style.fg.unwrap_or(theme.fg))
            );
            if !class.is_empty() {
                let _ = write!(body, r#" class="{}""#, class.trim_end());
            }
            body.push_str(r#" xml:space="preserve">"#);
            escape(&r.text, &mut body);
            body.push_str("</text>\n");
        }
    }

    let mut s = String::with_capacity(body.len() + if o.embed_fonts { 2 << 20 } else { 1024 });
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
        w = g.width,
        h = g.height
    );
    s.push('\n');
    if let Some(t) = &o.title {
        s.push_str("<title>");
        escape(t, &mut s);
        s.push_str("</title>\n");
    }
    // Styles: font faces (optional, only those the runs use) and the shared text attributes.
    s.push_str("<style>\n");
    if o.embed_fonts {
        for (f, _) in fonts::FACES.iter().zip(used.0).filter(|(_, u)| *u) {
            let b64 = base64::engine::general_purpose::STANDARD.encode(f.data);
            let _ = writeln!(
                s,
                "@font-face{{font-family:\"{}\";font-weight:{};font-style:{};src:url(data:font/ttf;base64,{}) format(\"truetype\");}}",
                f.family,
                f.weight,
                if f.italic { "italic" } else { "normal" },
                b64
            );
        }
    }
    let _ = writeln!(
        s,
        "text{{font-family:\"{}\",\"{}\",ui-monospace,Menlo,Consolas,monospace;font-size:{}px;white-space:pre;}}",
        fonts::FAMILY,
        fonts::SYMBOLS_FAMILY,
        o.font_size
    );
    s.push_str(".b{font-weight:700}.i{font-style:italic}.d{opacity:.55}\n");
    s.push_str("</style>\n");
    // Background.
    let _ = writeln!(
        s,
        r#"<rect width="{}" height="{}" rx="{}" fill="{}"/>"#,
        g.width,
        g.height,
        o.radius,
        hex(theme.bg)
    );
    s.push_str(&body);
    s.push_str("</svg>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_merge_equal_styles_and_keep_wide_cells() {
        let mut c = Canvas::new(10, 1);
        let st = CellStyle::new().fg(Rgb(1, 2, 3));
        c.put_str(0, 0, "ab", st);
        c.put_str(2, 0, "🔥", st); // two cells
        c.put_str(4, 0, "x", CellStyle::new().fg(Rgb(9, 9, 9)).bold());
        let r = runs(&c, 0);
        assert_eq!(r[0].text, "ab🔥");
        assert_eq!(r[0].cells, 4);
        assert_eq!(r[1].text, "x");
        assert!(r[1].style.bold);
    }

    #[test]
    fn control_characters_never_make_the_xml_invalid() {
        let mut c = Canvas::new(8, 1);
        c.put_str(0, 0, "a\x1b\0\u{9b}b", CellStyle::new());
        let o = SvgOptions {
            embed_fonts: false,
            title: Some("t\x07\u{FFFF}\n".into()),
            ..Default::default()
        };
        let svg = to_svg(&c, &Theme::dark(), &o);
        assert!(svg.contains("a␛␀\u{FFFD}b"), "{svg}");
        assert!(svg.contains("<title>t␇\u{FFFD}␊</title>"), "{svg}");
        assert!(!svg.chars().any(|ch| ch < ' ' && ch != '\n'));
    }

    #[test]
    fn svg_has_background_text_and_escapes() {
        let mut c = Canvas::new(8, 2);
        c.put_str(0, 0, "a<b>&", CellStyle::new());
        c.put_str(0, 1, "hl", CellStyle::new().bg(Rgb(10, 20, 30)));
        let th = Theme::dark();
        let o = SvgOptions {
            embed_fonts: false,
            ..Default::default()
        };
        let svg = to_svg(&c, &th, &o);
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("a&lt;b&gt;&amp;"));
        assert!(svg.contains(r##"fill="#0a141e""##), "cell background rect");
        assert!(!svg.contains("@font-face"));
        let g = geometry(&c, &o);
        assert_eq!(g.cell_w, 9.6);
        assert!(svg.contains(&format!(r#"width="{}""#, g.width)));
        let embedded = to_svg(&c, &th, &SvgOptions::default());
        assert_eq!(embedded.matches("@font-face").count(), 1, "regular only");
    }

    fn faces(svg: &str) -> Vec<String> {
        svg.lines()
            .filter(|l| l.starts_with("@font-face"))
            .map(|l| l.split(";src:").next().unwrap().to_string())
            .collect()
    }

    #[test]
    fn only_the_faces_the_frame_draws_are_embedded() {
        let th = Theme::dark();
        let o = SvgOptions::default();
        let face = |w: u16, style: &str| {
            format!("@font-face{{font-family:\"JetBrains Mono\";font-weight:{w};font-style:{style}")
        };
        let symbols = "@font-face{font-family:\"Tiza Symbols\";font-weight:400;font-style:normal";
        // Plain text: regular only.
        let mut c = Canvas::new(20, 3);
        c.put_str(0, 0, "let x = 1; ─│┌", CellStyle::new());
        assert_eq!(faces(&to_svg(&c, &th, &o)), vec![face(400, "normal")]);
        // A bold run brings the bold face; a bold run of spaces does not.
        c.put_str(0, 1, "   ", CellStyle::new().bold());
        assert_eq!(faces(&to_svg(&c, &th, &o)).len(), 1);
        c.put_str(0, 1, "Title", CellStyle::new().bold());
        assert_eq!(
            faces(&to_svg(&c, &th, &o)),
            vec![face(400, "normal"), face(700, "normal")]
        );
        // Italic and bold italic, each its own face.
        c.put_str(6, 1, "note", CellStyle::new().italic());
        c.put_str(11, 1, "both", CellStyle::new().bold().italic());
        assert_eq!(
            faces(&to_svg(&c, &th, &o)),
            vec![
                face(400, "normal"),
                face(700, "normal"),
                face(400, "italic"),
                face(700, "italic")
            ]
        );
        // A symbol JetBrains Mono lacks brings the fallback.
        c.put_str(0, 2, "★ hot", CellStyle::new());
        assert_eq!(faces(&to_svg(&c, &th, &o)).last().unwrap(), symbols);
        // `--no-embed-fonts`: none at all.
        let none = SvgOptions {
            embed_fonts: false,
            ..Default::default()
        };
        assert!(faces(&to_svg(&c, &th, &none)).is_empty());
    }
}
