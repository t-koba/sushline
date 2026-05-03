use crate::buffer::LineBuffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UndoEntry {
    pub(crate) start: usize,
    pub(crate) deleted: Vec<u8>,
    pub(crate) inserted: Vec<u8>,
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
