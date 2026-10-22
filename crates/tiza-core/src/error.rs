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
    /// A file that is not a view of the document (`file «x» is not a view
    /// …`): `not_found`.
    #[error("{0}")]
    ViewNotFound(String),
    /// A move that does not exist (`move.delete`): `move #N does not exist`.
    #[error("move {0} does not exist")]
    MoveNotFound(String),
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
    /// Another error, said of one thing (`annotation a2, step 1: range
    /// 9..9 does not fit…`); the API code is that of the error inside.
    #[error("{what}: {inner}")]
    Within { what: String, inner: Box<Error> },
    /// A message of its own about another error (quoted in it); the API
    /// code is that of the error inside.
    #[error("{message}")]
    Because { message: String, inner: Box<Error> },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// serde's message, an `invalid type` value cut to [`QUOTED_MAX`]
    /// characters (t-189).
    #[error("{}", shorten_quoted_value(&.0.to_string()))]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Stable code for the API (socket/MCP): clients branch on it, not on the message.
    pub fn code(&self) -> &'static str {
        match self {
            Error::LineOutOfRange { .. } | Error::SpanOutOfRange { .. } => "out_of_range",
            Error::NotFound(_)
            | Error::ViewNotFound(_)
            | Error::StepNotFound(_)
            | Error::MoveNotFound(_) => "not_found",
            Error::DuplicateId(_) => "duplicate_id",
            Error::TextNotFound(_) | Error::NoMatch(_) | Error::MoveNoMatch { .. } => {
                "text_not_found"
            }
            Error::Invalid(_) => "invalid_argument",
            Error::UnsupportedSchema { .. } => "unsupported_schema",
            Error::Io(_) => "io",
            Error::Json(_) => "invalid_json",
            Error::Within { inner, .. } | Error::Because { inner, .. } => inner.code(),
        }
    }
}

/// What is left of a value or a target an error does not repeat.
pub const ELIDED: &str = "«…»";

/// How a socket error for parameters that do not deserialize starts: the
/// rest is serde's message, through [`serde_text`].
pub const INVALID_PARAMETERS: &str = "invalid parameters: ";

/// The values serde quotes in its messages: how each opens and closes.
const QUOTED: [(&str, char); 5] = [
    ("unknown variant `", '`'),
    ("string \"", '"'),
    ("integer `", '`'),
    ("floating point `", '`'),
    ("character `", '`'),
];

/// Longest value an `invalid type` message quotes, in characters (t-189).
pub const QUOTED_MAX: usize = 40;

/// Where the value serde quotes in `head` is: the range from its opening
/// delimiter to its closing one, both included.
fn quoted_range(head: &str, open: &str, close: char) -> Option<std::ops::Range<usize>> {
    let k = head.find(open)?;
    let from = k + open.len() - 1;
    let to = head[from + 1..]
        .rfind(close)
        .map_or(head.len(), |e| from + 1 + e + 1);
    Some(from..to)
}

/// serde's message for a value it could not read (t-167 R1, R2; t-189):
/// a closed value (a color, a type, an op…: `unknown variant`) is answered
/// with the valid names only, never with what was sent, and so is an
/// `invalid value`; an `invalid type` names what was sent (`invalid type:
/// string "three", expected u32`), cut by [`shorten_quoted_value`]. serde's
/// list of what it expected (after the last `, expected `) is Tiza's own
/// names and stays whole. A field name stays: `unknown field `x`` names
/// it, as `missing field` and `duplicate field` do.
pub fn serde_text(msg: &str) -> String {
    if msg.contains("invalid type: ") {
        return shorten_quoted_value(msg);
    }
    let (head, tail) = msg.split_at(msg.rfind(", expected ").unwrap_or(msg.len()));
    let mut head = head.to_string();
    for (open, close) in QUOTED {
        if let Some(r) = quoted_range(&head, open, close) {
            head.replace_range(r, ELIDED);
        }
    }
    head + tail
}

/// An `invalid type` message with the value it quotes cut to
/// [`QUOTED_MAX`] characters and `…` (t-189): `invalid type: string
/// "xxxx…", expected u32`. serde writes a string as Rust's `{:?}` does, so
/// a newline or an escape is already `\n` or `\u{1b}`, never raw. What comes
/// before `invalid type: ` (a caller's prefix, as rmcp's `failed to
/// deserialize parameters: `) stays; any other message is returned as it
/// is.
pub fn shorten_quoted_value(msg: &str) -> String {
    let Some(at) = msg.find("invalid type: ") else {
        return msg.to_string();
    };
    let (prefix, msg) = msg.split_at(at);
    let (head, tail) = msg.split_at(msg.rfind(", expected ").unwrap_or(msg.len()));
    let mut head = head.to_string();
    for (open, close) in QUOTED {
        if let Some(r) = quoted_range(&head, open, close) {
            let inner = &head[r.start + 1..r.end.saturating_sub(1).max(r.start + 1)];
            if inner.chars().count() > QUOTED_MAX {
                let cut: String = inner.chars().take(QUOTED_MAX).collect();
                head.replace_range(r.start + 1..r.end - 1, &format!("{cut}…"));
            }
            break;
        }
    }
    format!("{prefix}{head}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_text_keeps_the_list_and_the_field_names_and_drops_the_value() {
        for (msg, want) in [
            (
                "unknown variant `sk_live_x`, expected one of `box`, `note`",
                "unknown variant «…», expected one of `box`, `note`",
            ),
            (
                "unknown variant `a`, expected `b` at line 3 column 9",
                "unknown variant «…», expected `b` at line 3 column 9",
            ),
            // A value that says `, expected ` itself: serde's list is the last.
            (
                "unknown variant `x, expected `y`, expected `box`",
                "unknown variant «…», expected `box`",
            ),
            // `invalid type` names what was sent (t-189).
            (
                "invalid type: string \"Zq7\\\"v\", expected u32",
                "invalid type: string \"Zq7\\\"v\", expected u32",
            ),
            (
                "invalid type: integer `48213`, expected a string",
                "invalid type: integer `48213`, expected a string",
            ),
            (
                "invalid value: floating point `1.5`, expected u8",
                "invalid value: floating point «…», expected u8",
            ),
            (
                "unknown field `colour`, expected one of `color`, `step`",
                "unknown field `colour`, expected one of `color`, `step`",
            ),
            ("missing field `span`", "missing field `span`"),
            (
                "unknown variant `x`, there are no variants",
                "unknown variant «…», there are no variants",
            ),
        ] {
            assert_eq!(serde_text(msg), want, "{msg}");
        }
    }

    #[test]
    fn shorten_quoted_value_cuts_a_long_value_and_nothing_else() {
        let long = format!(
            "invalid type: string \"{}\\n\", expected u32",
            "x".repeat(60)
        );
        assert_eq!(
            shorten_quoted_value(&long),
            format!("invalid type: string \"{}…\", expected u32", "x".repeat(40))
        );
        let short = "invalid type: string \"three\", expected u32 at line 2 column 7";
        assert_eq!(shorten_quoted_value(short), short);
        let variant =
            "unknown variant `xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx`, expected `a`";
        assert_eq!(shorten_quoted_value(variant), variant);
    }
}
