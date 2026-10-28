//! Internal crate of [Tiza](https://github.com/luiscarlosjayk/tiza),
//! published only so that `cargo install tiza` works: it has no stable
//! API. Use the `tiza` binary.
//!
//! `tiza-core`: the Tiza core, with no terminal dependencies.
//!
//! - [`model`][]: document, anchors and annotations (serializable to `.tiza`).
//! - [`layout`][]: from document + theme + width to a [`canvas::Canvas`] of cells.
//! - [`highlight`][]: syntax highlighting (syntect + two-face).
//! - [`protocol`][]: types of the local NDJSON API shared by the TUI, CLI and MCP.
//! - [`fs`][]: open sources/`.tiza` files and save.
//! - [`history`][]: undo/redo.
//! - [`printable`][]: control characters made visible.
//! - [`config`][]: `config.toml`, the user's personal settings.

pub mod anim;
pub mod canvas;
pub mod carry;
pub mod cast;
pub mod check;
pub mod config;
pub mod effects;
pub mod error;
pub mod fs;
pub mod highlight;
pub mod history;
pub mod layout;
pub mod model;
pub mod moves;
pub mod printable;
pub mod protocol;
pub mod schema;
pub mod socket;
pub mod stale;
pub mod steps;
pub mod theme;

pub use error::Error;
pub use model::{Anchor, Annotation, Document, MarkStyle, Shape, TextSpan, ThemeName};
pub use theme::Theme;

pub const APP_NAME: &str = "tiza";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Short git commit the binary was built from (`unknown` outside a checkout,
/// `-dirty` when the tree had uncommitted changes).
pub const GIT_SHA: &str = env!("TIZA_GIT_SHA");
/// `0.0.1 (abc1234def)`: what `tiza --version` prints.
pub const VERSION_LONG: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("TIZA_GIT_SHA"), ")");

/// Text logo (minimum one-cell-wide form: `╭▶`).
pub const WORDMARK: &str = "╭─ Tiza ─▶";
