//! Moves (ADR 0007): the code changes between steps. A move is applied when
//! the presenter reaches `at_step` and reverted when going back; the document
//! is a pure function of the step, recomputed from the step-0 code.
//! `code.patch` edits lines (t-098); `code.link` switches the shown file
//! (a *view*, with an optional window of lines); `cursor`, `select` and
//! `wait` set the cursor, a selection and a pause (t-105). Annotations are
//! authored against the code at their own step, belong to the view shown at
//! that step, and are shifted through that view's later patches by the
//! patch's own line map — never by a text search.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::Error;
use crate::model::{Anchor, Annotation, Document, Shape, TextSpan};
use crate::protocol::{MoveSpec, Where};
use crate::redact::{MaskSpan, RedactFate};

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
    /// keys (or an unknown one) is a load-time error.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rebind: BTreeMap<String, MoveSpec>,
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
                    Error::Invalid(format!(
                        "code.link lines «{s}»: use a line or A-B (a window of the file)"
                    ))
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
    /// over the whole code as it is at this step (ADR 0007 §2). Without it,
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
        typing_units(&self.content_lines(), self.typing) as u64 * self.speed().max(1)
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
        return Err(Error::TextNotFound(format!("{what} «{text}»")));
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

    /// Applies the patch at a target already resolved (on the masked code,
    /// t-140: the text under a redact is never matched) to `code`.
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
/// edited, its line map and the view's code right after it (masked when
/// the projection is), with the redact masks of that view at that point.
#[derive(Debug, Clone)]
pub struct AppliedPatch {
    pub index: usize,
    pub at_step: u32,
    pub file: ViewKey,
    pub map: LineMap,
    pub after: Vec<String>,
    /// The masks of the view's redacts right after the patch (t-140).
    pub masks: Vec<MaskSpan>,
}

/// Where one redact is while the moves are applied (t-140). A redact is
/// marked on the code at the end of its own `step` (step 0: the untouched
/// file; see `Document::reframe_redacts`) and is followed from there
/// through every later patch of its view, with the rules of a reload
/// (t-117): it follows the text around it, or fails CLOSED.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RedactAt {
    /// What its mask covers: one span, or several lines when a patch that
    /// changed the line count rewrote its line.
    pub spans: Vec<TextSpan>,
    /// Copies of the value a patch wrote elsewhere: masked too.
    pub copies: Vec<TextSpan>,
    /// It could not follow its text (stored by a reload, or through a patch).
    pub widened: bool,
    /// A patch deleted its line with nothing in its place.
    pub dropped: bool,
    /// The move (index) that last widened or dropped it.
    pub by_move: Option<usize>,
    /// The moves (indexes) whose content copied its value where no mask was.
    pub copied_by: Vec<usize>,
    /// The move (index) after which it really could not follow its text
    /// (its own mask widened), as opposed to [`RedactAt::copy_by`] (t-174).
    pub lost_by: Option<usize>,
    /// The move (index) that last wrote a copy of its value (or a piece)
    /// that is masked as one of [`RedactAt::copies`] (t-174).
    pub copy_by: Option<usize>,
    /// The text under its mask while it still covered it exactly (taken on
    /// the first patch it goes through, t-157): searched for — whole or in
    /// pieces — in every line a later patch writes, even once the mask
    /// itself was widened. Never shown.
    pub value: Option<String>,
}

impl RedactAt {
    /// Every span its mask covers: its own, then the copies.
    pub fn all(&self) -> impl Iterator<Item = &TextSpan> {
        self.spans.iter().chain(self.copies.iter())
    }
}

/// Which code a redact's columns refer to (t-154).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactFrame {
    /// The untouched file (`pin: 0`, or `step: 0`).
    File,
    /// The code right after the move at this index (`pin: N` = index N-1),
    /// the move that wrote its line.
    AfterMove(usize),
    /// The code at the end of this step (no pin: ADR 0007 §3d). On a step
    /// with patches this reading is ambiguous for a document Tiza did not
    /// pin itself, so the projection also masks the file's reading.
    EndOfStep(u32),
}

/// The frame of redact `a` (`None`: not a redact). A pin that names no
/// `code.patch` (a hand-edited file) falls back to the step's reading.
pub fn redact_frame(doc: &Document, a: &Annotation) -> Option<RedactFrame> {
    let Shape::Redact { pin, .. } = &a.shape else {
        return None;
    };
    Some(match pin {
        Some(0) => RedactFrame::File,
        Some(n)
            if doc
                .moves
                .get(*n as usize - 1)
                .is_some_and(|m| matches!(m.action, MoveAction::CodePatch(_))) =>
        {
            RedactFrame::AfterMove(*n as usize - 1)
        }
        _ if a.step == 0 => RedactFrame::File,
        _ => RedactFrame::EndOfStep(a.step),
    })
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
    /// Where each redact is at the step, by annotation index (`None`: not a
    /// redact, or marked on a line that does not exist yet at this step).
    pub redacts: Vec<Option<RedactAt>>,
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
        .ok_or_else(|| Error::TextNotFound(format!("{what} «{text}»")))?;
    Ok(Anchor {
        line: span.line,
        col: if end { span.end } else { span.start },
    })
}

/// The document at `step`: the moves `<= step` applied in step order then
/// list order, each on the view shown at its point, with every redact
/// followed through them (t-140). `masked`: match text targets on the
/// code with the redact masks of that point drawn on it, so a `find` never
/// probes a secret (and `AppliedPatch::after` is the masked code). An
/// error names the move that failed.
pub fn project(
    doc: &Document,
    moves: &[Move],
    step: u32,
    masked: bool,
) -> Result<Projection, Error> {
    let mut order: Vec<(usize, &Move)> = moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.at_step <= step)
        .collect();
    order.sort_by_key(|(i, m)| (m.at_step, *i));
    let track = crate::redact::has_redacts(doc);
    // One slot per annotation, then one per annotation for its "ghost":
    // a redact Tiza did not pin (no `pin`, t-154) whose `step` has moves
    // can be read two ways — its columns on the code after that step's
    // moves (ADR 0007 §3d), or on the file as it was before them (a file
    // written by hand or by an agent, a `.tiza` saved before t-140). Fail
    // closed: both are masked, the second followed from step 0 through
    // every patch like a step-0 one. A pinned redact has one reading.
    let n = doc.annotations.len();
    let mut red: Vec<Option<RedactAt>> = vec![None; 2 * n];
    let frames: Vec<Option<RedactFrame>> = doc
        .annotations
        .iter()
        .map(|a| redact_frame(doc, a))
        .collect();
    if track && !doc.redact_from_step {
        for (i, a) in doc.annotations.iter().enumerate() {
            let Shape::Redact { span, widened, .. } = &a.shape else {
                continue;
            };
            let ambiguous = matches!(frames[i], Some(RedactFrame::EndOfStep(s)) if s >= 1)
                && moves
                    .iter()
                    .any(|m| m.at_step <= a.step && matches!(m.action, MoveAction::CodePatch(_)));
            let fits = doc.view_base_code(&a.file, false).is_some_and(|c| {
                span.line
                    .checked_sub(doc.view_first_line(&a.file))
                    .and_then(|k| c.get(k as usize))
                    .is_some_and(|t| t.graphemes(true).count() as u32 >= span.end)
            });
            if ambiguous && fits {
                red[n + i] = Some(RedactAt {
                    spans: vec![*span],
                    widened: *widened,
                    ..Default::default()
                });
            }
        }
    }
    let mut p = Projection::default();
    p.codes
        .insert(None, doc.view_base_code(&None, false).unwrap_or_default());
    for (i, m) in order {
        let named = |e: Error| name_move(e, i, m.at_step);
        if track {
            // Redacts marked at an earlier step exist from here on.
            birth(doc, &mut red, &frames, |f| match f {
                RedactFrame::File => true,
                RedactFrame::EndOfStep(s) => s < m.at_step,
                RedactFrame::AfterMove(_) => false,
            });
        }
        // The code a text target is matched on.
        let probe = |code: &[String], key: &ViewKey, first: u32, red: &[Option<RedactAt>]| {
            if masked && track {
                crate::redact::mask_with(code, first, &view_masks(doc, red, key))
            } else {
                code.to_vec()
            }
        };
        match &m.action {
            MoveAction::CodePatch(patch) => {
                let key = p.shown.file.clone();
                let first = doc.view_first_line(&key);
                let code = p.codes.get(&key).cloned().unwrap_or_default();
                let target = patch
                    .resolve(&probe(&code, &key, first, &red), first)
                    .map_err(named)?;
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
                let mut masks = Vec::new();
                if track {
                    follow_redacts(doc, &mut red, &key, &code, &new_code, first, &map, i, m);
                    // Redacts pinned to this move start here, on the code
                    // it just wrote (t-154).
                    birth(doc, &mut red, &frames, |f| f == RedactFrame::AfterMove(i));
                    masks = view_masks(doc, &red, &key);
                }
                let after = if masked && track {
                    crate::redact::mask_with(&new_code, first, &masks)
                } else {
                    new_code.clone()
                };
                p.codes.insert(key.clone(), new_code);
                p.patches.push(AppliedPatch {
                    index: i,
                    at_step: m.at_step,
                    file: key,
                    map,
                    after,
                    masks,
                });
            }
            MoveAction::CodeLink(link) => {
                let key = doc.view_key(&link.file);
                if !p.codes.contains_key(&key) {
                    let base = doc.view_base_code(&key, false).ok_or_else(|| {
                        named(Error::Invalid(format!(
                            "code.link «{}»: the file is not loaded (it could not be read next to the document; `:embed` stores it)",
                            link.file
                        )))
                    })?;
                    p.codes.insert(key.clone(), base);
                }
                let window = link.window().map_err(named)?;
                p.shown = ViewRef { file: key, window };
                p.cursor = None;
                p.selection = None;
            }
            MoveAction::Cursor(c) => {
                let key = p.shown.file.clone();
                let first = doc.view_first_line(&key);
                let code = p.codes.get(&key).cloned().unwrap_or_default();
                let last = first + code.len().saturating_sub(1) as u32;
                if c.line < first || c.line > last {
                    return Err(named(Error::LineOutOfRange {
                        line: c.line,
                        first,
                        last,
                    }));
                }
                let len = code[(c.line - first) as usize].graphemes(true).count() as u32;
                let col = c.col.unwrap_or(0);
                if col > len {
                    return Err(named(Error::SpanOutOfRange {
                        line: c.line,
                        start: col,
                        end: col,
                        len,
                    }));
                }
                p.cursor = Some(Anchor { line: c.line, col });
            }
            MoveAction::Select(s) => {
                let key = p.shown.file.clone();
                let first = doc.view_first_line(&key);
                let code = probe(
                    &p.codes.get(&key).cloned().unwrap_or_default(),
                    &key,
                    first,
                    &red,
                );
                let a = resolve_point(doc, &code, first, &s.from, "select from", false)
                    .map_err(named)?;
                let b =
                    resolve_point(doc, &code, first, &s.to, "select to", true).map_err(named)?;
                let (a, b) = if (b.line, b.col) < (a.line, a.col) {
                    (b, a)
                } else {
                    (a, b)
                };
                p.selection = Some((a, b));
            }
            MoveAction::Wait(w) => {
                if w.ms == 0 {
                    return Err(named(Error::Invalid("wait: ms must be >= 1".into())));
                }
            }
        }
    }
    if track {
        birth(doc, &mut red, &frames, |f| match f {
            RedactFrame::File => true,
            RedactFrame::EndOfStep(s) => s <= step,
            // Pinned to a move that already ran but not born there (its
            // move edited another file: a hand-edited pin): fail closed,
            // its stored span is masked anyway.
            RedactFrame::AfterMove(j) => moves.get(j).is_some_and(|m| m.at_step <= step),
        });
    }
    // Each ghost joins its redact (or stands for it, before it exists).
    let ghosts = red.split_off(n);
    for (slot, ghost) in red.iter_mut().zip(ghosts) {
        let Some(g) = ghost.filter(|g| !g.dropped) else {
            continue;
        };
        match slot {
            Some(r) if !r.dropped => {
                let mine: Vec<TextSpan> = r.all().copied().collect();
                for sp in g.all() {
                    if !mine.contains(sp) {
                        r.copies.push(*sp);
                    }
                }
                r.by_move = r.by_move.or(g.by_move);
                r.lost_by = r.lost_by.or(g.lost_by);
                r.copy_by = r.copy_by.or(g.copy_by);
                r.copied_by.extend(g.copied_by);
            }
            _ => *slot = Some(g),
        }
    }
    p.redacts = red;
    Ok(p)
}

/// The annotation behind slot `i` of the redact slots (a ghost's is its
/// redact's).
fn slot_annotation(doc: &Document, i: usize) -> &Annotation {
    &doc.annotations[i % doc.annotations.len()]
}

/// Starts following the redacts whose frame `born` says exists now, at
/// their stored span.
fn birth(
    doc: &Document,
    red: &mut [Option<RedactAt>],
    frames: &[Option<RedactFrame>],
    born: impl Fn(RedactFrame) -> bool,
) {
    for ((a, slot), frame) in doc.annotations.iter().zip(red.iter_mut()).zip(frames) {
        if slot.is_some() {
            continue;
        }
        if let Shape::Redact { span, widened, .. } = &a.shape
            && frame.is_some_and(&born)
        {
            *slot = Some(RedactAt {
                spans: vec![*span],
                widened: *widened,
                ..Default::default()
            });
        }
    }
}

/// The masks the redacts of view `key` draw now (merged where they overlap
/// when drawn; no kept tail once widened or copied).
pub fn view_masks(doc: &Document, red: &[Option<RedactAt>], key: &ViewKey) -> Vec<MaskSpan> {
    let mut out = Vec::new();
    for (i, r) in red.iter().enumerate() {
        let Some(r) = r else { continue };
        let a = slot_annotation(doc, i);
        if &a.file != key || r.dropped {
            continue;
        }
        let keep = if r.widened || r.spans.len() != 1 || !r.copies.is_empty() {
            0
        } else {
            crate::redact::shown_tail(&a.shape)
        };
        for sp in &r.spans {
            out.push(MaskSpan {
                line: sp.line,
                start: sp.start,
                end: sp.end,
                keep,
            });
        }
        for sp in &r.copies {
            out.push(MaskSpan {
                line: sp.line,
                start: sp.start,
                end: sp.end,
                keep: 0,
            });
        }
    }
    out
}

/// Follows the redacts of view `key` through one patch (`before` → `after`,
/// `map`): each span by [`follow_span`], a `rebind` only when it covers
/// what the mask had to cover, and — fail closed — every copy of a masked
/// value that the patch wrote elsewhere is masked too.
#[allow(clippy::too_many_arguments)]
fn follow_redacts(
    doc: &Document,
    red: &mut [Option<RedactAt>],
    key: &ViewKey,
    before: &[String],
    after: &[String],
    first: u32,
    map: &LineMap,
    index: usize,
    m: &Move,
) {
    let line_of = |code: &[String], l: u32| -> Option<String> {
        code.get(l.checked_sub(first)? as usize).cloned()
    };
    // Every mask on the old code, by line: the wildcards a redact's
    // surrounding text is matched with.
    let before_masks: Vec<MaskSpan> = view_masks(doc, red, key);
    let others_on = |line: u32| -> Vec<(usize, usize)> {
        before_masks
            .iter()
            .filter(|m| m.line == line)
            .map(|m| (m.start as usize, m.end as usize))
            .collect()
    };
    // The values under the masks (never shown, only searched for): taken
    // while a mask is still exact and remembered after it widens (t-157).
    let mut secrets: Vec<(usize, String)> = Vec::new();
    for (i, slot) in red.iter_mut().enumerate() {
        let Some(r) = slot.as_mut() else { continue };
        let a = slot_annotation(doc, i);
        if &a.file != key || r.dropped {
            continue;
        }
        if r.value.is_none()
            && !r.widened
            && r.spans.len() == 1
            && r.copies.is_empty()
            && let Some(text) = line_of(before, r.spans[0].line)
        {
            let sp = r.spans[0];
            r.value = Some(crate::model::grapheme_slice(&text, sp.start, sp.end).to_string());
        }
        if let Some(value) = &r.value
            && value.trim().chars().count() >= 3
        {
            secrets.push((i, value.clone()));
        }
    }
    let follow_all = |spans: &[TextSpan], widened: &mut bool| -> Vec<TextSpan> {
        let mut out = Vec::new();
        for sp in spans {
            match follow_span(before, after, first, map, &others_on(sp.line), *sp) {
                RedactFate::Followed(s) => out.push(s),
                RedactFate::Widened(v) => {
                    *widened = true;
                    out.extend(v);
                }
                RedactFate::Dropped => {}
            }
        }
        out
    };
    let n = doc.annotations.len();
    for (i, slot) in red.iter_mut().enumerate() {
        let a = slot_annotation(doc, i);
        if &a.file != key {
            continue;
        }
        let Some(r) = slot.as_mut() else { continue };
        if r.dropped {
            continue;
        }
        let mut widened_now = false;
        let mut spans = follow_all(&r.spans, &mut widened_now);
        let mut copies = follow_all(&r.copies, &mut widened_now);
        if i < n
            && let Some(spec) = m.rebind.get(&a.id)
            && let Some(rb) = rebound(doc, &a.shape, spec, after, first)
            && spans
                .iter()
                .all(|s| s.line == rb.line && rb.start <= s.start && s.end <= rb.end)
        {
            // A rebind is allowed only onto a span that covers the value.
            spans = vec![rb];
        }
        if spans.is_empty() {
            spans = std::mem::take(&mut copies);
            // Its own text is gone; it lives on a copy now: it could not
            // follow its text (said as such, t-174), through the move that
            // wrote that copy.
            if !spans.is_empty() {
                r.lost_by = r.copy_by.or(r.by_move).or(Some(index));
            }
        }
        if spans.is_empty() {
            r.dropped = true;
            r.spans.clear();
            r.by_move = Some(index);
            continue;
        }
        if widened_now {
            r.widened = true;
            r.by_move = Some(index);
            r.lost_by = Some(index);
        }
        r.spans = spans;
        r.copies = copies;
    }
    // Copies of a masked value in the new lines — whole, or a piece of 4+
    // characters of it — are masked, whatever line, and fail CLOSED
    // (t-157): the whole token that holds the copy, or the rest of the line
    // when the token's end is unclear. A move added later at an earlier
    // step can retarget a `position` move onto a line no redact covers, so
    // the copy is all there is to go by.
    for (i, value) in secrets {
        for l in map.inserted.clone() {
            let Some(text) = line_of(after, l) else {
                continue;
            };
            for (b0, b1, whole) in crate::redact::value_hits(&text, &value) {
                let (t0, t1) = crate::redact::token_around(&text, b0, b1);
                let s = crate::model::grapheme_index(&text, t0);
                let e = crate::model::grapheme_index(&text, t1);
                let covered = red.iter().enumerate().any(|(j, r)| {
                    slot_annotation(doc, j).file == *key
                        && r.as_ref().is_some_and(|r| {
                            !r.dropped
                                && r.all()
                                    .any(|sp| sp.line == l && sp.start <= s && e <= sp.end)
                        })
                });
                if !covered && let Some(r) = red[i].as_mut() {
                    r.copies.push(TextSpan {
                        line: l,
                        start: s,
                        end: e,
                    });
                    r.widened = true;
                    r.by_move = Some(index);
                    r.copy_by = Some(index);
                    // Only a copy of the whole value makes `move.add`
                    // refuse the move (t-140 AC6); a piece is masked.
                    if whole && !r.copied_by.contains(&index) {
                        r.copied_by.push(index);
                    }
                }
            }
        }
    }
}

/// Where a `rebind` puts a redact on the code after its patch, when it
/// resolves to a valid span there.
fn rebound(
    doc: &Document,
    shape: &Shape,
    spec: &MoveSpec,
    after: &[String],
    first: u32,
) -> Option<TextSpan> {
    let mut d = doc.clone();
    d.moves.clear();
    d.base_code.clear();
    d.annotations.clear();
    d.code = after.to_vec();
    d.source.first_line = first;
    let mut shape = shape.clone();
    spec.apply(&mut shape, &d).ok()?;
    d.validate_shape(&shape).ok()?;
    match shape {
        Shape::Redact { span, .. } => Some(span),
        _ => None,
    }
}

/// Where one span of a redact goes through a patch (t-140), with the rules
/// of a reload (t-117) on the patch's own line map (ADR 0007 §3e): a line
/// the patch did not touch keeps it exactly (shifted); on a line the patch
/// rewrote it follows the text around it (`others`: the other masks of its
/// old line, as wildcards) on the new line at the same place, or on the
/// one new line of the patch that still has that text; failing that it
/// fails CLOSED — the rest of the new line from where its text no longer
/// matches (the whole line when that column is gone) when the patch kept
/// the line count, else every new line of the patch, whole. Dropped only
/// when its line was deleted with nothing in its place (or left blank).
pub fn follow_span(
    before: &[String],
    after: &[String],
    first: u32,
    map: &LineMap,
    others: &[(usize, usize)],
    span: TextSpan,
) -> RedactFate {
    if !map.replaced.contains(&span.line) {
        return match map.get(span.line) {
            Some(l) => RedactFate::Followed(TextSpan { line: l, ..span }),
            None => RedactFate::Dropped,
        };
    }
    let text = |code: &[String], l: u32| -> Option<String> {
        code.get(l.checked_sub(first)? as usize).cloned()
    };
    let Some(old) = text(before, span.line) else {
        return RedactFate::Dropped;
    };
    let og: Vec<&str> = old.graphemes(true).collect();
    let end = (span.end as usize).min(og.len());
    let start = (span.start as usize).min(end);
    let (literals, k) = crate::redact::literals_around(&og, others, start, end);
    let try_line = |l: u32| -> Option<TextSpan> {
        let new = text(after, l)?;
        if new == old {
            return Some(TextSpan {
                line: l,
                start: start as u32,
                end: end as u32,
            });
        }
        let (s, e) = *crate::redact::match_masks(&new, &literals)?.get(k)?;
        (s < e).then_some(TextSpan {
            line: l,
            start: s,
            end: e,
        })
    };
    let same_count = map.replaced.len() == map.inserted.len();
    let in_place = map.inserted.start + (span.line - map.replaced.start);
    if start < end {
        if same_count && let Some(found) = try_line(in_place) {
            return RedactFate::Followed(found);
        }
        let hits: Vec<TextSpan> = map
            .inserted
            .clone()
            .filter(|l| !(same_count && *l == in_place))
            .filter_map(try_line)
            .collect();
        match hits.as_slice() {
            [only] => return RedactFate::Followed(*only),
            [] => {}
            many => {
                // Its text is on several new lines: every one of them.
                let mut v = many.to_vec();
                if same_count
                    && let Some(rest) = text(after, in_place)
                        .and_then(|t| crate::redact::rest_of_line(&t, in_place, &og, start))
                {
                    v.push(rest);
                }
                return RedactFate::Widened(v);
            }
        }
    }
    if same_count {
        return match text(after, in_place)
            .and_then(|t| crate::redact::rest_of_line(&t, in_place, &og, start))
        {
            Some(sp) => RedactFate::Widened(vec![sp]),
            None => RedactFate::Dropped,
        };
    }
    let lines: Vec<TextSpan> = map
        .inserted
        .clone()
        .filter_map(|l| crate::redact::whole_line(&text(after, l)?, l))
        .collect();
    if lines.is_empty() {
        RedactFate::Dropped
    } else {
        RedactFate::Widened(lines)
    }
}

/// Where a line of view `key` at the end of `step` comes from (t-154).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineOrigin {
    /// The earliest step whose code already holds the line exactly as it is
    /// at the end of `step`, and the line's number at the end of that step.
    pub frame: u32,
    pub frame_line: u32,
    /// The move (index) that wrote the line, and its number right after
    /// that move; `None`: a line of the file (`frame` is then 0).
    pub writer: Option<(usize, u32)>,
}

/// The earliest step whose code already holds `line` of view `key` exactly
/// as it is at the end of `step` (no patch in between created or rewrote
/// it), and the line's number there: where a redact marked at `step` on
/// that line belongs, so the moves that come later — or that are added
/// later at earlier steps — follow it (t-140). `(0, line)` without moves.
pub fn frame_of_line(doc: &Document, key: &ViewKey, step: u32, line: u32) -> (u32, u32) {
    let o = line_origin(doc, key, step, line);
    (o.frame, o.frame_line)
}

/// [`frame_of_line`] plus the move that wrote the line (t-154).
pub fn line_origin(doc: &Document, key: &ViewKey, step: u32, line: u32) -> LineOrigin {
    let unknown = LineOrigin {
        frame: step,
        frame_line: line,
        writer: None,
    };
    let Ok(p) = project(doc, &doc.moves, step, false) else {
        return unknown;
    };
    let first = doc.view_first_line(key);
    // Each patch of the view with the code it was applied to.
    let mut prev = doc.view_base_code(key, false).unwrap_or_default();
    let mut patches: Vec<(&AppliedPatch, Vec<String>)> = Vec::new();
    for ap in p.patches.iter().filter(|ap| &ap.file == key) {
        let before = std::mem::replace(&mut prev, ap.after.clone());
        patches.push((ap, before));
    }
    let mut cur = line;
    let mut i = patches.len();
    while i > 0 {
        let st = patches[i - 1].0.at_step;
        // At the end of `st` the line is already what it is at `step`.
        let (frame, frame_line) = (st, cur);
        while i > 0 && patches[i - 1].0.at_step == st {
            let (ap, before) = &patches[i - 1];
            let wrote = LineOrigin {
                frame,
                frame_line,
                writer: Some((ap.index, cur)),
            };
            let Some(o) = ap.map.map.iter().position(|n| *n == Some(cur)) else {
                return wrote; // the patch wrote this line
            };
            let o = first + o as u32;
            if ap.map.replaced.contains(&o) {
                let old = before.get((o - first) as usize);
                let new = ap.after.get((cur - first) as usize);
                if old != new {
                    return wrote; // rewritten with other text
                }
            }
            cur = o;
            i -= 1;
        }
    }
    LineOrigin {
        frame: 0,
        frame_line: cur,
        writer: None,
    }
}

/// Where `line` of the code shown at step `from` is at step `to` (t-030:
/// the live pointer stays on its line across `n`/`p`): carried forward
/// through every patch of the shown view between the two steps, or back
/// through them in reverse. `None` when a patch removed the line, when the
/// two steps show different files, or when the moves do not apply. The
/// same line without moves. Only line maps are read: no text, masked or
/// not, reaches the caller.
pub fn carry_line(doc: &Document, from: u32, to: u32, line: u32) -> Option<u32> {
    if doc.moves.is_empty() || from == to {
        return Some(line);
    }
    let (lo, hi) = (from.min(to), from.max(to));
    let low = project(doc, &doc.moves, lo, false).ok()?;
    let high = project(doc, &doc.moves, hi, false).ok()?;
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

/// The code of view `key` right after the move at `index` (unmasked), and
/// its first line: what a redact pinned to that move is written on.
pub fn code_after_move(doc: &Document, index: usize) -> Option<(ViewKey, Vec<String>, u32)> {
    let m = doc.moves.get(index)?;
    let p = project(doc, &doc.moves, m.at_step, false).ok()?;
    let ap = p.patches.iter().find(|ap| ap.index == index)?;
    Some((
        ap.file.clone(),
        ap.after.clone(),
        doc.view_first_line(&ap.file),
    ))
}

/// Where a redact pinned to the move at `old_index` of `old` goes on `new`
/// (the same document after a move was added or deleted), the move now at
/// `new_index` (t-154). The move wrote its line, so it keeps its offset in
/// that move's new lines; it stays exact while that line's text is the
/// same, else it fails CLOSED like a patch (rest of the line, or the whole
/// line). `None`: nothing to say where (a move that no longer applies).
pub fn repin_span(
    old: &Document,
    old_index: usize,
    new: &Document,
    new_index: usize,
    span: TextSpan,
) -> Option<RedactFate> {
    let at = |d: &Document, i: usize| -> Option<AppliedPatch> {
        let m = d.moves.get(i)?;
        let p = project(d, &d.moves, m.at_step, false).ok()?;
        p.patches.into_iter().find(|ap| ap.index == i)
    };
    let (o, n) = (at(old, old_index)?, at(new, new_index)?);
    let first = new.view_first_line(&n.file);
    let line = (i64::from(n.map.inserted.start) + i64::from(span.line)
        - i64::from(o.map.inserted.start))
    .max(i64::from(first)) as u32;
    let text = |code: &[String], l: u32| -> Option<String> {
        code.get(l.checked_sub(first)? as usize).cloned()
    };
    let old_text = text(&o.after, span.line)?;
    let Some(new_text) = text(&n.after, line) else {
        // Its offset is past the move's lines (a hand-edited pin): the
        // whole of the move's last line, never nothing.
        let last = n.map.inserted.end.checked_sub(1)?;
        let t = text(&n.after, last)?;
        return Some(match crate::redact::whole_line(&t, last) {
            Some(sp) => RedactFate::Widened(vec![sp]),
            None => RedactFate::Dropped,
        });
    };
    if old_text == new_text {
        return Some(RedactFate::Followed(TextSpan { line, ..span }));
    }
    // The same rule as a patch that rewrote the line in place.
    let map = LineMap {
        first_line: span.line,
        map: vec![Some(line)],
        inserted: line..line + 1,
        replaced: span.line..span.line + 1,
        old_len: 1,
        new_len: 1,
    };
    let mut before = vec![String::new(); (span.line - first) as usize];
    before.push(old_text);
    let mut after = vec![String::new(); (line - first) as usize];
    after.push(new_text.clone());
    Some(match follow_span(&before, &after, first, &map, &[], span) {
        RedactFate::Dropped => match crate::redact::whole_line(&new_text, line) {
            Some(sp) => RedactFate::Widened(vec![sp]),
            None => RedactFate::Dropped,
        },
        f => f,
    })
}

/// Which lines a `code.patch` hits where it runs (t-157): the lines it
/// replaces or deletes, or the line an insert goes before (the last line
/// when it appends), with their text then — raw to compare, masked to quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchHit {
    pub at_step: u32,
    pub file: ViewKey,
    pub op: PatchOp,
    /// First and last line hit; for an insert, the line it goes before.
    pub from: u32,
    pub to: u32,
    /// An insert past the last line (it appends after `from`).
    pub appends: bool,
    pub text: Vec<String>,
    pub masked: Vec<String>,
}

/// [`PatchHit`] of every `code.patch` of `doc` that applies, by move index.
pub fn patch_hits(doc: &Document) -> BTreeMap<usize, PatchHit> {
    let mut out = BTreeMap::new();
    let (Ok(raw), Ok(shown)) = (
        project(doc, &doc.moves, u32::MAX, false),
        project(doc, &doc.moves, u32::MAX, true),
    ) else {
        return out;
    };
    let mut cur: BTreeMap<ViewKey, (Vec<String>, Vec<String>)> = BTreeMap::new();
    for (ap, mp) in raw.patches.iter().zip(&shown.patches) {
        let first = doc.view_first_line(&ap.file);
        let (before, before_masked) = cur
            .entry(ap.file.clone())
            .or_insert_with(|| {
                (
                    doc.view_base_code(&ap.file, false).unwrap_or_default(),
                    doc.view_base_code(&ap.file, true).unwrap_or_default(),
                )
            })
            .clone();
        if let MoveAction::CodePatch(patch) = &doc.moves[ap.index].action
            && let Ok(t) = patch.resolve(&before, first)
        {
            let last = first + before.len().saturating_sub(1) as u32;
            let (from, to, appends) = match patch.op {
                PatchOp::Insert if t.from > last => (last, last, true),
                PatchOp::Insert => (t.from, t.from, false),
                _ => (t.from, t.to, false),
            };
            let slice = |code: &[String]| -> Vec<String> {
                (from..=to)
                    .filter_map(|l| code.get(l.checked_sub(first)? as usize).cloned())
                    .collect()
            };
            out.insert(
                ap.index,
                PatchHit {
                    at_step: ap.at_step,
                    file: ap.file.clone(),
                    op: patch.op,
                    from,
                    to,
                    appends,
                    text: slice(&before),
                    masked: slice(&before_masked),
                },
            );
        }
        cur.insert(ap.file.clone(), (ap.after.clone(), mp.after.clone()));
    }
    out
}

/// A line as a retarget notice quotes it (t-157): the masked text, cut
/// where its first mask starts (`token=…`), short, and nothing at all when
/// what is left still looks like a secret.
fn quote_line(masked: &[String]) -> String {
    let Some(line) = masked.first() else {
        return "…".into();
    };
    let mut q = line.trim().to_string();
    let mut cut = false;
    if let Some(k) = q.find('•') {
        q.truncate(k);
        cut = true;
    }
    if q.graphemes(true).count() > 32 {
        q = q.graphemes(true).take(31).collect();
        cut = true;
    }
    if crate::redact::looks_like_secret(&q) {
        return "…".into();
    }
    if cut || masked.len() > 1 {
        q.push('…');
    }
    q
}

/// After the moves changed (`old` → `new`; `new_index` maps an old move
/// to its index in `new`, `None` when it was deleted), one notice per
/// `code.patch` that now hits other lines than before (t-157): a move
/// added or deleted at an earlier step shifts the code a `position`
/// target counts on (ADR 0007 §3d). The notice quotes only masked text.
pub fn retarget_notices(
    old: &Document,
    new: &Document,
    new_index: impl Fn(usize) -> Option<usize>,
) -> Vec<String> {
    let (before, after) = (patch_hits(old), patch_hits(new));
    let mut out = Vec::new();
    for (i, o) in &before {
        let Some(j) = new_index(*i) else { continue };
        let Some(n) = after.get(&j) else { continue };
        if o.text == n.text && o.appends == n.appends {
            continue;
        }
        let mut lines = if n.from == n.to {
            format!("L{}", n.from)
        } else {
            format!("L{}-{}", n.from, n.to)
        };
        if let Some(f) = &n.file {
            lines = format!("{lines} of {f}");
        }
        let (was, now) = (quote_line(&o.masked), quote_line(&n.masked));
        let what = match n.op {
            PatchOp::Replace => format!("replaces {lines} «{now}» instead of «{was}»"),
            PatchOp::Delete => format!("deletes {lines} «{now}» instead of «{was}»"),
            PatchOp::Insert => {
                let place = |h: &PatchHit, q: &str| {
                    if h.appends {
                        format!("after «{q}» (the end)")
                    } else {
                        format!("before «{q}»")
                    }
                };
                format!(
                    "inserts at {} {} instead of {}",
                    if n.appends {
                        format!("L{}", n.from + 1)
                    } else {
                        lines
                    },
                    place(n, &now),
                    place(o, &was)
                )
            }
        };
        out.push(format!(
            "move #{} (step {}) now {what}: check it",
            j + 1,
            n.at_step
        ));
    }
    out
}

/// [`retarget_notices`] between two states of the same document whose
/// moves differ by one move added, one deleted or none (an undo or redo
/// swapping one edit, t-157): each move of `old` is matched to its index
/// in `new` from the lists themselves. Moves changed any other way are
/// matched by index.
pub fn retarget_notices_between(old: &Document, new: &Document) -> Vec<String> {
    let (a, b) = (&old.moves, &new.moves);
    // The first index where the lists part: the move added or deleted
    // there (an added move is pushed last, a deleted one can be anywhere).
    let split = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    if b.len() == a.len() + 1 && a[split..] == b[split + 1..] {
        retarget_notices(old, new, |i| Some(if i < split { i } else { i + 1 }))
    } else if a.len() == b.len() + 1 && a[split + 1..] == b[split..] {
        retarget_notices(old, new, |i| match i.cmp(&split) {
            std::cmp::Ordering::Less => Some(i),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(i - 1),
        })
    } else {
        retarget_notices(old, new, |i| (i < b.len()).then_some(i))
    }
}

/// The moves as they are read back (`scene.get`, `move.list`, the MCP,
/// t-140): the content of every `code.patch` with the masks of that point
/// drawn on it — any text that lands under a redact after the patch shows
/// as the eight `•`. The authored moves stay in the document (a `.tiza` is
/// the presenter's own file).
pub fn masked_moves(doc: &Document) -> Vec<Move> {
    let mut out = doc.moves.clone();
    if !crate::redact::has_redacts(doc) {
        return out;
    }
    let Ok(p) = project(doc, &doc.moves, u32::MAX, false) else {
        // Moves that do not apply: nothing can say where the masks are, so
        // no content is handed out.
        for m in &mut out {
            if let MoveAction::CodePatch(patch) = &mut m.action
                && patch.content.is_some()
            {
                let n = patch.content_lines().len();
                patch.content = Some(vec![crate::redact::MASK; n].join("\n"));
            }
        }
        return out;
    };
    // The state at the end of each step that has patches: a redact marked
    // at that step on a line its move wrote masks that content too.
    let mut at_end: BTreeMap<u32, Projection> = BTreeMap::new();
    for ap in &p.patches {
        if !at_end.contains_key(&ap.at_step)
            && let Ok(q) = project(doc, &doc.moves, ap.at_step, false)
        {
            at_end.insert(ap.at_step, q);
        }
    }
    for (k, ap) in p.patches.iter().enumerate() {
        let MoveAction::CodePatch(patch) = &mut out[ap.index].action else {
            continue;
        };
        if ap.map.inserted.is_empty() {
            continue;
        }
        let first = doc.view_first_line(&ap.file);
        let lines = patch.content_lines();
        // Masks on the content, as lines 1..=n.
        let mut masks: Vec<MaskSpan> = Vec::new();
        let to_content = |m: &MaskSpan, line_of_j: &dyn Fn(u32) -> Option<u32>| {
            line_of_j(m.line).map(|j| MaskSpan { line: j, ..*m })
        };
        // Right after the patch.
        let inserted = ap.map.inserted.clone();
        let after_j = |l: u32| inserted.contains(&l).then(|| l - inserted.start + 1);
        masks.extend(ap.masks.iter().filter_map(|m| to_content(m, &after_j)));
        // At the end of its step: its lines followed through the later
        // patches of that step (only where the text is still the same).
        if let Some(q) = at_end.get(&ap.at_step) {
            let mut range = Some(inserted.clone());
            for later in p.patches[k + 1..]
                .iter()
                .filter(|l| l.file == ap.file && l.at_step == ap.at_step)
            {
                range = range.and_then(|r| {
                    let (a, b) = (later.map.get(r.start)?, later.map.get(r.end - 1)?);
                    (b >= a && b - a == r.end - 1 - r.start).then_some(a..b + 1)
                });
            }
            if let (Some(range), Some(code)) = (range, q.codes.get(&ap.file)) {
                let end_masks = view_masks(doc, &q.redacts, &ap.file);
                let same = |l: u32| -> Option<u32> {
                    let j = l.checked_sub(range.start)?;
                    let text = code.get((l.checked_sub(first)?) as usize)?;
                    (range.contains(&l) && lines.get(j as usize) == Some(text)).then_some(j + 1)
                };
                masks.extend(end_masks.iter().filter_map(|m| to_content(m, &same)));
            }
        }
        if masks.is_empty() {
            continue;
        }
        let new = crate::redact::mask_with(&lines, 1, &masks);
        if new != lines {
            let trailing = patch.content.as_deref().is_some_and(|c| c.ends_with('\n'));
            patch.content = Some(new.join("\n") + if trailing { "\n" } else { "" });
        }
    }
    out
}

/// Validates every move against the code at its step: targets resolve to
/// exactly one place, `at_step >= 1`, `rebind` ids exist and only on
/// patches, linked files are loaded, cursors and selections exist.
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
                "move #{} (step {}): rebind only applies to code.patch, not {}",
                i + 1,
                m.at_step,
                m.action.name()
            )));
        }
        for (id, spec) in &m.rebind {
            if !annotations.iter().any(|a| &a.id == id) {
                return Err(Error::Invalid(format!(
                    "move #{} (step {}): rebind names unknown annotation `{id}`",
                    i + 1,
                    m.at_step
                )));
            }
            if spec.is_empty() {
                return Err(Error::Invalid(format!(
                    "move #{} (step {}): rebind `{id}` says nowhere to go: use `at` {{find | line, col}}, `from`/`to` (arrows) or `from_line`/`to_line` (boxes, callouts, spotlights)",
                    i + 1,
                    m.at_step
                )));
            }
        }
    }
    // Apply in order, checking each patch's `rebind` finds on the code after
    // it: exactly one match, or an `occurrence` within the matches.
    let p = project(doc, moves, u32::MAX, true)?;
    for ap in &p.patches {
        let m = &moves[ap.index];
        let first = doc.view_first_line(&ap.file);
        for (id, spec) in &m.rebind {
            for w in [&spec.at, &spec.from, &spec.to].into_iter().flatten() {
                let (Some(text), None) = (&w.find, w.line) else {
                    continue;
                };
                let n = matches_of(&ap.after, first, text).len();
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
                    return Err(Error::Invalid(format!(
                        "move #{} (step {}): rebind `{id}` {why}",
                        ap.index + 1,
                        m.at_step
                    )));
                }
            }
        }
    }
    Ok(())
}

/// An error from applying a move, naming the move (1-based) and its step,
/// so an author sees which move stopped applying — also when the move that
/// failed is a later one than the one just added or edited.
fn name_move(e: Error, index: usize, at_step: u32) -> Error {
    match e {
        Error::TextNotFound(t) => {
            Error::TextNotFound(format!("{t} (move #{}, step {})", index + 1, at_step))
        }
        other => Error::Invalid(format!("move #{} (step {}): {other}", index + 1, at_step)),
    }
}

/// Shifts a shape through one patch's line map. `None` = unanchored.
pub fn shift_shape(shape: &Shape, map: &LineMap) -> Option<Shape> {
    let line = |l: u32| map.get(l);
    let span = |s: &TextSpan| {
        map.get_column_anchor(s.line).map(|l| TextSpan {
            line: l,
            start: s.start,
            end: s.end,
        })
    };
    let anchor = |a: &Anchor| {
        map.get_column_anchor(a.line).map(|l| Anchor {
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
        Shape::Redact {
            span: s,
            keep,
            label,
            widened,
            pin,
        } => Shape::Redact {
            span: span(s)?,
            keep: *keep,
            label: label.clone(),
            widened: *widened,
            pin: *pin,
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
/// the GIF export (t-120) turns the same plan into frames.
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
    let proj = project(doc, &doc.moves, step, false).ok()?;
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
            e.contains("move #5 (step 1)") && e.contains("occurrence"),
            "{e}"
        );
        let e = d
            .add_move(mv(
                serde_json::json!({"at_step": 1, "action": "cursor", "line": 2, "col": 99}),
            ))
            .unwrap_err()
            .to_string();
        assert!(e.contains("move #5 (step 1)"), "{e}");
    }

    #[test]
    fn a_redact_follows_its_text_or_fails_closed_through_a_patch() {
        let before = code("{\"token\": \"abc123secret\", \"user\": \"ana\"}\nnext");
        let span = TextSpan {
            line: 1,
            start: 11,
            end: 23,
        };
        let through = |content: &str, op: PatchOp| {
            let p = Patch {
                position: Some("1".into()),
                content: (op != PatchOp::Delete).then(|| content.to_string()),
                ..patch(op)
            };
            let (after, map) = p.apply(&before, 1).unwrap();
            follow_span(&before, &after, 1, &map, &[], span)
        };
        // The text around it intact: the new value, whatever its length.
        assert_eq!(
            through(
                "{\"token\": \"abc123LONGER99\", \"user\": \"ana\"}",
                PatchOp::Replace
            ),
            RedactFate::Followed(TextSpan {
                line: 1,
                start: 11,
                end: 25
            })
        );
        // The key renamed: from where the line first differs, to its end.
        assert_eq!(
            through(
                "{\"apiToken\": \"abc123secret\", \"user\": \"ana\"}",
                PatchOp::Replace
            ),
            RedactFate::Widened(vec![TextSpan {
                line: 1,
                start: 2,
                end: 43
            }])
        );
        // Split in two: both new lines, whole.
        assert_eq!(
            through(
                "{\"token\": \"abc123secret\",\n  \"user\": \"ana\"}",
                PatchOp::Replace
            ),
            RedactFate::Widened(vec![
                TextSpan {
                    line: 1,
                    start: 0,
                    end: 25
                },
                TextSpan {
                    line: 2,
                    start: 2,
                    end: 16
                }
            ])
        );
        // An insert above shifts it; a delete drops it.
        assert_eq!(
            through("// top", PatchOp::Insert),
            RedactFate::Followed(TextSpan { line: 2, ..span })
        );
        assert_eq!(through("", PatchOp::Delete), RedactFate::Dropped);
    }

    #[test]
    fn a_redact_is_pinned_to_the_code_its_line_comes_from() {
        let mut d = Document::from_code("a\nb\nc\n", "txt", None);
        let mv = |v: serde_json::Value| -> Move { serde_json::from_value(v).unwrap() };
        d.add_move(mv(serde_json::json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "top"})))
            .unwrap();
        d.add_move(mv(serde_json::json!({"at_step": 2, "action": "code.patch", "op": "replace", "position": "3", "content": "B"})))
            .unwrap();
        // L4 at step 2 is `c`, untouched since step 0, where it was L3.
        assert_eq!(frame_of_line(&d, &None, 2, 4), (0, 3));
        // L3 at step 2 was written by the step-2 move.
        assert_eq!(frame_of_line(&d, &None, 2, 3), (2, 3));
        // L1 at step 3 was inserted at step 1 and kept since.
        assert_eq!(frame_of_line(&d, &None, 3, 1), (1, 1));
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
