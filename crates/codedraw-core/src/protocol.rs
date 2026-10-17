//! Local API protocol (Unix socket, NDJSON), shared by the TUI, CLI and MCP.
//!
//! Design borrowed from herdr: one request per line `{id, method, params}`,
//! response `{id, result}` or `{id, error: {code, message}}`, dotted method
//! names. Clients ignore unknown fields.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{
    Anchor, Annotation, BorderStyle, Document, Emphasis, Head, LineStyle, MarkStyle, Placement,
    RangeMarkers, Route, Shape, Stroke, TextSpan, ThemeName,
};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Request {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Response {
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl Response {
    pub fn ok(id: Value, result: impl Serialize) -> Self {
        Self {
            id,
            result: Some(serde_json::to_value(result).unwrap_or(Value::Null)),
            error: None,
        }
    }
    pub fn err(id: Value, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

/// Where to anchor something: explicit coordinates or a text search.
/// An agent rarely knows the exact column; `find` is more robust.
#[derive(Debug, Clone, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct Where {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
    /// Text to search for in the code; with `line`, that line is searched first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub find: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<u32>,
    /// With `line` and `col`: end of the span (exclusive). Without it the span runs to the end of the line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<u32>,
}

impl Where {
    pub fn resolve_span(&self, doc: &Document) -> Result<TextSpan, crate::Error> {
        if let Some(f) = &self.find {
            return doc
                .find_text(f, self.occurrence.unwrap_or(1), self.line)
                .ok_or_else(|| crate::Error::TextNotFound(f.clone()));
        }
        let line = self
            .line
            .ok_or_else(|| crate::Error::Invalid("missing `line` or `find`".into()))?;
        let start = self.col.unwrap_or(0);
        Ok(TextSpan {
            line,
            start,
            end: self.end.unwrap_or_else(|| doc.line_len(line)),
        })
    }

    pub fn resolve_anchor(&self, doc: &Document) -> Result<Anchor, crate::Error> {
        if let Some(f) = &self.find {
            let s = doc
                .find_text(f, self.occurrence.unwrap_or(1), self.line)
                .ok_or_else(|| crate::Error::TextNotFound(f.clone()))?;
            return Ok(Anchor {
                line: s.line,
                col: s.start,
            });
        }
        let line = self
            .line
            .ok_or_else(|| crate::Error::Invalid("missing `line` or `find`".into()))?;
        Ok(Anchor {
            line,
            col: self.col.unwrap_or(0),
        })
    }
}

/// Parameters of `annotation.add`: a shape described in an agent-friendly way.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShapeSpec {
    Highlight {
        at: Where,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        emphasis: Emphasis,
    },
    Mark {
        kind: String,
        at: Where,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        style: MarkStyle,
        #[serde(default)]
        emphasis: Emphasis,
    },
    Box {
        #[serde(default)]
        from_line: Option<u32>,
        #[serde(default)]
        to_line: Option<u32>,
        /// Text markers instead of line numbers (t-097): the box runs from
        /// the line of `find_start` to the line of `find_end`.
        #[serde(default)]
        find_start: Option<String>,
        #[serde(default)]
        find_end: Option<String>,
        #[serde(default)]
        label: Option<String>,
        #[serde(flatten, default)]
        frame: BorderStyle,
    },
    Arrow {
        from: Where,
        to: Where,
        #[serde(default)]
        label: Option<String>,
        #[serde(flatten, default)]
        line: LineStyle,
    },
    Note {
        line: u32,
        text: String,
    },
    Callout {
        #[serde(default)]
        from_line: Option<u32>,
        #[serde(default)]
        to_line: Option<u32>,
        #[serde(default)]
        find_start: Option<String>,
        #[serde(default)]
        find_end: Option<String>,
        text: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        placement: Placement,
        #[serde(default)]
        width: Option<usize>,
        #[serde(flatten, default)]
        frame: BorderStyle,
    },
    Spotlight {
        #[serde(default)]
        from_line: Option<u32>,
        #[serde(default)]
        to_line: Option<u32>,
        #[serde(default)]
        find_start: Option<String>,
        #[serde(default)]
        find_end: Option<String>,
        #[serde(default)]
        ids: Vec<String>,
        #[serde(default)]
        until: Option<u32>,
    },
}

/// A line range given either as numbers or as two text markers.
fn resolve_range(
    doc: &Document,
    what: &str,
    from_line: Option<u32>,
    to_line: Option<u32>,
    find_start: Option<&str>,
    find_end: Option<&str>,
) -> Result<(u32, u32, Option<RangeMarkers>), crate::Error> {
    match (find_start, find_end) {
        (Some(s), Some(e)) => {
            if from_line.is_some() || to_line.is_some() {
                return Err(crate::Error::Invalid(format!(
                    "{what}: give either from_line/to_line or find_start/find_end, not both"
                )));
            }
            let (a, b) = doc.resolve_markers(s, e)?;
            Ok((
                a,
                b,
                Some(RangeMarkers {
                    start: s.to_string(),
                    end: e.to_string(),
                }),
            ))
        }
        (Some(_), None) | (None, Some(_)) => Err(crate::Error::Invalid(format!(
            "{what}: find_start and find_end go together"
        ))),
        (None, None) => {
            let a = from_line.ok_or_else(|| {
                crate::Error::Invalid(format!(
                    "{what}: from_line (or find_start/find_end) is required"
                ))
            })?;
            Ok((a, to_line.unwrap_or(a), None))
        }
    }
}

impl ShapeSpec {
    pub fn resolve(&self, doc: &Document) -> Result<Shape, crate::Error> {
        self.resolve_with_markers(doc).map(|(s, _)| s)
    }

    /// Resolves the spec and returns the markers the range came from, if any.
    pub fn resolve_with_markers(
        &self,
        doc: &Document,
    ) -> Result<(Shape, Option<RangeMarkers>), crate::Error> {
        let mut markers = None;
        let shape = match self {
            ShapeSpec::Highlight {
                at,
                label,
                emphasis,
            } => Shape::Highlight {
                span: at.resolve_span(doc)?,
                label: label.clone(),
                emphasis: *emphasis,
            },
            ShapeSpec::Mark {
                kind,
                at,
                label,
                style,
                emphasis,
            } => Shape::Mark {
                kind: kind.clone(),
                style: style.clone(),
                span: at.resolve_span(doc)?,
                label: label.clone(),
                emphasis: *emphasis,
            },
            ShapeSpec::Box {
                from_line,
                to_line,
                find_start,
                find_end,
                label,
                frame,
            } => {
                let (a, b, m) = resolve_range(
                    doc,
                    "box",
                    *from_line,
                    *to_line,
                    find_start.as_deref(),
                    find_end.as_deref(),
                )?;
                markers = m;
                Shape::Box {
                    from_line: a,
                    to_line: b,
                    label: label.clone(),
                    frame: *frame,
                }
            }
            ShapeSpec::Arrow {
                from,
                to,
                label,
                line,
            } => Shape::Arrow {
                from: from.resolve_anchor(doc)?,
                to: to.resolve_anchor(doc)?,
                label: label.clone(),
                line: *line,
            },
            ShapeSpec::Note { line, text } => Shape::Note {
                line: *line,
                text: text.clone(),
            },
            ShapeSpec::Callout {
                from_line,
                to_line,
                find_start,
                find_end,
                text,
                title,
                placement,
                width,
                frame,
            } => {
                let (a, b, m) = resolve_range(
                    doc,
                    "callout",
                    *from_line,
                    *to_line,
                    find_start.as_deref(),
                    find_end.as_deref(),
                )?;
                markers = m;
                Shape::Callout {
                    from_line: a,
                    // A callout keeps `to_line` only when a range was given.
                    to_line: (b != a || to_line.is_some() || m_is_some(&markers)).then_some(b),
                    text: text.clone(),
                    title: title.clone(),
                    placement: *placement,
                    width: *width,
                    frame: *frame,
                }
            }
            ShapeSpec::Spotlight {
                from_line,
                to_line,
                find_start,
                find_end,
                ids,
                until,
            } => {
                let (a, b) = if find_start.is_some() || find_end.is_some() {
                    let (a, b, m) = resolve_range(
                        doc,
                        "spotlight",
                        *from_line,
                        *to_line,
                        find_start.as_deref(),
                        find_end.as_deref(),
                    )?;
                    markers = m;
                    (Some(a), Some(b))
                } else {
                    (*from_line, to_line.or(*from_line))
                };
                Shape::Spotlight {
                    from_line: a,
                    to_line: b,
                    ids: ids.clone(),
                    until: *until,
                }
            }
        };
        Ok((shape, markers))
    }
}

fn m_is_some(m: &Option<RangeMarkers>) -> bool {
    m.is_some()
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddParams {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub step: Option<u32>,
    /// Palette name (orange, blue, green, purple, pink, cyan, yellow, red, gray) or `#rrggbb`.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(flatten)]
    pub shape: ShapeSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateParams {
    pub id: String,
    #[serde(default)]
    pub step: Option<u32>,
    /// New color; an empty string returns to the automatic color.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    /// If present, replaces the whole shape.
    #[serde(default)]
    pub shape: Option<ShapeSpec>,
    /// Moves the annotation, keeping its type, kind and style (`move`).
    #[serde(default, rename = "move", skip_serializing_if = "Option::is_none")]
    pub mv: Option<MoveSpec>,
    /// Arrows: new stroke (solid, dashed, dotted, thick, double).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    /// Arrows: new corner style (rounded, sharp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<Route>,
    /// Arrows: new heads; each end optional (`{"start": "dot"}` keeps the other).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heads: Option<HeadsPatch>,
    /// Boxes and callouts: new border stroke.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<Stroke>,
    /// Boxes and callouts: new corners.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corners: Option<Route>,
    /// Boxes and callouts: tint the inside (`true`), do not (`false`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<bool>,
    /// Highlights and marks: how the span is drawn (fill, underline, brackets, strike).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emphasis: Option<Emphasis>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HeadsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Head>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<Head>,
}

impl UpdateParams {
    pub fn has_line_style(&self) -> bool {
        self.stroke.is_some() || self.route.is_some() || self.heads.is_some()
    }
    pub fn has_border_style(&self) -> bool {
        self.border.is_some() || self.corners.is_some() || self.fill.is_some()
    }
}

/// Partial re-anchoring of an existing annotation. Which fields apply depends
/// on the shape: `at` for highlights, marks and notes; `from`/`to` for arrows;
/// `from_line`/`to_line` for boxes, callouts, spotlights and notes. A field
/// that does not apply to the annotation's type is an `invalid_argument`
/// error, never silently ignored.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MoveSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Where>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Where>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Where>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_line: Option<u32>,
}

impl MoveSpec {
    pub fn is_empty(&self) -> bool {
        self.at.is_none()
            && self.from.is_none()
            && self.to.is_none()
            && self.from_line.is_none()
            && self.to_line.is_none()
    }

    /// Applies the move to `shape`. Validation of the resulting ranges is left
    /// to `Document::update`.
    pub fn apply(&self, shape: &mut Shape, doc: &Document) -> Result<(), crate::Error> {
        let kind = shape.kind_name();
        let reject = |field: &str, hint: &str| {
            Err(crate::Error::Invalid(format!(
                "`{field}` does not apply to a {kind}; use {hint}"
            )))
        };
        if self.is_empty() {
            return Err(crate::Error::Invalid(
                "`move` needs at, from, to, from_line or to_line".into(),
            ));
        }
        match shape {
            Shape::Highlight { span, .. } | Shape::Mark { span, .. } => {
                if self.from.is_some() || self.to.is_some() {
                    return reject("from/to", "`at`");
                }
                if self.from_line.is_some() || self.to_line.is_some() {
                    return reject("from_line/to_line", "`at`");
                }
                if let Some(at) = &self.at {
                    *span = at.resolve_span(doc)?;
                }
            }
            Shape::Note { line, .. } => {
                if self.from.is_some() || self.to.is_some() {
                    return reject("from/to", "`at` or `from_line`");
                }
                if self.to_line.is_some() {
                    return reject("to_line", "`at` or `from_line`");
                }
                if let Some(at) = &self.at {
                    *line = at.resolve_anchor(doc)?.line;
                }
                if let Some(l) = self.from_line {
                    *line = l;
                }
            }
            Shape::Box {
                from_line, to_line, ..
            } => {
                if self.at.is_some() || self.from.is_some() || self.to.is_some() {
                    return reject("at/from/to", "`from_line`/`to_line`");
                }
                if let Some(l) = self.from_line {
                    *from_line = l;
                }
                if let Some(l) = self.to_line {
                    *to_line = l;
                }
            }
            Shape::Arrow { from, to, .. } => {
                if self.at.is_some() {
                    return reject("at", "`from`/`to`");
                }
                if self.from_line.is_some() || self.to_line.is_some() {
                    return reject("from_line/to_line", "`from`/`to`");
                }
                if let Some(w) = &self.from {
                    *from = w.resolve_anchor(doc)?;
                }
                if let Some(w) = &self.to {
                    *to = w.resolve_anchor(doc)?;
                }
            }
            Shape::Callout {
                from_line, to_line, ..
            } => {
                if self.at.is_some() || self.from.is_some() || self.to.is_some() {
                    return reject("at/from/to", "`from_line`/`to_line`");
                }
                if let Some(l) = self.from_line {
                    *from_line = l;
                }
                if let Some(l) = self.to_line {
                    *to_line = Some(l);
                }
            }
            Shape::Spotlight {
                from_line, to_line, ..
            } => {
                if self.at.is_some() || self.from.is_some() || self.to.is_some() {
                    return reject("at/from/to", "`from_line`/`to_line`");
                }
                if let Some(l) = self.from_line {
                    *from_line = Some(l);
                    if to_line.is_none() {
                        *to_line = Some(l);
                    }
                }
                if let Some(l) = self.to_line {
                    *to_line = Some(l);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IdParams {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StepParams {
    pub step: u32,
}

/// Response of `scene.get`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Scene {
    pub revision: u64,
    pub step: u32,
    pub max_step: u32,
    pub document: Document,
}

/// Params of `scene.put`: the document itself (a `.codedraw` object with the
/// code embedded; file references cannot be resolved by the instance).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PutParams {
    pub document: Document,
}

/// Params of `scene.patch`. Every field optional; anything else is an error.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PatchParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeName>,
    /// `[vertical, horizontal]`; `null` restores the default.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub padding: Option<Option<(u16, u16)>>,
    /// `null` clears the title.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub title: Option<Option<String>>,
    /// Styles merged by name (an existing name is replaced).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub styles: Option<std::collections::BTreeMap<String, MarkStyle>>,
    /// Full annotations merged by id: same id replaces, new id adds,
    /// `{"id": "a1", "deleted": true}` removes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Vec<Value>>,
}

/// Distinguishes an absent field (`None`) from an explicit `null`
/// (`Some(None)`) in a patch.
fn present<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Result of `scene.put` and `scene.patch`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SceneChanged {
    pub revision: u64,
    pub lines: usize,
    pub annotations: usize,
    pub step: u32,
    pub max_step: u32,
    /// `scene.patch`: ids added, replaced and deleted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaced: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<String>,
}

/// Response of `status`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Status {
    pub app: String,
    pub version: String,
    /// Git commit the instance was built from (`unknown` outside a checkout).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub build: String,
    pub protocol: u32,
    pub revision: u64,
    pub file: Option<String>,
    pub lines: usize,
    pub annotations: usize,
    pub step: u32,
    pub max_step: u32,
    pub cols: u16,
    pub rows: u16,
    /// Human-readable warnings about the viewport (small terminal, clipped code).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Supervised mode: mutations wait for the presenter's approval.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub supervised: bool,
    /// Proposals waiting for a decision.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pending: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Result of a mutation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Mutated {
    pub id: String,
    pub revision: u64,
}

/// Supported methods, for `api.methods` and for the help.
pub const METHODS: &[(&str, &str)] = &[
    ("ping", "Checks that the instance responds"),
    (
        "status",
        "Instance summary: file, lines, annotations, step, terminal size",
    ),
    ("api.methods", "This list"),
    (
        "api.schema",
        "JSON Schema of every method (params and result) and of the request/response lines",
    ),
    ("code.get", "Code with line numbers (text)"),
    (
        "scene.get",
        "Full document with annotations and current step (code embedded)",
    ),
    ("scene.list", "One line per annotation, in document order"),
    (
        "annotation.add",
        "Adds an annotation (highlight, mark, box, arrow, note, callout, spotlight); arrows take stroke, route and heads",
    ),
    (
        "annotation.update",
        "Changes the step, color, label, shape, arrow style (stroke, route, heads) of an annotation, or moves it (`move`: at, from, to, from_line, to_line) keeping its type and style",
    ),
    ("annotation.delete", "Deletes an annotation by id"),
    (
        "style.set",
        "Defines or updates a reusable mark style (name, glyph, legend, color)",
    ),
    (
        "style.list",
        "Lists document styles, built-in kinds and aliases",
    ),
    (
        "scene.clear",
        "Removes every annotation (or only those of `step`); can be undone in the TUI",
    ),
    (
        "scene.put",
        "Replaces the whole live document (code embedded, annotations, step, theme, styles…) atomically; undoable in the TUI",
    ),
    (
        "scene.patch",
        "Partial change: step, theme, padding, title, styles (merged by name) and annotations merged by id (last write wins; `{\"id\": …, \"deleted\": true}` removes one); unknown keys are rejected",
    ),
    ("step.set", "Sets the current presentation step"),
    ("step.next", "Advances one step"),
    ("step.prev", "Goes back one step"),
    (
        "focus.set",
        "Dims everything outside `from_line..to_line`; without lines, clears the focus",
    ),
    ("theme.set", "Switches the theme: `dark` or `light`"),
    ("render.text", "Returns the current canvas as plain text"),
    (
        "follow.set",
        "Turns follow mode on or off (`enabled`): reload the code when the source file is saved, re-anchoring the annotations; fails when the code is embedded",
    ),
    (
        "follow.status",
        "Follow mode state: enabled, path, reloads, last_reload (HH:MM:SSZ), missing",
    ),
    (
        "supervise.set",
        "Turns supervised mode on or off (`enabled`): while on, every mutating request waits for the presenter's y/n in the TUI (60 s, then `rejected`); turning it off rejects what is pending",
    ),
    (
        "supervise.status",
        "Supervised mode state: enabled, pending proposals, accepted/rejected counts",
    ),
];

/// Renders an annotation in the `scene.list` format.
pub fn describe(a: &Annotation) -> String {
    let base = describe_shape(a);
    match &a.markers {
        Some(m) => format!("{base} @«{}»..@«{}»", m.start, m.end),
        None => base,
    }
}

fn describe_shape(a: &Annotation) -> String {
    match &a.shape {
        Shape::Highlight {
            span,
            label,
            emphasis,
        } => format!(
            "{} [step {}] highlight{} L{} {}..{}{}",
            a.id,
            a.step,
            if emphasis.is_default() {
                String::new()
            } else {
                format!(" ({})", emphasis.name())
            },
            span.line,
            span.start,
            span.end,
            label
                .as_ref()
                .map(|l| format!(" «{l}»"))
                .unwrap_or_default()
        ),
        Shape::Mark {
            kind, span, label, ..
        } => format!(
            "{} [step {}] mark {} L{} {}..{}{}",
            a.id,
            a.step,
            kind,
            span.line,
            span.start,
            span.end,
            label
                .as_ref()
                .map(|l| format!(" «{l}»"))
                .unwrap_or_default()
        ),
        Shape::Box {
            from_line,
            to_line,
            label,
            ..
        } => format!(
            "{} [step {}] box L{}-{}{}",
            a.id,
            a.step,
            from_line,
            to_line,
            label
                .as_ref()
                .map(|l| format!(" «{l}»"))
                .unwrap_or_default()
        ),
        Shape::Arrow {
            from, to, label, ..
        } => format!(
            "{} [step {}] arrow L{}:{} → L{}:{}{}",
            a.id,
            a.step,
            from.line,
            from.col,
            to.line,
            to.col,
            label
                .as_ref()
                .map(|l| format!(" «{l}»"))
                .unwrap_or_default()
        ),
        Shape::Note { line, text } => {
            format!("{} [step {}] note L{} «{}»", a.id, a.step, line, text)
        }
        Shape::Callout {
            from_line,
            to_line,
            text,
            title,
            ..
        } => format!(
            "{} [step {}] callout L{}-{}{} «{}»",
            a.id,
            a.step,
            from_line,
            to_line.unwrap_or(*from_line),
            title.as_ref().map(|t| format!(" {t}:")).unwrap_or_default(),
            if text.len() > 40 {
                format!(
                    "{}…",
                    &text[..text
                        .char_indices()
                        .nth(40)
                        .map(|(i, _)| i)
                        .unwrap_or(text.len())]
                )
            } else {
                text.clone()
            }
        ),
        Shape::Spotlight {
            from_line,
            to_line,
            ids,
            until,
        } => format!(
            "{} [step {}{}] spotlight{}{}",
            a.id,
            a.step,
            until.map(|u| format!("-{u}")).unwrap_or_default(),
            from_line
                .map(|f| format!(" L{}-{}", f, to_line.unwrap_or(f)))
                .unwrap_or_default(),
            if ids.is_empty() {
                String::new()
            } else {
                format!(" [{}]", ids.join(" "))
            }
        ),
    }
}
