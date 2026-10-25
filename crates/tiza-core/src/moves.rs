//! Moves (ADR 0007): the code changes between steps. A move is applied when
//! the presenter reaches `at_step` and reverted when going back; the document
//! is a pure function of the step, recomputed from the step-0 code.
//! `code.patch` edits lines; `code.link` switches the shown file
//! (a *view*, with an optional window of lines); `cursor`, `select` and
//! `wait` set the cursor, a selection and a pause. Annotations are
//! authored against the code at their own step, belong to the view shown at
//! that step, and are shifted through that view's later patches by the
//! patch's own line map — never by a text search.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::Error;
use crate::model::{Anchor, Annotation, Document, Shape, TextSpan};
use crate::protocol::{MoveSpec, Where};

/// One move of the demo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Move {
    /// Step at which it runs (going forward) and is reverted (going back). `>= 1`.
    pub at_step: u32,
    #[serde(flatten)]
    pub action: MoveAction,
    /// Where annotations the patch replaces or deletes go afterwards, by id,
    /// with the same fields as `annotation.update`'s `move`: `{"at": {"find":
    /// "…", "occurrence": N}}` or `{"at": {"line": N, "col": C}}` for
    /// highlights, marks and notes; `{"from": {…}, "to": {…}}` for arrows;
    /// `{"from_line": A, "to_line": B}` for boxes, callouts and spotlights.
    /// Resolved against the code after the patch. A spec with none of those
    /// keys (or an unknown one) is a load-time error. An entry whose
    /// annotation is not in the document (deleted, or dropped by a paste or
    /// a reload) is kept as written and ignored, and `check` names it;
    /// `move.add` and `scene.put` refuse one the caller writes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rebind: BTreeMap<String, MoveSpec>,
    /// Set in a live instance when the code changed under the move by
    /// something the demo did not write (a follow reload, a paste) and the
    /// move no longer matches it: the move is skipped until a later change
    /// makes it match again. Never saved nor sent (see [`crate::stale`]).
    #[serde(skip)]
    pub stale: Option<crate::stale::Stale>,
    /// Annotations hidden because they stood on text this move writes and
    /// it went stale; live only, like `stale`.
    #[serde(skip)]
    pub held: Vec<crate::stale::Hold>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action")]
pub enum MoveAction {
    /// Insert, replace or delete whole lines of the file shown at the step.
    #[serde(rename = "code.patch")]
    CodePatch(Patch),
    /// Show another file (or the main file again), optionally cropped to a
    /// window of lines. Each file is a view with its own annotations.
    #[serde(rename = "code.link")]
    CodeLink(Link),
    /// Put the cursor on a line (and column) of the shown file.
    #[serde(rename = "cursor")]
    Cursor(CursorAt),
    /// Select a span of the shown file, from one point to another.
    #[serde(rename = "select")]
    Select(SelectSpec),
    /// Pause for `ms` before the step's annotations appear (TUI and
    /// recordings; a still render ignores it).
    #[serde(rename = "wait")]
    Wait(Wait),
}

impl MoveAction {
    /// The JSON `action` name.
    pub fn name(&self) -> &'static str {
        match self {
            MoveAction::CodePatch(_) => "code.patch",
            MoveAction::CodeLink(_) => "code.link",
            MoveAction::Cursor(_) => "cursor",
            MoveAction::Select(_) => "select",
            MoveAction::Wait(_) => "wait",
        }
    }
}

/// `code.link`: `file` is the path as `code.file` would name it (relative to
/// the `.tiza` directory); `lines` is only a window (`"7-30"` or `"7"`) that
/// crops what is shown — annotations keep real file line numbers and the
/// same file with another window is the same view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Link {
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<String>,
}

impl Link {
    /// The window as `(first, last)`, or `None` for the whole file.
    pub fn window(&self) -> Result<Option<(u32, u32)>, Error> {
        match &self.lines {
            None => Ok(None),
            Some(s) => {
                let (a, b) = parse_position(s).map_err(|_| {
                    Error::Invalid(
                        "code.link lines: use a line or A-B (a window of the file)".into(),
                    )
                })?;
                if a == 0 {
                    return Err(Error::Invalid(format!(
                        "code.link lines «{s}»: line ranges start at 1"
                    )));
                }
                Ok(Some((a, b)))
            }
        }
    }

    /// `file:A-B` as the status line shows it.
    pub fn label(&self) -> String {
        match &self.lines {
            Some(l) => format!("{}:{}", self.file, l.trim()),
            None => self.file.clone(),
        }
    }
}

/// `cursor`: a line (real file line number) and an optional grapheme column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CursorAt {
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
}

/// `select`: two points of the shown file, each a `find` (exactly one match
/// at that step, or with `occurrence`) or a `line`/`col`. With `find`, the
/// selection starts at the start of `from`'s text and ends at the end of
/// `to`'s text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SelectSpec {
    pub from: Where,
    pub to: Where,
}

/// `wait`: milliseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Wait {
    pub ms: u64,
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
    /// Which match of `find` / `after` / `before` / `find_start` to use when
    /// the text appears more than once: 1-based, counted in reading order
    /// over the whole code as it is at this step. Without it,
    /// more than one match is an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<u32>,
    /// The same for `find_end`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_occurrence: Option<u32>,
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

    /// How long the typing animation of this patch takes, in milliseconds
    /// (units × speed; 0 for an instant patch).
    pub fn typing_ms(&self) -> u64 {
        if self.typing == Typing::Instant {
            return 0;
        }
        (typing_units(&self.content_lines(), self.typing) as u64)
            .saturating_mul(self.speed().max(1))
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
    /// Old lines a `replace` overwrote: they map to a line (same offset in
    /// the new text) but their text is gone, so column anchors on them are
    /// unanchored (ADR 0007 §3a).
    pub replaced: std::ops::Range<u32>,
    pub old_len: u32,
    pub new_len: u32,
}

impl LineMap {
    /// New line of a column anchor on `old`: `None` on a replaced line too.
    pub fn get_column_anchor(&self, old: u32) -> Option<u32> {
        if self.replaced.contains(&old) {
            return None;
        }
        self.get(old)
    }

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

/// Lines of every match of `text`, one entry per match in reading order (a
/// line holding the text twice appears twice).
pub fn matches_of(code: &[String], first_line: u32, text: &str) -> Vec<u32> {
    if text.is_empty() {
        return Vec::new();
    }
    code.iter()
        .enumerate()
        .flat_map(|(i, l)| std::iter::repeat_n(first_line + i as u32, l.matches(text).count()))
        .collect()
}

/// The line of the one match of `text`, or of its `occurrence`-th match
/// (1-based, counted in reading order over the whole code) when given.
fn find_unique(
    code: &[String],
    first_line: u32,
    text: &str,
    what: &str,
    occurrence: Option<u32>,
) -> Result<u32, Error> {
    if text.is_empty() {
        return Err(Error::Invalid(format!("{what}: empty text")));
    }
    let hits = matches_of(code, first_line, text);
    if hits.is_empty() {
        return Err(Error::NoMatch(format!("{what} «{text}»")));
    }
    match occurrence {
        Some(0) => Err(Error::Invalid(format!(
            "{what} «{text}»: occurrence is 1-based"
        ))),
        Some(n) => hits.get(n as usize - 1).copied().ok_or_else(|| {
            Error::Invalid(format!(
                "{what} «{text}»: occurrence {n} asked, only {} match{} at this step",
                hits.len(),
                if hits.len() == 1 { "" } else { "es" }
            ))
        }),
        None => match hits.as_slice() {
            [one] => Ok(*one),
            many => Err(Error::Invalid(format!(
                "{what} «{text}» matches {} times ({}); a move target must match exactly once (add occurrence: N)",
                many.len(),
                many.iter()
                    .take(5)
                    .map(|l| format!("L{l}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        },
    }
}

fn parse_position(s: &str) -> Result<(u32, u32), Error> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(['-', ':']).map(str::trim).collect();
    let nums: Result<Vec<u32>, _> = parts.iter().map(|p| p.parse::<u32>()).collect();
    match nums
        .map_err(|_| Error::Invalid("position: use a line or A-B".into()))?
        .as_slice()
    {
        [a] => Ok((*a, *a)),
        [a, b] if a <= b => Ok((*a, *b)),
        // Never the text received: a malformed value is not
        // repeated.
        _ => Err(Error::Invalid("position: use a line or A-B".into())),
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
                    find_unique(code, first_line, t, "after", self.occurrence)? + 1
                } else {
                    find_unique(
                        code,
                        first_line,
                        self.before.as_deref().unwrap_or(""),
                        "before",
                        self.occurrence,
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
                    let l = find_unique(code, first_line, t, "find", self.occurrence)?;
                    (l, l)
                } else {
                    let (Some(s), Some(e)) = (&self.find_start, &self.find_end) else {
                        return Err(Error::Invalid("find_start and find_end go together".into()));
                    };
                    let a = find_unique(code, first_line, s, "find_start", self.occurrence)?;
                    let b = find_unique(code, first_line, e, "find_end", self.end_occurrence)?;
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
        Ok(self.apply_target(code, first_line, t))
    }

    /// Applies the patch at a target already resolved to `code`.
    pub fn apply_target(
        &self,
        code: &[String],
        first_line: u32,
        t: Target,
    ) -> (Vec<String>, LineMap) {
        let old_len = code.len() as u32;
        let content = self.content_lines();
        let mut out: Vec<String> = Vec::with_capacity(code.len() + content.len());
        let mut map: Vec<Option<u32>> = Vec::with_capacity(code.len());
        let inserted;
        let mut replaced = 0..0;
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
                if self.op == PatchOp::Replace {
                    replaced = t.from..t.to + 1;
                }
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
        (
            out.clone(),
            LineMap {
                first_line,
                map,
                inserted,
                replaced,
                old_len,
                new_len: out.len() as u32,
            },
        )
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

/// A view of the document: `None` is the main file (the document's own
/// `code`), `Some(file)` a file a `code.link` named, as written there.
pub type ViewKey = Option<String>;

/// What is shown: the view and the window of its lines (`None`: all).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ViewRef {
    pub file: ViewKey,
    pub window: Option<(u32, u32)>,
}

impl ViewRef {
    /// `queue.rs:10-30`, `queue.rs`, or `None` for the main file.
    pub fn label(&self) -> Option<String> {
        let f = self.file.as_ref()?;
        Some(match self.window {
            Some((a, b)) if a == b => format!("{f}:{a}"),
            Some((a, b)) => format!("{f}:{a}-{b}"),
            None => f.clone(),
        })
    }
}

/// One applied patch: its index in the moves list, its step, the view it
/// edited, its line map and the view's code right after it.
#[derive(Debug, Clone)]
pub struct AppliedPatch {
    pub index: usize,
    pub at_step: u32,
    pub file: ViewKey,
    pub map: LineMap,
    pub after: Vec<String>,
}

/// The document's state at a step: every view's code with its patches
/// `<= step` applied, what is shown, and the cursor/selection the moves
/// left (shifted through later patches of the same view).
#[derive(Debug, Clone, Default)]
pub struct Projection {
    pub shown: ViewRef,
    /// Code of every view that was shown or edited (always the main one).
    pub codes: BTreeMap<ViewKey, Vec<String>>,
    pub patches: Vec<AppliedPatch>,
    pub cursor: Option<Anchor>,
    pub selection: Option<(Anchor, Anchor)>,
}

impl Projection {
    /// The shown code cropped to the window, and its first line.
    pub fn shown_code(&self, doc: &Document) -> (Vec<String>, u32) {
        let code = self
            .codes
            .get(&self.shown.file)
            .cloned()
            .unwrap_or_default();
        let first = doc.view_first_line(&self.shown.file);
        crop(code, first, self.shown.window)
    }
}

/// `code[first..]` cropped to `window`, clamped to the lines that exist.
pub fn crop(code: Vec<String>, first: u32, window: Option<(u32, u32)>) -> (Vec<String>, u32) {
    let Some((a, b)) = window else {
        return (code, first);
    };
    let last = first + code.len().saturating_sub(1) as u32;
    let a = a.clamp(first, last.max(first));
    let b = b.clamp(a, last.max(first));
    let out = code
        .into_iter()
        .skip((a - first) as usize)
        .take((b - a + 1) as usize)
        .collect();
    (out, a)
}

/// Resolves a point of a `select` on `code`: a `find` must match exactly
/// once (on its `line` when given, else in the whole code) or carry
/// `occurrence`. `end` selects the end of the found text (for `to`).
fn resolve_point(
    doc: &Document,
    code: &[String],
    first_line: u32,
    w: &Where,
    what: &str,
    end: bool,
) -> Result<Anchor, Error> {
    let Some(text) = &w.find else {
        let line = w
            .line
            .ok_or_else(|| Error::Invalid(format!("{what}: give `find` or `line`")))?;
        let last = first_line + code.len() as u32 - 1;
        if line < first_line || line > last {
            return Err(Error::LineOutOfRange {
                line,
                first: first_line,
                last,
            });
        }
        let len = code[(line - first_line) as usize].graphemes(true).count() as u32;
        let col = w.col.unwrap_or(if end { len } else { 0 });
        if col > len {
            return Err(Error::SpanOutOfRange {
                line,
                start: col,
                end: col,
                len,
            });
        }
        return Ok(Anchor { line, col });
    };
    let scope: Vec<String> = match w.line {
        Some(l) => {
            let last = first_line + code.len() as u32 - 1;
            if l < first_line || l > last {
                return Err(Error::LineOutOfRange {
                    line: l,
                    first: first_line,
                    last,
                });
            }
            vec![code[(l - first_line) as usize].clone()]
        }
        None => code.to_vec(),
    };
    let scope_first = w.line.unwrap_or(first_line);
    find_unique(&scope, scope_first, text, what, w.occurrence)?;
    let mut d = doc.clone();
    d.moves.clear();
    d.base_code.clear();
    d.annotations.clear();
    d.code = code.to_vec();
    d.source.first_line = first_line;
    let span = d
        .find_text(text, w.occurrence.unwrap_or(1), w.line)
        .ok_or_else(|| Error::NoMatch(format!("{what} «{text}»")))?;
    Ok(Anchor {
        line: span.line,
        col: if end { span.end } else { span.start },
    })
}

/// The moves `<= step` in the order they run: step order, then list order.
pub(crate) fn replay_order(moves: &[Move], step: u32) -> Vec<(usize, &Move)> {
    let mut order: Vec<(usize, &Move)> = moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.at_step <= step)
        .collect();
    order.sort_by_key(|(i, m)| (m.at_step, *i));
    order
}

/// The document at `step`: the moves `<= step` applied in step order then
/// list order, each on the view shown at its point. A stale move is
/// skipped. An error names the move that failed.
pub fn project(doc: &Document, moves: &[Move], step: u32) -> Result<Projection, Error> {
    let mut p = Projection::start(doc);
    for (i, m) in replay_order(moves, step) {
        if m.stale.is_some() {
            continue;
        }
        p.apply_move(doc, i, m)
            .map_err(|e| name_move(e, i, m.at_step))?;
    }
    Ok(p)
}

impl Projection {
    /// Step 0: the main file's code, nothing shown but it.
    pub(crate) fn start(doc: &Document) -> Projection {
        let mut p = Projection::default();
        p.codes
            .insert(None, doc.view_base_code(&None).unwrap_or_default());
        p
    }

    /// Runs move `m` (number `i` of the list) on the view shown now. On an
    /// error nothing changes and the error does not name the move yet.
    pub(crate) fn apply_move(&mut self, doc: &Document, i: usize, m: &Move) -> Result<(), Error> {
        let p = self;
        match &m.action {
            MoveAction::CodePatch(patch) => {
                let key = p.shown.file.clone();
                let first = doc.view_first_line(&key);
                let code = p.codes.get(&key).cloned().unwrap_or_default();
                let target = patch.resolve(&code, first)?;
                let (new_code, map) = patch.apply_target(&code, first, target);
                if let Some(c) = p.cursor {
                    p.cursor = map.get(c.line).map(|line| Anchor { line, col: c.col });
                }
                if let Some((a, b)) = p.selection {
                    p.selection =
                        match (map.get_column_anchor(a.line), map.get_column_anchor(b.line)) {
                            (Some(la), Some(lb)) => Some((
                                Anchor {
                                    line: la,
                                    col: a.col,
                                },
                                Anchor {
                                    line: lb,
                                    col: b.col,
                                },
                            )),
                            _ => None,
                        };
                }
                let after = new_code.clone();
                p.codes.insert(key.clone(), new_code);
                p.patches.push(AppliedPatch {
                    index: i,
                    at_step: m.at_step,
                    file: key,
                    map,
                    after,
                });
            }
            MoveAction::CodeLink(link) => {
                let key = doc.view_key(&link.file);
                let window = link.window()?;
                if !p.codes.contains_key(&key) {
                    let base = doc.view_base_code(&key).ok_or_else(|| {
                        Error::Invalid(format!(
                            "code.link «{}»: the file is not loaded (it could not be read next to the document; `:embed` stores it)",
                            link.file
                        ))
                    })?;
                    p.codes.insert(key.clone(), base);
                }
                p.shown = ViewRef { file: key, window };
                p.cursor = None;
                p.selection = None;
            }
            MoveAction::Cursor(c) => {
                let key = p.shown.file.clone();
                let first = doc.view_first_line(&key);
                let code = p.codes.get(&key).map(Vec::as_slice).unwrap_or_default();
                let last = first + code.len().saturating_sub(1) as u32;
                if c.line < first || c.line > last {
                    return Err(Error::LineOutOfRange {
                        line: c.line,
                        first,
                        last,
                    });
                }
                let len = code[(c.line - first) as usize].graphemes(true).count() as u32;
                let col = c.col.unwrap_or(0);
                if col > len {
                    return Err(Error::SpanOutOfRange {
                        line: c.line,
                        start: col,
                        end: col,
                        len,
                    });
                }
                p.cursor = Some(Anchor { line: c.line, col });
            }
            MoveAction::Select(s) => {
                let key = p.shown.file.clone();
                let first = doc.view_first_line(&key);
                let code = p.codes.get(&key).map(Vec::as_slice).unwrap_or_default();
                let a = resolve_point(doc, code, first, &s.from, "select from", false)?;
                let b = resolve_point(doc, code, first, &s.to, "select to", true)?;
                let (a, b) = if (b.line, b.col) < (a.line, a.col) {
                    (b, a)
                } else {
                    (a, b)
                };
                p.selection = Some((a, b));
            }
            MoveAction::Wait(w) => {
                if w.ms == 0 {
                    return Err(Error::Invalid("wait: ms must be >= 1".into()));
                }
            }
        }
        Ok(())
    }
}

/// Where `line` of the code shown at step `from` is at step `to` (so
/// the live pointer stays on its line across `n`/`p`): carried forward
/// through every patch of the shown view between the two steps, or back
/// through them in reverse. `None` when a patch removed the line, when the
/// two steps show different files, or when the moves do not apply. The
/// same line without moves.
pub fn carry_line(doc: &Document, from: u32, to: u32, line: u32) -> Option<u32> {
    if doc.moves.is_empty() || from == to {
        return Some(line);
    }
    let (lo, hi) = (from.min(to), from.max(to));
    let low = project(doc, &doc.moves, lo).ok()?;
    let high = project(doc, &doc.moves, hi).ok()?;
    if low.shown.file != high.shown.file {
        return None;
    }
    let key = &high.shown.file;
    let maps: Vec<&LineMap> = high
        .patches
        .iter()
        .filter(|p| &p.file == key && p.at_step > lo && p.at_step <= hi)
        .map(|p| &p.map)
        .collect();
    let mut cur = line;
    if to > from {
        for m in maps {
            cur = m.get(cur)?;
        }
    } else {
        for m in maps.iter().rev() {
            let old = m.map.iter().position(|n| *n == Some(cur))?;
            cur = m.first_line + old as u32;
        }
    }
    Some(cur)
}

/// Validates every move against the code at its step: targets resolve to
/// exactly one place, `at_step >= 1`, `rebind` only on patches, linked
/// files are loaded, cursors and selections exist. A `rebind` entry whose
/// annotation is not in `annotations` is skipped ([`ignored_rebinds`]).
/// The numbers a move may hold: a `wait` and a `code.patch`'s
/// typing speed within [`crate::model::MAX_HOLD_MS`], so no export of the
/// take can hang on its holds or overflow adding them up.
pub fn check_move_bounds(m: &Move, what: &str) -> Result<(), Error> {
    match &m.action {
        MoveAction::Wait(w) => crate::model::check_hold_ms(w.ms, &format!("{what}, wait")),
        MoveAction::CodePatch(p) => match p.speed_ms {
            Some(ms) => crate::model::check_hold_ms(ms, &format!("{what}, speed_ms")),
            None => Ok(()),
        },
        _ => Ok(()),
    }
}

pub fn validate(doc: &Document, moves: &[Move], annotations: &[Annotation]) -> Result<(), Error> {
    for (i, m) in moves.iter().enumerate() {
        if m.at_step == 0 {
            return Err(Error::Invalid(format!(
                "move #{}: at_step must be >= 1 (step 0 is the untouched code)",
                i + 1
            )));
        }
        if !m.rebind.is_empty() && !matches!(m.action, MoveAction::CodePatch(_)) {
            return Err(Error::Invalid(format!(
                "move #{}, step {}: rebind only applies to code.patch, not {}",
                i + 1,
                m.at_step,
                m.action.name()
            )));
        }
        for (id, spec) in &m.rebind {
            if !annotations.iter().any(|a| &a.id == id) {
                continue;
            }
            if spec.is_empty() {
                return Err(Error::Invalid(format!(
                    "move #{}, step {}: rebind «{id}» says nowhere to go: use `at` {{find | line, col}}, `from`/`to` (arrows) or `from_line`/`to_line` (boxes, callouts, spotlights)",
                    i + 1,
                    m.at_step
                )));
            }
        }
    }
    // Apply in order, checking each patch's `rebind` finds on the code after
    // it: exactly one match, or an `occurrence` within the matches.
    let p = project(doc, moves, u32::MAX)?;
    for ap in &p.patches {
        let m = &moves[ap.index];
        if let Some(why) = rebind_problem(m, annotations, &ap.after, doc.view_first_line(&ap.file))
        {
            return Err(Error::Invalid(format!(
                "move #{}, step {}: {why}",
                ap.index + 1,
                m.at_step
            )));
        }
    }
    Ok(())
}

/// What is wrong with the `find`s of a patch's `rebind` on `after`, the
/// code right after the patch (`rebind «a3» «x» matches 2 times…`), if
/// anything: each must match exactly once or carry an `occurrence` within
/// the matches. Entries for annotations not in `annotations` are ignored.
pub(crate) fn rebind_problem(
    m: &Move,
    annotations: &[Annotation],
    after: &[String],
    first: u32,
) -> Option<String> {
    for (id, spec) in &m.rebind {
        if !annotations.iter().any(|a| &a.id == id) {
            continue;
        }
        for w in [&spec.at, &spec.from, &spec.to].into_iter().flatten() {
            let (Some(text), None) = (&w.find, w.line) else {
                continue;
            };
            let n = matches_of(after, first, text).len();
            let bad = match w.occurrence {
                None if n == 1 => None,
                None => Some(format!(
                    "«{text}» matches {n} time{} after the patch; it must match exactly once (add occurrence: N)",
                    if n == 1 { "" } else { "s" }
                )),
                Some(0) => Some(format!("«{text}»: occurrence is 1-based")),
                Some(k) if (k as usize) > n => Some(format!(
                    "«{text}»: occurrence {k} asked, only {n} match{} after the patch",
                    if n == 1 { "" } else { "es" }
                )),
                Some(_) => None,
            };
            if let Some(why) = bad {
                return Some(format!("rebind «{id}» {why}"));
            }
        }
    }
    None
}

/// A `rebind` entry naming an annotation the document does not have: it is
/// kept as written and ignored when the move applies, so the entry works
/// again if that annotation comes back (an undo, or one added with that
/// id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredRebind {
    /// 0-based, as in `doc.moves`.
    pub index: usize,
    pub at_step: u32,
    pub id: String,
}

impl IgnoredRebind {
    /// Where it is, as `check` places a move: `move #3, step 2`.
    pub fn place(&self) -> String {
        format!("move #{}, step {}", self.index + 1, self.at_step)
    }

    /// What `check` and an export's stderr say about it.
    pub fn message(&self) -> String {
        format!(
            "rebind names {}, which is not in the document; the entry is ignored",
            self.id
        )
    }
}

/// Every `rebind` entry of `doc` whose annotation is not in the document,
/// in move order.
pub fn ignored_rebinds(doc: &Document) -> Vec<IgnoredRebind> {
    doc.moves
        .iter()
        .enumerate()
        .flat_map(|(i, m)| {
            m.rebind
                .keys()
                .filter(|id| doc.find_annotation(id).is_none())
                .map(move |id| IgnoredRebind {
                    index: i,
                    at_step: m.at_step,
                    id: id.clone(),
                })
        })
        .collect()
}

/// The entries [`ignored_rebinds`] finds in `after` that were not ignored
/// in `before` (the same moves before a change to the annotations), said on
/// the status line as `move #3's rebind of a4 is now ignored`. `None` when
/// there are none.
pub fn newly_ignored(before: &Document, after: &Document) -> Option<String> {
    let was = ignored_rebinds(before);
    let now: Vec<String> = ignored_rebinds(after)
        .into_iter()
        .filter(|r| !was.contains(r))
        .map(|r| format!("move #{}'s rebind of {}", r.index + 1, r.id))
        .collect();
    (!now.is_empty()).then(|| {
        format!(
            "{} {} now ignored",
            now.join(", "),
            if now.len() == 1 { "is" } else { "are" }
        )
    })
}

/// Refuses `ann` when an ignored `rebind` entry names its id and would not
/// work with it back: an entry that says nowhere to go, or a `find` that
/// does not match exactly once after its patch. Taking the id would make
/// the document fail to load; any other id is fine.
pub fn check_named_by_rebind(doc: &Document, ann: &Annotation) -> Result<(), Error> {
    let id = &ann.id;
    let refused = |i: usize, why: String| {
        Error::Invalid(format!(
            "annotation {id}: move #{} (step {}) has a rebind for {id} that would not work with it back ({why}); use another id",
            i + 1,
            doc.moves[i].at_step
        ))
    };
    for (i, m) in doc.moves.iter().enumerate() {
        if m.rebind.get(id).is_some_and(|spec| spec.is_empty()) {
            return Err(refused(i, "it says nowhere to go".into()));
        }
    }
    let Ok(p) = project(doc, &doc.moves, u32::MAX) else {
        return Ok(());
    };
    let mut annotations = doc.annotations.clone();
    annotations.push(ann.clone());
    for ap in &p.patches {
        let m = &doc.moves[ap.index];
        let Some(spec) = m.rebind.get(id) else {
            continue;
        };
        let only = Move {
            rebind: BTreeMap::from([(id.clone(), spec.clone())]),
            ..m.clone()
        };
        if let Some(why) = rebind_problem(
            &only,
            &annotations,
            &ap.after,
            doc.view_first_line(&ap.file),
        ) {
            return Err(refused(ap.index, why));
        }
    }
    Ok(())
}

/// Refuses a `rebind` entry of `m` (move #`index + 1`) that names an
/// annotation not in `annotations`, unless `kept` says the document already
/// held that entry: a caller that writes one has a typo, while one the
/// document kept after its annotation went away is loaded as it is.
pub fn check_rebind_ids(
    m: &Move,
    index: usize,
    annotations: &[Annotation],
    kept: impl Fn(&str, &MoveSpec) -> bool,
) -> Result<(), Error> {
    for (id, spec) in &m.rebind {
        if !annotations.iter().any(|a| &a.id == id) && !kept(id, spec) {
            return Err(Error::Invalid(format!(
                "move #{}, step {}: rebind names unknown annotation «{id}»",
                index + 1,
                m.at_step
            )));
        }
    }
    Ok(())
}

/// An error from applying a move, naming the move (1-based) and its step,
/// so an author sees which move stopped applying — also when the move that
/// failed is a later one than the one just added or edited.
pub(crate) fn name_move(e: Error, index: usize, at_step: u32) -> Error {
    match e {
        Error::TextNotFound(t) => Error::MoveNoMatch {
            index: index + 1,
            step: at_step,
            what: format!("text «{t}»"),
        },
        Error::NoMatch(what) => Error::MoveNoMatch {
            index: index + 1,
            step: at_step,
            what,
        },
        e @ Error::MoveNoMatch { .. } => e,
        other => Error::Invalid(format!("move #{}, step {}: {other}", index + 1, at_step)),
    }
}

/// Shifts a shape through one patch's line map. `None` = unanchored.
pub fn shift_shape(shape: &Shape, map: &LineMap) -> Option<Shape> {
    shift_shape_by(shape, &|l| map.get(l), &|l, _| map.get_column_anchor(l))
}

/// Shifts a shape line by line: `line` maps a line for line-level anchors
/// (notes, the ends of a range), `column_line` for column anchors (spans,
/// arrow ends), given the line and how many graphemes from its start the
/// anchor stands on (a span's end, an arrow end's column plus one). `None`
/// = unanchored.
pub fn shift_shape_by(
    shape: &Shape,
    line: &dyn Fn(u32) -> Option<u32>,
    column_line: &dyn Fn(u32, u32) -> Option<u32>,
) -> Option<Shape> {
    let span = |s: &TextSpan| {
        column_line(s.line, s.end).map(|l| TextSpan {
            line: l,
            start: s.start,
            end: s.end,
        })
    };
    let anchor = |a: &Anchor| {
        column_line(a.line, a.col.saturating_add(1)).map(|l| Anchor {
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

/// One typed move inside a [`TypingPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypingItem {
    /// Lines of the step's full code (absolute) the content occupies.
    pub range: std::ops::Range<u32>,
    pub lines: Vec<String>,
    pub mode: Typing,
    /// Milliseconds per unit (line or grapheme).
    pub speed_ms: u64,
    pub units: usize,
}

/// The typing animation of a step: the code with every move of the step
/// applied, and the typed moves in order. The TUI reveals it over time;
/// the GIF export turns the same plan into frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypingPlan {
    pub full: Vec<String>,
    pub items: Vec<TypingItem>,
}

impl TypingPlan {
    /// The code while item `index` has `revealed` units shown: earlier
    /// items complete, later ones blank (the line count never changes, so
    /// every shifted annotation stays put).
    pub fn code_at(&self, first_line: u32, index: usize, revealed: usize) -> Vec<String> {
        let mut code = self.full.clone();
        for (k, item) in self.items.iter().enumerate() {
            let shown = if k < index {
                continue;
            } else if k == index {
                partial_content(&item.lines, item.mode, revealed)
            } else {
                vec![String::new(); item.lines.len()]
            };
            for (i, l) in shown.into_iter().enumerate() {
                // A typed line outside the shown window is simply not drawn.
                let Some(at) = (item.range.start + i as u32).checked_sub(first_line) else {
                    continue;
                };
                if let Some(slot) = code.get_mut(at as usize) {
                    *slot = l;
                }
            }
        }
        code
    }

    /// Every intermediate code of the animation with how long it stays, in
    /// order, ending with the full code at the last item's speed: what a
    /// recording draws between the step's arrival and its annotations.
    pub fn frames(&self, first_line: u32) -> Vec<(Vec<String>, u64)> {
        let mut out = Vec::new();
        for (k, item) in self.items.iter().enumerate() {
            for revealed in 0..item.units {
                out.push((self.code_at(first_line, k, revealed), item.speed_ms));
            }
        }
        let last_speed = self.items.last().map_or(0, |i| i.speed_ms);
        out.push((self.full.clone(), last_speed));
        out
    }
}

/// The typing animation of `step`, when any move at that step is typed or
/// waits: the content lines of each typing move of the shown view,
/// followed through the maps of the patches applied after it (same step,
/// same view) so they point into the final code, and every `wait` as one
/// unit of `ms`, all in list order. `None` when nothing is typed or waited
/// at `step` or the moves do not apply.
pub fn typing_plan(doc: &crate::Document, step: u32) -> Option<TypingPlan> {
    if !doc.moves.iter().any(|m| {
        m.at_step == step
            && match &m.action {
                MoveAction::CodePatch(p) => p.typing != Typing::Instant,
                MoveAction::Wait(_) => true,
                _ => false,
            }
    }) {
        return None;
    }
    let proj = project(doc, &doc.moves, step).ok()?;
    let (full, _) = proj.shown_code(doc);
    let mut items: Vec<(usize, TypingItem)> = Vec::new();
    for (k, ap) in proj.patches.iter().enumerate() {
        let MoveAction::CodePatch(p) = &doc.moves[ap.index].action else {
            continue;
        };
        if ap.at_step != step
            || ap.file != proj.shown.file
            || p.typing == Typing::Instant
            || ap.map.inserted.is_empty()
        {
            continue;
        }
        let mut range = ap.map.inserted.clone();
        let mut lost = false;
        for later in proj.patches[k + 1..].iter().filter(|l| l.file == ap.file) {
            match (later.map.get(range.start), later.map.get(range.end - 1)) {
                (Some(a), Some(b)) if b >= a => range = a..b + 1,
                _ => {
                    lost = true;
                    break;
                }
            }
        }
        if lost {
            continue;
        }
        let lines = p.content_lines();
        items.push((
            ap.index,
            TypingItem {
                range,
                units: typing_units(&lines, p.typing),
                lines,
                mode: p.typing,
                speed_ms: p.speed().max(1),
            },
        ));
    }
    for (i, m) in moves_at(&doc.moves, step) {
        if let MoveAction::Wait(w) = &m.action {
            items.push((
                i,
                TypingItem {
                    range: 0..0,
                    lines: Vec::new(),
                    mode: Typing::Instant,
                    speed_ms: w.ms.max(1),
                    units: 1,
                },
            ));
        }
    }
    if items.is_empty() {
        return None;
    }
    items.sort_by_key(|(i, _)| *i);
    Some(TypingPlan {
        full,
        items: items.into_iter().map(|(_, it)| it).collect(),
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
            occurrence: None,
            end_occurrence: None,
        }
    }

    #[test]
    fn occurrence_counts_matches_over_the_whole_code() {
        let c = code("a\nb a\na a\nc");
        assert_eq!(matches_of(&c, 1, "a"), vec![1, 2, 3, 3]);
        let del = |occ: Option<u32>| Patch {
            find: Some("a".into()),
            occurrence: occ,
            ..patch(PatchOp::Delete)
        };
        let e = del(None).resolve(&c, 1).unwrap_err().to_string();
        assert!(
            e.contains("matches 4 times") && e.contains("occurrence: N"),
            "{e}"
        );
        assert_eq!(del(Some(2)).resolve(&c, 1).unwrap().from, 2);
        assert_eq!(
            del(Some(4)).resolve(&c, 1).unwrap().from,
            3,
            "two matches on one line count twice"
        );
        let e = del(Some(5)).resolve(&c, 1).unwrap_err().to_string();
        assert!(e.contains("occurrence 5 asked, only 4 matches"), "{e}");
        assert!(
            del(Some(0))
                .resolve(&c, 1)
                .unwrap_err()
                .to_string()
                .contains("1-based")
        );
    }

    #[test]
    fn replaced_lines_keep_line_anchors_but_drop_column_anchors() {
        let c = code("a\nb\nc\nd");
        let p = Patch {
            position: Some("2-3".into()),
            content: Some("B\nC\nX".into()),
            ..patch(PatchOp::Replace)
        };
        let (_, map) = p.apply(&c, 1).unwrap();
        assert_eq!(map.replaced, 2..4);
        assert_eq!(map.get(2), Some(2));
        assert_eq!(
            map.get_column_anchor(2),
            None,
            "the text under the column is gone"
        );
        assert_eq!(
            map.get_column_anchor(4),
            Some(5),
            "after the edit: shifted, kept"
        );
        let note = Shape::Note {
            line: 2,
            text: "n".into(),
        };
        assert!(matches!(
            shift_shape(&note, &map),
            Some(Shape::Note { line: 2, .. })
        ));
        let hl = Shape::Highlight {
            span: TextSpan {
                line: 2,
                start: 0,
                end: 1,
            },
            label: None,
            emphasis: Default::default(),
        };
        assert!(shift_shape(&hl, &map).is_none());
        let hl4 = Shape::Highlight {
            span: TextSpan {
                line: 4,
                start: 0,
                end: 1,
            },
            label: None,
            emphasis: Default::default(),
        };
        assert!(matches!(
            shift_shape(&hl4, &map),
            Some(Shape::Highlight {
                span: TextSpan { line: 5, .. },
                ..
            })
        ));
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
        assert!(e.contains("matches 2 times") && e.contains("L1, L3"), "{e}");
        let p = Patch {
            find: Some("q".into()),
            ..patch(PatchOp::Delete)
        };
        assert!(matches!(p.resolve(&c, 1), Err(Error::NoMatch(_))));
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
    fn views_are_keyed_by_file_and_windows_only_crop() {
        let mut d = Document::from_code("a\nb\nc\nd\n", "rs", Some("/x/src/main.rs".into()));
        d.reference_file("src/main.rs", None);
        assert_eq!(d.view_key("./src/main.rs"), None, "the main file by name");
        assert_eq!(d.view_key("main.rs"), None, "the main file by file name");
        assert_eq!(d.view_key("/x/src/main.rs"), None, "the main file by path");
        assert_eq!(d.view_key(" lib.rs "), Some("lib.rs".into()));
        d.view_code.insert("lib.rs".into(), code("x\ny\nz"));
        let link = |step: u32, file: &str, lines: Option<&str>| Move {
            at_step: step,
            action: MoveAction::CodeLink(Link {
                file: file.into(),
                lines: lines.map(String::from),
            }),
            rebind: BTreeMap::new(),
            stale: None,
            held: Vec::new(),
        };
        d.add_move(link(1, "lib.rs", Some("2-9"))).unwrap();
        d.add_move(link(2, "main.rs", Some("3-4"))).unwrap();
        d.add_move(link(3, "lib.rs", None)).unwrap();
        assert_eq!(d.code_at(1).unwrap(), (code("y\nz"), 2), "clamped window");
        assert_eq!(
            d.code_at(2).unwrap(),
            (code("c\nd"), 3),
            "the main file, windowed"
        );
        assert_eq!(d.code_at(3).unwrap(), (code("x\ny\nz"), 1));
        assert_eq!(d.shown_view_at(0), ViewRef::default());
        assert_eq!(
            d.shown_view_at(2).label(),
            None,
            "the main file has no label"
        );
        assert_eq!(d.shown_view_at(1).label().as_deref(), Some("lib.rs:2-9"));
        assert_eq!(d.link_files(), ["lib.rs"]);
        // An annotation added at step 1 belongs to lib.rs; at step 2 to the main file.
        let note = |step: u32, line: u32| Annotation {
            id: String::new(),
            step,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Note {
                line,
                text: "n".into(),
            },
        };
        d.add(note(1, 1)).unwrap(); // L1 of lib.rs: outside the window, still valid
        d.add(note(2, 1)).unwrap(); // L1 of main.rs
        assert_eq!(d.annotations[0].file.as_deref(), Some("lib.rs"));
        assert_eq!(d.annotations[1].file, None);
        assert!(matches!(
            d.add(note(1, 9)),
            Err(Error::LineOutOfRange { line: 9, .. })
        ));
        let v = d.view_at(1);
        assert_eq!(v.doc.annotations.len(), 1, "only lib.rs's annotation, kept");
        assert!(v.unanchored.is_empty());
        assert_eq!(d.view_at(2).doc.annotations[0].id, "a2");
        // Embedding stores the linked file; the main code stays the base.
        d.step = 3;
        d.sync_code_to_step();
        d.embed_code();
        assert_eq!(d.views["lib.rs"], code("x\ny\nz"));
        let mut saved = d.clone();
        saved.sync_code_src();
        assert_eq!(saved.source.first_line, 1);
        assert!(matches!(&saved.code_src, crate::model::CodeSource::Embedded(c) if c.len() == 4));
    }

    #[test]
    fn cursor_select_and_wait_project_and_shift() {
        let mut d = Document::from_code("fn a() {}\nlet u = 1;\nlet v = u;\n", "rs", None);
        let mv = |v: serde_json::Value| -> Move { serde_json::from_value(v).unwrap() };
        d.add_move(mv(
            serde_json::json!({"at_step": 1, "action": "cursor", "line": 2, "col": 4}),
        ))
        .unwrap();
        d.add_move(mv(serde_json::json!({"at_step": 1, "action": "select", "from": {"find": "u = 1"}, "to": {"find": "u;"}})))
            .unwrap();
        d.add_move(mv(serde_json::json!({"at_step": 2, "action": "code.patch", "op": "insert", "position": "1", "content": "// top"})))
            .unwrap();
        d.add_move(mv(
            serde_json::json!({"at_step": 3, "action": "wait", "ms": 50}),
        ))
        .unwrap();
        let v1 = d.view_at(1);
        assert_eq!(v1.cursor, Some(Anchor { line: 2, col: 4 }));
        let (a, b) = v1.selection.unwrap();
        assert_eq!((a.line, a.col, b.line, b.col), (2, 4, 3, 10));
        // Shifted by the insert at step 2, cleared at step 0.
        let v2 = d.view_at(2);
        assert_eq!(v2.cursor, Some(Anchor { line: 3, col: 4 }));
        assert_eq!(v2.selection.unwrap().1.line, 4);
        assert!(d.view_at(0).cursor.is_none());
        // The wait is one typing unit of 50 ms; only at its step.
        let plan = typing_plan(&d, 3).unwrap();
        assert_eq!(
            (
                plan.items.len(),
                plan.items[0].units,
                plan.items[0].speed_ms
            ),
            (1, 1, 50)
        );
        assert!(typing_plan(&d, 2).is_none());
        assert_eq!(d.typing_ms_at(3), 50);
        assert_eq!(d.wait_ms_at(3), 50);
        // Errors name the move.
        let e = d
            .add_move(mv(serde_json::json!({"at_step": 1, "action": "select", "from": {"find": "let"}, "to": {"find": "u;"}})))
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("move #5, step 1") && e.contains("occurrence"),
            "{e}"
        );
        let e = d
            .add_move(mv(
                serde_json::json!({"at_step": 1, "action": "cursor", "line": 2, "col": 99}),
            ))
            .unwrap_err()
            .to_string();
        assert!(e.contains("move #5, step 1"), "{e}");
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
