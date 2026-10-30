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
    /// `start marker «x»`), that matches nothing (never put inside
    /// another pair of quotes as if the phrase were the text searched).
    #[error("{0} matches nothing in the code")]
    NoMatch(String),
    /// A `find` whose hit starts or ends inside a character (a grapheme)
    /// where the column counts (`Document::find_span`): `edge` is `starts`
    /// or `ends`, `col` the grapheme of `character`.
    #[error(
        "text {text:?} {edge} inside the character {character:?} at line {line}, col {col}: put the whole character in the find, or use line and col"
    )]
    InsideCharacter {
        text: String,
        edge: &'static str,
        character: String,
        line: u32,
        col: u32,
    },
    /// A move's target that matches nothing at its step, in the words of
    /// `tiza check`: `move #2, step 3: find «x» matches nothing in the code
    /// at this step`. `index` is the number of the move in the messages
    /// (`Document::move_number`), from 1; `what` names the target.
    #[error("move #{index}, step {step}: {what} matches nothing in the code at this step")]
    MoveNoMatch {
        index: usize,
        step: u32,
        what: String,
    },
    #[error("{0}")]
    Invalid(String),
    /// `step.goto` / `tiza goto` with an id no named step has.
    #[error("{0}")]
    StepNotFound(String),
    #[error(
        ".tiza document with schema {found}: this version of Tiza reads schema {supported} at most"
    )]
    UnsupportedSchema { found: u64, supported: u32 },
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
    /// characters. Not a `source`: a chain printed with `{:#}`
    /// would repeat serde's message whole after the cut one.
    #[error("{}", shorten_quoted_value(&.0.to_string()))]
    Json(serde_json::Error),
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
            Error::Invalid(_) | Error::InsideCharacter { .. } => "invalid_argument",
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

/// What to write instead of the key that serde's message `msg` names
/// (`unknown field `colum`, expected one of `line`, `col`, …`): the key
/// of the list that it is a typo of, or for a position the name of the
/// same field there (`start` is `col`). `None` for another message, or
/// when no key of the list is near.
pub fn unknown_field_hint(msg: &str) -> Option<String> {
    let rest = msg.split("unknown field `").nth(1)?;
    let (key, rest) = rest.split_once('`')?;
    let expected: Vec<&str> = rest
        .split("expected")
        .nth(1)
        .unwrap_or("")
        .split('`')
        .skip(1)
        .step_by(2)
        .collect();
    crate::moves::position_key_hint(key, &expected)
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

/// The values serde quotes in its messages: how each opens and closes.
const QUOTED: [(&str, char); 5] = [
    ("unknown variant `", '`'),
    ("string \"", '"'),
    ("integer `", '`'),
    ("floating point `", '`'),
    ("character `", '`'),
];

/// Longest string an `invalid type` message quotes, in characters.
pub const QUOTED_MAX: usize = 40;

/// `s` cut to [`QUOTED_MAX`] characters and `…`: a value of the code that
/// an error quotes, such as a character with a thousand accents.
pub fn shorten(s: &str) -> String {
    match s.char_indices().nth(QUOTED_MAX) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// What may come before serde's own text in a message that reaches
/// [`serde_text`]: rmcp's wording for MCP arguments.
const SERDE_PREFIXES: [&str; 2] = ["", "failed to deserialize parameters: "];

/// serde's message for a value it could not read:
/// a closed value (a color, a type, an op…: `unknown variant`) is answered
/// with the valid names only, never with what was sent, and so is an
/// `invalid value`; an `invalid type` names what was sent (`invalid type:
/// string "three", expected u32`), cut by [`shorten_quoted_value`]. Which
/// one it is comes from how serde's text starts, never from what it quotes
/// (a variant sent as `invalid type: …` stays hidden). serde's list of what
/// it expected (after the last `, expected `) is Tiza's own names and stays
/// whole. A field name stays: `unknown field `x`` names it, as `missing
/// field` and `duplicate field` do.
pub fn serde_text(msg: &str) -> String {
    if invalid_type_at(msg).is_some() {
        return shorten_quoted_value(msg);
    }
    let (head, tail) = msg.split_at(msg.rfind(", expected ").unwrap_or(msg.len()));
    let mut head = head.to_string();
    for (open, close) in QUOTED {
        if let Some(k) = head.find(open) {
            let from = k + open.len() - 1;
            let to = head[from + 1..]
                .rfind(close)
                .map_or(head.len(), |e| from + 1 + e + 1);
            head.replace_range(from..to, ELIDED);
        }
    }
    head + tail
}

/// Where serde's `invalid type: ` starts in `msg`: at its start, or after
/// a known prefix ([`SERDE_PREFIXES`]); `None` for any other message.
fn invalid_type_at(msg: &str) -> Option<usize> {
    SERDE_PREFIXES
        .iter()
        .find(|p| {
            msg.strip_prefix(**p)
                .is_some_and(|r| r.starts_with("invalid type: "))
        })
        .map(|p| p.len())
}

/// An `invalid type` message with the string it quotes cut to
/// [`QUOTED_MAX`] characters and `…`: `invalid type: string
/// "xxxx…", expected u32`. serde writes a string as Rust's `{:?}` does, so
/// a newline or an escape is already `\n` or `\u{1b}`, never raw; the cut
/// never splits one. Numbers and other kinds are short and stay; any
/// other message is returned as it is.
pub fn shorten_quoted_value(msg: &str) -> String {
    const OPEN: &str = "invalid type: string \"";
    let Some(at) = invalid_type_at(msg) else {
        return msg.to_string();
    };
    let Some(value) = msg[at..].strip_prefix(OPEN) else {
        return msg.to_string();
    };
    let start = msg.len() - value.len();
    // The string's pieces up to its closing quote: one character, or one
    // whole escape (`\"`, `\n`, `\u{1b}`).
    let mut pieces: Vec<&str> = Vec::new();
    let mut rest = value;
    let closed = loop {
        let Some(c) = rest.chars().next() else {
            break false;
        };
        if c == '"' {
            break true;
        }
        let len = if c == '\\' {
            match rest[1..].chars().next() {
                Some('u') => rest.find('}').map_or(rest.len(), |e| e + 1),
                Some(n) => 1 + n.len_utf8(),
                None => 1,
            }
        } else {
            c.len_utf8()
        };
        pieces.push(&rest[..len]);
        rest = &rest[len..];
    };
    let shown: usize = pieces.iter().map(|p| p.chars().count()).sum();
    if !closed || shown <= QUOTED_MAX {
        return msg.to_string();
    }
    let mut cut = String::new();
    for p in pieces {
        if cut.chars().count() + p.chars().count() > QUOTED_MAX {
            break;
        }
        cut.push_str(p);
    }
    format!("{}{cut}…{rest}", &msg[..start])
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
            // `invalid type` names what was sent.
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
        // An escape is never cut in half: `\u{7}` goes whole or not at all.
        let esc = format!(
            "invalid type: string \"{}\\u{{7}}yyy\", expected u32",
            "x".repeat(38)
        );
        assert_eq!(
            shorten_quoted_value(&esc),
            format!("invalid type: string \"{}…\", expected u32", "x".repeat(38))
        );
        // A value that says `", expected ` or `unknown variant` itself.
        let tricky = format!(
            "invalid type: string \"{}\\\", expected unknown variant `x\", expected u32",
            "a".repeat(50)
        );
        assert_eq!(
            shorten_quoted_value(&tricky),
            format!("invalid type: string \"{}…\", expected u32", "a".repeat(40))
        );
        // No closing quote, multibyte text: as it is, never a panic.
        let open = "invalid type: string \"é";
        assert_eq!(shorten_quoted_value(open), open);
        // rmcp's prefix stays.
        let mcp = format!(
            "failed to deserialize parameters: invalid type: string \"{}\", expected u32",
            "z".repeat(45)
        );
        assert!(
            shorten_quoted_value(&mcp).ends_with(&format!("{}…\", expected u32", "z".repeat(40)))
        );
    }

    #[test]
    fn a_closed_value_that_says_invalid_type_stays_hidden() {
        for msg in [
            "unknown variant `invalid type: SECRET1`, expected one of `box`, `note`",
            "failed to deserialize parameters: unknown variant `invalid type: SECRET4`, expected `insert`",
            "invalid value: string \"invalid type: SECRET\", expected u8",
        ] {
            assert!(
                !serde_text(msg).contains("SECRET"),
                "{msg}: {}",
                serde_text(msg)
            );
        }
    }
}
