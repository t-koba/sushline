use crate::buffer::LineBuffer;
use crate::completion::{CompletionResponse, CompletionType};
use crate::keymap::{EditCommand, KeyBinding};
use crate::prompt::Prompt;
use crate::terminal::TerminalSize;
use history::HistoryUndoEntry;
use std::collections::BTreeMap;

mod completion;
mod kill;
mod repeat;
mod search;
mod undo;
mod vi;

pub(crate) use completion::*;
pub(crate) use kill::*;
pub(crate) use repeat::*;
pub(crate) use search::*;
pub(crate) use undo::*;
pub(crate) use vi::*;

pub(crate) struct EditorState {
    pub(crate) prompt: Prompt,
    pub(crate) buffer: LineBuffer,
    pub(crate) input: InputState,
    pub(crate) kill: KillRingState,
    pub(crate) undo: UndoState,
    pub(crate) search: SearchState,
    pub(crate) completion: CompletionState,
    pub(crate) vi: ViModeState,
    pub(crate) macro_state: MacroState,
    pub(crate) paste: BracketedPasteState,
    pub(crate) display: DisplayState,
    pub(crate) numeric_arg: Option<i32>,
    pub(crate) mark: Option<usize>,
    pub(crate) overwrite_mode: bool,
    pub(crate) original_line: Vec<u8>,
}

#[derive(Debug, Default)]
pub(crate) struct InputState {
    pub(crate) quoted_insert: bool,
    pub(crate) pending_key: Vec<u8>,
    pub(crate) skipping_csi: bool,
    pub(crate) csi_sequence_started: bool,
    pub(crate) pending_replace: bool,
    pub(crate) named_command: Option<String>,
    pub(crate) prefix_meta: bool,
}

#[derive(Debug, Default)]
pub(crate) struct KillRingState {
    pub(crate) kill_ring: Vec<Vec<u8>>,
    pub(crate) last_was_kill: bool,
    pub(crate) last_yank: Option<YankState>,
}

#[derive(Debug, Default)]
pub(crate) struct UndoState {
    pub(crate) undo_stack: Vec<UndoEntry>,
    pub(crate) pending_undo: Option<LineBuffer>,
    pub(crate) last_undo_was_insert: bool,
}

#[derive(Debug, Default)]
pub(crate) struct SearchState {
    pub(crate) reverse_search: Option<ReverseSearchState>,
    pub(crate) non_incremental_search: Option<NonIncrementalSearchState>,
    pub(crate) last_search: Option<Vec<u8>>,
    pub(crate) last_search_direction: Option<SearchDirection>,
}

#[derive(Debug, Default)]
pub(crate) struct CompletionState {
    pub(crate) last_completion: Option<CompletionResponse>,
    pub(crate) last_attempt: Option<CompletionAttemptState>,
    pub(crate) menu_completion: Option<MenuCompletionState>,
    pub(crate) last_yank_arg: Option<LastYankArgState>,
}

#[derive(Debug)]
pub(crate) struct CompletionAttemptState {
    pub(crate) completion_type: CompletionType,
    pub(crate) line: Vec<u8>,
    pub(crate) point: usize,
    pub(crate) unmodified: bool,
}

#[derive(Debug, Default)]
pub(crate) struct ViModeState {
    pub(crate) pending_char_search: Option<CharSearchMode>,
    pub(crate) pending_char_search_operator: Option<(ViOperator, usize, Vec<u8>)>,
    pub(crate) last_char_search: Option<(CharSearchMode, char)>,
    pub(crate) vi_operator: Option<ViOperator>,
    pub(crate) vi_operator_key: Option<Vec<u8>>,
    pub(crate) vi_count_keys: Vec<u8>,
    pub(crate) last_vi_change: Option<Vec<u8>>,
    pub(crate) vi_insert_change: Option<Vec<u8>>,
    pub(crate) pending_vi_mark: Option<ViMarkAction>,
    pub(crate) pending_mark_operator: Option<(ViOperator, usize, Vec<u8>)>,
    pub(crate) vi_marks: BTreeMap<char, usize>,
    pub(crate) pending_vi_register: bool,
    pub(crate) active_vi_register: Option<char>,
    pub(crate) vi_registers: BTreeMap<char, Vec<u8>>,
}

#[derive(Debug, Default)]
pub(crate) struct MacroState {
    pub(crate) keyboard_macro: Option<Vec<u8>>,
    pub(crate) last_keyboard_macro: Option<Vec<u8>>,
    pub(crate) replaying_macro: bool,
}

#[derive(Debug, Default)]
pub(crate) struct BracketedPasteState {
    pub(crate) bracketed_paste: bool,
    pub(crate) bracketed_paste_start: Option<usize>,
    pub(crate) bracketed_paste_pending: Vec<u8>,
}

#[derive(Debug, Default)]
pub(crate) struct DisplayState {
    pub(crate) rendered_rows: u16,
    pub(crate) last_terminal_size: Option<TerminalSize>,
}

impl EditorState {
    pub(crate) fn new(prompt: Prompt, initial_line: Option<Vec<u8>>) -> Self {
        let original_line = initial_line.unwrap_or_default();
        Self {
            prompt,
            buffer: LineBuffer::from_bytes(original_line.clone()),
            input: InputState::default(),
            kill: KillRingState::default(),
            undo: UndoState::default(),
            search: SearchState::default(),
            completion: CompletionState::default(),
            vi: ViModeState::default(),
            macro_state: MacroState::default(),
            paste: BracketedPasteState::default(),
            display: DisplayState::default(),
            numeric_arg: None,
            mark: None,
            overwrite_mode: false,
            original_line,
        }
    }

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

    pub(crate) fn cancel_pending_command(&mut self) {
        self.input.pending_key.clear();
        self.input.quoted_insert = false;
        self.input.skipping_csi = false;
        self.input.csi_sequence_started = false;
        self.numeric_arg = None;
        self.search.reverse_search = None;
        self.search.non_incremental_search = None;
        self.input.named_command = None;
        self.input.pending_replace = false;
        self.vi.pending_char_search = None;
        self.vi.pending_char_search_operator = None;
        self.vi.pending_vi_mark = None;
        self.vi.pending_mark_operator = None;
        self.vi.pending_vi_register = false;
        self.vi.vi_operator = None;
        self.vi.vi_operator_key = None;
        self.vi.vi_count_keys.clear();
        self.input.prefix_meta = false;
        self.after_non_kill_command();
    }

    pub(crate) fn after_self_insert(&mut self) {
        self.kill.last_was_kill = false;
        self.kill.last_yank = None;
        self.search.reverse_search = None;
        self.undo.last_undo_was_insert = true;
        self.completion.last_completion = None;
        self.completion.last_attempt = None;
        self.completion.menu_completion = None;
    }

    pub(crate) fn region_bounds(&self) -> Option<(usize, usize)> {
        let mark = self.mark?;
        let point = self.buffer.point();
        if mark == point {
            return None;
        }
        Some((mark.min(point), mark.max(point)))
    }

    pub(crate) fn start_keyboard_macro(&mut self) {
        self.macro_state.keyboard_macro = Some(Vec::new());
    }

    pub(crate) fn end_keyboard_macro(&mut self) {
        if let Some(macro_bytes) = self.macro_state.keyboard_macro.take() {
            self.macro_state.last_keyboard_macro = Some(macro_bytes);
        }
    }

    pub(crate) fn record_macro_binding(&mut self, key: &[u8], binding: &KeyBinding) {
        if self.macro_state.keyboard_macro.is_none() || self.macro_state.replaying_macro {
            return;
        }
        if matches!(
            binding,
            KeyBinding::Command(EditCommand::StartKbdMacro | EditCommand::EndKbdMacro)
        ) {
            return;
        }

        if let Some(macro_bytes) = self.macro_state.keyboard_macro.as_mut() {
            macro_bytes.extend_from_slice(key);
        }
    }

    pub(crate) fn begin_vi_insert_change(&mut self, key: &[u8]) {
        if self.macro_state.replaying_macro {
            return;
        }
        let mut change = std::mem::take(&mut self.vi.vi_count_keys);
        change.extend_from_slice(key);
        self.vi.vi_insert_change = Some(change);
    }

    pub(crate) fn record_vi_insert_bytes(&mut self, key: &[u8]) {
        if let Some(change) = self.vi.vi_insert_change.as_mut() {
            change.extend_from_slice(key);
        }
    }

    pub(crate) fn finish_vi_insert_change(&mut self, key: &[u8]) {
        if let Some(mut change) = self.vi.vi_insert_change.take() {
            change.extend_from_slice(key);
            self.vi.last_vi_change = Some(change);
        }
    }

    pub(crate) fn vi_key_sequence_for_change(&mut self, key: &[u8]) -> Vec<u8> {
        let mut change = std::mem::take(&mut self.vi.vi_count_keys);
        change.extend_from_slice(key);
        change
    }

    pub(crate) fn consume_csi_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            if !self.input.csi_sequence_started {
                if *byte == 0x1b {
                    continue;
                }
                if matches!(*byte, b'[' | b'O') {
                    self.input.csi_sequence_started = true;
                    continue;
                }
                self.input.skipping_csi = false;
                break;
            }
            if (0x40..=0x7e).contains(byte) {
                self.input.skipping_csi = false;
                self.input.csi_sequence_started = false;
                break;
            }
        }
    }

    pub(crate) fn finish_vi_operator_change(&mut self, op: ViOperator, change: Vec<u8>) {
        if matches!(op, ViOperator::Change) {
            self.vi.vi_insert_change = Some(change.clone());
        }
        self.vi.last_vi_change = Some(change);
    }

    pub(crate) fn set_vi_operator(&mut self, op: ViOperator, key: &[u8]) {
        self.vi.vi_operator = Some(op);
        let mut change = std::mem::take(&mut self.vi.vi_count_keys);
        change.extend_from_slice(key);
        self.vi.vi_operator_key = Some(change);
    }

    pub(crate) fn take_vi_operator(&mut self) -> Option<(ViOperator, usize, Vec<u8>)> {
        let op = self.vi.vi_operator.take()?;
        let key = self.vi.vi_operator_key.take().unwrap_or_default();
        Some((op, self.buffer.point(), key))
    }

    pub(crate) fn vi_operator_prompt(&self) -> Option<&'static str> {
        match self.vi.vi_operator? {
            ViOperator::Change => Some("c"),
            ViOperator::Delete => Some("d"),
            ViOperator::Yank => Some("y"),
        }
    }

    pub(crate) fn after_non_kill_command(&mut self) {
        self.commit_pending_undo();
        self.kill.last_was_kill = false;
        self.kill.last_yank = None;
        self.search.reverse_search = None;
        self.undo.last_undo_was_insert = false;
    }

    pub(crate) fn push_kill(&mut self, text: impl Into<Vec<u8>>, direction: KillDirection) {
        let text = text.into();
        self.kill.last_yank = None;
        if text.is_empty() {
            self.kill.last_was_kill = true;
            return;
        }
        self.store_active_vi_register(&text);

        if self.kill.last_was_kill
            && let Some(last) = self.kill.kill_ring.last_mut()
        {
            match direction {
                KillDirection::Forward => last.extend_from_slice(&text),
                KillDirection::Backward => {
                    last.splice(0..0, text.iter().copied());
                }
            }
        } else {
            self.kill.kill_ring.push(text);
        }
        self.kill.last_was_kill = true;
    }

    pub(crate) fn store_active_vi_register(&mut self, text: &[u8]) {
        let Some(register) = self.vi.active_vi_register.take() else {
            return;
        };
        if matches!(register, '_' | ':' | '.' | '%' | '#') {
            return;
        }
        let key = register.to_ascii_lowercase();
        if register.is_ascii_uppercase() {
            self.vi
                .vi_registers
                .entry(key)
                .or_default()
                .extend_from_slice(text);
        } else {
            self.vi.vi_registers.insert(key, text.to_vec());
        }
    }

    pub(crate) fn yank(&mut self) {
        self.kill.last_was_kill = false;
        let Some(index) = self.kill.kill_ring.len().checked_sub(1) else {
            self.kill.last_yank = None;
            return;
        };
        self.yank_from_index(index);
    }

    pub(crate) fn yank_pop(&mut self) {
        self.kill.last_was_kill = false;
        let Some(last_yank) = self.kill.last_yank else {
            return;
        };
        if self.kill.kill_ring.is_empty() {
            self.kill.last_yank = None;
            return;
        }

        let next_index = if last_yank.kill_index == 0 {
            self.kill.kill_ring.len() - 1
        } else {
            last_yank.kill_index - 1
        };
        let text = self.kill.kill_ring[next_index].clone();
        self.buffer
            .replace_range_bytes(last_yank.start, last_yank.end, &text);
        self.kill.last_yank = Some(YankState {
            start: last_yank.start,
            end: last_yank.start + text.len(),
            kill_index: next_index,
        });
    }

    pub(crate) fn yank_from_index(&mut self, index: usize) {
        let text = self.kill.kill_ring[index].clone();
        let start = self.buffer.point();
        repeat(self, |state| {
            state.buffer.insert_bytes(&text);
        });
        self.kill.last_yank = Some(YankState {
            start,
            end: self.buffer.point(),
            kill_index: index,
        });
    }

    pub(crate) fn vi_put(&mut self) {
        self.kill.last_was_kill = false;
        if let Some(register) = self.vi.active_vi_register.take()
            && let Some(text) = self
                .vi
                .vi_registers
                .get(&register.to_ascii_lowercase())
                .cloned()
        {
            repeat(self, |state| {
                state.buffer.insert_bytes(&text);
            });
            self.kill.last_yank = None;
            return;
        }
        self.yank();
    }

    pub(crate) fn vi_put_before(&mut self) {
        self.kill.last_was_kill = false;
        if let Some(register) = self.vi.active_vi_register.take()
            && let Some(text) = self
                .vi
                .vi_registers
                .get(&register.to_ascii_lowercase())
                .cloned()
        {
            repeat(self, |state| {
                state.buffer.insert_bytes(&text);
            });
            self.kill.last_yank = None;
            return;
        }
        let Some(index) = self.kill.kill_ring.len().checked_sub(1) else {
            self.kill.last_yank = None;
            return;
        };
        self.yank_from_index(index);
    }
}
