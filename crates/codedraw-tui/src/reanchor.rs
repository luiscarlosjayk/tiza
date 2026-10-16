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

use std::collections::HashMap;

use codedraw_core::Document;
use codedraw_core::model::{Anchor, Shape, TextSpan};
use unicode_segmentation::UnicodeSegmentation;

/// Farthest a line may move from its expected position and still be "the same line".
pub const MAX_DISTANCE: u32 = 50;

/// Largest `old × new` middle (after trimming the common prefix and suffix)
/// the exact LCS is computed for; bigger edits fall back to unique-line anchors.
const LCS_BUDGET: usize = 4_000_000;

/// Outcome of re-anchoring a document's annotations.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub kept: usize,
    pub dropped: usize,
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

    /// New number of `old_line`, or `None` when that line is gone.
    pub fn line(&mut self, old_line: u32) -> Option<u32> {
        if let Ok(i) = self.matched.binary_search_by_key(&old_line, |(o, _)| *o) {
            return Some(self.matched[i].1);
        }
        let text = self.old.line_text(old_line)?.trim();
        let expected = self.expected(old_line);
        if text.is_empty() {
            // A blank line follows its neighbours; it must still be blank.
            return self
                .new
                .line_text(expected)
                .is_some_and(|l| l.trim().is_empty())
                .then_some(expected);
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
            return Some(*only);
        }
        // Repeated text (one `}` among many): the only survivor is a line that
        // collapsed with an identical neighbour (`}` `}` → `}`); both keep
        // their annotations on the line left. Otherwise this copy was deleted
        // and the annotation goes with it rather than jumping to another copy.
        [old_line.checked_sub(1), Some(old_line + 1)]
            .into_iter()
            .flatten()
            .filter(|&l| self.old.line_text(l).map(str::trim) == Some(text))
            .find_map(|l| {
                self.matched
                    .binary_search_by_key(&l, |(o, _)| *o)
                    .ok()
                    .map(|i| self.matched[i].1)
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
        // The line by its text; failing that, the line edited *in place*
        // (same expected position) as long as it still holds the occurrence.
        let line = match self.line(span.line) {
            Some(l) => l,
            None => self.expected(span.line),
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
        let s = nth_occurrence(&g, &text, occurrence)?;
        Some(TextSpan {
            line,
            start: s as u32,
            end: (s + end - start) as u32,
        })
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

    /// A line range: both edges by text when possible; when only one edge
    /// survives (the presenter edited the other line), the range keeps its
    /// height from the surviving edge, clamped to the code.
    pub fn range(&mut self, from: u32, to: u32) -> Option<(u32, u32)> {
        let height = to.saturating_sub(from);
        let last = self.new.last_line();
        let first = self.new.source.first_line;
        match (self.line(from), self.line(to)) {
            (Some(a), Some(b)) => Some((a.min(b), a.max(b))),
            (Some(a), None) => Some((a, (a + height).min(last))),
            (None, Some(b)) => Some((b.saturating_sub(height).max(first), b)),
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
                let a = match from_line {
                    Some(f) => Some(self.line(*f)?),
                    None => None,
                };
                let b = match to_line {
                    Some(t) => Some(self.line(*t)?),
                    None => None,
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
    out
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
        assert_eq!(r.line(2000), None, "edited in place: whole-line kinds drop");
        assert_eq!(r.line(2001), Some(2002));
        assert_eq!(r.line(2999), Some(3000));
        // Fully reversed (over the LCS budget) must not panic.
        let rev: String = (0..3000).rev().map(|i| format!("line {i}\n")).collect();
        let rev = doc(&rev);
        let mut r = Remap::new(&old, &rev);
        let _ = r.line(1500);
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
