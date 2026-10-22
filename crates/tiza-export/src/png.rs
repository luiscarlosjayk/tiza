//! SVG → PNG with resvg, using the embedded fonts.

use resvg::usvg;

use crate::{Error, fonts};

#[derive(Debug, Clone)]
pub struct PngOptions {
    /// Device scale: 2 renders at twice the SVG's pixel size (crisp on HiDPI and video).
    pub scale: f32,
    /// Also load the system fonts as a fallback (emoji, CJK). Off by default so
    /// the output is identical on every machine.
    pub system_fonts: bool,
}

impl Default for PngOptions {
    fn default() -> Self {
        Self {
            scale: 2.0,
            system_fonts: false,
        }
    }
}

fn font_db(system_fonts: bool) -> usvg::fontdb::Database {
    let mut db = usvg::fontdb::Database::new();
    for f in fonts::FACES {
        db.load_font_data(f.data.to_vec());
    }
    if system_fonts {
        db.load_system_fonts();
    }
    db.set_monospace_family(fonts::FAMILY);
    db
}

/// Rasterizes an SVG produced by [`crate::to_svg`] (fonts need not be embedded
/// in it: the rasterizer carries its own copies).
pub fn to_png(svg: &str, o: &PngOptions) -> Result<Vec<u8>, Error> {
    rasterize(svg, o)?
        .encode_png()
        .map_err(|e| Error::Png(e.to_string()))
}

/// The pixels of an SVG at `o.scale` (what `to_png` encodes; the GIF
/// encoder reads them directly).
pub fn rasterize(svg: &str, o: &PngOptions) -> Result<resvg::tiny_skia::Pixmap, Error> {
    let opts = usvg::Options {
        fontdb: std::sync::Arc::new(font_db(o.system_fonts)),
        font_family: fonts::FAMILY.to_string(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &opts).map_err(|e| Error::Svg(e.to_string()))?;
    let size = tree.size();
    let w = (size.width() * o.scale).ceil().max(1.0) as u32;
    let h = (size.height() * o.scale).ceil().max(1.0) as u32;
    // Bounded before any allocation: a typo in width or scale must be an
    // error, never an aborted process.
    if w > crate::MAX_SIDE_PX
        || h > crate::MAX_SIDE_PX
        || u64::from(w) * u64::from(h) > crate::MAX_PIXELS
    {
        return Err(Error::TooBig(w, h));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or(Error::Alloc(w, h))?;
    let ts = resvg::tiny_skia::Transform::from_scale(o.scale, o.scale);
    resvg::render(&tree, ts, &mut pixmap.as_mut());
    Ok(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::{SvgOptions, to_svg};
    use tiza_core::canvas::{Canvas, CellStyle, Rgb};
    use tiza_core::theme::Theme;

    #[test]
    fn png_has_the_scaled_size_and_paints_text() {
        let mut c = Canvas::new(12, 2);
        c.put_str(
            0,
            0,
            "fn main() {}",
            CellStyle::new().fg(Rgb(255, 255, 255)),
        );
        c.put_str(0, 1, "╭─ ★ ✎ ─╮", CellStyle::new().fg(Rgb(255, 128, 0)));
        let o = SvgOptions {
            embed_fonts: false,
            ..Default::default()
        };
        let svg = to_svg(&c, &Theme::dark(), &o);
        let png = to_png(&svg, &PngOptions::default()).unwrap();
        let pm = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        let g = crate::svg::geometry(&c, &o);
        assert_eq!(
            (pm.width(), pm.height()),
            ((g.width * 2.0) as u32, (g.height * 2.0) as u32)
        );
        // Something was drawn in the text rows: not every pixel is the background.
        let bg = Theme::dark().bg;
        let painted = pm
            .pixels()
            .iter()
            .filter(|p| (p.red(), p.green(), p.blue()) != (bg.0, bg.1, bg.2))
            .count();
        assert!(painted > 200, "{painted} painted pixels");
    }
}
