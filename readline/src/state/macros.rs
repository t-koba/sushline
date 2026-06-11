use crate::keymap::{EditCommand, KeyBinding};

use super::EditorState;

#[derive(Debug, Default)]
pub(crate) struct MacroState {
    pub(crate) keyboard_macro: Option<Vec<u8>>,
    pub(crate) last_keyboard_macro: Option<Vec<u8>>,
    pub(crate) replaying_macro: bool,
    pub(crate) last_recorded_self_insert: bool,
}

impl EditorState {
    pub(crate) fn start_keyboard_macro(&mut self) {
        self.macro_state.keyboard_macro = Some(Vec::new());
        self.macro_state.last_recorded_self_insert = false;
    }

    pub(crate) fn end_keyboard_macro(&mut self) {
        if let Some(macro_bytes) = self.macro_state.keyboard_macro.take() {
            self.macro_state.last_keyboard_macro = Some(macro_bytes);
        }
        self.macro_state.last_recorded_self_insert = false;
    }

    pub(crate) fn record_macro_binding(&mut self, key: &[u8], binding: &KeyBinding) {
        if self.macro_state.keyboard_macro.is_none() || self.macro_state.replaying_macro {
            return;
        }
        if matches!(
            binding,
            KeyBinding::Command(
                EditCommand::StartKbdMacro
                    | EditCommand::EndKbdMacro
                    | EditCommand::CallLastKbdMacro
            )
        ) {
            return;
        }

        let is_self_insert = matches!(binding, KeyBinding::Command(EditCommand::SelfInsert));
        if is_self_insert && self.macro_state.last_recorded_self_insert {
            return;
        }
        if let Some(macro_bytes) = self.macro_state.keyboard_macro.as_mut() {
            macro_bytes.extend_from_slice(key);
        }
        self.macro_state.last_recorded_self_insert = is_self_insert;
    }

    pub(crate) fn record_macro_insert_bytes(&mut self, bytes: &[u8]) {
        if self.macro_state.keyboard_macro.is_none() || self.macro_state.replaying_macro {
            return;
        }
        if self.macro_state.last_recorded_self_insert {
            return;
        }
        if let Some(macro_bytes) = self.macro_state.keyboard_macro.as_mut() {
            macro_bytes.extend_from_slice(bytes);
        }
        self.macro_state.last_recorded_self_insert = true;
    }
}
