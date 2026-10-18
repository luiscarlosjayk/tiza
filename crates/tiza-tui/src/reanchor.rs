//! Re-anchoring annotations after the code changes (paste, follow mode).
//!
//! Each annotation is matched in two stages:
//!
//! 1. the **line**, by its whole trimmed text — never by a bare word, so a
//!    note on `}` does not jump to the first brace. A line diff (LCS over the
//!    trimmed lines) matches the two versions *in order*, so each repeated
//!    `}` or `fn new()` maps to its own copy, and lines inserted or deleted
//!    above simply shift the rest. A line the diff could not match (edited,
//!    deleted, or moved) is looked up by its text: when that text is unique
//!    in the new version and within [`MAX_DISTANCE`] of the expected position
//!    (old number plus the shift of the nearest matched line above), the
//!    block moved and the annotation follows it; when the text is repeated,
//!    the annotated copy is gone and the annotation is dropped — it never
//!    jumps to another copy (the one exception: two identical neighbouring
//!    lines collapsed into one, `}` `}` → `}`, keep both annotations on the
//!    survivor);
//! 2. the **span** inside that line: the same occurrence number of the same
//!    text. A line edited in place keeps its spans as long as the line at
//!    the expected position still holds that occurrence; if the occurrence
//!    is gone, the annotation is dropped rather than moved to another one.
//!
//! Beyond [`MAX_DISTANCE`] lines from the expected position, a matching line
//! counts as a different place and the annotation is dropped.
//!
//! Lines the diff did not match are judged by their *hunk* (the gap between
//! the matched lines around them): a line replaced in place (1↔1 hunk, or
//! line i of an N↔N hunk) keeps its position — a note on it survives, like a
//! box edge, and a span stays when its text is still there; a span whose line
//! was split or joined by a formatter (1→N, N→1) follows its text when exactly
//! one line of the hunk's new side holds it. A range (box, callout,
//! spotlight) never stretches after an edge that moved away with a block: it
//! keeps its height from the edge that stayed.

use std::collections::HashMap;

use tiza_core::Document;
use tiza_core::model::{Anchor, Shape, TextSpan};
use unicode_segmentation::UnicodeSegmentation;

/// Farthest a line may move from its expected position and still be "the same line".
pub const MAX_DISTANCE: u32 = 50;

/// Largest `old × new` middle (after trimming the common prefix and suffix)
/// the exact LCS is computed for; bigger edits fall back to unique-line anchors.
const LCS_BUDGET: usize = 4_000_000;

/// Outcome of re-anchoring a document's annotations.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Whether the reload pushed an undo step.
    pub recorded: bool,
    pub kept: usize,
    pub dropped: usize,
}

/// How a line was found in the new version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// Matched by the diff: the same line, in place (possibly shifted).
    Matched,
    /// Not matched, but its unique text is elsewhere: the block moved.
    Moved,
    /// Replaced in place (a 1↔1 hunk): same position, new text.
    InPlace,
    /// One of several identical neighbours that collapsed into one.
    Collapsed,
}

/// Line mapping from `old` to `new`, shared by every annotation of one reload.
pub struct Remap<'a> {
    old: &'a Document,
    new: &'a Document,
    /// Old line → new line, increasing in both, for the lines the diff matched.
    matched: Vec<(u32, u32)>,
    /// Trimmed text → new lines with that text.
    index: HashMap<&'a str, Vec<u32>>,
}

impl<'a> Remap<'a> {
    pub fn new(old: &'a Document, new: &'a Document) -> Remap<'a> {
        let mut index: HashMap<&str, Vec<u32>> = HashMap::new();
        for (i, l) in new.code.iter().enumerate() {
            let t = l.trim();
            if !t.is_empty() {
                index
                    .entry(t)
                    .or_default()
                    .push(new.source.first_line + i as u32);
            }
        }
        let a: Vec<&str> = old.code.iter().map(|l| l.trim()).collect();
        let b: Vec<&str> = new.code.iter().map(|l| l.trim()).collect();
        let matched = line_diff(&a, &b)
            .into_iter()
            .map(|(i, j)| {
                (
                    old.source.first_line + i as u32,
                    new.source.first_line + j as u32,
                )
            })
            .collect();
        Remap {
            old,
            new,
            matched,
            index,
        }
    }

    /// Shift of the nearest matched line above `old_line` (or below, when
    /// none is above): where the line is expected to be now.
    fn expected_from(&self, old_line: u32, above: bool) -> Option<u32> {
        let pair = if above {
            self.matched.iter().rev().find(|(o, _)| *o <= old_line)
        } else {
            self.matched.iter().find(|(o, _)| *o > old_line)
        };
        let (o, n) = *pair?;
        Some((old_line as i64 + n as i64 - o as i64).max(self.new.source.first_line as i64) as u32)
    }

    /// Where `old_line` is expected to be now (shift of the nearest matched
    /// line above it, else below, else unchanged).
    fn expected(&self, old_line: u32) -> u32 {
        self.expected_from(old_line, true)
            .or_else(|| self.expected_from(old_line, false))
            .unwrap_or(old_line)
    }

    /// The hunk around an unmatched `old_line`: the old lines and the new
    /// lines between the nearest matched pairs above and below it.
    fn hunk(&self, old_line: u32) -> (std::ops::Range<u32>, std::ops::Range<u32>) {
        let i = self.matched.partition_point(|(o, _)| *o < old_line);
        let (o1, n1) = if i == 0 {
            (
                self.old.source.first_line.saturating_sub(1),
                self.new.source.first_line.saturating_sub(1),
            )
        } else {
            self.matched[i - 1]
        };
        let (o2, n2) = self
            .matched
            .get(i)
            .copied()
            .unwrap_or((self.old.last_line() + 1, self.new.last_line() + 1));
        (o1 + 1..o2, n1 + 1..n2)
    }

    /// New number of `old_line`, or `None` when that line is gone.
    pub fn line(&mut self, old_line: u32) -> Option<u32> {
        self.locate(old_line).map(|(l, _)| l)
    }

    /// New number of `old_line` and how it was found; `None` when it is gone.
    pub fn locate(&mut self, old_line: u32) -> Option<(u32, Found)> {
        if let Ok(i) = self.matched.binary_search_by_key(&old_line, |(o, _)| *o) {
            return Some((self.matched[i].1, Found::Matched));
        }
        let text = self.old.line_text(old_line)?.trim();
        let expected = self.expected(old_line);
        if text.is_empty() {
            // A blank line follows its neighbours; it must still be blank.
            return self
                .new
                .line_text(expected)
                .is_some_and(|l| l.trim().is_empty())
                .then_some((expected, Found::Matched));
        }
        // Not matched by the diff. A block moved: the text is unique on both
        // sides, nobody else took it, and it is close enough.
        let in_old = self.old.code.iter().filter(|l| l.trim() == text).count();
        if in_old == 1
            && let Some([only]) = self.index.get(text).map(Vec::as_slice)
            && only.abs_diff(expected) <= MAX_DISTANCE
            && self
                .matched
                .binary_search_by_key(only, |(_, n)| *n)
                .is_err()
        {
            return Some((*only, Found::Moved));
        }
        let (ho, hn) = self.hunk(old_line);
        // Replaced in place: the line is still there, with other text. A
        // note on it (or a range edge) stays; spans check their text later.
        // An N↔N hunk (a parameter renamed on the signature line and on the
        // line that uses it) pairs its lines i↔i the same way; only when
        // the counts differ does the reflow search of `span` apply.
        if !ho.is_empty() && ho.len() == hn.len() {
            return Some((hn.start + (old_line - ho.start), Found::InPlace));
        }
        // Repeated text (one `}` among many): the only survivor is a run of
        // identical lines that shrank (`}` `}` → `}`); the annotations of the
        // deleted copies stay on the line left. Any other deletion means this
        // copy is gone, and the annotation goes with it rather than jumping to
        // another copy — even when a neighbour happens to have the same text.
        if !hn.is_empty()
            || !ho
                .clone()
                .all(|l| self.old.line_text(l).map(str::trim) == Some(text))
        {
            return None;
        }
        [old_line.checked_sub(1), Some(old_line + 1)]
            .into_iter()
            .flatten()
            .filter(|&l| self.old.line_text(l).map(str::trim) == Some(text))
            .find_map(|l| {
                self.matched
                    .binary_search_by_key(&l, |(o, _)| *o)
                    .ok()
                    .map(|i| (self.matched[i].1, Found::Collapsed))
            })
    }

    /// New place of a span: same line (by text), same occurrence of the same text.
    pub fn span(&mut self, span: &TextSpan) -> Option<TextSpan> {
        let old_line = self.old.line_text(span.line)?;
        let og: Vec<&str> = old_line.graphemes(true).collect();
        let end = (span.end as usize).min(og.len());
        let start = (span.start as usize).min(end);
        if start == end {
            return None;
        }
        let text = og[start..end].concat();
        let occurrence = count_occurrences(&og[..start].concat(), &text);
        // The line by its text or its place; failing that, a formatter
        // split or joined it (1→N, N→1 hunk): the span follows its text when
        // exactly one line of the hunk's new side holds it.
        let line = match self.line(span.line) {
            Some(l) => l,
            None => {
                let (_, hn) = self.hunk(span.line);
                let mut holders = hn.filter(|&l| {
                    self.new
                        .line_text(l)
                        .is_some_and(|t| t.contains(text.as_str()))
                });
                let only = holders.next()?;
                if holders.next().is_some() {
                    return None;
                }
                only
            }
        };
        let new_line = self.new.line_text(line)?;
        if new_line == old_line {
            return Some(TextSpan {
                line,
                start: start as u32,
                end: end as u32,
            });
        }
        let g: Vec<&str> = new_line.graphemes(true).collect();
        // The same occurrence number; a reflowed line that holds the text
        // once takes it whatever the old count was.
        let s = nth_occurrence(&g, &text, occurrence).or_else(|| {
            (count_occurrences(new_line, &text) == 1).then(|| nth_occurrence(&g, &text, 0))?
        })?;
        Some(TextSpan {
            line,
            start: s as u32,
            end: (s + end - start) as u32,
        })
    }

    /// New place of a redact (t-117). Never by the text under the mask: that
    /// is exactly what changes when a key is rotated. A redact is anchored
    /// by the text AROUND it on its line — the text before it (what
    /// `:redact L:ANCHOR` typed) and, when it stops mid-line, the text after
    /// it — with the other masks of the line as wildcards too, so two
    /// secrets rotated on one line both stay masked. On the new line (found
    /// like any line, or the one line of its hunk that still has that
    /// text), the mask covers whatever sits between those texts now: a
    /// rotated value stays masked whatever its new length, and `keep=N`
    /// shows the tail of the NEW value.
    ///
    /// When that text cannot be followed it fails CLOSED (Luis, 2026-10-17:
    /// dropping a mask reveals the secret): if its line is still there the
    /// mask covers the rest of that line from its old start column (the
    /// whole line when that column is gone), and if the line cannot be told
    /// apart (a hunk that changed size) it covers every line of the hunk's
    /// new side. Only when its text disappeared altogether — deleted with
    /// nothing in its place, or replaced by a blank line — is it dropped.
    /// The caller says so loudly either way.
    pub fn redact(&mut self, span: &TextSpan) -> RedactFate {
        let Some(old_text) = self.old.line_text(span.line) else {
            return RedactFate::Dropped;
        };
        let og: Vec<&str> = old_text.graphemes(true).collect();
        let clamp = |s: &TextSpan| {
            let e = (s.end as usize).min(og.len());
            ((s.start as usize).min(e), e)
        };
        let (start, end) = clamp(span);
        // Every mask of this line (any step), merged where they touch; the
        // literal text between them anchors this one.
        let others: Vec<(usize, usize)> = self
            .old
            .annotations
            .iter()
            .filter(|a| a.file.is_none())
            .filter_map(|a| match &a.shape {
                Shape::Redact { span: s, .. } if s.line == span.line => Some(clamp(s)),
                _ => None,
            })
            .collect();
        let (literals, k) = tiza_core::redact::literals_around(&og, &others, start, end);
        let located = self.line(span.line);
        let new = self.new;
        let try_line = |l: u32| -> Option<TextSpan> {
            let (s, e) = *match_masks(new.line_text(l)?, &literals)?.get(k)?;
            (s < e).then_some(TextSpan {
                line: l,
                start: s,
                end: e,
            })
        };
        if start < end {
            if let Some(found) = located.and_then(try_line) {
                return RedactFate::Followed(found);
            }
            // The line moved and changed at once (a line added next to the
            // rotated one): the one line of its hunk that still has that text.
            let (_, hunk) = self.hunk(span.line);
            let mut hits = hunk.filter(|l| Some(*l) != located).filter_map(try_line);
            if let Some(only) = hits.next()
                && hits.next().is_none()
            {
                return RedactFate::Followed(only);
            }
        }
        // Fail closed. Its line is still there: the rest of it.
        if let Some(l) = located {
            return match rest_of_line(new, l, &og, start) {
                Some(sp) => RedactFate::Widened(vec![sp]),
                None => RedactFate::Dropped,
            };
        }
        // Which line it is cannot be told (the hunk changed size): every
        // line of the hunk's new side, whole. An empty new side is a pure
        // deletion: the text is gone, and only then is the redact dropped.
        let (_, hunk) = self.hunk(span.line);
        let lines: Vec<TextSpan> = hunk.filter_map(|l| whole_line(new, l)).collect();
        if lines.is_empty() {
            RedactFate::Dropped
        } else {
            RedactFate::Widened(lines)
        }
    }

    /// New place of a point anchor: the word under it identifies it.
    pub fn anchor(&mut self, a: &Anchor) -> Option<Anchor> {
        let word = word_span(self.old, a.line, a.col)?;
        let found = self.span(&word)?;
        Some(Anchor {
            line: found.line,
            col: found.start + a.col.saturating_sub(word.start),
        })
    }

    /// A line range (box, callout, spotlight — one rule for the three). Both
    /// edges in place: the range follows them (it grows with lines inserted
    /// inside). One edge gone or moved away with a block while the other
    /// stayed: the range keeps its height from the edge that stayed — it
    /// never stretches after the one that left. Both edges moved: the block
    /// moved as a whole and the range goes with it, same height. Both gone:
    /// `None`.
    pub fn range(&mut self, from: u32, to: u32) -> Option<(u32, u32)> {
        let height = to.saturating_sub(from);
        let last = self.new.last_line();
        let first = self.new.source.first_line;
        let down = |a: u32| (a, (a + height).min(last));
        let up = |b: u32| (b.saturating_sub(height).max(first), b);
        use Found::*;
        match (self.locate(from), self.locate(to)) {
            (Some((a, Moved)), Some((b, Moved))) => Some(down(a.min(b))),
            (Some((_, Moved)), Some((b, _))) => Some(up(b)),
            (Some((a, _)), Some((_, Moved))) => Some(down(a)),
            (Some((a, _)), Some((b, _))) => Some((a.min(b), a.max(b))),
            (Some((a, _)), None) => Some(down(a)),
            (None, Some((b, _))) => Some(up(b)),
            (None, None) => None,
        }
    }

    /// Re-anchors `shape`; `None` when the text it pointed at no longer exists.
    pub fn shape(&mut self, shape: &Shape) -> Option<Shape> {
        Some(match shape {
            Shape::Highlight {
                span,
                label,
                emphasis,
            } => Shape::Highlight {
                span: self.span(span)?,
                label: label.clone(),
                emphasis: *emphasis,
            },
            Shape::Mark {
                kind,
                span,
                label,
                style,
                emphasis,
            } => Shape::Mark {
                kind: kind.clone(),
                span: self.span(span)?,
                label: label.clone(),
                style: style.clone(),
                emphasis: *emphasis,
            },
            // Only a redact that followed its text; the widened and dropped
            // outcomes are the caller's (`redact`), which must say so.
            Shape::Redact {
                span,
                keep,
                label,
                widened,
            } => match self.redact(span) {
                RedactFate::Followed(span) => Shape::Redact {
                    span,
                    keep: *keep,
                    label: label.clone(),
                    widened: *widened,
                },
                RedactFate::Widened(_) | RedactFate::Dropped => return None,
            },
            Shape::Box {
                from_line,
                to_line,
                label,
                frame,
            } => {
                let (a, b) = self.range(*from_line, *to_line)?;
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
                line,
            } => {
                let (f, t) = (self.anchor(from)?, self.anchor(to)?);
                // Both ends landing on the same cell means one of them lost its
                // text and matched the other's: the arrow has nothing to point at.
                if f == t && from != to {
                    return None;
                }
                Shape::Arrow {
                    from: f,
                    to: t,
                    label: label.clone(),
                    line: *line,
                }
            }
            Shape::Note { line, text } => Shape::Note {
                line: self.line(*line)?,
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
                let (a, b) = match to_line {
                    Some(t) => {
                        let (a, b) = self.range(*from_line, *t)?;
                        (a, Some(b))
                    }
                    None => (self.line(*from_line)?, None),
                };
                Shape::Callout {
                    from_line: a,
                    to_line: b,
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
                let (a, b) = match (from_line, to_line) {
                    (Some(f), Some(t)) => {
                        let (a, b) = self.range(*f, *t)?;
                        (Some(a), Some(b))
                    }
                    (Some(f), None) => (Some(self.line(*f)?), None),
                    (None, Some(t)) => (None, Some(self.line(*t)?)),
                    (None, None) => (None, None),
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
}

/// What a reload or paste does to a redact (t-117, fail-closed); moves use
/// the same rules (t-140), so the type and its helpers live in core.
pub use tiza_core::redact::RedactFate;

/// The rest of line `l` from the old start column of a mask (see
/// [`tiza_core::redact::rest_of_line`]).
fn rest_of_line(new: &Document, l: u32, og: &[&str], start: usize) -> Option<TextSpan> {
    tiza_core::redact::rest_of_line(new.line_text(l)?, l, og, start)
}

/// Line `l` from its first non-blank character to its last one.
fn whole_line(new: &Document, l: u32) -> Option<TextSpan> {
    tiza_core::redact::whole_line(new.line_text(l)?, l)
}

use tiza_core::redact::match_masks;

/// Pairs `(i, j)` of equal lines, increasing in both, from an LCS of `a` and
/// `b` after trimming their common prefix and suffix. When the middle is too
/// large for the exact LCS, lines unique in both sides are matched instead
/// (patience-diff style).
fn line_diff(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf]
    {
        suf += 1;
    }
    let (ma, mb) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let mut out: Vec<(usize, usize)> = (0..pre).map(|i| (i, i)).collect();
    if !ma.is_empty() && !mb.is_empty() {
        let middle = if ma.len() * mb.len() <= LCS_BUDGET {
            lcs(ma, mb)
        } else {
            unique_pairs(ma, mb)
        };
        out.extend(middle.into_iter().map(|(i, j)| (i + pre, j + pre)));
    }
    out.extend((0..suf).rev().map(|k| (a.len() - 1 - k, b.len() - 1 - k)));
    slide_to_runs(a, b, &mut out);
    out
}

/// Among equal-length alignments the LCS picks one arbitrarily, so a `}`
/// of a block that moved away can get paired with the `}` of the block
/// that took its place, leaving that block's own brace unmatched. Prefer
/// contiguous runs: a pair that continues the previous one on one side but
/// skips lines on the other is moved to the line right after the previous
/// pair when that line has the same text — the alignment keeps its length
/// and the hunks become whole blocks (so a moved block is one deletion plus
/// one insertion, and its annotations travel with it).
fn slide_to_runs(a: &[&str], b: &[&str], pairs: &mut [(usize, usize)]) {
    for k in 1..pairs.len() {
        let (po, pn) = pairs[k - 1];
        let (o, n) = pairs[k];
        if n == pn + 1 && o > po + 1 && a[po + 1] == a[o] {
            pairs[k] = (po + 1, n);
        } else if o == po + 1 && n > pn + 1 && b[pn + 1] == b[n] {
            pairs[k] = (o, pn + 1);
        }
    }
}

/// Exact longest common subsequence (dynamic programming), as index pairs.
fn lcs(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[at(i, j)] = if a[i] == b[j] {
                dp[at(i + 1, j + 1)] + 1
            } else {
                dp[at(i + 1, j)].max(dp[at(i, j + 1)])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if dp[at(i + 1, j)] >= dp[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Lines unique in both slices, matched and kept in increasing order.
fn unique_pairs(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let mut count_a: HashMap<&str, (usize, usize)> = HashMap::new();
    for (i, l) in a.iter().enumerate() {
        let e = count_a.entry(l).or_insert((0, i));
        e.0 += 1;
    }
    let mut count_b: HashMap<&str, (usize, usize)> = HashMap::new();
    for (j, l) in b.iter().enumerate() {
        let e = count_b.entry(l).or_insert((0, j));
        e.0 += 1;
    }
    let mut pairs: Vec<(usize, usize)> = count_a
        .iter()
        .filter(|(l, (n, _))| *n == 1 && !l.is_empty())
        .filter_map(|(l, (_, i))| match count_b.get(l) {
            Some((1, j)) => Some((*i, *j)),
            _ => None,
        })
        .collect();
    pairs.sort_unstable();
    longest_increasing(&pairs)
}

/// Longest chain of pairs increasing in both coordinates (input sorted by the first).
fn longest_increasing(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; pairs.len()];
    for (i, &(_, n)) in pairs.iter().enumerate() {
        let pos = tails.partition_point(|&t| pairs[t].1 < n);
        if pos > 0 {
            prev[i] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
    }
    let mut out = Vec::new();
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        out.push(pairs[i]);
        cur = prev[i];
    }
    out.reverse();
    out
}

fn count_occurrences(hay: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    hay.matches(needle).count()
}

/// Grapheme index where the `n`-th (0-based) occurrence of `text` starts.
fn nth_occurrence(g: &[&str], text: &str, n: usize) -> Option<usize> {
    let tg: Vec<&str> = text.graphemes(true).collect();
    if tg.is_empty() || tg.len() > g.len() {
        return None;
    }
    let mut seen = 0;
    for i in 0..=g.len() - tg.len() {
        if g[i..i + tg.len()] == tg[..] {
            if seen == n {
                return Some(i);
            }
            seen += 1;
        }
    }
    None
}

/// The word (same character class) around grapheme `col` of `line`.
pub fn word_span(doc: &Document, line: u32, col: u32) -> Option<TextSpan> {
    let text = doc.line_text(line)?;
    let gs: Vec<&str> = text.graphemes(true).collect();
    if gs.is_empty() {
        return None;
    }
    let i = (col as usize).min(gs.len() - 1);
    let cls = crate::app::App::word_class(gs[i]);
    let mut a = i;
    while a > 0 && crate::app::App::word_class(gs[a - 1]) == cls {
        a -= 1;
    }
    let mut b = i;
    while b + 1 < gs.len() && crate::app::App::word_class(gs[b + 1]) == cls {
        b += 1;
    }
    Some(TextSpan {
        line,
        start: a as u32,
        end: b as u32 + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(code: &str) -> Document {
        Document::from_code(code, "rs", None)
    }

    #[test]
    fn repeated_braces_keep_their_own_copy() {
        let old = doc("fn a() {\n    x();\n}\nfn b() {\n    y();\n}\nfn c() {\n    z();\n}\n");
        let new = doc(
            "// 1\n// 2\n// 3\n// 4\nfn a() {\n    x();\n}\nfn b() {\n    y();\n}\nfn c() {\n    z();\n}\n",
        );
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.line(3), Some(7));
        assert_eq!(r.line(6), Some(10));
        assert_eq!(r.line(9), Some(13));
    }

    #[test]
    fn deleted_copy_is_dropped_not_moved_to_another_copy() {
        let old = doc("}\n// a\n// b\n// c\n}\n// d\n");
        let new = doc("}\n// a\n// b\n// c\n// d\n");
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.line(5), None, "the first brace is another place");
        assert_eq!(r.line(1), Some(1));
        assert_eq!(r.line(6), Some(5));
    }

    #[test]
    fn moved_block_follows_when_unique_and_near() {
        let old = doc("fn a() {}\nfn b() {}\nfn c() {}\n");
        let new = doc("fn b() {}\nfn c() {}\nfn a() {}\n");
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.line(1), Some(3));
        assert_eq!(r.line(2), Some(1));
        let far: String = (0..60).map(|i| format!("// {i}\n")).collect();
        let old = doc(&format!("fn a() {{}}\n{far}"));
        let new = doc(&format!("{far}fn a() {{}}\n"));
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.line(1), None, "beyond MAX_DISTANCE");
    }

    #[test]
    fn same_occurrence_inside_a_changed_line() {
        let old = doc("    takes(s, s.len());\n");
        let new = doc("    takes(s.clone(), s.len());\n");
        let mut r = Remap::new(&old, &new);
        let sp = TextSpan {
            line: 1,
            start: 13,
            end: 14,
        };
        assert_eq!(
            r.span(&sp),
            Some(TextSpan {
                line: 1,
                start: 21,
                end: 22
            })
        );
        let third = doc("    takes(s);\n");
        let mut r = Remap::new(&old, &third);
        assert_eq!(r.span(&sp), None, "the occurrence is gone: dropped");
    }

    #[test]
    fn adjacent_identical_lines_one_deleted() {
        // `}` `}` → `}`: the annotation on the second brace stays on the brace left.
        let old = doc("fn f() {\n    fn g() {\n    }\n}\n\nx\n");
        let new = doc("fn f() {\n    fn g() {\n}\n\nx\n");
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.line(4), Some(3));
        assert_eq!(r.line(3), Some(3));
    }

    #[test]
    fn replaced_line_keeps_its_place_but_a_span_needs_its_text() {
        let old = doc("struct S {\n    items: Vec<T>,\n}\n");
        let new = doc("struct S {\n    items: VecDeque<T>,\n}\n");
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.locate(2), Some((2, Found::InPlace)));
        let span = TextSpan {
            line: 2,
            start: 11,
            end: 17,
        }; // `Vec<T>`
        assert_eq!(r.span(&span), None, "its text is gone");
        let items = TextSpan {
            line: 2,
            start: 4,
            end: 9,
        }; // `items`
        assert_eq!(r.span(&items).map(|s| (s.line, s.start)), Some((2, 4)));
        // Completely different text: a note still has its line.
        let new2 = doc("struct S {\n    return;\n}\n");
        let mut r = Remap::new(&old, &new2);
        assert_eq!(r.locate(2), Some((2, Found::InPlace)));
    }

    #[test]
    fn n_to_n_hunk_pairs_its_lines_in_place() {
        // A parameter renamed on the signature line and on the line that
        // uses it: a 2↔2 hunk, paired i↔i like a 1↔1 edit.
        let old = doc(
            "impl S {\n    fn push(&mut self, item: T) {\n        self.items.push(item);\n    }\n}\n",
        );
        let new = doc(
            "impl S {\n    fn push(&mut self, value: T) {\n        self.items.push(value);\n    }\n}\n",
        );
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.locate(2), Some((2, Found::InPlace)));
        assert_eq!(r.locate(3), Some((3, Found::InPlace)));
        let push = TextSpan {
            line: 3,
            start: 19,
            end: 23,
        }; // `push` on the call line
        assert_eq!(r.span(&push).map(|s| (s.line, s.start)), Some((3, 19)));
        assert_eq!(r.range(2, 4), Some((2, 4)));
        // Lines 2, 3 and 4 edited (3↔3): the same.
        let new3 = doc(
            "impl S {\n    fn push(&mut self, value: T) {\n        self.items.push(value);\n    } // end\n}\n",
        );
        let mut r = Remap::new(&old, &new3);
        assert_eq!(r.span(&push).map(|s| (s.line, s.start)), Some((3, 19)));
        assert_eq!(r.range(2, 4), Some((2, 4)));
        // Its text gone from the paired line: dropped, not moved elsewhere.
        let new4 = doc(
            "impl S {\n    fn put(&mut self, value: T) {\n        self.items.insert(0, value);\n    }\n}\n",
        );
        let mut r = Remap::new(&old, &new4);
        assert_eq!(r.span(&push), None);
    }

    #[test]
    fn formatter_split_and_join_follow_the_span() {
        let old = doc("fn new() -> Self {\n    Self { items: Vec::new() }\n}\n");
        let new = doc("fn new() -> Self {\n    Self {\n        items: Vec::new(),\n    }\n}\n");
        let span = TextSpan {
            line: 2,
            start: 18,
            end: 26,
        }; // `Vec::new`
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.line(2), None, "1→3 hunk: no single line to stand for it");
        assert_eq!(
            r.span(&span).map(|s| (s.line, s.start, s.end)),
            Some((3, 15, 23))
        );
        // And back: 3→1.
        let mut r = Remap::new(&new, &old);
        let back = TextSpan {
            line: 3,
            start: 15,
            end: 23,
        };
        assert_eq!(
            r.span(&back).map(|s| (s.line, s.start, s.end)),
            Some((2, 18, 26))
        );
        // The text in two of the new lines: ambiguous, dropped.
        let two = doc(
            "fn new() -> Self {\n    Self {\n        a: Vec::new(),\n        b: Vec::new(),\n    }\n}\n",
        );
        let mut r = Remap::new(&old, &two);
        assert_eq!(r.span(&span), None);
    }

    #[test]
    fn ranges_keep_their_height_when_an_edge_moves_away() {
        // The block holding the bottom edge (unique `pub fn peek`) moves below.
        let old = doc("a\n}\n\npub fn peek() {\n    x\n}\n\npub fn len() {\n    y\n}\n");
        let new = doc("a\n}\n\npub fn len() {\n    y\n}\n\npub fn peek() {\n    x\n}\n");
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.locate(4), Some((8, Found::Moved)));
        assert_eq!(
            r.range(1, 4),
            Some((1, 4)),
            "the top edge stayed: same height, no stretch"
        );
        // Only the moved block: the range travels with it.
        assert_eq!(r.range(4, 5), Some((8, 9)));
        // A repeated `}` next to a matched `}` is not a collapse when its hunk
        // holds other lines: the peek block moved, its brace went with it.
        assert_eq!(r.locate(6), None);
        assert_eq!(r.range(4, 6), Some((8, 10)));
    }

    #[test]
    fn a_moved_block_is_one_deletion_plus_one_insertion() {
        // peek (with its closing brace) is cut and pasted above push. The LCS
        // alone may pair peek's old `}` with pop's new `}`; the run heuristic
        // keeps pop's brace with pop, so the callout on peek travels with it.
        let old = doc(
            "fn push() {\n    a\n}\n\nfn pop() {\n    b\n}\n\nfn peek() {\n    c\n}\n\nfn len() {\n}\n",
        );
        let new = doc(
            "fn peek() {\n    c\n}\n\nfn push() {\n    a\n}\n\nfn pop() {\n    b\n}\n\nfn len() {\n}\n",
        );
        let mut r = Remap::new(&old, &new);
        assert_eq!(
            r.locate(7),
            Some((11, Found::Matched)),
            "pop's brace stays pop's"
        );
        assert_eq!(r.locate(9), Some((1, Found::Moved)));
        assert_eq!(r.locate(11), None, "peek's brace went with its block");
        assert_eq!(r.range(9, 11), Some((1, 3)));
    }

    #[test]
    fn big_files_with_a_small_edit_stay_exact() {
        let old: String = (0..3000).map(|i| format!("line {i}\n")).collect();
        let mut lines: Vec<String> = old.lines().map(String::from).collect();
        lines.insert(1500, "// new".into());
        lines[2000] = "// edited".into();
        let new = lines.join("\n") + "\n";
        let (old, new) = (doc(&old), doc(&new));
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.line(1000), Some(1000));
        assert_eq!(r.line(1501), Some(1502));
        assert_eq!(
            r.locate(2000),
            Some((2001, Found::InPlace)),
            "edited in place (1↔1 hunk): the line keeps its position"
        );
        assert_eq!(r.line(2001), Some(2002));
        assert_eq!(r.line(2999), Some(3000));
        // Fully reversed (over the LCS budget) must not panic.
        let rev: String = (0..3000).rev().map(|i| format!("line {i}\n")).collect();
        let rev = doc(&rev);
        let mut r = Remap::new(&old, &rev);
        let _ = r.line(1500);
    }

    fn with_redacts(code: &str, spans: &[(u32, u32, u32)]) -> Document {
        let mut d = Document::from_code(code, "txt", None);
        for (i, (line, start, end)) in spans.iter().enumerate() {
            d.annotations.push(tiza_core::model::Annotation {
                id: format!("a{}", i + 1),
                step: 1,
                color: None,
                markers: None,
                file: None,
                shape: Shape::Redact {
                    span: TextSpan {
                        line: *line,
                        start: *start,
                        end: *end,
                    },
                    keep: 4,
                    label: None,
                    widened: false,
                },
            });
        }
        d
    }

    fn masked_text(d: &Document, sp: &TextSpan) -> String {
        let g: Vec<&str> = d.line_text(sp.line).unwrap().graphemes(true).collect();
        g[sp.start as usize..sp.end as usize].concat()
    }

    fn followed(f: RedactFate) -> TextSpan {
        match f {
            RedactFate::Followed(sp) => sp,
            other => panic!("expected Followed, got {other:?}"),
        }
    }

    /// The one span a fail-closed redact widened to.
    fn widened_one(f: RedactFate) -> TextSpan {
        match f {
            RedactFate::Widened(v) if v.len() == 1 => v[0],
            other => panic!("expected Widened(one line), got {other:?}"),
        }
    }

    #[test]
    fn rotated_value_stays_masked_by_the_text_before_it() {
        // `:redact 2:STRIPE_KEY=` → columns 11..end of line.
        let old = with_redacts(
            "# keys\nSTRIPE_KEY=sk_live_OLDVALUEp7dc\nNAME=demo\n",
            &[(2, 11, 31)],
        );
        let sp = TextSpan {
            line: 2,
            start: 11,
            end: 31,
        };
        // Same key, a longer new value; plus a line added above.
        let new = doc("# keys\n# rotated\nSTRIPE_KEY=sk_live_NEWVALUEzzzz9999extra\nNAME=demo\n");
        let mut r = Remap::new(&old, &new);
        let got = followed(r.redact(&sp));
        assert_eq!(got.line, 3);
        assert_eq!(masked_text(&new, &got), "sk_live_NEWVALUEzzzz9999extra");
        // A shorter one, indented by a formatter.
        let new = doc("# keys\n  STRIPE_KEY=x1\nNAME=demo\n");
        let mut r = Remap::new(&old, &new);
        let got = followed(r.redact(&sp));
        assert_eq!(masked_text(&new, &got), "x1");
    }

    #[test]
    fn a_prefix_that_is_gone_widens_to_the_rest_of_the_line() {
        let old = with_redacts(
            "# keys\nSTRIPE_KEY=sk_live_OLDVALUEp7dc\nNAME=demo\n",
            &[(2, 11, 31)],
        );
        let sp = TextSpan {
            line: 2,
            start: 11,
            end: 31,
        };
        // The key renamed (the line replaced in place): never dropped, the
        // mask covers the line from its old column on.
        let new = doc("# keys\nSTRIPE=sk_live_NEWVALUE\nNAME=demo\n");
        let mut r = Remap::new(&old, &new);
        let got = widened_one(r.redact(&sp));
        // From where the line first differs (`STRIPE` is unchanged).
        assert_eq!((got.line, got.start, got.end), (2, 6, 23));
        assert_eq!(masked_text(&new, &got), "=sk_live_NEWVALUE");
        // Shorter than the old column: the whole line.
        let new = doc("# keys\nK=v-new\nNAME=demo\n");
        let mut r = Remap::new(&old, &new);
        let got = widened_one(r.redact(&sp));
        assert_eq!(masked_text(&new, &got), "K=v-new");
        // The value emptied: the column is gone, the whole line is masked
        // (the next save may put a new value there).
        let new = doc("# keys\nSTRIPE_KEY=\nNAME=demo\n");
        let mut r = Remap::new(&old, &new);
        let got = widened_one(r.redact(&sp));
        assert_eq!(masked_text(&new, &got), "STRIPE_KEY=");
        // De-indented with the prefix intact: never one column of the value.
        let old2 = with_redacts("    TOKEN=old-one\n", &[(1, 10, 17)]);
        let new = doc("TOKEN=\n");
        let mut r = Remap::new(&old2, &new);
        let got = widened_one(r.redact(&TextSpan {
            line: 1,
            start: 10,
            end: 17,
        }));
        assert_eq!(masked_text(&new, &got), "TOKEN=");
    }

    #[test]
    fn a_line_deleted_with_nothing_in_its_place_is_dropped() {
        let old = with_redacts("a\nTOKEN=old-secret-1\nb\n", &[(2, 6, 18)]);
        let sp = TextSpan {
            line: 2,
            start: 6,
            end: 18,
        };
        let new = doc("a\nb\n");
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.redact(&sp), RedactFate::Dropped);
        // Replaced by a blank line: nothing left to hide, dropped too.
        let new = doc("a\n\nb\n");
        let mut r = Remap::new(&old, &new);
        assert_eq!(r.redact(&sp), RedactFate::Dropped);
    }

    #[test]
    fn a_hunk_that_changed_size_widens_to_all_its_new_lines() {
        let old = with_redacts("a\nTOKEN=old-secret-1\nb\n", &[(2, 6, 18)]);
        let sp = TextSpan {
            line: 2,
            start: 6,
            end: 18,
        };
        // One line became three, none of them with `TOKEN=`: which one holds
        // the value cannot be told, so all three are masked, whole.
        let new = doc("a\nAUTH_TOKEN = \"n3w-s3cr3t\"\n  # rotated\nREGION=eu\nb\n");
        let mut r = Remap::new(&old, &new);
        match r.redact(&sp) {
            RedactFate::Widened(v) => {
                let got: Vec<(u32, String)> =
                    v.iter().map(|s| (s.line, masked_text(&new, s))).collect();
                assert_eq!(
                    got,
                    vec![
                        (2, "AUTH_TOKEN = \"n3w-s3cr3t\"".to_string()),
                        (3, "# rotated".to_string()),
                        (4, "REGION=eu".to_string()),
                    ]
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rotated_line_that_moved_is_found_in_its_hunk() {
        let old = with_redacts("a\nTOKEN=old-secret-1\nb\n", &[(2, 6, 18)]);
        // A line inserted right after the rotated one: a 1→2 hunk.
        let new = doc("a\nTOKEN=brand-new-secret\nEXTRA=1\nb\n");
        let mut r = Remap::new(&old, &new);
        let got = followed(r.redact(&TextSpan {
            line: 2,
            start: 6,
            end: 18,
        }));
        assert_eq!(
            (got.line, masked_text(&new, &got).as_str()),
            (2, "brand-new-secret")
        );
    }

    #[test]
    fn mid_line_redact_follows_both_sides_or_widens() {
        // `{"token": "abc123", "user": "ana"}` with the value selected.
        let code = "{\"token\": \"abc123\", \"user\": \"ana\"}\n";
        let sp = TextSpan {
            line: 1,
            start: 11,
            end: 17,
        };
        let old = with_redacts(code, &[(1, 11, 17)]);
        let new = doc("{\"token\": \"zz-much-longer-99\", \"user\": \"ana\"}\n");
        let mut r = Remap::new(&old, &new);
        let got = followed(r.redact(&sp));
        assert_eq!(masked_text(&new, &got), "zz-much-longer-99");
        // One side changed: the rest of the line from the value on.
        let new = doc("{\"token\": \"zz-much-longer-99\", \"usr\": 1}\n");
        let mut r = Remap::new(&old, &new);
        let got = widened_one(r.redact(&sp));
        assert_eq!(masked_text(&new, &got), "zz-much-longer-99\", \"usr\": 1}");
        // Both sides changed: from where the line first differs (`{"tok`),
        // not from the old column — that would show `zz`.
        let new = doc("{\"tok\": \"zz-much-longer-99\", \"usr\": 1}\n");
        let mut r = Remap::new(&old, &new);
        let got = widened_one(r.redact(&sp));
        assert_eq!(
            masked_text(&new, &got),
            "\": \"zz-much-longer-99\", \"usr\": 1}"
        );
        assert!(!new.code[0][..got.start as usize].contains("zz"));
        // ... and shorter than the old column: from where it differs.
        let new = doc("{}\n");
        let mut r = Remap::new(&old, &new);
        let got = widened_one(r.redact(&sp));
        assert_eq!(masked_text(&new, &got), "}");
    }

    #[test]
    fn two_masks_on_one_line_both_rotate() {
        // `postgres://admin:PASS@host/db?key=KEY` with both values masked.
        let code = "URL=postgres://admin:hunter2@db/app?key=k-111\n";
        let old = with_redacts(code, &[(1, 21, 28), (1, 40, 45)]);
        let new = doc("URL=postgres://admin:s3cr3t-new@db/app?key=k-222-longer\n");
        let mut r = Remap::new(&old, &new);
        let p = followed(r.redact(&TextSpan {
            line: 1,
            start: 21,
            end: 28,
        }));
        let k = followed(r.redact(&TextSpan {
            line: 1,
            start: 40,
            end: 45,
        }));
        assert_eq!(masked_text(&new, &p), "s3cr3t-new");
        assert_eq!(masked_text(&new, &k), "k-222-longer");
    }

    #[test]
    fn lcs_pairs_are_increasing() {
        let a = ["x", "}", "y", "}", "}"];
        let b = ["}", "x", "}", "}", "z", "}"];
        let p = line_diff(&a, &b);
        for w in p.windows(2) {
            assert!(w[0].0 < w[1].0 && w[0].1 < w[1].1, "{p:?}");
        }
        assert!(p.len() >= 4, "{p:?}");
    }
}
