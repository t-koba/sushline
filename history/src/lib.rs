//! History storage, navigation, persistence, and expansion.
#![warn(missing_docs)]

/// Expansion.
pub mod expansion;
mod file;

use std::borrow::Cow;

/// The marker that begins a timestamp record.
const TIMESTAMP_MARKER: u8 = b'#';

#[derive(Debug, Clone, PartialEq, Eq)]
/// HistoryEntry.
pub struct HistoryEntry {
    /// Line bytes.
    pub line_bytes: Vec<u8>,
    /// Retained decoded timestamp metadata, including any NUL suffix. File
    /// input replaces invalid UTF-8 with U+FFFD.
    pub timestamp: Option<String>,
    /// Modified.
    pub modified: bool,
    /// Undo list.
    pub undo_list: Vec<HistoryUndoEntry>,
}

impl HistoryEntry {
    /// Line.
    pub fn line(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.line_bytes)
    }

    /// A GNU-compatible timestamp-text view: the stored text before its first
    /// NUL, or `None` if the entry has no timestamp. This does not control writes.
    pub fn gnu_timestamp_text(&self) -> Option<&str> {
        self.timestamp
            .as_deref()
            .map(|text| text.split('\0').next().unwrap_or(text))
    }

    /// The seconds the entry's timestamp record carries.
    ///
    /// The record's text is the `#`-plus-digits form the reader recognizes,
    /// and the seconds are the number that follows the `#`, read the way
    /// the C library's number conversion reads it: the whitespace that
    /// conversion skips, an optional sign, then a decimal prefix. Missing
    /// digits or a value outside `i64` yields `None`.
    ///
    /// GNU's function reports the absence of a time as zero, so there a
    /// record whose time is the epoch cannot be told from a record without
    /// one; this reports the absence instead, so the two are told apart.
    /// Unlike this accessor, GNU's `history_get_time` uses the mutable
    /// `history_comment_char` and returns zero when the marker does not match
    /// or conversion fails. On the installed GNU History 8.2 library, that
    /// character was zero by default, so a `#` record returned zero until the
    /// caller set it to `#`. Readline 8.3 source likewise checks the current
    /// comment character before calling `strtol`.
    pub fn timestamp_seconds(&self) -> Option<i64> {
        let text = self.timestamp.as_deref()?;
        let digits = text.strip_prefix(TIMESTAMP_MARKER as char)?;
        parse_seconds(digits)
    }
}

/// Read the base-10 integer prefix after a timestamp record's marker.
/// The whitespace the C library's number conversion skips and a sign are
/// accepted; no digits or a value outside `i64` has no value.
fn parse_seconds(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() && is_c_whitespace(bytes[index]) {
        index += 1;
    }
    let negative = match bytes.get(index) {
        Some(b'-') => {
            index += 1;
            true
        }
        Some(b'+') => {
            index += 1;
            false
        }
        _ => false,
    };
    let first_digit = index;
    let mut magnitude: u64 = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        magnitude = magnitude
            .checked_mul(10)?
            .checked_add(u64::from(bytes[index] - b'0'))?;
        index += 1;
    }
    if index == first_digit {
        return None;
    }
    if negative {
        if magnitude > (1u64 << 63) {
            return None;
        }
        Some(-(magnitude as i128) as i64)
    } else {
        if magnitude > i64::MAX as u64 {
            return None;
        }
        Some(magnitude as i64)
    }
}

/// The whitespace the C library's number conversion skips.
///
/// It is the ASCII whitespace set plus the vertical tab, which the C
/// library counts and Rust's ASCII whitespace set leaves out.
fn is_c_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | b'\x0c' | b'\r')
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// HistoryUndoEntry.
pub struct HistoryUndoEntry {
    /// Start.
    pub start: usize,
    /// Deleted.
    pub deleted: Vec<u8>,
    /// Inserted.
    pub inserted: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// HistoryDirection.
pub enum HistoryDirection {
    /// Previous.
    Previous,
    /// Next.
    Next,
}

/// How a history file's physical lines are grouped into entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryMultiline {
    /// Each physical line is an entry of its own. This is GNU's default:
    /// `history_multiline_entries` is 0 unless the caller changes it. A
    /// leading `#`-plus-digits line selects timestamped-file format; later
    /// marker lines in that format delimit records. In a file whose first
    /// physical line is a command, later marker-looking lines are command text,
    /// matching the pinned GNU 8.3 full-file fixture.
    #[default]
    PerLine,
    /// The physical lines that follow a timestamp record make one entry,
    /// dropping leading blanks and retaining interior blanks. Installed GNU 8.2
    /// dropped an interior blank in a full-file comparison; pinned GNU 8.3
    /// retained one in a range fixture. For a plain file,
    /// selected physical lines form one entry, including blank lines. Pinned
    /// GNU History 8.3 range and full-file plain fixtures show the same joined
    /// bytes; see `COMPATIBILITY.md` for the fixture-scoped observations.
    Joined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// HistoryState.
pub struct HistoryState {
    /// Offset.
    pub offset: usize,
    /// Length.
    pub length: usize,
    /// Size.
    pub size: usize,
    /// Stifled.
    pub stifled: bool,
    /// Max entries.
    pub max_entries: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// HistorySearchMatch.
pub struct HistorySearchMatch {
    /// Entry index.
    pub entry_index: usize,
    /// Byte offset.
    pub byte_offset: usize,
    /// Line bytes.
    pub line_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
/// History.
///
/// Stifling, removal and state restoration can retain an offset beyond the list. In
/// that state cursor-relative navigation and searches return `None` without
/// moving it. Use `set_pos`, `reset_cursor`, `beginning_bytes`, or `end_bytes`
/// to recover a valid position.
pub struct History {
    entries: Vec<HistoryEntry>,
    search_masks: Vec<u64>,
    cursor: Option<usize>,
    current_edit: Vec<u8>,
    max_entries: Option<usize>,
    file_loaded_len: usize,
}

impl History {
    /// New.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a history entry, including an empty line.
    ///
    /// Preserve the numeric position; an unpositioned end becomes the newly
    /// appended entry if there is room. This matches the measured GNU 8.3
    /// unstifled and stifled append fixtures.
    pub fn push(&mut self, line: impl Into<String>) {
        self.push_bytes(line.into().into_bytes());
    }

    /// Add a byte-oriented history entry, including an empty line.
    ///
    /// Preserve the numeric position as in [`Self::push`].
    pub fn push_bytes(&mut self, line: Vec<u8>) {
        let position = self.where_history();
        self.push_entry(line, None, false);
        self.restore_offset(position);
    }

    /// Enforce max len.
    pub fn enforce_max_len(&mut self, max_len: Option<usize>) {
        let Some(max_len) = max_len else {
            return;
        };
        if self.entries.len() > max_len {
            let remove = self.entries.len() - max_len;
            self.entries.drain(..remove);
            self.search_masks.drain(..remove);
            self.file_loaded_len = self.file_loaded_len.saturating_sub(remove);
        }
        self.reset_cursor();
    }

    /// Stifle.
    pub fn stifle(&mut self, max_entries: usize) {
        let position = self.where_history();
        self.max_entries = Some(max_entries);
        self.enforce_max_len(Some(max_entries));
        self.restore_offset(position);
    }

    /// Unstifle.
    pub fn unstifle(&mut self) -> Option<usize> {
        self.max_entries.take()
    }

    /// Is stifled.
    pub fn is_stifled(&self) -> bool {
        self.max_entries.is_some()
    }

    /// Max entries.
    pub fn max_entries(&self) -> Option<usize> {
        self.max_entries
    }

    /// Len.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.search_masks.clear();
        self.file_loaded_len = 0;
        self.reset_cursor();
    }

    /// State.
    pub fn state(&self) -> HistoryState {
        HistoryState {
            offset: self.where_history(),
            length: self.entries.len(),
            size: self.entries.capacity(),
            stifled: self.is_stifled(),
            max_entries: self.max_entries,
        }
    }

    /// Restore state, including an offset beyond the end produced by stifling.
    pub fn set_state(&mut self, state: HistoryState) -> bool {
        self.max_entries = state.max_entries.filter(|_| state.stifled);
        self.restore_offset(state.offset);
        true
    }

    /// Remove an entry while preserving the numeric position.
    pub fn remove(&mut self, index: usize) -> Option<HistoryEntry> {
        if index >= self.entries.len() {
            // An invalid removal is a no-op: the positioned cursor is kept.
            return None;
        }
        let position = self.where_history();
        let removed = self.entries.remove(index);
        self.search_masks.remove(index);
        if index < self.file_loaded_len {
            self.file_loaded_len = self.file_loaded_len.saturating_sub(1);
        }
        self.reset_cursor();
        self.restore_offset(position);
        Some(removed)
    }

    /// Replace an entry's line, preserving its timestamp and undo list and marking it modified.
    pub fn replace(&mut self, index: usize, line: impl Into<String>) -> Option<HistoryEntry> {
        let entry = self.entries.get_mut(index)?;
        let line = line.into();
        let timestamp = entry.timestamp.clone();
        let undo_list = entry.undo_list.clone();
        let previous = std::mem::replace(
            entry,
            HistoryEntry {
                line_bytes: line.into_bytes(),
                timestamp,
                modified: true,
                undo_list,
            },
        );
        self.search_masks[index] = byte_bigram_mask(&entry.line_bytes);
        // Replacing content keeps the positioned cursor (and any stashed
        // current edit), matching the pinned GNU behavior.
        Some(previous)
    }

    /// Add time.
    ///
    /// The value is the timestamp record's text, the `#`-plus-digits form the
    /// reader recognizes and GNU writes, so a history written with
    /// timestamps is a history the reader reads back with its timestamps.
    pub fn add_time(&mut self, timestamp: impl Into<String>) -> bool {
        let Some(entry) = self.entries.last_mut() else {
            return false;
        };
        entry.timestamp = Some(timestamp.into());
        true
    }

    /// Get.
    pub fn get(&self, index: usize) -> Option<&HistoryEntry> {
        self.entries.get(index)
    }

    /// Undo list.
    pub fn undo_list(&self, index: usize) -> Option<&[HistoryUndoEntry]> {
        self.entries
            .get(index)
            .map(|entry| entry.undo_list.as_slice())
    }

    /// Update an entry's line bytes, preserving its timestamp and undo list.
    ///
    /// The entry is marked modified only when the bytes actually change, so
    /// traversing history without editing does not dirty entries.
    pub fn set_line_bytes(&mut self, index: usize, line: Vec<u8>) -> bool {
        let Some(entry) = self.entries.get_mut(index) else {
            return false;
        };
        if entry.line_bytes != line {
            self.search_masks[index] = byte_bigram_mask(&line);
            entry.line_bytes = line;
            entry.modified = true;
        }
        true
    }

    /// Set undo list.
    pub fn set_undo_list(&mut self, index: usize, undo_list: Vec<HistoryUndoEntry>) -> bool {
        let Some(entry) = self.entries.get_mut(index) else {
            return false;
        };
        entry.undo_list = undo_list;
        true
    }

    /// Current index.
    pub fn current_index(&self) -> Option<usize> {
        self.cursor.filter(|&pos| pos < self.entries.len())
    }

    /// Entries.
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Where history.
    pub fn where_history(&self) -> usize {
        self.cursor.unwrap_or(self.entries.len())
    }

    /// Current history.
    pub fn current_history(&self) -> Option<&HistoryEntry> {
        self.cursor.and_then(|idx| self.entries.get(idx))
    }

    /// Set pos.
    pub fn set_pos(&mut self, pos: usize) -> bool {
        if pos > self.entries.len() {
            return false;
        }
        self.restore_offset(pos);
        true
    }

    fn restore_offset(&mut self, pos: usize) {
        self.cursor = (pos != self.entries.len()).then_some(pos);
    }

    fn offset_beyond_end(&self) -> bool {
        self.where_history() > self.entries.len()
    }

    /// Previous history.
    pub fn previous_history(&mut self) -> Option<&HistoryEntry> {
        let pos = self.where_history();
        if pos == 0 || pos > self.entries.len() {
            return None;
        }
        self.cursor = Some(pos - 1);
        self.current_history()
    }

    /// Next history.
    pub fn next_history(&mut self) -> Option<&HistoryEntry> {
        let pos = self.cursor?;
        if pos > self.entries.len() {
            return None;
        }
        let next = pos + 1;
        if next >= self.entries.len() {
            self.cursor = None;
            return None;
        }
        self.cursor = Some(next);
        self.current_history()
    }

    /// Total bytes of resident lines plus resident timestamps (UTF-8 byte
    /// lengths, without terminators); this is content accounting, not a
    /// heap-allocation measure.
    pub fn total_bytes(&self) -> usize {
        self.entries
            .iter()
            .map(|entry| {
                entry.line_bytes.len() + entry.timestamp.as_ref().map(|ts| ts.len()).unwrap_or(0)
            })
            .sum()
    }

    fn push_entry(
        &mut self,
        line: Vec<u8>,
        timestamp: Option<String>,
        suppress_adjacent_duplicate: bool,
    ) {
        if !suppress_adjacent_duplicate
            || self.entries.last().map(|entry| entry.line_bytes.as_slice()) != Some(line.as_slice())
        {
            self.search_masks.push(byte_bigram_mask(&line));
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

    /// Reset cursor.
    pub fn reset_cursor(&mut self) {
        self.cursor = None;
        self.current_edit.clear();
    }

    /// Revert current edit.
    pub fn revert_current_edit(&mut self) {
        self.reset_cursor();
    }

    /// Navigate bytes.
    pub fn navigate_bytes(
        &mut self,
        direction: HistoryDirection,
        current: Vec<u8>,
    ) -> Option<Vec<u8>> {
        if self.entries.is_empty() || self.offset_beyond_end() {
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

    /// Beginning bytes.
    pub fn beginning_bytes(&mut self, current: Vec<u8>) -> Option<Vec<u8>> {
        if self.entries.is_empty() {
            self.cursor = None;
            return None;
        }
        if self.cursor.is_none() {
            self.current_edit = current;
        }
        self.cursor = Some(0);
        Some(self.entries[0].line_bytes.clone())
    }

    /// End bytes.
    pub fn end_bytes(&mut self) -> Option<Vec<u8>> {
        self.cursor?;
        self.cursor = None;
        Some(self.current_edit.clone())
    }

    /// Next after current cursor bytes.
    pub fn next_after_current_cursor_bytes(&self) -> Option<Vec<u8>> {
        let cursor = self.cursor?;
        self.entries
            .get(cursor.checked_add(1)?)
            .map(|entry| entry.line_bytes.clone())
    }

    /// Search prefix backward bytes.
    pub fn search_prefix_backward_bytes(
        &mut self,
        prefix: &[u8],
        current: Vec<u8>,
    ) -> Option<Vec<u8>> {
        self.search_prefix_backward_bytes_with_case(prefix, current, false)
    }

    /// Search prefix backward bytes with optional ASCII-only case-insensitive matching.
    pub fn search_prefix_backward_bytes_with_case(
        &mut self,
        prefix: &[u8],
        current: Vec<u8>,
        ignore_case: bool,
    ) -> Option<Vec<u8>> {
        if self.offset_beyond_end() {
            return None;
        }
        if self.entries.is_empty() {
            return None;
        }
        if self.cursor.is_none() {
            self.current_edit = current;
        }

        let start = self.cursor.unwrap_or(self.entries.len());
        let found = self.entries[..start]
            .iter()
            .rposition(|entry| starts_with_bytes(&entry.line_bytes, prefix, ignore_case))?;
        self.cursor = Some(found);
        Some(self.entries[found].line_bytes.clone())
    }

    /// Search prefix forward bytes.
    pub fn search_prefix_forward_bytes(&mut self, prefix: &[u8]) -> Option<Vec<u8>> {
        self.search_prefix_forward_bytes_with_case(prefix, false)
    }

    /// Search prefix forward bytes with optional ASCII-only case-insensitive matching.
    pub fn search_prefix_forward_bytes_with_case(
        &mut self,
        prefix: &[u8],
        ignore_case: bool,
    ) -> Option<Vec<u8>> {
        if self.offset_beyond_end() {
            return None;
        }
        let cursor = self.cursor?;

        if let Some(offset) = self.entries[cursor + 1..]
            .iter()
            .position(|entry| starts_with_bytes(&entry.line_bytes, prefix, ignore_case))
        {
            let found = cursor + 1 + offset;
            self.cursor = Some(found);
            return Some(self.entries[found].line_bytes.clone());
        }

        // A failed forward search from the newest entry returns to the saved
        // current edit (Bash 5.3 PTY: `alp` backward to `alpha two`, forward
        // past the newest match restores `alp`); from an older entry it
        // preserves the current line (cross-case fixtures stay at the typed
        // prefix instead of reverting to a stale saved edit).
        if cursor + 1 >= self.entries.len() {
            self.cursor = None;
            return Some(self.current_edit.clone());
        }
        None
    }

    /// Search containing backward index bytes.
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

    /// Search forward for a containing byte substring, exclusive of `after`.
    /// `None` starts at index zero; indexes at or beyond the last entry yield
    /// no match.
    pub fn search_containing_forward_index_bytes(
        &self,
        needle: &[u8],
        after: Option<usize>,
    ) -> Option<(usize, Vec<u8>)> {
        if needle.is_empty() {
            return None;
        }
        let start = after
            .map(|idx| idx.saturating_add(1))
            .unwrap_or(0)
            .min(self.entries.len());
        self.entries[start..]
            .iter()
            .enumerate()
            .find(|(_, entry)| find_bytes(&entry.line_bytes, needle).is_some())
            .map(|(offset, entry)| (start + offset, entry.line_bytes.clone()))
    }

    /// History search bytes.
    pub fn history_search_bytes(
        &mut self,
        needle: &[u8],
        direction: HistoryDirection,
    ) -> Option<HistorySearchMatch> {
        self.history_search_bytes_with_case(needle, direction, false)
    }

    /// History search bytes with optional ASCII-only case-insensitive matching.
    ///
    /// Like GNU `history_search`, the positioned entry is included in both
    /// directions; repeat commands express their step-past policy at their
    /// own call sites instead.
    pub fn history_search_bytes_with_case(
        &mut self,
        needle: &[u8],
        direction: HistoryDirection,
        ignore_case: bool,
    ) -> Option<HistorySearchMatch> {
        if self.offset_beyond_end() {
            return None;
        }
        let start = self.where_history();
        let found =
            self.search_from_pos_bytes_with_case(needle, direction, start, false, ignore_case)?;
        self.cursor = Some(found.entry_index);
        Some(found)
    }

    /// History search prefix.
    pub fn history_search_prefix(
        &mut self,
        prefix: &str,
        direction: HistoryDirection,
    ) -> Option<HistorySearchMatch> {
        if self.offset_beyond_end() {
            return None;
        }
        let start = self.where_history();
        let found = self.search_from_pos(prefix, direction, start, true)?;
        self.cursor = Some(found.entry_index);
        Some(found)
    }

    /// Search from a zero-based history position, including that entry, without
    /// moving the current cursor. A one-past-end position searches backward
    /// from the newest entry and has no forward entries. Positions beyond the
    /// history end fall back to the current cursor, matching GNU History. If
    /// that cursor is beyond the end, the fallback returns `None`.
    pub fn history_search_pos(
        &self,
        needle: &str,
        direction: HistoryDirection,
        pos: usize,
    ) -> Option<HistorySearchMatch> {
        let start = if pos > self.entries.len() {
            if self.offset_beyond_end() {
                return None;
            }
            self.where_history()
        } else {
            pos
        };
        self.search_from_pos(needle, direction, start, false)
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
                // A one-byte ignore-case query has no bigram bits to filter on.
                if !anchored && (!ignore_case || needle.len() > 1) {
                    let needle_mask = byte_bigram_mask(needle);
                    let finder = memchr::memmem::FinderRev::new(needle);
                    return self.entries[..=end].iter().enumerate().rev().find_map(
                        |(idx, entry)| {
                            if self.search_masks[idx] & needle_mask != needle_mask {
                                return None;
                            }
                            let byte_offset = if ignore_case {
                                rfind_bytes_with_case(&entry.line_bytes, needle, ignore_case)
                            } else {
                                finder.rfind(&entry.line_bytes)
                            }?;
                            Some(HistorySearchMatch {
                                entry_index: idx,
                                byte_offset,
                                line_bytes: entry.line_bytes.clone(),
                            })
                        },
                    );
                }
                self.entries[..=end]
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(idx, entry)| {
                        let byte_offset = if anchored {
                            starts_with_bytes(&entry.line_bytes, needle, ignore_case).then_some(0)
                        } else {
                            // GNU scans backwards from the end of the line,
                            // so the offset is the last occurrence.
                            rfind_bytes_with_case(&entry.line_bytes, needle, ignore_case)
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

    /// Search containing forward from cursor bytes without wrapping at history end.
    pub fn search_containing_forward_from_cursor_bytes(
        &mut self,
        needle: &[u8],
        current: Vec<u8>,
    ) -> Option<Vec<u8>> {
        self.search_containing_forward_from_cursor_bytes_with_case(needle, current, false)
    }

    /// Search containing forward from cursor bytes with optional ASCII-only case-insensitive matching.
    pub fn search_containing_forward_from_cursor_bytes_with_case(
        &mut self,
        needle: &[u8],
        current: Vec<u8>,
        ignore_case: bool,
    ) -> Option<Vec<u8>> {
        if self.offset_beyond_end() {
            return None;
        }
        if self.entries.is_empty() {
            return None;
        }
        if self.cursor.is_none() {
            self.current_edit = current;
            return None;
        }
        let start = self.cursor?;
        let current_matches =
            find_bytes_with_case(&self.entries[start].line_bytes, needle, ignore_case).is_some();
        let found = self.entries[start + usize::from(current_matches)..]
            .iter()
            .position(|entry| {
                find_bytes_with_case(&entry.line_bytes, needle, ignore_case).is_some()
            })
            .map(|offset| start + usize::from(current_matches) + offset)?;
        self.cursor = Some(found);
        Some(self.entries[found].line_bytes.clone())
    }

    /// Search containing backward from cursor bytes.
    pub fn search_containing_backward_from_cursor_bytes(
        &mut self,
        needle: &[u8],
        current: Vec<u8>,
    ) -> Option<Vec<u8>> {
        self.search_containing_backward_from_cursor_bytes_with_case(needle, current, false)
    }

    /// Search containing backward from cursor bytes with optional ASCII-only case-insensitive matching.
    pub fn search_containing_backward_from_cursor_bytes_with_case(
        &mut self,
        needle: &[u8],
        current: Vec<u8>,
        ignore_case: bool,
    ) -> Option<Vec<u8>> {
        if self.offset_beyond_end() {
            return None;
        }
        if self.entries.is_empty() {
            return None;
        }
        if self.cursor.is_none() {
            self.current_edit = current;
        }
        let start = self.cursor.unwrap_or(self.entries.len());
        let found = self.entries[..start].iter().rposition(|entry| {
            find_bytes_with_case(&entry.line_bytes, needle, ignore_case).is_some()
        })?;
        self.cursor = Some(found);
        Some(self.entries[found].line_bytes.clone())
    }

    /// Get 1 based entry.
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
    if !ignore_case {
        return memchr::memmem::find(haystack, needle);
    }
    haystack
        .windows(needle.len())
        .position(|window| bytes_equal(window, needle, ignore_case))
}

fn rfind_bytes_with_case(haystack: &[u8], needle: &[u8], ignore_case: bool) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if !ignore_case {
        return memchr::memmem::rfind(haystack, needle);
    }
    if needle.len() == 1 {
        let lower = needle[0].to_ascii_lowercase();
        let upper = needle[0].to_ascii_uppercase();
        return if lower == upper {
            memchr::memrchr(lower, haystack)
        } else {
            memchr::memrchr2(lower, upper, haystack)
        };
    }
    haystack
        .windows(needle.len())
        .rposition(|window| bytes_equal(window, needle, ignore_case))
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

// A substring contains all of its needle's ASCII-folded byte pairs. These are
// only a prefilter: exact matching preserves semantics in both case modes.
// Hash collisions admit extra checks; zero pairs (including one-byte needles) filter none.
fn byte_bigram_mask(bytes: &[u8]) -> u64 {
    bytes.windows(2).fold(0, |mask, pair| {
        let k =
            u16::from_be_bytes([pair[0].to_ascii_lowercase(), pair[1].to_ascii_lowercase()]) as u32;
        let product = k.wrapping_mul(0x9e37_79b1);
        let h = product ^ (product >> 16);
        mask | (1u64 << (h & 63))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        History, HistoryDirection, HistorySearchMatch, find_bytes_with_case, rfind_bytes_with_case,
    };

    #[test]
    fn previous_search_matches_reference_after_mutations() {
        fn check(history: &History) {
            let mut needles: Vec<&[u8]> = vec![
                b"",
                b"a",
                b"z",
                b"abc",
                b"ABC",
                b"aba",
                b"replaced",
                b"absent",
                b"\x00\xff",
                b"\xff",
                b"\xff\x80",
                b"\x80\x00",
            ];
            // Every substring must survive the filter, including raw bytes.
            for entry in history.entries() {
                for start in 0..entry.line_bytes.len() {
                    for end in start + 1..=entry.line_bytes.len() {
                        needles.push(&entry.line_bytes[start..end]);
                    }
                }
            }
            for needle in needles {
                for pos in 0..=history.len() {
                    for (anchored, ignore_case) in [(false, false), (true, false), (false, true)] {
                        let expected = history.entries().iter().enumerate().rev().find_map(
                            |(index, entry)| {
                                if index > pos || needle.is_empty() {
                                    return None;
                                }
                                let byte_offset =
                                    entry.line_bytes.windows(needle.len()).rposition(|bytes| {
                                        if ignore_case {
                                            bytes.eq_ignore_ascii_case(needle)
                                        } else {
                                            bytes == needle
                                        }
                                    })?;
                                let byte_offset = if anchored {
                                    entry.line_bytes.starts_with(needle).then_some(0)?
                                } else {
                                    byte_offset
                                };
                                Some(HistorySearchMatch {
                                    entry_index: index,
                                    byte_offset,
                                    line_bytes: entry.line_bytes.clone(),
                                })
                            },
                        );
                        assert_eq!(
                            history.search_from_pos_bytes_with_case(
                                needle,
                                HistoryDirection::Previous,
                                pos,
                                anchored,
                                ignore_case,
                            ),
                            expected,
                            "needle={needle:?}, pos={pos}, anchored={anchored}, ignore_case={ignore_case}",
                        );
                    }
                }
            }
        }

        let mut history = History::new();
        for line in [
            b"old abc".as_slice(),
            b"ababa",
            b"\x00\xffa\x00\xff",
            b"ABC",
            b"",
        ] {
            history.push_bytes(line.to_vec());
            check(&history);
        }
        // A suppressed append must not insert a mask of its own.
        history.push_entry(Vec::new(), None, true);
        check(&history);
        history.enforce_max_len(Some(4));
        check(&history);
        history.stifle(4);
        history.push("new abc abc");
        check(&history);
        assert!(history.replace(0, "replaced aba aba").is_some());
        check(&history);
        assert!(history.set_line_bytes(1, b"\xff\x80\x00\xff\x80".to_vec()));
        check(&history);
        assert!(history.set_line_bytes(1, b"\xff\x80\x00\xff\x80".to_vec()));
        assert!(history.remove(2).is_some());
        check(&history);
        assert!(history.remove(history.len()).is_none());
        assert!(history.replace(history.len(), "invalid").is_none());
        assert!(!history.set_line_bytes(history.len(), vec![0]));
        check(&history);
        history.clear();
        check(&history);
        history.push_bytes(b"\x80\x00abcabc".to_vec());
        history.push("ABC");
        check(&history);
        history.stifle(0);
        history.push("discarded");
        check(&history);
    }

    #[test]
    fn byte_substring_matching_preserves_boundaries_and_case() {
        for ignore_case in [false, true] {
            assert_eq!(rfind_bytes_with_case(b"", b"", ignore_case), Some(0));
            assert_eq!(rfind_bytes_with_case(b"abc", b"", ignore_case), Some(0));
            assert_eq!(rfind_bytes_with_case(b"", b"a", ignore_case), None);
            assert_eq!(rfind_bytes_with_case(b"ab", b"abc", ignore_case), None);
            assert_eq!(
                rfind_bytes_with_case(b"\xffabcabc", b"abc", ignore_case),
                Some(4)
            );
            assert_eq!(
                rfind_bytes_with_case(b"ababa", b"aba", ignore_case),
                Some(2)
            );
            assert_eq!(
                rfind_bytes_with_case(b"\x00\xffa\x00\xff", b"\x00\xff", ignore_case),
                Some(3)
            );
            assert_eq!(
                rfind_bytes_with_case("Ä".as_bytes(), "ä".as_bytes(), ignore_case),
                None
            );
            assert_eq!(find_bytes_with_case(b"", b"", ignore_case), Some(0));
            assert_eq!(find_bytes_with_case(b"abc", b"", ignore_case), Some(0));
            assert_eq!(find_bytes_with_case(b"", b"a", ignore_case), None);
            assert_eq!(find_bytes_with_case(b"ab", b"abc", ignore_case), None);
            assert_eq!(
                find_bytes_with_case(b"\xffabcabc", b"abc", ignore_case),
                Some(1)
            );
            assert_eq!(
                find_bytes_with_case(b"a\x00\xff", b"\x00\xff", ignore_case),
                Some(1)
            );
            assert_eq!(
                find_bytes_with_case("Ä".as_bytes(), "ä".as_bytes(), ignore_case),
                None
            );
        }
        assert_eq!(rfind_bytes_with_case(b"aA", b"a", true), Some(1));
        assert_eq!(rfind_bytes_with_case(b"aA", b"A", true), Some(1));
        assert_eq!(rfind_bytes_with_case(b"\xff\x80\xff", b"\xff", true), Some(2));
        assert_eq!(rfind_bytes_with_case(b"aA", b"a", false), Some(0));
        assert_eq!(rfind_bytes_with_case(b"xABC", b"abc", false), None);
        assert_eq!(rfind_bytes_with_case(b"xABCabc", b"ABC", false), Some(1));
        assert_eq!(rfind_bytes_with_case(b"xABCabc", b"ABC", true), Some(4));
        assert_eq!(rfind_bytes_with_case(b"aBaBa", b"AbA", true), Some(2));
        assert_eq!(find_bytes_with_case(b"xABC", b"abc", false), None);
        assert_eq!(find_bytes_with_case(b"xABC", b"abc", true), Some(1));
    }
}
