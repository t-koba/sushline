use crate::buffer::LineBuffer;
use history::HistoryUndoEntry;

use super::EditorState;

#[derive(Debug, Default)]
pub(crate) struct UndoState {
    pub(crate) undo_stack: Vec<UndoEntry>,
    pub(crate) pending_undo: Option<LineBuffer>,
    pub(crate) last_undo_was_insert: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UndoEntry {
    pub(crate) start: usize,
    pub(crate) deleted: Vec<u8>,
    pub(crate) inserted: Vec<u8>,
}

impl EditorState {
    pub(crate) fn record_undo(&mut self) {
        self.undo
            .pending_undo
            .get_or_insert_with(|| self.buffer.clone());
    }

    pub(crate) fn undo(&mut self) {
        self.commit_pending_undo();
        if let Some(entry) = self.undo.undo_stack.pop() {
            entry.undo(&mut self.buffer);
        }
        self.after_non_kill_command();
    }

    pub(crate) fn undo_snapshot_lines(&self) -> Vec<HistoryUndoEntry> {
        self.undo
            .undo_stack
            .iter()
            .map(|entry| HistoryUndoEntry {
                start: entry.start,
                deleted: entry.deleted.clone(),
                inserted: entry.inserted.clone(),
            })
            .collect()
    }

    pub(crate) fn restore_undo_snapshot_lines(&mut self, lines: &[HistoryUndoEntry]) {
        self.undo.undo_stack = lines
            .iter()
            .map(|entry| UndoEntry {
                start: entry.start,
                deleted: entry.deleted.clone(),
                inserted: entry.inserted.clone(),
            })
            .collect();
    }

    pub(crate) fn commit_pending_undo(&mut self) {
        let Some(before) = self.undo.pending_undo.take() else {
            return;
        };
        if before != self.buffer
            && let Some(entry) = UndoEntry::from_buffers(&before, &self.buffer)
        {
            self.undo.undo_stack.push(entry);
        }
    }
}

impl UndoEntry {
    pub(crate) fn from_buffers(before: &LineBuffer, after: &LineBuffer) -> Option<Self> {
        let before = before.as_bytes();
        let after = after.as_bytes();
        if before == after {
            return None;
        }
        let mut start = 0;
        let limit = before.len().min(after.len());
        while start < limit && before[start] == after[start] {
            start += 1;
        }
        let mut before_end = before.len();
        let mut after_end = after.len();
        while before_end > start
            && after_end > start
            && before[before_end - 1] == after[after_end - 1]
        {
            before_end -= 1;
            after_end -= 1;
        }
        Some(Self {
            start,
            deleted: before[start..before_end].to_vec(),
            inserted: after[start..after_end].to_vec(),
        })
    }

    pub(crate) fn undo(self, buffer: &mut LineBuffer) {
        buffer.replace_range_bytes(self.start, self.start + self.inserted.len(), &self.deleted);
    }
}
