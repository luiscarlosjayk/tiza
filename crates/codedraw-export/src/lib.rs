//! Export of a rendered [`Canvas`] to SVG and PNG.
//!
//! The SVG is a grid of monospace text runs with the theme's colors, so it
//! looks exactly like the terminal. Fonts (JetBrains Mono, OFL, plus a small
//! symbols fallback derived from DejaVu Sans Mono) are embedded, both in the
//! SVG (`@font-face`, optional) and in the PNG rasterizer, so the output does
//! not depend on the fonts installed on the machine.
pub mod fonts;
pub mod png;
pub mod svg;

pub use png::{PngOptions, to_png};
pub use svg::{SvgOptions, to_svg};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid SVG: {0}")]
    Svg(String),
    #[error("could not allocate a {0}×{1} image")]
    Alloc(u32, u32),
    #[error("PNG encoding failed: {0}")]
    Png(String),
}
