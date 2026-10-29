//! Internal crate of [Tiza](https://github.com/luiscarlosjayk/tiza),
//! published only so that `cargo install tiza` works: it has no stable
//! API. Use the `tiza` binary.
//!
//! Export of a rendered [`Canvas`] to SVG, PNG, GIF and lossless WebP.
//!
//! The SVG is a grid of monospace text runs with the theme's colors, so it
//! looks exactly like the terminal. Fonts (JetBrains Mono, OFL, plus a small
//! symbols fallback derived from DejaVu Sans Mono) are embedded, both in the
//! SVG (`@font-face`, optional) and in the PNG rasterizer, so the output does
//! not depend on the fonts installed on the machine.
pub mod fonts;
pub mod gif;
pub mod limits;
pub mod png;
pub mod svg;
pub mod webp;

pub use gif::{GifOptions, GifReport, to_gif, to_gif_from};
pub use limits::{check_lines, check_svg_lines, image_size, pixel_size, svg_size};
pub use png::{PngOptions, to_png};
pub use svg::{SvgOptions, to_svg};
pub use webp::{WebpAnimOptions, WebpReport, WebpStill, to_webp, to_webp_anim, to_webp_anim_from};

/// The frames of an animation, one at a time: each canvas (all of one
/// size) and how long it stays, in milliseconds. An encoder may go over
/// them more than once (a GIF counts its colors, then writes them), and
/// each time they are the same frames.
pub trait Frames {
    /// Gives each frame to `f`, in order; stops at the first error.
    fn each(
        &self,
        f: &mut dyn FnMut(&tiza_core::canvas::Canvas, u64) -> Result<(), Error>,
    ) -> Result<(), Error>;
}

impl Frames for [(tiza_core::canvas::Canvas, u64)] {
    fn each(
        &self,
        f: &mut dyn FnMut(&tiza_core::canvas::Canvas, u64) -> Result<(), Error>,
    ) -> Result<(), Error> {
        for (canvas, delay_ms) in self {
            f(canvas, *delay_ms)?;
        }
        Ok(())
    }
}

/// A take plays its record back at each pass: only the changes of its
/// frames stay in memory.
impl Frames for tiza_core::anim::Take {
    fn each(
        &self,
        f: &mut dyn FnMut(&tiza_core::canvas::Canvas, u64) -> Result<(), Error>,
    ) -> Result<(), Error> {
        tiza_core::anim::Take::each(self, |frame| f(frame.canvas, frame.delay_ms))
    }
}

/// Largest PNG side, in pixels.
pub const MAX_SIDE_PX: u32 = 16_384;
/// Largest PNG, in pixels (≈ 100 MP, 400 MB of RGBA).
pub const MAX_PIXELS: u64 = 100_000_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid SVG: {0}")]
    Svg(String),
    #[error("could not allocate a {0}×{1} image")]
    Alloc(u32, u32),
    #[error(
        "a {0}×{1} px image is over the limit ({MAX_SIDE_PX} px per side, {MAX_PIXELS} px in all): use a smaller width, scale, font size or margin, or export fewer lines (FILE:A-B)"
    )]
    TooBig(u32, u32),
    #[error(
        "{lines} lines of code make an image at least {height} px high, over the limit ({MAX_SIDE_PX} px per side, {MAX_PIXELS} px in all): export fewer lines (FILE:A-B), or use a smaller scale, font size or margin"
    )]
    TooManyLines { lines: usize, height: u32 },
    #[error(
        "a {0}×{1} px SVG is over the limit ({side} px per side, {all} px in all): use a smaller width, font size or margin, or export fewer lines (FILE:A-B)",
        side = limits::SVG_MAX_SIDE_PX,
        all = limits::SVG_MAX_PIXELS
    )]
    SvgTooBig(u32, u32),
    #[error(
        "{lines} lines of code make an SVG at least {height} px high, over the limit ({side} px per side, {all} px in all): export fewer lines (FILE:A-B), or use a smaller font size or margin",
        side = limits::SVG_MAX_SIDE_PX,
        all = limits::SVG_MAX_PIXELS
    )]
    SvgTooManyLines { lines: usize, height: u32 },
    #[error("PNG encoding failed: {0}")]
    Png(String),
    #[error("GIF encoding failed: {0}")]
    Gif(String),
    /// Says what is wrong itself: the CLI puts `could not encode the WebP`
    /// before it.
    #[error(transparent)]
    Webp(#[from] webp::WebpError),
}
