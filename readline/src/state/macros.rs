use crate::keymap::{EditCommand, KeyBinding};

use super::EditorState;

/// Maximum number of simultaneously active inputrc macro expansions.
///
/// GNU Readline defines `MAX_MACRO_LEVEL` as 16 and rejects the push once
/// `macro_level > MAX_MACRO_LEVEL`, so 17 nested macro inputs can be active
/// while the 18th entry fails.
pub(crate) const MAX_INPUTRC_MACRO_DEPTH: usize = 17;

#[derive(Debug, Default)]
pub(crate) struct MacroState {
    pub(crate) keyboard_macro: Option<Vec<u8>>,
    pub(crate) last_keyboard_macro: Option<Vec<u8>>,
    pub(crate) replaying_macro: bool,
    pub(crate) last_recorded_self_insert: bool,
    pub(crate) inputrc_macro_depth: usize,
    /// Start of a still-pending numeric prefix recorded in the active macro.
    pub(crate) numeric_arg_recording_start: Option<usize>,
}

impl EditorState {
    /// Discard an active keyboard-macro definition without promoting it.
    ///
    /// GNU `rl_start_kbd_macro` calls `_rl_abort_internal()` when invoked
    /// while already defining (`RL_STATE_MACRODEF`); the partial definition
    /// is dropped and the completed definition, if any, is left intact.
    pub(crate) fn abort_keyboard_macro_definition(&mut self) {
        self.macro_state.keyboard_macro = None;
        self.macro_state.last_recorded_self_insert = false;
        self.macro_state.numeric_arg_recording_start = None;
    }

    /// Start a fresh keyboard-macro definition.
    ///
    /// The caller handles the active-recording case via
    /// [`Self::abort_keyboard_macro_definition`]; this helper only resets to
    /// a fresh (or appended) definition when not already recording.
    pub(crate) fn start_keyboard_macro(&mut self, append_previous: bool) -> Option<Vec<u8>> {
        let previous_macro = append_previous
            .then(|| self.macro_state.last_keyboard_macro.clone())
            .flatten();
        self.macro_state.keyboard_macro = Some(previous_macro.clone().unwrap_or_default());
        self.macro_state.last_recorded_self_insert = false;
        self.macro_state.numeric_arg_recording_start = None;
        previous_macro
    }

    pub(crate) fn end_keyboard_macro(&mut self) {
        if let Some(start) = self.macro_state.numeric_arg_recording_start.take()
            && let Some(macro_bytes) = self.macro_state.keyboard_macro.as_mut()
        {
            // The numeric prefix belongs to end-kbd-macro's argument, not
            // to the definition being recorded.
            macro_bytes.truncate(start);
        }
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

        if matches!(
            binding,
            KeyBinding::Command(EditCommand::DigitArgument | EditCommand::UniversalArgument)
        ) && self.numeric_arg.is_none()
            && self.macro_state.numeric_arg_recording_start.is_none()
        {
            self.macro_state.numeric_arg_recording_start =
                self.macro_state.keyboard_macro.as_ref().map(Vec::len);
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
