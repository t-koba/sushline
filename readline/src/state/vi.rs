use std::collections::BTreeMap;

use super::EditorState;

#[derive(Debug, Default)]
pub(crate) struct ViModeState {
    pub(crate) pending_char_search: Option<CharSearchMode>,
    /// Numeric count for both named character-search commands and vi
    /// `f`/`F`/`t`/`T`; absent means a single search without a count.
    pub(crate) pending_char_search_count: Option<i32>,
    pub(crate) pending_char_search_operator: Option<(ViOperator, usize, Vec<u8>)>,
    pub(crate) last_char_search: Option<(CharSearchMode, char)>,
    pub(crate) vi_operator: Option<ViOperator>,
    pub(crate) vi_operator_key: Option<Vec<u8>>,
    pub(crate) vi_count_keys: Vec<u8>,
    pub(crate) last_vi_change: Option<Vec<u8>>,
    /// GNU records a cancelled vi operator as a redo that appends a blank at
    /// the end of the line and rings, replacing the previous change.
    pub(crate) last_change_is_cancelled_operator: bool,
    /// Whether the redo change was synthesized from an uninterrupted initial
    /// insertion rather than recorded from an explicit vi command.
    pub(crate) last_change_is_initial_insert: bool,
    pub(crate) vi_insert_change: Option<Vec<u8>>,
    /// True while `replay_vi_change` feeds a recorded command back in; new
    /// changes recorded during replay must not reset the replay counter.
    pub(crate) replaying_vi_change: bool,
    /// Number of zero-width redo replays since the original change; GNU adds
    /// an invisible undo step from the second replay onward.
    pub(crate) zero_width_redo_replays: u32,
    pub(crate) pending_vi_mark: Option<ViMarkAction>,
    pub(crate) pending_mark_operator: Option<(ViOperator, usize, Vec<u8>)>,
    pub(crate) vi_marks: BTreeMap<char, usize>,
    pub(crate) command_mode_entered: bool,
    /// Bytes inserted before this read's first vi command-mode entry.
    pub(crate) initial_insert_bytes: Vec<u8>,
    pub(crate) defer_undo_commit: bool,
    pub(crate) pending_vi_register: bool,
    pub(crate) active_vi_register: Option<char>,
    pub(crate) vi_registers: BTreeMap<char, Vec<u8>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ViMarkAction {
    Set,
    Goto,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CharSearchMode {
    Forward,
    Backward,
    TillForward,
    TillBackward,
}

impl CharSearchMode {
    pub(crate) fn reversed(self) -> Self {
        match self {
            Self::Forward => Self::Backward,
            Self::Backward => Self::Forward,
            Self::TillForward => Self::TillBackward,
            Self::TillBackward => Self::TillForward,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViOperator {
    Change,
    Delete,
    Yank,
}

impl EditorState {
    fn note_new_vi_change(&mut self) {
        if !self.vi.replaying_vi_change {
            self.vi.zero_width_redo_replays = 0;
        }
    }

    pub(crate) fn set_vi_redo(&mut self, change: Vec<u8>) {
        self.note_new_vi_change();
        self.vi.last_vi_change = Some(change);
        self.vi.last_change_is_cancelled_operator = false;
        self.vi.last_change_is_initial_insert = false;
    }

    pub(crate) fn set_initial_insert_vi_redo(&mut self, change: Vec<u8>) {
        self.vi.last_vi_change = Some(change);
        self.vi.last_change_is_cancelled_operator = false;
        self.vi.last_change_is_initial_insert = true;
    }

    pub(crate) fn mark_vi_redo_cancelled_operator(&mut self) {
        self.vi.last_vi_change = Some(Vec::new());
        self.vi.last_change_is_cancelled_operator = true;
        self.vi.last_change_is_initial_insert = false;
    }

    pub(crate) fn begin_vi_insert_change(&mut self, key: &[u8]) {
        if self.macro_state.replaying_macro {
            return;
        }
        self.note_new_vi_change();
        let mut change = std::mem::take(&mut self.vi.vi_count_keys);
        change.extend_from_slice(key);
        self.vi.vi_insert_change = Some(change);
    }

    pub(crate) fn record_vi_insert_bytes(&mut self, key: &[u8]) {
        if !self.vi.command_mode_entered {
            self.vi.initial_insert_bytes.extend_from_slice(key);
        }
        if let Some(change) = self.vi.vi_insert_change.as_mut() {
            change.extend_from_slice(key);
        }
    }

    pub(crate) fn finish_vi_insert_change(&mut self, key: &[u8]) {
        if let Some(mut change) = self.vi.vi_insert_change.take() {
            change.extend_from_slice(key);
            self.set_vi_redo(change);
        }
    }

    /// Replace the count recorded in a vi change sequence. GNU keeps an
    /// explicit count typed before `.` and remembers it for later replays.
    pub(crate) fn vi_change_with_count(change: &[u8], count: i32) -> Vec<u8> {
        let mut digits = change;
        if digits.first() == Some(&b'-') {
            digits = &digits[1..];
        }
        let digit_len = digits
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        let prefix_len = change.len() - digits.len() + digit_len;
        let mut result = count.to_string().into_bytes();
        result.extend_from_slice(&change[prefix_len..]);
        result
    }

    pub(crate) fn vi_key_sequence_for_change(&mut self, key: &[u8]) -> Vec<u8> {
        let mut change = std::mem::take(&mut self.vi.vi_count_keys);
        change.extend_from_slice(key);
        change
    }

    pub(crate) fn finish_vi_operator_change(&mut self, op: ViOperator, change: Vec<u8>) {
        if matches!(op, ViOperator::Change) {
            self.vi.vi_insert_change = Some(change.clone());
        }
        self.note_new_vi_change();
        self.set_vi_redo(change);
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
}
