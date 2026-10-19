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
//!
//! # Errors (t-167, t-183)
//!
//! A redact masks the code and nothing else. A target matches on the masked
//! code, so «not found» reads the same whether the text is under a mask or
//! nowhere. Only the document's own targets in a load error are not quoted
//! when it has a redact ([`unquoted`]).

use std::sync::OnceLock;

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

use crate::Error;
use crate::model::{Anchor, Annotation, Document, Shape, TextSpan};

/// What a masked span shows (eight bullets, whatever the span's length).
pub const MASK: &str = "••••••••";
/// Cells the mask takes (before the kept tail).
pub const MASK_WIDTH: u32 = 8;

/// The one answer an agent (socket, MCP) gets when a request would take a
/// redact away or uncover the code under one (t-183 AC3): deleting,
/// narrowing, moving or retyping a redact, changing a masked line or
/// sending other code than the masked copy `scene.get` hands out. The same
/// words whatever the request held, so the answer never depends on what
/// is under a mask.
pub const AGENT_REFUSAL: &str = "a redact is the presenter's: an agent may add one, but not delete, narrow, move or retype one, change a masked line or send other code than scene.get hands out";

/// What a move that would rewrite or delete a masked line is told (t-183
/// AC4), after `move #N, step S: `.
pub const MASKED_LINE: &str = "which a redact masks: a move never changes or deletes a masked line";

/// Whether `e` is the [`MASKED_LINE`] refusal.
pub fn is_masked_line_error(e: &Error) -> bool {
    e.to_string().contains(MASKED_LINE)
}

/// The refusal of a redact on a line a move wrote (t-183, D4): a mask
/// covers only the file's own code. `id` may be empty (a redact not added
/// yet); `n` is the move, 1-based.
pub fn on_move_line(id: &str, n: usize) -> Error {
    let which = if id.trim().is_empty() {
        "the redact".to_string()
    } else {
        format!("redact {id}")
    };
    Error::Invalid(format!(
        "{which} is on a line move #{n} writes: a mask covers only the file's own code — put the value in the file, or delete the redact"
    ))
}

/// Redacts as the version before t-183 saved them (D3, D4), on a raw
/// `.tiza` before it is read: `redact_from_step` goes (masks show from
/// step 0, which only masks more); a redact `widened` by a reload keeps no
/// tail; `pin: 0` is the file's own code (`step: 0`); `pin: N`, a line move
/// #N wrote, is refused with [`on_move_line`].
pub fn migrate_legacy(raw: &mut serde_json::Value) -> Result<(), Error> {
    if let Some(o) = raw.as_object_mut() {
        o.remove("redact_from_step");
    }
    let Some(anns) = raw.get_mut("annotations").and_then(|a| a.as_array_mut()) else {
        return Ok(());
    };
    for a in anns {
        let Some(o) = a.as_object_mut() else {
            continue;
        };
        if o.get("type").and_then(|t| t.as_str()) != Some("redact") {
            continue;
        }
        if o.remove("widened").and_then(|w| w.as_bool()) == Some(true) {
            o.insert("keep".into(), 0.into());
        }
        match o.remove("pin").map(|p| p.as_u64()) {
            Some(Some(0)) => {
                o.insert("step".into(), 0.into());
            }
            Some(Some(n)) => {
                let id = o.get("id").and_then(|i| i.as_str()).unwrap_or_default();
                return Err(on_move_line(id, n as usize));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Characters a redact leaves visible after its bullets: its `keep`.
pub fn shown_tail(shape: &Shape) -> u8 {
    match shape {
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

/// The document with every mask applied: masked code lines, the redact
/// spans narrowed to the mask, and the other annotations of those lines
/// re-mapped. A document without redacts comes back unchanged.
///
/// Masks that overlap on a line are drawn as one. A span that runs past
/// the end of its line shows no kept tail.
pub fn masked(doc: &Document) -> Document {
    let mut out = doc.clone();
    if !has_redacts(doc) {
        return out;
    }
    let mut by_line: std::collections::BTreeMap<u32, Vec<(usize, TextSpan, u8)>> =
        Default::default();
    for (i, a) in doc.annotations.iter().enumerate() {
        if let Shape::Redact { span, .. } = &a.shape {
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

/// A column of a masked line (as [`masked`] draws it with `masks`, on a
/// line of `len` graphemes) back on the real line (t-183): before a mask
/// the same column, after it shifted back by what the mask added, inside
/// it the mask's own edge (`end`: its real end, else its real start), so
/// whatever an agent placed on the copy covers what the mask covers.
pub fn unmask_col(masks: &[MaskSpan], len: usize, col: u32, end: bool) -> u32 {
    let spans: Vec<(usize, usize, u8, usize)> = masks
        .iter()
        .map(|m| {
            let e = (m.end as usize).min(len);
            let keep = if (m.end as usize) > len { 0 } else { m.keep };
            ((m.start as usize).min(e), e, keep, 0)
        })
        .collect();
    let c = i64::from(col);
    // Copy column minus real column, before the group at hand.
    let mut delta: i64 = 0;
    for g in merge_groups(spans) {
        let real = (g.end - g.start) as i64;
        let shown = i64::from(MASK_WIDTH) + (g.keep as usize).min(g.end - g.start).min(8) as i64;
        let copy_start = g.start as i64 + delta;
        if c <= copy_start {
            break;
        }
        if c < copy_start + shown {
            return if end { g.end } else { g.start } as u32;
        }
        delta += shown - real;
    }
    (c - delta).max(0) as u32
}

/// The column anchors of `shape` placed on the masked copy, moved to the
/// real code ([`unmask_col`]); `masks` are every mask of its file at its
/// step, `len` gives a line's length on the real code.
pub fn unmask_shape(shape: &mut Shape, masks: &[MaskSpan], len: impl Fn(u32) -> usize) {
    let on =
        |line: u32| -> Vec<MaskSpan> { masks.iter().filter(|m| m.line == line).copied().collect() };
    match shape {
        Shape::Highlight { span, .. } | Shape::Mark { span, .. } | Shape::Redact { span, .. } => {
            let ms = on(span.line);
            if ms.is_empty() {
                return;
            }
            let n = len(span.line);
            let s = unmask_col(&ms, n, span.start, false);
            let e = unmask_col(&ms, n, span.end, true);
            span.start = s;
            span.end = e.max(s.saturating_add(1));
        }
        Shape::Arrow { from, to, .. } => {
            for at in [from, to] {
                let ms = on(at.line);
                if !ms.is_empty() {
                    at.col = unmask_col(&ms, len(at.line), at.col, false);
                }
            }
        }
        _ => {}
    }
}

/// Whether `[start, end)` on `line` touches a redacted span (any step): a
/// text search must never confirm or deny what is under a mask.
pub fn overlaps_mask(annotations: &[Annotation], line: u32, start: u32, end: u32) -> bool {
    annotations.iter().any(|a| {
        matches!(&a.shape, Shape::Redact { span, .. } if span.line == line && start < span.end && end > span.start)
    })
}

/// Only the masked code lines (what `code.get` and the `tiza://code`
/// resource return, on the view of the step).
pub fn masked_code(doc: &Document) -> Vec<String> {
    if !has_redacts(doc) {
        return doc.code.clone();
    }
    masked(doc).code
}

/// The step-0 code of the main file with its redacts applied (what a save
/// embeds, what `:embed` freezes), so a sidecar never carries a secret.
pub fn masked_base(doc: &Document) -> Vec<String> {
    if !has_redacts(doc) {
        return doc.base().to_vec();
    }
    let mut d = doc.clone();
    d.code = doc.base().to_vec();
    d.source.first_line = doc.base_first();
    d.annotations = base_redacts(doc, &None);
    d.moves.clear();
    d.base_code.clear();
    masked(&d).code
}

/// The annotations as they go with the masked step-0 code that a save and
/// `scene.get` write (t-174 AC10): every redact covers exactly its mask
/// (eight `•` plus the kept tail) instead of the real value's columns, and
/// every other annotation on a masked line is re-mapped the same way, at
/// its own step (the masked line is the same at every step: no move
/// rewrites it, t-183). So the reopened (or re-put) document shows exactly
/// what was shown — no extra `•` when the value was shorter than the
/// mask, nothing shifted.
pub fn embedded_annotations(doc: &Document) -> Vec<Annotation> {
    if !has_redacts(doc) {
        return doc.annotations.clone();
    }
    let mut out = doc.annotations.clone();
    // Each file at each step its annotations are written on.
    let mut groups: std::collections::BTreeMap<(Option<String>, u32), Vec<usize>> =
        Default::default();
    for (i, a) in doc.annotations.iter().enumerate() {
        groups.entry((a.file.clone(), a.step)).or_default().push(i);
    }
    for ((key, step), members) in groups {
        let redacts = doc.redacts_at(&key, step);
        if redacts.is_empty() {
            continue;
        }
        let Ok((code, first)) = doc.view_code_at(&key, step) else {
            continue;
        };
        let mut d = doc.clone();
        d.moves.clear();
        d.base_code.clear();
        d.code = code;
        d.source.first_line = first;
        let n = redacts.len();
        let others: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&i| !matches!(doc.annotations[i].shape, Shape::Redact { .. }))
            .collect();
        d.annotations = redacts;
        d.annotations
            .extend(others.iter().map(|&i| doc.annotations[i].clone()));
        let m = masked(&d);
        for &i in &members {
            if matches!(doc.annotations[i].shape, Shape::Redact { .. })
                && let Some(r) = m.annotations[..n].iter().find(|r| r.id == out[i].id)
            {
                out[i].shape = r.shape.clone();
            }
        }
        for (k, &i) in others.iter().enumerate() {
            out[i].shape = m.annotations[n + k].shape.clone();
        }
    }
    out
}

/// The redacts of view `key` (on that view's own code, t-183).
pub fn base_redacts(doc: &Document, key: &Option<String>) -> Vec<Annotation> {
    doc.annotations
        .iter()
        .filter(|a| &a.file == key && matches!(a.shape, Shape::Redact { .. }))
        .cloned()
        .collect()
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

pub use crate::error::ELIDED;

/// `e` with every text it quotes `«…»` taken out, when `doc` has a redact
/// (t-183 D7): a load error, a step change or `tiza check` names the moves
/// and markers by the document's own targets, and one of them may be the
/// value itself (the leak of t-167: printed on opening, in a recording).
/// Every quote goes alike, so the answer says nothing about the mask. What
/// an agent just sent is quoted as it wrote it.
pub fn unquoted(doc: &Document, e: Error) -> Error {
    if !has_redacts(doc) {
        return e;
    }
    match e {
        // The bare text, quoted by its `Display`.
        Error::TextNotFound(_) => Error::TextNotFound("…".into()),
        Error::NoMatch(s) => Error::NoMatch(elide_quotes(&s)),
        Error::MoveNoMatch { index, step, what } => Error::MoveNoMatch {
            index,
            step,
            what: elide_quotes(&what),
        },
        Error::Invalid(s) => Error::Invalid(elide_quotes(&s)),
        Error::StepNotFound(s) => Error::StepNotFound(elide_quotes(&s)),
        other => other,
    }
}

/// Whether a raw `.tiza` has a redact annotation, before it is parsed.
pub fn raw_has_redacts(raw: &serde_json::Value) -> bool {
    raw.get("annotations")
        .and_then(|a| a.as_array())
        .is_some_and(|a| {
            a.iter()
                .any(|x| x.get("type").and_then(|t| t.as_str()) == Some("redact"))
        })
}

/// serde's error for a `.tiza` that does not parse: without the value it
/// read when the document has a redact (t-183 D7), as it is otherwise.
pub fn parse_error(raw_redacted: bool, e: serde_json::Error) -> Error {
    if raw_redacted {
        Error::JsonText(crate::error::serde_text(&e.to_string()))
    } else {
        Error::Json(e)
    }
}

/// `msg` with every `«…»` quote as [`ELIDED`].
pub fn elide_quotes(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut rest = msg;
    while let Some(k) = rest.find('«') {
        out.push_str(&rest[..k]);
        out.push_str(ELIDED);
        let after = &rest[k + '«'.len_utf8()..];
        rest = after.find('»').map_or("", |e| &after[e + '»'.len_utf8()..]);
    }
    out.push_str(rest);
    out
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
            },
        }
    }

    #[test]
    fn a_load_error_quotes_no_target_only_with_redacts() {
        let e = || Error::MoveNoMatch {
            index: 2,
            step: 3,
            what: "find «token=abc» and «x»".into(),
        };
        let plain = doc("token=abc123\n");
        assert_eq!(unquoted(&plain, e()).to_string(), e().to_string());
        let mut d = plain.clone();
        d.annotations.push(redact("r1", 1, 6, 12, 0));
        assert_eq!(
            unquoted(&d, e()).to_string(),
            "move #2, step 3: find «…» and «…» matches nothing in the code at this step"
        );
        assert_eq!(
            unquoted(&d, Error::TextNotFound("abc".into())).to_string(),
            "text \"…\" not found in the code"
        );
        assert_eq!(elide_quotes("a «b» c «d"), "a «…» c «…»");
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
        let m = masked(&d);
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
    }

    #[test]
    fn a_line_shorter_than_its_mask_shows_no_tail_and_overlaps_merge() {
        // A value being typed: the tail of what is there so far would be
        // the start of the secret.
        let mut d = doc("API_KEY=sk-li\n");
        d.annotations.push(redact("r1", 1, 8, 28, 4));
        assert_eq!(masked(&d).code[0], "API_KEY=••••••••");
        // Two masks that overlap are drawn once.
        let mut d = doc("token=abcdefghijklmnop end\n");
        d.annotations.push(redact("r1", 1, 6, 16, 2));
        d.annotations.push(redact("r2", 1, 10, 22, 0));
        assert_eq!(masked(&d).code[0], "token=•••••••• end");
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
        let m = masked(&d);
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
