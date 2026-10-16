//! Document model: source code + annotations anchored to the text.
//!
//! Anchors live in code coordinates (absolute 1-based line and 0-based
//! grapheme offset), never in screen cells. The conversion to cells happens
//! only when rendering (see `layout`).

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::error::Error;

/// Schema version of the `.codedraw` file.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source: Source,
    /// How the code is stored in the file: embedded (array of lines) or a
    /// reference to a file (`{"file": "src/main.rs", "lines": "12-48"}`).
    #[serde(rename = "code")]
    pub code_src: CodeSource,
    /// Code lines in memory, without the trailing newline. Filled in on load
    /// (see `fs::load_document`) and never serialized directly.
    #[serde(skip)]
    pub code: Vec<String>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// Current presentation step. 0 = code only, no annotations;
    /// annotations live at steps ≥ 1.
    #[serde(default = "one")]
    pub step: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeName>,
}

/// Where the code of a `.codedraw` comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CodeSource {
    /// Copy of the code inside the document: portable, but frozen.
    Embedded(Vec<String>),
    /// Reference to an existing file. `file` may be absolute or relative to the
    /// `.codedraw` directory; `lines` is an optional range `"12-48"` or `"7"`.
    File {
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lines: Option<String>,
    },
}

impl Default for CodeSource {
    fn default() -> Self {
        CodeSource::Embedded(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Source {
    /// Path of the original file, if there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Language identifier (extension or name): `rs`, `ts`, `py`…
    #[serde(default)]
    pub language: String,
    /// Number of the first displayed line (allows opening `main.rs:12-48`).
    #[serde(default = "one")]
    pub first_line: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    Dark,
    Light,
}

/// A point in the code: absolute line (1-based) and grapheme (0-based).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Anchor {
    pub line: u32,
    pub col: u32,
}

/// A range within ONE line, in graphemes, exclusive end.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TextSpan {
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

/// Semantic marks for explaining Rust (each with a fixed color, glyph and legend).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Move,
    Borrow,
    BorrowMut,
    Drop,
    Lifetime,
}

impl MarkKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "move" | "m" => Self::Move,
            "borrow" | "ref" | "&" | "b" => Self::Borrow,
            "mut" | "borrow_mut" | "borrowmut" | "&mut" => Self::BorrowMut,
            "drop" | "d" => Self::Drop,
            "lifetime" | "life" | "'a" | "l" => Self::Lifetime,
            _ => return None,
        })
    }

    /// Short legend shown next to the mark.
    pub fn legend(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Borrow => "borrow &",
            Self::BorrowMut => "borrow &mut",
            Self::Drop => "drop",
            Self::Lifetime => "lifetime",
        }
    }

    /// Glyph that accompanies the legend (the shape tells marks apart without relying on color).
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Move => "━▶",
            Self::Borrow => "╌▶",
            Self::BorrowMut => "═▶",
            Self::Drop => "╳",
            Self::Lifetime => "┆",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    /// Highlights a text range.
    Highlight {
        span: TextSpan,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Highlight with meaning (move, borrow…).
    Mark {
        kind: MarkKind,
        span: TextSpan,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Box around a range of lines.
    Box {
        from_line: u32,
        to_line: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Arrow from one point of the code to another.
    Arrow {
        from: Anchor,
        to: Anchor,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Text note anchored to a line.
    Note { line: u32, text: String },
    /// Directs attention: while active (from `step` to `until`, or only at
    /// `step`), dims the code outside `lines` and the annotations outside `ids`.
    /// With only `ids`, the focused code is that of those annotations.
    Spotlight {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_line: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_line: Option<u32>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ids: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        until: Option<u32>,
    },
}

impl Shape {
    /// Line ranges the shape occupies (to focus the code of an annotation).
    /// An arrow occupies only its two endpoints, not the stretch between them.
    pub fn line_ranges(&self) -> Vec<(u32, u32)> {
        match self {
            Shape::Highlight { span, .. } | Shape::Mark { span, .. } => {
                vec![(span.line, span.line)]
            }
            Shape::Box {
                from_line, to_line, ..
            } => vec![(*from_line, *to_line)],
            Shape::Arrow { from, to, .. } => vec![(from.line, from.line), (to.line, to.line)],
            Shape::Note { line, .. } => vec![(*line, *line)],
            Shape::Spotlight {
                from_line, to_line, ..
            } => from_line
                .map(|f| vec![(f, to_line.unwrap_or(f))])
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Annotation {
    /// Short, stable public id (`a1`, `a2`…), chosen by the client or assigned.
    pub id: String,
    /// Step at which it appears (1-based).
    #[serde(default = "one")]
    pub step: u32,
    /// Color chosen by the presenter: a palette name (`blue`, `verde`…) or `#rrggbb`.
    /// Without a color, arrows rotate through a palette and the rest use the theme color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(flatten)]
    pub shape: Shape,
}

fn one() -> u32 {
    1
}

impl Document {
    pub fn from_code(code: &str, language: impl Into<String>, path: Option<String>) -> Self {
        let lines: Vec<String> = code.lines().map(|l| l.to_string()).collect();
        Self {
            schema: SCHEMA_VERSION,
            title: None,
            source: Source {
                path,
                language: language.into(),
                first_line: 1,
            },
            code_src: CodeSource::Embedded(lines.clone()),
            code: lines,
            annotations: Vec::new(),
            step: 1,
            theme: None,
        }
    }

    /// Switches the source to a file reference (the code in memory does not change).
    pub fn reference_file(&mut self, file: impl Into<String>, lines: Option<(u32, u32)>) {
        self.code_src = CodeSource::File {
            file: file.into(),
            lines: lines.map(|(a, b)| {
                if a == b {
                    a.to_string()
                } else {
                    format!("{a}-{b}")
                }
            }),
        };
    }

    /// Freezes the code inside the document (no longer depends on the file).
    pub fn embed_code(&mut self) {
        self.code_src = CodeSource::Embedded(self.code.clone());
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self.code_src, CodeSource::Embedded(_))
    }

    /// Keeps `code_src` consistent with `code` before serializing.
    pub fn sync_code_src(&mut self) {
        if let CodeSource::Embedded(lines) = &mut self.code_src {
            *lines = self.code.clone();
        }
    }

    /// Last absolute line of the document.
    pub fn last_line(&self) -> u32 {
        self.source.first_line + self.code.len().saturating_sub(1) as u32
    }

    pub fn has_line(&self, line: u32) -> bool {
        line >= self.source.first_line && line <= self.last_line()
    }

    /// Text of an absolute line.
    pub fn line_text(&self, line: u32) -> Option<&str> {
        if !self.has_line(line) {
            return None;
        }
        self.code
            .get((line - self.source.first_line) as usize)
            .map(|s| s.as_str())
    }

    /// Number of graphemes on a line.
    pub fn line_len(&self, line: u32) -> u32 {
        self.line_text(line)
            .map(|t| t.graphemes(true).count() as u32)
            .unwrap_or(0)
    }

    /// Highest step used by any annotation (at least 1).
    pub fn max_step(&self) -> u32 {
        self.annotations
            .iter()
            .map(|a| a.step)
            .max()
            .unwrap_or(1)
            .max(1)
    }

    pub fn find_annotation(&self, id: &str) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.id == id)
    }

    /// Next free id with the `a` prefix (a1, a2, …).
    pub fn next_id(&self) -> String {
        let mut n = self.annotations.len() as u32 + 1;
        loop {
            let id = format!("a{n}");
            if self.find_annotation(&id).is_none() {
                return id;
            }
            n += 1;
        }
    }

    /// Validates that a shape points to existing lines/columns.
    pub fn validate_shape(&self, shape: &Shape) -> Result<(), Error> {
        let check_line = |l: u32| {
            if self.has_line(l) {
                Ok(())
            } else {
                Err(Error::LineOutOfRange {
                    line: l,
                    first: self.source.first_line,
                    last: self.last_line(),
                })
            }
        };
        let check_span = |s: &TextSpan| {
            check_line(s.line)?;
            let len = self.line_len(s.line);
            if s.start >= s.end || s.end > len {
                return Err(Error::SpanOutOfRange {
                    line: s.line,
                    start: s.start,
                    end: s.end,
                    len,
                });
            }
            Ok(())
        };
        let check_anchor = |a: &Anchor| {
            check_line(a.line)?;
            let len = self.line_len(a.line);
            if a.col > len {
                return Err(Error::SpanOutOfRange {
                    line: a.line,
                    start: a.col,
                    end: a.col,
                    len,
                });
            }
            Ok(())
        };
        match shape {
            Shape::Highlight { span, .. } | Shape::Mark { span, .. } => check_span(span),
            Shape::Box {
                from_line, to_line, ..
            } => {
                check_line(*from_line)?;
                check_line(*to_line)?;
                if from_line > to_line {
                    return Err(Error::Invalid(format!(
                        "the box starts at {from_line} and ends earlier, at {to_line}"
                    )));
                }
                Ok(())
            }
            Shape::Arrow { from, to, .. } => {
                check_anchor(from)?;
                check_anchor(to)
            }
            Shape::Note { line, text } => {
                check_line(*line)?;
                if text.trim().is_empty() {
                    return Err(Error::Invalid("the note is empty".into()));
                }
                Ok(())
            }
            Shape::Spotlight {
                from_line,
                to_line,
                ids,
                ..
            } => {
                if from_line.is_none() && ids.is_empty() {
                    return Err(Error::Invalid(
                        "a spotlight needs a line range, annotation ids, or both".into(),
                    ));
                }
                if let Some(a) = from_line {
                    check_line(*a)?;
                    let b = to_line.unwrap_or(*a);
                    check_line(b)?;
                    if *a > b {
                        return Err(Error::Invalid(format!(
                            "the spotlight starts at {a} and ends earlier, at {b}"
                        )));
                    }
                }
                for id in ids {
                    if self.find_annotation(id).is_none() {
                        return Err(Error::NotFound(id.clone()));
                    }
                }
                Ok(())
            }
        }
    }

    /// Adds an annotation. If `id` is empty a new one is assigned. Returns the id.
    pub fn add(&mut self, mut ann: Annotation) -> Result<String, Error> {
        self.validate_shape(&ann.shape)?;
        if ann.id.trim().is_empty() {
            ann.id = self.next_id();
        } else if self.find_annotation(&ann.id).is_some() {
            return Err(Error::DuplicateId(ann.id));
        }
        if ann.step == 0 {
            ann.step = 1;
        }
        let id = ann.id.clone();
        self.annotations.push(ann);
        Ok(id)
    }

    pub fn update(&mut self, ann: Annotation) -> Result<(), Error> {
        self.validate_shape(&ann.shape)?;
        let slot = self
            .annotations
            .iter_mut()
            .find(|a| a.id == ann.id)
            .ok_or_else(|| Error::NotFound(ann.id.clone()))?;
        *slot = ann;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<Annotation, Error> {
        let pos = self
            .annotations
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        Ok(self.annotations.remove(pos))
    }

    /// Searches `text` in the code and returns the span of the `occurrence`-th
    /// match (1-based). If `near_line` is given, that line is searched first.
    pub fn find_text(
        &self,
        text: &str,
        occurrence: u32,
        near_line: Option<u32>,
    ) -> Option<TextSpan> {
        if text.is_empty() {
            return None;
        }
        let occurrence = occurrence.max(1);
        let mut seen = 0;
        let search_line = |line: u32, seen: &mut u32| -> Option<TextSpan> {
            let t = self.line_text(line)?;
            let mut from = 0;
            while let Some(pos) = t[from..].find(text) {
                let byte_start = from + pos;
                *seen += 1;
                if *seen == occurrence {
                    let start = grapheme_index(t, byte_start);
                    let end = grapheme_index(t, byte_start + text.len());
                    return Some(TextSpan { line, start, end });
                }
                from = byte_start + text.len().max(1);
            }
            None
        };
        if let Some(l) = near_line {
            if let Some(s) = search_line(l, &mut seen) {
                return Some(s);
            }
            seen = 0;
        }
        for line in self.source.first_line..=self.last_line() {
            if let Some(s) = search_line(line, &mut seen) {
                return Some(s);
            }
        }
        None
    }
}

/// Converts a byte offset into `s` to the corresponding grapheme index.
pub fn grapheme_index(s: &str, byte_offset: usize) -> u32 {
    s.grapheme_indices(true)
        .take_while(|(b, _)| *b < byte_offset)
        .count() as u32
}

/// Returns the slice of `s` between graphemes `start..end`.
pub fn grapheme_slice(s: &str, start: u32, end: u32) -> &str {
    let mut it = s.grapheme_indices(true);
    let b0 = it
        .by_ref()
        .nth(start as usize)
        .map(|(b, _)| b)
        .unwrap_or(s.len());
    let b1 = if end > start {
        s.grapheme_indices(true)
            .nth(end as usize)
            .map(|(b, _)| b)
            .unwrap_or(s.len())
    } else {
        b0
    };
    &s[b0..b1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        Document::from_code(
            "fn main() {\n    let s = String::from(\"hola\");\n    takes(s);\n}\n",
            "rs",
            None,
        )
    }

    #[test]
    fn find_text_returns_grapheme_span() {
        let d = doc();
        let s = d.find_text("takes(s)", 1, None).unwrap();
        assert_eq!(
            s,
            TextSpan {
                line: 3,
                start: 4,
                end: 12
            }
        );
        assert_eq!(
            grapheme_slice(d.line_text(3).unwrap(), s.start, s.end),
            "takes(s)"
        );
    }

    #[test]
    fn find_text_second_occurrence() {
        let d = doc();
        // On line 2, "o" appears in `from` (1) and in `hola` (2).
        let s = d.find_text("o", 2, Some(2)).unwrap();
        assert_eq!(s.line, 2);
        assert_eq!(s.start, 27);
        // If the requested line has too few occurrences, the whole document is searched.
        let s = d.find_text("s", 2, Some(2)).unwrap();
        assert_eq!(s.line, 3);
    }

    #[test]
    fn add_validates_and_assigns_ids() {
        let mut d = doc();
        let id = d
            .add(Annotation {
                id: String::new(),
                step: 1,
                color: None,
                shape: Shape::Note {
                    line: 2,
                    text: "s is born here".into(),
                },
            })
            .unwrap();
        assert_eq!(id, "a1");
        let err = d
            .add(Annotation {
                id: String::new(),
                step: 1,
                color: None,
                shape: Shape::Note {
                    line: 99,
                    text: "x".into(),
                },
            })
            .unwrap_err();
        assert!(matches!(err, Error::LineOutOfRange { line: 99, .. }));
    }

    #[test]
    fn roundtrip_json() {
        let mut d = doc();
        d.add(Annotation {
            id: "a1".into(),
            step: 2,
            color: None,
            shape: Shape::Arrow {
                from: Anchor { line: 2, col: 8 },
                to: Anchor { line: 3, col: 10 },
                label: Some("move".into()),
            },
        })
        .unwrap();
        let json = serde_json::to_string_pretty(&d).unwrap();
        let mut back: Document = serde_json::from_str(&json).unwrap();
        if let CodeSource::Embedded(lines) = &back.code_src {
            back.code = lines.clone();
        }
        assert_eq!(d, back);
        assert!(json.contains("\"type\": \"arrow\""));
    }

    #[test]
    fn unicode_graphemes() {
        let d = Document::from_code("let año = \"ñandú\"; // 🦀 crab\n", "rs", None);
        let s = d.find_text("🦀", 1, None).unwrap();
        assert_eq!(
            grapheme_slice(d.line_text(1).unwrap(), s.start, s.end),
            "🦀"
        );
        assert_eq!(s.end - s.start, 1);
    }
}
