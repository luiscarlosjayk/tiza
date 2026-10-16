//! MCP → local API bridge. Each tool is exactly one socket method; that way
//! an agent and the presenter do the same thing and the TUI is the only one
//! that touches the document.

use std::path::PathBuf;

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    schemars, tool, tool_handler, tool_router,
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
    #[schemars(description = "First line enclosed by the box (1-based)")]
    pub from_line: u32,
    #[schemars(description = "Last line enclosed by the box (inclusive)")]
    pub to_line: u32,
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
pub struct HighlightParams {
    pub at: At,
    pub label: Option<String>,
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray or #rrggbb. Arrows without a color rotate through the palette."
    )]
    pub color: Option<String>,
    pub id: Option<String>,
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
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportParams {
    #[schemars(
        description = "Where to write the image. The extension picks the format: .png or .svg"
    )]
    pub path: String,
    #[schemars(description = "Step to show; defaults to the current one")]
    pub step: Option<u32>,
    #[schemars(description = "Width in columns; defaults to the live terminal's width")]
    pub width: Option<usize>,
    #[schemars(description = "dark or light; defaults to the document's theme")]
    pub theme: Option<String>,
    #[schemars(description = "PNG device scale (default 2)")]
    pub scale: Option<f32>,
    #[schemars(description = "Font size in px (default 16)")]
    pub font_size: Option<f32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CalloutParams {
    #[schemars(description = "First line the callout explains (1-based)")]
    pub from_line: u32,
    #[schemars(description = "Last line it explains; defaults to from_line")]
    pub to_line: Option<u32>,
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
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SpotlightParams {
    #[schemars(description = "First line to highlight (the rest of the code is dimmed)")]
    pub from_line: Option<u32>,
    #[schemars(description = "Last line to highlight; defaults to from_line")]
    pub to_line: Option<u32>,
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
pub struct RenderParams {
    #[schemars(description = "Width in columns; defaults to that of the terminal running the TUI")]
    pub width: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CodeDrawServer {
    socket: PathBuf,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

fn text(v: impl ToString) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(v.to_string())])
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

#[tool_router]
impl CodeDrawServer {
    pub fn new(socket: PathBuf) -> Self {
        Self {
            socket,
            tool_router: Self::tool_router(),
        }
    }

    /// Calls the live instance. Turns connection errors into messages useful to the agent.
    async fn call(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let path = self.socket.clone();
        let method_owned = method.to_string();
        let resp = tokio::task::spawn_blocking(move || codedraw_tui::ipc::call(&path, &method_owned, params))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .map_err(|e| {
                McpError::internal_error(
                    format!(
                        "No CodeDraw instance is listening on {} ({e}). Ask the user to open `codedraw <file>` in a terminal, or pass --socket/--session to the MCP server.",
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
        description = "State of the live CodeDraw instance: open file, lines, annotations, current step and terminal size. Call it first to learn whether an instance exists."
    )]
    async fn status(&self, Parameters(_p): Parameters<Empty>) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(&self.call("status", json!({})).await?)))
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
        description = "Draws a box around a range of lines, with an optional label on the top edge."
    )]
    async fn add_box(
        &self,
        Parameters(p): Parameters<BoxParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self
            .call("annotation.add", json!({"type": "box", "from_line": p.from_line, "to_line": p.to_line, "label": p.label, "step": p.step, "id": p.id, "color": p.color}))
            .await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Draws an arrow from one point of the code to another (e.g. from `let s` to `takes(s)`), with an optional label. Use `find` to anchor by text."
    )]
    async fn add_arrow(
        &self,
        Parameters(p): Parameters<ArrowParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self
            .call("annotation.add", json!({"type": "arrow", "from": p.from, "to": p.to, "label": p.label, "step": p.step, "id": p.id, "color": p.color}))
            .await?;
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
        description = "Highlights a text fragment (a variable, an expression) with an optional label."
    )]
    async fn highlight(
        &self,
        Parameters(p): Parameters<HighlightParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self.call("annotation.add", json!({"type": "highlight", "at": p.at, "label": p.label, "step": p.step, "id": p.id, "color": p.color})).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Semantic mark on a fragment: a built-in kind (warn, error, ok, read, write, move, drop, scope…), a reusable style from define_style, or a one-off inline style. Each shows glyph + legend; language-agnostic."
    )]
    async fn add_mark(
        &self,
        Parameters(p): Parameters<MarkParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self
            .call("annotation.add", json!({"type": "mark", "kind": p.kind, "style": p.style.unwrap_or_default(), "at": p.at, "label": p.label, "step": p.step, "id": p.id, "color": p.color}))
            .await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Changes the step, color or label of an existing annotation, or moves it (`at` for highlights/marks/notes, `from`/`to` for arrows, `from_line`/`to_line` for boxes, callouts, spotlights and notes) keeping its type, kind and style. Fields that do not apply to the annotation's type fail with invalid_argument; unknown fields are rejected."
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
        let v = self
            .call(
                "annotation.add",
                json!({"type": "callout", "from_line": p.from_line, "to_line": p.to_line, "text": p.text, "title": p.title, "placement": placement, "width": p.width, "step": p.step, "color": p.color, "id": p.id}),
            )
            .await?;
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
                json!({"type": "spotlight", "from_line": p.from_line, "to_line": p.to_line, "ids": p.ids.unwrap_or_default(), "until": p.until, "step": p.step, "id": p.id}),
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
        description = "Exports the scene as an image file (.png or .svg) with embedded fonts, for thumbnails, posts and docs. Renders from the live document without touching the TUI."
    )]
    async fn export_image(
        &self,
        Parameters(p): Parameters<ExportParams>,
    ) -> Result<CallToolResult, McpError> {
        let ext = std::path::Path::new(&p.path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let is_png = match ext.as_deref() {
            Some("png") => true,
            Some("svg") => false,
            _ => {
                return Err(McpError::invalid_params(
                    "path must end in .png or .svg",
                    None,
                ));
            }
        };
        let mut scene: codedraw_core::protocol::Scene =
            serde_json::from_value(self.call("scene.get", json!({})).await?)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        codedraw_core::fs::resolve_code(&mut scene.document, None)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let status: codedraw_core::protocol::Status =
            serde_json::from_value(self.call("status", json!({})).await?)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let theme_name = match p.theme.as_deref() {
            Some(t) => match t.trim().to_ascii_lowercase().as_str() {
                "dark" | "oscuro" => codedraw_core::ThemeName::Dark,
                "light" | "claro" => codedraw_core::ThemeName::Light,
                _ => {
                    return Err(McpError::invalid_params(
                        format!("unknown theme: {t} (dark, light)"),
                        None,
                    ));
                }
            },
            None => scene
                .document
                .theme
                .unwrap_or(codedraw_core::ThemeName::Dark),
        };
        let theme = codedraw_core::Theme::from_name(theme_name);
        let layout = codedraw_core::layout::layout(
            &scene.document,
            &theme,
            &codedraw_core::layout::LayoutOptions {
                width: p
                    .width
                    .unwrap_or((status.cols as usize).saturating_sub(2).max(40)),
                step: p.step.unwrap_or(scene.step),
                focus: None,
                line_numbers: true,
                tab_width: 4,
            },
        );
        let svg_opts = codedraw_export::SvgOptions {
            font_size: p.font_size.unwrap_or(16.0),
            embed_fonts: !is_png,
            title: scene.document.title.clone(),
            ..Default::default()
        };
        let svg = codedraw_export::to_svg(&layout.canvas, &theme, &svg_opts);
        let bytes = if is_png {
            codedraw_export::to_png(
                &svg,
                &codedraw_export::PngOptions {
                    scale: p.scale.unwrap_or(2.0),
                    system_fonts: false,
                },
            )
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
        } else {
            svg.into_bytes()
        };
        std::fs::write(&p.path, &bytes).map_err(|e| {
            McpError::internal_error(format!("could not write {}: {e}", p.path), None)
        })?;
        Ok(text(pretty(&json!({
            "path": p.path,
            "bytes": bytes.len(),
            "format": if is_png { "png" } else { "svg" },
            "step": p.step.unwrap_or(scene.step),
        }))))
    }
}

#[tool_handler]
impl ServerHandler for CodeDrawServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("codedraw", codedraw_core::VERSION))
            .with_instructions(
                "CodeDraw is a TUI that shows code and lets you draw on top of it (boxes, arrows, highlights, notes, callouts and \
                 language-agnostic marks such as move, read, write, drop, scope) to explain it on video. Recommended flow: \
                 1) `status` to confirm there is an instance; 2) `get_code` to see exact lines and text; \
                 3) annotate with `add_mark`, `add_arrow`, `add_box`, `add_note` (one line), `add_callout` (a paragraph), `highlight`, using `find` with the exact text; \
                 4) group into steps with `step` and advance with `next_step`; 5) `render_text` to verify, `export_image` for a PNG/SVG thumbnail. \
                 Keep labels short: they are drawn in a ~100-column terminal.",
            )
    }
}

/// Starts the MCP server over stdio (blocks until the client closes).
pub async fn serve(socket: PathBuf) -> anyhow::Result<()> {
    let service = CodeDrawServer::new(socket).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
