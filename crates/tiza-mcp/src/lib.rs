//! Internal crate of [Tiza](https://github.com/luiscarlosjayk/tiza),
//! published only so that `cargo install tiza` works: it has no stable
//! API. Use the `tiza` binary.
//!
//! MCP → local API bridge. Each tool asks the live instance through its
//! socket, most of them with exactly one method (`list_annotations`,
//! `export_image` and the `tiza://render.md` resource use two). That way an
//! agent and the presenter do the same thing, and the TUI is the only one
//! that touches the document.

use std::path::PathBuf;

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, Implementation, ListResourcesResult, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
        ResourceContents, ServerCapabilities, ServerConfig,
    },
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde_json::{Value, json};

/// Where to anchor something in the code. The most robust option for an agent is `find`:
/// the exact text to search for (optionally narrowed to `line` and to the `occurrence`-th match).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct At {
    #[schemars(
        description = "Absolute line number (1-based), as shown in the TUI. Optional when using `find`."
    )]
    pub line: Option<u32>,
    #[schemars(
        description = "Grapheme offset within the line (0-based). Rarely needed; prefer `find`."
    )]
    pub col: Option<u32>,
    #[schemars(
        description = "Exact text to search for in the code (e.g. \"takes(s)\"). With `line`, that line is searched first."
    )]
    pub find: Option<String>,
    #[schemars(description = "Which match of `find` to use (1-based). Defaults to the first.")]
    pub occurrence: Option<u32>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct InlineStyle {
    #[schemars(description = "Glyph, one or two cells wide (box-drawing, arrows, emoji)")]
    pub glyph: Option<String>,
    #[schemars(description = "Short legend shown after the glyph")]
    pub legend: Option<String>,
    #[schemars(description = "Palette name or #rrggbb")]
    pub color: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DefineStyleParams {
    #[schemars(description = "Name to use later as `kind` (e.g. \"hot\")")]
    pub name: String,
    pub glyph: Option<String>,
    pub legend: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Empty {}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BoxParams {
    #[schemars(
        description = "First line enclosed by the box (1-based). Or give find_start/find_end instead."
    )]
    pub from_line: Option<u32>,
    #[schemars(description = "Last line enclosed by the box (inclusive); defaults to from_line")]
    pub to_line: Option<u32>,
    #[schemars(
        description = "Text on the first line of the box (e.g. a comment marker `// begin` or code like `fn takes`), instead of from_line. Must be given with find_end."
    )]
    pub find_start: Option<String>,
    #[schemars(
        description = "Text on the last line of the box, searched after find_start (same line allowed). Fails with text_not_found naming the marker."
    )]
    pub find_end: Option<String>,
    #[schemars(description = "Label drawn on the top edge")]
    pub label: Option<String>,
    #[schemars(
        description = "Presentation step at which it appears (1-based). Defaults to the current step."
    )]
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray or #rrggbb. Arrows without a color rotate through the palette."
    )]
    pub color: Option<String>,
    #[schemars(description = "Custom id (e.g. \"box-takes\"); if omitted, a1, a2… is assigned")]
    pub id: Option<String>,
    #[schemars(description = "Border stroke: solid (default), dashed, dotted, thick, double")]
    pub border: Option<String>,
    #[schemars(description = "Corners: rounded (default) or sharp")]
    pub corners: Option<String>,
    #[schemars(
        description = "Tint the inside: true/false (boxes default to true, callouts to false)"
    )]
    pub fill: Option<bool>,
    #[schemars(
        description = "Optional. Experimental. The enter effect when the step goes forward: draw (the stroke is drawn) or fade. Draw needs a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike."
    )]
    pub effect: Option<tiza_core::effects::Effect>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ArrowParams {
    pub from: At,
    pub to: At,
    #[schemars(description = "Text next to the arrow head (e.g. \"move\")")]
    pub label: Option<String>,
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray or #rrggbb. Arrows without a color rotate through the palette."
    )]
    pub color: Option<String>,
    pub id: Option<String>,
    #[schemars(description = "Line stroke: solid (default), dashed, dotted, thick, double")]
    pub stroke: Option<String>,
    #[schemars(description = "Corners: rounded (default) or sharp")]
    pub route: Option<String>,
    #[schemars(description = "Head at the source: none (default), arrow, dot, bar, open")]
    pub head_start: Option<String>,
    #[schemars(
        description = "Head at the target: arrow (default), none, dot, bar, open. none+none draws a plain connector."
    )]
    pub head_end: Option<String>,
    #[schemars(
        description = "Optional. Experimental. The enter effect when the step goes forward: draw (the stroke is drawn) or fade. Draw needs a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike."
    )]
    pub effect: Option<tiza_core::effects::Effect>,
}

/// An argument that the tool refuses itself, said as a refusal of the
/// instance is said: the message, then the code in parentheses, so that an
/// agent can tell the cases apart.
fn invalid_argument(msg: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(format!("{msg} (invalid_argument)"), None)
}

/// Validates box/callout border strings and returns the protocol fields.
fn border_style_json(
    border: &Option<String>,
    corners: &Option<String>,
    fill: Option<bool>,
) -> Result<serde_json::Map<String, Value>, McpError> {
    use tiza_core::model::{Route, Stroke};
    let mut m = serde_json::Map::new();
    if let Some(b) = border {
        let v = Stroke::parse(b).ok_or_else(|| {
            invalid_argument(format!("unknown border (use {})", Stroke::NAMES.join(", ")))
        })?;
        m.insert("border".into(), serde_json::to_value(v).unwrap());
    }
    if let Some(c) = corners {
        let v = Route::parse(c).ok_or_else(|| {
            invalid_argument(format!("unknown corners (use {})", Route::NAMES.join(", ")))
        })?;
        m.insert("corners".into(), serde_json::to_value(v).unwrap());
    }
    if let Some(f) = fill {
        m.insert("fill".into(), Value::Bool(f));
    }
    Ok(m)
}

/// Validates an emphasis name for highlights and marks.
fn emphasis_json(e: &Option<String>) -> Result<Option<Value>, McpError> {
    match e {
        None => Ok(None),
        Some(e) => tiza_core::model::Emphasis::parse(e)
            .map(|v| Some(serde_json::to_value(v).unwrap()))
            .ok_or_else(|| {
                invalid_argument(format!(
                    "unknown emphasis (use {})",
                    tiza_core::model::Emphasis::NAMES.join(", ")
                ))
            }),
    }
}

/// Validates the arrow style strings and returns the protocol fields
/// (`stroke`, `route`, `heads{start?,end?}`).
fn line_style_json(
    stroke: &Option<String>,
    route: &Option<String>,
    start: &Option<String>,
    end: &Option<String>,
) -> Result<serde_json::Map<String, Value>, McpError> {
    use tiza_core::model::{Head, Route, Stroke};
    let mut m = serde_json::Map::new();
    if let Some(s) = stroke {
        let v = Stroke::parse(s).ok_or_else(|| {
            invalid_argument(format!("unknown stroke (use {})", Stroke::NAMES.join(", ")))
        })?;
        m.insert("stroke".into(), serde_json::to_value(v).unwrap());
    }
    if let Some(r) = route {
        let v = Route::parse(r).ok_or_else(|| {
            invalid_argument(format!("unknown route (use {})", Route::NAMES.join(", ")))
        })?;
        m.insert("route".into(), serde_json::to_value(v).unwrap());
    }
    if start.is_some() || end.is_some() {
        let mut h = serde_json::Map::new();
        for (key, val) in [("start", start), ("end", end)] {
            if let Some(v) = val {
                let hv = Head::parse(v).ok_or_else(|| {
                    invalid_argument(format!(
                        "unknown {key} head (use {})",
                        Head::NAMES.join(", ")
                    ))
                })?;
                h.insert(key.into(), serde_json::to_value(hv).unwrap());
            }
        }
        m.insert("heads".into(), Value::Object(h));
    }
    Ok(m)
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NoteParams {
    #[schemars(description = "Line the note is anchored to")]
    pub line: u32,
    #[schemars(
        description = "Note text (a short sentence: goes in the right margin or below the line)"
    )]
    pub text: String,
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray or #rrggbb. Arrows without a color rotate through the palette."
    )]
    pub color: Option<String>,
    pub id: Option<String>,
    #[schemars(
        description = "Optional. Experimental. The enter effect when the step goes forward: draw (the stroke is drawn) or fade. Draw needs a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike."
    )]
    pub effect: Option<tiza_core::effects::Effect>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct HighlightParams {
    pub at: At,
    pub label: Option<String>,
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray or #rrggbb. Arrows without a color rotate through the palette."
    )]
    pub color: Option<String>,
    pub id: Option<String>,
    #[schemars(
        description = "How the span is drawn: fill (background tint, default), underline (▔ row under it), brackets (⌈ ⌉ beside it), strike (struck through)"
    )]
    pub emphasis: Option<String>,
    #[schemars(
        description = "Optional. Experimental. The enter effect when the step goes forward: draw (the stroke is drawn) or fade. Draw needs a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike."
    )]
    pub effect: Option<tiza_core::effects::Effect>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MarkParams {
    #[schemars(
        description = "Built-in kind (info, note, warn, error, ok, question, important, read, write, move, new, drop, scope, flow, todo; aliases like borrow/borrow_mut/lifetime work), a style defined with define_style, or any name when `style` carries a glyph."
    )]
    pub kind: String,
    #[schemars(
        description = "Inline one-off style for this mark: {glyph, legend, color}. Overrides the kind's fields."
    )]
    pub style: Option<InlineStyle>,
    pub at: At,
    #[schemars(description = "Extra text after the legend (e.g. \"s goes into t\")")]
    pub label: Option<String>,
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray or #rrggbb. Arrows without a color rotate through the palette."
    )]
    pub color: Option<String>,
    pub id: Option<String>,
    #[schemars(
        description = "How the span is drawn: fill (background tint, default), underline (▔ row under it), brackets (⌈ ⌉ beside it), strike (struck through)"
    )]
    pub emphasis: Option<String>,
    #[schemars(
        description = "Optional. Experimental. The enter effect when the step goes forward: draw (the stroke is drawn) or fade. Draw needs a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike."
    )]
    pub effect: Option<tiza_core::effects::Effect>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateParams {
    pub id: String,
    #[schemars(description = "New step at which it appears")]
    pub step: Option<u32>,
    #[schemars(
        description = "New color (palette name or #rrggbb); an empty string returns to automatic"
    )]
    pub color: Option<String>,
    #[schemars(
        description = "New label: the text of a note, the title of a callout. An empty string removes a label or a title. A note keeps its text."
    )]
    pub label: Option<String>,
    #[schemars(
        description = "Move a highlight, mark or note to another position (same `find`/line/col as add_mark). Kind, style and label are kept."
    )]
    pub at: Option<At>,
    #[schemars(description = "Arrows only: new source anchor")]
    pub from: Option<At>,
    #[schemars(description = "Arrows only: new target anchor")]
    pub to: Option<At>,
    #[schemars(description = "Boxes, callouts, spotlights and notes: new first line")]
    pub from_line: Option<u32>,
    #[schemars(description = "Boxes, callouts and spotlights: new last line")]
    pub to_line: Option<u32>,
    #[schemars(description = "Arrows: new stroke (solid, dashed, dotted, thick, double)")]
    pub stroke: Option<String>,
    #[schemars(description = "Arrows: new corners (rounded, sharp)")]
    pub route: Option<String>,
    #[schemars(description = "Arrows: head at the source (none, arrow, dot, bar, open)")]
    pub head_start: Option<String>,
    #[schemars(description = "Arrows: head at the target (none, arrow, dot, bar, open)")]
    pub head_end: Option<String>,
    #[schemars(
        description = "Boxes/callouts: border stroke (solid, dashed, dotted, thick, double)"
    )]
    pub border: Option<String>,
    #[schemars(description = "Boxes/callouts: corners (rounded, sharp)")]
    pub corners: Option<String>,
    #[schemars(description = "Boxes/callouts: tint the inside (true/false)")]
    pub fill: Option<bool>,
    #[schemars(
        description = "Highlights/marks: how the span is drawn (fill, underline, brackets, strike)"
    )]
    pub emphasis: Option<String>,
    /// Absent keeps the effect, `null` removes it (`present`), as `file`.
    #[serde(default, deserialize_with = "tiza_core::protocol::present")]
    #[schemars(
        with = "Option<tiza_core::effects::Effect>",
        description = "Optional. Experimental. The enter effect when the step goes forward: draw (the stroke is drawn) or fade. Draw needs a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike. `null` removes the effect. Leave it out to keep the effect of the annotation."
    )]
    pub effect: Option<Option<tiza_core::effects::Effect>>,
    /// Absent keeps the file, `null` is the main file (`present`), as in
    /// the socket's `annotation.update`.
    #[serde(default, deserialize_with = "tiza_core::protocol::present")]
    #[schemars(
        with = "Option<String>",
        description = "The file the annotation goes to, as a `code.link` names it. `null` is the main file. Leave it out to keep the file of the annotation."
    )]
    pub file: Option<Option<String>>,
    /// Not a parameter (not in the schema): refused here, with what to give
    /// instead, as the tool has no `shape`.
    #[serde(default)]
    #[schemars(skip)]
    pub text: Option<Value>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportParams {
    #[schemars(
        description = "Where to write the file. The extension picks the format: .png, .svg, .txt, .ansi, .md, .gif (an animated GIF of every step, drawn by Tiza itself), or .webp (lossless: with `step`, one step, as a .png; without it, every step, as a .gif)"
    )]
    pub path: String,
    #[schemars(
        description = "Step to show; defaults to the current one. Past the last step, the last is exported and the reply says so in `warning`"
    )]
    pub step: Option<u32>,
    #[schemars(description = "Width in columns; defaults to the live terminal's width")]
    pub width: Option<usize>,
    #[schemars(description = "dark or light; defaults to the document's theme")]
    pub theme: Option<String>,
    #[schemars(description = "PNG/WebP/GIF device scale (default 2)")]
    pub scale: Option<f32>,
    #[schemars(
        description = "GIF and animated WebP: seconds each step stays on screen, 0 to 3600 (default 2; the last one twice as long)"
    )]
    pub interval: Option<f64>,
    #[schemars(
        description = "GIF and animated WebP: start at step 1 instead of step 0, the bare code (default false)"
    )]
    pub skip_zero: Option<bool>,
    #[schemars(description = "Font size in px (default 16)")]
    pub font_size: Option<f32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CalloutParams {
    #[schemars(description = "First line the callout explains (1-based)")]
    pub from_line: Option<u32>,
    #[schemars(description = "Last line it explains; defaults to from_line")]
    pub to_line: Option<u32>,
    #[schemars(
        description = "Text on the first line of the range, instead of from_line (with find_end)"
    )]
    pub find_start: Option<String>,
    #[schemars(description = "Text on the last line of the range, searched after find_start")]
    pub find_end: Option<String>,
    #[schemars(
        description = "Body: two or three sentences, wrapped by words. Newlines start new paragraphs."
    )]
    pub text: String,
    #[schemars(description = "Short title embedded in the top border")]
    pub title: Option<String>,
    #[schemars(
        description = "margin (default: right margin, aligned with from_line), below (own rows under to_line; drawn inside a box only when the box contains the whole range, otherwise after its bottom border) or auto"
    )]
    pub placement: Option<String>,
    #[schemars(
        description = "Max text width in columns (default ~34 in the margin, full code width below)"
    )]
    pub width: Option<usize>,
    pub step: Option<u32>,
    pub color: Option<String>,
    pub id: Option<String>,
    #[schemars(description = "Border stroke: solid (default), dashed, dotted, thick, double")]
    pub border: Option<String>,
    #[schemars(description = "Corners: rounded (default) or sharp")]
    pub corners: Option<String>,
    #[schemars(
        description = "Tint the inside: true/false (boxes default to true, callouts to false)"
    )]
    pub fill: Option<bool>,
    #[schemars(
        description = "Optional. Experimental. The enter effect when the step goes forward: draw (the stroke is drawn) or fade. Draw needs a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike."
    )]
    pub effect: Option<tiza_core::effects::Effect>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SpotlightParams {
    #[schemars(
        description = "First line to keep in focus; Tiza dims the code and the annotations outside the range"
    )]
    pub from_line: Option<u32>,
    #[schemars(description = "Last line to keep in focus; defaults to from_line")]
    pub to_line: Option<u32>,
    #[schemars(
        description = "Text on the first line of the range, instead of from_line (with find_end)"
    )]
    pub find_start: Option<String>,
    #[schemars(description = "Text on the last line of the range, searched after find_start")]
    pub find_end: Option<String>,
    #[schemars(
        description = "Ids of annotations that keep their color, also outside the range; their code stays in focus"
    )]
    pub ids: Option<Vec<String>>,
    #[schemars(description = "Step at which it activates (defaults to the current one)")]
    pub step: Option<u32>,
    #[schemars(description = "Last step at which it stays active; defaults to `step` only")]
    pub until: Option<u32>,
    pub id: Option<String>,
    #[schemars(
        description = "Optional. Experimental. The enter effect when the step goes forward: draw (the stroke is drawn) or fade. Draw needs a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike."
    )]
    pub effect: Option<tiza_core::effects::Effect>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearParams {
    #[schemars(
        description = "If given, removes only the annotations of that step; otherwise all of them",
        with = "u32"
    )]
    #[serde(
        default,
        deserialize_with = "step_number",
        skip_serializing_if = "Option::is_none"
    )]
    pub step: Option<u32>,
}

/// A `step` that is there must be a number: `null` is refused, so that it
/// never stands for "every annotation".
fn step_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    <u32 as serde::Deserialize>::deserialize(d).map(Some)
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct IdParams {
    pub id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AddMoveParams {
    #[schemars(
        description = "Step at which the code changes (>= 1). Going back to an earlier step reverts it."
    )]
    pub at_step: u32,
    #[schemars(
        description = "`code.patch` (default: edit lines of the file shown at the step), `code.link` (show another file — or the main file again — from this step on; each file keeps its own annotations), `cursor`, `select`, `wait` or `scroll` (move the view of the TUI to a `line`, a `find` text or an `annotation`, with `align`)"
    )]
    pub action: Option<String>,
    #[schemars(description = "code.patch: `insert`, `replace` or `delete` (whole lines)")]
    pub op: Option<String>,
    #[schemars(
        description = "code.link: the file to show, relative to the .tiza directory (as `code.file` names it); naming the main file links back to it"
    )]
    pub file: Option<String>,
    #[schemars(
        description = "code.link: a window `A-B` (or `A`) of the file to show; only a crop, annotations keep real file line numbers and a note outside the window is kept but not drawn"
    )]
    pub lines: Option<String>,
    #[schemars(description = "cursor, scroll: the line (real file line number) of the shown file")]
    pub line: Option<u32>,
    #[schemars(description = "cursor: grapheme column (default 0)")]
    pub col: Option<u32>,
    #[schemars(
        description = "select: where the selection starts — `{find}` (exactly one match at that step, or with `occurrence`) or `{line, col}`"
    )]
    pub from: Option<Value>,
    #[schemars(
        description = "select: where it ends — `{find}` (the end of the found text) or `{line, col}`"
    )]
    pub to: Option<Value>,
    #[schemars(
        description = "wait: milliseconds to pause before the step's annotations appear (TUI, GIF and tape; a still render ignores it)"
    )]
    pub ms: Option<u64>,
    #[schemars(
        description = "insert: the line the content goes before (last+1 appends). replace/delete: a line or range `A-B`, as shown at that step"
    )]
    pub position: Option<String>,
    #[schemars(
        description = "insert: text of the line the content goes after (must match exactly one line)"
    )]
    pub after: Option<String>,
    #[schemars(
        description = "insert: text of the line the content goes before (must match exactly one line)"
    )]
    pub before: Option<String>,
    #[schemars(
        description = "replace/delete: text of the one line to change (must match exactly one line). scroll: the text to show: its first match, or the match that `occurrence` names"
    )]
    pub find: Option<String>,
    #[schemars(
        description = "replace/delete: text on the first line of the range (with find_end)"
    )]
    pub find_start: Option<String>,
    #[schemars(
        description = "replace/delete: text on the last line of the range (with find_start)"
    )]
    pub find_end: Option<String>,
    #[schemars(
        description = "Which match of find/after/before/find_start to use when the text appears more than once: 1-based, counted in reading order over the whole code as it is at that step. Without it, more than one match is an error, but a scroll goes to the first match"
    )]
    pub occurrence: Option<u32>,
    #[schemars(description = "The same for find_end")]
    pub end_occurrence: Option<u32>,
    #[schemars(
        description = "scroll: the id of the annotation to show, with its borders and labels. It can be an annotation of a later step"
    )]
    pub annotation: Option<String>,
    #[schemars(
        description = "scroll: where the target goes in the view: `top`, `center` (default), `bottom` or `show` (the least motion that shows all of it)"
    )]
    pub align: Option<String>,
    #[schemars(description = "insert/replace: the new lines, separated by newlines")]
    pub content: Option<String>,
    #[schemars(
        description = "`instant` (default), `line` or `char`: how the content appears when the presenter reaches the step"
    )]
    pub typing: Option<String>,
    #[schemars(
        description = "Milliseconds per line (`line`, default 120) or per character (`char`, default 25)"
    )]
    pub speed_ms: Option<u64>,
    #[schemars(
        description = "Where annotations the patch replaces or deletes go afterwards, by id: `{\"a3\": {\"at\": {\"find\": \"text\", \"occurrence\": 2}}}`, `{\"a4\": {\"from_line\": 7, \"to_line\": 9}}` (same fields as update_annotation's move), resolved on the code after the patch; a find must match exactly once there or carry occurrence. Needed for column anchors (highlights, marks, arrow ends) on a replaced line: those go (unanchored) otherwise"
    )]
    pub rebind: Option<serde_json::Map<String, Value>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MoveIndexParams {
    #[schemars(description = "1-based index, as list_moves shows it")]
    pub index: usize,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct StepParams {
    #[schemars(description = "Step to show (1-based). Clamped to the valid range.")]
    pub step: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GotoStepParams {
    #[schemars(
        description = "The `id` of a named step (the document's `steps` list), or a step number: `4` or `\"4\"` (a number always means a step number)."
    )]
    pub id: tiza_core::protocol::StepRef,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FocusParams {
    #[schemars(description = "First focused line; omit both to clear the focus")]
    pub from_line: Option<u32>,
    pub to_line: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PointAtParams {
    #[schemars(description = "The line to point at (with `find`: the line the text is on)")]
    pub line: Option<u32>,
    #[schemars(
        description = "Text that names the line: on `line` when given, else it must appear exactly once in the code shown"
    )]
    pub find: Option<String>,
    #[schemars(description = "First line of a range to point at (instead of `line`)")]
    pub from: Option<u32>,
    #[schemars(description = "Last line of the range (defaults to `from`)")]
    pub to: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ThemeParams {
    #[schemars(
        description = "dark, light, terminal (the host's palette), auto (follow the terminal) or toggle"
    )]
    pub theme: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FollowParams {
    #[schemars(
        description = "true to reload the code whenever the source file is saved (annotations are re-anchored to their text), false to stop"
    )]
    pub enabled: bool,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RenderParams {
    #[schemars(
        description = "Width in columns of the take. Without it, the width of the screen's code area (the terminal minus its padding), as `visible` judges it."
    )]
    pub width: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CheckParams {
    #[schemars(
        description = "Width in columns to check the labels at; defaults to 100, as `tiza render`"
    )]
    pub width: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct TizaServer {
    socket: PathBuf,
    /// Modification time of our own binary when the bridge started: if the
    /// file is newer now, `cargo build` replaced it and this process (and the
    /// tool list the client cached from it) is stale.
    exe_mtime: Option<std::time::SystemTime>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

/// Modification time of the running binary, if it can be read.
fn exe_mtime() -> Option<std::time::SystemTime> {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}

/// A tool's text reply, with its control characters shown as pictures
/// (`␛`, `␀`…) and never sent raw to the client.
fn text(v: impl ToString) -> CallToolResult {
    let v = v.to_string();
    CallToolResult::success(vec![ContentBlock::text(
        tiza_core::printable::printable_text(&v).into_owned(),
    )])
}

/// The reply of `annotation.add` / `annotation.update` with a first line
/// that says in words whether the annotation shows at the current step,
/// then the JSON.
fn annotation_reply(v: &Value, verb: &str) -> CallToolResult {
    match serde_json::from_value::<tiza_core::protocol::AnnotationMutated>(v.clone()) {
        Ok(m) => text(format!("{}\n{}", m.sentence(verb), pretty(v))),
        Err(_) => text(pretty(v)),
    }
}

/// Pretty JSON; DEL, C1 and bidi controls as `\uXXXX` (the same value;
/// serde_json escapes only C0).
fn pretty(v: &Value) -> String {
    let text = serde_json::to_string_pretty(v).unwrap_or_default();
    tiza_core::printable::escape_json_controls(&text).into_owned()
}

/// The `_meta` of the core tools: `anthropic/alwaysLoad` asks a
/// client that defers tools (Claude Code's tool search) to load them
/// upfront (<https://code.claude.com/docs/en/mcp>, «Mark a tool to load
/// upfront or stay deferred»). A client that does not know the key ignores it.
fn always_load() -> rmcp::model::MetaObject {
    rmcp::model::MetaObject(serde_json::Map::from_iter([(
        "anthropic/alwaysLoad".to_string(),
        Value::Bool(true),
    )]))
}

#[tool_router]
impl TizaServer {
    pub fn new(socket: PathBuf) -> Self {
        Self {
            socket,
            exe_mtime: exe_mtime(),
            tool_router: Self::tool_router(),
        }
    }

    /// A server that believes its binary was modified at `mtime` (tests).
    pub fn with_exe_mtime(socket: PathBuf, mtime: Option<std::time::SystemTime>) -> Self {
        Self {
            socket,
            exe_mtime: mtime,
            tool_router: Self::tool_router(),
        }
    }

    /// True when the `tiza` binary on disk is newer than this process.
    pub fn binary_rebuilt(&self) -> bool {
        matches!((self.exe_mtime, exe_mtime()), (Some(a), Some(b)) if b > a)
    }

    /// What `status` reports about the bridge itself (version, build, staleness).
    pub fn bridge_status(&self, instance: &Value) -> Value {
        let mut bridge = json!({
            "version": tiza_core::VERSION,
            "build": tiza_core::GIT_SHA,
        });
        let rebuilt = self.binary_rebuilt();
        let instance_build = instance.get("build").and_then(|b| b.as_str());
        let mismatch = instance_build.is_some_and(|b| b != tiza_core::GIT_SHA);
        if rebuilt {
            bridge["binary_rebuilt"] = json!(true);
        }
        if mismatch {
            bridge["instance_build"] = json!(instance_build);
        }
        if rebuilt || mismatch {
            bridge["hint"] = json!(
                "the tiza binary was rebuilt: this MCP process (and the tool list \
                 the client cached from it) may be stale. Ask the user to reconnect the \
                 server (in Claude Code: /mcp → tiza → reconnect) and, if the TUI \
                 build differs, to restart the TUI."
            );
        }
        bridge
    }

    /// Calls the live instance. Turns connection errors into messages useful to the agent.
    async fn call(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let path = self.socket.clone();
        let method_owned = method.to_string();
        let resp = tokio::task::spawn_blocking(move || tiza_tui::ipc::call(&path, &method_owned, params))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .map_err(|e| {
                let message = match tiza_tui::ipc::unanswered(&self.socket, &e) {
                    Some(mut why) => {
                        if let Some(first) = why.get_mut(0..1) {
                            first.make_ascii_uppercase();
                        }
                        format!("{why}. Ask the user to look at the Tiza terminal.")
                    }
                    None => format!(
                        "No Tiza instance is listening on {} ({e}). Ask the user to open `tiza <file>` in a terminal, or pass --socket/--session to the MCP server.",
                        self.socket.display()
                    ),
                };
                McpError::internal_error(message, None)
            })?;
        if let Some(err) = resp.error {
            return Err(McpError::invalid_params(
                format!(
                    "{} ({})",
                    tiza_core::printable::printable_text(&err.message),
                    err.code
                ),
                None,
            ));
        }
        Ok(resp.result.unwrap_or(Value::Null))
    }

    #[tool(
        meta = always_load(),
        description = "State of the live Tiza instance, the terminal where you draw a box, arrow, highlight or note on code to explain it step by step: open file, lines, annotations, current step and terminal size (`stale_moves`: how many moves are skipped because the code changed under them; `save_error`: why the last save of the document failed, or null: then your changes are not saved); plus `bridge` (this server's version/build; `binary_rebuilt`/`hint` when tiza was rebuilt and the client should reconnect). Call it first to learn whether an instance exists."
    )]
    async fn status(&self, Parameters(_p): Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut v = self.call("status", json!({})).await?;
        let bridge = self.bridge_status(&v);
        if let Some(obj) = v.as_object_mut() {
            obj.insert("bridge".into(), bridge);
        }
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Checks the live document, as `tiza check --json` does. It runs inside the instance on a copy and changes nothing: run it before you show a document. Errors: an anchor or a marker that matches nothing; a move whose target does not match exactly once at its step; a linked file that Tiza cannot read; an unknown or a duplicate id. Warnings: an annotation hidden as (unanchored) at a step; an annotation outside the `code.link` window of a step that shows its file (`outside_window`: `outside the window at step 3 (src/queue.rs:10-36): kept, but not drawn there`); an annotation whose own step does not show its file (`not_shown`: `its file src/stack.rs is not shown at step 4 (src/queue.rs is): kept, but not drawn at its step`); a label that `width` clips with «…», with its step, id and full text; a step with nothing new; a marker that matches more than once. `min_clean_width` is the narrowest width that clips no label. When `width` clips no label, `narrower` lists the narrower widths that clip none too. `revision` is the revision that it checked: the screen and the other tools keep working while it runs, so a check that waited can check a later revision. One check runs and one waits. A check at another width replaces the waiting one, which fails with `busy`: ask again."
    )]
    async fn check(
        &self,
        Parameters(p): Parameters<CheckParams>,
    ) -> Result<CallToolResult, McpError> {
        let width = p
            .width
            .map(tiza_core::layout::check_width)
            .transpose()
            .map_err(|e| invalid_argument(e.to_string()))?;
        // The instance checks the document it holds, linked files and all,
        // and sends back only the report.
        let report = self.call("check", json!({ "width": width })).await?;
        Ok(text(pretty(&report)))
    }

    #[tool(
        meta = always_load(),
        description = "Returns the code Tiza shows on screen, with line numbers, to pick where a box, arrow, highlight or note goes (the file a `code.link` left up, cropped to its window, with the patches of the current step applied). Read it before annotating to pick exact lines and texts; a first line names the file and window when a link is shown."
    )]
    async fn get_code(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        let v = self.call("code.get", json!({})).await?;
        let body = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
        Ok(text(match v.get("view").and_then(|t| t.as_str()) {
            Some(view) => format!("# {view}\n{body}"),
            None => body.to_string(),
        }))
    }

    #[tool(
        description = "Lists the existing annotations (id, step, kind and where they are), after a first line with the current step as the status line shows it (`step 3/7 · Shared borrow` on a named step). An annotation of a linked file ends with its file in parentheses; one hidden by a patch says (unanchored). A move the code changed under (a follow reload, a paste) and that no longer matches is listed last as `move #N … (stale: why)`: it is skipped until it matches again."
    )]
    async fn list_annotations(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        let st = self.call("status", json!({})).await?;
        let header = st
            .get("step_label")
            .and_then(|l| l.as_str())
            .filter(|l| !l.is_empty())
            .map(|l| format!("# {l}\n"))
            .unwrap_or_default();
        let v = self.call("scene.list", json!({})).await?;
        let lines: Vec<String> = v
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(text(format!(
            "{header}{}",
            if lines.is_empty() {
                "(no annotations)".to_string()
            } else {
                lines.join("\n")
            }
        )))
    }

    #[tool(
        meta = always_load(),
        description = "Draws a box around a range of lines (a function, a block) to explain it at a step, with an optional label on the top edge. Give from_line/to_line, or find_start/find_end (two texts that delimit the range, like Demo Time placeholders: `// begin` … `// end`, or code such as `let t` … `letters`). The end is the first match after the start, so a `}` is often an inner block's brace or a `{s}` in a string: to box a whole function, give its lines."
    )]
    async fn add_box(
        &self,
        Parameters(p): Parameters<BoxParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = json!({"type": "box", "from_line": p.from_line, "to_line": p.to_line, "find_start": p.find_start, "find_end": p.find_end, "label": p.label, "step": p.step, "id": p.id, "color": p.color, "effect": p.effect});
        for (k, v) in border_style_json(&p.border, &p.corners, p.fill)? {
            params[k] = v;
        }
        let v = self.call("annotation.add", params).await?;
        Ok(annotation_reply(&v, "added"))
    }

    #[tool(
        description = "Draws an arrow from one point of the code to another (e.g. from `let s` to `takes(s)`), with an optional label. Use `find` to anchor by text. Stroke, corners and heads are the author's choice: dashed for a borrow, thick for a move, head_end none + head_start none for a plain connector."
    )]
    async fn add_arrow(
        &self,
        Parameters(p): Parameters<ArrowParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = json!({"type": "arrow", "from": p.from, "to": p.to, "label": p.label, "step": p.step, "id": p.id, "color": p.color, "effect": p.effect});
        let style = line_style_json(&p.stroke, &p.route, &p.head_start, &p.head_end)?;
        for (k, v) in style {
            if k == "heads" {
                // annotation.add takes the whole heads object: fill the other end with its default.
                let mut heads = json!({"start": "none", "end": "arrow"});
                for (hk, hv) in v.as_object().unwrap() {
                    heads[hk] = hv.clone();
                }
                params["heads"] = heads;
            } else {
                params[k] = v;
            }
        }
        let v = self.call("annotation.add", params).await?;
        Ok(annotation_reply(&v, "added"))
    }

    #[tool(
        description = "Adds a text note anchored to a line (appears in the right margin or below it)."
    )]
    async fn add_note(
        &self,
        Parameters(p): Parameters<NoteParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self
            .call(
                "annotation.add",
                json!({"type": "note", "line": p.line, "text": p.text, "step": p.step, "id": p.id, "color": p.color, "effect": p.effect}),
            )
            .await?;
        Ok(annotation_reply(&v, "added"))
    }

    #[tool(
        description = "Highlights a text fragment (a variable, an expression) with an optional label; emphasis picks fill (tint, default), underline, brackets or strike."
    )]
    async fn highlight(
        &self,
        Parameters(p): Parameters<HighlightParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = json!({"type": "highlight", "at": p.at, "label": p.label, "step": p.step, "id": p.id, "color": p.color, "effect": p.effect});
        if let Some(e) = emphasis_json(&p.emphasis)? {
            params["emphasis"] = e;
        }
        let v = self.call("annotation.add", params).await?;
        Ok(annotation_reply(&v, "added"))
    }

    #[tool(
        description = "Semantic mark on a fragment: a built-in kind (warn, error, ok, read, write, move, drop, scope…), a reusable style from define_style, or a one-off inline style. Each shows glyph + legend; language-agnostic."
    )]
    async fn add_mark(
        &self,
        Parameters(p): Parameters<MarkParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = json!({"type": "mark", "kind": p.kind, "style": p.style.unwrap_or_default(), "at": p.at, "label": p.label, "step": p.step, "id": p.id, "color": p.color, "effect": p.effect});
        if let Some(e) = emphasis_json(&p.emphasis)? {
            params["emphasis"] = e;
        }
        let v = self.call("annotation.add", params).await?;
        Ok(annotation_reply(&v, "added"))
    }

    #[tool(
        description = "Changes an annotation that exists and keeps its id. It can change the step, the color or the label (the text of a note, the title of a callout). For an arrow, it can change the line style: stroke, route, head_start and head_end. For a box or a callout, it can change the border: border, corners and fill. For a highlight or a mark, it can change the emphasis: fill, underline, brackets or strike. To move the annotation and keep its type, kind and style, give its new place. Use `at` for highlights, marks and notes, `from` and `to` for arrows, and `from_line` and `to_line` for boxes, callouts, spotlights and notes. `file` puts it on another file of the document: a linked file as a `code.link` names it, or `null` for the main file. A field that does not apply to the type of the annotation fails with invalid_argument. Tiza refuses a field that it does not know."
    )]
    async fn update_annotation(
        &self,
        Parameters(p): Parameters<UpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = json!({"id": p.id, "step": p.step, "label": p.label, "color": p.color});
        let mv = json!({"at": p.at, "from": p.from, "to": p.to, "from_line": p.from_line, "to_line": p.to_line});
        if mv
            .as_object()
            .is_some_and(|m| m.values().any(|v| !v.is_null()))
        {
            params["move"] = mv;
        }
        for (k, v) in line_style_json(&p.stroke, &p.route, &p.head_start, &p.head_end)? {
            params[k] = v;
        }
        for (k, v) in border_style_json(&p.border, &p.corners, p.fill)? {
            params[k] = v;
        }
        if let Some(e) = emphasis_json(&p.emphasis)? {
            params["emphasis"] = e;
        }
        if let Some(file) = &p.file {
            params["file"] = json!(file);
        }
        if let Some(effect) = &p.effect {
            params["effect"] = json!(effect);
        }
        if p.text.is_some() {
            return Err(invalid_argument(
                "update_annotation has no `text`. Use `label` to change the text of a note or the title of a callout. To change the text of a callout, remove it and add it again",
            ));
        }
        let v = self.call("annotation.update", params).await?;
        Ok(annotation_reply(&v, "updated"))
    }

    #[tool(
        description = "Removes every annotation at once (or only those of one step with `step`). Useful between takes. The presenter can undo it with `u`."
    )]
    async fn clear_annotations(
        &self,
        Parameters(p): Parameters<ClearParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self
                .call(
                    "scene.clear",
                    // No step: no key at all (`"step": null` is refused).
                    match p.step {
                        Some(s) => json!({ "step": s }),
                        None => json!({}),
                    },
                )
                .await?,
        )))
    }

    #[tool(
        description = "Adds a bordered multi-line text block (callout) explaining a range of lines: use it for two or three sentences instead of a note. Placement margin (default), below or auto."
    )]
    async fn add_callout(
        &self,
        Parameters(p): Parameters<CalloutParams>,
    ) -> Result<CallToolResult, McpError> {
        let placement = match p
            .placement
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            None | Some("margin") => "margin",
            Some("below") => "below",
            Some("auto") => "auto",
            Some(_) => {
                return Err(invalid_argument("unknown placement (margin, below, auto)"));
            }
        };
        let mut params = json!({"type": "callout", "from_line": p.from_line, "to_line": p.to_line, "find_start": p.find_start, "find_end": p.find_end, "text": p.text, "title": p.title, "placement": placement, "width": p.width, "step": p.step, "color": p.color, "id": p.id, "effect": p.effect});
        for (k, v) in border_style_json(&p.border, &p.corners, p.fill)? {
            params[k] = v;
        }
        let v = self.call("annotation.add", params).await?;
        Ok(annotation_reply(&v, "added"))
    }

    #[tool(
        description = "Directs attention in a step: dims the code outside a line range, and each annotation that does not touch the range and is not in `ids`. An arrow touches the range with one end inside it, or when it goes across it. With only `ids`, it dims every annotation outside `ids`. Use it as a 'look here' step between reveals. Active from `step` to `until`."
    )]
    async fn add_spotlight(
        &self,
        Parameters(p): Parameters<SpotlightParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self
            .call(
                "annotation.add",
                json!({"type": "spotlight", "from_line": p.from_line, "to_line": p.to_line, "find_start": p.find_start, "find_end": p.find_end, "ids": p.ids.unwrap_or_default(), "until": p.until, "step": p.step, "id": p.id, "effect": p.effect}),
            )
            .await?;
        Ok(annotation_reply(&v, "added"))
    }

    #[tool(
        description = "Defines (or updates) a reusable mark style in the document: name + glyph + legend + color. Afterwards use `name` as `kind` in add_mark."
    )]
    async fn define_style(
        &self,
        Parameters(p): Parameters<DefineStyleParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self
            .call(
                "style.set",
                json!({"name": p.name, "glyph": p.glyph, "legend": p.legend, "color": p.color}),
            )
            .await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Lists the mark kinds available, with their glyphs: the document's styles, the user's personal styles (`personal`) and the built-in kinds. A personal style goes into the document when a mark uses it."
    )]
    async fn list_kinds(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(&self.call("style.list", json!({})).await?)))
    }

    #[tool(description = "Deletes an annotation by id.")]
    async fn delete_annotation(
        &self,
        Parameters(p): Parameters<IdParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self.call("annotation.delete", json!({"id": p.id})).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Adds a move. The move runs when the presenter reaches `at_step`, and Tiza reverts it when the presenter goes back. `code.patch` (the default) inserts, replaces or deletes whole lines of the file that the step shows. Each target must match exactly once in that code. The patch shifts the annotations of earlier steps. An annotation that loses all its lines is hidden as (unanchored), unless `rebind` names its new place. The patch also carries the annotations of that file at that step or later, and the line numbers of later moves, so that they stay on the same text. Tiza refuses a patch that removes or changes the text under one of them, and names it. `typing: line|char` types the content in place. `code.link {file, lines?}` shows another file from that step on. Each file keeps its own annotations, and `lines` is only a window. An annotation added at a step belongs to the file that the step shows (see list_annotations). `cursor {line, col?}`, `select {from, to}` and `wait {ms}` set the cursor, a selection and a pause. `scroll {line | find | annotation, align?}` moves the view of the TUI to that target when the presenter reaches the step; the exports ignore it. `render --step N` and the image exports apply the moves at once. When the move hides annotations that a step drew, Tiza makes the move and `warning` in the reply names them: outside a `code.link` window (`a3: outside the window at step 3 (src/queue.rs:10-36): kept, not drawn there`), or of a file that their step no longer shows (`a6: its file src/stack.rs is not shown at step 4 (src/queue.rs is): kept, not drawn there`). `delete_move` replies with the same `warning`. An argument that the action does not use fails, for example `align` with a `code.patch`."
    )]
    async fn add_move(
        &self,
        Parameters(p): Parameters<AddMoveParams>,
    ) -> Result<CallToolResult, McpError> {
        let action = p.action.unwrap_or_else(|| "code.patch".into());
        let mut m = json!({"at_step": p.at_step, "action": action});
        let o = m.as_object_mut().expect("object");
        let mut put = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                o.insert(k.to_string(), v);
            }
        };
        put("op", p.op.map(Value::from));
        put("file", p.file.map(Value::from));
        put("lines", p.lines.map(Value::from));
        put("line", p.line.map(Value::from));
        put("col", p.col.map(Value::from));
        put("from", p.from);
        put("to", p.to);
        put("ms", p.ms.map(Value::from));
        put("position", p.position.map(Value::from));
        put("after", p.after.map(Value::from));
        put("before", p.before.map(Value::from));
        put("find", p.find.map(Value::from));
        put("find_start", p.find_start.map(Value::from));
        put("find_end", p.find_end.map(Value::from));
        put("occurrence", p.occurrence.map(Value::from));
        put("end_occurrence", p.end_occurrence.map(Value::from));
        put("content", p.content.map(Value::from));
        put("typing", p.typing.map(Value::from));
        put("speed_ms", p.speed_ms.map(Value::from));
        put("annotation", p.annotation.map(Value::from));
        put("align", p.align.map(Value::from));
        put("rebind", p.rebind.map(Value::Object));
        Ok(text(pretty(&self.call("move.add", m).await?)))
    }

    #[tool(
        description = "Deletes a move by its 1-based index (see list_moves). Deleting a `code.patch` carries the annotations and later moves it had carried back; a delete that would leave an annotation without its line, or a later move that no longer applies, is refused, naming it."
    )]
    async fn delete_move(
        &self,
        Parameters(p): Parameters<MoveIndexParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("move.delete", json!({"index": p.index})).await?,
        )))
    }

    #[tool(
        description = "Lists the moves (index, step, summary), the annotation ids unanchored at the current step and the file/window a `code.link` shows (`view`). A move the code changed under (a follow reload, a paste) and that no longer matches it has `stale: true` and `stale_reason` (`line 2 changed`, `find «x» matches nothing in the code`): it is skipped when stepping and kept in the file as written (a line-number target follows the change only when where its lines went is exact); fix it (delete and add it again on the code as it is now, preferably with `find` in a document that reads its code by reference) or put the code back as it was (that clears it)."
    )]
    async fn list_moves(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(&self.call("move.list", json!({})).await?)))
    }

    #[tool(
        description = "Sets the current presentation step (annotations with step <= the step are shown)."
    )]
    async fn set_step(
        &self,
        Parameters(p): Parameters<StepParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("step.set", json!({"step": p.step})).await?,
        )))
    }

    #[tool(
        description = "Jumps to a named step by its `id` (the document's `steps` list) or a step number: the code is recomputed from step 0 with every move up to that step, as if the presenter walked there, and its typing lands complete. An unknown id fails with not_found. Returns the step, its id/title and `label` (`step 3/7 · Shared borrow`)."
    )]
    async fn goto_step(
        &self,
        Parameters(p): Parameters<GotoStepParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("step.goto", json!({"id": p.id})).await?,
        )))
    }

    #[tool(
        description = "Advances one step in the presentation (past the last one, back to step 0 when the document has `loop: true`)."
    )]
    async fn next_step(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(&self.call("step.next", json!({})).await?)))
    }

    #[tool(description = "Goes back one step in the presentation.")]
    async fn prev_step(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(&self.call("step.prev", json!({})).await?)))
    }

    #[tool(
        description = "Focuses a range of lines (the rest is dimmed). Without parameters, clears the focus."
    )]
    async fn set_focus(
        &self,
        Parameters(p): Parameters<FocusParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self
                .call(
                    "focus.set",
                    json!({"from_line": p.from_line, "to_line": p.to_line}),
                )
                .await?,
        )))
    }

    #[tool(
        description = "Points at a line (or a range) on the live screen, like a laser pointer: that line is drawn normally and the rest of the code is dimmed while you talk about it. Nothing is created: it is not an annotation, never saved, never in an export, a GIF or a recording, and the next step keeps it on the same line (it goes off when the line disappears). Use it to say «this line» during a live explanation; use `add_spotlight` when the focus belongs to a step of the recording. Give `line`, `line` + `find`, `find` alone (must appear once) or `from`/`to`. The presenter's `f` key is the same pointer: the last one wins. `status` shows it as `pointer`."
    )]
    async fn point_at(
        &self,
        Parameters(p): Parameters<PointAtParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = serde_json::Map::new();
        for (k, v) in [
            ("line", p.line.map(Value::from)),
            ("find", p.find.map(Value::from)),
            ("from", p.from.map(Value::from)),
            ("to", p.to.map(Value::from)),
        ] {
            if let Some(v) = v {
                params.insert(k.into(), v);
            }
        }
        Ok(text(pretty(
            &self.call("pointer.set", Value::Object(params)).await?,
        )))
    }

    #[tool(
        description = "Turns the live pointer off (yours or the presenter's `f`): the code is drawn normally again."
    )]
    async fn clear_pointer(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(&self.call("pointer.clear", json!({})).await?)))
    }

    #[tool(
        description = "Changes the theme: dark, light, terminal (the host's own palette), auto (follow the terminal's background) or toggle."
    )]
    async fn set_theme(
        &self,
        Parameters(p): Parameters<ThemeParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("theme.set", json!({"theme": p.theme})).await?,
        )))
    }

    #[tool(
        description = "Turns follow mode on or off: with it on, saving the source file in the editor reloads the code in the TUI and the annotations follow their text. Fails when the code is embedded and no followed file is known (read from stdin, or frozen with `:embed`); over code a paste or an undo put on a followed file, even in a reopened document, it loads that file. At a step with a `code.patch` applied it turns on paused: a save waits until the presenter is back at `resume_step` (the step before the first patch) or earlier, then loads (`pending` says that a change waits). The file watcher starts on a thread of its own: if it cannot start, follow turns off soon after the reply and `follow.status` gives the `reason`. Returns the follow status (enabled, path, reloads, last_reload, paused, pending, resume_step, reason)."
    )]
    async fn set_follow(
        &self,
        Parameters(p): Parameters<FollowParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self
                .call("follow.set", json!({"enabled": p.enabled}))
                .await?,
        )))
    }

    #[tool(
        meta = always_load(),
        description = "Returns the Tiza screen as plain text: the code with its box, arrow, highlight and note annotations at the current step, as the audience sees it (without the live pointer). It is the take (the layout of presentation mode), the same text `tiza render --format txt` writes for this step at the width the second line names. Without `width` it is at the width of the screen's code area: it agrees with the `visible` of your add calls, and `check` at that width gives the same verdict. A second line of the result names the width. Use it to check the result of what you drew."
    )]
    async fn render_text(
        &self,
        Parameters(p): Parameters<RenderParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self.call("render.text", json!({"width": p.width})).await?;
        let mut result = text(v.get("text").and_then(|t| t.as_str()).unwrap_or(""));
        // The width, after the drawing: what to give `check` for the same take.
        if let Some(w) = v.get("width").and_then(|w| w.as_u64()) {
            let said = match p.width {
                None => format!(
                    "This is the take at {w} columns, the width that `visible` uses. Give `check` the same width (`width: {w}`) to check this take."
                ),
                Some(_) => format!(
                    "This is the take at {w} columns. Give `check` the same width (`width: {w}`) to check this take."
                ),
            };
            result.content.push(ContentBlock::text(said));
        }
        Ok(result)
    }

    #[tool(
        description = "Exports the scene to a file: .png or .svg (embedded fonts) for thumbnails and posts, .txt (plain box-drawing text), .ansi (24-bit colors, for `cat`) or .md (fenced block for a README, an issue or a chat), .gif (every step), or .webp (lossless: one step with `step`, else every step). Renders from the live document without touching the TUI, linked files included. Always answers: with the file written, or with an error (one that takes longer than 120 s is abandoned and nothing is written). A step past the last one exports the last and says so in `warning`."
    )]
    async fn export_image(
        &self,
        Parameters(p): Parameters<ExportParams>,
    ) -> Result<CallToolResult, McpError> {
        let ext = std::path::Path::new(&p.path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let format = match ext.as_deref() {
            Some("png") => "png",
            Some("svg") => "svg",
            Some("txt") => "txt",
            Some("ansi") | Some("ans") => "ansi",
            Some("md") | Some("markdown") => "md",
            Some("gif") => "gif",
            Some("webp") => "webp",
            _ => {
                return Err(invalid_argument(
                    "path must end in .png, .svg, .txt, .ansi, .md, .gif or .webp",
                ));
            }
        };
        // Every number is checked before anything is drawn, as `tiza
        // render` checks it: the font size of an image, the scale of the
        // pixels. A text export uses neither.
        let bad = |e: tiza_core::Error| invalid_argument(e.to_string());
        let pixels = matches!(format, "png" | "webp" | "gif");
        let scale = p.scale.unwrap_or(2.0);
        if pixels {
            tiza_core::layout::check_scale(scale).map_err(bad)?;
        }
        let font_size = p.font_size.unwrap_or(16.0);
        if pixels || format == "svg" {
            tiza_core::layout::check_font_size(font_size).map_err(bad)?;
        }
        let interval = tiza_core::anim::check_interval(p.interval.unwrap_or(2.0)).map_err(bad)?;
        let width = match p.width {
            Some(w) => Some(tiza_core::layout::check_width(w).map_err(bad)?),
            None => None,
        };
        let theme_name = match p.theme.as_deref() {
            Some(t) => match t.trim().to_ascii_lowercase().as_str() {
                "dark" | "oscuro" => Some(tiza_core::ThemeName::Dark),
                "light" | "claro" => Some(tiza_core::ThemeName::Light),
                _ => {
                    return Err(invalid_argument("unknown theme (dark, light)"));
                }
            },
            None => None,
        };
        let mut scene: tiza_core::protocol::Scene =
            serde_json::from_value(self.call("scene.get", json!({})).await?)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        // Its linked files too, as the instance shows them: an
        // export never draws another file than the TUI, in silence.
        // What the screen skips and hides, the export does too.
        tiza_core::stale::apply_marks(&mut scene.document, &scene.stale, &scene.held);
        tiza_core::fs::load_scene(&mut scene.document).map_err(|e| {
            McpError::internal_error(format!("the live document cannot be exported: {e}"), None)
        })?;
        let status: tiza_core::protocol::Status =
            serde_json::from_value(self.call("status", json!({})).await?)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        // The gutter the screen shows: when the document does not set
        // it, the instance's config file decided it, and `status` says how.
        if scene.document.numbers.is_none() {
            scene.document.numbers = Some(status.numbers);
        }
        let job = ExportJob {
            path: p.path,
            format,
            // A .webp without `step` is every step, as a .gif.
            animated: format == "gif" || (format == "webp" && p.step.is_none()),
            asked_step: p.step.unwrap_or(scene.step),
            width: width.unwrap_or(tiza_core::layout::width_for_terminal(status.cols)),
            theme_name,
            scale,
            font_size,
            interval,
            skip_zero: p.skip_zero.unwrap_or(false),
            doc: scene.document,
            phase: Default::default(),
        };
        // Drawn and encoded on a blocking thread with a deadline:
        // the server keeps answering `status` and `tools/list` meanwhile,
        // and the call always answers.
        let phase = job.phase.clone();
        let deadline = export_deadline();
        let mut handle = tokio::task::spawn_blocking(move || job.run());
        let joined = match tokio::time::timeout(deadline, &mut handle).await {
            Ok(joined) => joined,
            Err(_) => {
                // Past the deadline: abandoned, unless the file is already
                // in place (it was renamed just now), then that answer.
                let written = {
                    let mut ph = phase.lock().unwrap_or_else(|e| e.into_inner());
                    if *ph != ExportPhase::Written {
                        *ph = ExportPhase::Abandoned;
                    }
                    *ph == ExportPhase::Written
                };
                if written {
                    handle.await
                } else {
                    return Err(McpError::internal_error(
                        format!(
                            "export_image did not finish within {} s, and Tiza stopped it: nothing was written. Export fewer steps, a smaller width or scale, or a still (`step`) instead of a .gif or an animated .webp. `tiza render` in a terminal has no time limit",
                            deadline.as_secs_f64()
                        ),
                        None,
                    ));
                }
            }
        };
        match joined {
            Ok(r) => r,
            // A panic in the render: never its text (it may quote the
            // document).
            Err(e) if e.is_panic() => Err(panicked("tool export_image", e.into_panic())),
            Err(_) => Err(McpError::internal_error("export_image was cancelled", None)),
        }
    }
}

/// Where an `export_image` render stands: the deadline and the
/// rename of the finished file take the same lock, so the reply never says
/// «nothing was written» about a file that was.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ExportPhase {
    #[default]
    Running,
    Abandoned,
    Written,
}

/// How long one `export_image` may take: 120 s, fixed. A build
/// with the `test-hooks` feature (the tests' binary, never a release)
/// reads `TIZA_MCP_EXPORT_TIMEOUT_MS`. Past it the call answers an error
/// and nothing is written.
fn export_deadline() -> std::time::Duration {
    #[cfg(feature = "test-hooks")]
    if let Some(ms) = std::env::var("TIZA_MCP_EXPORT_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        return std::time::Duration::from_millis(ms);
    }
    std::time::Duration::from_secs(120)
}

/// Test hooks, only in a build with the `test-hooks` feature:
/// `TIZA_MCP_TEST_HOOK` names a place — `tool:<name>` panics in the call
/// itself, `export:render` in the export's blocking render, and
/// `export:slow-write` holds the finished export 800 ms before it is
/// written (a slow disk at the deadline).
#[allow(unused_variables)]
fn test_hook(place: &str) {
    #[cfg(feature = "test-hooks")]
    if std::env::var("TIZA_MCP_TEST_HOOK").is_ok_and(|v| v == place) {
        if place == "export:slow-write" {
            std::thread::sleep(std::time::Duration::from_millis(800));
        } else {
            panic!("TIZA_MCP_TEST_HOOK={place}");
        }
    }
}

/// An image over the limits, in words that an agent can act on:
/// `export_image` has `width`, `scale` and `font_size`, but no margin and
/// no range of lines.
fn too_big_message(e: &tiza_export::Error) -> String {
    let (side, all) = (tiza_export::MAX_SIDE_PX, tiza_export::MAX_PIXELS);
    let (svg_side, svg_all) = (
        tiza_export::limits::SVG_MAX_SIDE_PX,
        tiza_export::limits::SVG_MAX_PIXELS,
    );
    match e {
        tiza_export::Error::TooManyLines { lines, height } => format!(
            "{lines} lines of code make an image at least {height} px high, over the limit ({side} px per side, {all} px in all): use a smaller scale or font_size, or export a range of lines with `tiza render FILE:A-B`"
        ),
        tiza_export::Error::TooBig(w, h) => format!(
            "a {w}×{h} px image is over the limit ({side} px per side, {all} px in all): use a smaller width, scale or font_size"
        ),
        tiza_export::Error::SvgTooManyLines { lines, height } => format!(
            "{lines} lines of code make an SVG at least {height} px high, over the limit ({svg_side} px per side, {svg_all} px in all): use a smaller font_size, or export a range of lines with `tiza render FILE:A-B`"
        ),
        tiza_export::Error::SvgTooBig(w, h) => format!(
            "a {w}×{h} px SVG is over the limit ({svg_side} px per side, {svg_all} px in all): use a smaller width or font_size"
        ),
        other => other.to_string(),
    }
}

/// One `export_image`, everything it needs owned, so it runs on a
/// blocking thread.
struct ExportJob {
    path: String,
    format: &'static str,
    /// Every step (a .gif, or a .webp without `step`).
    animated: bool,
    asked_step: u32,
    width: usize,
    theme_name: Option<tiza_core::ThemeName>,
    scale: f32,
    font_size: f32,
    interval: f64,
    skip_zero: bool,
    doc: tiza_core::Document,
    /// Shared with the deadline: see [`ExportPhase`].
    phase: std::sync::Arc<std::sync::Mutex<ExportPhase>>,
}

impl ExportJob {
    fn run(self) -> Result<CallToolResult, McpError> {
        test_hook("export:render");
        let doc = &self.doc;
        let format = self.format;
        let animated = self.animated;
        let last = doc.max_step();
        // A step past the last one: the last, said in the reply,
        // as `tiza render` says it on stderr.
        let step = self.asked_step.min(last);
        // The code alone can make the image too large: refused before any
        // layout, which would fill the memory with a huge document. Here,
        // in the job, so that the deadline covers the count too.
        let first = u32::from(self.skip_zero);
        if matches!(format, "png" | "webp" | "gif") {
            let steps = if animated {
                first..=last.max(first)
            } else {
                step..=step
            };
            let lines = steps.map(|s| doc.lines_at(s)).max().unwrap_or(0);
            tiza_export::check_lines(
                lines,
                &tiza_export::SvgOptions {
                    font_size: self.font_size,
                    ..Default::default()
                },
                &tiza_export::PngOptions {
                    scale: self.scale,
                    system_fonts: false,
                },
            )
            .map_err(|e| invalid_argument(too_big_message(&e)))?;
        }
        if format == "svg" {
            tiza_export::check_svg_lines(
                doc.lines_at(step),
                &tiza_export::SvgOptions {
                    font_size: self.font_size,
                    ..Default::default()
                },
            )
            .map_err(|e| invalid_argument(too_big_message(&e)))?;
        }
        let warning = (!animated && self.asked_step > last).then(|| {
            format!(
                "step {} does not exist (the document has steps 0 to {last}); exported step {last}",
                self.asked_step
            )
        });
        let theme_name = self
            .theme_name
            .unwrap_or(doc.theme.unwrap_or(tiza_core::ThemeName::Dark));
        // The config file and the document's overrides apply, as in `tiza
        // render`; a terminal theme or `reset` tokens have no palette in a
        // file and fall back. What Tiza did not take from the config file,
        // or a config file that it cannot read, is said in the reply, never
        // fatal. A `$TIZA_CONFIG` that names no file (what tests and scripts
        // use for «no config») is not, as on the status bar.
        let (config, mut said) = match tiza_core::config::load() {
            Ok(cfg) => (
                cfg.theme.clone(),
                cfg.warning_lines()
                    .into_iter()
                    .filter(|_| cfg.path.is_some())
                    .map(|w| format!("config: {w}"))
                    .collect(),
            ),
            Err(e) => (None, vec![format!("config ignored: {e}")]),
        };
        let (theme, _) = tiza_core::Theme::resolve(
            theme_name,
            true,
            config.as_ref(),
            doc.theme_overrides.as_ref(),
        );
        let (theme, _notice) = theme.for_export();
        // An animation lays out a step that the check above covered (with
        // `skip_zero`, step 0 is not one); its width is the take's.
        let layout_step = if animated { step.max(first) } else { step };
        let layout = tiza_core::layout::layout(
            doc,
            &theme,
            &tiza_core::layout::LayoutOptions {
                width: self.width,
                step: layout_step,
                focus: None,
                pointer: None,
                line_numbers: doc.numbers_or(true),
                tab_width: 4,
                typing: false,
                stable: true,
                margin_cap: doc.margin.map(usize::from),
            },
        );
        let svg_opts = tiza_export::SvgOptions {
            font_size: self.font_size,
            embed_fonts: format == "svg",
            title: doc.title.clone(),
            ..Default::default()
        };
        let png_opts = tiza_export::PngOptions {
            scale: self.scale,
            system_fonts: false,
        };
        let too_big = |e: tiza_export::Error| match e {
            tiza_export::Error::TooBig(..)
            | tiza_export::Error::TooManyLines { .. }
            | tiza_export::Error::SvgTooBig(..)
            | tiza_export::Error::SvgTooManyLines { .. }
            | tiza_export::Error::Webp(tiza_export::webp::WebpError::Size(..)) => {
                invalid_argument(too_big_message(&e))
            }
            other => McpError::internal_error(other.to_string(), None),
        };
        let mut anim_frames = 0usize;
        let bytes = match format {
            "txt" => (layout.canvas.to_plain_text() + "\n").into_bytes(),
            "ansi" => layout.canvas.to_ansi().into_bytes(),
            "md" => layout.canvas.to_markdown(doc.title.as_deref()).into_bytes(),
            "gif" | "webp" if animated => {
                // Every step, the typed moves between them, one geometry
                // for the take.
                // Made once; each pass of an encoder plays it back.
                let take = tiza_core::anim::Take::record(
                    doc,
                    &theme,
                    &tiza_core::anim::AnimOptions {
                        interval: self.interval,
                        width: layout.canvas.width,
                        line_numbers: doc.numbers_or(true),
                        from_zero: !self.skip_zero,
                        margin_cap: None,
                    },
                );
                anim_frames = take.len();
                let svg = tiza_export::SvgOptions {
                    embed_fonts: false,
                    title: None,
                    ..svg_opts.clone()
                };
                if format == "webp" {
                    tiza_export::to_webp_anim_from(
                        &take,
                        &theme,
                        &tiza_export::WebpAnimOptions {
                            svg,
                            png: png_opts,
                            plays: 0,
                        },
                    )
                    .map_err(too_big)?
                    .0
                } else {
                    tiza_export::to_gif_from(
                        &take,
                        &theme,
                        &tiza_export::GifOptions {
                            svg,
                            png: png_opts,
                            repeat: None,
                        },
                    )
                    .map_err(too_big)?
                    .0
                }
            }
            "png" => {
                // Over the limits: an error before the SVG, not after it.
                tiza_export::image_size(&layout.canvas, &svg_opts, &png_opts).map_err(too_big)?;
                let svg = tiza_export::to_svg(&layout.canvas, &theme, &svg_opts);
                tiza_export::to_png(&svg, &png_opts).map_err(too_big)?
            }
            "webp" => {
                tiza_export::image_size(&layout.canvas, &svg_opts, &png_opts).map_err(too_big)?;
                let svg = tiza_export::to_svg(&layout.canvas, &theme, &svg_opts);
                tiza_export::to_webp(&svg, &png_opts)
                    .map_err(too_big)?
                    .bytes
            }
            _ => {
                tiza_export::svg_size(&layout.canvas, &svg_opts).map_err(too_big)?;
                tiza_export::to_svg(&layout.canvas, &theme, &svg_opts).into_bytes()
            }
        };
        test_hook("export:slow-write");
        // Written next to the target and renamed under the lock the
        // deadline takes: past the deadline nothing appears, and
        // the reply never says «nothing was written» about a file that was.
        let target = std::path::Path::new(&self.path);
        let tmp = target.with_file_name(format!(
            ".{}.tiza-{}.tmp",
            target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            std::process::id()
        ));
        if *self.phase.lock().unwrap_or_else(|e| e.into_inner()) == ExportPhase::Abandoned {
            return Err(McpError::internal_error("abandoned", None));
        }
        std::fs::write(&tmp, &bytes).map_err(|e| {
            McpError::internal_error(format!("could not write {}: {e}", self.path), None)
        })?;
        {
            let mut ph = self.phase.lock().unwrap_or_else(|e| e.into_inner());
            if *ph == ExportPhase::Abandoned {
                let _ = std::fs::remove_file(&tmp);
                return Err(McpError::internal_error("abandoned", None));
            }
            if let Err(e) = std::fs::rename(&tmp, target) {
                let _ = std::fs::remove_file(&tmp);
                return Err(McpError::internal_error(
                    format!("could not write {}: {e}", self.path),
                    None,
                ));
            }
            *ph = ExportPhase::Written;
        }
        let mut reply = json!({
            "path": self.path,
            "bytes": bytes.len(),
            "format": format,
            "step": step,
        });
        said.extend(warning);
        if !said.is_empty() {
            reply["warning"] = json!(said.join("; "));
        }
        if animated {
            reply["frames"] = json!(anim_frames);
            reply["steps"] = json!(format!("{first}-{}", last.max(first)));
            reply.as_object_mut().unwrap().remove("step");
        }
        Ok(text(pretty(&reply)))
    }
}

/// The resources the live instance exposes: a client that supports
/// resources can read the scene, the code or the rendered frame without a
/// tool call (and attach them as context).
const RESOURCES: &[(&str, &str, &str, &str)] = &[
    (
        "tiza://scene",
        "scene",
        "The live document as JSON (code embedded, annotations, styles, step, theme). Read-only: change the document with the tools (add_box, update_annotation…).",
        "application/json",
    ),
    (
        "tiza://code",
        "code",
        "The code currently shown, with line numbers, as text.",
        "text/plain",
    ),
    (
        "tiza://render",
        "render",
        "What the audience sees: the current step as plain text (box-drawing characters).",
        "text/plain",
    ),
    (
        "tiza://render.md",
        "render.md",
        "The same frame as a Markdown fenced block, ready to paste.",
        "text/markdown",
    ),
];

impl TizaServer {
    async fn resource_text(&self, uri: &str) -> Result<String, McpError> {
        match uri {
            "tiza://scene" => {
                let v = self.call("scene.get", json!({})).await?;
                Ok(pretty(&v))
            }
            "tiza://code" => {
                let v = self.call("code.get", json!({})).await?;
                Ok(v["text"].as_str().unwrap_or("").to_string())
            }
            "tiza://render" => {
                let v = self.call("render.text", json!({})).await?;
                Ok(v["text"].as_str().unwrap_or("").to_string())
            }
            "tiza://render.md" => {
                let v = self.call("render.text", json!({})).await?;
                let status = self.call("status", json!({})).await?;
                let title = status["file"].as_str().map(|f| {
                    std::path::Path::new(f)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| f.to_string())
                });
                let mut out = String::new();
                if let Some(t) = title {
                    // One line, as `render --format md`.
                    let t = tiza_core::printable::printable_line(&t);
                    out.push_str(&format!("**{t}** (step {})\n\n", status["step"]));
                }
                out.push_str("```text\n");
                out.push_str(v["text"].as_str().unwrap_or("").trim_end());
                out.push_str("\n```\n");
                Ok(out)
            }
            _ => Err(McpError::resource_not_found(
                format!(
                    "unknown resource; available: {}",
                    RESOURCES.iter().map(|r| r.0).collect::<Vec<_>>().join(", ")
                ),
                None,
            )),
        }
    }
}

#[tool_handler]
impl ServerHandler for TizaServer {
    /// The tool router's call, with one change: arguments that do
    /// not deserialize come back as `failed to deserialize parameters:
    /// invalid type: string "…"`, raised here before the instance is asked:
    /// the value serde could not read is taken out of that message
    /// ([`tiza_core::error::serde_text`]), as the instance does with its
    /// own.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        let tool = request.name.to_string();
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        // Every call answers: a panic while it runs (a bug) is an
        // error reply, never a call left waiting forever.
        let mut resp = answer_always(async {
            test_hook(&format!("tool:{tool}"));
            self.tool_router.call(tcc).await
        })
        .await
        .map_err(|p| panicked(&format!("tool {tool}"), p))?
        .map_err(|mut e| {
            // An error can quote a path or a text the agent sent (an
            // export's `could not write …`): shown, never raw.
            if let std::borrow::Cow::Owned(m) = tiza_core::printable::printable_text(&e.message) {
                e.message = m.into();
            }
            e
        })?;
        if let rmcp::model::CallToolResponse::Complete(r) = &mut resp
            && r.is_error == Some(true)
        {
            for c in &mut r.content {
                if let rmcp::model::ContentBlock::Text(t) = c {
                    if t.text.starts_with("failed to deserialize parameters") {
                        t.text = tiza_core::error::serde_text(&t.text);
                    }
                    // A field name serde quotes is shown, not sent raw.
                    t.text = tiza_core::printable::printable_text(&t.text).into_owned();
                }
            }
        }
        Ok(resp)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(
            RESOURCES
                .iter()
                .map(|(uri, name, desc, mime)| {
                    Resource::new(*uri, *name)
                        .with_description(*desc)
                        .with_mime_type(*mime)
                })
                .collect(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let text = answer_always(self.resource_text(&request.uri))
            .await
            .map_err(|p| panicked("resource", p))??;
        let mime = RESOURCES
            .iter()
            .find(|r| r.0 == request.uri)
            .map(|r| r.3)
            .unwrap_or("text/plain");
        // The code and the render with their control characters shown;
        // the scene's JSON has them escaped already.
        let text = tiza_core::printable::printable_text(&text).into_owned();
        Ok(
            ReadResourceResult::new(vec![ResourceContents::TextResourceContents {
                uri: request.uri,
                mime_type: Some(mime.into()),
                text,
                meta: None,
            }])
            .into(),
        )
    }

    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
            .with_server_info(Implementation::new("tiza", tiza_core::VERSION))
            .with_instructions(
                "Tiza: draw on top of code in your terminal to explain, debug and teach. \
                 Use it to explain code to a person or another agent, live or recorded, step by step: \
                 a code walkthrough, a lesson, a pull request review, a debugging session with a person or an agent, a tutorial or a presentation. \
                 It draws a box, arrow, highlight, note, callout or mark (move, read, write, drop…) on the live code, \
                 groups them in steps, can type code or change it at a step (a move, `add_move`), link another file, \
                 and export PNG, SVG, GIF or text.\n\
                 Flow: `status` → `get_code` → annotate with `find` (exact text) and `step`; each reply says whether it shows at the step on screen.\n\
                 Then `next_step`/`goto_step`, `render_text` to verify and `export_image` to save.\n\
                 Annotating at step N creates step N (goto_step goes no further than the last). \
                 A `find_end` of `}` takes the first brace after find_start, often an inner block's. \
                 Keep labels short (~100 columns). \
                 Same views without a call: `tiza://scene`, `tiza://code`, `tiza://render`, `tiza://render.md`.",
            )
    }
}

/// A future whose panic is caught and returned: the tool router
/// runs each call in a task of its own, and a panic there drops the task
/// without a reply, so the client waits forever while the server still
/// answers other calls. With this, a panic becomes an error reply.
struct AnswerAlways<'a, T>(std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>);

impl<T> std::future::Future for AnswerAlways<'_, T> {
    type Output = Result<T, Box<dyn std::any::Any + Send>>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let inner = &mut self.0;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inner.as_mut().poll(cx))) {
            Ok(std::task::Poll::Ready(v)) => std::task::Poll::Ready(Ok(v)),
            Ok(std::task::Poll::Pending) => std::task::Poll::Pending,
            Err(p) => std::task::Poll::Ready(Err(p)),
        }
    }
}

fn answer_always<'a, T>(
    f: impl std::future::Future<Output = T> + Send + 'a,
) -> AnswerAlways<'a, T> {
    AnswerAlways(Box::pin(f))
}

/// The error reply for a call that panicked: what failed, never the
/// panic's own text (it may quote document text); the panic itself
/// is on the server's stderr.
fn panicked(what: &str, _payload: Box<dyn std::any::Any + Send>) -> McpError {
    McpError::internal_error(
        format!(
            "internal error: the {what} call failed with a bug in Tiza (a panic; its message is on the MCP server's stderr). Please report it; the live instance is unaffected"
        ),
        None,
    )
}

/// Starts the MCP server over stdio (blocks until the client closes).
pub async fn serve(socket: PathBuf) -> anyhow::Result<()> {
    let service = TizaServer::new(socket).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call that panics answers with an error instead of
    /// leaving the client waiting; one that does not keeps its result.
    #[tokio::test]
    async fn answer_always_turns_a_panic_into_an_error_reply() {
        let boom = answer_always(async {
            tokio::task::yield_now().await;
            let v: Vec<u32> = Vec::new();
            v[0]
        })
        .await;
        let e = panicked("tool export_image", boom.unwrap_err());
        assert!(
            e.message.contains("export_image") && e.message.contains("bug in Tiza"),
            "{}",
            e.message
        );
        assert_eq!(answer_always(async { 7 }).await.unwrap(), 7);
    }
}
