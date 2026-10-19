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
//! # Errors and echoes (t-167)
//!
//! The code is masked by [`masked`]. An error or an echo that can quote a
//! target text the agent or the document wrote goes through [`Echo`]: on
//! a document with any redact no target is quoted (`«…»`).

use std::sync::{Arc, OnceLock};

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

/// The annotations as they go with the masked step-0 code that a save and
/// `scene.get` write (t-174 AC10): the redacts of the step-0 code cover
/// exactly their mask (eight `•` plus the kept tail) instead of the real
/// value's columns, and without moves the other annotations of those lines
/// are re-mapped the same way. So the reopened (or re-put) document shows
/// exactly what was shown — no extra `•` when the value was shorter than
/// the mask, nothing shifted.
pub fn embedded_annotations(doc: &Document) -> Vec<Annotation> {
    if !has_redacts(doc) {
        return doc.annotations.clone();
    }
    if doc.moves.is_empty() {
        return masked(doc, u32::MAX).annotations;
    }
    let mut out = doc.annotations.clone();
    // The projection at each step: an annotation's columns are on the code
    // at the end of its own step (ADR 0007 §3d), its lines shifted by the
    // moves before it.
    let mut projections: std::collections::BTreeMap<u32, Option<crate::moves::Projection>> =
        Default::default();
    let mut keys: Vec<Option<String>> = vec![None];
    // A linked file read from disk again on reopening keeps its real
    // columns (t-174 review 3): only one frozen in `views` is masked.
    keys.extend(
        doc.view_code
            .keys()
            .filter(|k| {
                doc.views
                    .keys()
                    .any(|v| doc.view_key(v).as_ref() == Some(*k))
            })
            .cloned()
            .map(Some),
    );
    for key in keys {
        let redacts = base_redacts(doc, &key);
        if redacts.is_empty() {
            continue;
        }
        let Some(code) = doc.view_base_code(&key, false) else {
            continue;
        };
        let first = doc.view_first_line(&key);
        let lines: Vec<u32> = redacts
            .iter()
            .filter_map(|r| match &r.shape {
                Shape::Redact { span, .. } => Some(span.line),
                _ => None,
            })
            .collect();
        let base_text =
            |l: u32| -> Option<String> { code.get(l.checked_sub(first)? as usize).cloned() };
        let mut d = doc.clone();
        d.code = code.clone();
        d.source.first_line = first;
        d.moves.clear();
        d.base_code.clear();
        d.annotations = redacts.clone();
        let m = masked(&d, u32::MAX);
        for r in out.iter_mut().filter(|a| a.file == key) {
            if let Some(n) = m.annotations.iter().find(|n| n.id == r.id) {
                r.shape = n.shape.clone();
            }
        }
        for a in out.iter_mut().filter(|a| a.file == key) {
            if !matches!(
                a.shape,
                Shape::Highlight { .. } | Shape::Mark { .. } | Shape::Arrow { .. }
            ) {
                continue;
            }
            // Its step's code and where each step-0 line went by then.
            let proj = projections
                .entry(a.step)
                .or_insert_with(|| crate::moves::project(doc, &doc.moves, a.step, false).ok());
            let Some(proj) = proj.as_ref() else { continue };
            let Some(at_step) = proj.codes.get(&key) else {
                continue;
            };
            let to_step = |b: u32| -> Option<u32> {
                let mut l = b;
                for ap in proj.patches.iter().filter(|p| p.file == key) {
                    l = ap.map.get_column_anchor(l)?;
                }
                Some(l)
            };
            let text_at =
                |l: u32| -> Option<&String> { at_step.get(l.checked_sub(first)? as usize) };
            // Step line → step-0 line, for the redacted lines whose text is
            // still the same at its step.
            let back: Vec<(u32, u32)> = lines
                .iter()
                .filter_map(|b| {
                    let l = to_step(*b)?;
                    (text_at(l).cloned() == base_text(*b)).then_some((l, *b))
                })
                .collect();
            if back.is_empty() {
                continue;
            }
            // On the step-0 lines (others out of the way), masked, back.
            const AWAY: u32 = u32::MAX / 2;
            let to_base = |l: u32| {
                back.iter()
                    .find(|(s, _)| *s == l)
                    .map_or(AWAY + l, |(_, b)| *b)
            };
            let from_base = |l: u32| {
                if l >= AWAY {
                    l - AWAY
                } else {
                    back.iter().find(|(_, b)| *b == l).map_or(l, |(s, _)| *s)
                }
            };
            let mut probe = a.clone();
            let touched = map_column_lines(&mut probe.shape, to_base);
            if !touched {
                continue;
            }
            let mut d1 = d.clone();
            d1.annotations = redacts.iter().cloned().chain([probe]).collect();
            let m1 = masked(&d1, u32::MAX);
            if let Some(n) = m1.annotations.last() {
                let mut shape = n.shape.clone();
                map_column_lines(&mut shape, from_base);
                a.shape = shape;
            }
        }
    }
    out
}

/// Moves the lines of a shape's column anchors through `f`; `true` when
/// one of them landed on a line `f` knows (below `u32::MAX / 2`).
fn map_column_lines(shape: &mut Shape, f: impl Fn(u32) -> u32) -> bool {
    let mut touched = false;
    let mut go = |l: &mut u32| {
        *l = f(*l);
        touched |= *l < u32::MAX / 2;
    };
    match shape {
        Shape::Highlight { span, .. } | Shape::Mark { span, .. } => go(&mut span.line),
        Shape::Arrow { from, to, .. } => {
            go(&mut from.line);
            go(&mut to.line);
        }
        _ => {}
    }
    touched
}

/// The redacts of view `key` whose span is on that view's step-0 code: all
/// of them without moves; with moves, those of step 0 (and, with
/// `redact_from_step`, every one: their spans were marked at their own
/// step, so masking them on the step-0 code can only mask more).
pub fn base_redacts(doc: &Document, key: &Option<String>) -> Vec<Annotation> {
    doc.annotations
        .iter()
        .filter(|a| &a.file == key && matches!(a.shape, Shape::Redact { .. }))
        .filter(|a| {
            doc.moves.is_empty()
                || match crate::moves::redact_frame(doc, a) {
                    Some(crate::moves::RedactFrame::File) => true,
                    // Marked at their own step: masking them on the step-0
                    // code can only mask more.
                    Some(crate::moves::RedactFrame::EndOfStep(_)) => doc.redact_from_step,
                    _ => false,
                }
        })
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

/// Shortest piece of a masked value that counts as a copy of it (t-157):
/// four characters, at least one of them a letter or digit. A value
/// shorter than that only counts whole.
pub const MIN_PIECE: usize = 4;

/// A character as the copy rule compares it: without case (t-174 rework,
/// Luis's decision A: `HUNTER2` and `Hunter2` are copies of `hunter2`).
/// A character whose lowercase is several (`İ`) compares by the first.
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// A text as the copy rule reads it: each character folded
/// ([`fold`]) with its byte offset.
fn folded(text: &str) -> Vec<(usize, char)> {
    text.char_indices().map(|(i, c)| (i, fold(c))).collect()
}

/// One masked value prepared for the copy rule once (t-174 review: the
/// filter runs on every frame): its folded characters and its folded
/// pieces of [`MIN_PIECE`] characters with a letter or digit. Never
/// printed (Debug says nothing of it).
#[derive(Clone, Default)]
pub struct Needle {
    chars: Vec<char>,
    pieces: std::collections::HashSet<[char; MIN_PIECE]>,
}

impl std::fmt::Debug for Needle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Needle")
    }
}

/// The window of `MIN_PIECE` characters at `k`.
fn window(t: &[(usize, char)], k: usize) -> [char; MIN_PIECE] {
    let mut w = ['\0'; MIN_PIECE];
    for (i, c) in w.iter_mut().enumerate() {
        *c = t[k + i].1;
    }
    w
}

impl Needle {
    pub fn new(value: &str) -> Needle {
        let chars: Vec<char> = value.chars().map(fold).collect();
        let mut pieces = std::collections::HashSet::new();
        if chars.len() > MIN_PIECE {
            for w in chars.windows(MIN_PIECE) {
                if w.iter().any(|c| c.is_alphanumeric()) {
                    let mut a = ['\0'; MIN_PIECE];
                    a.copy_from_slice(w);
                    pieces.insert(a);
                }
            }
        }
        Needle { chars, pieces }
    }

    /// [`value_hits`] on a text already [`folded`] (`len`: its bytes).
    fn hits(&self, t: &[(usize, char)], len: usize) -> Vec<(usize, usize, bool)> {
        let mut hits: Vec<(usize, usize, bool)> = Vec::new();
        let v = &self.chars;
        if v.is_empty() || t.len() < v.len().min(MIN_PIECE) {
            return hits;
        }
        // Byte end of the char at `k`.
        let end = |k: usize| t.get(k + 1).map_or(len, |x| x.0);
        let mut k = 0;
        while k + v.len() <= t.len() {
            if t[k..k + v.len()].iter().zip(v).all(|(a, b)| a.1 == *b) {
                hits.push((t[k].0, end(k + v.len() - 1), true));
                k += v.len();
            } else {
                k += 1;
            }
        }
        if !self.pieces.is_empty() {
            // Overlapping copies of a piece (`aaaa` in `aaaaa`) too.
            for k in 0..t.len().saturating_sub(MIN_PIECE - 1) {
                if self.pieces.contains(&window(t, k)) {
                    hits.push((t[k].0, end(k + MIN_PIECE - 1), false));
                }
            }
        }
        hits.sort_unstable();
        let mut out: Vec<(usize, usize, bool)> = Vec::new();
        for (s, e, whole) in hits {
            match out.last_mut() {
                Some(last) if s <= last.1 => {
                    last.1 = last.1.max(e);
                    last.2 |= whole;
                }
                _ => out.push((s, e, whole)),
            }
        }
        out
    }
}

/// Every masked value of a filter prepared once ([`Needle`]), and every
/// window of [`MIN_PIECE`] characters of any of them: a text with none of
/// those windows holds no copy, which is what most texts are, and is
/// answered with one pass over it.
#[derive(Clone, Default)]
struct Needles {
    list: Vec<Needle>,
    any: std::collections::HashSet<[char; MIN_PIECE]>,
}

impl std::fmt::Debug for Needles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Needles({})", self.list.len())
    }
}

impl Needles {
    fn of(values: &[String]) -> Needles {
        let list: Vec<Needle> = values.iter().map(|v| Needle::new(v)).collect();
        let mut any = std::collections::HashSet::new();
        for n in &list {
            for w in n.chars.windows(MIN_PIECE) {
                let mut a = ['\0'; MIN_PIECE];
                a.copy_from_slice(w);
                any.insert(a);
            }
        }
        Needles { list, any }
    }

    /// Whether the folded text `t` may hold a copy at all.
    fn may_hold(&self, t: &[(usize, char)]) -> bool {
        !self.list.is_empty()
            && t.len() >= MIN_PIECE
            && (0..=t.len() - MIN_PIECE).any(|k| self.any.contains(&window(t, k)))
    }

    /// The hits of each value in `text`, in the values' order (empty when
    /// it holds none).
    fn hits(&self, text: &str) -> Vec<Vec<(usize, usize, bool)>> {
        let t = folded(text);
        if !self.may_hold(&t) {
            return Vec::new();
        }
        self.list.iter().map(|n| n.hits(&t, text.len())).collect()
    }

    fn any_hit(&self, text: &str) -> bool {
        self.hits(text).iter().any(|h| !h.is_empty())
    }
}

/// Byte ranges of `text` that hold `value` (t-157): every copy of the
/// whole value, and every run made of pieces of it of [`MIN_PIECE`]+
/// characters, merged where they touch; `true` when the range holds the
/// whole value. Without case (t-174 rework, Luis's decision A): fail
/// closed. Only searched for, never shown.
pub fn value_hits(text: &str, value: &str) -> Vec<(usize, usize, bool)> {
    if value.is_empty() || text.is_empty() {
        return Vec::new();
    }
    Needle::new(value).hits(&folded(text), text.len())
}

/// Whether `text` holds a whole copy of `value`, without case (the copy
/// rule's [`value_hits`], whole only).
pub fn holds_value(text: &str, value: &str) -> bool {
    value_hits(text, value).iter().any(|h| h.2)
}

/// Whether `c` can be part of a token around a secret (t-157): anything
/// but blanks, quotes and the punctuation that separates a value from its
/// key or from the next item.
fn token_char(c: char) -> bool {
    !c.is_whitespace()
        && !matches!(
            c,
            '"' | '\''
                | '`'
                | ','
                | ';'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '<'
                | '>'
                | '='
                | ':'
        )
}

/// The whole token of `text` around bytes `[b0, b1)` (t-157): grown to the
/// left and to the right over [`token_char`]s. When the token ends at a
/// `=` or `:` its end is unclear (base64 padding, `user:password`), so it
/// runs to the end of the line (trailing blanks aside): fail closed.
pub fn token_around(text: &str, b0: usize, b1: usize) -> (usize, usize) {
    let mut s = b0;
    while let Some(c) = text[..s].chars().next_back() {
        if !token_char(c) {
            break;
        }
        s -= c.len_utf8();
    }
    let mut e = b1;
    while let Some(c) = text[e..].chars().next() {
        if !token_char(c) {
            if matches!(c, '=' | ':') {
                e = text.trim_end().len().max(e);
            }
            break;
        }
        e += c.len_utf8();
    }
    (s, e)
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

/// The texts under every mask at any step (the step-0 code and every step
/// a move changes): at least four characters, longest first. Only searched
/// for, never shown (`tiza check`, [`Echo`]).
pub fn masked_values(doc: &Document) -> Vec<String> {
    if !has_redacts(doc) {
        return Vec::new();
    }
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut take = |d: &Document, step: u32| {
        for a in &d.annotations {
            if let Shape::Redact { span, .. } = &a.shape
                && active(d, a, step)
                && let Some(t) = d.line_text(span.line)
            {
                let g: Vec<&str> = t.graphemes(true).collect();
                let (s, e) = (
                    (span.start as usize).min(g.len()),
                    (span.end as usize).min(g.len()),
                );
                let v = g[s..e].concat();
                let v = v.trim();
                if v.chars().count() >= MIN_PIECE && !v.contains(MASK) {
                    out.insert(v.to_string());
                }
            }
        }
    };
    if doc.moves.is_empty() {
        take(doc, u32::MAX);
    } else {
        // The projected code of every step, never the code in memory: a
        // move being typed shows part of its content, and the cache key
        // ([`values_key`]) holds what the projection reads, not that.
        // Only the steps where something changes (t-166).
        for step in doc.change_steps(doc.max_step()) {
            take(&doc.projected_view_at(step).doc, step);
        }
        // And every redact on a file's own lines, of the main file or of a
        // linked one, whether or not a step shows it (t-174 review 2).
        let mut keys: Vec<Option<String>> = vec![None];
        keys.extend(doc.view_code.keys().cloned().map(Some));
        for key in keys {
            let Some(code) = doc.view_base_code(&key, false) else {
                continue;
            };
            let first = doc.view_first_line(&key);
            for r in base_redacts(doc, &key) {
                if let Shape::Redact { span, .. } = &r.shape
                    && let Some(t) = span
                        .line
                        .checked_sub(first)
                        .and_then(|i| code.get(i as usize))
                {
                    let g: Vec<&str> = t.graphemes(true).collect();
                    let (s0, e0) = (
                        (span.start as usize).min(g.len()),
                        (span.end as usize).min(g.len()),
                    );
                    let v = g[s0..e0].concat();
                    let v = v.trim();
                    if v.chars().count() >= MIN_PIECE && !v.contains(MASK) {
                        out.insert(v.to_string());
                    }
                }
            }
        }
    }
    let mut v: Vec<String> = out.into_iter().collect();
    v.sort_by_key(|s| std::cmp::Reverse(s.len()));
    v
}

/// Every target text the document itself wrote (t-167): the `find`,
/// `after`, `before`, `find_start`/`find_end` of its moves, the
/// `from`/`to.find` of a `select`, the `rebind` finds, and the range
/// markers of its annotations.
pub fn document_targets(doc: &Document) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in &doc.moves {
        out.extend(Document::move_targets(m).into_iter().map(|(_, t)| t));
    }
    for a in &doc.annotations {
        if let Some(mk) = &a.markers {
            out.push(mk.start.clone());
            out.push(mk.end.clone());
        }
    }
    out
}

/// What is left of a quoted target in an error or an echo (t-167).
pub const ELIDED: &str = "«…»";

/// How a socket error for parameters that do not deserialize starts: the
/// rest is serde's message, filtered with [`Echo::serde_text`].
pub const INVALID_PARAMETERS: &str = "invalid parameters: ";

/// Whether an `invalid_argument` message carries serde's words (socket
/// parameters, a `scene.patch` annotation) and so serde's list of valid
/// names, which [`Echo::serde_text`] keeps.
fn is_serde_message(s: &str) -> bool {
    s.starts_with(INVALID_PARAMETERS)
        || [
            "unknown variant ",
            "unknown field ",
            "invalid type: ",
            "invalid value: ",
        ]
        .iter()
        .any(|m| s.contains(m))
}

/// The one filter for an error or an echo that can quote a target text
/// the agent or the document wrote (t-167): `find`, `after`, `before`,
/// `find_start`/`find_end`, `select`, `rebind`, range markers, `at.find`,
/// `from`/`to.find`, a step id. On a document with any redact, nothing
/// quotes such a text — not only when it falls under a mask: the
/// difference between «…» and the text would tell the agent its text is
/// under one (an oracle). Every text the request or the document wrote is
/// taken out the same way, whatever it holds (so that is no oracle
/// either), and then any token of the message holding a piece of
/// [`MIN_PIECE`]+ characters of a masked value (the copy rule of t-157).
/// Without redacts every text is quoted as before. `tiza check` and
/// `move.add` say «…» the same way (t-154).
#[derive(Clone, Default)]
pub struct Echo {
    active: bool,
    written: Vec<String>,
    values: Arc<Vec<String>>,
    /// `values` prepared once ([`Needles`]).
    needles: Arc<Needles>,
}

/// Says how many values and texts it holds, never which.
impl std::fmt::Debug for Echo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Echo(active: {}, {} values, {} written)",
            self.active,
            self.values.len(),
            self.written.len()
        )
    }
}

/// A plain lowercase word (`note`, `dashed`, `secret`): the vocabulary of
/// messages and of enum values, which the filter leaves where it stands
/// unquoted — otherwise «a note needs `line`» would lose its words.
fn plain_word(t: &str) -> bool {
    !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_lowercase() || matches!(c, '.' | '-' | '_'))
}

impl Echo {
    /// The filter for errors and echoes about `doc` (inactive when it has
    /// no redact), knowing the targets the document wrote.
    pub fn new(doc: &Document) -> Echo {
        if !has_redacts(doc) {
            return Echo::default();
        }
        Echo::with_values(Arc::new(masked_values(doc))).written(document_targets(doc))
    }

    /// [`Echo::new`] for a document still being loaded (t-166): its code is
    /// resolved but [`Document::normalize_loaded`] has not set the step-0
    /// code of a document with moves yet, which the masked values are read
    /// from; so an error raised while loading never quotes them.
    pub fn loading(doc: &Document) -> Echo {
        if doc.moves.is_empty() || !doc.base_code.is_empty() {
            return Echo::new(doc);
        }
        let mut probe = doc.clone();
        probe.base_code = probe.code.clone();
        probe.base_first_line = probe.source.first_line;
        Echo::new(&probe)
    }

    /// [`Echo::new`] without the document's targets, for listings that
    /// need [`Echo::quote`] alone.
    pub fn quoting(doc: &Document) -> Echo {
        if !has_redacts(doc) {
            return Echo::default();
        }
        Echo::with_values(Arc::new(masked_values(doc)))
    }

    /// An active filter for `values`, prepared once.
    fn with_values(values: Arc<Vec<String>>) -> Echo {
        Echo {
            active: true,
            written: Vec::new(),
            needles: Arc::new(Needles::of(&values)),
            values,
        }
    }

    /// Always active, with no document to read values from: for errors
    /// raised before there is one (a `.tiza` that does not parse, MCP
    /// parameters that do not deserialize). Fail closed.
    pub fn closed() -> Echo {
        Echo {
            active: true,
            ..Echo::default()
        }
    }

    /// Also active when the request itself carries a redact (a
    /// `scene.put` or `scene.patch` that brings one): fail closed.
    pub fn redacted(mut self, yes: bool) -> Echo {
        self.active |= yes;
        self
    }

    /// The texts the agent wrote (every string of its request, a command
    /// line's words). Each counts as written and trimmed: a step id is
    /// trimmed before it is looked up.
    pub fn written<I, S>(mut self, texts: I) -> Echo
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if !self.active {
            return self;
        }
        for t in texts {
            let t: String = t.into();
            let trimmed = t.trim();
            if !trimmed.is_empty() && trimmed.len() != t.len() {
                self.written.push(trimmed.to_string());
            }
            if !t.is_empty() {
                self.written.push(t);
            }
        }
        self.written.sort_by_key(|t| std::cmp::Reverse(t.len()));
        self.written.dedup();
        self
    }

    /// Whether this filter changes anything (the document has a redact).
    pub fn active(&self) -> bool {
        self.active
    }

    /// A target as an echo quotes it: `«text»`, or [`ELIDED`].
    pub fn quote(&self, text: &str) -> String {
        if self.active {
            ELIDED.into()
        } else {
            format!("«{text}»")
        }
    }

    /// `text` with every copy, or piece of [`MIN_PIECE`]+ characters, of a
    /// masked value taken out with its whole token (t-157), for the free
    /// text an echo repeats (a label, a note).
    pub fn copies(&self, text: &str) -> String {
        self.take_out(text, false)
    }

    /// Every hit of a masked value in `text` (only whole copies with
    /// `whole_only`) taken out with its whole token.
    fn take_out(&self, text: &str, whole_only: bool) -> String {
        let mut out = text.to_string();
        for n in self.needles.list.iter() {
            let mut tokens: Vec<(usize, usize)> = Vec::new();
            for (b0, b1, whole) in n.hits(&folded(&out), out.len()) {
                if whole_only && !whole {
                    continue;
                }
                let (t0, t1) = token_around(&out, b0, b1);
                match tokens.last_mut() {
                    Some(last) if t0 <= last.1 => last.1 = last.1.max(t1),
                    _ => tokens.push((t0, t1)),
                }
            }
            for (t0, t1) in tokens.into_iter().rev() {
                out.replace_range(t0..t1, "…");
            }
        }
        out
    }

    /// The copy rule on an error's tokens: a token (a run of
    /// [`token_char`]s) holding a piece of a masked value goes, except a
    /// plain lowercase word of the message's own template (`looks like a
    /// secret`), which may share a piece with a value. A whole copy goes
    /// anyway, and so does a plain word the request, the command line or
    /// the request or the command line wrote (t-167 rework: `mango-quartz` of
    /// a masked `zebra-mango-quartz`) — only Tiza's fixed words stay.
    fn take_out_tokens(&self, text: &str) -> String {
        if self.values.is_empty() {
            return text.to_string();
        }
        let written_word = |tok: &str| {
            self.written
                .iter()
                .any(|t| t.split(|c: char| !token_char(c)).any(|w| w == tok))
        };
        let out = self.take_out(text, true);
        let mut res = String::with_capacity(out.len());
        let mut tok = String::new();
        let flush = |tok: &mut String, res: &mut String| {
            if !tok.is_empty() {
                let hit = (!plain_word(tok) || written_word(tok)) && self.needles.any_hit(tok);
                res.push_str(if hit { "…" } else { tok });
                tok.clear();
            }
        };
        for c in out.chars() {
            if token_char(c) {
                tok.push(c);
            } else {
                flush(&mut tok, &mut res);
                res.push(c);
            }
        }
        flush(&mut tok, &mut res);
        res
    }

    /// An error message or an echo with no target quoted, in this order:
    /// a message that is a written text itself goes whole; every written
    /// text quoted `«…»`, `"…"` (or as `{:?}` prints it), `` `…` `` or
    /// `'…'` becomes [`ELIDED`]; every other `«…»` quote too (fail closed:
    /// a target this filter was not told about); a written text of
    /// [`MIN_PIECE`]+ characters that is not a plain lowercase word goes
    /// wherever it stands (`unknown method: …`); and last the copy rule on
    /// the tokens left.
    pub fn text(&self, msg: &str) -> String {
        if !self.active {
            return msg.to_string();
        }
        if self.written.iter().any(|t| t == msg || t == msg.trim()) {
            return "…".into();
        }
        let mut s = elide_asked_numbers(msg);
        for t in &self.written {
            for q in [
                format!("«{t}»"),
                format!("{t:?}"),
                format!("\"{t}\""),
                format!("`{t}`"),
                format!("'{t}'"),
            ] {
                s = s.replace(&q, ELIDED);
            }
        }
        let mut out = String::with_capacity(s.len());
        let mut rest = s.as_str();
        while let Some(k) = rest.find('«') {
            out.push_str(&rest[..k]);
            out.push_str(ELIDED);
            let after = &rest[k + '«'.len_utf8()..];
            rest = match after.find('»') {
                Some(e) => &after[e + '»'.len_utf8()..],
                None => "",
            };
        }
        out.push_str(rest);
        for t in &self.written {
            // Its trimmed copy is in the list too (and keeps the blanks).
            let untrimmed = t.trim().len() != t.len();
            if t.chars().count() >= MIN_PIECE && !plain_word(t) && !untrimmed && t != ELIDED {
                out = out.replace(t.as_str(), "…");
            }
        }
        self.take_out_tokens(&out)
    }

    /// [`Echo::text`] on a serde message (`unknown variant `x`, expected
    /// one of `highlight`, `box`, …`), keeping serde's list of what it
    /// expected (t-167 rework): those are Tiza's own fixed names, and
    /// filtering them as written texts (the document says `"box"`) made the
    /// message claim `box` is not valid. Only the part before the last
    /// `, expected ` holds the value received, and it is filtered whole; in
    /// the list a plain lowercase name stays even when written, everything
    /// else goes through the filter as usual (a value is always before
    /// serde's own `expected`, so the last one is serde's).
    pub fn serde_text(&self, msg: &str) -> String {
        if !self.active {
            return msg.to_string();
        }
        let msg = &scrub_unknown_field(msg);
        let Some(k) = msg.rfind(", expected ") else {
            return self.text(msg);
        };
        let (head, tail) = msg.split_at(k);
        // serde's list never holds a Tiza quote: a tail that does is a
        // written text that happens to say `, expected ` — filtered whole.
        if tail.contains(['«', '»', '"']) {
            return self.text(msg);
        }
        let mut names = self.clone();
        names.written.retain(|t| !plain_word(t));
        format!("{}{}", self.text(head), names.text(tail))
    }

    /// [`Echo::text`] on the message of an error, keeping its kind (and
    /// so its `code`).
    pub fn error(&self, e: Error) -> Error {
        if !self.active {
            return e;
        }
        match e {
            // Without a «…» quote the message is the bare `find` itself.
            Error::TextNotFound(s) if !s.contains('«') => Error::TextNotFound("…".into()),
            Error::TextNotFound(s) => Error::TextNotFound(self.text(&s)),
            Error::NoMatch(s) => Error::NoMatch(self.text(&s)),
            Error::MoveNoMatch { index, step, what } => Error::MoveNoMatch {
                index,
                step,
                what: self.text(&what),
            },
            // serde's own list of valid names stays (t-167 rework).
            Error::Invalid(s) if is_serde_message(&s) => Error::Invalid(self.serde_text(&s)),
            Error::Invalid(s) => Error::Invalid(self.text(&s)),
            Error::StepNotFound(s) => Error::StepNotFound(self.text(&s)),
            Error::NotFound(s) => Error::NotFound(self.text(&s)),
            // Always `#N`, the number asked: never said (t-167 review 2).
            Error::MoveNotFound(_) => Error::MoveNotFound(format!("#{ELIDED}")),
            // The numbers the request wrote are not repeated (t-167 review).
            e @ (Error::LineOutOfRange { .. } | Error::SpanOutOfRange { .. }) => {
                Error::RangeText(self.text(&e.to_string()))
            }
            Error::RangeText(s) => Error::RangeText(self.text(&s)),
            Error::DuplicateId(s) => Error::DuplicateId(self.text(&s)),
            // serde quotes the values it could not read (`invalid type:
            // string "…"`, `unknown variant `…``).
            Error::Json(j) => Error::JsonText(self.serde_text(&j.to_string())),
            Error::JsonText(s) => Error::JsonText(self.serde_text(&s)),
            other => other,
        }
    }
}

/// serde's message for a request value it could not read, without the
/// values the request wrote, with or without redacts (t-167 rework: a
/// closed value's error never repeats what was sent; serde's list of valid
/// names stays). For socket parameters and `scene.patch` annotations.
pub fn serde_message(e: &serde_json::Error, request: &serde_json::Value) -> String {
    Echo::closed()
        .written(json_written(request))
        .serde_text(&e.to_string())
}

/// Every string in a JSON value, object keys included (the texts an
/// agent's request or a document wrote; a key is written text too: t-167
/// review, `unknown field `…``).
pub fn json_strings(v: &serde_json::Value) -> Vec<String> {
    json_texts(v, false)
}

/// [`json_strings`] plus every number, as JSON writes it: everything an
/// agent's request wrote, for the error filter of a request (t-167
/// review: `move #…`, `line … does not exist` repeat no number sent).
pub fn json_written(v: &serde_json::Value) -> Vec<String> {
    json_texts(v, true)
}

fn json_texts(v: &serde_json::Value, numbers: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![v];
    while let Some(v) = stack.pop() {
        match v {
            serde_json::Value::String(s) => {
                if numbers {
                    out.extend(digit_runs(s));
                }
                out.push(s.clone());
            }
            serde_json::Value::Number(n) if numbers => out.push(n.to_string()),
            serde_json::Value::Array(a) => stack.extend(a.iter()),
            serde_json::Value::Object(o) => {
                for k in o.keys() {
                    if numbers {
                        out.extend(digit_runs(k));
                    }
                    out.push(k.clone());
                }
                stack.extend(o.values());
            }
            _ => {}
        }
    }
    out
}

/// serde's `unknown field `x`` without `x`, with or without redacts: the
/// name of a field that is not one is never repeated (t-167 review); the
/// list of the expected ones stays.
fn scrub_unknown_field(msg: &str) -> String {
    const K: &str = "unknown field `";
    let mut out = String::with_capacity(msg.len());
    let mut rest = msg;
    while let Some(k) = rest.find(K) {
        out.push_str(&rest[..k]);
        out.push_str("unknown field ");
        out.push_str(ELIDED);
        let after = &rest[k + K.len()..];
        // The name ends at serde's own words, the last ones: a name may
        // hold a grave accent or `, expected ` itself (t-167 review 2).
        // (A struct with no field says only `there are no fields`, after
        // a name that may itself say `, expected `.)
        let end = after
            .strip_suffix("`, there are no fields")
            .map(str::len)
            .or_else(|| after.rfind("`, expected "))
            .or_else(|| after.rfind("`, there are no fields"))
            .or_else(|| after.rfind('`'));
        rest = match end {
            Some(e) => &after[e + 1..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// The runs of digits of a text, as written and without leading zeros
/// (`"3-04821"` → `3`, `04821`, `4821`): a number inside a text is
/// written too, in the form a message would print it (t-167 review 2).
pub fn digit_runs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for run in text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|r| !r.is_empty())
    {
        out.push(run.to_string());
        let bare = run.trim_start_matches('0');
        if bare != run && !bare.is_empty() {
            out.push(bare.to_string());
        }
    }
    out
}

/// The numbers a message says about what was asked — `line N does not
/// exist`, `step N is beyond the last step`, `range A..B does not fit on
/// line L`, `move #N does not exist` — as [`ELIDED`]: on a document with
/// redacts the message is the same whatever number was sent, or how it
/// was written (t-167 review 2). The document's own bounds stay.
fn elide_asked_numbers(msg: &str) -> String {
    // (before, after): the number between them goes.
    const PHRASES: [(&str, &str); 5] = [
        ("line ", " does not exist"),
        ("step ", " is beyond the last step"),
        ("range ", " does not fit on line "),
        ("does not fit on line ", " ("),
        ("move #", " does not exist"),
    ];
    let mut out = msg.to_string();
    for (before, after) in PHRASES {
        let mut res = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(k) = rest.find(before) {
            let start = k + before.len();
            res.push_str(&rest[..start]);
            let tail = &rest[start..];
            let n = tail
                .char_indices()
                .find(|&(_, c)| !(c.is_ascii_digit() || c == '.' || c == '…'))
                .map_or(tail.len(), |(i, _)| i);
            if n > 0 && tail[n..].starts_with(after) {
                res.push_str(ELIDED);
                rest = &tail[n..];
            } else {
                rest = tail;
            }
        }
        res.push_str(rest);
        out = res;
    }
    out
}

/// Whether a request carries a redact annotation (`"type": "redact"`
/// anywhere: a `scene.put` document, a `scene.patch` annotation).
pub fn json_has_redact(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Array(a) => a.iter().any(json_has_redact),
        serde_json::Value::Object(o) => {
            o.get("type").and_then(|t| t.as_str()) == Some("redact")
                || o.values().any(json_has_redact)
        }
        _ => false,
    }
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
                pin: None,
            },
        }
    }

    #[test]
    fn keys_and_numbers_are_written_and_unknown_fields_are_never_named() {
        let req = serde_json::json!({"a": {"123SECRETx": 1}, "n": 4821, "s": "x"});
        let mut w = json_written(&req);
        w.sort();
        assert_eq!(w, ["1", "123", "123SECRETx", "4821", "a", "n", "s", "x"]);
        let mut st = json_strings(&req);
        st.sort();
        assert_eq!(st, ["123SECRETx", "a", "n", "s", "x"]);
        assert_eq!(
            scrub_unknown_field("unknown field `999NOTHINx`, expected `line` or `find`"),
            "unknown field «…», expected `line` or `find`"
        );
        let e = Echo::closed().written(json_written(&req));
        assert_eq!(
            e.serde_text("unknown field `zzz`, expected one of `line`, `find`"),
            "unknown field «…», expected one of `line`, `find`"
        );
        assert_eq!(
            e.text("move #4821 does not exist"),
            "move #«…» does not exist"
        );
        // Review 2: digits inside a text, grave accents in a key, numbers
        // the message says.
        assert_eq!(digit_runs("3-04821:x"), ["3", "04821", "4821"]);
        assert_eq!(
            scrub_unknown_field("unknown field `x`, expected `SEC`, expected `line`"),
            "unknown field «…», expected `line`"
        );
        assert_eq!(
            scrub_unknown_field("unknown field `a`12SECRET`, there are no fields"),
            "unknown field «…», there are no fields"
        );
        assert_eq!(
            elide_asked_numbers(
                "move #1, step 2: line 5555 does not exist (the document spans 1 to 4)"
            ),
            "move #1, step 2: line «…» does not exist (the document spans 1 to 4)"
        );
        assert_eq!(
            elide_asked_numbers("range 0..99 does not fit on line 2 (it has 6 graphemes)"),
            "range «…» does not fit on line «…» (it has 6 graphemes)"
        );
        assert_eq!(
            elide_asked_numbers("step 5555 is beyond the last step (3)"),
            "step «…» is beyond the last step (3)"
        );
    }

    #[test]
    fn echo_keeps_serdes_list_and_masks_written_lowercase_pieces() {
        // t-167 rework point 2: the document says `box`; serde's list stays.
        let mut d = doc("token=zebra-mango-quartz\nport=1\n");
        d.annotations.push(redact("r1", 1, 6, 24, 0));
        let e = Echo::new(&d).written(["box", "note", "Zq7"]);
        let msg =
            "unknown variant `Zq7`, expected one of `highlight`, `box`, `note` at line 3 column 9";
        assert_eq!(
            e.serde_text(msg),
            "unknown variant «…», expected one of `highlight`, `box`, `note` at line 3 column 9"
        );
        assert_eq!(
            e.error(Error::Invalid(format!("{INVALID_PARAMETERS}{msg}")))
                .to_string(),
            format!(
                "{INVALID_PARAMETERS}unknown variant «…», expected one of `highlight`, `box`, `note` at line 3 column 9"
            )
        );
        // A tail holding a Tiza quote is not serde's: filtered whole.
        assert_eq!(e.serde_text("find «a, expected zz» here"), "find «…» here");
        // t-174 rework point 2: a lowercase piece of the value that the
        // request wrote goes, unquoted or not; Tiza's own words stay.
        let e = Echo::new(&d).written(["mango-quartz"]);
        assert_eq!(
            e.text("unknown theme: mango-quartz (dark, light)"),
            "unknown theme: … (dark, light)"
        );
        // Without a redact the filter changes nothing.
        let plain = doc("port=1\n");
        assert_eq!(Echo::new(&plain).serde_text(msg), msg);
    }

    #[test]
    fn echo_quotes_no_target_on_a_document_with_redacts() {
        let plain = doc("token=abc123SECRETxyz789\nport=1\n");
        let e = Echo::new(&plain).written(["NOWHERE"]);
        assert!(!e.active());
        assert_eq!(
            e.text("find «NOWHERE» (move #1)"),
            "find «NOWHERE» (move #1)"
        );
        assert_eq!(e.quote("port"), "«port»");

        let mut d = plain.clone();
        d.annotations.push(redact("r1", 1, 6, 24, 0));
        let e = Echo::new(&d).written(["x»SECRET", "nope\"q"]);
        assert!(e.active());
        // Any quote, under the mask or not; a quote with `»` inside.
        assert_eq!(e.text("find «port» (move #1)"), "find «…» (move #1)");
        assert_eq!(e.text("find «x»SECRET» here"), "find «…» here");
        // Debug-quoted, the whole message, and an unclosed quote.
        assert_eq!(
            e.text("no step is named \"nope\\\"q\""),
            "no step is named «…»"
        );
        assert_eq!(e.text("x»SECRET"), "…");
        assert_eq!(e.text("start «abc"), "start «…»");
        // A whole copy of the value goes with its token; an error's own
        // words keep pieces they share with it.
        assert_eq!(e.text("bad abc123SECRETxyz789, really"), "bad …, really");
        // A token with a piece of the value goes; plain lowercase words
        // (the message's own) stay, and so does the rest of the message.
        assert_eq!(e.text("unknown color: 123SECRETx"), "unknown color: …");
        assert_eq!(e.text("unknown variant `xyz789Q`"), "unknown variant `…`");
        assert_eq!(e.text("a note needs `line`"), "a note needs `line`");
        // A written text is taken out wherever it stands, trimmed too.
        let e = e.clone().written([" Qq9-step"]);
        assert_eq!(
            e.text("no step is named \"Qq9-step\""),
            "no step is named «…»"
        );
        assert_eq!(e.text("unknown method: Qq9-step"), "unknown method: …");
        // The copy rule for free text: a piece of 4+ characters.
        assert_eq!(e.copies("see 123SECRET now"), "see … now");
        assert_eq!(e.quote("port"), ELIDED);
        assert_eq!(
            e.error(Error::TextNotFound("port".into())).to_string(),
            "text \"…\" not found in the code"
        );
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
    fn copies_and_pieces_of_a_value_grow_to_their_whole_token() {
        let v = "abc123secret";
        let tok = |line: &str| -> Vec<String> {
            value_hits(line, v)
                .into_iter()
                .map(|(s, e, _)| {
                    let (s, e) = token_around(line, s, e);
                    line[s..e].to_string()
                })
                .collect()
        };
        // Start of a longer token, in the middle, quoted, twice.
        assert_eq!(
            tok("token=abc123secretLONGER-v9"),
            ["abc123secretLONGER-v9"]
        );
        assert_eq!(tok("x=PREabc123secretPOST"), ["PREabc123secretPOST"]);
        assert_eq!(tok(r#"k="abc123secret-v2", n=1"#), ["abc123secret-v2"]);
        assert_eq!(
            tok("a=abc123secretX b=abc123secretY"),
            ["abc123secretX", "abc123secretY"]
        );
        // An unclear end (`=`, `:`): the rest of the line.
        assert_eq!(tok("t=abc123secret==  "), ["abc123secret=="]);
        assert_eq!(
            tok("u=abc123secret:more stuff"),
            ["abc123secret:more stuff"]
        );
        // Pieces of 4+ characters count; 3 do not; nor pieces without a
        // letter or digit.
        assert_eq!(tok("id=abc123 ref=abc-x"), ["abc123"]);
        assert_eq!(tok("name=Xcret9"), ["Xcret9"]);
        assert!(tok("ref=abc-x sec").is_empty());
        let whole: Vec<bool> = value_hits("a=abc123secret b=c123s", v)
            .iter()
            .map(|h| h.2)
            .collect();
        assert_eq!(whole, [true, false], "only a whole copy is whole");
        assert!(value_hits("a=---- y", "x----z").is_empty());
        // Non-ASCII around a piece keeps char boundaries.
        assert_eq!(tok("é=ñabc123ñ"), ["ñabc123ñ"]);
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
