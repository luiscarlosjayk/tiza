//! Undo/redo via snapshots of the annotations (documents are small).

use crate::model::{Annotation, Document};

#[derive(Debug, Default, Clone)]
pub struct History {
    undo: Vec<Vec<Annotation>>,
    redo: Vec<Vec<Annotation>>,
}

impl History {
    const MAX: usize = 200;

    /// Call BEFORE mutating `doc.annotations`.
    pub fn record(&mut self, doc: &Document) {
        self.undo.push(doc.annotations.clone());
        if self.undo.len() > Self::MAX {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, doc: &mut Document) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo
            .push(std::mem::replace(&mut doc.annotations, prev));
        true
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo
            .push(std::mem::replace(&mut doc.annotations, next));
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
