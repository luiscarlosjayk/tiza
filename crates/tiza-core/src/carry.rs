//! Keeping a document consistent when one of its patches is added or
//! deleted (ADR 0007 §3a, §3d).
//!
//! Annotations are written against the code as it is at their own step, and
//! a move's line numbers (`position`, a cursor, a `select` by line) against
//! the code as it is where that move runs. Adding a `code.patch` at step S changes that code for every
//! annotation of its file at step S or later and for every move that runs
//! after it; deleting one changes it back. So the change carries them, by
//! the patch's own effect and never by a text search: every line keeps an
//! identity through the moves (the step-0 line it is, or the patch that
//! wrote it), the moves are replayed with and without the change, and each
//! line number goes to the line with the same identity on the other side.
//! A line the change rewrites keeps its line-level anchors (notes, the ends
//! of a range) at the same offset, and its column anchors only where the
//! line is unchanged from its start up to them. The change is refused,
//! naming what it is about, when carrying would lose something — an
//! anchor's text, the first or last line of a range or a `code.link`
//! window, the line an insertion point follows, a later move's text target
//! that would match another line — or would move a `rebind` line (rare:
//! refused rather than carried). So deleting a move always gives back the
//! document from before it was added.

use std::collections::{BTreeMap, HashMap};

use crate::Error;
use crate::model::{Annotation, Document, Shape};
use crate::moves::{Move, MoveAction, PatchOp, Target, ViewKey, shift_shape_by};
use crate::protocol::Where;
use unicode_segmentation::UnicodeSegmentation;

/// The change to a document's moves: the last move of the new list was
/// appended, or the move at this index of the old list was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Deleted(usize),
}

/// The identity of one line of one view: a step-0 line by its index, or a
/// line a patch wrote, by the patch (its place in the replay) and the
/// line's offset in the patch's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LineId {
    Base(usize),
    Written { slot: usize, k: usize },
}

#[derive(Debug, Clone)]
pub(crate) struct Lines {
    pub(crate) code: Vec<String>,
    pub(crate) ids: Vec<LineId>,
}

/// One side of the replay: every view's code with the identity of each line.
#[derive(Debug, Clone, Default)]
pub(crate) struct World {
    pub(crate) views: BTreeMap<ViewKey, Lines>,
    pub(crate) shown: ViewKey,
}

/// What a patch did while replayed: identities it rewrote, as
/// `(before, after)` pairs at the same offset.
pub(crate) type Rewritten = Vec<(LineId, LineId)>;

/// Where a patch applied: its target and the identities there.
pub(crate) type Hit = (Target, [Option<LineId>; 2]);

impl World {
    pub(crate) fn lines(&mut self, doc: &Document, key: &ViewKey) -> Option<&mut Lines> {
        if !self.views.contains_key(key) {
            let code = doc.view_base_code(key)?;
            let ids = (0..code.len()).map(LineId::Base).collect();
            self.views.insert(key.clone(), Lines { code, ids });
        }
        self.views.get_mut(key)
    }

    /// Replays `m` (the `slot`-th move of the replay) on this side. Only
    /// patches and links change the code; cursors, selections and waits are
    /// checked by [`crate::moves::validate`] afterwards. For a patch, also
    /// where it applied: its target and the identity of the lines there (the
    /// line it inserts before, `None` at the end; the ends of a range).
    pub(crate) fn apply(
        &mut self,
        doc: &Document,
        m: &Move,
        slot: usize,
    ) -> Result<(Rewritten, Option<Hit>), Error> {
        match &m.action {
            MoveAction::CodePatch(p) => {
                let key = self.shown.clone();
                let first = doc.view_first_line(&key);
                let Some(lines) = self.lines(doc, &key) else {
                    return Ok((Vec::new(), None));
                };
                let target = p.resolve(&lines.code, first)?;
                let id = |l: u32| lines.ids.get(l.checked_sub(first)? as usize).copied();
                let hit = match p.op {
                    PatchOp::Insert if p.before.is_some() => [id(target.from), None],
                    // As `Mapper::insertion` holds it: by the line before.
                    PatchOp::Insert => [target.from.checked_sub(1).and_then(id), None],
                    PatchOp::Replace | PatchOp::Delete => [id(target.from), id(target.to)],
                };
                let (code, map) = p.apply_target(&lines.code, first, target);
                let written = |n: u32| LineId::Written {
                    slot,
                    k: (n - map.inserted.start) as usize,
                };
                // A patch that leaves no line leaves one empty line, its own.
                let mut ids: Vec<LineId> = (0..code.len())
                    .map(|i| LineId::Written {
                        slot,
                        k: usize::MAX - i,
                    })
                    .collect();
                for (o, n) in map.map.iter().enumerate() {
                    if let Some(n) = n {
                        ids[(n - first) as usize] = lines.ids[o];
                    }
                }
                for n in map.inserted.clone() {
                    ids[(n - first) as usize] = written(n);
                }
                let rewritten = map
                    .replaced
                    .clone()
                    .filter_map(|o| Some((lines.ids[(o - first) as usize], written(map.get(o)?))))
                    .collect();
                *lines = Lines { code, ids };
                Ok((rewritten, Some((target, hit))))
            }
            MoveAction::CodeLink(l) => {
                let key = doc.view_key(&l.file);
                // A file that is not loaded is reported by the validation.
                let _ = self.lines(doc, &key);
                self.shown = key;
                Ok((Vec::new(), None))
            }
            _ => Ok((Vec::new(), None)),
        }
    }
}

/// Line numbers of one view from the side without the change to the side
/// with it (or the reverse for a deletion: always old → new).
struct Mapper<'a> {
    first: u32,
    old: &'a Lines,
    new: &'a Lines,
    new_at: HashMap<LineId, u32>,
    alias: &'a HashMap<LineId, LineId>,
}

impl<'a> Mapper<'a> {
    fn new(
        doc: &Document,
        key: &ViewKey,
        old: &'a World,
        new: &'a World,
        alias: &'a HashMap<LineId, LineId>,
    ) -> Option<Mapper<'a>> {
        // `None` for a linked file that is not loaded: it is reported by the
        // validation.
        let (o, n) = (old.views.get(key)?, new.views.get(key)?);
        let first = doc.view_first_line(key);
        Some(Mapper {
            first,
            old: o,
            new: n,
            new_at: n
                .ids
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, first + i as u32))
                .collect(),
            alias,
        })
    }

    fn id(&self, line: u32) -> Option<LineId> {
        self.old
            .ids
            .get(line.checked_sub(self.first)? as usize)
            .copied()
    }

    /// The same line on the other side, a rewritten one included.
    fn line(&self, line: u32) -> Option<u32> {
        let id = self.id(line)?;
        self.new_at
            .get(&id)
            .or_else(|| self.alias.get(&id).and_then(|a| self.new_at.get(a)))
            .copied()
    }

    /// The same line for a column anchor standing on its first `upto`
    /// graphemes: a rewritten line keeps it only when the rewrite left that
    /// part of the line as it was (an edit further right does not move it).
    fn column_line(&self, line: u32, upto: u32) -> Option<u32> {
        let id = self.id(line)?;
        if let Some(n) = self.new_at.get(&id) {
            return Some(*n);
        }
        let n = *self.alias.get(&id).and_then(|a| self.new_at.get(a))?;
        let before = &self.old.code[(line - self.first) as usize];
        let after = &self.new.code[(n - self.first) as usize];
        let take = (upto as usize).min(before.graphemes(true).count());
        let same = before
            .graphemes(true)
            .take(take)
            .eq(after.graphemes(true).take(take))
            && after.graphemes(true).count() >= take;
        same.then_some(n)
    }

    /// An insertion point (the content goes before `line`; one past the
    /// last line appends), held by the line before it, which must still be
    /// there (the first line holds itself): what the change puts at that
    /// point goes after the insertion, and deleting the change gives the
    /// point back.
    fn insertion(&self, line: u32) -> Option<u32> {
        if line <= self.first {
            return Some(line);
        }
        Some(self.line(line - 1)? + 1)
    }

    /// A range: both ends must still be there (a range is never trimmed,
    /// so deleting the move gives it back as it was).
    fn range(&self, a: u32, b: u32) -> Option<(u32, u32)> {
        Some((self.line(a)?, self.line(b)?))
    }
}

/// What the change does to the anchor of `shape` it cannot carry.
fn lost_at(shape: &Shape, map: &Mapper) -> String {
    let point = |line: u32, upto: u32, what: &str| {
        if map.line(line).is_none() {
            Some(format!("removes line {line}, where {what} is"))
        } else {
            map.column_line(line, upto)
                .is_none()
                .then(|| format!("rewrites line {line} under {what}"))
        }
    };
    let lost = match shape {
        Shape::Highlight { span, .. } | Shape::Mark { span, .. } => {
            point(span.line, span.end, "the text it marks")
        }
        Shape::Arrow { from, to, .. } => point(from.line, from.col + 1, "its start")
            .or_else(|| point(to.line, to.col + 1, "its end")),
        Shape::Note { line, .. } => point(*line, 0, "it"),
        _ => None,
    };
    lost.or_else(|| {
        shape
            .line_ranges()
            .iter()
            .find_map(|(f, t)| match (map.line(*f), map.line(*t)) {
                (None, _) => Some(format!(
                    "removes line {f}, its first line ({})",
                    lines_text(*f, *t)
                )),
                (_, None) => Some(format!(
                    "removes line {t}, its last line ({})",
                    lines_text(*f, *t)
                )),
                _ => None,
            })
    })
    .unwrap_or_else(|| "removes its lines".into())
}

/// `line 7` or `lines 7-9`.
fn lines_text(a: u32, b: u32) -> String {
    if a == b {
        format!("line {a}")
    } else {
        format!("lines {a}-{b}")
    }
}

/// `a-b` (or `a` when it is one line and `was`, the text carried, was one
/// number), so carrying a value there and back gives it as it was.
pub(crate) fn position_text(was: &str, a: u32, b: u32) -> String {
    if a == b && !was.contains(['-', ':']) {
        a.to_string()
    } else {
        format!("{a}-{b}")
    }
}

/// Parses a `position`, a `code.link` window or a `lines`, `A`, `A-B`, `A:B`.
fn parse_range(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.trim().split(['-', ':']).map(str::trim).collect();
    match parts.as_slice() {
        [a] => {
            let a = a.parse().ok()?;
            Some((a, a))
        }
        [a, b] => Some((a.parse().ok()?, b.parse().ok()?)),
        _ => None,
    }
}

/// Every refusal in one format: `<what it is about>: <the change> <what it
/// does to it>; <what to do>`, e.g. `annotation a1, step 1: move #5 (step
/// 1), the one being added, removes or rewrites line 3, where it is
/// anchored; delete or move a1 first`.
fn refused(about: String, who: &str, does: String, fix: String) -> Error {
    Error::Invalid(format!("{about}: {who} {does}; {fix}"))
}

/// Carries a move's own line numbers, written against the code just
/// before it runs. `index` is the move's number as `move.list` shows it
/// now (0-based).
fn carry_before(m: &Move, index: usize, map: Option<&Mapper>, who: &str) -> Result<Move, Error> {
    let Some(map) = map else {
        return Ok(m.clone());
    };
    let mut m = m.clone();
    let n = index + 1;
    let about = format!("move #{n}, step {}", m.at_step);
    // There is no `move.update`: the move is deleted and added again.
    let fix = format!("delete move #{n} and add it again on the code as it is then");
    let lost = |what: String| refused(about.clone(), who, what, fix.clone());
    match &mut m.action {
        MoveAction::CodePatch(p) => {
            if let Some(pos) = p.position.clone()
                && let Some((a, b)) = parse_range(&pos)
            {
                let (x, y) = match p.op {
                    PatchOp::Insert => {
                        let x = map.insertion(a).ok_or_else(|| {
                            lost(format!("removes line {}, the line it inserts after", a - 1))
                        })?;
                        (x, x)
                    }
                    PatchOp::Replace | PatchOp::Delete => map.range(a, b).ok_or_else(|| {
                        let (l, which) = match map.line(a) {
                            None if a == b => (a, "the line it changes".to_string()),
                            None => (a, format!("the first of the lines it changes ({a}-{b})")),
                            Some(_) => (b, format!("the last of the lines it changes ({a}-{b})")),
                        };
                        lost(format!("removes line {l}, {which}"))
                    })?,
                };
                if (x, y) != (a, b) {
                    p.position = Some(position_text(&pos, x, y));
                }
            }
        }
        MoveAction::Cursor(c) => {
            c.line = map
                .line(c.line)
                .ok_or_else(|| lost(format!("removes its cursor line ({})", c.line)))?;
        }
        MoveAction::Select(s) => {
            for w in [&mut s.from, &mut s.to] {
                if let Some(l) = w.line {
                    w.line = Some(
                        map.line(l)
                            .ok_or_else(|| lost(format!("removes the line it selects ({l})")))?,
                    );
                }
            }
        }
        MoveAction::CodeLink(_) | MoveAction::Wait(_) => {}
    }
    Ok(m)
}

/// Refuses the change when the move it is about cannot be carried: a
/// removed line of `what`.
fn lost_line(m: &Move, index: usize, who: &str, what: String) -> Error {
    let n = index + 1;
    refused(
        format!("move #{n}, step {}", m.at_step),
        who,
        what,
        format!("delete move #{n} and add it again on the code as it is then"),
    )
}

/// The insert being added or deleted, when the change is one: its place in
/// the replay (the slot its lines are written with) and its step.
#[derive(Debug, Clone, Copy)]
struct Insert {
    slot: usize,
    at_step: u32,
    change: Change,
}

/// Where the first line of a window goes when the change is an insert of
/// the link's own step right before that line: the window grows to start
/// at the inserted lines, so they are seen at the step that adds them
/// (with `typing`, typed on screen). Deleting that insert gives the start
/// back to the line after them. `None`: the usual carry.
fn grown_start(a: u32, map: &Mapper, insert: Option<Insert>, link_step: u32) -> Option<u32> {
    let ins = insert.filter(|i| i.at_step == link_step)?;
    let mine =
        |id: Option<LineId>| matches!(id, Some(LineId::Written { slot, .. }) if slot == ins.slot);
    match ins.change {
        Change::Added => {
            // The lines right above the window's first line, written by it.
            let at = |n: u32| map.new.ids.get(n.checked_sub(map.first)? as usize).copied();
            let mut x = map.line(a)?;
            if !mine(x.checked_sub(1).and_then(at)) {
                return None;
            }
            while mine(x.checked_sub(1).and_then(at)) {
                x -= 1;
            }
            Some(x)
        }
        Change::Deleted(_) => {
            // The window starts on the first line it wrote: the line after
            // them takes its place.
            if !mine(map.id(a)) || (a > map.first && mine(map.id(a - 1))) {
                return None;
            }
            let mut l = a;
            while mine(map.id(l)) {
                l += 1;
            }
            map.line(l)
        }
    }
}

/// A `code.link` window, on the file it shows at the end of its step: both
/// ends must still be there.
fn carry_window(
    m: &mut Move,
    index: usize,
    map: Option<&Mapper>,
    who: &str,
    insert: Option<Insert>,
) -> Result<(), Error> {
    let Some(map) = map else { return Ok(()) };
    let at = m.clone();
    if let MoveAction::CodeLink(l) = &mut m.action
        && let Some(w) = l.lines.clone()
        && let Some((a, b)) = parse_range(&w)
    {
        let start = grown_start(a, map, insert, at.at_step).or_else(|| map.line(a));
        match (start, map.line(b)) {
            (Some(x), Some(y)) => l.lines = Some(position_text(&w, x, y)),
            (None, _) => {
                let what = format!("removes line {a}, the first line of its window");
                return Err(lost_line(&at, index, who, what));
            }
            (_, None) => {
                let what = format!("removes line {b}, the last line of its window");
                return Err(lost_line(&at, index, who, what));
            }
        }
    }
    Ok(())
}

/// The `rebind` lines of a patch (on the code right after it) are rare, so
/// the change is refused when it would move one. An entry whose annotation
/// is not in `annotations` is ignored: its lines are carried instead, so it
/// keeps pointing at its text if that annotation comes back, and one whose
/// line the change removes stays as written.
fn carry_rebind(
    m: &mut Move,
    index: usize,
    map: Option<&Mapper>,
    who: &str,
    annotations: &[Annotation],
) -> Result<(), Error> {
    let Some(map) = map else { return Ok(()) };
    let mut rebind = std::mem::take(&mut m.rebind);
    for (id, spec) in rebind.iter_mut() {
        let ignored = !annotations.iter().any(|a| &a.id == id);
        let wheres = [&mut spec.at, &mut spec.from, &mut spec.to]
            .into_iter()
            .flatten()
            .map(|w: &mut Where| &mut w.line);
        let lines = wheres.chain([&mut spec.from_line, &mut spec.to_line]);
        for line in lines {
            let Some(l) = *line else { continue };
            match map.line(l) {
                Some(now) if ignored => *line = Some(now),
                _ if ignored => {}
                Some(now) if now == l => {}
                _ => {
                    let what = format!("moves line {l}, where its rebind puts {id}");
                    return Err(lost_line(m, index, who, what));
                }
            }
        }
    }
    m.rebind = rebind;
    Ok(())
}

/// The change, as the messages name it: `move #9 (step 1), the one being
/// added,`, `deleting move #3 (step 2)`.
fn who(change: Change, old: &[Move], new: &[Move]) -> String {
    match change {
        Change::Added => format!(
            "move #{} (step {}), the one being added,",
            new.len(),
            new.last().map_or(0, |m| m.at_step)
        ),
        Change::Deleted(i) => format!(
            "deleting move #{} (step {})",
            i + 1,
            old.get(i).map_or(0, |m| m.at_step)
        ),
    }
}

/// `doc`'s moves are `old`; `new` is `old` with the change applied. Returns
/// `new` with every move's line numbers carried and `doc`'s annotations
/// carried (in their order). An annotation or a move the change would
/// leave without its line is an error naming it.
pub fn carry(
    doc: &Document,
    old: &[Move],
    new: &[Move],
    change: Change,
) -> Result<(Vec<Move>, Vec<Annotation>), Error> {
    // The replay order of both lists at once (step, then list order, as
    // `moves::project`): `(index in old, index in new)`.
    let mut order: Vec<(Option<usize>, Option<usize>)> = match change {
        Change::Added => (0..old.len())
            .map(|i| (Some(i), Some(i)))
            .chain(std::iter::once((None, Some(old.len()))))
            .collect(),
        Change::Deleted(d) => (0..old.len())
            .map(|i| {
                (
                    Some(i),
                    match i.cmp(&d) {
                        std::cmp::Ordering::Less => Some(i),
                        std::cmp::Ordering::Equal => None,
                        std::cmp::Ordering::Greater => Some(i - 1),
                    },
                )
            })
            .collect(),
    };
    let step_of = |(o, n): &(Option<usize>, Option<usize>)| match (o, n) {
        (Some(i), _) => (old[*i].at_step, *i),
        (None, Some(i)) => (new[*i].at_step, *i),
        (None, None) => (u32::MAX, usize::MAX),
    };
    order.sort_by_key(step_of);
    let insert = order.iter().enumerate().find_map(|(slot, (o, n))| {
        let m = match (o, n) {
            (None, Some(n)) => &new[*n],
            (Some(o), None) => &old[*o],
            _ => return None,
        };
        matches!(&m.action, MoveAction::CodePatch(p) if p.op == PatchOp::Insert).then_some(Insert {
            slot,
            at_step: m.at_step,
            change,
        })
    });

    let mut out: Vec<Move> = new.to_vec();
    let mut r = Replay {
        doc,
        who: who(change, old, new),
        anns: doc.annotations.clone(),
        by_step: (0..doc.annotations.len()).collect(),
        next: 0,
        old: World::default(),
        new: World::default(),
        alias: HashMap::new(),
        insert,
    };
    r.by_step.sort_by_key(|i| r.anns[*i].step);
    let named = |e: Error, i: usize, m: &Move| crate::moves::name_move(e, i, m.at_step);
    // `code.link` windows crop the code at the end of their step: carried
    // once every move of that step has run on both sides.
    let mut windows: Vec<(usize, usize)> = Vec::new();
    for (slot, item) in order.iter().enumerate() {
        let (step, _) = step_of(item);
        r.carry_annotations(step.saturating_sub(1))?;
        r.carry_windows(&mut windows, step, &mut out)?;
        if let (Some(o), Some(n)) = *item
            && matches!(old[o].action, MoveAction::CodeLink(_))
            && old[o].stale.is_none()
        {
            // A link that another one of its step follows is never shown:
            // its window is left as it is.
            windows.retain(|(_, w)| new[*w].at_step != new[n].at_step);
            if matches!(&new[n].action, MoveAction::CodeLink(l) if l.lines.is_some()) {
                windows.push((o, n));
            }
        }
        match *item {
            // A stale move is not applied (crate::stale), so it is neither
            // replayed nor carried: it keeps its numbers.
            (Some(o), _) if old[o].stale.is_some() => {}
            (Some(o), Some(n)) => {
                let shown = r.old.shown.clone();
                r.load(&shown);
                let before = Mapper::new(doc, &shown, &r.old, &r.new, &r.alias);
                let mut m = carry_before(&new[n], o, before.as_ref(), &r.who)?;
                let (_, was) = r
                    .old
                    .apply(doc, &old[o], slot)
                    .map_err(|e| named(e, o, &old[o]))?;
                // It applied before the change: if it fails now, the change
                // is why (its text target gone, or matching twice).
                let (_, now) = r.new.apply(doc, &m, slot).map_err(|e| Error::Because {
                    message: refused(
                        format!("move #{}, step {}", o + 1, m.at_step),
                        &r.who,
                        format!("makes it fail ({e})"),
                        format!(
                            "delete move #{} and add it again with `occurrence` or a longer text",
                            o + 1
                        ),
                    )
                    .to_string(),
                    inner: Box::new(e),
                })?;
                // It must apply on the same lines as before, carried: a text
                // target the change makes match another line (an explicit
                // `occurrence` counts the new text too) would move in silence.
                if let (Some((_, iw)), Some((_, inow))) = (was, now)
                    && iw.map(|i| i.map(|i| *r.alias.get(&i).unwrap_or(&i))) != inow
                {
                    return Err(refused(
                        format!("move #{}, step {}", o + 1, m.at_step),
                        &r.who,
                        "makes its text target match another line with the same text".into(),
                        format!(
                            "delete move #{} and add it again with `occurrence` or a longer text",
                            o + 1
                        ),
                    ));
                }
                let shown = r.new.shown.clone();
                r.load(&shown);
                let after = Mapper::new(doc, &shown, &r.old, &r.new, &r.alias);
                carry_rebind(&mut m, o, after.as_ref(), &r.who, &doc.annotations)?;
                out[n] = m;
            }
            (None, Some(n)) => {
                let (rewritten, _) = r
                    .new
                    .apply(doc, &new[n], slot)
                    .map_err(|e| named(e, n, &new[n]))?;
                r.alias.extend(rewritten);
            }
            (Some(o), None) => {
                let (rewritten, _) = r
                    .old
                    .apply(doc, &old[o], slot)
                    .map_err(|e| named(e, o, &old[o]))?;
                r.alias.extend(rewritten.into_iter().map(|(b, a)| (a, b)));
            }
            (None, None) => {}
        }
    }
    r.carry_annotations(u32::MAX)?;
    r.carry_windows(&mut windows, u32::MAX, &mut out)?;
    Ok((out, r.anns))
}

/// The replay in progress: both sides, the identities the change rewrote
/// (old side → new side) and the annotations, carried in step order.
struct Replay<'d> {
    doc: &'d Document,
    who: String,
    anns: Vec<Annotation>,
    by_step: Vec<usize>,
    next: usize,
    old: World,
    new: World,
    alias: HashMap<LineId, LineId>,
    insert: Option<Insert>,
}

impl Replay<'_> {
    /// Carries the windows of the links in `pending` whose step ends before
    /// `step` (both ends must still be there).
    fn carry_windows(
        &mut self,
        pending: &mut Vec<(usize, usize)>,
        step: u32,
        out: &mut [Move],
    ) -> Result<(), Error> {
        let (done, wait): (Vec<_>, Vec<_>) =
            pending.drain(..).partition(|(_, n)| out[*n].at_step < step);
        *pending = wait;
        for (o, n) in done {
            let MoveAction::CodeLink(l) = &out[n].action else {
                continue;
            };
            let key = self.doc.view_key(&l.file);
            self.load(&key);
            let map = Mapper::new(self.doc, &key, &self.old, &self.new, &self.alias);
            carry_window(&mut out[n], o, map.as_ref(), &self.who, self.insert)?;
        }
        Ok(())
    }

    /// A view on both sides, as its step-0 code where nothing touched it yet.
    fn load(&mut self, key: &ViewKey) {
        let _ = self.old.lines(self.doc, key);
        let _ = self.new.lines(self.doc, key);
    }

    /// Carries the annotations of every step up to `upto`: every move of
    /// those steps has run on both sides, so their code is final there. The
    /// ends of a range must stay (a box is never trimmed).
    fn carry_annotations(&mut self, upto: u32) -> Result<(), Error> {
        while self.next < self.by_step.len() && self.anns[self.by_step[self.next]].step <= upto {
            let i = self.by_step[self.next];
            self.next += 1;
            let file = self.anns[i].file.clone();
            self.load(&file);
            let a = &self.anns[i];
            let Some(map) = Mapper::new(self.doc, &file, &self.old, &self.new, &self.alias) else {
                continue;
            };
            // The change did not touch this file up to this step: nothing to
            // carry (an annotation already outside the code stays as it was).
            if map.old.ids == map.new.ids {
                continue;
            }
            let ranges = a.shape.line_ranges();
            // Already outside the code at its step (a document that came
            // that way): nothing to carry, and nothing this change removed.
            if let Some(l) = ranges
                .iter()
                .flat_map(|(f, t)| [*f, *t])
                .find(|l| map.id(*l).is_none())
            {
                let last = map.first + map.old.ids.len() as u32 - 1;
                return Err(refused(
                    format!("annotation {}, step {}", a.id, a.step),
                    &self.who,
                    format!(
                        "cannot carry it: it was already outside the code (line {l}; {} has lines {}-{last} at step {})",
                        self.doc.file_label(&a.file),
                        map.first,
                        a.step
                    ),
                    format!("move or delete {} first", a.id),
                ));
            }
            let ends_kept = ranges.iter().all(|(f, t)| map.range(*f, *t).is_some());
            let shape = ends_kept
                .then(|| {
                    shift_shape_by(&a.shape, &|l| map.line(l), &|l, upto| {
                        map.column_line(l, upto)
                    })
                })
                .flatten();
            let Some(shape) = shape else {
                return Err(refused(
                    format!("annotation {}, step {}", a.id, a.step),
                    &self.who,
                    lost_at(&a.shape, &map),
                    format!("delete or move {} first", a.id),
                ));
            };
            self.anns[i].shape = shape;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::fs::parse_document;
    use crate::moves::Move;

    fn doc() -> crate::Document {
        parse_document(
            &serde_json::json!({
                "schema": 1,
                "source": {"language": "rs"},
                "code": ["fn main() {", "    let a = 1;", "    let b = 2;", "    show(a, b);", "}"],
                "moves": [
                    {"at_step": 2, "action": "code.patch", "op": "insert", "position": "5", "content": "    done();"},
                    {"at_step": 3, "action": "code.patch", "op": "delete", "position": "4"}
                ],
                "annotations": [
                    {"id": "m1", "step": 1, "type": "mark", "kind": "read", "span": {"line": 3, "start": 8, "end": 9}},
                    {"id": "n3", "step": 3, "type": "note", "line": 4, "text": "done"}
                ]
            })
            .to_string(),
            None,
        )
        .unwrap()
    }

    fn patch(v: serde_json::Value) -> Move {
        serde_json::from_value(v).unwrap()
    }

    fn json(d: &crate::Document) -> serde_json::Value {
        serde_json::to_value(d).unwrap()
    }

    #[test]
    fn an_insert_carries_later_annotations_and_moves_and_its_delete_carries_them_back() {
        let mut d = doc();
        let before = json(&d);
        d.add_move(patch(serde_json::json!(
            {"at_step": 1, "action": "code.patch", "op": "insert", "position": "2", "content": "    // one\n    // two"}
        )))
        .unwrap();
        let j = json(&d);
        assert_eq!(j["annotations"][0]["span"]["line"], 5, "{j}");
        assert_eq!(j["annotations"][1]["line"], 6, "{j}");
        assert_eq!(j["moves"][0]["position"], "7", "{j}");
        assert_eq!(j["moves"][1]["position"], "6", "{j}");
        for a in &d.annotations {
            d.validate_annotation(a).unwrap();
        }
        d.delete_move(2).unwrap();
        assert_eq!(json(&d), before);
    }

    #[test]
    fn removing_the_text_of_a_later_annotation_or_move_is_refused_naming_it() {
        let mut d = doc();
        let before = json(&d);
        let e = d
            .add_move(patch(serde_json::json!(
                {"at_step": 1, "action": "code.patch", "op": "delete", "position": "3"}
            )))
            .unwrap_err()
            .to_string();
        assert!(
            e == "annotation m1, step 1: move #3 (step 1), the one being added, removes line 3, where the text it marks is; delete or move m1 first",
            "{e}"
        );
        let e = d
            .add_move(patch(serde_json::json!(
                {"at_step": 2, "action": "code.patch", "op": "delete", "find": "show(a, b);"}
            )))
            .unwrap_err()
            .to_string();
        assert!(
            e == "move #2, step 3: move #3 (step 2), the one being added, removes line 4, the line it changes; delete move #2 and add it again on the code as it is then",
            "{e}"
        );
        assert_eq!(json(&d), before);
    }

    #[test]
    fn a_rewritten_line_keeps_a_mark_whose_text_is_unchanged_up_to_it() {
        let mut d = doc();
        d.add_move(patch(serde_json::json!(
            {"at_step": 1, "action": "code.patch", "op": "replace", "find": "let b = 2;", "content": "    let b = 2; // two"}
        )))
        .unwrap();
        assert_eq!(json(&d)["annotations"][0]["span"]["line"], 3);
        let mut d = doc();
        let e = d
            .add_move(patch(serde_json::json!(
                {"at_step": 1, "action": "code.patch", "op": "replace", "find": "let b = 2;", "content": "    let c = 2;"}
            )))
            .unwrap_err()
            .to_string();
        assert!(e.starts_with("annotation m1, step 1: "), "{e}");
    }

    #[test]
    fn a_later_text_target_that_would_match_another_line_is_refused() {
        let mut d = parse_document(
            &serde_json::json!({
                "schema": 1,
                "source": {"language": "rs"},
                "code": ["a();", "b();", "c();"],
                "moves": [
                    {"at_step": 2, "action": "code.patch", "op": "insert", "after": "b();", "occurrence": 1, "content": "x();"}
                ]
            })
            .to_string(),
            None,
        )
        .unwrap();
        let e = d
            .add_move(patch(serde_json::json!(
                {"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "b();"}
            )))
            .unwrap_err()
            .to_string();
        assert!(
            e == "move #1, step 2: move #2 (step 1), the one being added, makes its text target match another line with the same text; delete move #1 and add it again with `occurrence` or a longer text",
            "{e}"
        );
    }

    fn refusal(d: &mut crate::Document, m: serde_json::Value) -> String {
        let before = json(d);
        let e = d.add_move(patch(m)).unwrap_err().to_string();
        assert_eq!(json(d), before, "a refused move changes nothing");
        e
    }

    #[test]
    fn a_range_is_never_trimmed_and_a_later_range_takes_lines_inserted_inside() {
        let mut d = parse_document(
            &serde_json::json!({
                "schema": 1,
                "source": {"language": "rs"},
                "code": ["a();", "b();", "c();", "d();", "e();"],
                "moves": [
                    {"at_step": 3, "action": "code.patch", "op": "delete", "position": "2-4"}
                ],
                "annotations": [
                    {"id": "b1", "step": 2, "type": "box", "from_line": 1, "to_line": 2}
                ]
            })
            .to_string(),
            None,
        )
        .unwrap();
        let e = refusal(
            &mut d,
            serde_json::json!({"at_step": 1, "action": "code.patch", "op": "delete", "position": "2"}),
        );
        assert!(
            e.starts_with("annotation b1, step 2: move #2 (step 1), the one being added, removes line 2, its last line (lines 1-2)"),
            "{e}"
        );
        let before = json(&d);
        d.add_move(patch(serde_json::json!(
            {"at_step": 1, "action": "code.patch", "op": "insert", "position": "3", "content": "x();"}
        )))
        .unwrap();
        assert_eq!(json(&d)["moves"][0]["position"], "2-5");
        assert_eq!(d.code_at(3).unwrap().0, vec!["a();", "e();"]);
        d.delete_move(1).unwrap();
        assert_eq!(json(&d), before);
    }

    #[test]
    fn a_window_is_carried_and_a_rebind_line_the_change_would_move_is_refused() {
        let mut d = parse_document(
            &serde_json::json!({
                "schema": 1,
                "source": {"language": "rs", "path": "/x/main.rs"},
                "code": ["a();", "b();", "c();"],
                "moves": [
                    {"at_step": 2, "action": "code.patch", "op": "replace", "position": "3", "content": "C();",
                     "rebind": {"n1": {"from_line": 3, "to_line": 3}}},
                    {"at_step": 3, "action": "code.link", "file": "main.rs", "lines": "2-3"}
                ],
                "annotations": [{"id": "n1", "step": 1, "type": "box", "from_line": 3, "to_line": 3}]
            })
            .to_string(),
            None,
        )
        .unwrap();
        let e = refusal(
            &mut d,
            serde_json::json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "z();"}),
        );
        assert!(
            e.starts_with("move #1, step 2: move #3 (step 1), the one being added, moves line 3, where its rebind puts n1"),
            "{e}"
        );
        d.moves[0].rebind.clear();
        let before = json(&d);
        d.add_move(patch(serde_json::json!(
            {"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "z();"}
        )))
        .unwrap();
        assert_eq!(json(&d)["moves"][1]["lines"], "3-4");
        d.delete_move(2).unwrap();
        assert_eq!(json(&d), before);
        let e = refusal(
            &mut d,
            serde_json::json!({"at_step": 1, "action": "code.patch", "op": "delete", "position": "2"}),
        );
        assert!(
            e.contains("removes line 2, the first line of its window"),
            "{e}"
        );
    }

    #[test]
    fn an_insert_at_a_window_first_line_at_its_step_grows_it_and_its_delete_shrinks_it() {
        let doc = |moves: serde_json::Value| {
            parse_document(
                &serde_json::json!({
                    "schema": 1,
                    "source": {"language": "rs", "path": "/x/main.rs"},
                    "code": ["a();", "b();", "c();", "d();", "e();"],
                    "moves": moves
                })
                .to_string(),
                None,
            )
            .unwrap()
        };
        let link = serde_json::json!({"at_step": 2, "action": "code.link", "file": "main.rs", "lines": "2-4"});
        let mut d = doc(serde_json::json!([link]));
        let before = json(&d);
        let insert = |step: u32| {
            patch(serde_json::json!(
                {"at_step": step, "action": "code.patch", "op": "insert", "position": "2", "content": "x();\ny();"}
            ))
        };
        // At the link's step: the new lines are inside the window.
        d.add_move(insert(2)).unwrap();
        assert_eq!(json(&d)["moves"][0]["lines"], "2-6");
        assert_eq!(
            d.code_at(2).unwrap(),
            (
                vec![
                    "x();".into(),
                    "y();".into(),
                    "b();".into(),
                    "c();".into(),
                    "d();".into()
                ],
                2
            )
        );
        d.delete_move(1).unwrap();
        assert_eq!(json(&d), before);
        // At an earlier step: the window follows its text.
        d.add_move(insert(1)).unwrap();
        assert_eq!(json(&d)["moves"][0]["lines"], "4-6");
        d.delete_move(1).unwrap();
        assert_eq!(json(&d), before);
        // A window set on the lines an insert of its step wrote: deleting
        // the insert starts it at the line after them, and adding the
        // insert again gives the same document back.
        let mut d = doc(serde_json::json!([
            {"at_step": 2, "action": "code.link", "file": "main.rs", "lines": "2-6"},
            {"at_step": 2, "action": "code.patch", "op": "insert", "position": "2", "content": "x();\ny();"}
        ]));
        let with = json(&d);
        d.delete_move(1).unwrap();
        assert_eq!(json(&d)["moves"][0]["lines"], "2-4");
        d.add_move(insert(2)).unwrap();
        assert_eq!(json(&d), with);
    }

    #[test]
    fn the_window_of_a_link_its_step_replaces_is_left_alone() {
        let mut d = parse_document(
            &serde_json::json!({
                "schema": 1,
                "source": {"language": "rs", "path": "/x/main.rs"},
                "code": ["a();", "b();", "c();", "d();"],
                "moves": [
                    {"at_step": 2, "action": "code.link", "file": "main.rs", "lines": "2-3"},
                    {"at_step": 2, "action": "code.link", "file": "main.rs", "lines": "1-3"}
                ]
            })
            .to_string(),
            None,
        )
        .unwrap();
        let before = json(&d);
        d.add_move(patch(serde_json::json!(
            {"at_step": 1, "action": "code.patch", "op": "delete", "position": "2"}
        )))
        .unwrap();
        let j = json(&d);
        assert_eq!(j["moves"][0]["lines"], "2-3", "{j}");
        assert_eq!(j["moves"][1]["lines"], "1-2", "{j}");
        d.delete_move(2).unwrap();
        assert_eq!(json(&d), before);
    }
}
