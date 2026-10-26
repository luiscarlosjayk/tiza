//! The size limits of an export, in one place. A raster image (PNG, WebP,
//! a frame of a GIF or a video) can have at most [`MAX_SIDE_PX`] px per
//! side and [`MAX_PIXELS`] px in all. An SVG holds no pixels, so its limit
//! is higher ([`SVG_MAX_SIDE_PX`], [`SVG_MAX_PIXELS`]), but the SVG of a
//! huge document still fills the memory.
//!
//! One formula gives the size, from [`crate::svg::geometry_for`]: `rasterize`
//! uses it, and so do the checks that come before the SVG ([`image_size`],
//! [`svg_size`]) and before the layout ([`check_lines`],
//! [`check_svg_lines`]), so they cannot give different answers. Bounded
//! before any allocation: a typo in width or scale must be an error, never
//! an aborted process.

use tiza_core::canvas::Canvas;

use crate::Error;
use crate::png::PngOptions;
use crate::svg::{SvgOptions, geometry_for};

pub use crate::{MAX_PIXELS, MAX_SIDE_PX};

/// Largest side of an SVG, in px: 8 times that of a raster image.
pub const SVG_MAX_SIDE_PX: u32 = 8 * MAX_SIDE_PX;
/// Largest SVG, in px: 64 times a raster image.
pub const SVG_MAX_PIXELS: u64 = 64 * MAX_PIXELS;

/// `width` × `height` px at `scale`, rounded up, if within `side` px per
/// side and `pixels` px in all; else that size as the error.
fn within(
    width: f32,
    height: f32,
    scale: f32,
    side: u32,
    pixels: u64,
) -> Result<(u32, u32), (u32, u32)> {
    let w = (width * scale).ceil().max(1.0) as u32;
    let h = (height * scale).ceil().max(1.0) as u32;
    if w > side || h > side || u64::from(w) * u64::from(h) > pixels {
        Err((w, h))
    } else {
        Ok((w, h))
    }
}

/// The size in pixels of a raster image of `width` × `height` SVG px at
/// `scale`, or [`Error::TooBig`].
pub fn pixel_size(width: f32, height: f32, scale: f32) -> Result<(u32, u32), Error> {
    within(width, height, scale, MAX_SIDE_PX, MAX_PIXELS).map_err(|(w, h)| Error::TooBig(w, h))
}

/// The size in pixels of the raster image of a canvas of `cols` × `rows`
/// cells.
pub fn pixel_size_for(
    cols: usize,
    rows: usize,
    svg: &SvgOptions,
    png: &PngOptions,
) -> Result<(u32, u32), Error> {
    let g = geometry_for(cols, rows, svg);
    pixel_size(g.width, g.height, png.scale)
}

/// The size in pixels of the raster image of `canvas`, before its SVG is
/// made: an image over the limits fails here, not after the SVG of a huge
/// document fills the memory.
pub fn image_size(
    canvas: &Canvas,
    svg: &SvgOptions,
    png: &PngOptions,
) -> Result<(u32, u32), Error> {
    pixel_size_for(canvas.width, canvas.height, svg, png)
}

/// Refuses, before any layout, a take whose code alone makes the raster
/// image too large. A layout has a row for each line of the code that it
/// shows, and is at least one column wide, so `lines` rows of one column
/// are the smallest image that it can give: this never refuses an image
/// that [`image_size`] accepts.
pub fn check_lines(lines: usize, svg: &SvgOptions, png: &PngOptions) -> Result<(), Error> {
    // Too large with no code at all: the margin, the font size or the
    // scale, not the lines.
    pixel_size_for(1, 0, svg, png)?;
    match pixel_size_for(1, lines, svg, png) {
        Err(Error::TooBig(_, height)) => Err(Error::TooManyLines { lines, height }),
        r => r.map(|_| ()),
    }
}

/// The size in px of the SVG of a canvas of `cols` × `rows` cells, or
/// [`Error::SvgTooBig`]: the same geometry as a raster image, at scale 1,
/// with the limits of an SVG.
fn svg_size_for(cols: usize, rows: usize, svg: &SvgOptions) -> Result<(u32, u32), Error> {
    let g = geometry_for(cols, rows, svg);
    within(g.width, g.height, 1.0, SVG_MAX_SIDE_PX, SVG_MAX_PIXELS)
        .map_err(|(w, h)| Error::SvgTooBig(w, h))
}

/// The size in px of the SVG of `canvas`, before it is made.
pub fn svg_size(canvas: &Canvas, svg: &SvgOptions) -> Result<(u32, u32), Error> {
    svg_size_for(canvas.width, canvas.height, svg)
}

/// [`check_lines`] for an SVG: never refuses an SVG that [`svg_size`]
/// accepts.
pub fn check_svg_lines(lines: usize, svg: &SvgOptions) -> Result<(), Error> {
    svg_size_for(1, 0, svg)?;
    match svg_size_for(1, lines, svg) {
        Err(Error::SvgTooBig(_, height)) => Err(Error::SvgTooManyLines { lines, height }),
        r => r.map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::png::rasterize;
    use crate::svg::to_svg;
    use tiza_core::theme::Theme;

    /// The size before the SVG is the size that `rasterize` makes: one
    /// function for both.
    #[test]
    fn image_size_is_the_size_of_the_rasterized_image() {
        let theme = Theme::dark();
        for (cols, rows, font_size, margin_cells, scale) in [
            (20, 1, 16.0, 1.5, 2.0),
            (100, 37, 13.0, 0.0, 1.0),
            (73, 5, 9.5, 3.25, 1.5),
            (400, 2, 4.0, 20.0, 0.25),
        ] {
            let c = Canvas::new(cols, rows);
            let svg = SvgOptions {
                font_size,
                margin_cells,
                embed_fonts: false,
                ..Default::default()
            };
            let png = PngOptions {
                scale,
                system_fonts: false,
            };
            let pm = rasterize(&to_svg(&c, &theme, &svg), &png).unwrap();
            assert_eq!(
                image_size(&c, &svg, &png).unwrap(),
                (pm.width(), pm.height())
            );
        }
    }

    /// `check_lines` is a lower bound: what it refuses, the exact size
    /// refuses too, at any width, margin and scale.
    #[test]
    fn check_lines_never_refuses_what_the_exact_size_accepts() {
        for font_size in [4.0, 16.0, 96.0] {
            for margin_cells in [0.0, 1.5, 20.0] {
                for scale in [0.25, 2.0, 8.0] {
                    let svg = SvgOptions {
                        font_size,
                        margin_cells,
                        ..Default::default()
                    };
                    let png = PngOptions {
                        scale,
                        system_fonts: false,
                    };
                    for lines in (0..40_000).step_by(97).chain([783, 784, 785, 786]) {
                        if check_lines(lines, &svg, &png).is_err() {
                            for cols in [1, 20, 100, 400] {
                                assert!(
                                    pixel_size_for(cols, lines, &svg, &png).is_err(),
                                    "{lines} lines, {cols} columns, {font_size} px, {margin_cells} cells, scale {scale}"
                                );
                            }
                        }
                    }
                }
            }
        }
        // And it refuses a document whose code alone is too high.
        let o = (SvgOptions::default(), PngOptions::default());
        assert!(matches!(
            check_lines(200_000, &o.0, &o.1),
            Err(Error::TooManyLines { lines: 200_000, .. })
        ));
        assert!(check_lines(100, &o.0, &o.1).is_ok());
    }

    /// The same for an SVG: what `check_svg_lines` refuses, `svg_size`
    /// refuses too; under the limit, the SVG has the size that it says.
    #[test]
    fn check_svg_lines_never_refuses_what_svg_size_accepts() {
        for font_size in [4.0, 16.0, 96.0] {
            for margin_cells in [0.0, 1.5, 20.0] {
                let svg = SvgOptions {
                    font_size,
                    margin_cells,
                    ..Default::default()
                };
                for lines in (0..60_000).step_by(211) {
                    if check_svg_lines(lines, &svg).is_err() {
                        for cols in [1, 20, 400] {
                            assert!(
                                svg_size_for(cols, lines, &svg).is_err(),
                                "{lines} {cols} {font_size} {margin_cells}"
                            );
                        }
                    }
                }
            }
        }
        let o = SvgOptions::default();
        assert!(matches!(
            check_svg_lines(7_000, &o),
            Err(Error::SvgTooManyLines { lines: 7_000, .. })
        ));
        assert!(check_svg_lines(6_000, &o).is_ok());
        let c = Canvas::new(30, 4);
        let text = to_svg(&c, &Theme::dark(), &o);
        let (w, h) = svg_size(&c, &o).unwrap();
        assert!(
            text.contains(&format!("width=\"{w}\"")) && text.contains(&format!("height=\"{h}\""))
        );
    }
}
