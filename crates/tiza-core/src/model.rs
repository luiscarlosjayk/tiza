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

/// Schema version of the `.tiza` file.
pub const SCHEMA_VERSION: u32 = 1;

/// The last step a document can have (t-166): annotations, moves and
/// `steps` entries name steps 0 to this. A take is laid out over every
/// step (the stable geometry, `check`, a GIF), so an
/// unbounded step number made one export cost seconds per million empty
/// steps; real takes have tens.
pub const MAX_STEP: u32 = 999;

/// The longest hold a document or an export asks for, in milliseconds
/// (t-166): a `wait` move, a step's `auto_advance_ms`, a `code.patch`'s
/// typing time per unit, the `--interval` of a recording. One hour; a
/// longer one is a typo (or an attack on the encoder) and is refused.
pub const MAX_HOLD_MS: u64 = 3_600_000;

/// The largest `source.first_line` (t-166): line numbers are `u32`, and a
/// first line near its end overflowed as soon as a line was counted after
/// it.
pub const MAX_FIRST_LINE: u32 = 1_000_000_000;

/// `step` of `what` within 0..=[`MAX_STEP`] (t-166).
pub fn check_step(step: u32, what: &str) -> Result<(), Error> {
    if step > MAX_STEP {
        return Err(Error::Invalid(format!(
            "{what}: step {step} is past the last step a document can have ({MAX_STEP})"
        )));
    }
    Ok(())
}

/// How a bounds error names an annotation: by its id, or `annotation`
/// alone for one that has none yet (t-166: not `annotation : step …`).
fn annotation_what(id: &str) -> String {
    if id.trim().is_empty() {
        "annotation".into()
    } else {
        format!("annotation {id}")
    }
}

/// A hold of `ms` milliseconds for `what` within [`MAX_HOLD_MS`] (t-166).
pub fn check_hold_ms(ms: u64, what: &str) -> Result<(), Error> {
    if ms > MAX_HOLD_MS {
        return Err(Error::Invalid(format!(
            "{what}: {ms} ms is longer than the longest hold ({MAX_HOLD_MS} ms, one hour)"
        )));
    }
    Ok(())
}

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
        }
    }

    /// Line ranges the shape occupies (to focus the code of an annotation).
    /// An arrow occupies only its two endpoints, not the stretch between them.
    pub fn line_ranges(&self) -> Vec<(u32, u32)> {
        match self {
            Shape::Highlight { span, .. } | Shape::Mark { span, .. } => {
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
    /// Step at which it appears (1-based).
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
        | Shape::Arrow { label, .. } => {
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

/// Why an annotation is not drawn at a step ([`Document::hidden_reason`],
/// t-155). `Display` gives the words the socket reply carries in `hidden`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hidden {
    /// No annotation has that id.
    NoSuch(String),
    /// A spotlight past its `until`.
    SpotlightPast { from: u32, end: u32 },
    /// It appears at a later step.
    Later(u32),
    /// Its file is not the one shown at `step` (t-105).
    OtherFile {
        file: String,
        shown: String,
        step: u32,
    },
    /// No line of it survived a patch.
    Unanchored(u32),
    /// Its lines fall outside the window shown at `step`.
    Outside { step: u32, first: u32, last: u32 },
}

impl std::fmt::Display for Hidden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Hidden::NoSuch(id) => write!(f, "no annotation {id}"),
            Hidden::SpotlightPast { from, end } if from == end => {
                write!(f, "a spotlight shows only at its step {end}")
            }
            Hidden::SpotlightPast { from, end } => {
                write!(f, "a spotlight shows from step {from} to {end}")
            }
            Hidden::Later(s) => write!(f, "it appears at step {s}"),
            Hidden::OtherFile { file, shown, step } => {
                write!(
                    f,
                    "its file {file} is not shown at step {step} ({shown} is)"
                )
            }
            Hidden::Unanchored(s) => write!(
                f,
                "it lost its anchor at step {s} (unanchored: no line of it survived a patch)"
            ),
            Hidden::Outside { step, first, last } => write!(
                f,
                "its lines are outside the code shown at step {step} ({first}-{last})"
            ),
        }
    }
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
    pub fn embed_code(&mut self) {
        self.code_src = CodeSource::Embedded(self.base().to_vec());
        // Every linked file too, so the demo stays one portable file.
        for file in self.link_files() {
            if let Some(code) = self.view_code.get(&file) {
                self.views.insert(file, code.clone());
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
    /// linked file embedded, the first line that of the main file.
    pub fn embedded_copy(&self) -> Document {
        let mut d = self.clone();
        d.sync_code_src();
        d.embed_code();
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
    /// trimmed, without `.` segments, repeated `/`, or a `dir/..` pair
    /// (`sub/../app.env` is `app.env`, never another view of the same
    /// file). A leading `..` stays; aliases that only the
    /// disk can tell (`../dir/x`, an absolute path, a symlink, another
    /// case) are settled by `fs::canonical_link`.
    pub fn normalize_file(file: &str) -> String {
        let f = file.trim();
        let absolute = f.starts_with('/');
        let mut parts: Vec<&str> = Vec::new();
        for seg in f.split('/') {
            match seg {
                "" | "." => {}
                ".." if parts.last().is_some_and(|p| *p != "..") => {
                    parts.pop();
                }
                ".." if absolute => {}
                s => parts.push(s),
            }
        }
        let joined = parts.join("/");
        if absolute {
            format!("/{joined}")
        } else {
            joined
        }
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
    pub fn view_base_code(&self, key: &ViewKey) -> Option<Vec<String>> {
        Some(match key {
            None => self.base().to_vec(),
            Some(f) => self.view_code.get(f)?.clone(),
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
            .filter(|m| m.at_step == step && m.stale.is_none())
            .map(|m| match &m.action {
                MoveAction::CodePatch(p) => p.typing_ms(),
                MoveAction::Wait(w) => w.ms,
                _ => 0,
            })
            .fold(0u64, u64::saturating_add)
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
            .fold(0u64, u64::saturating_add)
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
        let p = moves::project(self, &self.moves, step)?;
        Ok(p.shown_code(self))
    }

    /// The full code of one view at `step` (its step-0 code with its
    /// patches `<= step`), and its first line. Annotations of that view are
    /// validated against it, whatever window is shown.
    pub fn view_code_at(&self, key: &ViewKey, step: u32) -> Result<(Vec<String>, u32), Error> {
        if self.moves.is_empty() {
            return Ok((self.code.clone(), self.source.first_line));
        }
        let p = moves::project(self, &self.moves, step)?;
        let first = self.view_first_line(key);
        match p.codes.get(key) {
            Some(c) => Ok((c.clone(), first)),
            None => self.view_base_code(key).map(|c| (c, first)).ok_or_else(|| {
                Error::ViewNotFound(format!(
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
                // The TUI shows it on a step change (`moves: …`).
                Some(e)
            }
        }
    }

    /// Validates every move against the code at its step (ADR 0007 §3). A
    /// `rebind` entry whose annotation is gone is ignored, not an error.
    pub fn validate_moves(&self) -> Result<(), Error> {
        moves::validate(self, &self.moves, &self.annotations)
    }

    /// Adds a move (validated against the code at its step). Returns its
    /// index. A `code.link` needs its file loaded first (`fs::load_view`
    /// or an entry in `views`).
    ///
    /// A `code.patch` changes the code that the annotations of its file at
    /// its step and later, and the moves that run after it, were written
    /// against: they are carried through it ([`crate::carry`]), so they
    /// stay on the same text. One whose line it removes is an error naming
    /// it, and nothing changes.
    pub fn add_move(&mut self, m: Move) -> Result<usize, Error> {
        let what = format!("move #{}", self.moves.len() + 1);
        check_step(m.at_step, &what)?;
        moves::check_move_bounds(&m, &what)?;
        moves::check_rebind_ids(&m, self.moves.len(), &self.annotations, |_, _| false)?;
        let mut with = self.clone();
        if with.moves.is_empty() {
            with.base_code = with.code.clone();
            with.base_first_line = with.source.first_line;
        }
        let mut list = with.moves.clone();
        list.push(m);
        if matches!(list[list.len() - 1].action, MoveAction::CodePatch(_)) {
            let (carried, annotations) =
                crate::carry::carry(&with, &with.moves, &list, crate::carry::Change::Added)?;
            list = carried;
            with.annotations = annotations;
        }
        with.moves = list;
        let i = with.moves.len() - 1;
        // A later move that applied before and fails now fails because of
        // this one: say so.
        moves::validate(&with, &with.moves, &with.annotations).map_err(|e| {
            if e.to_string().starts_with(&format!("move #{}, ", i + 1)) {
                e
            } else {
                Error::Within {
                    what: format!(
                        "move #{} (step {}), the one being added, makes a later move fail",
                        i + 1,
                        with.moves[i].at_step
                    ),
                    inner: Box::new(e),
                }
            }
        })?;
        // Only a patch or a link changes the code annotations stand on (a
        // link sends the later patches to another file).
        if matches!(
            with.moves[i].action,
            MoveAction::CodePatch(_) | MoveAction::CodeLink(_)
        ) {
            self.still_fit(
                &with,
                &format!("move #{} (step {})", i + 1, with.moves[i].at_step),
            )?;
        }
        *self = with;
        self.sync_code_to_step();
        Ok(self.moves.len() - 1)
    }

    /// Every annotation that fits its step in `self` still fits it in
    /// `after` (the document once a move is added or deleted, `change`).
    fn still_fit(&self, after: &Document, change: &str) -> Result<(), Error> {
        let fit = self.annotation_checks();
        for ((b, was), now) in after
            .annotations
            .iter()
            .zip(fit)
            .zip(after.annotation_checks())
        {
            if was.is_ok()
                && let Err(e) = now
            {
                return Err(Error::Within {
                    what: format!("annotation {}, step {}: after {change}", b.id, b.step),
                    inner: Box::new(e),
                });
            }
        }
        Ok(())
    }

    /// [`Document::validate_annotation`] of every annotation, in order, with
    /// the code of each file at each step worked out once (a document with
    /// many annotations and moves would replay the moves for each one).
    pub fn annotation_checks(&self) -> Vec<Result<(), Error>> {
        if self.moves.is_empty() {
            return self
                .annotations
                .iter()
                .map(|a| self.validate_annotation(a))
                .collect();
        }
        let mut order: Vec<usize> = (0..self.annotations.len()).collect();
        order.sort_by(|&x, &y| {
            let (a, b) = (&self.annotations[x], &self.annotations[y]);
            (a.step, &a.file).cmp(&(b.step, &b.file))
        });
        // One replay per step gives the code of every file at that step.
        let mut proj: Option<(u32, Result<moves::Projection, ()>)> = None;
        let mut d = self.clone();
        d.moves.clear();
        d.base_code.clear();
        let mut out: Vec<Option<Result<(), Error>>> =
            (0..self.annotations.len()).map(|_| None).collect();
        let mut at: Option<(ViewKey, u32)> = None;
        for i in order {
            let a = &self.annotations[i];
            if at.as_ref() != Some(&(a.file.clone(), a.step)) {
                if proj.as_ref().map(|(k, _)| *k) != Some(a.step) {
                    let p = moves::project(self, &self.moves, a.step).map_err(|_| ());
                    proj = Some((a.step, p));
                }
                // As `view_code_at` in `validate_shape_in`: a move that no
                // longer applies leaves the base code shown.
                let code = match proj.as_ref().map(|(_, p)| p) {
                    Some(Ok(p)) => p
                        .codes
                        .get(&a.file)
                        .cloned()
                        .or_else(|| self.view_base_code(&a.file))
                        .map(|c| (c, self.view_first_line(&a.file))),
                    _ => None,
                };
                let (code, first) =
                    code.unwrap_or_else(|| (self.base().to_vec(), self.base_first()));
                d.code = code;
                d.source.first_line = first;
                at = Some((a.file.clone(), a.step));
            }
            out[i] = Some(d.validate_shape(&a.shape));
        }
        out.into_iter().map(|r| r.unwrap_or(Ok(()))).collect()
    }

    /// Deletes the move at `index` (0-based). A `code.patch` carries the
    /// annotations and later moves back, as [`Document::add_move`] carried
    /// them; one on a line that patch wrote is an error naming it.
    pub fn delete_move(&mut self, index: usize) -> Result<Move, Error> {
        if index >= self.moves.len() {
            return Err(Error::MoveNotFound(format!("#{}", index + 1)));
        }
        let mut with = self.clone();
        let m = with.moves.remove(index);
        if matches!(m.action, MoveAction::CodePatch(_)) {
            let (carried, annotations) = crate::carry::carry(
                self,
                &self.moves,
                &with.moves,
                crate::carry::Change::Deleted(index),
            )?;
            with.moves = carried;
            with.annotations = annotations;
        }
        if with.moves.is_empty() {
            with.code = std::mem::take(&mut with.base_code);
            with.source.first_line = with.base_first_line;
        }
        // The moves left must still apply (a deleted `code.link` sends the
        // later patches and selections to another file), named as
        // `move.list` shows them now: a pause holds the deleted one's place.
        let change = format!("deleting move #{} (step {})", index + 1, m.at_step);
        // The only link to a file takes it out of the document: its
        // annotations would belong to no view.
        if let MoveAction::CodeLink(l) = &m.action
            && let Some(key) = self.view_key(&l.file)
            && !with.link_files().contains(&key)
        {
            let ids: Vec<&str> = with
                .annotations
                .iter()
                .filter(|a| a.file.as_ref() == Some(&key))
                .map(|a| a.id.as_str())
                .collect();
            if !ids.is_empty() {
                return Err(Error::Invalid(format!(
                    "{change}: {} belong{} to {key}, which no other code.link shows; delete {} first",
                    ids.join(", "),
                    if ids.len() == 1 { "s" } else { "" },
                    if ids.len() == 1 { "it" } else { "them" },
                )));
            }
        }
        let mut numbered = with.moves.clone();
        numbered.insert(
            index,
            Move {
                at_step: m.at_step,
                action: MoveAction::Wait(moves::Wait { ms: 1 }),
                rebind: BTreeMap::new(),
                stale: None,
                held: Vec::new(),
            },
        );
        moves::validate(&with, &numbered, &with.annotations).map_err(|e| Error::Within {
            what: change.clone(),
            inner: Box::new(e),
        })?;
        // Only a patch or a link changes the code annotations stand on.
        if matches!(m.action, MoveAction::CodePatch(_) | MoveAction::CodeLink(_)) {
            self.still_fit(&with, &change)?;
        }
        *self = with;
        if !self.moves.is_empty() {
            self.sync_code_to_step();
        }
        Ok(m)
    }

    /// Every annotation's shape at `step`, shifted through the patches of
    /// `proj` (a projection at `step` or later) that run after its own step
    /// and by `step`; `None` for one a patch unanchors or a stale move
    /// holds.
    pub fn shapes_at(&self, proj: &moves::Projection, step: u32) -> Vec<Option<Shape>> {
        let mut shapes: Vec<Option<Shape>> = self
            .annotations
            .iter()
            .map(|a| Some(a.shape.clone()))
            .collect();
        let mut scratch: Option<Document> = None;
        for ap in proj.patches.iter().filter(|ap| ap.at_step <= step) {
            let m = &self.moves[ap.index];
            for (a, slot) in self.annotations.iter().zip(shapes.iter_mut()) {
                // Authored on the code as shown at its own step: only later
                // steps' patches of its own file shift it.
                if a.file != ap.file || a.step >= ap.at_step {
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
        // Kept as they were while the move that writes their text is stale.
        let held = crate::stale::held(self);
        for (a, slot) in self.annotations.iter().zip(shapes.iter_mut()) {
            if held.contains(&a.id) {
                *slot = None;
            }
        }
        shapes
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
        let Ok(proj) = moves::project(self, &self.moves, step) else {
            // An invalid move was reported at load/add time; show the base.
            return plain();
        };
        let shapes = self.shapes_at(&proj, step);
        let (code, first) = proj.shown_code(self);
        let mut doc = self.clone();
        doc.moves.clear();
        doc.base_code.clear();
        let live = step == self.step;
        doc.code = if live { self.code.clone() } else { code };
        doc.source.first_line = if live { self.source.first_line } else { first };
        doc.step = step;
        let mut unanchored = Vec::new();
        doc.annotations = self
            .annotations
            .iter()
            .zip(shapes)
            .filter_map(|(a, s)| match s {
                Some(shape) if a.file == proj.shown.file => Some(Annotation { shape, ..a.clone() }),
                Some(_) => None,
                None => {
                    if a.step <= step {
                        unanchored.push(a.id.clone());
                    }
                    None
                }
            })
            .collect();
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

    /// Why annotation `id` is not on screen at `step`, in words (`appears
    /// at step 1`, `its file lib.rs is not shown at step 2`…), or `None`
    /// when it is drawn there (t-155): [`Document::hidden_reason`] as text.
    pub fn why_hidden(&self, id: &str, step: u32) -> Option<String> {
        self.hidden_reason(id, step).map(|h| h.to_string())
    }

    /// Why annotation `id` is not on screen at `step`, or `None` when it is
    /// drawn there (t-155). Follows the same rules as the layout: its file
    /// must be the one shown, its step reached (a spotlight also not past
    /// its `until`), its anchor kept and its
    /// lines inside the window. An unknown id is reported as such.
    pub fn hidden_reason(&self, id: &str, step: u32) -> Option<Hidden> {
        let Some(a) = self.find_annotation(id) else {
            return Some(Hidden::NoSuch(id.to_string()));
        };
        match &a.shape {
            Shape::Spotlight { until, .. } if step >= a.step => {
                let end = until.unwrap_or(a.step);
                if step > end {
                    return Some(Hidden::SpotlightPast { from: a.step, end });
                }
            }
            _ if a.step > step => return Some(Hidden::Later(a.step)),
            _ => {}
        }
        let shown = self.shown_view_at(step);
        if a.file != shown.file {
            return Some(Hidden::OtherFile {
                file: self.file_label(&a.file),
                shown: self.file_label(&shown.file),
                step,
            });
        }
        let view = self.view_at(step);
        let Some(v) = view.doc.find_annotation(id) else {
            return Some(Hidden::Unanchored(step));
        };
        let outside = v
            .shape
            .line_ranges()
            .iter()
            .any(|(f, t)| !view.doc.has_line(*f) || !view.doc.has_line(*t));
        outside.then(|| Hidden::Outside {
            step,
            first: view.doc.source.first_line,
            last: view.doc.last_line(),
        })
    }

    /// A file as the document names it: a linked one by its path, the main
    /// one as `code.file` names it (like a link does), or by its file name
    /// when the code is embedded (`the main file` without one).
    pub fn file_label(&self, f: &ViewKey) -> String {
        match f {
            Some(f) => f.clone(),
            None => match &self.code_src {
                CodeSource::File { file, .. } => file.clone(),
                CodeSource::Embedded(_) => self
                    .source
                    .path
                    .as_deref()
                    .and_then(|p| std::path::Path::new(p).file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "the main file".into()),
            },
        }
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
        // Saturating (t-166): a document never gets here with a first line
        // past [`MAX_FIRST_LINE`], and this never overflows if one does.
        let len = u32::try_from(self.code.len().saturating_sub(1)).unwrap_or(u32::MAX);
        self.source.first_line.saturating_add(len)
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
    /// The steps, up to `last`, at which what a take draws can change
    /// (t-166): 0, each annotation's step and the one after it (a spotlight
    /// shows only at its own), each move's step. Every other step draws
    /// exactly as the last of these before it, so a loop over the take that
    /// looks for a maximum, a union or the first step of something visits
    /// only these, and costs the same whatever the number of empty steps.
    pub fn change_steps(&self, last: u32) -> Vec<u32> {
        let mut out: Vec<u32> = std::iter::once(0)
            .chain(
                self.annotations
                    .iter()
                    .flat_map(|a| [a.step, a.step.saturating_add(1)]),
            )
            .chain(self.moves.iter().map(|m| m.at_step))
            .filter(|s| *s <= last)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// [`Document::change_steps`] with the run of steps each one stands
    /// for: `(step, steps it draws like)`, the runs covering 0..=`last`.
    pub fn change_runs(&self, last: u32) -> Vec<(u32, std::ops::RangeInclusive<u32>)> {
        let steps = self.change_steps(last);
        steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let end = steps.get(i + 1).map_or(last, |n| n - 1);
                (*s, *s..=end)
            })
            .collect()
    }

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

    /// Next free id with the `a` prefix (a1, a2, …). An id a move's
    /// `rebind` names is never given out, even when its annotation is gone:
    /// the entry would re-anchor the new annotation instead.
    pub fn next_id(&self) -> String {
        let mut n = self.annotations.len() as u32 + 1;
        loop {
            let id = format!("a{n}");
            if self.find_annotation(&id).is_none()
                && !self.moves.iter().any(|m| m.rebind.contains_key(&id))
            {
                return id;
            }
            n += 1;
        }
    }

    /// Validates that a shape points to existing lines/columns.
    /// Tidies a document that came from a file: single-line texts (control
    /// characters become spaces), `step` within range, unique ids.
    pub fn normalize_loaded(&mut self) -> Result<(), Error> {
        self.check_bounds()?;
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
        }
        if !self.moves.is_empty() {
            self.validate_moves()?;
            if let Some(e) = self.sync_code_to_step() {
                return Err(e);
            }
        }
        Ok(())
    }

    /// The numbers a loaded or put document may hold (t-166): steps within
    /// [`MAX_STEP`], holds within [`MAX_HOLD_MS`], a first line within
    /// [`MAX_FIRST_LINE`]. Ids and numbers only, never document text.
    pub fn check_bounds(&self) -> Result<(), Error> {
        if self.source.first_line > MAX_FIRST_LINE {
            return Err(Error::Invalid(format!(
                "source.first_line {} is past the largest first line ({MAX_FIRST_LINE})",
                self.source.first_line
            )));
        }
        for a in &self.annotations {
            check_step(a.step, &annotation_what(&a.id))?;
        }
        for (i, m) in self.moves.iter().enumerate() {
            let what = format!("move #{}", i + 1);
            check_step(m.at_step, &what)?;
            moves::check_move_bounds(m, &what)?;
        }
        // `steps` entries: in `steps::normalize`, which every path runs.
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
            Shape::Mark {
                span, kind, style, ..
            } => {
                check_span(span)?;
                if self.resolve_style(kind, style).is_none() {
                    return Err(Error::Invalid(format!(
                        "unknown mark kind (use one of {}, a style from this document, or an inline style with a glyph)",
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
                    return Err(Error::ViewNotFound(format!(
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
        // Step 0 is the untouched code: an annotation goes to step 1 at least.
        if ann.step == 0 {
            ann.step = 1;
        }
        check_step(ann.step, &annotation_what(&ann.id))?;
        self.resolve_file(&mut ann)?;
        self.validate_annotation(&ann)?;
        sanitize_texts(&mut ann.shape);
        if ann.id.trim().is_empty() {
            ann.id = self.next_id();
        } else if self.find_annotation(&ann.id).is_some() {
            return Err(Error::DuplicateId(ann.id));
        }
        let id = ann.id.clone();
        self.annotations.push(ann);
        Ok(id)
    }

    /// Validates an annotation on the code at its step.
    pub fn validate_annotation(&self, a: &Annotation) -> Result<(), Error> {
        self.validate_shape_in(&a.shape, &a.file, a.step)
    }

    pub fn update(&mut self, mut ann: Annotation) -> Result<(), Error> {
        check_step(ann.step, &annotation_what(&ann.id))?;
        self.resolve_file(&mut ann)?;
        self.validate_shape_in(&ann.shape, &ann.file, ann.step)?;
        sanitize_texts(&mut ann.shape);
        let slot = self
            .annotations
            .iter_mut()
            .find(|a| a.id == ann.id)
            .ok_or_else(|| Error::NotFound(ann.id.clone()))?;
        *slot = ann;
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
            .ok_or_else(|| Error::NoMatch(format!("start marker «{start}»")))?;
        // Same line after the start text, or a later line.
        if let Some(e) = self.find_text_after(end, s.line, s.end) {
            return Ok((s.line, e.line));
        }
        if self.find_text(end, 1, None).is_some() {
            return Err(Error::Invalid(format!(
                "end marker «{end}» appears only before the start marker «{start}» (line {})",
                s.line
            )));
        }
        Err(Error::NoMatch(format!(
            "end marker «{end}» (after line {})",
            s.line
        )))
    }

    /// The first hit of `text` that starts at or after grapheme `col` of
    /// `line`, or on a later line.
    pub fn find_text_after(&self, text: &str, line: u32, col: u32) -> Option<TextSpan> {
        if text.is_empty() {
            return None;
        }
        for l in line..=self.last_line() {
            let Some(t) = self.line_text(l) else {
                continue;
            };
            let from = if l == line {
                grapheme_byte_offset(t, col).unwrap_or(t.len())
            } else {
                0
            };
            if let Some(pos) = t[from..].find(text) {
                let byte_start = from + pos;
                return Some(TextSpan {
                    line: l,
                    start: grapheme_index(t, byte_start),
                    end: grapheme_index(t, byte_start + text.len()),
                });
            }
        }
        None
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
        let search_line = |line: u32, seen: &mut u32| -> Option<TextSpan> {
            let t = self.line_text(line)?;
            let mut from = 0;
            while let Some(pos) = t[from..].find(text) {
                let byte_start = from + pos;
                *seen += 1;
                if *seen == occurrence {
                    return Some(TextSpan {
                        line,
                        start: grapheme_index(t, byte_start),
                        end: grapheme_index(t, byte_start + text.len()),
                    });
                }
                from = byte_start + text.len().max(1);
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
