//! Local API protocol (Unix socket, NDJSON), shared by the TUI, CLI and MCP.
//!
//! Design borrowed from herdr: one request per line `{id, method, params}`,
//! response `{id, result}` or `{id, error: {code, message}}`, dotted method
//! names. Clients ignore unknown fields.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Anchor, Annotation, Document, MarkStyle, Placement, Shape, TextSpan};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShapeSpec {
    Highlight {
        at: Where,
        #[serde(default)]
        label: Option<String>,
    },
    Mark {
        kind: String,
        at: Where,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        style: MarkStyle,
    },
    Box {
        from_line: u32,
        to_line: u32,
        #[serde(default)]
        label: Option<String>,
    },
    Arrow {
        from: Where,
        to: Where,
        #[serde(default)]
        label: Option<String>,
    },
    Note {
        line: u32,
        text: String,
    },
    Callout {
        from_line: u32,
        #[serde(default)]
        to_line: Option<u32>,
        text: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        placement: Placement,
        #[serde(default)]
        width: Option<usize>,
    },
    Spotlight {
        #[serde(default)]
        from_line: Option<u32>,
        #[serde(default)]
        to_line: Option<u32>,
        #[serde(default)]
        ids: Vec<String>,
        #[serde(default)]
        until: Option<u32>,
    },
}

impl ShapeSpec {
    pub fn resolve(&self, doc: &Document) -> Result<Shape, crate::Error> {
        Ok(match self {
            ShapeSpec::Highlight { at, label } => Shape::Highlight {
                span: at.resolve_span(doc)?,
                label: label.clone(),
            },
            ShapeSpec::Mark {
                kind,
                at,
                label,
                style,
            } => Shape::Mark {
                kind: kind.clone(),
                style: style.clone(),
                span: at.resolve_span(doc)?,
                label: label.clone(),
            },
            ShapeSpec::Box {
                from_line,
                to_line,
                label,
            } => Shape::Box {
                from_line: *from_line,
                to_line: *to_line,
                label: label.clone(),
            },
            ShapeSpec::Arrow { from, to, label } => Shape::Arrow {
                from: from.resolve_anchor(doc)?,
                to: to.resolve_anchor(doc)?,
                label: label.clone(),
            },
            ShapeSpec::Note { line, text } => Shape::Note {
                line: *line,
                text: text.clone(),
            },
            ShapeSpec::Callout {
                from_line,
                to_line,
                text,
                title,
                placement,
                width,
            } => Shape::Callout {
                from_line: *from_line,
                to_line: *to_line,
                text: text.clone(),
                title: title.clone(),
                placement: *placement,
                width: *width,
            },
            ShapeSpec::Spotlight {
                from_line,
                to_line,
                ids,
                until,
            } => Shape::Spotlight {
                from_line: *from_line,
                to_line: to_line.or(*from_line),
                ids: ids.clone(),
                until: *until,
            },
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdParams {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepParams {
    pub step: u32,
}

/// Response of `scene.get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub revision: u64,
    pub step: u32,
    pub max_step: u32,
    pub document: Document,
}

/// Response of `status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub app: String,
    pub version: String,
    pub protocol: u32,
    pub revision: u64,
    pub file: Option<String>,
    pub lines: usize,
    pub annotations: usize,
    pub step: u32,
    pub max_step: u32,
    pub cols: u16,
    pub rows: u16,
}

/// Result of a mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    ("code.get", "Code with line numbers (text)"),
    (
        "scene.get",
        "Full document with annotations and current step",
    ),
    (
        "annotation.add",
        "Adds an annotation (highlight, mark, box, arrow, note, callout, spotlight)",
    ),
    (
        "annotation.update",
        "Changes the step, label or shape of an annotation",
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
    ("step.set", "Sets the current presentation step"),
    ("step.next", "Advances one step"),
    ("step.prev", "Goes back one step"),
    ("render.text", "Returns the current canvas as plain text"),
];

/// Renders an annotation in the `scene.list` format.
pub fn describe(a: &Annotation) -> String {
    match &a.shape {
        Shape::Highlight { span, label } => format!(
            "{} [step {}] highlight L{} {}..{}{}",
            a.id,
            a.step,
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
        Shape::Arrow { from, to, label } => format!(
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
