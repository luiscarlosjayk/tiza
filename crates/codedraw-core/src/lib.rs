//! `codedraw-core`: the CodeDraw core, with no terminal dependencies.
//!
//! - [`model`][]: document, anchors and annotations (serializable to `.codedraw`).
//! - [`layout`][]: from document + theme + width to a [`canvas::Canvas`] of cells.
//! - [`highlight`][]: syntax highlighting (syntect + two-face).
//! - [`protocol`][]: types of the local NDJSON API shared by the TUI, CLI and MCP.
//! - [`fs`][]: open sources/`.codedraw` files and save.
//! - [`history`][]: undo/redo.

pub mod canvas;
pub mod cast;
pub mod error;
pub mod fs;
pub mod highlight;
pub mod history;
pub mod layout;
pub mod model;
pub mod protocol;
pub mod schema;
pub mod socket;
pub mod theme;

pub use error::Error;
pub use model::{Anchor, Annotation, Document, MarkStyle, Shape, TextSpan, ThemeName};
pub use theme::Theme;

pub const APP_NAME: &str = "codedraw";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Short git commit the binary was built from (`unknown` outside a checkout,
/// `-dirty` when the tree had uncommitted changes).
pub const GIT_SHA: &str = env!("CODEDRAW_GIT_SHA");
/// `0.0.1 (abc1234def)`: what `codedraw --version` prints.
pub const VERSION_LONG: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("CODEDRAW_GIT_SHA"),
    ")"
);

/// Text logo (minimum one-cell-wide form: `╭▶`).
pub const WORDMARK: &str = "╭─ CodeDraw ─▶";
