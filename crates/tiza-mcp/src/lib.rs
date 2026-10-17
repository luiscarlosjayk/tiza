//! MCP → local API bridge. Each tool is exactly one socket method; that way
//! an agent and the presenter do the same thing and the TUI is the only one
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
            McpError::invalid_params(
                format!("unknown border: {b} (use {})", Stroke::NAMES.join(", ")),
                None,
            )
        })?;
        m.insert("border".into(), serde_json::to_value(v).unwrap());
    }
    if let Some(c) = corners {
        let v = Route::parse(c).ok_or_else(|| {
            McpError::invalid_params(
                format!("unknown corners: {c} (use {})", Route::NAMES.join(", ")),
                None,
            )
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
                McpError::invalid_params(
                    format!(
                        "unknown emphasis: {e} (use {})",
                        tiza_core::model::Emphasis::NAMES.join(", ")
                    ),
                    None,
                )
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
            McpError::invalid_params(
                format!("unknown stroke: {s} (use {})", Stroke::NAMES.join(", ")),
                None,
            )
        })?;
        m.insert("stroke".into(), serde_json::to_value(v).unwrap());
    }
    if let Some(r) = route {
        let v = Route::parse(r).ok_or_else(|| {
            McpError::invalid_params(
                format!("unknown route: {r} (use {})", Route::NAMES.join(", ")),
                None,
            )
        })?;
        m.insert("route".into(), serde_json::to_value(v).unwrap());
    }
    if start.is_some() || end.is_some() {
        let mut h = serde_json::Map::new();
        for (key, val) in [("start", start), ("end", end)] {
            if let Some(v) = val {
                let hv = Head::parse(v).ok_or_else(|| {
                    McpError::invalid_params(
                        format!("unknown head: {v} (use {})", Head::NAMES.join(", ")),
                        None,
                    )
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
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RedactParams {
    #[schemars(
        description = "Where the secret is. `{line, col, end}` masks those columns; `{line, find}` masks what FOLLOWS the found text to the end of the line (`find` is the anchor before the secret — KEY=, Bearer, password: — never the secret itself, which is refused)."
    )]
    pub at: At,
    #[schemars(
        description = "Trailing characters left visible after the bullets (0-8, default 0)"
    )]
    pub keep: Option<u8>,
    pub label: Option<String>,
    pub step: Option<u32>,
    pub id: Option<String>,
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
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MarkParams {
    #[schemars(
        description = "Built-in kind (info, warn, error, ok, question, important, read, write, move, new, drop, scope, flow, todo; aliases like borrow/borrow_mut/lifetime work), a style defined with define_style, or any name when `style` carries a glyph."
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
        description = "New label (or new text if it is a note, new title if it is a callout). An empty string removes it."
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
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportParams {
    #[schemars(
        description = "Where to write the file. The extension picks the format: .png, .svg, .txt, .ansi, .md, or .gif (an animated GIF of every step, drawn by Tiza itself)"
    )]
    pub path: String,
    #[schemars(description = "Step to show; defaults to the current one")]
    pub step: Option<u32>,
    #[schemars(description = "Width in columns; defaults to the live terminal's width")]
    pub width: Option<usize>,
    #[schemars(description = "dark or light; defaults to the document's theme")]
    pub theme: Option<String>,
    #[schemars(description = "PNG device scale (default 2; 1 for a GIF)")]
    pub scale: Option<f32>,
    #[schemars(
        description = "GIF: seconds each step stays on screen (default 2; the last one twice as long)"
    )]
    pub interval: Option<f64>,
    #[schemars(
        description = "GIF: start at the first annotated step instead of step 0 (default false)"
    )]
    pub skip_zero: Option<bool>,
    #[schemars(description = "Font size in px (default 16)")]
    pub font_size: Option<f32>,
    #[schemars(
        description = "SVG/PNG: set notes, labels and legends in the brand serif (Tiza Serif) instead of the monospace font (default false)"
    )]
    pub brand_font: Option<bool>,
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
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SpotlightParams {
    #[schemars(description = "First line to highlight (the rest of the code is dimmed)")]
    pub from_line: Option<u32>,
    #[schemars(description = "Last line to highlight; defaults to from_line")]
    pub to_line: Option<u32>,
    #[schemars(
        description = "Text on the first line of the range, instead of from_line (with find_end)"
    )]
    pub find_start: Option<String>,
    #[schemars(description = "Text on the last line of the range, searched after find_start")]
    pub find_end: Option<String>,
    #[schemars(
        description = "Ids of annotations to highlight; the others are dimmed and their code stays in focus"
    )]
    pub ids: Option<Vec<String>>,
    #[schemars(description = "Step at which it activates (defaults to the current one)")]
    pub step: Option<u32>,
    #[schemars(description = "Last step at which it stays active; defaults to `step` only")]
    pub until: Option<u32>,
    pub id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ClearParams {
    #[schemars(
        description = "If given, removes only the annotations of that step; otherwise all of them"
    )]
    pub step: Option<u32>,
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
    #[schemars(description = "`insert`, `replace` or `delete` (whole lines)")]
    pub op: String,
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
        description = "replace/delete: text of the one line to change (must match exactly one line)"
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
        description = "Which match of find/after/before/find_start to use when the text appears more than once: 1-based, counted in reading order over the whole code as it is at that step. Without it, more than one match is an error"
    )]
    pub occurrence: Option<u32>,
    #[schemars(description = "The same for find_end")]
    pub end_occurrence: Option<u32>,
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
pub struct FocusParams {
    #[schemars(description = "First focused line; omit both to clear the focus")]
    pub from_line: Option<u32>,
    pub to_line: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ThemeParams {
    #[schemars(description = "dark, light or toggle")]
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
    #[schemars(description = "Width in columns; defaults to that of the terminal running the TUI")]
    pub width: Option<u32>,
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

fn text(v: impl ToString) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(v.to_string())])
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
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
                McpError::internal_error(
                    format!(
                        "No Tiza instance is listening on {} ({e}). Ask the user to open `tiza <file>` in a terminal, or pass --socket/--session to the MCP server.",
                        self.socket.display()
                    ),
                    None,
                )
            })?;
        if let Some(err) = resp.error {
            return Err(McpError::invalid_params(
                format!("{} ({})", err.message, err.code),
                None,
            ));
        }
        Ok(resp.result.unwrap_or(Value::Null))
    }

    #[tool(
        description = "State of the live Tiza instance: open file, lines, annotations, current step and terminal size, plus `bridge` (this server's version/build; `binary_rebuilt`/`hint` when tiza was rebuilt and the client should reconnect). Call it first to learn whether an instance exists."
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
        description = "Returns the code currently shown, with line numbers. Read it before annotating to pick exact lines and texts."
    )]
    async fn get_code(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        let v = self.call("code.get", json!({})).await?;
        Ok(text(
            v.get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
        ))
    }

    #[tool(description = "Lists the existing annotations (id, step, kind and where they are).")]
    async fn list_annotations(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        let v = self.call("scene.list", json!({})).await?;
        let lines: Vec<String> = v
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(text(if lines.is_empty() {
            "(no annotations)".to_string()
        } else {
            lines.join("\n")
        }))
    }

    #[tool(
        description = "Draws a box around a range of lines, with an optional label on the top edge. Give from_line/to_line, or find_start/find_end (two texts that delimit the range, like Demo Time placeholders: `// begin` … `// end`, or code such as `fn takes` … `}`)."
    )]
    async fn add_box(
        &self,
        Parameters(p): Parameters<BoxParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = json!({"type": "box", "from_line": p.from_line, "to_line": p.to_line, "find_start": p.find_start, "find_end": p.find_end, "label": p.label, "step": p.step, "id": p.id, "color": p.color});
        for (k, v) in border_style_json(&p.border, &p.corners, p.fill)? {
            params[k] = v;
        }
        let v = self.call("annotation.add", params).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Draws an arrow from one point of the code to another (e.g. from `let s` to `takes(s)`), with an optional label. Use `find` to anchor by text. Stroke, corners and heads are the author's choice: dashed for a borrow, thick for a move, head_end none + head_start none for a plain connector."
    )]
    async fn add_arrow(
        &self,
        Parameters(p): Parameters<ArrowParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = json!({"type": "arrow", "from": p.from, "to": p.to, "label": p.label, "step": p.step, "id": p.id, "color": p.color});
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
        Ok(text(pretty(&v)))
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
                json!({"type": "note", "line": p.line, "text": p.text, "step": p.step, "id": p.id, "color": p.color}),
            )
            .await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Highlights a text fragment (a variable, an expression) with an optional label; emphasis picks fill (tint, default), underline, brackets or strike."
    )]
    async fn highlight(
        &self,
        Parameters(p): Parameters<HighlightParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = json!({"type": "highlight", "at": p.at, "label": p.label, "step": p.step, "id": p.id, "color": p.color});
        if let Some(e) = emphasis_json(&p.emphasis)? {
            params["emphasis"] = e;
        }
        let v = self.call("annotation.add", params).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Masks a secret (a token, a password, a URL with credentials) with eight bullets on every surface at once — the screen, every export, the recordings and what get_code / the resources return — without touching the file on disk. Only the columns are stored, never the text. `at.find` is the text BEFORE the secret (`API_KEY=`, `Bearer`); the mask runs from there to the end of the line. Active from step 0 whatever `step`. Several per line allowed."
    )]
    async fn redact(
        &self,
        Parameters(p): Parameters<RedactParams>,
    ) -> Result<CallToolResult, McpError> {
        let params = json!({"type": "redact", "at": p.at, "keep": p.keep.unwrap_or(0), "label": p.label, "step": p.step, "id": p.id});
        let v = self.call("annotation.add", params).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Semantic mark on a fragment: a built-in kind (warn, error, ok, read, write, move, drop, scope…), a reusable style from define_style, or a one-off inline style. Each shows glyph + legend; language-agnostic."
    )]
    async fn add_mark(
        &self,
        Parameters(p): Parameters<MarkParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut params = json!({"type": "mark", "kind": p.kind, "style": p.style.unwrap_or_default(), "at": p.at, "label": p.label, "step": p.step, "id": p.id, "color": p.color});
        if let Some(e) = emphasis_json(&p.emphasis)? {
            params["emphasis"] = e;
        }
        let v = self.call("annotation.add", params).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Changes the step, color, label, arrow style (stroke, route, head_start, head_end), box/callout border (border, corners, fill) or highlight/mark emphasis (fill, underline, brackets, strike) of an existing annotation, or moves it (`at` for highlights/marks/notes, `from`/`to` for arrows, `from_line`/`to_line` for boxes, callouts, spotlights and notes) keeping its type, kind and style. Fields that do not apply to the annotation's type fail with invalid_argument; unknown fields are rejected."
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
        let v = self.call("annotation.update", params).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Removes every annotation at once (or only those of one step with `step`). Useful between takes. The presenter can undo it with `u`."
    )]
    async fn clear_annotations(
        &self,
        Parameters(p): Parameters<ClearParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("scene.clear", json!({"step": p.step})).await?,
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
            Some(other) => {
                return Err(McpError::invalid_params(
                    format!("unknown placement: {other} (margin, below, auto)"),
                    None,
                ));
            }
        };
        let mut params = json!({"type": "callout", "from_line": p.from_line, "to_line": p.to_line, "find_start": p.find_start, "find_end": p.find_end, "text": p.text, "title": p.title, "placement": placement, "width": p.width, "step": p.step, "color": p.color, "id": p.id});
        for (k, v) in border_style_json(&p.border, &p.corners, p.fill)? {
            params[k] = v;
        }
        let v = self.call("annotation.add", params).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Directs attention in a step: dims the code outside a line range and/or the annotations outside `ids`. Use it as a 'look here' step between reveals. Active from `step` to `until`."
    )]
    async fn add_spotlight(
        &self,
        Parameters(p): Parameters<SpotlightParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self
            .call(
                "annotation.add",
                json!({"type": "spotlight", "from_line": p.from_line, "to_line": p.to_line, "find_start": p.find_start, "find_end": p.find_end, "ids": p.ids.unwrap_or_default(), "until": p.until, "step": p.step, "id": p.id}),
            )
            .await?;
        Ok(text(pretty(&v)))
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
        description = "Lists the mark kinds available: the document's styles and the built-in ones, with their glyphs."
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
        description = "Adds a move (ADR 0007): a `code.patch` that inserts, replaces or deletes whole lines when the presenter reaches `at_step` (reverted when going back). Targets are matched on the code as shown at that step and must match exactly once. Annotations of earlier steps are shifted through the patch; those that lose every line are hidden as (unanchored) unless `rebind` names where they go. With `typing: line|char` the content is typed in place in the TUI."
    )]
    async fn add_move(
        &self,
        Parameters(p): Parameters<AddMoveParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut m = json!({"at_step": p.at_step, "action": "code.patch", "op": p.op});
        let o = m.as_object_mut().expect("object");
        let mut put = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                o.insert(k.to_string(), v);
            }
        };
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
        put("rebind", p.rebind.map(Value::Object));
        Ok(text(pretty(&self.call("move.add", m).await?)))
    }

    #[tool(description = "Deletes a move by its 1-based index (see list_moves).")]
    async fn delete_move(
        &self,
        Parameters(p): Parameters<MoveIndexParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("move.delete", json!({"index": p.index})).await?,
        )))
    }

    #[tool(
        description = "Lists the moves (index, step, summary) and the annotation ids unanchored at the current step."
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

    #[tool(description = "Advances one step in the presentation.")]
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

    #[tool(description = "Changes the theme: dark, light or toggle.")]
    async fn set_theme(
        &self,
        Parameters(p): Parameters<ThemeParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("theme.set", json!({"theme": p.theme})).await?,
        )))
    }

    #[tool(
        description = "Turns follow mode on or off: with it on, saving the source file in the editor reloads the code in the TUI and the annotations follow their text. Fails when the code is embedded (no file to follow). Returns the follow status (enabled, path, reloads, last_reload)."
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
        description = "Returns what is on screen as plain text (code + annotations of the current step). Use it to check the result of what you drew."
    )]
    async fn render_text(
        &self,
        Parameters(p): Parameters<RenderParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self.call("render.text", json!({"width": p.width})).await?;
        Ok(text(
            v.get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
        ))
    }

    #[tool(
        description = "Exports the scene to a file: .png or .svg (embedded fonts) for thumbnails and posts, .txt (plain box-drawing text), .ansi (24-bit colors, for `cat`) or .md (fenced block for a README, an issue or a chat). Renders from the live document without touching the TUI."
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
            _ => {
                return Err(McpError::invalid_params(
                    "path must end in .png, .svg, .txt, .ansi, .md or .gif",
                    None,
                ));
            }
        };
        let is_png = format == "png";
        let mut scene: tiza_core::protocol::Scene =
            serde_json::from_value(self.call("scene.get", json!({})).await?)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        tiza_core::fs::resolve_code(&mut scene.document, None)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let status: tiza_core::protocol::Status =
            serde_json::from_value(self.call("status", json!({})).await?)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let theme_name = match p.theme.as_deref() {
            Some(t) => match t.trim().to_ascii_lowercase().as_str() {
                "dark" | "oscuro" => tiza_core::ThemeName::Dark,
                "light" | "claro" => tiza_core::ThemeName::Light,
                _ => {
                    return Err(McpError::invalid_params(
                        format!("unknown theme: {t} (dark, light)"),
                        None,
                    ));
                }
            },
            None => scene.document.theme.unwrap_or(tiza_core::ThemeName::Dark),
        };
        let theme = tiza_core::Theme::from_name(theme_name);
        let layout = tiza_core::layout::layout(
            &scene.document,
            &theme,
            &tiza_core::layout::LayoutOptions {
                width: match p.width {
                    Some(w) => tiza_core::layout::check_width(w)
                        .map_err(|e| McpError::invalid_params(e.to_string(), None))?,
                    None => (status.cols as usize).saturating_sub(2).max(40),
                },
                step: p.step.unwrap_or(scene.step),
                focus: None,
                line_numbers: scene.document.numbers,
                tab_width: 4,
                typing: false,
                redact: true,
                stable: true,
                margin_cap: scene.document.margin.map(usize::from),
            },
        );
        let bytes = match format {
            "txt" => Some((layout.canvas.to_plain_text() + "\n").into_bytes()),
            "ansi" => Some(layout.canvas.to_ansi().into_bytes()),
            "md" => Some(
                layout
                    .canvas
                    .to_markdown(scene.document.title.as_deref())
                    .into_bytes(),
            ),
            _ => None,
        };
        let bad = |e: tiza_core::Error| McpError::invalid_params(e.to_string(), None);
        let is_gif = format == "gif";
        let scale =
            tiza_core::layout::check_scale(p.scale.unwrap_or(if is_gif { 1.0 } else { 2.0 }))
                .map_err(bad)?;
        let font_size =
            tiza_core::layout::check_font_size(p.font_size.unwrap_or(16.0)).map_err(bad)?;
        let svg_opts = tiza_export::SvgOptions {
            font_size,
            embed_fonts: !is_png,
            brand_font: p.brand_font.unwrap_or(false),
            title: scene.document.title.clone(),
            ..Default::default()
        };
        let too_big = |e: tiza_export::Error| match e {
            tiza_export::Error::TooBig(..) => McpError::invalid_params(e.to_string(), None),
            other => McpError::internal_error(other.to_string(), None),
        };
        let mut gif_frames = 0usize;
        let bytes = if let Some(b) = bytes {
            b
        } else if is_gif {
            // Every step, the typed moves between them, one geometry for the
            // take, masks on (t-120).
            let frames: Vec<(tiza_core::canvas::Canvas, u64)> = tiza_core::anim::frames(
                &scene.document,
                &theme,
                &tiza_core::anim::AnimOptions {
                    interval: p.interval.unwrap_or(2.0),
                    width: layout.canvas.width,
                    line_numbers: scene.document.numbers,
                    from_zero: !p.skip_zero.unwrap_or(false),
                },
            )
            .into_iter()
            .map(|f| (f.canvas, f.delay_ms))
            .collect();
            gif_frames = frames.len();
            let (bytes, _) = tiza_export::to_gif(
                &frames,
                &theme,
                &tiza_export::GifOptions {
                    svg: tiza_export::SvgOptions {
                        embed_fonts: false,
                        title: None,
                        ..svg_opts.clone()
                    },
                    png: tiza_export::PngOptions {
                        scale,
                        system_fonts: false,
                    },
                    repeat: None,
                },
            )
            .map_err(too_big)?;
            bytes
        } else if is_png {
            let svg = tiza_export::to_svg(&layout.canvas, &theme, &svg_opts);
            tiza_export::to_png(
                &svg,
                &tiza_export::PngOptions {
                    scale,
                    system_fonts: false,
                },
            )
            .map_err(too_big)?
        } else {
            tiza_export::to_svg(&layout.canvas, &theme, &svg_opts).into_bytes()
        };
        std::fs::write(&p.path, &bytes).map_err(|e| {
            McpError::internal_error(format!("could not write {}: {e}", p.path), None)
        })?;
        let mut reply = json!({
            "path": p.path,
            "bytes": bytes.len(),
            "format": format,
            "step": p.step.unwrap_or(scene.step),
        });
        if is_gif {
            reply["frames"] = json!(gif_frames);
            reply["steps"] = json!(format!("0-{}", scene.document.max_step()));
            reply.as_object_mut().unwrap().remove("step");
        }
        Ok(text(pretty(&reply)))
    }
}

/// The resources the live instance exposes (t-044): a client that supports
/// resources can read the scene, the code or the rendered frame without a
/// tool call (and attach them as context).
const RESOURCES: &[(&str, &str, &str, &str)] = &[
    (
        "tiza://scene",
        "scene",
        "The live document as JSON (code embedded, annotations, styles, step, theme).",
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
                    out.push_str(&format!("**{t}** (step {})\n\n", status["step"]));
                }
                out.push_str("```text\n");
                out.push_str(v["text"].as_str().unwrap_or("").trim_end());
                out.push_str("\n```\n");
                Ok(out)
            }
            other => Err(McpError::resource_not_found(
                format!(
                    "unknown resource {other}; available: {}",
                    RESOURCES.iter().map(|r| r.0).collect::<Vec<_>>().join(", ")
                ),
                None,
            )),
        }
    }
}

#[tool_handler]
impl ServerHandler for TizaServer {
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
        let text = self.resource_text(&request.uri).await?;
        let mime = RESOURCES
            .iter()
            .find(|r| r.0 == request.uri)
            .map(|r| r.3)
            .unwrap_or("text/plain");
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
                "Tiza is a TUI that shows code and lets you draw on top of it (boxes, arrows, highlights, notes, callouts and \
                 language-agnostic marks such as move, read, write, drop, scope) to explain it on video. Recommended flow: \
                 1) `status` to confirm there is an instance; 2) `get_code` to see exact lines and text; \
                 3) annotate with `add_mark`, `add_arrow`, `add_box`, `add_note` (one line), `add_callout` (a paragraph), `highlight`, using `find` with the exact text; \
                 4) group into steps with `step` and advance with `next_step`; 5) `render_text` to verify, `export_image` for a PNG/SVG thumbnail. \
                 Resources `tiza://scene`, `tiza://code`, `tiza://render` and `tiza://render.md` give the same views without a tool call. \
                 Keep labels short: they are drawn in a ~100-column terminal.",
            )
    }
}

/// Starts the MCP server over stdio (blocks until the client closes).
pub async fn serve(socket: PathBuf) -> anyhow::Result<()> {
    let service = TizaServer::new(socket).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
