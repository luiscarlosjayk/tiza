//! Document model: source code + annotations anchored to the text.
//!
//! Anchors live in code coordinates (absolute 1-based line and 0-based
//! grapheme offset), never in screen cells. The conversion to cells happens
//! only when rendering (see `layout`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::error::Error;

/// Schema version of the `.codedraw` file.
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
}

/// Where the code of a `.codedraw` comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
#[serde(untagged)]
pub enum CodeSource {
    /// Copy of the code inside the document: portable, but frozen.
    Embedded(Vec<String>),
    /// Reference to an existing file. `file` may be absolute or relative to the
    /// `.codedraw` directory; `lines` is an optional range `"12-48"` or `"7"`.
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
    #[serde(flatten)]
    pub shape: Shape,
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
        self.code_src = CodeSource::Embedded(self.code.clone());
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self.code_src, CodeSource::Embedded(_))
    }

    /// Keeps `code_src` consistent with `code` before serializing.
    pub fn sync_code_src(&mut self) {
        if let CodeSource::Embedded(lines) = &mut self.code_src {
            *lines = self.code.clone();
        }
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

    /// Highest step used by any annotation (at least 1).
    pub fn max_step(&self) -> u32 {
        self.annotations
            .iter()
            .map(|a| a.step)
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

    /// Adds an annotation. If `id` is empty a new one is assigned. Returns the id.
    pub fn add(&mut self, mut ann: Annotation) -> Result<String, Error> {
        self.validate_shape(&ann.shape)?;
        sanitize_texts(&mut ann.shape);
        if ann.id.trim().is_empty() {
            ann.id = self.next_id();
        } else if self.find_annotation(&ann.id).is_some() {
            return Err(Error::DuplicateId(ann.id));
        }
        if ann.step == 0 {
            ann.step = 1;
        }
        let id = ann.id.clone();
        self.annotations.push(ann);
        Ok(id)
    }

    pub fn update(&mut self, mut ann: Annotation) -> Result<(), Error> {
        self.validate_shape(&ann.shape)?;
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
                    let start = grapheme_index(t, byte_start);
                    let end = grapheme_index(t, byte_start + text.len());
                    return Some(TextSpan { line, start, end });
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
