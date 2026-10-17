//! Moves (ADR 0007): the code changes between steps. A `code.patch` move is
//! applied when the presenter reaches `at_step` and reverted when going back;
//! the document is a pure function of the step, recomputed from the step-0
//! code. Annotations are authored against the code at their own step and
//! shifted through later patches by the patch's own line map — never by a
//! text search (t-098).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::Error;
use crate::model::{Anchor, Annotation, Shape, TextSpan};
use crate::protocol::MoveSpec;

/// One move of the demo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Move {
    /// Step at which it runs (going forward) and is reverted (going back). `>= 1`.
    pub at_step: u32,
    #[serde(flatten)]
    pub action: MoveAction,
    /// Where annotations the patch replaces or deletes go afterwards, by id:
    /// `{"find": "…"}`, `{"line": N, "col": C}`, `{"from_line": A, "to_line": B}`…
    /// (the same fields as `annotation.update`'s `move`), resolved against the
    /// code after the patch.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rebind: BTreeMap<String, MoveSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action")]
pub enum MoveAction {
    /// Insert, replace or delete whole lines.
    #[serde(rename = "code.patch")]
    CodePatch(Patch),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Patch {
    pub op: PatchOp,
    /// `insert`: the line the content goes before (`last + 1` appends).
    /// `replace`/`delete`: the line range (`"7"` or `"7-9"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    /// `insert`: text of the line the content goes after / before (exactly one line matches).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// `replace`/`delete`: text of the one line to change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub find: Option<String>,
    /// `replace`/`delete`: the range from the line of `find_start` to the line of `find_end`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub find_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub find_end: Option<String>,
    /// New lines (`insert`, `replace`); `\n` separates lines, a trailing `\n` is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Typing::is_instant")]
    pub typing: Typing,
    /// Milliseconds per line (`typing: line`, default 120) or per character (`char`, default 25).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PatchOp {
    Insert,
    Replace,
    Delete,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Typing {
    #[default]
    Instant,
    Line,
    Char,
}

impl Typing {
    pub fn is_instant(&self) -> bool {
        *self == Typing::Instant
    }
}

impl Patch {
    pub fn content_lines(&self) -> Vec<String> {
        match &self.content {
            None => Vec::new(),
            Some(c) => c
                .strip_suffix('\n')
                .unwrap_or(c)
                .split('\n')
                .map(|l| l.to_string())
                .collect(),
        }
    }

    /// Default typing speed in milliseconds per unit.
    pub fn speed(&self) -> u64 {
        self.speed_ms.unwrap_or(match self.typing {
            Typing::Instant => 0,
            Typing::Line => 120,
            Typing::Char => 25,
        })
    }
}

/// What a patch did to the line numbers: `old line -> new line`, `None` for
/// a line the patch removed. Lines are absolute (`first_line`-based).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMap {
    pub first_line: u32,
    /// Index = old line - first_line.
    pub map: Vec<Option<u32>>,
    /// Lines the patch inserted (absolute, in the new code).
    pub inserted: std::ops::Range<u32>,
    pub old_len: u32,
    pub new_len: u32,
}

impl LineMap {
    pub fn get(&self, old: u32) -> Option<u32> {
        self.map
            .get(old.checked_sub(self.first_line)? as usize)
            .copied()
            .flatten()
    }
}

/// Where a patch applies, resolved on the code it is applied to:
/// `(first line, last line)` of the affected range (for an insert, the
/// insertion point as `(at, at - 1)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub from: u32,
    pub to: u32,
}

fn find_unique(code: &[String], first_line: u32, text: &str, what: &str) -> Result<u32, Error> {
    if text.is_empty() {
        return Err(Error::Invalid(format!("{what}: empty text")));
    }
    let hits: Vec<u32> = code
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains(text))
        .map(|(i, _)| first_line + i as u32)
        .collect();
    match hits.as_slice() {
        [one] => Ok(*one),
        [] => Err(Error::TextNotFound(format!("{what} «{text}»"))),
        many => Err(Error::Invalid(format!(
            "{what} «{text}» matches {} lines ({}); a move target must match exactly once",
            many.len(),
            many.iter()
                .take(5)
                .map(|l| format!("L{l}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn parse_position(s: &str) -> Result<(u32, u32), Error> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(['-', ':']).map(str::trim).collect();
    let nums: Result<Vec<u32>, _> = parts.iter().map(|p| p.parse::<u32>()).collect();
    match nums
        .map_err(|_| Error::Invalid(format!("position «{s}»: use a line or A-B")))?
        .as_slice()
    {
        [a] => Ok((*a, *a)),
        [a, b] if a <= b => Ok((*a, *b)),
        _ => Err(Error::Invalid(format!("position «{s}»: use a line or A-B"))),
    }
}

impl Patch {
    /// Resolves the target on `code` (the code at this move's step, before
    /// the patch). Every text target must match exactly one line.
    pub fn resolve(&self, code: &[String], first_line: u32) -> Result<Target, Error> {
        let last = first_line + code.len() as u32 - 1;
        let in_range = |l: u32, what: &str| {
            if l < first_line || l > last {
                Err(Error::LineOutOfRange {
                    line: l,
                    first: first_line,
                    last,
                })
            } else {
                let _ = what;
                Ok(l)
            }
        };
        match self.op {
            PatchOp::Insert => {
                let given = [
                    self.position.is_some(),
                    self.after.is_some(),
                    self.before.is_some(),
                ]
                .iter()
                .filter(|b| **b)
                .count();
                if given != 1 {
                    return Err(Error::Invalid(
                        "insert: give exactly one of position, after or before".into(),
                    ));
                }
                if self.find.is_some() || self.find_start.is_some() || self.find_end.is_some() {
                    return Err(Error::Invalid(
                        "insert: find/find_start/find_end are for replace and delete; use position, after or before".into(),
                    ));
                }
                if self.content_lines().is_empty() {
                    return Err(Error::Invalid("insert: content is required".into()));
                }
                let at = if let Some(p) = &self.position {
                    let (a, b) = parse_position(p)?;
                    if a != b {
                        return Err(Error::Invalid(format!(
                            "insert: position «{p}» must be one line (the content goes before it)"
                        )));
                    }
                    if a < first_line || a > last + 1 {
                        return Err(Error::LineOutOfRange {
                            line: a,
                            first: first_line,
                            last: last + 1,
                        });
                    }
                    a
                } else if let Some(t) = &self.after {
                    find_unique(code, first_line, t, "after")? + 1
                } else {
                    find_unique(
                        code,
                        first_line,
                        self.before.as_deref().unwrap_or(""),
                        "before",
                    )?
                };
                Ok(Target {
                    from: at,
                    to: at.wrapping_sub(1),
                })
            }
            PatchOp::Replace | PatchOp::Delete => {
                if self.after.is_some() || self.before.is_some() {
                    return Err(Error::Invalid(format!(
                        "{}: after/before are for insert; use position, find or find_start/find_end",
                        op_name(self.op)
                    )));
                }
                let given = [
                    self.position.is_some(),
                    self.find.is_some(),
                    self.find_start.is_some() || self.find_end.is_some(),
                ]
                .iter()
                .filter(|b| **b)
                .count();
                if given != 1 {
                    return Err(Error::Invalid(format!(
                        "{}: give exactly one of position, find or find_start/find_end",
                        op_name(self.op)
                    )));
                }
                if self.op == PatchOp::Replace && self.content_lines().is_empty() {
                    return Err(Error::Invalid(
                        "replace: content is required (use delete to remove lines)".into(),
                    ));
                }
                if self.op == PatchOp::Delete && self.content.is_some() {
                    return Err(Error::Invalid("delete: content is not allowed".into()));
                }
                let (from, to) = if let Some(p) = &self.position {
                    let (a, b) = parse_position(p)?;
                    (in_range(a, "position")?, in_range(b, "position")?)
                } else if let Some(t) = &self.find {
                    let l = find_unique(code, first_line, t, "find")?;
                    (l, l)
                } else {
                    let (Some(s), Some(e)) = (&self.find_start, &self.find_end) else {
                        return Err(Error::Invalid("find_start and find_end go together".into()));
                    };
                    let a = find_unique(code, first_line, s, "find_start")?;
                    let b = find_unique(code, first_line, e, "find_end")?;
                    if b < a {
                        return Err(Error::Invalid(format!(
                            "find_end «{e}» (L{b}) is before find_start «{s}» (L{a})"
                        )));
                    }
                    (a, b)
                };
                Ok(Target { from, to })
            }
        }
    }

    /// Applies the patch to `code`, returning the new code and the line map.
    pub fn apply(&self, code: &[String], first_line: u32) -> Result<(Vec<String>, LineMap), Error> {
        let t = self.resolve(code, first_line)?;
        let old_len = code.len() as u32;
        let content = self.content_lines();
        let mut out: Vec<String> = Vec::with_capacity(code.len() + content.len());
        let mut map: Vec<Option<u32>> = Vec::with_capacity(code.len());
        let inserted;
        match self.op {
            PatchOp::Insert => {
                let at = (t.from - first_line) as usize;
                out.extend_from_slice(&code[..at]);
                for i in 0..at {
                    map.push(Some(first_line + i as u32));
                }
                inserted = t.from..t.from + content.len() as u32;
                out.extend(content.iter().cloned());
                for i in at..code.len() {
                    map.push(Some(first_line + (i + inserted.len()) as u32));
                }
                out.extend_from_slice(&code[at..]);
            }
            PatchOp::Replace | PatchOp::Delete => {
                let a = (t.from - first_line) as usize;
                let b = (t.to - first_line) as usize; // inclusive
                out.extend_from_slice(&code[..a]);
                for i in 0..a {
                    map.push(Some(first_line + i as u32));
                }
                let new_lines = if self.op == PatchOp::Replace {
                    content.clone()
                } else {
                    Vec::new()
                };
                inserted = t.from..t.from + new_lines.len() as u32;
                out.extend(new_lines.iter().cloned());
                // A replaced line maps to the same offset in the new text when it exists.
                for k in 0..=(b - a) {
                    map.push(if k < new_lines.len() {
                        Some(t.from + k as u32)
                    } else {
                        None
                    });
                }
                let delta = new_lines.len() as i64 - (b - a + 1) as i64;
                for i in (b + 1)..code.len() {
                    map.push(Some((first_line as i64 + i as i64 + delta) as u32));
                }
                out.extend_from_slice(&code[b + 1..]);
            }
        }
        if out.is_empty() {
            out.push(String::new());
        }
        Ok((
            out.clone(),
            LineMap {
                first_line,
                map,
                inserted,
                old_len,
                new_len: out.len() as u32,
            },
        ))
    }
}

fn op_name(op: PatchOp) -> &'static str {
    match op {
        PatchOp::Insert => "insert",
        PatchOp::Replace => "replace",
        PatchOp::Delete => "delete",
    }
}

/// Moves of one step, in list order.
pub fn moves_at(moves: &[Move], step: u32) -> impl Iterator<Item = (usize, &Move)> {
    moves
        .iter()
        .enumerate()
        .filter(move |(_, m)| m.at_step == step)
}

/// One applied move: its index in `Document::moves`, its step and its line map.
pub type AppliedMove = (usize, u32, LineMap);

/// The code at `step`: the step-0 code with every move `<= step` applied, in
/// step order then list order. Returns the code and the line map of every
/// applied move (with its index and step), for shifting annotations.
pub fn apply_until(
    base: &[String],
    first_line: u32,
    moves: &[Move],
    step: u32,
) -> Result<(Vec<String>, Vec<AppliedMove>), Error> {
    let mut code = base.to_vec();
    let mut maps = Vec::new();
    let mut order: Vec<(usize, &Move)> = moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.at_step <= step)
        .collect();
    order.sort_by_key(|(i, m)| (m.at_step, *i));
    for (i, m) in order {
        let MoveAction::CodePatch(p) = &m.action;
        let (new_code, map) = p.apply(&code, first_line).map_err(|e| match e {
            Error::TextNotFound(t) => {
                Error::TextNotFound(format!("{t} (move #{}, step {})", i + 1, m.at_step))
            }
            Error::LineOutOfRange { .. } => e,
            other => Error::Invalid(format!("move #{} (step {}): {other}", i + 1, m.at_step)),
        })?;
        code = new_code;
        maps.push((i, m.at_step, map));
    }
    Ok((code, maps))
}

/// Validates every move against the code at its step: targets resolve to
/// exactly one place, `at_step >= 1`, `rebind` ids exist, content present.
pub fn validate(
    base: &[String],
    first_line: u32,
    moves: &[Move],
    annotations: &[Annotation],
) -> Result<(), Error> {
    for (i, m) in moves.iter().enumerate() {
        if m.at_step == 0 {
            return Err(Error::Invalid(format!(
                "move #{}: at_step must be >= 1 (step 0 is the untouched code)",
                i + 1
            )));
        }
        for id in m.rebind.keys() {
            if !annotations.iter().any(|a| &a.id == id) {
                return Err(Error::Invalid(format!(
                    "move #{} (step {}): rebind names unknown annotation `{id}`",
                    i + 1,
                    m.at_step
                )));
            }
        }
    }
    let last = moves.iter().map(|m| m.at_step).max().unwrap_or(0);
    apply_until(base, first_line, moves, last).map(|_| ())
}

/// Shifts a shape through one patch's line map. `None` = unanchored.
pub fn shift_shape(shape: &Shape, map: &LineMap) -> Option<Shape> {
    let line = |l: u32| map.get(l);
    let span = |s: &TextSpan| {
        line(s.line).map(|l| TextSpan {
            line: l,
            start: s.start,
            end: s.end,
        })
    };
    let anchor = |a: &Anchor| {
        line(a.line).map(|l| Anchor {
            line: l,
            col: a.col,
        })
    };
    // A range keeps its surviving lines: the first mapped line from the top,
    // the last mapped line from the bottom.
    let range = |from: u32, to: u32| -> Option<(u32, u32)> {
        let a = (from..=to).find_map(line)?;
        let b = (from..=to).rev().find_map(line)?;
        Some((a, b))
    };
    Some(match shape {
        Shape::Highlight {
            span: s,
            label,
            emphasis,
        } => Shape::Highlight {
            span: span(s)?,
            label: label.clone(),
            emphasis: *emphasis,
        },
        Shape::Mark {
            kind,
            span: s,
            label,
            style,
            emphasis,
        } => Shape::Mark {
            kind: kind.clone(),
            span: span(s)?,
            label: label.clone(),
            style: style.clone(),
            emphasis: *emphasis,
        },
        Shape::Box {
            from_line,
            to_line,
            label,
            frame,
        } => {
            let (a, b) = range(*from_line, *to_line)?;
            Shape::Box {
                from_line: a,
                to_line: b,
                label: label.clone(),
                frame: *frame,
            }
        }
        Shape::Arrow {
            from,
            to,
            label,
            line: ls,
        } => Shape::Arrow {
            from: anchor(from)?,
            to: anchor(to)?,
            label: label.clone(),
            line: *ls,
        },
        Shape::Note { line: l, text } => Shape::Note {
            line: line(*l)?,
            text: text.clone(),
        },
        Shape::Callout {
            from_line,
            to_line,
            text,
            title,
            placement,
            width,
            frame,
        } => {
            let (a, b) = range(*from_line, to_line.unwrap_or(*from_line))?;
            Shape::Callout {
                from_line: a,
                to_line: to_line.map(|_| b),
                text: text.clone(),
                title: title.clone(),
                placement: *placement,
                width: *width,
                frame: *frame,
            }
        }
        Shape::Spotlight {
            from_line,
            to_line,
            ids,
            until,
        } => {
            let (a, b) = match from_line {
                Some(f) => {
                    let (a, b) = range(*f, to_line.unwrap_or(*f))?;
                    (Some(a), Some(b))
                }
                None => (None, None),
            };
            Shape::Spotlight {
                from_line: a,
                to_line: b,
                ids: ids.clone(),
                until: *until,
            }
        }
    })
}

/// Partial content of a typing animation: `revealed` units of the patch's
/// content (lines for `Typing::Line`, graphemes for `Typing::Char`), the
/// rest blank, so the line count — and every shifted annotation — stays put.
pub fn partial_content(lines: &[String], typing: Typing, revealed: usize) -> Vec<String> {
    match typing {
        Typing::Instant => lines.to_vec(),
        Typing::Line => lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                if i < revealed {
                    l.clone()
                } else {
                    String::new()
                }
            })
            .collect(),
        Typing::Char => {
            let mut left = revealed;
            lines
                .iter()
                .map(|l| {
                    let n = l.graphemes(true).count();
                    let take = left.min(n);
                    left = left.saturating_sub(n + 1); // +1: the newline counts as a unit
                    l.graphemes(true).take(take).collect()
                })
                .collect()
        }
    }
}

/// Number of typing units in `lines` for `typing`.
pub fn typing_units(lines: &[String], typing: Typing) -> usize {
    match typing {
        Typing::Instant => 0,
        Typing::Line => lines.len(),
        Typing::Char => lines
            .iter()
            .map(|l| l.graphemes(true).count() + 1)
            .sum::<usize>()
            .saturating_sub(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(s: &str) -> Vec<String> {
        s.lines().map(str::to_string).collect()
    }

    fn patch(op: PatchOp) -> Patch {
        Patch {
            op,
            position: None,
            after: None,
            before: None,
            find: None,
            find_start: None,
            find_end: None,
            content: None,
            typing: Typing::Instant,
            speed_ms: None,
        }
    }

    #[test]
    fn insert_shifts_lines_after_the_edit() {
        let c = code("a\nb\nc");
        let p = Patch {
            position: Some("2".into()),
            content: Some("x\ny\n".into()),
            ..patch(PatchOp::Insert)
        };
        let (out, map) = p.apply(&c, 1).unwrap();
        assert_eq!(out, code("a\nx\ny\nb\nc"));
        assert_eq!(map.get(1), Some(1));
        assert_eq!(map.get(2), Some(4));
        assert_eq!(map.get(3), Some(5));
        assert_eq!(map.inserted, 2..4);
        // after / before by text, and append at last + 1.
        let p = Patch {
            after: Some("b".into()),
            content: Some("z".into()),
            ..patch(PatchOp::Insert)
        };
        assert_eq!(p.apply(&c, 1).unwrap().0, code("a\nb\nz\nc"));
        let p = Patch {
            position: Some("4".into()),
            content: Some("z".into()),
            ..patch(PatchOp::Insert)
        };
        assert_eq!(p.apply(&c, 1).unwrap().0, code("a\nb\nc\nz"));
    }

    #[test]
    fn replace_maps_same_offset_and_delete_unmaps() {
        let c = code("a\nb\nc\nd");
        let p = Patch {
            position: Some("2-3".into()),
            content: Some("B".into()),
            ..patch(PatchOp::Replace)
        };
        let (out, map) = p.apply(&c, 1).unwrap();
        assert_eq!(out, code("a\nB\nd"));
        assert_eq!(map.get(2), Some(2), "first replaced line keeps its offset");
        assert_eq!(map.get(3), None, "no second line in the new text");
        assert_eq!(map.get(4), Some(3));
        let p = Patch {
            find: Some("c".into()),
            ..patch(PatchOp::Delete)
        };
        let (out, map) = p.apply(&c, 1).unwrap();
        assert_eq!(out, code("a\nb\nd"));
        assert_eq!(map.get(3), None);
        assert_eq!(map.get(4), Some(3));
    }

    #[test]
    fn text_targets_must_be_unique() {
        let c = code("x\ny\nx");
        let p = Patch {
            find: Some("x".into()),
            ..patch(PatchOp::Delete)
        };
        let e = p.resolve(&c, 1).unwrap_err().to_string();
        assert!(e.contains("matches 2 lines") && e.contains("L1, L3"), "{e}");
        let p = Patch {
            find: Some("q".into()),
            ..patch(PatchOp::Delete)
        };
        assert!(matches!(p.resolve(&c, 1), Err(Error::TextNotFound(_))));
    }

    #[test]
    fn shapes_shift_shrink_or_unanchor() {
        let c = code("a\nb\nc\nd\ne");
        let p = Patch {
            position: Some("3".into()),
            ..patch(PatchOp::Delete)
        };
        let (_, map) = p.apply(&c, 1).unwrap();
        let bx = Shape::Box {
            from_line: 2,
            to_line: 3,
            label: None,
            frame: Default::default(),
        };
        match shift_shape(&bx, &map).unwrap() {
            Shape::Box {
                from_line, to_line, ..
            } => assert_eq!((from_line, to_line), (2, 2)),
            _ => panic!(),
        }
        let note = Shape::Note {
            line: 3,
            text: "x".into(),
        };
        assert!(shift_shape(&note, &map).is_none(), "its line is gone");
        let note5 = Shape::Note {
            line: 5,
            text: "x".into(),
        };
        assert!(matches!(
            shift_shape(&note5, &map),
            Some(Shape::Note { line: 4, .. })
        ));
    }

    #[test]
    fn partial_typing_keeps_the_line_count() {
        let lines = code("ab\ncd");
        assert_eq!(partial_content(&lines, Typing::Line, 1), vec!["ab", ""]);
        assert_eq!(partial_content(&lines, Typing::Char, 3), vec!["ab", ""]);
        assert_eq!(partial_content(&lines, Typing::Char, 4), vec!["ab", "c"]);
        assert_eq!(typing_units(&lines, Typing::Char), 5);
        assert_eq!(typing_units(&lines, Typing::Line), 2);
    }
}
