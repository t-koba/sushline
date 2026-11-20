use crate::buffer::LineBuffer;
use history::HistoryUndoEntry;

use super::EditorState;

#[derive(Debug, Clone, Default)]
pub(crate) struct UndoState {
    pub(crate) undo_stack: Vec<UndoEntry>,
    pub(crate) pending_undo: Option<LineBuffer>,
    pub(crate) last_undo_was_insert: bool,
    /// Buffer before a vi put and its dot-replays, kept so their undo entries
    /// can be merged into the single GNU entry as each replay commits.
    pub(crate) vi_put_undo_before: Option<LineBuffer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UndoEntry {
    pub(crate) start: usize,
    pub(crate) deleted: Vec<u8>,
    pub(crate) inserted: Vec<u8>,
}

impl EditorState {
    pub(crate) fn record_undo(&mut self) {
        // A non-insert edit closes the pending insert run, so each command
        // gets its own undo entry. Without this, an insert immediately undone
        // by a delete leaves no entry and GNU's per-command undo diverges.
        self.commit_pending_undo();
        self.undo
            .pending_undo
            .get_or_insert_with(|| self.buffer.clone());
    }

    /// Record the start of an insert run with GNU `rl_insert_text` grouping:
    /// only single-byte inserts extend the pending entry while it holds fewer
    /// than 20 inserted bytes; anything wider commits the run first.
    pub(crate) fn record_insert_undo(&mut self, single_byte: bool) {
        if !self.undo.last_undo_was_insert {
            // Continuing an insert run keeps the pending point open: a change
            // operator may have deliberately deferred its commit so the
            // following insert joins it in one entry.
            self.undo
                .pending_undo
                .get_or_insert_with(|| self.buffer.clone());
            return;
        }
        if !single_byte {
            self.commit_pending_undo();
            self.record_undo();
            return;
        }
        let capped = self
            .undo
            .pending_undo
            .as_ref()
            .and_then(|before| UndoEntry::from_buffers(before, &self.buffer))
            .map(|entry| entry.inserted.len() >= 20)
            .unwrap_or(false);
        if capped {
            self.commit_pending_undo();
            self.record_undo();
        }
    }

    /// GNU pushes an undo entry even when a command such as a failed vi
    /// replace leaves the buffer unchanged, so the next undo consumes it
    /// silently instead of ringing on an empty list.
    pub(crate) fn record_noop_undo(&mut self) {
        self.commit_pending_undo();
        self.undo.undo_stack.push(UndoEntry {
            start: self.buffer.point(),
            deleted: Vec::new(),
            inserted: Vec::new(),
        });
    }

    pub(crate) fn start_vi_put_undo(&mut self, before: &LineBuffer) {
        self.undo.vi_put_undo_before = Some(before.clone());
    }

    pub(crate) fn clear_vi_put_undo(&mut self) {
        self.undo.vi_put_undo_before = None;
    }

    /// Merge the just-committed put entry into the earlier put entry when
    /// both are part of the same uninterrupted put/dot sequence.
    pub(crate) fn extend_vi_put_undo(&mut self, before_edit: &LineBuffer) -> bool {
        let Some(before) = self.undo.vi_put_undo_before.clone() else {
            return false;
        };
        let stack = &self.undo.undo_stack;
        if stack.len() < 2
            || UndoEntry::from_buffers(&before, before_edit).as_ref()
                != Some(&stack[stack.len() - 2])
            || UndoEntry::from_buffers(before_edit, &self.buffer).as_ref() != stack.last()
        {
            self.undo.vi_put_undo_before = None;
            return false;
        }
        self.undo.undo_stack.pop();
        self.undo.undo_stack.pop();
        if let Some(combined) = UndoEntry::from_buffers(&before, &self.buffer) {
            self.undo.undo_stack.push(combined);
        }
        true
    }

    pub(crate) fn undo(&mut self) -> bool {
        self.commit_pending_undo();
        self.undo.vi_put_undo_before = None;
        let undone = if let Some(entry) = self.undo.undo_stack.pop() {
            entry.undo(&mut self.buffer);
            // GNU leaves vi point on the last character when an undo restores
            // text at the end of the line; the raw buffer edit lands just
            // past it, where the next vi command would be blocked. Emacs mode
            // keeps the end-of-line point and must not be clamped.
            if self.vi.command_mode_entered
                && !self.buffer.is_empty()
                && self.buffer.point() == self.buffer.as_bytes().len()
            {
                self.buffer.move_backward();
            }
            true
        } else {
            false
        };
        // GNU keeps the modified marker until the whole edit sequence for
        // this history line has been undone. Byte equality alone clears it
        // too early when a partial undo happens to restore the original text
        // while earlier edits remain on the undo stack.
        if undone && self.undo.undo_stack.is_empty() {
            self.history_modified = false;
        }
        self.after_non_kill_command();
        undone
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
        self.undo.vi_put_undo_before = None;
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
