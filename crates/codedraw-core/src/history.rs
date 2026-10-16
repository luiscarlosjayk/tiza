//! Undo/redo via snapshots of what the user edits: code, annotations and
//! styles (documents are small). The code is part of the snapshot so that a
//! paste or a future `code.patch` move can be undone together with the
//! annotations it dropped or re-anchored.

use std::collections::BTreeMap;

use crate::model::{Annotation, CodeSource, Document, MarkStyle};

#[derive(Debug, Clone)]
struct Snapshot {
    code: Vec<String>,
    code_src: CodeSource,
    annotations: Vec<Annotation>,
    styles: BTreeMap<String, MarkStyle>,
}

impl Snapshot {
    fn take(doc: &Document) -> Self {
        Self {
            code: doc.code.clone(),
            code_src: doc.code_src.clone(),
            annotations: doc.annotations.clone(),
            styles: doc.styles.clone(),
        }
    }

    /// Puts the snapshot into `doc` and returns what was there.
    fn swap(self, doc: &mut Document) -> Self {
        let current = Self::take(doc);
        doc.code = self.code;
        doc.code_src = self.code_src;
        doc.annotations = self.annotations;
        doc.styles = self.styles;
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

    /// Call BEFORE mutating the document.
    pub fn record(&mut self, doc: &Document) {
        self.undo.push(Snapshot::take(doc));
        if self.undo.len() > Self::MAX {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, doc: &mut Document) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(prev.swap(doc));
        true
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(next.swap(doc));
        true
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
        h.record(&d);
        d.add(Annotation {
            id: "a1".into(),
            step: 1,
            color: None,
            shape: Shape::Note {
                line: 1,
                text: "x".into(),
            },
        })
        .unwrap();
        assert_eq!(d.annotations.len(), 1);
        assert!(h.undo(&mut d));
        assert_eq!(d.annotations.len(), 0);
        assert!(h.redo(&mut d));
        assert_eq!(d.annotations.len(), 1);
        assert!(!h.redo(&mut d));
    }
}
