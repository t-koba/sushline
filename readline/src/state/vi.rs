use std::collections::BTreeMap;

use super::EditorState;

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
