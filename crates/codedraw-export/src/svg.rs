//! Canvas → SVG.

use std::fmt::Write as _;

use base64::Engine as _;
use codedraw_core::canvas::{Canvas, CellStyle, Rgb};
use codedraw_core::theme::Theme;

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
    /// everywhere (~1.2 MB). Off, the SVG relies on the viewer's fonts.
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
    let cell_w = o.font_size * fonts::CELL_WIDTH_EM;
    let cell_h = o.font_size * fonts::CELL_HEIGHT_EM;
    let margin = (o.margin_cells * cell_h).round();
    Geometry {
        cell_w,
        cell_h,
        margin,
        width: (canvas.width as f32 * cell_w + 2.0 * margin).ceil(),
        height: (canvas.height as f32 * cell_h + 2.0 * margin).ceil(),
    }
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

fn escape(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
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

/// Renders the canvas as a standalone SVG document.
pub fn to_svg(canvas: &Canvas, theme: &Theme, o: &SvgOptions) -> String {
    let g = geometry(canvas, o);
    let mut s = String::with_capacity(64 * 1024);
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
    // Styles: font faces (optional) and the shared text attributes.
    s.push_str("<style>\n");
    if o.embed_fonts {
        for f in fonts::FACES {
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
    s.push_str(".b{font-weight:700}.i{font-style:italic}.d{opacity:.55}\n</style>\n");
    // Background.
    let _ = writeln!(
        s,
        r#"<rect width="{}" height="{}" rx="{}" fill="{}"/>"#,
        g.width,
        g.height,
        o.radius,
        hex(theme.bg)
    );
    // Cell backgrounds, merged per run.
    for y in 0..canvas.height {
        for r in runs(canvas, y) {
            if let Some(bg) = r.style.bg
                && bg != theme.bg
            {
                let _ = writeln!(
                    s,
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
        for r in runs(canvas, y) {
            if r.text.trim().is_empty() {
                continue;
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
                s,
                r#"<text x="{:.2}" y="{:.2}" textLength="{:.2}" lengthAdjust="spacing" fill="{}""#,
                g.margin + r.x as f32 * g.cell_w,
                base_y,
                r.cells as f32 * g.cell_w,
                hex(r.style.fg.unwrap_or(theme.fg))
            );
            if !class.is_empty() {
                let _ = write!(s, r#" class="{}""#, class.trim_end());
            }
            s.push_str(r#" xml:space="preserve">"#);
            escape(&r.text, &mut s);
            s.push_str("</text>\n");
        }
    }
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
        assert!(embedded.matches("@font-face").count() == fonts::FACES.len());
    }
}
