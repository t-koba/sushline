use crate::buffer::LineBuffer;
use crate::prompt::Prompt;

mod completion;
mod display;
mod input;
mod kill;
mod macros;
mod paste;
mod repeat;
mod search;
mod undo;
mod vi;

pub(crate) use completion::*;
pub(crate) use display::*;
pub(crate) use input::*;
pub(crate) use kill::*;
pub(crate) use macros::*;
pub(crate) use paste::*;
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
    pub(crate) numeric_arg_sign_only: bool,
    /// Sign retained when the numeric argument is zero or sign-only.
    pub(crate) numeric_arg_negative: bool,
    /// The pending count is an implicit universal-argument multiplier.
    pub(crate) numeric_arg_universal: bool,
    /// An explicit count was followed by a no-op universal-argument command.
    pub(crate) numeric_arg_universal_noop: bool,
    pub(crate) mark: Option<usize>,
    pub(crate) overwrite_mode: bool,
    pub(crate) original_line: Vec<u8>,
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
            numeric_arg_sign_only: false,
            numeric_arg_negative: false,
            numeric_arg_universal: false,
            numeric_arg_universal_noop: false,
            mark: None,
            overwrite_mode: false,
            original_line,
        }
    }

    pub(crate) fn cancel_pending_command(&mut self) {
        self.input.pending_key.clear();
        self.input.prefix_meta_command_len = 0;
        self.input.quoted_insert = false;
        self.input.skipping_csi = false;
        self.input.csi_sequence_started = false;
        self.take_numeric_arg();
        self.search.reverse_search = None;
        self.search.non_incremental_search = None;
        self.search.quoted_pending = false;
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

    /// Drop a pending numeric argument once a complete command has run.
    /// Prefix/pending input (meta/CSI/quote/replace/named/vi-search/mark/
    /// register/operator) keeps the argument for the command it prefixes.
    pub(crate) fn consume_numeric_arg_unless_prefix(&mut self) {
        if self.input.prefix_meta
            || self.input.skipping_csi
            || self.input.quoted_insert
            || self.input.pending_replace
            || self.input.named_command.is_some()
            || self.vi.pending_char_search.is_some()
            || self.vi.pending_vi_mark.is_some()
            || self.vi.pending_vi_register
            || self.vi.vi_operator.is_some()
        {
            return;
        }
        self.take_numeric_arg();
    }

    pub(crate) fn take_numeric_arg(&mut self) -> Option<i32> {
        let arg = self.numeric_arg.take();
        self.numeric_arg_sign_only = false;
        self.numeric_arg_negative = false;
        self.numeric_arg_universal = false;
        self.numeric_arg_universal_noop = false;
        self.macro_state.numeric_arg_recording_start = None;
        arg
    }

    pub(crate) fn after_self_insert(&mut self) {
        self.kill.last_yank = None;
        self.search.reverse_search = None;
        self.undo.last_undo_was_insert = true;
        self.completion.last_completion = None;
        self.completion.last_attempt = None;
        self.completion.menu_completion = None;
    }

    pub(crate) fn region_bounds(&self) -> Option<(usize, usize)> {
        // GNU treats an unset mark as line start for region commands (tested
        // kill-region and copy-region-as-kill with no mark set); a degenerate
        // empty region stays a silent no-op.
        let mark = self.mark.unwrap_or(0);
        let point = self.buffer.point();
        if mark == point {
            return None;
        }
        Some((mark.min(point), mark.max(point)))
    }

    pub(crate) fn after_non_kill_command(&mut self) {
        self.commit_pending_undo();
        self.kill.last_was_kill = false;
        self.kill.last_yank = None;
        self.search.reverse_search = None;
        self.undo.last_undo_was_insert = false;
    }
}
