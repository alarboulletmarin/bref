//! Undo and redo: two stacks of whatever a surface needs to go back (a diagram, a batch of cell
//! changes), capped. Pure; the surface applies the step, this only keeps the order.

/// How many steps are kept, unless a surface has a reason to keep fewer.
pub const DEPTH: usize = 200;

pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    cap: usize,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self::capped(DEPTH)
    }
}

impl<T> History<T> {
    /// A history that keeps at most `cap` steps.
    pub fn capped(cap: usize) -> Self {
        History { undo: Vec::new(), redo: Vec::new(), cap }
    }

    /// A new step done by the user: what it replaced can be restored, and the redo is gone.
    pub fn record(&mut self, step: T) {
        self.undo.push(step);
        if self.undo.len() > self.cap {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// The step to go back to; the caller applies it, then hands the way back to `undone`.
    pub fn undo(&mut self) -> Option<T> {
        self.undo.pop()
    }

    /// The step to go forward to; the caller applies it, then hands the way back to `redone`.
    pub fn redo(&mut self) -> Option<T> {
        self.redo.pop()
    }

    /// The way back to the state an undo left.
    pub fn undone(&mut self, back: T) {
        self.redo.push(back);
    }

    /// The way back to the state a redo left (the redo stack stays).
    pub fn redone(&mut self, back: T) {
        self.undo.push(back);
    }

    /// Takes the last recorded step back without making it a redo (a gesture that changed nothing).
    pub fn cancel(&mut self) -> Option<T> {
        self.undo.pop()
    }

    pub fn last(&self) -> Option<&T> {
        self.undo.last()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.undo.len()
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
    fn keeps_the_most_recent_steps() {
        let mut h = History::capped(3);
        for i in 0..5 {
            h.record(i);
        }
        assert_eq!((h.len(), h.last()), (3, Some(&4)));
        assert_eq!(h.undo(), Some(4));
        assert_eq!(h.undo(), Some(3));
        assert_eq!(h.undo(), Some(2));
        assert_eq!(h.undo(), None);
    }

    #[test]
    fn undo_then_redo_then_a_new_step() {
        let mut h = History::default();
        h.record(1);
        let back = h.undo().unwrap();
        h.undone(back + 10);
        assert!(h.can_redo() && !h.can_undo());
        let again = h.redo().unwrap();
        h.redone(again);
        assert!(h.can_undo() && !h.can_redo());
        h.undone(0);
        h.record(2);
        assert!(!h.can_redo());
        h.cancel();
        assert_eq!(h.last(), Some(&11));
    }
}
