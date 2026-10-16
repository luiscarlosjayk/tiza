use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("la línea {line} no existe (el documento va de {first} a {last})")]
    LineOutOfRange { line: u32, first: u32, last: u32 },
    #[error("el rango {start}..{end} no cabe en la línea {line} (tiene {len} grafemas)")]
    SpanOutOfRange {
        line: u32,
        start: u32,
        end: u32,
        len: u32,
    },
    #[error("no existe la anotación {0}")]
    NotFound(String),
    #[error("ya existe una anotación con id {0}")]
    DuplicateId(String),
    #[error("no se encontró el texto {0:?} en el código")]
    TextNotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("archivo .codedraw con schema {found}; esta versión entiende hasta {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Código estable para la API (socket/MCP): los clientes ramifican por él, no por el mensaje.
    pub fn code(&self) -> &'static str {
        match self {
            Error::LineOutOfRange { .. } | Error::SpanOutOfRange { .. } => "out_of_range",
            Error::NotFound(_) => "not_found",
            Error::DuplicateId(_) => "duplicate_id",
            Error::TextNotFound(_) => "text_not_found",
            Error::Invalid(_) => "invalid_argument",
            Error::UnsupportedSchema { .. } => "unsupported_schema",
            Error::Io(_) => "io",
            Error::Json(_) => "invalid_json",
        }
    }
}
