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

/// Composition inherited from a universal-argument no-op on an explicit count.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericArgComposition {
    None,
    /// The no-op was seen, but no multiplier has followed it.
    Pending,
    /// A multiplier followed the no-op; keep explicit-count composition.
    Chained,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericArgLiteralTail {
    None,
    /// Subsequent bare argument keys insert as text.
    Literal,
}

#[derive(Debug, Clone)]
pub(crate) struct OverwriteStep {
    pub(crate) start: usize,
    pub(crate) replaced: Vec<u8>,
    pub(crate) inserted: Vec<u8>,
}

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
    /// A Meta-prefixed digit has contributed to the pending argument since the last sign.
    pub(crate) numeric_arg_meta_digit: bool,
    /// Sign retained when the numeric argument is zero or sign-only.
    pub(crate) numeric_arg_negative: bool,
    /// The pending count is an implicit universal-argument multiplier.
    pub(crate) numeric_arg_universal: bool,
    pub(crate) numeric_arg_composition: NumericArgComposition,
    pub(crate) numeric_arg_tail: NumericArgLiteralTail,
    pub(crate) mark: Option<usize>,
    pub(crate) overwrite_mode: bool,
    /// Whether typing in overwrite mode is being tracked for backspace.
    pub(crate) overwrite_tracking: bool,
    /// Each typed unit in vi replace mode, for backspace to undo in reverse.
    pub(crate) overwrite_steps: Vec<OverwriteStep>,
    pub(crate) original_line: Vec<u8>,
    /// History entry represented by the buffer; a search overlay can differ from the cursor.
    pub(crate) history_entry_index: Option<usize>,
    /// Whether the buffer has been edited since that history entry was recalled.
    /// Manual erases keep this set even when the bytes match again; undo and
    /// revert-line clear it when they restore the recalled text.
    pub(crate) history_modified: bool,
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
            numeric_arg_meta_digit: false,
            numeric_arg_negative: false,
            numeric_arg_universal: false,
            numeric_arg_composition: NumericArgComposition::None,
            numeric_arg_tail: NumericArgLiteralTail::None,
            mark: None,
            overwrite_mode: false,
            overwrite_tracking: false,
            overwrite_steps: Vec::new(),
            original_line,
            history_entry_index: None,
            history_modified: false,
        }
    }

    /// Begin vi replace mode with an empty backspace history.
    pub(crate) fn begin_vi_overwrite(&mut self) {
        self.overwrite_steps.clear();
        self.overwrite_tracking = true;
        self.overwrite_mode = true;
    }

    /// Leave vi replace mode; a later backspace keeps its insert-mode behavior.
    pub(crate) fn end_vi_overwrite(&mut self) {
        self.overwrite_tracking = false;
        self.overwrite_steps.clear();
        self.overwrite_mode = false;
    }

    /// Remember one typed unit so backspace can restore what it replaced.
    pub(crate) fn record_overwrite_step(
        &mut self,
        start: usize,
        replaced: Vec<u8>,
        inserted: &[u8],
    ) {
        if self.overwrite_tracking {
            self.overwrite_steps.push(OverwriteStep {
                start,
                replaced,
                inserted: inserted.to_vec(),
            });
        }
    }

    /// Undo the last typed unit in vi replace mode.
    pub(crate) fn undo_overwrite_step(&mut self) -> bool {
        let Some(step) = self.overwrite_steps.pop() else {
            return false;
        };
        self.buffer.replace_range_bytes(
            step.start,
            step.start + step.inserted.len(),
            &step.replaced,
        );
        self.buffer.set_point(step.start);
        true
    }

    pub(crate) fn cancel_pending_command(&mut self) {
        self.input.pending_key.clear();
        self.input.pending_char_bytes.clear();
        self.input.prefix_meta_command_len = 0;
        self.input.quoted_insert = false;
        self.input.quoted_insert_remaining = 0;
        self.input.quoted_multibyte_tail = false;
        self.input.quoted_single_byte = false;
        self.input.skipping_csi = false;
        self.input.csi_sequence_started = false;
        self.input.csi_bare_introducer = None;
        self.input.csi_introducer_pending = false;
        self.input.arrow_key_prefix = false;
        self.take_numeric_arg();
        self.search.reverse_search = None;
        self.search.non_incremental_search = None;
        self.search.quoted_pending = false;
        self.input.named_command = None;
        self.input.pending_replace = false;
        self.input.pending_replace_esc = false;
        self.input.pending_replace_esc_run = false;
        self.vi.pending_char_search = None;
        self.vi.pending_char_search_count = None;
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
        self.numeric_arg_meta_digit = false;
        self.numeric_arg_negative = false;
        self.numeric_arg_universal = false;
        self.numeric_arg_composition = NumericArgComposition::None;
        self.numeric_arg_tail = NumericArgLiteralTail::None;
        self.macro_state.numeric_arg_recording_start = None;
        arg
    }

    pub(crate) fn note_history_edit(&mut self) {
        if self.history_entry_index.is_some()
            && self.buffer.as_bytes() != self.original_line.as_slice()
        {
            self.history_modified = true;
        }
    }

    pub(crate) fn after_self_insert(&mut self) {
        // Coalesced typed input can bypass command dispatch.
        self.note_history_edit();
        self.search.history_navigation_needle = None;
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
        self.note_history_edit();
        if self.vi.defer_undo_commit {
            self.vi.defer_undo_commit = false;
        } else {
            self.commit_pending_undo();
        }
        self.kill.last_was_kill = false;
        self.kill.last_yank = None;
        self.kill.last_was_vi_put = false;
        self.search.reverse_search = None;
        self.undo.last_undo_was_insert = false;
    }
}
