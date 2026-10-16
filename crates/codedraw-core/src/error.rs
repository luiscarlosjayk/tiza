use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("line {line} does not exist (the document spans {first} to {last})")]
    LineOutOfRange { line: u32, first: u32, last: u32 },
    #[error("range {start}..{end} does not fit on line {line} (it has {len} graphemes)")]
    SpanOutOfRange {
        line: u32,
        start: u32,
        end: u32,
        len: u32,
    },
    #[error("annotation {0} does not exist")]
    NotFound(String),
    #[error("an annotation with id {0} already exists")]
    DuplicateId(String),
    #[error("text {0:?} not found in the code")]
    TextNotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error(".codedraw file with schema {found}; this version understands up to {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Stable code for the API (socket/MCP): clients branch on it, not on the message.
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
