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
pub mod png;
pub mod svg;
pub mod webp;

pub use gif::{GifOptions, GifReport, to_gif};
pub use png::{PngOptions, to_png};
pub use svg::{SvgOptions, to_svg};
pub use webp::{WebpAnimOptions, WebpReport, WebpStill, to_webp, to_webp_anim};

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
        "a {0}×{1} px image is over the limit ({MAX_SIDE_PX} px per side, {MAX_PIXELS} px in all): lower width, scale or font size"
    )]
    TooBig(u32, u32),
    #[error("PNG encoding failed: {0}")]
    Png(String),
    #[error("GIF encoding failed: {0}")]
    Gif(String),
    /// Says what is wrong itself: the CLI puts `could not encode the WebP`
    /// before it.
    #[error(transparent)]
    Webp(#[from] webp::WebpError),
}
