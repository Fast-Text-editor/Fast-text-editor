use crate::{Edit, Selection};

/// The kind of a transaction, used to decide what can be coalesced together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnKind {
    Insert,
    Delete,
    Other,
}

/// A group of edits that undo/redo as a single unit.
#[derive(Debug, Clone)]
pub struct Transaction {
    pub edits: Vec<Edit>,
    pub kind: TxnKind,
    /// Selection before the transaction.
    pub before: Selection,
    /// Selection after the transaction.
    pub after: Selection,
}

impl Transaction {
    pub fn new(kind: TxnKind, before: Selection) -> Self {
        Self {
            edits: Vec::new(),
            kind,
            before,
            after: before,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Applies the inverse of every edit, newest first.
    pub fn revert(&self, buffer: &mut crate::Buffer) {
        for edit in self.edits.iter().rev() {
            edit.inverse().apply(buffer);
        }
    }

    /// Applies every edit in order.
    pub fn apply(&self, buffer: &mut crate::Buffer) {
        for edit in &self.edits {
            edit.apply(buffer);
        }
    }
}

/// A bounded stack of undo/redo transactions.
#[derive(Debug)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl History {
    /// Creates a history holding at most `limit` transactions.
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Records a transaction. When `coalesce` is set and the new transaction is
    /// adjacent to and the same kind as the previous one, they are merged into
    /// a single undo unit (so a burst of typing undoes together).
    pub fn record(&mut self, txn: Transaction, coalesce: bool) {
        if txn.is_empty() {
            return;
        }
        self.redo.clear();

        if coalesce {
            if let Some(last) = self.undo.last_mut() {
                if last.kind == txn.kind && last.kind != TxnKind::Other && adjacent(last, &txn) {
                    last.edits.extend(txn.edits);
                    last.after = txn.after;
                    return;
                }
            }
        }

        self.undo.push(txn);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
    }

    /// Pops the most recent transaction for undoing.
    pub fn undo(&mut self) -> Option<Transaction> {
        let txn = self.undo.pop()?;
        self.redo.push(txn.clone());
        Some(txn)
    }

    /// Pops the most recent undone transaction for redoing.
    pub fn redo(&mut self) -> Option<Transaction> {
        let txn = self.redo.pop()?;
        self.undo.push(txn.clone());
        Some(txn)
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
}

/// Whether `next` continues `last` at the caret for coalescing purposes.
fn adjacent(last: &Transaction, next: &Transaction) -> bool {
    let (Some(le), Some(ne)) = (last.edits.last(), next.edits.first()) else {
        return false;
    };
    match (le, ne) {
        (Edit::Insert { .. }, Edit::Insert { at, .. }) => le.end() == *at,
        (Edit::Delete { at, .. }, Edit::Delete { .. }) => ne.end() == *at || le.end() == ne.at(),
        _ => false,
    }
}
