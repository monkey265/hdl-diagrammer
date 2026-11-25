/// Snapshot-based undo/redo.
#[derive(Default)]
pub struct History<T: Clone> {
    undo: Vec<T>,
    redo: Vec<T>,
}

impl<T: Clone> History<T> {
    /// Record `before` (the state prior to an edit); a new edit invalidates redo.
    pub fn push(&mut self, before: &T) {
        self.undo.push(before.clone());
        self.redo.clear();
    }
    pub fn undo(&mut self, current: &T) -> Option<T> {
        let prev = self.undo.pop()?;
        self.redo.push(current.clone());
        Some(prev)
    }
    pub fn redo(&mut self, current: &T) -> Option<T> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        Some(next)
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_undo_redo_roundtrip() {
        let mut h = History::default();
        h.push(&1); // state was 1, edited to 2
        h.push(&2); // then edited to 3
        assert_eq!(h.undo(&3), Some(2));
        assert_eq!(h.undo(&2), Some(1));
        assert_eq!(h.undo(&1), None);
        assert_eq!(h.redo(&1), Some(2));
        assert_eq!(h.redo(&2), Some(3));
        assert_eq!(h.redo(&3), None);
    }

    #[test]
    fn history_push_clears_redo() {
        let mut h = History::default();
        h.push(&1);
        assert_eq!(h.undo(&2), Some(1));
        h.push(&1);
        assert_eq!(h.redo(&5), None);
    }
}
