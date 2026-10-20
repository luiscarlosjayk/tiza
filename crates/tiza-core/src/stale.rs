//! Moves the code changed under (ADR 0007 §7). A follow reload or a paste
//! replaces the step-0 code with text the demo did not write, so a move is
//! never moved to follow it (that is for the author's own `move.add`,
//! [`crate::carry`]): a move that no longer matches the new code is marked
//! stale and skipped, and a later change that makes it match again clears
//! the mark. Nothing in the document changes; the mark lives only in the
//! instance (and in its undo history, with the moves).
//!
//! A move no longer matches when the lines of its `position` target do not
//! hold the text they held when it last matched (or are past the end), or
//! when it does not apply on the new code: a `find`, `after`, `before`,
//! `select` or `rebind` text that matches nothing or more than once, a line
//! past the end. Moves are checked in the order they run, without the stale
//! ones, so a move whose target a stale one wrote is stale too.

use std::collections::{BTreeMap, HashMap};

use crate::model::Document;
use crate::moves::{Move, MoveAction, PatchOp, Projection, ViewKey, rebind_problem, replay_order};

/// Why a move is skipped, and what it needs to match again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
    /// In words: `line 2 changed`, `find «x» matches nothing in the code`.
    pub reason: String,
    /// The view the move edits.
    pub file: ViewKey,
    /// Annotations of its step or later hidden while it is stale: they were
    /// on text it writes (`(unanchored)`, kept as they are).
    pub holds: Vec<String>,
    /// For a `position` target, the text its lines held when it last
    /// matched.
    expected: Option<Region>,
}

/// The text of the lines from `from` on (`None`: past the end).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Region {
    from: u32,
    text: Vec<Option<String>>,
}

/// The lines a `position` target stands on in `code`: the range a replace
/// or a delete changes, or the line an insert goes before and the one
/// above it.
fn region(m: &Move, code: &[String], first: u32) -> Option<Region> {
    let MoveAction::CodePatch(p) = &m.action else {
        return None;
    };
    let pos = p.position.as_deref()?;
    let mut parts = pos.split(['-', ':']).map(|n| n.trim().parse::<u32>().ok());
    let a = parts.next()??;
    let b = parts.next().flatten().unwrap_or(a);
    let (from, to) = match p.op {
        PatchOp::Insert => (a.saturating_sub(1).max(first), a),
        PatchOp::Replace | PatchOp::Delete => (a, b),
    };
    let line = |l: u32| code.get(l.checked_sub(first)? as usize).cloned();
    Some(Region {
        from,
        text: (from..=to).map(line).collect(),
    })
}

impl Region {
    /// Why `code` no longer holds this text, or `None` when it does.
    fn differs(&self, code: &[String], first: u32) -> Option<String> {
        let now = Region {
            from: self.from,
            text: (0..self.text.len() as u32)
                .map(|k| {
                    code.get((self.from + k).checked_sub(first)? as usize)
                        .cloned()
                })
                .collect(),
        };
        if now == *self {
            return None;
        }
        let to = self.from + self.text.len() as u32 - 1;
        let pairs = self.text.iter().zip(&now.text);
        if let Some(k) = pairs
            .clone()
            .position(|(was, is)| was.is_some() && is.is_none())
        {
            return Some(format!(
                "line {} is past the end of the code ({} lines)",
                self.from + k as u32,
                code.len()
            ));
        }
        let whitespace = pairs.filter(|(was, is)| was != is).all(|(was, is)| {
            matches!((was, is), (Some(a), Some(b)) if a.split_whitespace().eq(b.split_whitespace()))
        });
        Some(format!(
            "{} changed{}",
            if self.from == to {
                format!("line {to}")
            } else {
                format!("lines {}-{to}", self.from)
            },
            if whitespace { " (whitespace)" } else { "" }
        ))
    }
}

/// Marks the moves of `doc` that no longer match its code and clears the
/// mark of those that match again. `before` is the document just before
/// its code changed: the lines of each `position` target are compared with
/// what they held there (or, for a move already stale, when it last
/// matched). Without `before` (a `scene.put` of a document whose moves were
/// stale), only the moves already marked are checked again. Returns the
/// notice for the status line, `None` when no mark changed.
pub fn revalidate(before: Option<&Document>, doc: &mut Document) -> Option<String> {
    if doc.moves.is_empty() {
        return None;
    }
    let mut expected: BTreeMap<usize, Region> = doc
        .moves
        .iter()
        .enumerate()
        .filter_map(|(i, m)| Some((i, m.stale.as_ref()?.expected.clone()?)))
        .collect();
    if let Some(old) = before {
        let mut p = Projection::start(old);
        for (i, m) in replay_order(&old.moves, u32::MAX) {
            if m.stale.is_some() {
                continue;
            }
            let key = p.shown.file.clone();
            if let Some(r) = p
                .codes
                .get(&key)
                .and_then(|code| region(m, code, old.view_first_line(&key)))
            {
                expected.entry(i).or_insert(r);
            }
            let _ = p.apply_move(old, i, m);
        }
    }
    let was: Vec<Option<String>> = doc
        .moves
        .iter()
        .map(|m| m.stale.as_ref().map(|s| s.reason.clone()))
        .collect();
    let order: Vec<usize> = replay_order(&doc.moves, u32::MAX)
        .into_iter()
        .map(|(i, _)| i)
        .collect();
    let mut p = Projection::start(doc);
    for i in order {
        if before.is_none() && doc.moves[i].stale.is_none() {
            let m = doc.moves[i].clone();
            let _ = p.apply_move(doc, i, &m);
            continue;
        }
        let m = doc.moves[i].clone();
        let key = p.shown.file.clone();
        let first = doc.view_first_line(&key);
        let exp = expected.get(&i);
        let moved = p
            .codes
            .get(&key)
            .and_then(|code| exp.and_then(|r| r.differs(code, first)));
        let reason = moved.or_else(|| {
            let saved = (!m.rebind.is_empty()).then(|| p.clone());
            match p.apply_move(doc, i, &m) {
                Err(e) => Some(e.to_string()),
                Ok(()) => {
                    let why = saved.as_ref().and_then(|_| {
                        let ap = p.patches.last()?;
                        rebind_problem(&m, &ap.after, first)
                    });
                    if let (Some(_), Some(saved)) = (&why, saved) {
                        p = saved;
                    }
                    why
                }
            }
        });
        let holds = m.stale.map(|s| s.holds).unwrap_or_default();
        doc.moves[i].stale = reason.map(|reason| Stale {
            reason,
            file: key,
            holds,
            expected: exp.cloned(),
        });
    }
    doc.sync_code_to_step();
    notice(doc, &was)
}

/// `2 moves no longer match the code: #3 (…), #5 (…); they are skipped —
/// fix them before the next run`, and `move #5 matches the code again`,
/// when some mark appeared, changed its reason or went.
fn notice(doc: &Document, was: &[Option<String>]) -> Option<String> {
    let now: Vec<(usize, &str)> = doc
        .moves
        .iter()
        .enumerate()
        .filter_map(|(i, m)| Some((i, m.stale.as_ref()?.reason.as_str())))
        .collect();
    let new = now
        .iter()
        .any(|(i, r)| was.get(*i).and_then(|w| w.as_deref()) != Some(*r));
    let cleared: Vec<String> = was
        .iter()
        .enumerate()
        .filter(|(i, w)| w.is_some() && doc.moves.get(*i).is_some_and(|m| m.stale.is_none()))
        .map(|(i, _)| format!("#{}", i + 1))
        .collect();
    let mut parts = Vec::new();
    if new {
        let list = now
            .iter()
            .map(|(i, r)| format!("#{} ({r})", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(if now.len() == 1 {
            format!(
                "1 move no longer matches the code: {list}; it is skipped — fix it before the next run"
            )
        } else {
            format!(
                "{} moves no longer match the code: {list}; they are skipped — fix them before the next run",
                now.len()
            )
        });
    }
    match cleared.as_slice() {
        [] => {}
        [one] => parts.push(format!("move {one} matches the code again")),
        many => parts.push(format!("moves {} match the code again", many.join(", "))),
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Gives the moves of `doc` (a `scene.put` of the open document) the marks
/// of the same moves, at the same place, in `from`, so they load as they
/// were; [`revalidate`] without `before` then checks them again.
pub fn inherit(from: &Document, doc: &mut Document) {
    for (m, o) in doc.moves.iter_mut().zip(&from.moves) {
        let same = Move {
            stale: None,
            ..o.clone()
        } == *m;
        if same && o.stale.is_some() {
            m.stale = o.stale.clone();
        }
    }
}

/// Hides, behind a patch of its file at its step or before that was not
/// stale in `before` (the document just before the change) and is stale in
/// `doc`, every annotation that stands on a line that patch wrote: with the
/// patch skipped that line is another text, so the annotation is hidden as
/// `(unanchored)` and kept as it is until the patch matches again. Returns
/// the ids it hid (the ones already hidden stay so).
pub fn hold_new(before: &Document, doc: &mut Document) -> Vec<String> {
    let held = held(before);
    let mut wrote: HashMap<(usize, u32), Vec<u32>> = HashMap::new();
    let mut out = Vec::new();
    for a in &before.annotations {
        if held.contains(&a.id) {
            continue;
        }
        let lines = a.shape.line_ranges();
        let order: Vec<usize> = replay_order(&doc.moves, a.step)
            .into_iter()
            .map(|(i, _)| i)
            .filter(|i| {
                let m = &doc.moves[*i];
                matches!(m.action, MoveAction::CodePatch(_))
                    && m.stale.as_ref().is_some_and(|s| s.file == a.file)
                    && before.moves.get(*i).is_some_and(|b| b.stale.is_none())
            })
            .collect();
        for i in order {
            let w = wrote
                .entry((i, a.step))
                .or_insert_with(|| written(before, i, a.step));
            if lines
                .iter()
                .any(|(f, t)| w.iter().any(|l| (*f..=*t).contains(l)))
                && let Some(s) = doc.moves[i].stale.as_mut()
            {
                s.holds.push(a.id.clone());
                out.push(a.id.clone());
                break;
            }
        }
    }
    out
}

/// The lines patch `index` of `doc` wrote, as they are at `step` (carried
/// through the patches of the same file after it).
fn written(doc: &Document, index: usize, step: u32) -> Vec<u32> {
    let Ok(p) = crate::moves::project(doc, &doc.moves, step) else {
        return Vec::new();
    };
    let Some(k) = p.patches.iter().position(|ap| ap.index == index) else {
        return Vec::new();
    };
    let file = &p.patches[k].file;
    p.patches[k]
        .map
        .inserted
        .clone()
        .filter_map(|l| {
            p.patches[k + 1..]
                .iter()
                .filter(|later| &later.file == file)
                .try_fold(l, |l, later| later.map.get(l))
        })
        .collect()
}

/// Ids of the annotations stale moves hide.
pub fn held(doc: &Document) -> Vec<String> {
    doc.moves
        .iter()
        .filter_map(|m| m.stale.as_ref())
        .flat_map(|s| s.holds.iter().cloned())
        .collect()
}

/// How many moves are stale.
pub fn count(doc: &Document) -> usize {
    doc.moves.iter().filter(|m| m.stale.is_some()).count()
}
