//! Protocolo de la API local (socket Unix, NDJSON), compartido por TUI, CLI y MCP.
//!
//! Diseño tomado de herdr: una petición por línea `{id, method, params}`,
//! respuesta `{id, result}` o `{id, error: {code, message}}`, métodos con
//! puntos. Los clientes ignoran campos desconocidos.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Anchor, Annotation, Document, MarkKind, Shape, TextSpan};

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

/// Dónde anclar algo: coordenadas explícitas o búsqueda de texto.
/// Un agente rara vez sabe la columna exacta; `find` es más robusto.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Where {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
    /// Texto a buscar en el código; con `line` restringe la búsqueda a esa línea primero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub find: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<u32>,
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
            .ok_or_else(|| crate::Error::Invalid("falta `line` o `find`".into()))?;
        let start = self.col.unwrap_or(0);
        Ok(TextSpan {
            line,
            start,
            end: doc.line_len(line),
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
            .ok_or_else(|| crate::Error::Invalid("falta `line` o `find`".into()))?;
        Ok(Anchor {
            line,
            col: self.col.unwrap_or(0),
        })
    }
}

/// Parámetros de `annotation.add`: una forma descrita de manera amigable para agentes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShapeSpec {
    Highlight {
        at: Where,
        #[serde(default)]
        label: Option<String>,
    },
    Mark {
        kind: MarkKind,
        at: Where,
        #[serde(default)]
        label: Option<String>,
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
}

impl ShapeSpec {
    pub fn resolve(&self, doc: &Document) -> Result<Shape, crate::Error> {
        Ok(match self {
            ShapeSpec::Highlight { at, label } => Shape::Highlight {
                span: at.resolve_span(doc)?,
                label: label.clone(),
            },
            ShapeSpec::Mark { kind, at, label } => Shape::Mark {
                kind: *kind,
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
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddParams {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub step: Option<u32>,
    /// Nombre de la paleta (orange, blue, green, purple, pink, cyan, yellow, red, gray) o `#rrggbb`.
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
    /// Nuevo color; cadena vacía vuelve al color automático.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    /// Si viene, reemplaza la forma completa.
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

/// Respuesta de `scene.get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub revision: u64,
    pub step: u32,
    pub max_step: u32,
    pub document: Document,
}

/// Respuesta de `status`.
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

/// Resultado de una mutación.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutated {
    pub id: String,
    pub revision: u64,
}

/// Métodos soportados, para `api.methods` y para la ayuda.
pub const METHODS: &[(&str, &str)] = &[
    ("ping", "Comprueba que la instancia responde"),
    (
        "status",
        "Resumen de la instancia: archivo, líneas, anotaciones, paso, tamaño de terminal",
    ),
    ("code.get", "Código con números de línea (texto)"),
    (
        "scene.get",
        "Documento completo con anotaciones y paso actual",
    ),
    (
        "annotation.add",
        "Añade una anotación (highlight, mark, box, arrow, note)",
    ),
    (
        "annotation.update",
        "Cambia paso, etiqueta o forma de una anotación",
    ),
    ("annotation.delete", "Elimina una anotación por id"),
    ("step.set", "Cambia el paso actual de la presentación"),
    ("step.next", "Avanza un paso"),
    ("step.prev", "Retrocede un paso"),
    ("render.text", "Devuelve el lienzo actual como texto plano"),
];

/// Representa una anotación en el formato de `scene.list`.
pub fn describe(a: &Annotation) -> String {
    match &a.shape {
        Shape::Highlight { span, label } => format!(
            "{} [paso {}] highlight L{} {}..{}{}",
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
        Shape::Mark { kind, span, label } => format!(
            "{} [paso {}] mark {} L{} {}..{}{}",
            a.id,
            a.step,
            kind.legend(),
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
            "{} [paso {}] box L{}-{}{}",
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
            "{} [paso {}] arrow L{}:{} → L{}:{}{}",
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
            format!("{} [paso {}] note L{} «{}»", a.id, a.step, line, text)
        }
    }
}
