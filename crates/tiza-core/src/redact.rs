//! Redaction (t-117): a `Shape::Redact` masks a span of code on every
//! surface at once — the TUI, every export, the recordings, and what agents
//! read — because Tiza owns the buffer it shows. The mask is applied in one
//! place, [`masked`], at the boundary every output path goes through: the
//! layout (screen, txt/md/ansi/svg/png, `.cast`/`.tape`, `render.text`) and
//! the code readers (`code.get`, `scene.get`, the `tiza://code` resource,
//! the embedded `code` a save writes). The file on disk never changes.
//!
//! A mask is eight `•` plus the last `keep` characters of the span: never
//! the real text, never its length. Other annotations on the same line are
//! shifted by the difference, and anything overlapping the masked span is
//! clamped to it (the redact wins: nothing under it is readable).

use std::sync::OnceLock;

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

use crate::model::{Anchor, Annotation, Document, Shape, TextSpan};

/// What a masked span shows (eight bullets, whatever the span's length).
pub const MASK: &str = "••••••••";
/// Cells the mask takes (before the kept tail).
pub const MASK_WIDTH: u32 = 8;

/// Whether `a` masks the code at `step`: from step 0 by default, or from
/// its own step when the document says `redact_from_step`.
pub fn active(doc: &Document, a: &Annotation, step: u32) -> bool {
    matches!(a.shape, Shape::Redact { .. }) && (!doc.redact_from_step || a.step <= step)
}

/// Characters a redact leaves visible after its bullets: its `keep`, or
/// none once it was widened (it no longer knows where the value ends, so a
/// tail could be part of a new secret).
pub fn shown_tail(shape: &Shape) -> u8 {
    match shape {
        Shape::Redact { widened: true, .. } => 0,
        Shape::Redact { keep, .. } => *keep,
        _ => 0,
    }
}

/// Whether the document has any redact annotation.
pub fn has_redacts(doc: &Document) -> bool {
    doc.annotations
        .iter()
        .any(|a| matches!(a.shape, Shape::Redact { .. }))
}

/// The document with every active mask applied: masked code lines, the
/// redact spans narrowed to the mask, and the other annotations of those
/// lines re-mapped. A document without redacts comes back unchanged.
///
/// Masks that overlap on a line (two redacts widened by a move, t-140) are
/// drawn as one. A span that runs past the end of its line (a line being
/// typed, t-140) shows no kept tail: the tail of a partial value is the
/// start of the secret.
pub fn masked(doc: &Document, step: u32) -> Document {
    let mut out = doc.clone();
    if !has_redacts(doc) {
        return out;
    }
    let mut by_line: std::collections::BTreeMap<u32, Vec<(usize, TextSpan, u8)>> =
        Default::default();
    for (i, a) in doc.annotations.iter().enumerate() {
        if let Shape::Redact { span, .. } = &a.shape
            && active(doc, a, step)
        {
            by_line
                .entry(span.line)
                .or_default()
                .push((i, *span, shown_tail(&a.shape)));
        }
    }
    for (line, masks) in by_line.iter() {
        let Some(idx) = line
            .checked_sub(doc.source.first_line)
            .map(|i| i as usize)
            .filter(|i| *i < doc.code.len())
        else {
            continue;
        };
        let len = out.code[idx].graphemes(true).count();
        let spans: Vec<(usize, usize, u8, usize)> = masks
            .iter()
            .map(|(i, sp, keep)| {
                let end = (sp.end as usize).min(len);
                let keep = if (sp.end as usize) > len { 0 } else { *keep };
                ((sp.start as usize).min(end), end, keep, *i)
            })
            .collect();
        // Later groups first so the earlier columns stay valid while the
        // text is rewritten.
        for group in merge_groups(spans).into_iter().rev() {
            let text = out.code[idx].clone();
            let g: Vec<&str> = text.graphemes(true).collect();
            let (start, end, keep) = (group.start, group.end, group.keep);
            let keep = (keep as usize).min(end - start).min(8);
            let tail: String = g[end - keep..end].concat();
            let new_len = MASK_WIDTH + keep as u32;
            out.code[idx] = format!("{}{MASK}{tail}{}", g[..start].concat(), g[end..].concat());
            let delta = new_len as i64 - (end - start) as i64;
            let (s0, s1) = (start as u32, end as u32);
            // The redacts of the group now cover the mask.
            for &m in &group.members {
                if let Shape::Redact { span: sp, .. } = &mut out.annotations[m].shape {
                    sp.start = s0;
                    sp.end = s0 + new_len;
                }
            }
            for (j, a) in out.annotations.iter_mut().enumerate() {
                if group.members.contains(&j) {
                    continue;
                }
                remap_shape(&mut a.shape, *line, s0, s1, delta);
            }
        }
    }
    out
}

/// Masks of one line merged where they overlap: the columns, the kept tail
/// (none when several masks were merged) and the indexes they came from.
struct MaskGroup {
    start: usize,
    end: usize,
    keep: u8,
    members: Vec<usize>,
}

fn merge_groups(mut spans: Vec<(usize, usize, u8, usize)>) -> Vec<MaskGroup> {
    spans.retain(|(s, e, _, _)| s < e);
    spans.sort_by_key(|(s, e, _, _)| (*s, *e));
    let mut out: Vec<MaskGroup> = Vec::new();
    for (s, e, keep, i) in spans {
        match out.last_mut() {
            Some(last) if s < last.end => {
                last.end = last.end.max(e);
                last.keep = 0;
                last.members.push(i);
            }
            _ => out.push(MaskGroup {
                start: s,
                end: e,
                keep,
                members: vec![i],
            }),
        }
    }
    out
}

/// One mask to draw on plain code lines: its line, grapheme columns and
/// kept tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskSpan {
    pub line: u32,
    pub start: u32,
    pub end: u32,
    pub keep: u8,
}

/// `code` with `masks` drawn on it (merged where they overlap; no kept
/// tail on a span that runs past its line). Only the lines change.
pub fn mask_with(code: &[String], first_line: u32, masks: &[MaskSpan]) -> Vec<String> {
    let mut out = code.to_vec();
    let mut by_line: std::collections::BTreeMap<u32, Vec<MaskSpan>> = Default::default();
    for m in masks {
        by_line.entry(m.line).or_default().push(*m);
    }
    for (line, ms) in by_line {
        let Some(idx) = line
            .checked_sub(first_line)
            .map(|i| i as usize)
            .filter(|i| *i < out.len())
        else {
            continue;
        };
        let g: Vec<String> = out[idx].graphemes(true).map(str::to_string).collect();
        let len = g.len();
        let spans: Vec<(usize, usize, u8, usize)> = ms
            .iter()
            .map(|m| {
                let end = (m.end as usize).min(len);
                let keep = if (m.end as usize) > len { 0 } else { m.keep };
                ((m.start as usize).min(end), end, keep, 0)
            })
            .collect();
        let mut text = String::new();
        let mut prev = 0;
        for group in merge_groups(spans) {
            let keep = (group.keep as usize).min(group.end - group.start).min(8);
            text.push_str(&g[prev..group.start].concat());
            text.push_str(MASK);
            text.push_str(&g[group.end - keep..group.end].concat());
            prev = group.end;
        }
        text.push_str(&g[prev..].concat());
        out[idx] = text;
    }
    out
}

/// Masks `code` with every redact in `annotations` (any step), without
/// touching annotations: for text matching that must not see a secret,
/// where only the lines matter. The spans are taken as they are stored, so
/// on a document with moves pass only the redacts of that code's step.
pub fn mask_lines(code: &[String], first_line: u32, annotations: &[Annotation]) -> Vec<String> {
    let masks: Vec<MaskSpan> = annotations
        .iter()
        .filter_map(|a| match &a.shape {
            Shape::Redact { span, .. } => Some(MaskSpan {
                line: span.line,
                start: span.start,
                end: span.end,
                keep: shown_tail(&a.shape),
            }),
            _ => None,
        })
        .collect();
    mask_with(code, first_line, &masks)
}

/// Whether `[start, end)` on `line` touches a redacted span (any step): a
/// text search must never confirm or deny what is under a mask.
pub fn overlaps_mask(annotations: &[Annotation], line: u32, start: u32, end: u32) -> bool {
    annotations.iter().any(|a| {
        matches!(&a.shape, Shape::Redact { span, .. } if span.line == line && start < span.end && end > span.start)
    })
}

/// Only the masked code lines at `step` (what `code.get` and the
/// `tiza://code` resource return).
pub fn masked_code(doc: &Document, step: u32) -> Vec<String> {
    if !has_redacts(doc) {
        return doc.code.clone();
    }
    masked(doc, step).code
}

/// The step-0 code with every redact applied (what a save embeds, what
/// `:embed` freezes), so a sidecar never carries a secret. On a document
/// with moves only the redacts already there at step 0 are drawn (one
/// marked on a line a move writes lives in that move's step, t-140).
pub fn masked_base(doc: &Document) -> Vec<String> {
    if !has_redacts(doc) {
        return doc.base().to_vec();
    }
    if doc.moves.is_empty() {
        return masked(doc, u32::MAX).code;
    }
    let mut d = doc.clone();
    d.code = doc.base().to_vec();
    d.source.first_line = doc.base_first();
    d.annotations = base_redacts(doc, &None);
    d.moves.clear();
    d.base_code.clear();
    masked(&d, u32::MAX).code
}

/// The redacts of view `key` whose span is on that view's step-0 code: all
/// of them without moves; with moves, those of step 0 (and, with
/// `redact_from_step`, every one: their spans were marked at their own
/// step, so masking them on the step-0 code can only mask more).
pub fn base_redacts(doc: &Document, key: &Option<String>) -> Vec<Annotation> {
    doc.annotations
        .iter()
        .filter(|a| &a.file == key && matches!(a.shape, Shape::Redact { .. }))
        .filter(|a| doc.moves.is_empty() || a.step == 0 || doc.redact_from_step)
        .cloned()
        .collect()
}

/// What a reload, a paste or a move does to a redact (t-117, t-140): it
/// fails CLOSED.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedactFate {
    /// It followed the text around it: the mask covers the value there now.
    Followed(TextSpan),
    /// It could not follow its text but its place is still there: the mask
    /// covers the rest of its line, or (its line cannot be told apart) every
    /// line of its hunk, whole. Never empty.
    Widened(Vec<TextSpan>),
    /// Its text is gone altogether: deleted with nothing in its place.
    Dropped,
}

/// The literal text around mask `[start, end)` of an old line whose other
/// masks are `others` (grapheme columns): before the first mask, between
/// each pair, after the last — and which mask is this one. Masks that touch
/// are merged.
pub fn literals_around(
    og: &[&str],
    others: &[(usize, usize)],
    start: usize,
    end: usize,
) -> (Vec<String>, usize) {
    let mut spans: Vec<(usize, usize)> = others
        .iter()
        .map(|(s, e)| {
            let e = (*e).min(og.len());
            ((*s).min(e), e)
        })
        .filter(|(s, e)| s < e)
        .collect();
    spans.push((start, end));
    spans.sort_unstable();
    let mut masks: Vec<(usize, usize)> = Vec::new();
    for (s, e) in spans {
        match masks.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => masks.push((s, e)),
        }
    }
    let k = masks
        .iter()
        .position(|(s, e)| *s <= start && end <= *e)
        .unwrap_or(0);
    let mut literals: Vec<String> = Vec::with_capacity(masks.len() + 1);
    let mut prev = 0;
    for (s, e) in &masks {
        literals.push(og[prev..*s].concat());
        prev = *e;
    }
    literals.push(og[prev..].concat());
    (literals, k)
}

/// The grapheme spans of the masks on `line`, given the literal text around
/// them on the old line (`literals[0]` before the first mask, `literals[i]`
/// between masks `i-1` and `i`, the last one after the last mask). Leading
/// indentation and trailing blanks are not part of the anchor; between two
/// masks the first occurrence of the text that separated them is taken.
/// `None` when the line no longer has that text around the masks.
pub fn match_masks(line: &str, literals: &[String]) -> Option<Vec<(u32, u32)>> {
    let n = literals.len().checked_sub(1)?;
    let lead = line.len() - line.trim_start().len();
    let first = literals[0].trim_start();
    if !line[lead..].starts_with(first) {
        return None;
    }
    let mut pos = lead + first.len();
    let body_end = line.trim_end().len().max(pos);
    let tail = literals[n].trim_end();
    if !line[..body_end].ends_with(tail) || body_end - tail.len() < pos {
        return None;
    }
    let end_all = body_end - tail.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let s = pos;
        let e = if i + 1 == n {
            end_all
        } else {
            let lit = literals[i + 1].as_str();
            let off = line[pos..end_all].find(lit)?;
            pos += off + lit.len();
            s + off
        };
        out.push((
            crate::model::grapheme_index(line, s),
            crate::model::grapheme_index(line, e),
        ));
    }
    Some(out)
}

/// The rest of line `l` (text `text`) from the old start column of a mask
/// to the end of the line — from earlier when the line shifted, so it never
/// uncovers the start of a value: when the text before the mask is still
/// there (after the indentation) the mask starts where that text ends now,
/// and when it is not, where the new line first differs from the old one
/// (`og`, nothing before that point moved). The whole line (from its first
/// non-blank character) when that column no longer exists; `None` for a
/// blank line (nothing left to hide).
pub fn rest_of_line(text: &str, l: u32, og: &[&str], start: usize) -> Option<TextSpan> {
    let ng: Vec<&str> = text.graphemes(true).collect();
    let indent = |g: &[&str]| g.iter().take_while(|c| c.trim().is_empty()).count();
    let (old_lead, new_lead) = (indent(og), indent(&ng));
    let len = ng.len() - ng.iter().rev().take_while(|c| c.trim().is_empty()).count();
    if new_lead >= len {
        return None;
    }
    let start = start.min(og.len());
    let before = og[old_lead.min(start)..start].concat();
    let from = if ng[new_lead..].concat().starts_with(&before) {
        start.min(new_lead + before.graphemes(true).count())
    } else {
        let common = og.iter().zip(&ng).take_while(|(a, b)| a == b).count();
        start.min(common)
    };
    let from = if from >= len { new_lead } else { from };
    Some(TextSpan {
        line: l,
        start: from as u32,
        end: len as u32,
    })
}

/// Line `l` (text `text`) from its first non-blank character to its last
/// one; `None` when it is blank.
pub fn whole_line(text: &str, l: u32) -> Option<TextSpan> {
    let ng: Vec<&str> = text.graphemes(true).collect();
    let lead = ng.iter().take_while(|c| c.trim().is_empty()).count();
    let len = ng.len() - ng.iter().rev().take_while(|c| c.trim().is_empty()).count();
    (lead < len).then_some(TextSpan {
        line: l,
        start: lead as u32,
        end: len as u32,
    })
}

/// Shifts or clamps a column anchor on `line` after `[s0, s1)` became a
/// run `delta` cells longer (or shorter).
fn remap_col(col: u32, s0: u32, s1: u32, delta: i64) -> u32 {
    if col >= s1 {
        (col as i64 + delta).max(s0 as i64) as u32
    } else if col > s0 {
        // Inside the masked span: clamped to the mask.
        (s0 as i64 + delta + (s1 - s0) as i64).min(col as i64) as u32
    } else {
        col
    }
}

fn remap_span(sp: &mut TextSpan, line: u32, s0: u32, s1: u32, delta: i64) {
    if sp.line != line {
        return;
    }
    let new_end_of_mask = (s1 as i64 + delta) as u32;
    if sp.start >= s1 {
        sp.start = (sp.start as i64 + delta) as u32;
        sp.end = (sp.end as i64 + delta) as u32;
    } else if sp.end > s0 {
        // Overlaps the mask: the redact wins, the span is clamped to the
        // part outside it (or to the mask itself when fully inside).
        if sp.start < s0 && sp.end > s1 {
            sp.end = (sp.end as i64 + delta) as u32;
        } else if sp.start < s0 {
            sp.end = s0.max(sp.start + 1);
        } else if sp.end > s1 {
            sp.start = new_end_of_mask;
            sp.end = (sp.end as i64 + delta) as u32;
            if sp.end <= sp.start {
                sp.end = sp.start + 1;
            }
        } else {
            sp.start = s0;
            sp.end = new_end_of_mask;
        }
    }
}

fn remap_shape(shape: &mut Shape, line: u32, s0: u32, s1: u32, delta: i64) {
    match shape {
        Shape::Highlight { span, .. } | Shape::Mark { span, .. } | Shape::Redact { span, .. } => {
            remap_span(span, line, s0, s1, delta)
        }
        Shape::Arrow { from, to, .. } => {
            for a in [from, to] {
                let Anchor { line: l, col } = a;
                if *l == line {
                    *col = remap_col(*col, s0, s1, delta);
                }
            }
        }
        Shape::Box { .. }
        | Shape::Note { .. }
        | Shape::Callout { .. }
        | Shape::Spotlight { .. } => {}
    }
}

/// One match of the built-in patterns (`:redact auto`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoMatch {
    pub line: u32,
    /// Grapheme columns of the value to mask.
    pub start: u32,
    pub end: u32,
    /// Which pattern matched (`jwt`, `bearer`, `env`, `url-credentials`, `aws`, `github`, `stripe`, `email`).
    pub what: &'static str,
}

struct Pattern {
    what: &'static str,
    re: Regex,
    /// Capture group holding the value to mask (0 = the whole match).
    group: usize,
}

fn patterns() -> &'static [Pattern] {
    static P: OnceLock<Vec<Pattern>> = OnceLock::new();
    P.get_or_init(|| {
        let p = |what, re: &str, group| Pattern {
            what,
            re: Regex::new(re).expect("static pattern"),
            group,
        };
        vec![
            // JSON Web Tokens: three base64url parts, the first starting with `eyJ`.
            p("jwt", r"eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}", 0),
            // Authorization headers and the like.
            p("bearer", r"(?i)\b(?:bearer|basic)\s+([A-Za-z0-9._~+/=-]{8,})", 1),
            // AWS access key ids, GitHub tokens, Stripe keys, Slack tokens, Google API keys.
            p("aws", r"\b(AKIA|ASIA)[0-9A-Z]{16}\b", 0),
            p("github", r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b|\bgithub_pat_[A-Za-z0-9_]{20,}\b", 0),
            p("stripe", r"\b(?:sk|pk|rk)_(?:live|test)_[A-Za-z0-9]{10,}\b", 0),
            p("slack", r"\bxox[abprs]-[A-Za-z0-9-]{10,}\b", 0),
            p("google", r"\bAIza[0-9A-Za-z_-]{30,}\b", 0),
            // Credentials inside a URL: scheme://user:password@host
            p("url-credentials", r"[a-zA-Z][a-zA-Z0-9+.-]*://[^\s/:@]+:([^\s@]+)@", 1),
            // key = value for password/secret/token/api_key in env, JSON, YAML, TOML, INI.
            p(
                "env",
                r#"(?i)(?:password|passwd|secret|token|api[_-]?key|apikey|access[_-]?key|private[_-]?key|client[_-]?secret)["']?\s*[:=]\s*["']?([^\s"',;}]{4,})"#,
                1,
            ),
            p("email", r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b", 0),
        ]
    })
}

/// Scans `code` with the built-in patterns and returns the values to mask,
/// in reading order, without overlaps (the first pattern to cover a place
/// keeps it). `first_line` numbers the lines.
pub fn auto_matches(code: &[String], first_line: u32) -> Vec<AutoMatch> {
    let mut out = Vec::new();
    for (i, text) in code.iter().enumerate() {
        let line = first_line + i as u32;
        let mut taken: Vec<(usize, usize)> = Vec::new();
        for pat in patterns() {
            for caps in pat.re.captures_iter(text) {
                let Some(m) = caps.get(pat.group) else {
                    continue;
                };
                let (bs, be) = (m.start(), m.end());
                if be <= bs || taken.iter().any(|(a, b)| bs < *b && be > *a) {
                    continue;
                }
                taken.push((bs, be));
                out.push(AutoMatch {
                    line,
                    start: crate::model::grapheme_index(text, bs),
                    end: crate::model::grapheme_index(text, be),
                    what: pat.what,
                });
            }
        }
    }
    out.sort_by_key(|m| (m.line, m.start));
    out
}

/// True when `text` looks like a secret by itself (a `:redact L:text`
/// anchor must be the text *before* the secret, never the secret: the
/// command line shows it and the history keeps it).
pub fn looks_like_secret(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    let one = [t.to_string()];
    if !auto_matches(&one, 1).is_empty() {
        return true;
    }
    // A long run without spaces: a token, a hash, a key.
    !t.contains(char::is_whitespace) && t.chars().count() >= 20
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(code: &str) -> Document {
        Document::from_code(code, "txt", None)
    }

    fn redact(id: &str, line: u32, start: u32, end: u32, keep: u8) -> Annotation {
        Annotation {
            id: id.into(),
            step: 3,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Redact {
                span: TextSpan { line, start, end },
                keep,
                label: None,
                widened: false,
            },
        }
    }

    #[test]
    fn mask_is_eight_bullets_plus_the_kept_tail_and_shifts_neighbours() {
        let mut d = doc("API_KEY=sk-live-abc123def456 # prod\nnext\n");
        d.annotations.push(redact("r1", 1, 8, 28, 4));
        d.annotations.push(Annotation {
            id: "h1".into(),
            step: 1,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Highlight {
                span: TextSpan {
                    line: 1,
                    start: 31,
                    end: 35,
                },
                label: None,
                emphasis: Default::default(),
            },
        });
        let m = masked(&d, 0);
        assert_eq!(m.code[0], "API_KEY=••••••••f456 # prod");
        assert_eq!(m.code[1], "next");
        let Shape::Redact { span, .. } = &m.annotations[0].shape else {
            panic!()
        };
        assert_eq!((span.start, span.end), (8, 20));
        let Shape::Highlight { span, .. } = &m.annotations[1].shape else {
            panic!()
        };
        assert_eq!((span.start, span.end), (23, 27), "`prod` moved left by 8");
        // Active from step 0 even though the redact is at step 3.
        assert_eq!(masked(&d, 0).code[0], m.code[0]);
        d.redact_from_step = true;
        assert!(masked(&d, 2).code[0].contains("sk-live"));
        assert!(!masked(&d, 3).code[0].contains("sk-live"));
    }

    #[test]
    fn a_line_shorter_than_its_mask_shows_no_tail_and_overlaps_merge() {
        // A value being typed: the tail of what is there so far would be
        // the start of the secret.
        let mut d = doc("API_KEY=sk-li\n");
        d.annotations.push(redact("r1", 1, 8, 28, 4));
        assert_eq!(masked(&d, 0).code[0], "API_KEY=••••••••");
        // Two masks that overlap are drawn once.
        let mut d = doc("token=abcdefghijklmnop end\n");
        d.annotations.push(redact("r1", 1, 6, 16, 2));
        d.annotations.push(redact("r2", 1, 10, 22, 0));
        assert_eq!(masked(&d, 0).code[0], "token=•••••••• end");
        let spans = [
            MaskSpan {
                line: 1,
                start: 6,
                end: 16,
                keep: 0,
            },
            MaskSpan {
                line: 1,
                start: 10,
                end: 22,
                keep: 0,
            },
        ];
        assert_eq!(mask_with(&d.code, 1, &spans)[0], "token=•••••••• end");
    }

    #[test]
    fn overlapping_span_is_clamped_to_the_mask() {
        let mut d = doc("token=abcdefghijklmnop end\n");
        d.annotations.push(redact("r1", 1, 6, 22, 0));
        d.annotations.push(Annotation {
            id: "h1".into(),
            step: 1,
            color: None,
            markers: None,
            file: None,
            shape: Shape::Highlight {
                span: TextSpan {
                    line: 1,
                    start: 10,
                    end: 14,
                },
                label: None,
                emphasis: Default::default(),
            },
        });
        let m = masked(&d, 0);
        assert_eq!(m.code[0], "token=•••••••• end");
        let Shape::Highlight { span, .. } = &m.annotations[1].shape else {
            panic!()
        };
        assert_eq!(
            (span.start, span.end),
            (6, 14),
            "inside the secret → the mask"
        );
    }

    #[test]
    fn auto_patterns_find_the_usual_shapes() {
        let code: Vec<String> = [
            "API_KEY=sk-live-abc123def456",
            "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV",
            "DATABASE_URL=postgres://admin:hunter2@db.internal:5432/app",
            "aws_access_key_id = AKIAIOSFODNN7EXAMPLE",
            "contact: ana@example.com",
            "let x = 1;",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let m = auto_matches(&code, 1);
        let whats: Vec<(u32, &str)> = m.iter().map(|x| (x.line, x.what)).collect();
        assert_eq!(
            whats,
            vec![
                (1, "env"),
                (2, "jwt"),
                (3, "url-credentials"),
                (4, "aws"),
                (5, "email")
            ]
        );
        // A real Stripe key by its prefix, a Bearer value by the header.
        let more: Vec<String> = [
            "key = sk_live_4eC39HqLyjWDarjtT1zdp7dc",
            "auth: Bearer abcdef0123456789",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let m2 = auto_matches(&more, 1);
        assert_eq!(
            m2[0].what, "stripe",
            "`key = ` is not an env-style name; the prefix catches it"
        );
        assert_eq!(
            &more[0][m2[0].start as usize..],
            "sk_live_4eC39HqLyjWDarjtT1zdp7dc"
        );
        assert_eq!(m2[1].what, "bearer");
        assert_eq!(&more[1][m2[1].start as usize..], "abcdef0123456789");
        // The Bearer value is the token, not the word.
        assert_eq!((m[1].start, m[1].end), (22, code[1].chars().count() as u32));
        // The URL credential is only the password.
        assert_eq!(&code[2][m[2].start as usize..m[2].end as usize], "hunter2");
        assert!(looks_like_secret("sk-live-abc123def456"));
        assert!(looks_like_secret(
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV"
        ));
        assert!(!looks_like_secret("API_KEY="));
        assert!(!looks_like_secret("Bearer"));
    }
}
