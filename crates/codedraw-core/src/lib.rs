//! `codedraw-core`: el núcleo de CodeDraw sin dependencias de terminal.
//!
//! - [`model`][]: documento, anclas y anotaciones (serializables a `.codedraw`).
//! - [`layout`][]: de documento + tema + ancho a un [`canvas::Canvas`] de celdas.
//! - [`highlight`][]: resaltado de sintaxis (syntect + two-face).
//! - [`protocol`][]: tipos de la API local NDJSON que comparten TUI, CLI y MCP.
//! - [`fs`][]: abrir fuentes/`.codedraw` y guardar.
//! - [`history`][]: deshacer/rehacer.

pub mod canvas;
pub mod error;
pub mod fs;
pub mod highlight;
pub mod history;
pub mod layout;
pub mod model;
pub mod protocol;
pub mod socket;
pub mod theme;

pub use error::Error;
pub use model::{Anchor, Annotation, Document, MarkKind, Shape, TextSpan, ThemeName};
pub use theme::Theme;

pub const APP_NAME: &str = "codedraw";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Logotipo en texto (una celda de ancho mínimo: `╭▶`).
pub const WORDMARK: &str = "╭─ CodeDraw ─▶";
