//! Undo/redo via snapshots of what the user edits: code, annotations and
//! styles (documents are small). The code is part of the snapshot so that a
//! paste or a future `code.patch` move can be undone together with the
//! annotations it dropped or re-anchored.

use std::collections::BTreeMap;

use crate::model::{Annotation, CodeSource, Document, MarkStyle};
use crate::moves::Move;

#[derive(Debug, Clone)]
struct Snapshot {
    /// What the edit after this snapshot did (`note a3`, `reload #54`…),
    /// so `u` can say what it undid.
    label: String,
    /// The step, for edits that replace the whole document (`scene.put`,
    /// `scene.patch`): undoing them brings the previous step back too.
    /// `None` for ordinary edits, whose undo keeps the current step.
    step: Option<u32>,
    code: Vec<String>,
    base_code: Vec<String>,
    moves: Vec<Move>,
    code_src: CodeSource,
    annotations: Vec<Annotation>,
    styles: BTreeMap<String, MarkStyle>,
    title: Option<String>,
    padding: Option<(u16, u16)>,
    numbers: bool,
}

impl Snapshot {
    fn take(doc: &Document, label: String, with_step: bool) -> Self {
        Self {
            label,
            step: with_step.then_some(doc.step),
            code: doc.code.clone(),
            base_code: doc.base_code.clone(),
            moves: doc.moves.clone(),
            code_src: doc.code_src.clone(),
            annotations: doc.annotations.clone(),
            styles: doc.styles.clone(),
            title: doc.title.clone(),
            padding: doc.padding,
            numbers: doc.numbers,
        }
    }

    /// Puts the snapshot into `doc` and returns what was there.
    fn swap(self, doc: &mut Document) -> Self {
        let current = Self::take(doc, self.label.clone(), self.step.is_some());
        doc.code = self.code;
        doc.base_code = self.base_code;
        doc.moves = self.moves;
        doc.code_src = self.code_src;
        doc.annotations = self.annotations;
        doc.styles = self.styles;
        doc.title = self.title;
        doc.padding = self.padding;
        doc.numbers = self.numbers;
        if let Some(step) = self.step {
            doc.step = step;
        }
        // The step may have changed since the snapshot: the shown code is
        // always the projection of the base at the current step.
        doc.sync_code_to_step();
        current
    }
}

#[derive(Debug, Default, Clone)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl History {
    const MAX: usize = 200;

    /// Call BEFORE mutating the document. `label` names the edit that
    /// follows (`note a3`, `reload #54`), for the undo/redo message.
    pub fn record(&mut self, doc: &Document, label: impl Into<String>) {
        self.push(Snapshot::take(doc, label.into(), false));
    }

    /// Like `record`, for an edit that replaces the whole document
    /// (`scene.put`, `scene.patch`): undo restores the step as well.
    pub fn record_with_step(&mut self, doc: &Document, label: impl Into<String>) {
        self.push(Snapshot::take(doc, label.into(), true));
    }

    fn push(&mut self, snapshot: Snapshot) {
        self.undo.push(snapshot);
        if self.undo.len() > Self::MAX {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Renames the last recorded edit (when its name is only known after
    /// the mutation, like the id an `add` assigns).
    pub fn label_last(&mut self, label: impl Into<String>) {
        if let Some(last) = self.undo.last_mut() {
            last.label = label.into();
        }
    }

    /// Drops the last recorded snapshot without touching the document: the
    /// mutation it was taken for failed and changed nothing.
    pub fn discard_last(&mut self) {
        self.undo.pop();
    }

    /// Undoes the last edit; returns its label.
    pub fn undo(&mut self, doc: &mut Document) -> Option<String> {
        let prev = self.undo.pop()?;
        let label = prev.label.clone();
        self.redo.push(prev.swap(doc));
        Some(label)
    }

    /// Redoes the last undone edit; returns its label.
    pub fn redo(&mut self, doc: &mut Document) -> Option<String> {
        let next = self.redo.pop()?;
        let label = next.label.clone();
        self.undo.push(next.swap(doc));
        Some(label)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Shape;

    #[test]
    fn undo_redo_roundtrip() {
        let mut d = Document::from_code("a\nb\n", "txt", None);
        let mut h = History::default();
        h.record(&d, "note a1");
        d.add(Annotation {
            id: "a1".into(),
            step: 1,
            color: None,
            markers: None,
            shape: Shape::Note {
                line: 1,
                text: "x".into(),
            },
        })
        .unwrap();
        assert_eq!(d.annotations.len(), 1);
        assert_eq!(h.undo(&mut d).as_deref(), Some("note a1"));
        assert_eq!(d.annotations.len(), 0);
        assert!(h.redo(&mut d).is_some());
        assert_eq!(d.annotations.len(), 1);
        assert!(h.redo(&mut d).is_none());
    }
}
