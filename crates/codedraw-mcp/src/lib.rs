//! Puente MCP → API local. Cada herramienta es exactamente un método del
//! socket; así un agente y el presentador hacen lo mismo y la TUI es la única
//! que toca el documento.

use std::path::PathBuf;

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde_json::{Value, json};

/// Dónde anclar algo en el código. Lo más robusto para un agente es `find`:
/// el texto exacto a buscar (opcionalmente acotado a `line` y a la `occurrence`-ésima aparición).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct At {
    #[schemars(
        description = "Número de línea absoluto (1-based), como se ve en la TUI. Opcional si usas `find`."
    )]
    pub line: Option<u32>,
    #[schemars(
        description = "Offset de grafema dentro de la línea (0-based). Rara vez necesario; prefiere `find`."
    )]
    pub col: Option<u32>,
    #[schemars(
        description = "Texto exacto a buscar en el código (p. ej. \"takes(s)\"). Con `line`, busca primero en esa línea."
    )]
    pub find: Option<String>,
    #[schemars(description = "Qué aparición de `find` usar (1-based). Por defecto la primera.")]
    pub occurrence: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Empty {}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BoxParams {
    #[schemars(description = "Primera línea que encierra la caja (1-based)")]
    pub from_line: u32,
    #[schemars(description = "Última línea que encierra la caja (inclusive)")]
    pub to_line: u32,
    #[schemars(description = "Etiqueta que se dibuja en el borde superior")]
    pub label: Option<String>,
    #[schemars(
        description = "Paso de la presentación en que aparece (1-based). Por defecto, el paso actual."
    )]
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray o #rrggbb. Las flechas sin color rotan la paleta."
    )]
    pub color: Option<String>,
    #[schemars(description = "Id propio (p. ej. \"caja-takes\"); si se omite se asigna a1, a2…")]
    pub id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ArrowParams {
    pub from: At,
    pub to: At,
    #[schemars(description = "Texto junto a la punta de la flecha (p. ej. \"move\")")]
    pub label: Option<String>,
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray o #rrggbb. Las flechas sin color rotan la paleta."
    )]
    pub color: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NoteParams {
    #[schemars(description = "Línea a la que se ancla la nota")]
    pub line: u32,
    #[schemars(
        description = "Texto de la nota (una frase corta: va al margen derecho o debajo de la línea)"
    )]
    pub text: String,
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray o #rrggbb. Las flechas sin color rotan la paleta."
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
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray o #rrggbb. Las flechas sin color rotan la paleta."
    )]
    pub color: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MarkParams {
    #[schemars(
        description = "Tipo de marca semántica de Rust: move, borrow, borrow_mut, drop o lifetime"
    )]
    pub kind: String,
    pub at: At,
    #[schemars(description = "Texto adicional tras la leyenda (p. ej. \"s pasa a t\")")]
    pub label: Option<String>,
    pub step: Option<u32>,
    #[schemars(
        description = "Color: orange, blue, green, purple, pink, cyan, yellow, red, gray o #rrggbb. Las flechas sin color rotan la paleta."
    )]
    pub color: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateParams {
    pub id: String,
    #[schemars(description = "Nuevo paso en el que aparece")]
    pub step: Option<u32>,
    #[schemars(
        description = "Nuevo color (nombre de la paleta o #rrggbb); cadena vacía vuelve al automático"
    )]
    pub color: Option<String>,
    #[schemars(
        description = "Nueva etiqueta (o nuevo texto si es una nota). Cadena vacía la quita."
    )]
    pub label: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ClearParams {
    #[schemars(description = "Si se da, borra solo las anotaciones de ese paso; si no, todas")]
    pub step: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct IdParams {
    pub id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct StepParams {
    #[schemars(description = "Paso a mostrar (1-based). Se recorta al rango válido.")]
    pub step: u32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FocusParams {
    #[schemars(description = "Primera línea enfocada; omite ambas para quitar el enfoque")]
    pub from_line: Option<u32>,
    pub to_line: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ThemeParams {
    #[schemars(description = "dark, light o toggle")]
    pub theme: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RenderParams {
    #[schemars(
        description = "Ancho en columnas; por defecto, el de la terminal donde corre la TUI"
    )]
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

fn mark_kind(s: &str) -> Result<&'static str, McpError> {
    codedraw_core::MarkKind::parse(s)
        .map(|k| match k {
            codedraw_core::MarkKind::Move => "move",
            codedraw_core::MarkKind::Borrow => "borrow",
            codedraw_core::MarkKind::BorrowMut => "borrow_mut",
            codedraw_core::MarkKind::Drop => "drop",
            codedraw_core::MarkKind::Lifetime => "lifetime",
        })
        .ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "tipo de marca desconocido: {s} (move, borrow, borrow_mut, drop, lifetime)"
                ),
                None,
            )
        })
}

#[tool_router]
impl CodeDrawServer {
    pub fn new(socket: PathBuf) -> Self {
        Self {
            socket,
            tool_router: Self::tool_router(),
        }
    }

    /// Llama a la instancia viva. Convierte errores de conexión en mensajes útiles para el agente.
    async fn call(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let path = self.socket.clone();
        let method_owned = method.to_string();
        let resp = tokio::task::spawn_blocking(move || codedraw_tui::ipc::call(&path, &method_owned, params))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .map_err(|e| {
                McpError::internal_error(
                    format!(
                        "No hay una instancia de CodeDraw escuchando en {} ({e}). Pide al usuario que abra `codedraw <archivo>` en una terminal, o pasa --socket/--session al servidor MCP.",
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
        description = "Estado de la instancia viva de CodeDraw: archivo abierto, líneas, anotaciones, paso actual y tamaño de la terminal. Úsala primero para saber si hay una instancia."
    )]
    async fn status(&self, Parameters(_p): Parameters<Empty>) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(&self.call("status", json!({})).await?)))
    }

    #[tool(
        description = "Devuelve el código que se está mostrando, con números de línea. Léelo antes de anotar para elegir líneas y textos exactos."
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

    #[tool(description = "Lista las anotaciones existentes (id, paso, tipo y dónde están).")]
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
            "(sin anotaciones)".to_string()
        } else {
            lines.join("\n")
        }))
    }

    #[tool(
        description = "Dibuja una caja alrededor de un rango de líneas, con etiqueta opcional en el borde superior."
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
        description = "Dibuja una flecha de un punto del código a otro (p. ej. de `let s` a `takes(s)`), con etiqueta opcional. Usa `find` para anclar por texto."
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
        description = "Añade una nota de texto anclada a una línea (aparece en el margen derecho o debajo)."
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
        description = "Resalta un fragmento de texto (una variable, una expresión) con etiqueta opcional."
    )]
    async fn highlight(
        &self,
        Parameters(p): Parameters<HighlightParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self.call("annotation.add", json!({"type": "highlight", "at": p.at, "label": p.label, "step": p.step, "id": p.id, "color": p.color})).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Marca semántica de Rust sobre un fragmento: move, borrow (&), borrow_mut (&mut), drop o lifetime. Cada tipo tiene color y glifo fijos y muestra su leyenda."
    )]
    async fn add_mark(
        &self,
        Parameters(p): Parameters<MarkParams>,
    ) -> Result<CallToolResult, McpError> {
        let kind = mark_kind(&p.kind)?;
        let v = self
            .call("annotation.add", json!({"type": "mark", "kind": kind, "at": p.at, "label": p.label, "step": p.step, "id": p.id, "color": p.color}))
            .await?;
        Ok(text(pretty(&v)))
    }

    #[tool(description = "Cambia el paso o la etiqueta de una anotación existente.")]
    async fn update_annotation(
        &self,
        Parameters(p): Parameters<UpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self
            .call(
                "annotation.update",
                json!({"id": p.id, "step": p.step, "label": p.label, "color": p.color}),
            )
            .await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Vacía todas las anotaciones de golpe (o solo las de un paso con `step`). Útil entre tomas. El presentador puede deshacerlo con `u`."
    )]
    async fn clear_annotations(
        &self,
        Parameters(p): Parameters<ClearParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("scene.clear", json!({"step": p.step})).await?,
        )))
    }

    #[tool(description = "Elimina una anotación por id.")]
    async fn delete_annotation(
        &self,
        Parameters(p): Parameters<IdParams>,
    ) -> Result<CallToolResult, McpError> {
        let v = self.call("annotation.delete", json!({"id": p.id})).await?;
        Ok(text(pretty(&v)))
    }

    #[tool(
        description = "Cambia el paso actual de la presentación (se muestran las anotaciones con step <= paso)."
    )]
    async fn set_step(
        &self,
        Parameters(p): Parameters<StepParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("step.set", json!({"step": p.step})).await?,
        )))
    }

    #[tool(description = "Avanza un paso en la presentación.")]
    async fn next_step(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(&self.call("step.next", json!({})).await?)))
    }

    #[tool(description = "Retrocede un paso en la presentación.")]
    async fn prev_step(
        &self,
        Parameters(_p): Parameters<Empty>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(&self.call("step.prev", json!({})).await?)))
    }

    #[tool(
        description = "Enfoca un rango de líneas (el resto se atenúa). Sin parámetros, quita el enfoque."
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

    #[tool(description = "Cambia el tema: dark, light o toggle.")]
    async fn set_theme(
        &self,
        Parameters(p): Parameters<ThemeParams>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text(pretty(
            &self.call("theme.set", json!({"theme": p.theme})).await?,
        )))
    }

    #[tool(
        description = "Devuelve lo que se ve en pantalla como texto plano (código + anotaciones del paso actual). Úsala para comprobar el resultado de lo que dibujaste."
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
}

#[tool_handler]
impl ServerHandler for CodeDrawServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("codedraw", codedraw_core::VERSION))
            .with_instructions(
                "CodeDraw es una TUI que muestra código y permite dibujar encima (cajas, flechas, resaltes, notas y marcas \
                 de Rust: move, borrow, borrow_mut, drop, lifetime) para explicarlo en video. Flujo recomendado: \
                 1) `status` para confirmar que hay una instancia; 2) `get_code` para ver líneas y texto exactos; \
                 3) anota con `add_mark`, `add_arrow`, `add_box`, `add_note`, `highlight`, usando `find` con el texto exacto; \
                 4) agrupa en pasos con `step` y avanza con `next_step`; 5) `render_text` para verificar. \
                 Mantén las etiquetas cortas: se dibujan en una terminal de ~100 columnas.",
            )
    }
}

/// Arranca el servidor MCP por stdio (bloquea hasta que el cliente cierra).
pub async fn serve(socket: PathBuf) -> anyhow::Result<()> {
    let service = CodeDrawServer::new(socket).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
