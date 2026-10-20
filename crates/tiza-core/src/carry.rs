//! Keeping a document consistent when one of its patches is added or
//! deleted (ADR 0007 §3a, §3d).
//!
//! Annotations are written against the code as it is at their own step, and
//! a move's line numbers (`position`, a cursor, a `select` or `rebind` by
//! line, a `code.link` window) against the code as it is where that move
//! runs. Adding a `code.patch` at step S changes that code for every
//! annotation of its file at step S or later and for every move that runs
//! after it; deleting one changes it back. So the change carries them, by
//! the patch's own effect and never by a text search: every line keeps an
//! identity through the moves (the step-0 line it is, or the patch that
//! wrote it), the moves are replayed with and without the change, and each
//! line number goes to the line with the same identity on the other side.
//! A line the change rewrites keeps its line-level anchors (notes, the ends
//! of a range) at the same offset, and its column anchors only where the
//! line is unchanged from its start up to them. Something whose text the
//! change removes cannot be carried, nor a later move whose text target the
//! change would make match another line: the change is refused, naming it.

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
enum LineId {
    Base(usize),
    Written { slot: usize, k: usize },
}

#[derive(Debug, Clone)]
struct Lines {
    code: Vec<String>,
    ids: Vec<LineId>,
}

/// One side of the replay: every view's code with the identity of each line.
#[derive(Debug, Clone, Default)]
struct World {
    views: BTreeMap<ViewKey, Lines>,
    shown: ViewKey,
}

/// What a patch did while replayed: identities it rewrote, as
/// `(before, after)` pairs at the same offset.
type Rewritten = Vec<(LineId, LineId)>;

impl World {
    fn lines(&mut self, doc: &Document, key: &ViewKey) -> Option<&mut Lines> {
        if !self.views.contains_key(key) {
            let code = doc.view_base_code(key)?;
            let ids = (0..code.len()).map(LineId::Base).collect();
            self.views.insert(key.clone(), Lines { code, ids });
        }
        self.views.get_mut(key)
    }

    /// Replays `m` (the `slot`-th move of the replay) on this side. Only
    /// patches and links change the code; cursors, selections and waits are
    /// checked by [`crate::moves::validate`] afterwards.
    fn apply(&mut self, doc: &Document, m: &Move, slot: usize) -> Result<Rewritten, Error> {
        Ok(self.apply_at(doc, m, slot)?.0)
    }

    /// The line a patch applies to on this side, without applying it.
    fn target(&mut self, doc: &Document, m: &Move) -> Result<Option<Target>, Error> {
        let MoveAction::CodePatch(p) = &m.action else {
            return Ok(None);
        };
        let key = self.shown.clone();
        let first = doc.view_first_line(&key);
        match self.lines(doc, &key) {
            Some(lines) => p.resolve(&lines.code, first).map(Some),
            None => Ok(None),
        }
    }

    /// [`World::apply`], with the place a patch applied at.
    fn apply_at(
        &mut self,
        doc: &Document,
        m: &Move,
        slot: usize,
    ) -> Result<(Rewritten, Option<Target>), Error> {
        match &m.action {
            MoveAction::CodePatch(p) => {
                let key = self.shown.clone();
                let first = doc.view_first_line(&key);
                let Some(lines) = self.lines(doc, &key) else {
                    return Ok((Vec::new(), None));
                };
                let target = p.resolve(&lines.code, first)?;
                let (code, map) = p.apply_target(&lines.code, first, target);
                let mut ids: Vec<Option<LineId>> = vec![None; code.len()];
                for (o, n) in map.map.iter().enumerate() {
                    if let Some(n) = n
                        && !map.inserted.contains(n)
                        && let Some(slot_id) = ids.get_mut((n - first) as usize)
                    {
                        *slot_id = Some(lines.ids[o]);
                    }
                }
                let mut rewritten = Vec::new();
                for n in map.inserted.clone() {
                    let id = LineId::Written {
                        slot,
                        k: (n - map.inserted.start) as usize,
                    };
                    if let Some(s) = ids.get_mut((n - first) as usize) {
                        *s = Some(id);
                    }
                }
                for o in map.replaced.clone() {
                    if let Some(n) = map.get(o) {
                        rewritten.push((
                            lines.ids[(o - first) as usize],
                            LineId::Written {
                                slot,
                                k: (n - map.inserted.start) as usize,
                            },
                        ));
                    }
                }
                // A patch that leaves no line leaves one empty line.
                let ids = ids
                    .into_iter()
                    .enumerate()
                    .map(|(i, id)| {
                        id.unwrap_or(LineId::Written {
                            slot,
                            k: usize::MAX - i,
                        })
                    })
                    .collect();
                *lines = Lines { code, ids };
                Ok((rewritten, Some(target)))
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
    /// last line appends): before the first line from there on that is
    /// still there, else at the end.
    fn insertion(&self, line: u32) -> u32 {
        let last = self.first + self.old.ids.len() as u32;
        (line..last)
            .find_map(|l| self.line(l))
            .unwrap_or(self.first + self.new.ids.len() as u32)
    }

    /// A range: from its first line still there to its last one.
    fn range(&self, a: u32, b: u32) -> Option<(u32, u32)> {
        let x = (a..=b).find_map(|l| self.line(l))?;
        let y = (a..=b).rev().find_map(|l| self.line(l))?;
        Some((x, y))
    }
}

/// The line (or range) of `shape` that `map` cannot carry, for the message.
fn lost_at(shape: &Shape, map: &Mapper) -> String {
    let column = |line: u32, upto: u32| map.column_line(line, upto).is_none().then_some(line);
    let lost = match shape {
        Shape::Highlight { span, .. } | Shape::Mark { span, .. } => column(span.line, span.end),
        Shape::Arrow { from, to, .. } => column(from.line, from.col.saturating_add(1))
            .or_else(|| column(to.line, to.col.saturating_add(1))),
        Shape::Note { line, .. } => Some(*line),
        _ => None,
    };
    match lost {
        Some(l) => lines_text(l, l),
        None => shape
            .line_ranges()
            .first()
            .map_or_else(|| "its lines".into(), |(f, t)| lines_text(*f, *t)),
    }
}

fn lines_text(a: u32, b: u32) -> String {
    if a == b {
        format!("line {a}")
    } else {
        format!("lines {a}-{b}")
    }
}

/// `a-b` (or `a` when it is one line and `was`, the text carried, was one
/// number), so carrying a value there and back gives it as it was.
fn position_text(was: &str, a: u32, b: u32) -> String {
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

/// The change, as the messages name it: `move #9 (step 1), the one being
/// added`, `deleting move #3 (step 2)`.
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

/// Carries a move's own line numbers, written against the code just
/// before it runs (`rebind` lines are carried after, see
/// [`carry_rebind`]). `index` is the move's number as `move.list` shows it
/// now (0-based).
fn carry_before(m: &Move, index: usize, map: Option<&Mapper>, who: &str) -> Result<Move, Error> {
    let Some(map) = map else {
        return Ok(m.clone());
    };
    let mut m = m.clone();
    let lost = |what: String| {
        Error::Invalid(format!(
            "move #{n}, step {step}: {who} removes {what}; change or delete move #{n} first",
            n = index + 1,
            step = m.at_step,
        ))
    };
    match &mut m.action {
        MoveAction::CodePatch(p) => {
            if let Some(pos) = p.position.clone()
                && let Some((a, b)) = parse_range(&pos)
            {
                let (x, y) = match p.op {
                    PatchOp::Insert => {
                        let x = map.insertion(a);
                        (x, x)
                    }
                    PatchOp::Replace | PatchOp::Delete => map
                        .range(a, b)
                        .ok_or_else(|| lost(format!("its target ({})", lines_text(a, b))))?,
                };
                if (x, y) != (a, b) {
                    p.position = Some(position_text(&pos, x, y));
                }
            }
        }
        MoveAction::Cursor(c) => {
            c.line = map
                .line(c.line)
                .ok_or_else(|| lost(format!("its cursor line ({})", c.line)))?;
        }
        MoveAction::Select(s) => {
            for w in [&mut s.from, &mut s.to] {
                if let Some(l) = w.line {
                    w.line = Some(
                        map.line(l)
                            .ok_or_else(|| lost(format!("the line it selects ({l})")))?,
                    );
                }
            }
        }
        MoveAction::CodeLink(_) | MoveAction::Wait(_) => {}
    }
    Ok(m)
}

/// A `code.link` window, on the file it shows (a window that no longer
/// holds any line is left as it is: a window only crops).
fn carry_window(m: &mut Move, map: Option<&Mapper>) {
    let (Some(map), MoveAction::CodeLink(l)) = (map, &mut m.action) else {
        return;
    };
    if let Some(w) = l.lines.clone()
        && let Some((a, b)) = parse_range(&w)
        && let Some((x, y)) = map.range(a, b)
        && (x, y) != (a, b)
    {
        l.lines = Some(position_text(&w, x, y));
    }
}

/// `rebind` lines are written against the code right after their patch.
fn carry_rebind(m: &mut Move, map: Option<&Mapper>) {
    let Some(map) = map else { return };
    let carry = |w: &mut Option<Where>| {
        if let Some(w) = w
            && let Some(l) = w.line
            && let Some(n) = map.line(l)
        {
            w.line = Some(n);
        }
    };
    for spec in m.rebind.values_mut() {
        carry(&mut spec.at);
        carry(&mut spec.from);
        carry(&mut spec.to);
        for l in [&mut spec.from_line, &mut spec.to_line]
            .into_iter()
            .flatten()
        {
            if let Some(n) = map.line(*l) {
                *l = n;
            }
        }
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

    let mut out: Vec<Move> = new.to_vec();
    let mut r = Replay {
        doc,
        who: who(change, old, new),
        added: change == Change::Added,
        anns: doc.annotations.clone(),
        by_step: (0..doc.annotations.len()).collect(),
        next: 0,
        old: World::default(),
        new: World::default(),
        alias: HashMap::new(),
    };
    r.by_step.sort_by_key(|i| r.anns[*i].step);
    let named = |e: Error, i: usize, m: &Move| crate::moves::name_move(e, i, m.at_step);
    for (slot, item) in order.iter().enumerate() {
        let (step, _) = step_of(item);
        r.carry_annotations(step.saturating_sub(1))?;
        match *item {
            (Some(o), Some(n)) => {
                let shown = r.old.shown.clone();
                r.load(&shown);
                let was = r
                    .old
                    .target(doc, &old[o])
                    .map_err(|e| named(e, o, &old[o]))?;
                let before = Mapper::new(doc, &shown, &r.old, &r.new, &r.alias);
                let mut m = carry_before(&new[n], o, before.as_ref(), &r.who)?;
                // Where it must apply now: where it applied, carried.
                let expected = match (&m.action, was, &before) {
                    (MoveAction::CodePatch(p), Some(t), Some(b)) => Some(match p.op {
                        PatchOp::Insert => Some((b.insertion(t.from), None)),
                        PatchOp::Replace | PatchOp::Delete => {
                            b.range(t.from, t.to).map(|(x, y)| (x, Some(y)))
                        }
                    }),
                    _ => None,
                };
                r.old
                    .apply(doc, &old[o], slot)
                    .map_err(|e| named(e, o, &old[o]))?;
                let (_, now) = r.new.apply_at(doc, &m, slot).map_err(|e| named(e, o, &m))?;
                if let (Some(expected), Some(now), Some(was)) = (expected, now, was) {
                    let got = match &m.action {
                        MoveAction::CodePatch(p) if p.op == PatchOp::Insert => (now.from, None),
                        _ => (now.from, Some(now.to)),
                    };
                    if expected != Some(got) {
                        let at = |t: Target| match &m.action {
                            MoveAction::CodePatch(p) if p.op == PatchOp::Insert => {
                                format!("before line {}", t.from)
                            }
                            _ => lines_text(t.from, t.to),
                        };
                        return Err(Error::Invalid(format!(
                            "move #{n}, step {step}: {who} makes its text target match another line ({now} instead of {was}); change move #{n} first (`occurrence`, a longer text)",
                            n = o + 1,
                            step = m.at_step,
                            who = r.who,
                            now = at(now),
                            was = at(was),
                        )));
                    }
                }
                let shown = r.new.shown.clone();
                r.load(&shown);
                let after = Mapper::new(doc, &shown, &r.old, &r.new, &r.alias);
                carry_window(&mut m, after.as_ref());
                carry_rebind(&mut m, after.as_ref());
                out[n] = m;
            }
            (None, Some(n)) => {
                let rewritten = r
                    .new
                    .apply(doc, &new[n], slot)
                    .map_err(|e| named(e, n, &new[n]))?;
                for (before, after) in rewritten {
                    r.alias.insert(before, after);
                }
            }
            (Some(o), None) => {
                let rewritten = r
                    .old
                    .apply(doc, &old[o], slot)
                    .map_err(|e| named(e, o, &old[o]))?;
                for (before, after) in rewritten {
                    r.alias.insert(after, before);
                }
            }
            (None, None) => {}
        }
    }
    r.carry_annotations(u32::MAX)?;
    Ok((out, r.anns))
}

/// The replay in progress: both sides, the identities the change rewrote
/// (old side → new side) and the annotations, carried in step order.
struct Replay<'d> {
    doc: &'d Document,
    who: String,
    added: bool,
    anns: Vec<Annotation>,
    by_step: Vec<usize>,
    next: usize,
    old: World,
    new: World,
    alias: HashMap<LineId, LineId>,
}

impl Replay<'_> {
    /// A view on both sides, as its step-0 code where nothing touched it yet.
    fn load(&mut self, key: &ViewKey) {
        let _ = self.old.lines(self.doc, key);
        let _ = self.new.lines(self.doc, key);
    }

    /// Carries the annotations of every step up to `upto`: every move of
    /// those steps has run on both sides, so their code is final there.
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
            let Some(shape) = shift_shape_by(&a.shape, &|l| map.line(l), &|l, upto| {
                map.column_line(l, upto)
            }) else {
                let at = lost_at(&a.shape, &map);
                let later = if self.added {
                    format!(", or add the move at a step after {}", a.step)
                } else {
                    String::new()
                };
                return Err(Error::Invalid(format!(
                    "annotation {id}, step {step}: {who} removes or rewrites {at}, where it is anchored; delete or move {id} first{later}",
                    who = self.who,
                    id = a.id,
                    step = a.step,
                )));
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
            e.starts_with("annotation m1, step 1: move #3 (step 1), the one being added, removes or rewrites line 3"),
            "{e}"
        );
        let e = d
            .add_move(patch(serde_json::json!(
                {"at_step": 2, "action": "code.patch", "op": "delete", "find": "show(a, b);"}
            )))
            .unwrap_err()
            .to_string();
        assert!(
            e.starts_with("move #2, step 3: move #3 (step 2), the one being added, removes its target (line 4)"),
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
            e.starts_with("move #1, step 2: move #2 (step 1), the one being added, makes its text target match another line"),
            "{e}"
        );
    }
}
