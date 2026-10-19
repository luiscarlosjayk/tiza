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
    /// A bare text that matches nothing (`at.find`, a `find` an agent sent).
    #[error("text {0:?} not found in the code")]
    TextNotFound(String),
    /// A target named with its role, quoted the Tiza way (`find «x»`,
    /// `start marker «x»`), that matches nothing (t-167: never put inside
    /// another pair of quotes as if the phrase were the text searched).
    #[error("{0} matches nothing in the code")]
    NoMatch(String),
    /// A move's target that matches nothing at its step, in the words of
    /// `tiza check`: `move #2, step 3: find «x» matches nothing in the code
    /// at this step` (t-167). `index` is 1-based; `what` names the target.
    #[error("move #{index}, step {step}: {what} matches nothing in the code at this step")]
    MoveNoMatch {
        index: usize,
        step: u32,
        what: String,
    },
    #[error("{0}")]
    Invalid(String),
    /// `step.goto` / `tiza goto` with an id no named step has (t-099).
    #[error("{0}")]
    StepNotFound(String),
    #[error(".tiza file with schema {found}; this version understands up to {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// A JSON error whose message went through the redact filter (t-167):
    /// the same `invalid_json`, without the values serde quoted.
    #[error("{0}")]
    JsonText(String),
}

impl Error {
    /// Stable code for the API (socket/MCP): clients branch on it, not on the message.
    pub fn code(&self) -> &'static str {
        match self {
            Error::LineOutOfRange { .. } | Error::SpanOutOfRange { .. } => "out_of_range",
            Error::NotFound(_) | Error::StepNotFound(_) => "not_found",
            Error::DuplicateId(_) => "duplicate_id",
            Error::TextNotFound(_) | Error::NoMatch(_) | Error::MoveNoMatch { .. } => {
                "text_not_found"
            }
            Error::Invalid(_) => "invalid_argument",
            Error::UnsupportedSchema { .. } => "unsupported_schema",
            Error::Io(_) => "io",
            Error::Json(_) | Error::JsonText(_) => "invalid_json",
        }
    }
}
