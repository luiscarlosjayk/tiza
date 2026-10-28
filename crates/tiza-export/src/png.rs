//! SVG → PNG with resvg, using the embedded fonts.

use std::sync::{Arc, OnceLock};

use resvg::tiny_skia::{Color, Pixmap, Transform};
use resvg::usvg;
use usvg::fontdb::{Database, Source};

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

/// The fonts of the rasterizer: the embedded faces, then the system fonts
/// with `system_fonts`. Each of the two databases is built once per
/// process, at its first use, and every rasterization after it shares it
/// (usvg only reads it). So a long-lived process that asks for the system
/// fonts keeps the ones that it found at its first export with them: it
/// sees a font installed after that only when it starts again. (Today only
/// `tiza render --system-fonts` asks for them, and it exports once.)
fn font_db(system_fonts: bool) -> Arc<Database> {
    static EMBEDDED: OnceLock<Arc<Database>> = OnceLock::new();
    static WITH_SYSTEM: OnceLock<Arc<Database>> = OnceLock::new();
    let db = if system_fonts {
        &WITH_SYSTEM
    } else {
        &EMBEDDED
    };
    Arc::clone(db.get_or_init(|| {
        let mut db = Database::new();
        for f in fonts::FACES {
            // The bytes in the binary, not a copy.
            db.load_font_source(Source::Binary(Arc::new(f.data)));
        }
        if system_fonts {
            db.load_system_fonts();
        }
        db.set_monospace_family(fonts::FAMILY);
        Arc::new(db)
    }))
}

/// Rasterizes an SVG produced by [`crate::to_svg`] (fonts need not be embedded
/// in it: the rasterizer carries its own copies).
pub fn to_png(svg: &str, o: &PngOptions) -> Result<Vec<u8>, Error> {
    rasterize(svg, o)?
        .encode_png()
        .map_err(|e| Error::Png(e.to_string()))
}

/// The pixels of an SVG at `o.scale` (what `to_png` encodes).
pub fn rasterize(svg: &str, o: &PngOptions) -> Result<Pixmap, Error> {
    let r = Raster::new(svg, o)?;
    let mut pixmap = Pixmap::new(r.width, r.height).ok_or(Error::Alloc(r.width, r.height))?;
    resvg::render(&r.tree, r.transform(0), &mut pixmap.as_mut());
    Ok(pixmap)
}

/// The height of a strip of [`Raster::strips`], in pixel rows.
pub(crate) const STRIP_ROWS: u32 = 1024;

/// An SVG parsed for rasterizing, and the size in pixels that it has.
pub(crate) struct Raster {
    tree: usvg::Tree,
    scale: f32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Raster {
    pub(crate) fn new(svg: &str, o: &PngOptions) -> Result<Self, Error> {
        let opts = usvg::Options {
            fontdb: font_db(o.system_fonts),
            font_family: fonts::FAMILY.to_string(),
            ..Default::default()
        };
        let tree = usvg::Tree::from_str(svg, &opts).map_err(|e| Error::Svg(e.to_string()))?;
        let size = tree.size();
        let (width, height) = crate::limits::pixel_size(size.width(), size.height(), o.scale)?;
        Ok(Self {
            tree,
            scale: o.scale,
            width,
            height,
        })
    }

    /// The transform that puts pixel row `top` of the image at the top of
    /// a pixmap.
    fn transform(&self, top: u32) -> Transform {
        let scale = Transform::from_scale(self.scale, self.scale);
        match top {
            0 => scale,
            _ => scale.post_translate(0.0, -(top as f32)),
        }
    }

    /// Rasterizes the image in strips of [`STRIP_ROWS`] rows, from top to
    /// bottom, and gives each strip to `each`: the pixels of the whole
    /// image are never in memory. The pixels are those of [`rasterize`],
    /// except at the edge of a strip: a glyph or a round corner that
    /// crosses it is cut there, and some of its edge pixels change a
    /// little. (tiny-skia cuts an image taller than 8191 rows in the same
    /// way at row 8191, so [`rasterize`] has such pixels there.)
    ///
    /// Only the strips `k` (0 at the top) for which `wanted(k)` is true are
    /// rasterized; `each` gets `k` and the pixels.
    pub(crate) fn strips(
        &self,
        buffers: &mut Strips,
        wanted: impl Fn(usize) -> bool,
        mut each: impl FnMut(usize, &Pixmap) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut top = 0;
        let mut k = 0;
        while top < self.height {
            let rows = STRIP_ROWS.min(self.height - top);
            if wanted(k) {
                let pixmap = buffers.get(self.width, rows)?;
                resvg::render(&self.tree, self.transform(top), &mut pixmap.as_mut());
                each(k, pixmap)?;
            }
            top += rows;
            k += 1;
        }
        Ok(())
    }
}

/// The pixmaps of [`Raster::strips`]: one for a whole strip and one for
/// the last, shorter strip. Keep one `Strips` for all the frames of an
/// animation, so that a frame allocates no strip pixmap.
#[derive(Default)]
pub(crate) struct Strips {
    pixmaps: Vec<Pixmap>,
}

impl Strips {
    /// A transparent pixmap of `width × rows`.
    fn get(&mut self, width: u32, rows: u32) -> Result<&mut Pixmap, Error> {
        let i = match self
            .pixmaps
            .iter()
            .position(|p| (p.width(), p.height()) == (width, rows))
        {
            Some(i) => i,
            None => {
                let p = Pixmap::new(width, rows).ok_or(Error::Alloc(width, rows))?;
                self.pixmaps.push(p);
                self.pixmaps.len() - 1
            }
        };
        let pixmap = &mut self.pixmaps[i];
        pixmap.fill(Color::TRANSPARENT);
        Ok(pixmap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::{SvgOptions, to_svg};
    use tiza_core::canvas::{Canvas, CellStyle, Rgb};
    use tiza_core::theme::Theme;

    #[test]
    fn the_font_databases_with_and_without_system_fonts_stay_apart() {
        let without = font_db(false);
        let with = font_db(true);
        assert!(!Arc::ptr_eq(&without, &with));
        assert_eq!(font_db(false).len(), fonts::FACES.len());
        assert!(Arc::ptr_eq(&font_db(true), &with), "built only once");
    }

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

    /// The strips of a frame, one under the other, are the whole frame. A
    /// frame of colored rows, with no text and square corners: no curve
    /// crosses the edge of a strip, so not one pixel may change. (A glyph
    /// or a round corner that crosses it changes a little.)
    #[test]
    fn a_frame_taller_than_a_strip_has_the_pixels_of_the_whole_frame() {
        let mut c = Canvas::new(6, 70);
        for row in 0..70usize {
            let n = row as u8;
            let bg = Rgb(n.wrapping_mul(37), n.wrapping_mul(91), 255 - n);
            c.put_str(0, row, "      ", CellStyle::new().bg(bg));
        }
        let svg = to_svg(
            &c,
            &Theme::dark(),
            &SvgOptions {
                radius: 0.0,
                ..Default::default()
            },
        );
        let o = PngOptions::default();
        let whole = rasterize(&svg, &o).unwrap();
        let (w, h) = (whole.width(), whole.height());
        assert!(
            h > 2 * STRIP_ROWS && h % STRIP_ROWS != 0,
            "{h} rows: two whole strips or more, and a shorter last one"
        );
        let raster = Raster::new(&svg, &o).unwrap();
        let mut buffers = Strips::default();
        // Two frames of an animation: the second one reuses the pixmaps of
        // the first, and its strips are the whole frame too.
        for _ in 0..2 {
            let mut joined = Vec::new();
            let mut heights = Vec::new();
            raster
                .strips(
                    &mut buffers,
                    |_| true,
                    |_, strip| {
                        assert_eq!(strip.width(), w);
                        heights.push(strip.height());
                        joined.extend_from_slice(strip.data());
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!(heights.iter().sum::<u32>(), h, "strips of {heights:?} rows");
            assert!(
                joined == whole.data(),
                "the strips differ from the whole frame"
            );
        }
        assert_eq!(
            buffers.pixmaps.len(),
            2,
            "one pixmap for a whole strip and one for the last strip"
        );
    }
}
