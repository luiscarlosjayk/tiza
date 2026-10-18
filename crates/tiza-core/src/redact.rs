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

/// Whether the document has any redact annotation.
pub fn has_redacts(doc: &Document) -> bool {
    doc.annotations
        .iter()
        .any(|a| matches!(a.shape, Shape::Redact { .. }))
}

/// The document with every active mask applied: masked code lines, the
/// redact spans narrowed to the mask, and the other annotations of those
/// lines re-mapped. A document without redacts comes back unchanged.
pub fn masked(doc: &Document, step: u32) -> Document {
    let mut out = doc.clone();
    if !has_redacts(doc) {
        return out;
    }
    // Per line, the active masks sorted by start; later ones first so the
    // earlier columns stay valid while the text is rewritten.
    let mut by_line: std::collections::BTreeMap<u32, Vec<(usize, TextSpan, u8)>> =
        Default::default();
    for (i, a) in doc.annotations.iter().enumerate() {
        if let Shape::Redact { span, keep, .. } = &a.shape
            && active(doc, a, step)
        {
            by_line
                .entry(span.line)
                .or_default()
                .push((i, *span, *keep));
        }
    }
    for (line, masks) in by_line.iter_mut() {
        masks.sort_by_key(|(_, s, _)| std::cmp::Reverse(s.start));
        let Some(idx) = line
            .checked_sub(doc.source.first_line)
            .map(|i| i as usize)
            .filter(|i| *i < doc.code.len())
        else {
            continue;
        };
        for (ann_index, span, keep) in masks.iter() {
            let text = out.code[idx].clone();
            let g: Vec<&str> = text.graphemes(true).collect();
            let end = (span.end as usize).min(g.len());
            let start = (span.start as usize).min(end);
            if start == end {
                continue;
            }
            let keep = (*keep as usize).min(end - start).min(8);
            let tail: String = g[end - keep..end].concat();
            let replacement = format!("{MASK}{tail}");
            let new_len = MASK_WIDTH + keep as u32;
            out.code[idx] = format!(
                "{}{}{}",
                g[..start].concat(),
                replacement,
                g[end..].concat()
            );
            let old_len = (end - start) as u32;
            let delta = new_len as i64 - old_len as i64;
            let (s0, s1) = (start as u32, end as u32);
            // The redact itself now covers the mask.
            if let Shape::Redact { span: sp, .. } = &mut out.annotations[*ann_index].shape {
                sp.end = s0 + new_len;
            }
            for (j, a) in out.annotations.iter_mut().enumerate() {
                if j == *ann_index {
                    continue;
                }
                remap_shape(&mut a.shape, *line, s0, s1, delta);
            }
        }
    }
    out
}

/// Masks `code` with every redact in `annotations` (any step), without
/// touching annotations: for text matching that must not see a secret
/// (`moves::validate`), where only the lines matter.
pub fn mask_lines(code: &[String], first_line: u32, annotations: &[Annotation]) -> Vec<String> {
    let mut out = code.to_vec();
    let mut spans: Vec<(u32, u32, u32, u8)> = annotations
        .iter()
        .filter_map(|a| match &a.shape {
            Shape::Redact { span, keep, .. } => Some((span.line, span.start, span.end, *keep)),
            _ => None,
        })
        .collect();
    spans.sort_by_key(|(l, s, _, _)| (*l, std::cmp::Reverse(*s)));
    for (line, start, end, keep) in spans {
        let Some(idx) = line
            .checked_sub(first_line)
            .map(|i| i as usize)
            .filter(|i| *i < out.len())
        else {
            continue;
        };
        let g: Vec<&str> = out[idx].graphemes(true).collect();
        let end = (end as usize).min(g.len());
        let start = (start as usize).min(end);
        if start == end {
            continue;
        }
        let keep = (keep as usize).min(end - start).min(8);
        let tail: String = g[end - keep..end].concat();
        out[idx] = format!("{}{MASK}{tail}{}", g[..start].concat(), g[end..].concat());
    }
    out
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
/// `:embed` freezes), so a sidecar never carries a secret. Redacts are
/// authored on the shown code; on a document without moves it is the base.
pub fn masked_base(doc: &Document) -> Vec<String> {
    if !has_redacts(doc) {
        return doc.base().to_vec();
    }
    if doc.moves.is_empty() {
        return masked(doc, u32::MAX).code;
    }
    let mut d = doc.clone();
    d.code = doc.base().to_vec();
    d.moves.clear();
    d.base_code.clear();
    masked(&d, u32::MAX).code
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
