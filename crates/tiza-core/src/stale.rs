//! Moves the code changed under (ADR 0007 §7). A follow reload or a paste
//! replaces the step-0 code with text the demo did not write. Unlike the
//! author's own `move.add` ([`crate::carry`]), such a change does not say
//! where each line went, so a move follows it only where that is certain:
//! the old and the new code share a start and an end, and a line there is
//! the same line, moved by as many lines as the change between them added
//! or removed. A move whose line numbers (a `position`, a `cursor` line, a
//! `select` by line) fall on those lines gets the new numbers. The change
//! is one block when its old and new lines share no line; otherwise there
//! were several edits, the common end may be a coincidence, and a move from
//! the change down is marked stale and skipped even when the text there
//! looks the same (`BaseMap`); so is one whose lines no longer hold
//! the text they held. A text target
//! (`find`, `after`, `before`, `select`, `rebind`) is stale when it matches
//! nothing or more than once. Moves are checked in the order they run,
//! without the stale ones, so a move whose target a stale one wrote is
//! stale too. A later change that makes a move match again clears the
//! mark. The mark lives only in the instance (and in its undo history, with
//! the moves).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::carry::{LineId, World, position_text};
use crate::model::{Annotation, Document};
use crate::moves::{
    Move, MoveAction, PatchOp, Projection, ViewKey, rebind_problem, replay_order, shift_shape_by,
};

/// Why a move is skipped, and what it needs to match again.
#[derive(Debug, Clone, PartialEq)]
pub struct Stale {
    /// In words: `line 2 changed`, `find «x» matches nothing in the code`.
    pub reason: String,
    /// The view the move edits.
    pub file: ViewKey,
    /// For line-number targets, the text their lines held when the move
    /// last matched.
    expected: Option<Region>,
    /// The step-0 code of its view when the move last matched, and the
    /// move as it was then: code put back exactly as it was gives it back.
    last_good: Option<(Vec<String>, Box<Move>)>,
}

impl Stale {
    /// A mark known only by its reason (a copy of the live document sent
    /// for an export): enough to skip the move.
    pub fn named(reason: impl Into<String>) -> Stale {
        Stale {
            reason: reason.into(),
            file: None,
            expected: None,
            last_good: None,
        }
    }
}

/// An annotation hidden because it stood on text this move writes, while
/// the move was stale or until what it writes again is where it fits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    pub id: String,
    /// The first line the move wrote at the annotation's step, when it was
    /// hidden.
    pub wrote_first: u32,
}

/// What [`revalidate`] has to say on the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    /// A move stopped matching (said loudly), rather than only matching
    /// again.
    pub loud: bool,
}

/// The text of the lines from `from` on (`None`: past the end).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Region {
    from: u32,
    text: Vec<Option<String>>,
}

/// The lines a move's line numbers stand on in `code`: the range a
/// replace or a delete by `position` changes, the line an insert by
/// `position` goes before and the one above it, a `cursor` line, the lines
/// of a `select` given by line.
fn region(m: &Move, code: &[String], first: u32) -> Option<Region> {
    let (from, to) = match &m.action {
        MoveAction::CodePatch(p) => {
            let pos = p.position.as_deref()?;
            let mut parts = pos.split(['-', ':']).map(|n| n.trim().parse::<u32>().ok());
            let a = parts.next()??;
            let b = parts.next().flatten().unwrap_or(a);
            match p.op {
                PatchOp::Insert => (a.saturating_sub(1).max(first), a),
                PatchOp::Replace | PatchOp::Delete => (a, b),
            }
        }
        MoveAction::Cursor(c) => (c.line, c.line),
        MoveAction::Select(s) => {
            let lines: Vec<u32> = [s.from.line, s.to.line].into_iter().flatten().collect();
            (*lines.iter().min()?, *lines.iter().max()?)
        }
        MoveAction::CodeLink(_) | MoveAction::Wait(_) => return None,
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
                "line {} is past the end of the code ({} line{})",
                self.from + k as u32,
                code.len(),
                if code.len() == 1 { "" } else { "s" }
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

/// What a load-time error adds that a stale reason does not need (the bar
/// is short; `move.list` keeps the reason whole).
const LOAD_ADVICE: &str = "; a move target must match exactly once (add occurrence: N)";

/// Marks the moves of `doc` that no longer match its code and clears the
/// mark of those that match again. `before` is the document just before
/// its code changed by something the demo did not write: the lines each
/// move's line numbers name are compared with what they held there (or,
/// for a move already stale, when it last matched). Without `before` (a
/// `scene.put`, a move added or deleted), the moves are checked as they
/// stand; a stale one on line numbers stays stale unless its view's code is
/// back to what it was when it last matched. Then the annotations held
/// by moves that match again go back where those moves write
/// ([`release`]). Returns the notice for the status line, `None` when no
/// mark changed.
pub fn refresh(before: Option<&Document>, doc: &mut Document) -> Option<Notice> {
    let notice = revalidate(before, doc);
    release(doc);
    notice
}

/// [`refresh`] without putting held annotations back: a reload or a paste
/// rebuilds the annotations first and releases them afterwards.
///
/// With `before`, the lines a move's numbers name follow the change when
/// where they went is known: each view's old and new step-0 code share a
/// start and an end ([`BaseMap`]); a line there keeps its identity (moved by
/// as many lines as the change between them added or removed), a line in
/// between does not, and a move on one is stale even when the text there is
/// the same. Lines a move wrote keep theirs through the replay, as in
/// [`crate::carry`]. A move that follows its lines is given the new numbers.
/// A `rebind` entry counts only when its annotation is in `doc.annotations`.
pub fn revalidate(before: Option<&Document>, doc: &mut Document) -> Option<Notice> {
    if doc.moves.is_empty() {
        return None;
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
    let mut follow = before.map(|old| Follow::new(old, doc));
    let mut p = Projection::start(doc);
    for i in order {
        let key = p.shown.file.clone();
        let first = doc.view_first_line(&key);
        let kept = doc.moves[i].stale.as_ref().and_then(|s| s.expected.clone());
        let base = doc.view_base_code(&key);
        let last_good = match (&doc.moves[i].stale, before) {
            (Some(s), _) => s.last_good.clone(),
            (None, Some(old)) => old
                .view_base_code(&key)
                .map(|code| (code, Box::new(doc.moves[i].clone()))),
            (None, None) => None,
        };
        // Where its lines went, or why that is not known. The code exactly
        // as it was when it last matched gives the move back as it was.
        // Without a change of code to follow, a stale move on line numbers
        // stays stale: where its lines are is not known.
        let (m, exp, mut reason) = match (&mut follow, &last_good) {
            (_, Some((code, then))) if base.as_ref() == Some(code) => {
                ((**then).clone(), None, None)
            }
            (Some(f), _) => f.carry(&doc.moves[i], kept, doc),
            (None, _) => {
                let still = kept
                    .is_some()
                    .then(|| doc.moves[i].stale.as_ref().map(|s| s.reason.clone()))
                    .flatten();
                (doc.moves[i].clone(), kept, still)
            }
        };
        if reason.is_none()
            && let (Some(r), Some(code)) = (&exp, p.codes.get(&key))
        {
            reason = r.differs(code, first);
        }
        let reason = reason.or_else(|| {
            let saved = (!m.rebind.is_empty()).then(|| p.clone());
            match p.apply_move(doc, i, &m) {
                Err(e) => Some(e.to_string().replace(LOAD_ADVICE, "")),
                Ok(()) => {
                    let why = saved.as_ref().and_then(|_| {
                        let ap = p.patches.last()?;
                        rebind_problem(&m, &doc.annotations, &ap.after, first)
                    });
                    if let (Some(_), Some(saved)) = (&why, saved) {
                        p = saved;
                    }
                    why
                }
            }
        });
        if let Some(f) = &mut follow {
            f.ran(doc, i, reason.is_none().then_some(&m));
        }
        let stale = reason.map(|reason| Stale {
            reason,
            file: key,
            expected: exp,
            last_good,
        });
        doc.moves[i] = Move { stale, ..m };
    }
    doc.sync_code_to_step();
    notice(doc, &was)
}

/// Where the step-0 lines of a view went. The lines between the old and
/// the new code's common start and end are the change. A line of the
/// common start or end next to the change whose text the new lines of the
/// change also have might be either copy, so its place is ambiguous, and
/// so is every line closer to the change than it: the start and end are
/// cut back to the first line whose text the change does not repeat
/// (`sure_prefix`, `sure_suffix`). When the old and the new lines of the
/// change share no line it is one block: the sure end moved by `delta`,
/// and a block of as many lines as before maps line for line (each line
/// rewritten in place). When they share a line there were several edits
/// (or lines moved), and the common end may be a coincidence (one `}`
/// alike after edits further up): nothing from the change down has a known
/// place. A line whose place is ambiguous or unknown makes its move stale.
/// This reads the change as the smallest one: a longer history that leaves
/// the same text in the same place (`---`, `old`, `---` with the first two
/// lines deleted and `new`, `---` appended) cannot be told from text alone.
struct BaseMap {
    sure_prefix: usize,
    prefix: usize,
    old_len: usize,
    suffix: usize,
    sure_suffix: usize,
    delta: i64,
    one_block: bool,
}

impl BaseMap {
    fn new(old: &[String], new: &[String]) -> BaseMap {
        let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let room = old.len().min(new.len()) - prefix;
        let suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(room)
            .take_while(|(a, b)| a == b)
            .count();
        let changed: HashSet<&String> = new[prefix..new.len() - suffix].iter().collect();
        let was: HashSet<&String> = old[prefix..old.len() - suffix].iter().collect();
        let one_block = !changed.iter().any(|l| was.contains(l));
        let sure_prefix = (0..prefix)
            .rev()
            .find(|k| !changed.contains(&old[*k]))
            .map_or(0, |k| k + 1);
        let tail = old.len() - suffix;
        let sure_suffix = old.len()
            - (tail..old.len())
                .find(|k| !changed.contains(&old[*k]))
                .unwrap_or(old.len());
        BaseMap {
            sure_prefix,
            prefix,
            old_len: old.len(),
            suffix,
            sure_suffix,
            delta: new.len() as i64 - old.len() as i64,
            one_block,
        }
    }

    /// Where step-0 line `k` (0-based) is now; `old_len` for one past the
    /// end.
    fn get(&self, k: usize) -> Option<usize> {
        let shifted = || usize::try_from(k as i64 + self.delta).ok();
        if k < self.sure_prefix {
            Some(k)
        } else if k < self.prefix || !self.one_block {
            None
        } else if k < self.old_len - self.suffix {
            (self.delta == 0).then_some(k)
        } else if k < self.old_len - self.sure_suffix {
            None
        } else if k == self.old_len {
            // The end follows the last line when that line's place is sure
            // (or the change reaches the end).
            (self.sure_suffix > 0 || self.suffix == 0)
                .then(shifted)
                .flatten()
        } else {
            shifted()
        }
    }
}

/// The replay of the moves on the old code and on the new one, with the
/// identity of every line ([`crate::carry::World`]).
struct Follow<'a> {
    old_doc: &'a Document,
    old: World,
    new: World,
    maps: BTreeMap<ViewKey, BaseMap>,
}

impl<'a> Follow<'a> {
    fn new(old_doc: &'a Document, doc: &Document) -> Follow<'a> {
        let mut keys: Vec<ViewKey> = vec![None];
        keys.extend(doc.view_code.keys().cloned().map(Some));
        let maps = keys
            .into_iter()
            .filter_map(|k| {
                let map = BaseMap::new(&old_doc.view_base_code(&k)?, &doc.view_base_code(&k)?);
                Some((k, map))
            })
            .collect();
        Follow {
            old_doc,
            old: World::default(),
            new: World::default(),
            maps,
        }
    }

    /// The move with its line numbers where those lines are now, and the
    /// text they must hold there (`kept`, for a move already stale: the text
    /// it last matched; else what they held before the change); or why where
    /// they went is not known (with that text at the numbers it keeps).
    fn carry(
        &mut self,
        m: &Move,
        kept: Option<Region>,
        doc: &Document,
    ) -> (Move, Option<Region>, Option<String>) {
        let key = self.old.shown.clone();
        let first = self.old_doc.view_first_line(&key);
        let Some(old) = self.old.lines(self.old_doc, &key).cloned() else {
            return (m.clone(), None, None);
        };
        let Some(at_old) = region(m, &old.code, first) else {
            return (m.clone(), kept, None);
        };
        let was = kept.unwrap_or(at_old);
        let Some(new) = self.new.lines(doc, &key).cloned() else {
            return (m.clone(), Some(was), None);
        };
        let map = self.maps.get(&key);
        let end_known = map.is_none_or(|m| m.get(m.old_len).is_some());
        // Each line of the region, or one past the end, on the new side.
        let place = |l: u32| -> Option<u32> {
            let Some(id) = old.ids.get(l.checked_sub(first)? as usize) else {
                return (end_known && l == first + old.ids.len() as u32)
                    .then(|| first + new.ids.len() as u32);
            };
            let id = match *id {
                LineId::Base(k) => LineId::Base(map.map_or(Some(k), |m| m.get(k))?),
                written => written,
            };
            new.ids
                .iter()
                .position(|n| *n == id)
                .map(|n| first + n as u32)
        };
        let lines: Option<Vec<u32>> = (0..was.text.len() as u32)
            .map(|k| place(was.from + k))
            .collect();
        let shift = lines.as_ref().and_then(|ls| {
            let at = *ls.first()?;
            ls.iter()
                .enumerate()
                .all(|(k, l)| *l == at + k as u32)
                .then(|| i64::from(at) - i64::from(was.from))
        });
        let Some(shift) = shift else {
            let why = was.differs(&new.code, first).unwrap_or_else(|| {
                format!(
                    "{}: the code around it changed, so where it went is not known",
                    lines_text(was.from, was.from + was.text.len() as u32 - 1)
                )
            });
            return (m.clone(), Some(was), Some(why));
        };
        let at = Region {
            from: u32::try_from(i64::from(was.from) + shift).unwrap_or(was.from),
            text: was.text,
        };
        (shifted(m, shift), Some(at), None)
    }

    /// Move `i` ran on the old side (unless it was stale there) and, when
    /// `now` is given, on the new side as `now`.
    fn ran(&mut self, doc: &Document, i: usize, now: Option<&Move>) {
        let old = &self.old_doc.moves[i];
        if old.stale.is_none() {
            let _ = self.old.apply(self.old_doc, old, i);
        }
        if let Some(m) = now {
            let _ = self.new.apply(doc, m, i);
        }
    }
}

/// `line 7` or `lines 7-9`.
fn lines_text(a: u32, b: u32) -> String {
    if a == b {
        format!("line {a}")
    } else {
        format!("lines {a}-{b}")
    }
}

/// `m` with every line number it holds moved by `by`.
fn shifted(m: &Move, by: i64) -> Move {
    if by == 0 {
        return m.clone();
    }
    let f = |l: u32| u32::try_from(i64::from(l) + by).unwrap_or(l);
    let mut m = m.clone();
    match &mut m.action {
        MoveAction::CodePatch(p) => {
            if let Some(pos) = p.position.clone() {
                let mut parts = pos.split(['-', ':']).map(|n| n.trim().parse::<u32>().ok());
                if let Some(Some(a)) = parts.next() {
                    let b = parts.next().flatten().unwrap_or(a);
                    p.position = Some(position_text(&pos, f(a), f(b)));
                }
            }
        }
        MoveAction::Cursor(c) => c.line = f(c.line),
        MoveAction::Select(s) => {
            for w in [&mut s.from, &mut s.to] {
                w.line = w.line.map(f);
            }
        }
        MoveAction::CodeLink(_) | MoveAction::Wait(_) => {}
    }
    m
}

/// `stale move skipped (u undoes or re-add): #1 (L2 changed)`,
/// and `move #5 matches the code again`, when some mark appeared, changed
/// its reason or went. The moves come first: the bar may cut the end.
fn notice(doc: &Document, was: &[Option<String>]) -> Option<Notice> {
    let now: Vec<(usize, &str)> = doc
        .moves
        .iter()
        .enumerate()
        .filter_map(|(i, m)| Some((i, m.stale.as_ref()?.reason.as_str())))
        .collect();
    let loud = now
        .iter()
        .any(|(i, r)| was.get(*i).and_then(|w| w.as_deref()) != Some(*r));
    let cleared: Vec<String> = was
        .iter()
        .enumerate()
        .filter(|(i, w)| w.is_some() && doc.moves.get(*i).is_some_and(|m| m.stale.is_none()))
        .map(|(i, _)| format!("#{}", i + 1))
        .collect();
    let mut parts = Vec::new();
    if loud {
        let list = now
            .iter()
            .map(|(i, r)| format!("#{} ({})", i + 1, short(r)))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(if now.len() == 1 {
            format!("stale move skipped (u undoes or re-add): {list}")
        } else {
            format!(
                "{} stale moves skipped (u undoes or re-add): {list}",
                now.len()
            )
        });
    }
    match cleared.as_slice() {
        [] => {}
        [one] => parts.push(format!("move {one} matches the code again")),
        many => parts.push(format!("moves {} match the code again", many.join(", "))),
    }
    (!parts.is_empty()).then(|| Notice {
        text: parts.join(" · "),
        loud,
    })
}

/// A reason as the bar gives it, so the first moves fit: `L6 place
/// unknown`, `L4 past the end`, `L2 changed`, `target not found`. `move.list`,
/// `scene.list` and `check` keep the whole reason.
fn short(reason: &str) -> String {
    let lines = |h: &str| h.replace("lines ", "L").replace("line ", "L");
    let head = |sep: &str| reason.split_once(sep).map(|(h, _)| lines(h));
    if let Some(h) = head(": the code around it changed") {
        format!("{h} place unknown")
    } else if let Some(h) = head(" is past the end").or_else(|| head(" does not exist")) {
        format!("{h} past the end")
    } else if reason.ends_with(" changed") || reason.ends_with(" changed (whitespace)") {
        lines(reason)
    } else if reason.starts_with("rebind") {
        "rebind does not match".into()
    } else if reason.contains("matches nothing") {
        "target not found".into()
    } else if reason.contains("occurrence") {
        "occurrence past the matches".into()
    } else if reason.contains(" times") {
        "target matches more than once".into()
    } else {
        let mut cut: String = reason.chars().take(32).collect();
        if cut.len() < reason.len() {
            cut.push('…');
        }
        cut
    }
}

/// Gives the moves of `doc` (a `scene.put` of the open document) the marks
/// and held annotations of the same moves in `from` — each move paired
/// with the first equal one of `from` not paired yet, wherever it is now —
/// so they load as they were; [`refresh`] then checks them again.
pub fn inherit(from: &Document, doc: &mut Document) {
    let mut used = vec![false; from.moves.len()];
    for m in doc.moves.iter_mut() {
        let found = from.moves.iter().enumerate().position(|(k, o)| {
            !used[k]
                && Move {
                    stale: None,
                    held: Vec::new(),
                    ..o.clone()
                } == *m
        });
        if let Some(k) = found {
            used[k] = true;
            m.stale = from.moves[k].stale.clone();
            m.held = from.moves[k].held.clone();
        }
    }
}

/// Hides, behind a patch of its file at its step or before that was not
/// stale in `before` (the document just before the change) and is stale in
/// `doc`, every annotation that stands on a line that patch wrote: with the
/// patch skipped that line is another text, so the annotation is hidden as
/// `(unanchored)` and kept as it is until the patch writes again where it
/// fits ([`release`]). Returns the ids it hid (the ones already hidden stay
/// so).
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
            if let Some(&wrote_first) = w.first()
                && lines
                    .iter()
                    .any(|(f, t)| w.iter().any(|l| (*f..=*t).contains(l)))
            {
                doc.moves[i].held.push(Hold {
                    id: a.id.clone(),
                    wrote_first,
                });
                out.push(a.id.clone());
                break;
            }
        }
    }
    out
}

/// Puts back the annotations held by moves that are not stale: shifted by
/// as many lines as what the move writes moved since it was hidden, when
/// every line of the annotation then falls on what the move writes (or
/// unshifted, when what it writes did not move). One that does not fit
/// stays hidden.
pub fn release(doc: &mut Document) {
    for i in 0..doc.moves.len() {
        if doc.moves[i].stale.is_some() || doc.moves[i].held.is_empty() {
            continue;
        }
        let holds = std::mem::take(&mut doc.moves[i].held);
        let mut kept = Vec::new();
        for h in holds {
            let Some(k) = doc.annotations.iter().position(|a| a.id == h.id) else {
                continue;
            };
            let a = &doc.annotations[k];
            let w = written(doc, i, a.step);
            let Some(&now_first) = w.first() else {
                kept.push(h);
                continue;
            };
            if now_first == h.wrote_first {
                continue;
            }
            match shifted_onto(a, i64::from(now_first) - i64::from(h.wrote_first), &w) {
                Some(shape) => doc.annotations[k].shape = shape,
                None => kept.push(h),
            }
        }
        doc.moves[i].held = kept;
    }
}

/// `a` moved by `delta` lines, when every line of it lands on `lines`.
fn shifted_onto(a: &Annotation, delta: i64, lines: &[u32]) -> Option<crate::model::Shape> {
    let by = |l: u32| u32::try_from(i64::from(l) + delta).ok();
    let shape = shift_shape_by(&a.shape, &by, &|l, _| by(l))?;
    shape
        .line_ranges()
        .iter()
        .all(|(f, t)| (*f..=*t).all(|l| lines.contains(&l)))
        .then_some(shape)
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

/// Ids of the annotations moves hide.
pub fn held(doc: &Document) -> Vec<String> {
    doc.moves
        .iter()
        .flat_map(|m| m.held.iter().map(|h| h.id.clone()))
        .collect()
}

/// The first line number a move holds (its `position`, a `cursor` line, a
/// `select` by line); `None` for a move on text.
pub(crate) fn first_line(m: &Move) -> Option<u32> {
    match &m.action {
        MoveAction::CodePatch(p) => p
            .position
            .as_deref()
            .and_then(|pos| pos.split(['-', ':']).next()?.trim().parse::<u32>().ok()),
        MoveAction::Cursor(c) => Some(c.line),
        MoveAction::Select(s) => s.from.line.or(s.to.line),
        MoveAction::CodeLink(_) | MoveAction::Wait(_) => None,
    }
}

/// What the next load does with the stale moves, since their marks are not
/// saved: a move on line numbers applies on the line its number says, one
/// on text that does not match stops the document from opening
/// (`1 stale move (#1): next time it applies on line 2 as written`). `None`
/// when no move is stale.
pub fn close_summary(doc: &Document) -> Option<String> {
    let (count, next) = close_parts(doc)?;
    Some(format!("{count}: {next}"))
}

/// [`close_summary`] as `:q` refuses with it, the way out first so the bar
/// keeps it: `1 stale move (#1) — :q! quits anyway: next time it applies on
/// line 2 as written`.
pub fn on_close(doc: &Document) -> Option<String> {
    let (count, next) = close_parts(doc)?;
    Some(format!("{count} — :q! quits anyway: {next}"))
}

fn close_parts(doc: &Document) -> Option<(String, String)> {
    let stale: Vec<(usize, &Move)> = doc
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.stale.is_some())
        .collect();
    let next = |m: &Move| -> String {
        match first_line(m) {
            Some(l) => format!("applies on line {l} as written"),
            None => "stops the document from opening".into(),
        }
    };
    let ids = stale
        .iter()
        .map(|(i, _)| format!("#{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    match stale.as_slice() {
        [] => None,
        [(_, m)] => Some((
            format!("1 stale move ({ids})"),
            format!("next time it {}", next(m)),
        )),
        many => Some((
            format!("{} stale moves ({ids})", many.len()),
            format!(
                "next time {}",
                many.iter()
                    .map(|(i, m)| format!("#{} {}", i + 1, next(m)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// How many moves are stale.
pub fn count(doc: &Document) -> usize {
    doc.moves.iter().filter(|m| m.stale.is_some()).count()
}

/// The marks `scene.get` sends next to the document (which never carries
/// them): the stale moves with their reason, and which move hides which
/// annotation.
pub fn marks(
    doc: &Document,
) -> (
    Vec<crate::protocol::StaleMark>,
    Vec<crate::protocol::HeldMark>,
) {
    let stale = doc
        .moves
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            Some(crate::protocol::StaleMark {
                index: i + 1,
                reason: m.stale.as_ref()?.reason.clone(),
            })
        })
        .collect();
    let held = doc
        .moves
        .iter()
        .enumerate()
        .flat_map(|(i, m)| {
            m.held.iter().map(move |h| crate::protocol::HeldMark {
                index: i + 1,
                id: h.id.clone(),
            })
        })
        .collect();
    (stale, held)
}

/// Puts the marks of a `scene.get` reply on its document, so an export of
/// it skips and hides what the live screen does.
pub fn apply_marks(
    doc: &mut Document,
    stale: &[crate::protocol::StaleMark],
    held: &[crate::protocol::HeldMark],
) {
    for s in stale {
        if let Some(m) = s.index.checked_sub(1).and_then(|i| doc.moves.get_mut(i)) {
            m.stale = Some(Stale::named(s.reason.clone()));
        }
    }
    for h in held {
        if let Some(m) = h.index.checked_sub(1).and_then(|i| doc.moves.get_mut(i)) {
            m.held.push(Hold {
                id: h.id.clone(),
                wrote_first: 0,
            });
        }
    }
}
