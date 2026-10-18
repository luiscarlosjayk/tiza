//! Document model: source code + annotations anchored to the text.
//!
//! Anchors live in code coordinates (absolute 1-based line and 0-based
//! grapheme offset), never in screen cells. The conversion to cells happens
//! only when rendering (see `layout`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::error::Error;
use crate::moves::{self, Move, MoveAction, ViewKey, ViewRef};
use crate::redact::RedactFate;

/// Schema version of the `.tiza` file.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct Document {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source: Source,
    /// How the code is stored in the file: embedded (array of lines) or a
    /// reference to a file (`{"file": "src/main.rs", "lines": "12-48"}`).
    #[serde(rename = "code")]
    pub code_src: CodeSource,
    /// Code lines in memory, without the trailing newline. Filled in on load
    /// (see `fs::load_document`) and never serialized directly.
    #[serde(skip)]
    pub code: Vec<String>,
    /// Reusable mark styles defined by the author, referenced by name in `kind`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub styles: BTreeMap<String, MarkStyle>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// Current presentation step. 0 = code only, no annotations;
    /// annotations live at steps ≥ 1.
    #[serde(default = "one")]
    pub step: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeName>,
    /// Blank cells around the drawing: `[vertical, horizontal]`. Saved so each take looks the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<(u16, u16)>,
    /// Code changes between steps (ADR 0007): applied when the presenter
    /// reaches `at_step`, reverted when going back. The embedded `code` is
    /// always the step-0 code.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moves: Vec<Move>,
    /// Step-0 code while the document has moves (`code` is then the
    /// projection at `step`). Empty when there are no moves: `code` is the base.
    #[serde(skip)]
    pub base_code: Vec<String>,
    /// First line of `base_code` while the document has moves (`source.first_line`
    /// is then that of the shown view). Meaningless without moves.
    #[serde(skip_serializing, skip_deserializing, default = "one")]
    pub base_first_line: u32,
    /// Code of the files `code.link` moves name, frozen inside the document
    /// by `:embed` (one entry per file, keyed as the move names it; the
    /// window stays in the move). A file absent here is read from disk next
    /// to the `.tiza` when the document loads.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub views: BTreeMap<String, Vec<String>>,
    /// Code of every linked file in memory (from `views` or from disk),
    /// keyed by the file as the move names it. Never serialized directly.
    #[serde(skip)]
    pub view_code: BTreeMap<String, Vec<String>>,
    /// Redact masks follow their annotation's `step` instead of being active
    /// from step 0 (off by default: a secret is never shown at an earlier step).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redact_from_step: bool,
    /// Line-number gutter (t-112). Off, the code starts at the left edge
    /// (lanes and boxes move with it); the TUI, `render` and the agents'
    /// renders all follow it. `render --no-numbers` / `--numbers` override
    /// it for one export.
    #[serde(default = "yes", skip_serializing_if = "Clone::clone")]
    pub numbers: bool,
    /// Cap on the right margin, in cells (t-113): long notes wrap at it
    /// instead of spreading across a wide terminal. `None`: dynamic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margin: Option<u16>,
    /// Per-token theme overrides (t-119), the same keys as the `[theme]`
    /// table of `~/.config/tiza/config.toml`; they apply on top of the
    /// config file's and under `:theme`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_overrides: Option<crate::theme::ThemeOverrides>,
    /// Named steps (ADR 0007 §4, t-099): a title, an id for `step.goto` and
    /// an optional `auto_advance_ms`, bound to a step by its explicit
    /// `step` number (never by the position in the list, so moving an
    /// annotation to another step shifts nothing). Sparse; saved sorted by
    /// `step`. Automation for recordings and booth loops, not speaker tools.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "crate::steps::serialize_sorted"
    )]
    pub steps: Vec<crate::steps::NamedStep>,
    /// `n` (and an `auto_advance_ms`) past the last step goes back to step 0.
    #[serde(default, rename = "loop", skip_serializing_if = "std::ops::Not::not")]
    pub looping: bool,
}

fn yes() -> bool {
    true
}

/// Where the code of a `.tiza` comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(untagged)]
pub enum CodeSource {
    /// Copy of the code inside the document: portable, but frozen.
    Embedded(Vec<String>),
    /// Reference to an existing file. `file` may be absolute or relative to the
    /// `.tiza` directory; `lines` is an optional range `"12-48"` or `"7"`.
    File {
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lines: Option<String>,
    },
}

impl Default for CodeSource {
    fn default() -> Self {
        CodeSource::Embedded(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, schemars::JsonSchema)]
pub struct Source {
    /// Path of the original file, if there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Language identifier (extension or name): `rs`, `ts`, `py`…
    #[serde(default)]
    pub language: String,
    /// Number of the first displayed line (allows opening `main.rs:12-48`).
    #[serde(default = "one")]
    pub first_line: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    Dark,
    Light,
    /// The host terminal's own palette (t-119): default background and
    /// foreground, roles on the 16 ANSI colours, syntax on the terminal's
    /// colours. Only meaningful in the TUI; exports fall back to dark/light.
    Terminal,
}

/// How a line is drawn: `─│` (solid), `╌╎` (dashed), `┄┆` (dotted), `━┃` (thick), `═║` (double).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Stroke {
    #[default]
    Solid,
    Dashed,
    Dotted,
    Thick,
    Double,
}

/// Corner shape of a routed line: `╭╰` (rounded, the cell-grid "curve") or `┌└` (sharp).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    #[default]
    Rounded,
    Sharp,
}

/// What is drawn at an end of a line.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Head {
    #[default]
    None,
    Arrow,
    Dot,
    Bar,
    Open,
}

/// Heads at both ends. Default: nothing at the start, `▶` at the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Heads {
    #[serde(default)]
    pub start: Head,
    #[serde(default = "Head::arrow")]
    pub end: Head,
}

impl Default for Heads {
    fn default() -> Self {
        Self {
            start: Head::None,
            end: Head::Arrow,
        }
    }
}

impl Head {
    fn arrow() -> Self {
        Head::Arrow
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "none" | "no" | "-" => Head::None,
            "arrow" | "triangle" => Head::Arrow,
            "dot" | "circle" => Head::Dot,
            "bar" | "flat" => Head::Bar,
            "open" | "chevron" => Head::Open,
            _ => return None,
        })
    }
    pub const NAMES: &[&str] = &["none", "arrow", "dot", "bar", "open"];
}

impl Stroke {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "solid" | "line" => Stroke::Solid,
            "dashed" | "dash" => Stroke::Dashed,
            "dotted" | "dot" | "dots" => Stroke::Dotted,
            "thick" | "bold" | "heavy" => Stroke::Thick,
            "double" => Stroke::Double,
            _ => return None,
        })
    }
    pub const NAMES: &[&str] = &["solid", "dashed", "dotted", "thick", "double"];
}

impl Route {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "rounded" | "round" | "curved" | "curve" => Route::Rounded,
            "sharp" | "square" | "elbow" => Route::Sharp,
            _ => return None,
        })
    }
    pub const NAMES: &[&str] = &["rounded", "sharp"];
}

/// Border of a box or callout: stroke, corners and whether the inside is
/// tinted. `fill` is `None` for the shape's default (boxes tint, callouts do not).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct BorderStyle {
    #[serde(default, skip_serializing_if = "is_default")]
    pub border: Stroke,
    #[serde(default, skip_serializing_if = "is_default")]
    pub corners: Route,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<bool>,
}

impl BorderStyle {
    /// Parses a `fill=` value: yes/tint/true or no/none/false.
    pub fn parse_fill(s: &str) -> Option<bool> {
        match s.trim().to_ascii_lowercase().as_str() {
            "yes" | "tint" | "true" | "on" => Some(true),
            "no" | "none" | "false" | "off" => Some(false),
            _ => None,
        }
    }
}

/// Stroke, corners and heads of a line-like annotation. Every field defaults
/// to today's look, so documents without them are unchanged.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct LineStyle {
    #[serde(default, skip_serializing_if = "is_default")]
    pub stroke: Stroke,
    #[serde(default, skip_serializing_if = "is_default")]
    pub route: Route,
    #[serde(default, skip_serializing_if = "is_default")]
    pub heads: Heads,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

impl Heads {
    /// Parses `END`, `START,END` or `START END`.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split([',', ' ', '/']).filter(|t| !t.is_empty()).collect();
        match parts.as_slice() {
            [end] => Some(Heads {
                start: Head::None,
                end: Head::parse(end)?,
            }),
            [start, end] => Some(Heads {
                start: Head::parse(start)?,
                end: Head::parse(end)?,
            }),
            _ => None,
        }
    }
}

/// A point in the code: absolute line (1-based) and grapheme (0-based).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
pub struct Anchor {
    pub line: u32,
    pub col: u32,
}

/// A range within ONE line, in graphemes, exclusive end.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, schemars::JsonSchema)]
pub struct TextSpan {
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

/// Semantic marks for explaining Rust (each with a fixed color, glyph and legend).
/// Visual style of a mark: a glyph, a short legend and a color name. Styles come
/// from three places, in priority order: an inline `style` on the annotation, the
/// document's reusable `styles` map, and the built-in kinds (see [`BUILTIN_KINDS`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema)]
pub struct MarkStyle {
    /// One or two cells wide (box-drawing, arrows, emoji…). Measured, never assumed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    /// Text shown after the glyph; the annotation's `label` is appended after `: `.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legend: Option<String>,
    /// Palette name (`red`, `cyan`, …) or `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl MarkStyle {
    pub fn new(glyph: &str, legend: &str, color: &str) -> Self {
        Self {
            glyph: Some(glyph.into()),
            legend: Some(legend.into()),
            color: Some(color.into()),
        }
    }

    /// `over` wins field by field; missing fields fall back to `self`.
    pub fn merged(&self, over: &MarkStyle) -> MarkStyle {
        MarkStyle {
            glyph: over.glyph.clone().or_else(|| self.glyph.clone()),
            legend: over.legend.clone().or_else(|| self.legend.clone()),
            color: over.color.clone().or_else(|| self.color.clone()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.glyph.is_none() && self.legend.is_none() && self.color.is_none()
    }
}

/// Built-in, language-agnostic kinds: (name, glyph, legend, color).
/// Chosen to be useful in any language; glyphs are one or two cells wide and
/// never emoji (ambiguous width across terminals).
pub const BUILTIN_KINDS: &[(&str, &str, &str, &str)] = &[
    ("info", "i", "info", "blue"),
    ("note", "·", "note", "gray"),
    ("warn", "!", "warning", "yellow"),
    ("error", "✗", "error", "red"),
    ("ok", "✓", "ok", "green"),
    ("question", "?", "why?", "purple"),
    ("important", "★", "important", "orange"),
    ("read", "╌▶", "read", "cyan"),
    ("write", "═▶", "write", "yellow"),
    ("move", "━▶", "move", "pink"),
    ("new", "+", "new", "green"),
    ("drop", "╳", "drop", "red"),
    ("scope", "┆", "scope", "purple"),
    ("flow", "→", "flow", "blue"),
    ("todo", "□", "todo", "gray"),
];

/// Aliases so older documents and language-specific habits keep working.
pub const KIND_ALIASES: &[(&str, &str)] = &[
    ("borrow", "read"),
    ("ref", "read"),
    ("&", "read"),
    ("borrow_mut", "write"),
    ("borrowmut", "write"),
    ("mut", "write"),
    ("&mut", "write"),
    ("lifetime", "scope"),
    ("life", "scope"),
    ("'a", "scope"),
    ("free", "drop"),
    ("delete", "drop"),
    ("alloc", "new"),
    ("create", "new"),
    ("bug", "error"),
    ("warning", "warn"),
    ("tip", "info"),
];

/// Canonical name of a built-in kind, resolving aliases. `None` if unknown.
pub fn builtin_kind(name: &str) -> Option<&'static str> {
    let n = name.trim().to_ascii_lowercase();
    if let Some((k, ..)) = BUILTIN_KINDS.iter().find(|(k, ..)| *k == n) {
        return Some(k);
    }
    KIND_ALIASES.iter().find(|(a, _)| *a == n).map(|(_, k)| *k)
}

pub fn builtin_style(name: &str) -> Option<MarkStyle> {
    let k = builtin_kind(name)?;
    BUILTIN_KINDS
        .iter()
        .find(|(n, ..)| *n == k)
        .map(|(_, g, l, c)| MarkStyle::new(g, l, c))
}

/// How a highlighted or marked span is drawn (t-096).
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Emphasis {
    /// Background tint (the default).
    #[default]
    Fill,
    /// A row under the line with ▔ under the span.
    Underline,
    /// ⌈ ⌉ in the cells beside the span (a glued side goes to the row below).
    Brackets,
    /// A long stroke through every character.
    Strike,
}

impl Emphasis {
    pub const NAMES: [&str; 4] = ["fill", "underline", "brackets", "strike"];

    pub fn parse(s: &str) -> Option<Emphasis> {
        match s.trim().to_ascii_lowercase().as_str() {
            "fill" | "tint" | "bg" => Some(Emphasis::Fill),
            "underline" | "under" => Some(Emphasis::Underline),
            "brackets" | "bracket" | "outline" => Some(Emphasis::Brackets),
            "strike" | "strikethrough" | "del" => Some(Emphasis::Strike),
            _ => None,
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Emphasis::Fill
    }

    pub fn name(&self) -> &'static str {
        Self::NAMES[*self as usize]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    /// Highlights a text range.
    Highlight {
        span: TextSpan,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Emphasis::is_default")]
        emphasis: Emphasis,
    },
    /// Highlight with meaning. `kind` names a built-in kind (`read`, `warn`, `move`…),
    /// an alias, or a reusable style from `Document::styles`; `style` overrides any
    /// field inline for this one annotation.
    Mark {
        kind: String,
        span: TextSpan,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "MarkStyle::is_empty")]
        style: MarkStyle,
        #[serde(default, skip_serializing_if = "Emphasis::is_default")]
        emphasis: Emphasis,
    },
    /// Box around a range of lines.
    Box {
        from_line: u32,
        to_line: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// Border stroke, corners and fill (all optional; defaults draw `╭─╮` with a tint).
        #[serde(flatten, default)]
        frame: BorderStyle,
    },
    /// Arrow from one point of the code to another.
    Arrow {
        from: Anchor,
        to: Anchor,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// Stroke, corners and heads (all optional; defaults draw `╭──▶`).
        #[serde(flatten, default)]
        line: LineStyle,
    },
    /// Text note anchored to a line.
    Note { line: u32, text: String },
    /// Bordered multi-line text block explaining a range of lines. Laid out as a
    /// block (reserving its rows), not as a per-line label, so a paragraph does
    /// not push every other label down.
    Callout {
        from_line: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_line: Option<u32>,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Placement::is_default")]
        placement: Placement,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<usize>,
        /// Border stroke, corners and fill (defaults: `╭─╮`, no fill).
        #[serde(flatten, default)]
        frame: BorderStyle,
    },
    /// Masks a span of code (a token, a password, a URL with credentials):
    /// on every surface the span shows as eight `•` plus the last `keep`
    /// characters, never the real text or its length (t-117). Active from
    /// step 0 whatever its `step`, unless `Document::redact_from_step`.
    /// Only the columns are stored: never the text under them.
    Redact {
        span: TextSpan,
        /// Characters left visible at the end of the span (0..=8).
        #[serde(default, skip_serializing_if = "is_zero_u8")]
        keep: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// Set by a reload or paste that could not follow the redact's text
        /// (fail-closed, t-117): the mask was widened to the rest of its
        /// line, or to whole lines, and keeps no tail. Cleared when the
        /// presenter marks the value again (a new redact over it).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        widened: bool,
        /// On a document with moves, which code the span's columns refer to
        /// (t-154), set by Tiza when it pins the redact: `0` the untouched
        /// file, `N` the code right after move #N (1-based), the move that
        /// wrote its line. Absent: the code at the end of the annotation's
        /// `step` (ADR 0007 §3d; `step: 0` is the untouched file). Moves
        /// added or deleted later keep it on that same text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pin: Option<u32>,
    },
    /// Directs attention: while active (from `step` to `until`, or only at
    /// `step`), dims the code outside `lines` and the annotations outside `ids`.
    /// With only `ids`, the focused code is that of those annotations.
    Spotlight {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_line: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_line: Option<u32>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ids: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        until: Option<u32>,
    },
}

/// Where a callout is drawn.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    /// In the right margin, aligned with `from_line` (default).
    #[default]
    Margin,
    /// As its own rows between `to_line` and the next code line.
    Below,
    /// Margin when the terminal has room, below otherwise.
    Auto,
}

impl Placement {
    pub fn is_default(&self) -> bool {
        *self == Placement::Margin
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s
            .trim()
            .trim_start_matches('@')
            .to_ascii_lowercase()
            .as_str()
        {
            "margin" | "right" => Some(Self::Margin),
            "below" | "under" => Some(Self::Below),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

impl Shape {
    /// The JSON `type` name of this shape.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Shape::Highlight { .. } => "highlight",
            Shape::Mark { .. } => "mark",
            Shape::Box { .. } => "box",
            Shape::Arrow { .. } => "arrow",
            Shape::Note { .. } => "note",
            Shape::Callout { .. } => "callout",
            Shape::Spotlight { .. } => "spotlight",
            Shape::Redact { .. } => "redact",
        }
    }

    /// Line ranges the shape occupies (to focus the code of an annotation).
    /// An arrow occupies only its two endpoints, not the stretch between them.
    pub fn line_ranges(&self) -> Vec<(u32, u32)> {
        match self {
            Shape::Highlight { span, .. }
            | Shape::Mark { span, .. }
            | Shape::Redact { span, .. } => {
                vec![(span.line, span.line)]
            }
            Shape::Box {
                from_line, to_line, ..
            } => vec![(*from_line, *to_line)],
            Shape::Arrow { from, to, .. } => vec![(from.line, from.line), (to.line, to.line)],
            Shape::Note { line, .. } => vec![(*line, *line)],
            Shape::Callout {
                from_line, to_line, ..
            } => vec![(*from_line, to_line.unwrap_or(*from_line))],
            Shape::Spotlight {
                from_line, to_line, ..
            } => from_line
                .map(|f| vec![(f, to_line.unwrap_or(f))])
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct Annotation {
    /// Short, stable public id (`a1`, `a2`…), chosen by the client or assigned.
    pub id: String,
    /// Step at which it appears (1-based). A redact may say 0 on a document
    /// with moves: it masks from step 0 anyway, and its step only says which
    /// code its columns refer to (0: the untouched file, t-140).
    #[serde(default = "one")]
    pub step: u32,
    /// Color chosen by the presenter: a palette name (`blue`, `verde`…) or `#rrggbb`.
    /// Without a color, arrows rotate through a palette and the rest use the theme color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Range shapes (box, callout, spotlight) created from two text markers
    /// (`find_start` / `find_end`, t-097): kept so the range can be
    /// re-resolved after the code changes (t-098). Lines stay the truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markers: Option<RangeMarkers>,
    /// The file (view) the annotation belongs to, as a `code.link` names it
    /// (t-105). Absent: the document's main file. Filled in from the view
    /// shown at the annotation's step when it is added without one. Lines
    /// are always real file line numbers; the annotation is drawn only
    /// while its file is shown and its lines fall inside the window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(flatten)]
    pub shape: Shape,
}

/// Two texts that delimit a line range: the first occurrence of `start`
/// anywhere, then the first occurrence of `end` at or after it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct RangeMarkers {
    pub start: String,
    pub end: String,
}

/// Control characters (tabs, newlines, escapes) become spaces: labels are one line.
pub fn single_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Applies [`single_line`] to the texts of a shape (callout bodies keep newlines).
pub fn sanitize_texts(shape: &mut Shape) {
    match shape {
        Shape::Highlight { label, .. }
        | Shape::Mark { label, .. }
        | Shape::Box { label, .. }
        | Shape::Arrow { label, .. }
        | Shape::Redact { label, .. } => {
            if let Some(l) = label {
                *l = single_line(l);
            }
        }
        Shape::Note { text, .. } => *text = single_line(text),
        Shape::Callout { title, text, .. } => {
            if let Some(t) = title {
                *t = single_line(t);
            }
            *text = text
                .chars()
                .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
                .collect();
        }
        Shape::Spotlight { .. } => {}
    }
}

fn one() -> u32 {
    1
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

/// A document projected to one step (see `Document::view_at`).
#[derive(Debug, Clone)]
pub struct View {
    /// The shown file cropped to its window, with the annotations of that
    /// file (the others are left out) shifted through its patches.
    pub doc: Document,
    /// Annotations of steps `<= step` hidden because no patch mapping survives.
    pub unanchored: Vec<String>,
    /// Which file and window is shown.
    pub view: ViewRef,
    /// Cursor and selection the moves left at this step (t-105).
    pub cursor: Option<Anchor>,
    pub selection: Option<(Anchor, Anchor)>,
}

impl Document {
    pub fn from_code(code: &str, language: impl Into<String>, path: Option<String>) -> Self {
        let code = code.strip_prefix('\u{feff}').unwrap_or(code);
        let mut lines: Vec<String> = code.lines().map(|l| l.to_string()).collect();
        if lines.is_empty() {
            // An empty file is still a document with one (empty) line: every
            // layout assumes at least one row.
            lines.push(String::new());
        }
        Self {
            schema: SCHEMA_VERSION,
            title: None,
            source: Source {
                path,
                language: language.into(),
                first_line: 1,
            },
            code_src: CodeSource::Embedded(lines.clone()),
            code: lines,
            styles: BTreeMap::new(),
            annotations: Vec::new(),
            step: 1,
            theme: None,
            padding: None,
            moves: Vec::new(),
            base_code: Vec::new(),
            base_first_line: 1,
            views: BTreeMap::new(),
            view_code: BTreeMap::new(),
            redact_from_step: false,
            numbers: true,
            margin: None,
            theme_overrides: None,
            steps: Vec::new(),
            looping: false,
        }
    }

    /// Switches the source to a file reference (the code in memory does not change).
    pub fn reference_file(&mut self, file: impl Into<String>, lines: Option<(u32, u32)>) {
        self.code_src = CodeSource::File {
            file: file.into(),
            lines: lines.map(|(a, b)| {
                if a == b {
                    a.to_string()
                } else {
                    format!("{a}-{b}")
                }
            }),
        };
    }

    /// Freezes the code inside the document (no longer depends on the file).
    /// Freezes the code inside the document. With redacts, what is frozen is
    /// the masked code: a `.tiza` never carries the text under a mask.
    pub fn embed_code(&mut self) {
        self.code_src = CodeSource::Embedded(crate::redact::masked_base(self));
        // Every linked file too (masked with its own redacts), so the demo
        // stays one portable file.
        for file in self.link_files() {
            if let Some(code) = self.view_code.get(&file) {
                let masked = crate::redact::mask_lines(
                    code,
                    1,
                    &crate::redact::base_redacts(self, &Some(file.clone())),
                );
                self.views.insert(file, masked);
            }
        }
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self.code_src, CodeSource::Embedded(_))
    }

    /// Keeps `code_src` (and `source.first_line`) consistent with the
    /// step-0 code of the main file before serializing: what is saved is
    /// never the projection of a step.
    pub fn sync_code_src(&mut self) {
        if matches!(self.code_src, CodeSource::Embedded(_)) {
            self.code_src = CodeSource::Embedded(self.base().to_vec());
        }
        if !self.moves.is_empty() {
            self.source.first_line = self.base_first_line;
        }
    }

    /// A copy ready for the wire (`scene.get`): the main code and every
    /// linked file embedded and masked, the moves' content masked, the
    /// first line that of the main file.
    pub fn embedded_copy(&self) -> Document {
        let mut d = self.clone();
        d.sync_code_src();
        d.embed_code();
        // Move content masked where it lands under a redact (t-140).
        d.moves = moves::masked_moves(self);
        d
    }

    // ---- moves (ADR 0007, t-098) and views (t-105) ----

    /// The step-0 code of the main file.
    pub fn base(&self) -> &[String] {
        if self.moves.is_empty() {
            &self.code
        } else {
            &self.base_code
        }
    }

    /// First line of the main file's step-0 code.
    pub fn base_first(&self) -> u32 {
        if self.moves.is_empty() {
            self.source.first_line
        } else {
            self.base_first_line
        }
    }

    /// Replaces the step-0 code (paste, follow, `:lang`). With moves applied
    /// the projection is recomputed.
    pub fn set_base_code(&mut self, lines: Vec<String>) {
        if self.moves.is_empty() {
            self.code = lines;
        } else {
            self.base_code = lines;
            self.sync_code_to_step();
        }
    }

    /// `file` as a `code.link` or an annotation names it, normalized:
    /// trimmed, without a leading `./`.
    pub fn normalize_file(file: &str) -> String {
        let mut f = file.trim();
        while let Some(rest) = f.strip_prefix("./") {
            f = rest;
        }
        f.to_string()
    }

    /// The view a file name points at: `None` when it names the main file
    /// (as `code.file` or `source.path` name it), else the normalized name.
    pub fn view_key(&self, file: &str) -> ViewKey {
        let f = Self::normalize_file(file);
        let main_names: Vec<String> = [
            match &self.code_src {
                CodeSource::File { file, .. } => Some(file.clone()),
                CodeSource::Embedded(_) => None,
            },
            self.source.path.clone(),
            self.source.path.as_ref().and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            }),
        ]
        .into_iter()
        .flatten()
        .map(|n| Self::normalize_file(&n))
        .collect();
        // Equal to a name of the main file, or a path suffix of its source
        // path (`src/stack.rs` for `/home/x/demo/src/stack.rs`), so an
        // embedded document moved elsewhere still knows which link is the
        // main file.
        let is_main = main_names
            .iter()
            .any(|n| *n == f || (n.contains('/') && n.ends_with(&format!("/{f}"))));
        if is_main { None } else { Some(f) }
    }

    /// First line of a view's code: the main file keeps its range, a linked
    /// file is read whole.
    pub fn view_first_line(&self, key: &ViewKey) -> u32 {
        match key {
            None => self.base_first(),
            Some(_) => 1,
        }
    }

    /// Step-0 code of a view (`None` when a linked file is not loaded).
    /// `masked`: with the redact masks of that view's annotations applied.
    pub fn view_base_code(&self, key: &ViewKey, masked: bool) -> Option<Vec<String>> {
        let code: &[String] = match key {
            None => self.base(),
            Some(f) => self.view_code.get(f)?,
        };
        Some(if masked {
            crate::redact::mask_lines(
                code,
                self.view_first_line(key),
                &crate::redact::base_redacts(self, key),
            )
        } else {
            code.to_vec()
        })
    }

    /// The annotations that belong to a view.
    pub fn annotations_of(&self, key: &ViewKey) -> Vec<Annotation> {
        self.annotations
            .iter()
            .filter(|a| &a.file == key)
            .cloned()
            .collect()
    }

    /// Files named by `code.link` moves other than the main file, in order
    /// of first appearance, without repeats.
    pub fn link_files(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in &self.moves {
            if let MoveAction::CodeLink(l) = &m.action
                && let Some(f) = self.view_key(&l.file)
                && !out.contains(&f)
            {
                out.push(f);
            }
        }
        out
    }

    /// The view and window shown at `step`: the last `code.link` with
    /// `at_step <= step`, or the main file. Cheap (no moves applied).
    pub fn shown_view_at(&self, step: u32) -> ViewRef {
        let mut links: Vec<(usize, &Move)> = self
            .moves
            .iter()
            .enumerate()
            .filter(|(_, m)| m.at_step <= step && matches!(m.action, MoveAction::CodeLink(_)))
            .collect();
        links.sort_by_key(|(i, m)| (m.at_step, *i));
        match links.last() {
            Some((_, m)) => {
                let MoveAction::CodeLink(l) = &m.action else {
                    unreachable!()
                };
                ViewRef {
                    file: self.view_key(&l.file),
                    window: l.window().unwrap_or(None),
                }
            }
            None => ViewRef::default(),
        }
    }

    /// Total time of the animation that runs at `step`, in milliseconds:
    /// the typing of its moves plus its waits (0 when nothing animates).
    pub fn typing_ms_at(&self, step: u32) -> u64 {
        self.moves
            .iter()
            .filter(|m| m.at_step == step)
            .map(|m| match &m.action {
                MoveAction::CodePatch(p) => p.typing_ms(),
                MoveAction::Wait(w) => w.ms,
                _ => 0,
            })
            .sum()
    }

    /// Milliseconds the `wait` moves of `step` add up to.
    pub fn wait_ms_at(&self, step: u32) -> u64 {
        self.moves
            .iter()
            .filter(|m| m.at_step == step)
            .map(|m| match &m.action {
                MoveAction::Wait(w) => w.ms,
                _ => 0,
            })
            .sum()
    }

    /// Whether any move is applied at the current step (`at_step <= step`).
    pub fn moves_applied(&self) -> bool {
        self.moves.iter().any(|m| m.at_step <= self.step)
    }

    /// The code shown at `step` and its first line: the view a `code.link`
    /// left up, cropped to its window, with every patch `<= step` applied.
    pub fn code_at(&self, step: u32) -> Result<(Vec<String>, u32), Error> {
        if self.moves.is_empty() {
            return Ok((self.code.clone(), self.source.first_line));
        }
        let p = moves::project(self, &self.moves, step, false)?;
        Ok(p.shown_code(self))
    }

    /// The full code of one view at `step` (its step-0 code with its
    /// patches `<= step`), and its first line. Annotations of that view are
    /// validated against it, whatever window is shown.
    pub fn view_code_at(&self, key: &ViewKey, step: u32) -> Result<(Vec<String>, u32), Error> {
        if self.moves.is_empty() {
            return Ok((self.code.clone(), self.source.first_line));
        }
        let p = moves::project(self, &self.moves, step, false)?;
        let first = self.view_first_line(key);
        match p.codes.get(key) {
            Some(c) => Ok((c.clone(), first)),
            None => self
                .view_base_code(key, false)
                .map(|c| (c, first))
                .ok_or_else(|| {
                    Error::NotFound(format!(
                        "file «{}» is not a view of this document",
                        key.as_deref().unwrap_or("?")
                    ))
                }),
        }
    }

    /// Makes `code` the projection at `step` (a pure function of the base:
    /// going back recomputes, no diff of diffs). A document without moves
    /// is untouched. Errors (an invalid move) leave the base code shown.
    pub fn sync_code_to_step(&mut self) -> Option<Error> {
        if self.moves.is_empty() {
            return None;
        }
        if self.base_code.is_empty() {
            // First call after load / first move added: `code` was the base.
            self.base_code = self.code.clone();
            self.base_first_line = self.source.first_line;
        }
        match self.code_at(self.step) {
            Ok((c, first)) => {
                self.code = c;
                self.source.first_line = first;
                None
            }
            Err(e) => {
                self.code = self.base_code.clone();
                self.source.first_line = self.base_first_line;
                Some(e)
            }
        }
    }

    /// Validates every move against the code at its step (ADR 0007 §3) and
    /// the `rebind` ids against the annotations.
    pub fn validate_moves(&self) -> Result<(), Error> {
        moves::validate(self, &self.moves, &self.annotations)
    }

    /// Adds a move (validated against the code at its step). Returns its
    /// index. A `code.link` needs its file loaded first (`fs::load_view`
    /// or an entry in `views`).
    pub fn add_move(&mut self, m: Move) -> Result<usize, Error> {
        self.add_move_noting(m).map(|(i, _)| i)
    }

    /// [`Document::add_move`] plus its notices (t-157): every `code.patch`
    /// that the new move retargets (it now hits other lines, because the
    /// new move shifted the code its `position` counts on), named without
    /// quoting masked text.
    pub fn add_move_noting(&mut self, m: Move) -> Result<(usize, Vec<String>), Error> {
        // Every redact is pinned to the code its columns refer to before
        // the new move changes what "the code at its step" is (t-140,
        // t-154), so the move is followed through, never read in its old
        // columns.
        let mut framed = self.clone();
        if framed.moves.is_empty() {
            framed.base_code = framed.code.clone();
            framed.base_first_line = framed.source.first_line;
        }
        framed.pin_redacts_with(&self.moves, None);
        // A target that looks like a secret is refused before anything
        // echoes it (t-154).
        framed.check_move_targets(&m, framed.moves.len())?;
        let mut trial = framed.moves.clone();
        trial.push(m.clone());
        let mut with = framed.clone();
        with.moves = trial.clone();
        // A redact pinned to a move whose lines a move at an earlier step
        // shifts keeps its offset in that move's lines (t-154).
        with.repin_redacts(&framed, Some);
        // The move's targets are matched with the masks drawn; a «not
        // found» does not repeat them.
        moves::validate(&with, &trial, &with.annotations).map_err(|e| with.scrub_targets(&m, e))?;
        with.check_move_content(trial.len() - 1)?;
        let notices = moves::retarget_notices(&framed, &with, Some);
        *self = with;
        self.sync_code_to_step();
        Ok((self.moves.len() - 1, notices))
    }

    /// Pins every redact Tiza can pin without losing a reading (t-140,
    /// t-154): see [`Document::pin_redacts_with`].
    pub fn reframe_redacts(&mut self) {
        let moves = self.moves.clone();
        self.pin_redacts_with(&moves, None);
    }

    /// Pins the redacts to the code their columns refer to, on `moves`
    /// (t-140, t-154), so later moves — and moves added later at earlier
    /// steps — follow them from there and never reinterpret their columns.
    /// A redact on a line of the file goes to the file (`step: 0`; with
    /// `redact_from_step`, `pin: 0` and its step stays its activation step).
    /// A redact on a line a move wrote goes to that move (`pin: N`, its
    /// step that move's) when Tiza itself marked it (`own`, the redact just
    /// added) or with `redact_from_step`; one Tiza did not pin (a file
    /// written by hand, by an agent, or saved before t-154) keeps its
    /// step's reading — moved to the earliest step that holds its line —
    /// so the projection still masks both readings.
    fn pin_redacts_with(&mut self, moves: &[Move], own: Option<&str>) {
        if !crate::redact::has_redacts(self) {
            return;
        }
        let from_step = self.redact_from_step;
        let mut d = self.clone();
        d.moves = moves.to_vec();
        for a in &mut self.annotations {
            let mine = own == Some(a.id.as_str());
            let Shape::Redact { span, pin, .. } = &mut a.shape else {
                continue;
            };
            if pin.is_some() || (a.step == 0 && !from_step) {
                continue;
            }
            if moves.is_empty() {
                // Without moves every redact is on the untouched file.
                if from_step {
                    *pin = Some(0);
                } else {
                    a.step = 0;
                }
                continue;
            }
            let o = moves::line_origin(&d, &a.file, a.step, span.line);
            match o.writer {
                None if o.frame == 0 => {
                    span.line = o.frame_line;
                    if from_step {
                        *pin = Some(0);
                    } else {
                        a.step = 0;
                    }
                }
                None => {}
                Some((index, line)) if mine || from_step => {
                    span.line = line;
                    *pin = Some(index as u32 + 1);
                    if !from_step {
                        a.step = moves[index].at_step;
                    }
                }
                Some(_) if o.frame < a.step => {
                    a.step = o.frame;
                    span.line = o.frame_line;
                }
                Some(_) => {}
            }
        }
    }

    /// After the moves changed (`old`: the document before, `new_index`:
    /// where each old move is now, `None` when it was deleted), every
    /// redact pinned to a move keeps its place in that move's lines
    /// (t-154). One whose move was deleted is dropped — its line no
    /// longer exists at any step — and named in the notices returned.
    fn repin_redacts(
        &mut self,
        old: &Document,
        new_index: impl Fn(usize) -> Option<usize>,
    ) -> Vec<String> {
        let mut notices = Vec::new();
        if !crate::redact::has_redacts(self) {
            return notices;
        }
        let snapshot = self.clone();
        let mut gone: Vec<String> = Vec::new();
        for a in &mut self.annotations {
            let Shape::Redact {
                span, pin, widened, ..
            } = &mut a.shape
            else {
                continue;
            };
            let Some(n) = pin.filter(|n| *n >= 1) else {
                continue;
            };
            let i = n as usize - 1;
            let Some(j) = new_index(i) else {
                notices.push(format!(
                    "redact {} dropped: move #{n}, which wrote its line, was deleted",
                    a.id
                ));
                gone.push(a.id.clone());
                continue;
            };
            *pin = Some(j as u32 + 1);
            if let Some(m) = snapshot.moves.get(j)
                && !snapshot.redact_from_step
            {
                a.step = m.at_step;
            }
            match moves::repin_span(old, i, &snapshot, j, *span) {
                Some(RedactFate::Followed(sp)) => *span = sp,
                Some(RedactFate::Widened(v)) if !v.is_empty() => {
                    *span = v[0];
                    *widened = true;
                }
                // Nothing on that line to hide any more, or nothing to say
                // where: kept as it is (masked at its stored columns).
                _ => {}
            }
        }
        self.annotations.retain(|a| !gone.contains(&a.id));
        notices
    }

    /// The target texts of a move (`find`, `after`, `before`,
    /// `find_start`, `find_end`, a `select` or `rebind` `find`), with the
    /// field each comes from.
    fn move_targets(m: &Move) -> Vec<(&'static str, String)> {
        let mut targets: Vec<(&'static str, String)> = Vec::new();
        let mut put = |what: &'static str, t: &Option<String>| {
            if let Some(t) = t {
                targets.push((what, t.clone()));
            }
        };
        match &m.action {
            MoveAction::CodePatch(p) => {
                put("find", &p.find);
                put("after", &p.after);
                put("before", &p.before);
                put("find_start", &p.find_start);
                put("find_end", &p.find_end);
            }
            MoveAction::Select(sel) => {
                put("from.find", &sel.from.find);
                put("to.find", &sel.to.find);
            }
            _ => {}
        }
        for spec in m.rebind.values() {
            for w in [&spec.at, &spec.from, &spec.to].into_iter().flatten() {
                put("rebind find", &w.find);
            }
        }
        targets
    }

    /// On a document with redacts, `move.add` refuses a target that looks
    /// like a secret, like a redact anchor does (t-154): the moves would
    /// save it. The error never names the text. A target that is (part
    /// of) a value under a mask is not refused here — it is matched on the
    /// masked code and simply not found, exactly like text that is not
    /// there, so the answer is no oracle (t-117) — and
    /// [`Document::scrub_targets`] keeps that answer from echoing it.
    fn check_move_targets(&self, m: &Move, index: usize) -> Result<(), Error> {
        if !crate::redact::has_redacts(self) {
            return Ok(());
        }
        for (what, t) in Self::move_targets(m) {
            if crate::redact::looks_like_secret(&t) {
                return Err(Error::Invalid(format!(
                    "move #{} (step {}): its `{what}` text looks like a secret, and the moves would save it — target the line by `position`, or by the text before the secret",
                    index + 1,
                    m.at_step
                )));
            }
        }
        Ok(())
    }

    /// An error about move `m` with its target texts taken out, on a
    /// document with redacts (t-154): a «not found» for a target that is
    /// under a mask must not hand the text back (and every target is
    /// scrubbed alike, so the answer says nothing about the mask).
    fn scrub_targets(&self, m: &Move, e: Error) -> Error {
        if !crate::redact::has_redacts(self) {
            return e;
        }
        let targets = Self::move_targets(m);
        let scrub = |s: String| -> String {
            let mut s = s;
            for (_, t) in &targets {
                if !t.is_empty() {
                    s = s.replace(&format!("«{t}»"), "«…»");
                }
            }
            s
        };
        match e {
            Error::TextNotFound(s) => Error::TextNotFound(scrub(s)),
            Error::Invalid(s) => Error::Invalid(scrub(s)),
            other => other,
        }
    }

    /// `move.add` refuses a `code.patch` whose content repeats a value that
    /// is under a redact at that point, unless every copy lands under a
    /// mask after the patch (t-140, AC6). The error never names the value.
    fn check_move_content(&self, index: usize) -> Result<(), Error> {
        let m = &self.moves[index];
        let MoveAction::CodePatch(_) = &m.action else {
            return Ok(());
        };
        if !crate::redact::has_redacts(self) {
            return Ok(());
        }
        let Ok(p) = moves::project(self, &self.moves, m.at_step, false) else {
            return Ok(());
        };
        let Some(ap) = p.patches.iter().find(|ap| ap.index == index) else {
            return Ok(());
        };
        let copied: Vec<String> = p
            .redacts
            .iter()
            .zip(&self.annotations)
            .filter_map(|(r, a)| {
                let r = r.as_ref()?;
                (a.file == ap.file && r.copied_by.contains(&index)).then(|| a.id.clone())
            })
            .collect();
        if copied.is_empty() {
            return Ok(());
        }
        Err(Error::Invalid(format!(
            "move #{} (step {}): its content repeats the value under redact {} where no mask covers it; it would show on screen — write the value in place of the masked one, or leave it out",
            index + 1,
            m.at_step,
            copied.join(", ")
        )))
    }

    /// Redacts a move widened or dropped at `step` (t-140): one notice
    /// each, to say loudly when the presenter reaches that step (and on
    /// `render --step N`). `cumulative`: every widened or dropped redact
    /// at `step`, not only those of the moves of that step.
    pub fn redact_notices_at(&self, step: u32, cumulative: bool) -> Vec<String> {
        if self.moves.is_empty() || !crate::redact::has_redacts(self) {
            return Vec::new();
        }
        let Ok(p) = moves::project(self, &self.moves, step, false) else {
            return Vec::new();
        };
        let shown = p.shown.file.clone();
        let mut out = Vec::new();
        for (a, r) in self.annotations.iter().zip(&p.redacts) {
            let Some(r) = r else { continue };
            let Some(by) = r.by_move else { continue };
            if a.file != shown || (!cumulative && self.moves[by].at_step != step) {
                continue;
            }
            if r.dropped {
                out.push(format!(
                    "redact {} dropped: move #{} deleted its line",
                    a.id,
                    by + 1
                ));
                continue;
            }
            let mut lines: Vec<u32> = r.all().map(|s| s.line).collect();
            lines.sort_unstable();
            let place = match (lines.first(), lines.last()) {
                (Some(a), Some(b)) if a == b => format!("the rest of L{a}"),
                (Some(a), Some(b)) => format!("L{a}-{b}"),
                _ => continue,
            };
            out.push(format!(
                "redact {} could not follow its text through move #{}: masking {place} — fix it before recording",
                a.id,
                by + 1
            ));
        }
        out
    }

    /// Deletes the move at `index` (0-based).
    pub fn delete_move(&mut self, index: usize) -> Result<Move, Error> {
        self.delete_move_noting(index).map(|(m, _)| m)
    }

    /// [`Document::delete_move`] plus the notices of the redacts it
    /// dropped: one pinned to the deleted move lost its line at every step
    /// (t-154). The others pinned to a move keep their place in its lines.
    pub fn delete_move_noting(&mut self, index: usize) -> Result<(Move, Vec<String>), Error> {
        if index >= self.moves.len() {
            return Err(Error::NotFound(format!("move #{}", index + 1)));
        }
        let moves = self.moves.clone();
        self.pin_redacts_with(&moves, None);
        let old = self.clone();
        let m = self.moves.remove(index);
        let new_index = |i: usize| match i.cmp(&index) {
            std::cmp::Ordering::Less => Some(i),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(i - 1),
        };
        let mut notices = self.repin_redacts(&old, new_index);
        // The moves the deletion retargets (t-157).
        notices.extend(moves::retarget_notices(&old, self, new_index));
        if self.moves.is_empty() {
            self.code = std::mem::take(&mut self.base_code);
            self.source.first_line = self.base_first_line;
        } else {
            self.sync_code_to_step();
        }
        Ok((m, notices))
    }

    /// The document as shown at `step`: the shown file (cropped to its
    /// window) at that step and every annotation of that file with `step <=
    /// step`, shifted deterministically through the patches of later steps
    /// (ADR 0007 §3). Annotations of other files are left out (they come
    /// back untouched when their file is shown); those that lose their
    /// anchor are left out and listed in `unanchored`. When `step` is the
    /// current step the in-memory `code` is kept (it may be a typing
    /// animation in progress).
    pub fn view_at(&self, step: u32) -> View {
        let plain = || View {
            doc: self.clone(),
            unanchored: Vec::new(),
            view: ViewRef::default(),
            cursor: None,
            selection: None,
        };
        if self.moves.is_empty() {
            return plain();
        }
        let Ok(proj) = moves::project(self, &self.moves, step, false) else {
            // An invalid move was reported at load/add time; show the base.
            return plain();
        };
        let mut shapes: Vec<Option<Shape>> = self
            .annotations
            .iter()
            .map(|a| Some(a.shape.clone()))
            .collect();
        let mut scratch: Option<Document> = None;
        for ap in &proj.patches {
            let m = &self.moves[ap.index];
            for (a, slot) in self.annotations.iter().zip(shapes.iter_mut()) {
                // Authored on the code as shown at its own step: only later
                // steps' patches of its own file shift it.
                if a.file != ap.file || a.step >= ap.at_step {
                    continue;
                }
                // Redacts are followed by the projection itself (t-140).
                if matches!(a.shape, Shape::Redact { .. }) {
                    continue;
                }
                let Some(cur) = slot.as_ref() else { continue };
                *slot = match m.rebind.get(&a.id) {
                    Some(spec) => {
                        let d = scratch.get_or_insert_with(|| {
                            let mut d = self.clone();
                            d.moves.clear();
                            d.base_code.clear();
                            d
                        });
                        d.code = ap.after.clone();
                        d.source.first_line = self.view_first_line(&ap.file);
                        let mut shape = cur.clone();
                        spec.apply(&mut shape, d)
                            .and_then(|_| d.validate_shape(&shape))
                            .ok()
                            .map(|_| shape)
                    }
                    None => moves::shift_shape(cur, &ap.map),
                };
            }
        }
        let (code, first) = proj.shown_code(self);
        let mut doc = self.clone();
        doc.moves.clear();
        doc.base_code.clear();
        doc.code = if step == self.step {
            self.code.clone()
        } else {
            code
        };
        doc.source.first_line = if step == self.step {
            self.source.first_line
        } else {
            first
        };
        doc.step = step;
        let mut unanchored = Vec::new();
        let mut extra: Vec<Annotation> = Vec::new();
        doc.annotations = self
            .annotations
            .iter()
            .zip(shapes)
            .enumerate()
            .filter_map(|(i, (a, s))| {
                if let Shape::Redact {
                    keep,
                    label,
                    widened,
                    pin,
                    ..
                } = &a.shape
                {
                    // Where the projection followed it (t-140); not there
                    // yet when it was marked on a line a later move writes.
                    if a.file != proj.shown.file {
                        return None;
                    }
                    let r = proj.redacts.get(i)?.as_ref()?;
                    if r.dropped {
                        if a.step <= step {
                            unanchored.push(a.id.clone());
                        }
                        return None;
                    }
                    let spans: Vec<TextSpan> = r.all().copied().collect();
                    let widened = *widened || r.widened || spans.len() > 1;
                    let mask = |span: TextSpan, id: String| Annotation {
                        id,
                        shape: Shape::Redact {
                            span,
                            keep: *keep,
                            label: label.clone(),
                            widened,
                            pin: *pin,
                        },
                        ..a.clone()
                    };
                    // One annotation per masked line: the redact's own id on
                    // the first, derived ids (never a real one) on the rest.
                    for (k, sp) in spans.iter().enumerate().skip(1) {
                        extra.push(mask(*sp, format!("{}~{}", a.id, k + 1)));
                    }
                    return spans.first().map(|sp| mask(*sp, a.id.clone()));
                }
                match s {
                    Some(shape) if a.file == proj.shown.file => {
                        Some(Annotation { shape, ..a.clone() })
                    }
                    Some(_) => None,
                    None => {
                        if a.step <= step {
                            unanchored.push(a.id.clone());
                        }
                        None
                    }
                }
            })
            .collect();
        doc.annotations.extend(extra);
        View {
            doc,
            unanchored,
            view: proj.shown,
            cursor: proj.cursor,
            selection: proj.selection,
        }
    }

    /// Ids of the annotations hidden at the current step because no patch
    /// mapping survives for them (`(unanchored)` in `scene.list`).
    pub fn unanchored(&self) -> Vec<String> {
        if self.moves.is_empty() {
            return Vec::new();
        }
        self.view_at(self.step).unanchored
    }

    /// Validates a shape against the code of `file` as it is at `step`
    /// (annotations are authored against the code at their own step, with
    /// real file line numbers, whatever window is shown).
    pub fn validate_shape_in(&self, shape: &Shape, file: &ViewKey, step: u32) -> Result<(), Error> {
        if self.moves.is_empty() {
            return self.validate_shape(shape);
        }
        let mut d = self.clone();
        d.moves.clear();
        d.base_code.clear();
        // A move that no longer applies leaves the base code shown
        // (`sync_code_to_step`), so that is what the shape is checked against.
        let (code, first) = self
            .view_code_at(file, step)
            .unwrap_or_else(|_| (self.base().to_vec(), self.base_first()));
        d.code = code;
        d.source.first_line = first;
        d.validate_shape(shape)
    }

    /// Validates a shape against the file shown at `step`.
    pub fn validate_shape_at(&self, shape: &Shape, step: u32) -> Result<(), Error> {
        self.validate_shape_in(shape, &self.shown_view_at(step).file, step)
    }

    /// Resolves the style of a mark: inline `style` > document `styles[kind]` >
    /// built-in kind (aliases included). `None` if `kind` is unknown and the
    /// inline style has no glyph (nothing to draw).
    pub fn resolve_style(&self, kind: &str, inline: &MarkStyle) -> Option<MarkStyle> {
        let base = self
            .styles
            .get(kind.trim())
            .cloned()
            .or_else(|| builtin_style(kind));
        match base {
            Some(b) => Some(b.merged(inline)),
            None if inline.glyph.is_some() => Some(MarkStyle {
                legend: inline
                    .legend
                    .clone()
                    .or_else(|| Some(kind.trim().to_string())),
                ..inline.clone()
            }),
            None => None,
        }
    }

    /// Names available as `kind`: document styles first, then built-ins.
    pub fn kind_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.styles.keys().cloned().collect();
        v.extend(BUILTIN_KINDS.iter().map(|(k, ..)| k.to_string()));
        v
    }

    /// Last absolute line of the document.
    pub fn last_line(&self) -> u32 {
        self.source.first_line + self.code.len().saturating_sub(1) as u32
    }

    pub fn has_line(&self, line: u32) -> bool {
        line >= self.source.first_line && line <= self.last_line()
    }

    /// Text of an absolute line.
    pub fn line_text(&self, line: u32) -> Option<&str> {
        if !self.has_line(line) {
            return None;
        }
        self.code
            .get((line - self.source.first_line) as usize)
            .map(|s| s.as_str())
    }

    /// Number of graphemes on a line.
    pub fn line_len(&self, line: u32) -> u32 {
        self.line_text(line)
            .map(|t| t.graphemes(true).count() as u32)
            .unwrap_or(0)
    }

    /// Highest step used by any annotation or move (at least 1).
    pub fn max_step(&self) -> u32 {
        self.annotations
            .iter()
            .map(|a| a.step)
            .chain(self.moves.iter().map(|m| m.at_step))
            .max()
            .unwrap_or(1)
            .max(1)
    }

    pub fn find_annotation(&self, id: &str) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.id == id)
    }

    /// Next free id with the `a` prefix (a1, a2, …).
    pub fn next_id(&self) -> String {
        let mut n = self.annotations.len() as u32 + 1;
        loop {
            let id = format!("a{n}");
            if self.find_annotation(&id).is_none() {
                return id;
            }
            n += 1;
        }
    }

    /// Validates that a shape points to existing lines/columns.
    /// Tidies a document that came from a file: single-line texts (control
    /// characters become spaces), `step` within range, unique ids.
    pub fn normalize_loaded(&mut self) -> Result<(), Error> {
        if self.code.is_empty() {
            // Every layout assumes at least one row (see `from_code`).
            self.code.push(String::new());
        }
        let mut seen = std::collections::BTreeSet::new();
        for a in &mut self.annotations {
            if !seen.insert(a.id.clone()) {
                return Err(Error::DuplicateId(a.id.clone()));
            }
            sanitize_texts(&mut a.shape);
        }
        if self.step > self.max_step() {
            self.step = self.max_step();
        }
        crate::steps::normalize(&mut self.steps)?;
        // Embedded linked files (`views`) become loaded views; a file read
        // from disk by the loader (`fs::load_views`) is already there.
        for (file, code) in &self.views {
            let key = Self::normalize_file(file);
            self.view_code.entry(key).or_insert_with(|| {
                if code.is_empty() {
                    vec![String::new()]
                } else {
                    code.clone()
                }
            });
        }
        let keys: Vec<Option<ViewKey>> = self
            .annotations
            .iter()
            .map(|a| a.file.as_deref().map(|f| self.view_key(f)))
            .collect();
        for (a, k) in self.annotations.iter_mut().zip(keys) {
            if let Some(k) = k {
                a.file = k;
            }
        }
        if !self.moves.is_empty() {
            self.base_code = self.code.clone();
            self.base_first_line = self.source.first_line;
            self.validate_moves()?;
            self.reframe_redacts();
            if let Some(e) = self.sync_code_to_step() {
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn validate_shape(&self, shape: &Shape) -> Result<(), Error> {
        let check_line = |l: u32| {
            if self.has_line(l) {
                Ok(())
            } else {
                Err(Error::LineOutOfRange {
                    line: l,
                    first: self.source.first_line,
                    last: self.last_line(),
                })
            }
        };
        let check_span = |s: &TextSpan| {
            check_line(s.line)?;
            let len = self.line_len(s.line);
            if s.start >= s.end || s.end > len {
                return Err(Error::SpanOutOfRange {
                    line: s.line,
                    start: s.start,
                    end: s.end,
                    len,
                });
            }
            Ok(())
        };
        let check_anchor = |a: &Anchor| {
            check_line(a.line)?;
            let len = self.line_len(a.line);
            if a.col > len {
                return Err(Error::SpanOutOfRange {
                    line: a.line,
                    start: a.col,
                    end: a.col,
                    len,
                });
            }
            Ok(())
        };
        match shape {
            Shape::Highlight { span, .. } => check_span(span),
            Shape::Redact { span, keep, .. } => {
                check_span(span)?;
                if *keep > 8 {
                    return Err(Error::Invalid(format!(
                        "keep={keep}: a redact shows at most 8 trailing characters"
                    )));
                }
                Ok(())
            }
            Shape::Mark {
                span, kind, style, ..
            } => {
                check_span(span)?;
                if self.resolve_style(kind, style).is_none() {
                    return Err(Error::Invalid(format!(
                        "unknown mark kind: {kind} (use one of {}, a style from this document, or an inline style with a glyph)",
                        BUILTIN_KINDS
                            .iter()
                            .map(|(k, ..)| *k)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
                Ok(())
            }
            Shape::Box {
                from_line, to_line, ..
            } => {
                check_line(*from_line)?;
                check_line(*to_line)?;
                if from_line > to_line {
                    return Err(Error::Invalid(format!(
                        "the box starts at {from_line} and ends earlier, at {to_line}"
                    )));
                }
                Ok(())
            }
            Shape::Arrow { from, to, .. } => {
                check_anchor(from)?;
                check_anchor(to)
            }
            Shape::Note { line, text } => {
                check_line(*line)?;
                if text.trim().is_empty() {
                    return Err(Error::Invalid("the note is empty".into()));
                }
                Ok(())
            }
            Shape::Callout {
                from_line,
                to_line,
                text,
                ..
            } => {
                check_line(*from_line)?;
                let b = to_line.unwrap_or(*from_line);
                check_line(b)?;
                if *from_line > b {
                    return Err(Error::Invalid(format!(
                        "the callout starts at {from_line} and ends before, at {b}"
                    )));
                }
                if text.trim().is_empty() {
                    return Err(Error::Invalid("the callout text is empty".into()));
                }
                Ok(())
            }
            Shape::Spotlight {
                from_line,
                to_line,
                ids,
                ..
            } => {
                if from_line.is_none() && ids.is_empty() {
                    return Err(Error::Invalid(
                        "a spotlight needs a line range, annotation ids, or both".into(),
                    ));
                }
                if let Some(a) = from_line {
                    check_line(*a)?;
                    let b = to_line.unwrap_or(*a);
                    check_line(b)?;
                    if *a > b {
                        return Err(Error::Invalid(format!(
                            "the spotlight starts at {a} and ends earlier, at {b}"
                        )));
                    }
                }
                for id in ids {
                    if self.find_annotation(id).is_none() {
                        return Err(Error::NotFound(id.clone()));
                    }
                }
                Ok(())
            }
        }
    }

    /// Settles the file an annotation belongs to: the one it names
    /// (normalized; it must be the main file or a linked one), else the
    /// view shown at its step (ADR 0007 §3d/§3f).
    fn resolve_file(&self, ann: &mut Annotation) -> Result<(), Error> {
        match &ann.file {
            Some(f) => {
                let key = self.view_key(f);
                if key.is_some() && !self.view_code.contains_key(key.as_deref().unwrap_or("")) {
                    return Err(Error::NotFound(format!(
                        "file «{f}» is not a view of this document (link it with a code.link move first)"
                    )));
                }
                ann.file = key;
            }
            None => {
                if !self.moves.is_empty() {
                    ann.file = self.shown_view_at(ann.step).file;
                }
            }
        }
        Ok(())
    }

    /// Adds an annotation. If `id` is empty a new one is assigned. Returns the id.
    pub fn add(&mut self, mut ann: Annotation) -> Result<String, Error> {
        // Step 0 is the untouched code: only a redact pinned there (t-140).
        let pinned = matches!(ann.shape, Shape::Redact { .. }) && !self.moves.is_empty();
        if ann.step == 0 && !pinned {
            ann.step = 1;
        }
        self.resolve_file(&mut ann)?;
        // A redact that comes pinned (an agent's `scene.get` → `scene.patch`
        // round trip, t-154) is checked on the code it is pinned to.
        self.validate_annotation(&ann)?;
        sanitize_texts(&mut ann.shape);
        if ann.id.trim().is_empty() {
            ann.id = self.next_id();
        } else if self.find_annotation(&ann.id).is_some() {
            return Err(Error::DuplicateId(ann.id));
        }
        let id = ann.id.clone();
        let redact = matches!(ann.shape, Shape::Redact { .. });
        self.annotations.push(ann);
        if redact && !self.moves.is_empty() {
            self.pin_own_redact(&id);
        }
        Ok(id)
    }

    /// Pins the redact `id` Tiza just added or moved (t-154): to the file,
    /// or to the move that wrote its line (`pin: N`), never left to the
    /// step's reading, so it gets no second reading and moves added or
    /// deleted later keep it on its text.
    fn pin_own_redact(&mut self, id: &str) {
        let moves = self.moves.clone();
        self.pin_redacts_with(&moves, Some(id));
    }

    /// Validates an annotation where its coordinates live: a pinned redact
    /// on the code it is pinned to (t-154), anything else on the code at
    /// its step.
    pub fn validate_annotation(&self, a: &Annotation) -> Result<(), Error> {
        if !self.moves.is_empty() {
            let frame = moves::redact_frame(self, a);
            let code = match frame {
                Some(moves::RedactFrame::File) => self
                    .view_base_code(&a.file, false)
                    .map(|c| (c, self.view_first_line(&a.file))),
                Some(moves::RedactFrame::AfterMove(i)) => {
                    moves::code_after_move(self, i).map(|(_, c, f)| (c, f))
                }
                _ => None,
            };
            if let Some((code, first)) = code {
                let mut d = self.clone();
                d.moves.clear();
                d.base_code.clear();
                d.code = code;
                d.source.first_line = first;
                return d.validate_shape(&a.shape);
            }
        }
        self.validate_shape_in(&a.shape, &a.file, a.step)
    }

    pub fn update(&mut self, mut ann: Annotation) -> Result<(), Error> {
        self.resolve_file(&mut ann)?;
        // A redact whose span, pin and file did not change (its label or
        // `keep` did) stays where it is pinned (t-154); a new span is on
        // the code at its step, as for any annotation, and is pinned again.
        let same_place = match (&ann.shape, self.find_annotation(&ann.id)) {
            (
                Shape::Redact { span, pin, .. },
                Some(Annotation {
                    shape:
                        Shape::Redact {
                            span: s0, pin: p0, ..
                        },
                    file,
                    step,
                    ..
                }),
            ) => span == s0 && pin == p0 && file == &ann.file && *step == ann.step,
            _ => false,
        };
        if let Shape::Redact { pin, .. } = &mut ann.shape
            && !same_place
        {
            *pin = None;
        }
        if same_place {
            self.validate_annotation(&ann)?;
        } else {
            self.validate_shape_in(&ann.shape, &ann.file, ann.step)?;
        }
        sanitize_texts(&mut ann.shape);
        let redact = matches!(ann.shape, Shape::Redact { .. });
        let id = ann.id.clone();
        let slot = self
            .annotations
            .iter_mut()
            .find(|a| a.id == ann.id)
            .ok_or_else(|| Error::NotFound(ann.id.clone()))?;
        *slot = ann;
        if redact && !self.moves.is_empty() && !same_place {
            self.pin_own_redact(&id);
        }
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<Annotation, Error> {
        let pos = self
            .annotations
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        Ok(self.annotations.remove(pos))
    }

    /// Searches `text` in the code and returns the span of the `occurrence`-th
    /// match (1-based). If `near_line` is given, that line is searched first.
    /// Resolves a line range from two markers (t-097): `start` is the first
    /// occurrence in the document, `end` the first occurrence at or after it
    /// (same line allowed when it comes after the start text). Errors name
    /// the missing marker; an `end` that only appears before the start is
    /// an invalid range, not a silent swap.
    pub fn resolve_markers(&self, start: &str, end: &str) -> Result<(u32, u32), Error> {
        if start.trim().is_empty() || end.trim().is_empty() {
            return Err(Error::Invalid(
                "find_start and find_end must both be non-empty".into(),
            ));
        }
        let s = self
            .find_text(start, 1, None)
            .ok_or_else(|| Error::TextNotFound(format!("start marker «{start}»")))?;
        // Same line, after the start text?
        if let Some(t) = self.line_text(s.line)
            && let Some(byte_end) = grapheme_byte_offset(t, s.end)
            && t[byte_end..].contains(end)
        {
            return Ok((s.line, s.line));
        }
        for line in (s.line + 1)..=self.last_line() {
            if self.line_text(line).is_some_and(|t| t.contains(end)) {
                return Ok((s.line, line));
            }
        }
        if self.find_text(end, 1, None).is_some() {
            return Err(Error::Invalid(format!(
                "end marker «{end}» appears only before the start marker «{start}» (line {})",
                s.line
            )));
        }
        Err(Error::TextNotFound(format!(
            "end marker «{end}» (after line {})",
            s.line
        )))
    }

    pub fn find_text(
        &self,
        text: &str,
        occurrence: u32,
        near_line: Option<u32>,
    ) -> Option<TextSpan> {
        if text.is_empty() {
            return None;
        }
        let occurrence = occurrence.max(1);
        let mut seen = 0;
        // The masks where they are now: with moves, followed to this step
        // (t-140); the stored spans refer to the code of their own step.
        let masks: Vec<Annotation> = if self.moves.is_empty() {
            Vec::new()
        } else {
            self.view_at(self.step).doc.annotations
        };
        let masks: &[Annotation] = if self.moves.is_empty() {
            &self.annotations
        } else {
            &masks
        };
        let search_line = |line: u32, seen: &mut u32| -> Option<TextSpan> {
            let t = self.line_text(line)?;
            let mut from = 0;
            while let Some(pos) = t[from..].find(text) {
                let byte_start = from + pos;
                let start = grapheme_index(t, byte_start);
                let end = grapheme_index(t, byte_start + text.len());
                from = byte_start + text.len().max(1);
                // A hit under a redact mask does not exist (t-117): a search
                // must never confirm what the mask hides.
                if crate::redact::overlaps_mask(masks, line, start, end) {
                    continue;
                }
                *seen += 1;
                if *seen == occurrence {
                    return Some(TextSpan { line, start, end });
                }
            }
            None
        };
        if let Some(l) = near_line {
            if let Some(s) = search_line(l, &mut seen) {
                return Some(s);
            }
            seen = 0;
        }
        for line in self.source.first_line..=self.last_line() {
            if let Some(s) = search_line(line, &mut seen) {
                return Some(s);
            }
        }
        None
    }
}

/// Converts a byte offset into `s` to the corresponding grapheme index.
pub fn grapheme_index(s: &str, byte_offset: usize) -> u32 {
    s.grapheme_indices(true)
        .take_while(|(b, _)| *b < byte_offset)
        .count() as u32
}

/// Byte offset where grapheme `index` starts (`s.len()` past the end).
pub fn grapheme_byte_offset(s: &str, index: u32) -> Option<usize> {
    Some(
        s.grapheme_indices(true)
            .nth(index as usize)
            .map(|(b, _)| b)
            .unwrap_or(s.len()),
    )
}

/// Returns the slice of `s` between graphemes `start..end`.
pub fn grapheme_slice(s: &str, start: u32, end: u32) -> &str {
    let mut it = s.grapheme_indices(true);
    let b0 = it
        .by_ref()
        .nth(start as usize)
        .map(|(b, _)| b)
        .unwrap_or(s.len());
    let b1 = if end > start {
        s.grapheme_indices(true)
            .nth(end as usize)
            .map(|(b, _)| b)
            .unwrap_or(s.len())
    } else {
        b0
    };
    &s[b0..b1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        Document::from_code(
            "fn main() {\n    let s = String::from(\"hola\");\n    takes(s);\n}\n",
            "rs",
            None,
        )
    }

    #[test]
    fn find_text_returns_grapheme_span() {
        let d = doc();
        let s = d.find_text("takes(s)", 1, None).unwrap();
        assert_eq!(
            s,
            TextSpan {
                line: 3,
                start: 4,
                end: 12
            }
        );
        assert_eq!(
            grapheme_slice(d.line_text(3).unwrap(), s.start, s.end),
            "takes(s)"
        );
    }

    #[test]
    fn find_text_second_occurrence() {
        let d = doc();
        // On line 2, "o" appears in `from` (1) and in `hola` (2).
        let s = d.find_text("o", 2, Some(2)).unwrap();
        assert_eq!(s.line, 2);
        assert_eq!(s.start, 27);
        // If the requested line has too few occurrences, the whole document is searched.
        let s = d.find_text("s", 2, Some(2)).unwrap();
        assert_eq!(s.line, 3);
    }

    #[test]
    fn add_validates_and_assigns_ids() {
        let mut d = doc();
        let id = d
            .add(Annotation {
                id: String::new(),
                step: 1,
                color: None,
                markers: None,
                file: None,
                shape: Shape::Note {
                    line: 2,
                    text: "s is born here".into(),
                },
            })
            .unwrap();
        assert_eq!(id, "a1");
        let err = d
            .add(Annotation {
                id: String::new(),
                step: 1,
                color: None,
                markers: None,
                file: None,
                shape: Shape::Note {
                    line: 99,
                    text: "x".into(),
                },
            })
            .unwrap_err();
        assert!(matches!(err, Error::LineOutOfRange { line: 99, .. }));
    }

    #[test]
    fn roundtrip_json() {
        let mut d = doc();
        d.add(Annotation {
            id: "a1".into(),
            step: 2,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Arrow {
                from: Anchor { line: 2, col: 8 },
                to: Anchor { line: 3, col: 10 },
                label: Some("move".into()),
                line: Default::default(),
            },
        })
        .unwrap();
        let json = serde_json::to_string_pretty(&d).unwrap();
        let mut back: Document = serde_json::from_str(&json).unwrap();
        if let CodeSource::Embedded(lines) = &back.code_src {
            back.code = lines.clone();
        }
        assert_eq!(d, back);
        assert!(json.contains("\"type\": \"arrow\""));
    }

    #[test]
    fn unicode_graphemes() {
        let d = Document::from_code("let año = \"ñandú\"; // 🦀 crab\n", "rs", None);
        let s = d.find_text("🦀", 1, None).unwrap();
        assert_eq!(
            grapheme_slice(d.line_text(1).unwrap(), s.start, s.end),
            "🦀"
        );
        assert_eq!(s.end - s.start, 1);
    }
}
