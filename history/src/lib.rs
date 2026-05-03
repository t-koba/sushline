pub mod expansion;
mod file;

use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub line_bytes: Vec<u8>,
    pub timestamp: Option<String>,
    pub modified: bool,
    pub undo_list: Vec<HistoryUndoEntry>,
}

impl HistoryEntry {
    pub fn line(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.line_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryUndoEntry {
    pub start: usize,
    pub deleted: Vec<u8>,
    pub inserted: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryDirection {
    Previous,
    Next,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryState {
    pub offset: usize,
    pub length: usize,
    pub size: usize,
    pub stifled: bool,
    pub max_entries: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySearchMatch {
    pub entry_index: usize,
    pub byte_offset: usize,
    pub line_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct History {
    entries: Vec<HistoryEntry>,
    cursor: Option<usize>,
    current_edit: Vec<u8>,
    max_entries: Option<usize>,
    file_loaded_len: usize,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, line: impl Into<String>) {
        let line = line.into();
        self.push_entry(line.into_bytes(), None, false);
    }

    pub fn push_bytes(&mut self, line: Vec<u8>) {
        self.push_entry(line, None, false);
    }

    pub fn enforce_max_len(&mut self, max_len: Option<usize>) {
        let Some(max_len) = max_len else {
            return;
        };
        if self.entries.len() > max_len {
            let remove = self.entries.len() - max_len;
            self.entries.drain(..remove);
            self.file_loaded_len = self.file_loaded_len.saturating_sub(remove);
        }
        self.reset_cursor();
    }

    pub fn stifle(&mut self, max_entries: usize) {
        self.max_entries = Some(max_entries);
        self.enforce_max_len(Some(max_entries));
    }

    pub fn unstifle(&mut self) -> Option<usize> {
        self.max_entries.take()
    }

    pub fn is_stifled(&self) -> bool {
        self.max_entries.is_some()
    }

    pub fn max_entries(&self) -> Option<usize> {
        self.max_entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.file_loaded_len = 0;
        self.reset_cursor();
    }

    pub fn state(&self) -> HistoryState {
        HistoryState {
            offset: self.where_history(),
            length: self.entries.len(),
            size: self.entries.capacity(),
            stifled: self.is_stifled(),
            max_entries: self.max_entries,
        }
    }

    pub fn set_state(&mut self, state: HistoryState) -> bool {
        if state.offset > self.entries.len() {
            return false;
        }
        self.max_entries = state.max_entries.filter(|_| state.stifled);
        self.set_pos(state.offset)
    }

    pub fn remove(&mut self, index: usize) -> Option<HistoryEntry> {
        let removed = (index < self.entries.len()).then(|| self.entries.remove(index));
        self.reset_cursor();
        removed
    }

    pub fn replace(&mut self, index: usize, line: impl Into<String>) -> Option<HistoryEntry> {
        let entry = self.entries.get_mut(index)?;
        let line = line.into();
        let previous = std::mem::replace(
            entry,
            HistoryEntry {
                line_bytes: line.into_bytes(),
                timestamp: None,
                modified: true,
                undo_list: entry.undo_list.clone(),
            },
        );
        self.reset_cursor();
        Some(previous)
    }

    pub fn add_time(&mut self, timestamp: impl Into<String>) -> bool {
        let Some(entry) = self.entries.last_mut() else {
            return false;
        };
        entry.timestamp = Some(timestamp.into());
        true
    }

    pub fn get(&self, index: usize) -> Option<&HistoryEntry> {
        self.entries.get(index)
    }

    pub fn undo_list(&self, index: usize) -> Option<&[HistoryUndoEntry]> {
        self.entries
            .get(index)
            .map(|entry| entry.undo_list.as_slice())
    }

    pub fn set_undo_list(&mut self, index: usize, undo_list: Vec<HistoryUndoEntry>) -> bool {
        let Some(entry) = self.entries.get_mut(index) else {
            return false;
        };
        entry.undo_list = undo_list;
        true
    }

    pub fn current_index(&self) -> Option<usize> {
        self.cursor
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn where_history(&self) -> usize {
        self.cursor.unwrap_or(self.entries.len())
    }

    pub fn current_history(&self) -> Option<&HistoryEntry> {
        self.cursor.and_then(|idx| self.entries.get(idx))
    }

    pub fn set_pos(&mut self, pos: usize) -> bool {
        if pos > self.entries.len() {
            return false;
        }
        self.cursor = (pos < self.entries.len()).then_some(pos);
        true
    }

    pub fn previous_history(&mut self) -> Option<&HistoryEntry> {
        let pos = self.where_history();
        if pos == 0 {
            return None;
        }
        self.cursor = Some(pos - 1);
        self.current_history()
    }

    pub fn next_history(&mut self) -> Option<&HistoryEntry> {
        let pos = self.cursor?;
        let next = pos + 1;
        if next >= self.entries.len() {
            self.cursor = None;
            return None;
        }
        self.cursor = Some(next);
        self.current_history()
    }

    pub fn total_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| entry.line_bytes.len())
            .sum()
    }

    fn push_entry(
        &mut self,
        line: Vec<u8>,
        timestamp: Option<String>,
        suppress_adjacent_duplicate: bool,
    ) {
        if line.is_empty() {
            self.reset_cursor();
            return;
        }
        if !suppress_adjacent_duplicate
            || self.entries.last().map(|entry| entry.line_bytes.as_slice()) != Some(line.as_slice())
        {
            self.entries.push(HistoryEntry {
                line_bytes: line,
                timestamp,
                modified: false,
                undo_list: Vec::new(),
            });
        }
        self.enforce_max_len(self.max_entries);
        self.reset_cursor();
    }

    pub fn reset_cursor(&mut self) {
        self.cursor = None;
        self.current_edit.clear();
    }

    pub fn revert_current_edit(&mut self) {
        self.reset_cursor();
    }

    pub fn navigate_bytes(
        &mut self,
        direction: HistoryDirection,
        current: Vec<u8>,
    ) -> Option<Vec<u8>> {
        if self.entries.is_empty() {
            return None;
        }

        match (direction, self.cursor) {
            (HistoryDirection::Previous, None) => {
                self.current_edit = current;
                self.cursor = Some(self.entries.len() - 1);
            }
            (HistoryDirection::Previous, Some(0)) => {}
            (HistoryDirection::Previous, Some(pos)) => self.cursor = Some(pos - 1),
            (HistoryDirection::Next, None) => return None,
            (HistoryDirection::Next, Some(pos)) if pos + 1 >= self.entries.len() => {
                self.cursor = None;
                return Some(self.current_edit.clone());
            }
            (HistoryDirection::Next, Some(pos)) => self.cursor = Some(pos + 1),
        }

        self.cursor.map(|pos| self.entries[pos].line_bytes.clone())
    }

    pub fn beginning_bytes(&mut self, current: Vec<u8>) -> Option<Vec<u8>> {
        if self.entries.is_empty() {
            return None;
        }
        if self.cursor.is_none() {
            self.current_edit = current;
        }
        self.cursor = Some(0);
        Some(self.entries[0].line_bytes.clone())
    }

    pub fn end_bytes(&mut self) -> Option<Vec<u8>> {
        self.cursor?;
        self.cursor = None;
        Some(self.current_edit.clone())
    }

    pub fn next_after_current_cursor_bytes(&self) -> Option<Vec<u8>> {
        let cursor = self.cursor?;
        self.entries
            .get(cursor + 1)
            .map(|entry| entry.line_bytes.clone())
    }

    pub fn search_prefix_backward_bytes(
        &mut self,
        prefix: &[u8],
        current: Vec<u8>,
    ) -> Option<Vec<u8>> {
        if self.entries.is_empty() {
            return None;
        }
        if self.cursor.is_none() {
            self.current_edit = current;
        }

        let start = self.cursor.unwrap_or(self.entries.len());
        let found = self.entries[..start]
            .iter()
            .rposition(|entry| entry.line_bytes.starts_with(prefix))?;
        self.cursor = Some(found);
        Some(self.entries[found].line_bytes.clone())
    }

    pub fn search_prefix_forward_bytes(&mut self, prefix: &[u8]) -> Option<Vec<u8>> {
        let cursor = self.cursor?;

        if let Some(offset) = self.entries[cursor + 1..]
            .iter()
            .position(|entry| entry.line_bytes.starts_with(prefix))
        {
            let found = cursor + 1 + offset;
            self.cursor = Some(found);
            return Some(self.entries[found].line_bytes.clone());
        }

        self.cursor = None;
        Some(self.current_edit.clone())
    }

    pub fn search_containing_backward_index_bytes(
        &self,
        needle: &[u8],
        before: Option<usize>,
    ) -> Option<(usize, Vec<u8>)> {
        if needle.is_empty() {
            return None;
        }
        let end = before.unwrap_or(self.entries.len()).min(self.entries.len());
        self.entries[..end]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, entry)| find_bytes(&entry.line_bytes, needle).is_some())
            .map(|(idx, entry)| (idx, entry.line_bytes.clone()))
    }

    pub fn search_containing_forward_index_bytes(
        &self,
        needle: &[u8],
        after: Option<usize>,
    ) -> Option<(usize, Vec<u8>)> {
        if needle.is_empty() {
            return None;
        }
        let start = after
            .map(|idx| idx + 1)
            .unwrap_or(0)
            .min(self.entries.len());
        self.entries[start..]
            .iter()
            .enumerate()
            .find(|(_, entry)| find_bytes(&entry.line_bytes, needle).is_some())
            .map(|(offset, entry)| (start + offset, entry.line_bytes.clone()))
    }

    pub fn history_search_bytes(
        &mut self,
        needle: &[u8],
        direction: HistoryDirection,
    ) -> Option<HistorySearchMatch> {
        self.history_search_bytes_with_case(needle, direction, false)
    }

    pub fn history_search_bytes_with_case(
        &mut self,
        needle: &[u8],
        direction: HistoryDirection,
        ignore_case: bool,
    ) -> Option<HistorySearchMatch> {
        let start = match direction {
            HistoryDirection::Previous => self.where_history().min(self.entries.len()),
            HistoryDirection::Next => self
                .where_history()
                .saturating_add(1)
                .min(self.entries.len()),
        };
        let found =
            self.search_from_pos_bytes_with_case(needle, direction, start, false, ignore_case)?;
        self.cursor = Some(found.entry_index);
        Some(found)
    }

    pub fn history_search_prefix(
        &mut self,
        prefix: &str,
        direction: HistoryDirection,
    ) -> Option<HistorySearchMatch> {
        let start = self.where_history().min(self.entries.len());
        let found = self.search_from_pos(prefix, direction, start, true)?;
        self.cursor = Some(found.entry_index);
        Some(found)
    }

    pub fn history_search_pos(
        &self,
        needle: &str,
        direction: HistoryDirection,
        pos: usize,
    ) -> Option<HistorySearchMatch> {
        self.search_from_pos(needle, direction, pos, false)
    }

    fn search_from_pos(
        &self,
        needle: &str,
        direction: HistoryDirection,
        pos: usize,
        anchored: bool,
    ) -> Option<HistorySearchMatch> {
        self.search_from_pos_bytes(needle.as_bytes(), direction, pos, anchored)
    }

    fn search_from_pos_bytes(
        &self,
        needle: &[u8],
        direction: HistoryDirection,
        pos: usize,
        anchored: bool,
    ) -> Option<HistorySearchMatch> {
        self.search_from_pos_bytes_with_case(needle, direction, pos, anchored, false)
    }

    fn search_from_pos_bytes_with_case(
        &self,
        needle: &[u8],
        direction: HistoryDirection,
        pos: usize,
        anchored: bool,
        ignore_case: bool,
    ) -> Option<HistorySearchMatch> {
        if needle.is_empty() || self.entries.is_empty() {
            return None;
        }
        match direction {
            HistoryDirection::Previous => {
                let end = pos.min(self.entries.len().saturating_sub(1));
                self.entries[..=end]
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(idx, entry)| {
                        let byte_offset = if anchored {
                            starts_with_bytes(&entry.line_bytes, needle, ignore_case).then_some(0)
                        } else {
                            find_bytes_with_case(&entry.line_bytes, needle, ignore_case)
                        }?;
                        Some(HistorySearchMatch {
                            entry_index: idx,
                            byte_offset,
                            line_bytes: entry.line_bytes.clone(),
                        })
                    })
            }
            HistoryDirection::Next => {
                let start = pos.min(self.entries.len());
                self.entries[start..]
                    .iter()
                    .enumerate()
                    .find_map(|(offset, entry)| {
                        let byte_offset = if anchored {
                            starts_with_bytes(&entry.line_bytes, needle, ignore_case).then_some(0)
                        } else {
                            find_bytes_with_case(&entry.line_bytes, needle, ignore_case)
                        }?;
                        Some(HistorySearchMatch {
                            entry_index: start + offset,
                            byte_offset,
                            line_bytes: entry.line_bytes.clone(),
                        })
                    })
            }
        }
    }

    pub fn search_containing_forward_from_cursor_bytes(
        &mut self,
        needle: &[u8],
        current: Vec<u8>,
    ) -> Option<Vec<u8>> {
        if needle.is_empty() || self.entries.is_empty() {
            return None;
        }
        if self.cursor.is_none() {
            self.current_edit = current;
            self.cursor = Some(0);
        }
        let start = self.cursor.unwrap_or(0);
        let current_matches = find_bytes(&self.entries[start].line_bytes, needle).is_some();
        let found = self.entries[start + usize::from(current_matches)..]
            .iter()
            .position(|entry| find_bytes(&entry.line_bytes, needle).is_some())
            .map(|offset| start + usize::from(current_matches) + offset)?;
        self.cursor = Some(found);
        Some(self.entries[found].line_bytes.clone())
    }

    pub fn search_containing_backward_from_cursor_bytes(
        &mut self,
        needle: &[u8],
        current: Vec<u8>,
    ) -> Option<Vec<u8>> {
        if needle.is_empty() || self.entries.is_empty() {
            return None;
        }
        if self.cursor.is_none() {
            self.current_edit = current;
        }
        let start = self.cursor.unwrap_or(self.entries.len());
        let found = self.entries[..start]
            .iter()
            .rposition(|entry| find_bytes(&entry.line_bytes, needle).is_some())?;
        self.cursor = Some(found);
        Some(self.entries[found].line_bytes.clone())
    }

    pub fn get_1_based_entry(&self, index: usize) -> Option<&HistoryEntry> {
        index.checked_sub(1).and_then(|idx| self.entries.get(idx))
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes_with_case(haystack, needle, false)
}

fn find_bytes_with_case(haystack: &[u8], needle: &[u8], ignore_case: bool) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| bytes_equal(window, needle, ignore_case))
}

fn starts_with_bytes(haystack: &[u8], needle: &[u8], ignore_case: bool) -> bool {
    haystack
        .get(..needle.len())
        .map(|window| bytes_equal(window, needle, ignore_case))
        .unwrap_or(false)
}

fn bytes_equal(left: &[u8], right: &[u8], ignore_case: bool) -> bool {
    if !ignore_case {
        return left == right;
    }
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}
