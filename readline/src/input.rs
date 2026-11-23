use crate::buffer::LineBuffer;
use crate::editor::{Editor, EditorOutcome, ReadlineError, ReadlineResult};
use crate::hooks::Hooks;
use crate::keymap::{EditCommand, KeyBinding, KeyMapName};
use crate::state::*;
use crate::terminal::{SigintPolicy, TerminalIo};
use crate::variables::BoolVariable;
use history::HistoryDirection;

const BRACKETED_PASTE_START_SEQUENCE: &[u8] = b"\x1b[200~";
const BRACKETED_PASTE_END_SEQUENCE: &[u8] = b"\x1b[201~";
const NUL_FROZEN_KEY_SEQUENCE_MAX: usize = 8;
/// Single-byte commands measured to leave a NUL-frozen search and act on the
/// established match line.
const NUL_FROZEN_LINE_COMMAND_KEYS: &[u8] = &[
    0x01, 0x02, 0x04, 0x05, 0x06, 0x09, 0x0b, 0x0c, 0x0e, 0x10, 0x14, 0x1e, 0x1f,
];
/// Query-editing keys measured to ring once after a successful NUL freeze
/// while the query stays frozen.
const NUL_FROZEN_RING_KEYS: &[u8] = &[0x08, 0x17, 0x18, 0x19, 0x1d, 0x7f];

// GNU Readline normalizes CR while collecting bracketed-paste input.
fn normalize_bracketed_paste_carriage_returns(bytes: &mut [u8]) {
    for byte in bytes {
        if *byte == b'\r' {
            *byte = b'\n';
        }
    }
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn handle_terminal_signal(
        &mut self,
        state: &mut EditorState,
        signal: i32,
    ) -> Result<Option<ReadlineResult>, ReadlineError> {
        #[cfg(unix)]
        if signal == libc::SIGINT {
            self.terminal.restore_mode()?;
            self.terminal.reraise_caught_sigint()?;
            self.handle_input_interrupt(state)?;
            if state.input.interrupted {
                return Ok(Some(ReadlineResult::Interrupted));
            }
            // Keep the current editor state and queued terminal bytes while
            // restoring the editor's terminal modes for the continuing read.
            self.reset_after_signal()?;
            return Ok(None);
        }
        self.terminal.restore_mode()?;
        #[cfg(unix)]
        unsafe {
            libc::raise(signal);
        }
        #[cfg(not(unix))]
        let _ = state;
        #[cfg(not(unix))]
        let _ = signal;
        self.terminal.enter_raw_mode()?;
        Ok(None)
    }

    pub(crate) fn handle_input_interrupt(
        &mut self,
        state: &mut EditorState,
    ) -> Result<(), ReadlineError> {
        self.echo_signal_interrupt(state)?;
        state.input.interrupted = self.sigint_policy == SigintPolicy::Interrupted;
        Ok(())
    }

    pub(super) fn handle_bytes(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        if !self.variables.is_on("sushline-vi-registers") {
            state.vi.pending_vi_register = false;
            state.vi.active_vi_register = None;
        }
        if state.input.prefix_meta {
            return self.handle_meta_prefix(state, bytes, hooks);
        }
        if state.input.skipping_csi {
            return self.handle_csi_skip(state, bytes, hooks);
        }
        if state.input.quoted_insert {
            return self.handle_quoted_insert(state, bytes, hooks);
        }
        // Once the named-command prompt is open, every byte belongs to the
        // query until a query control ends it. Translating high bytes to Meta
        // here would let byte-oriented mode dispatch them as editing commands
        // instead of showing the raw query text GNU shows.
        if state.input.named_command.is_some() {
            return self.handle_named_command(state, bytes, hooks);
        }
        if let Some(translated) = self.translate_meta_input(bytes) {
            return self.handle_bytes(state, &translated, hooks);
        }
        if state.paste.bracketed_paste {
            return self.handle_bracketed_paste_input(state, bytes, hooks);
        }
        if let Some(outcome) = self.handle_pending_vi_mark(state, bytes, hooks)? {
            return Ok(outcome);
        }
        if self.variables.is_on("sushline-vi-registers")
            && let Some(outcome) = self.handle_pending_vi_register(state, bytes, hooks)?
        {
            return Ok(outcome);
        }
        if let Some(outcome) = self.handle_pending_char_search(state, bytes, hooks)? {
            return Ok(outcome);
        }
        if state.input.pending_replace {
            return self.handle_replace_input(state, bytes, hooks);
        }
        if state.input.pending_replace_esc {
            // The retained ESC armed keyseq-timeout. Take it here: the
            // branches below decide afresh whether it prefixes this key,
            // and a timeout has already delivered it to `handle_unbound`.
            state.input.pending_key.clear();
            state.input.pending_replace_esc = false;
            if bytes.first() == Some(&0x1b) {
                // A second ESC starts a redundant bare ESC run. GNU consumes
                // the run before the next key sequence, so this replacement
                // ESC must not become part of it.
                if let Some(outcome) = self.handle_replace_esc_run(state, bytes, hooks)? {
                    return Ok(outcome);
                }
                return Ok(EditorOutcome::Continue);
            }
            state.input.pending_key.push(0x1b);
        } else if state.input.pending_replace_esc_run {
            if bytes.first() == Some(&0x1b) {
                if let Some(outcome) = self.handle_replace_esc_run(state, bytes, hooks)? {
                    return Ok(outcome);
                }
                return Ok(EditorOutcome::Continue);
            }
            state.input.pending_replace_esc_run = false;
            if matches!(bytes.first(), Some(b'[' | b'O')) {
                state.input.pending_key.push(0x1b);
            }
        }
        if let Some(outcome) = self.handle_numeric_argument_continuation(state, bytes, hooks)? {
            return Ok(outcome);
        }
        // Fragmentation-invariant: batched bytes take the same path as split
        // reads (longest-match split with complete-char SelfInsert and
        // incomplete-lead buffering in handle_key_dispatch), so mixed
        // control/multibyte chunks cannot swallow bindings and
        // macro/overwrite/undo stay consistent.
        self.handle_key_dispatch(state, bytes, hooks)
    }

    /// Dispatch the bytes of one terminal event.
    ///
    /// GNU Readline reads the available input as one batch, processes it
    /// character by character, and redisplays once. The editor dispatches
    /// the batch one byte at a time, like GNU's per-character reads, and
    /// reports the bytes that follow an accepted line, so the next read
    /// continues with them the way GNU continues with its input stream.
    ///
    /// The result carries the outcome of the dispatch, the bytes that
    /// follow an accepted line, and whether the batch was drawn.
    pub(super) fn dispatch_input_bytes(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<(EditorOutcome, Vec<u8>, bool), ReadlineError> {
        let mut rest = bytes;
        let mut drawn = false;
        while !rest.is_empty() {
            // The current byte together with everything still queued, for
            // key sequences that must be dispatched as one binding.
            let remaining = &bytes[bytes.len() - rest.len()..];
            let (byte, tail) = rest.split_at(1);
            rest = tail;
            if byte[0] == 0x03 && !state.input.quoted_insert && !state.paste.bracketed_paste {
                self.handle_input_interrupt(state)?;
                if state.input.interrupted {
                    return Ok((EditorOutcome::Continue, rest.to_vec(), drawn));
                }
                self.render(state)?;
                drawn = true;
                continue;
            }
            let paste_active = state.paste.bracketed_paste;
            // A key that is not text interrupts the pending text, so the
            // text is drawn before the key writes anything, like GNU
            // Readline, which draws the line before it rings the bell.
            if state.display.pending_text
                && byte[0].is_ascii_control()
                && !paste_active
                && !state.input.quoted_insert
            {
                self.render(state)?;
                drawn = true;
            }
            state.input.applied_command = false;
            state.input.changed_display = false;
            // A NUL-frozen search ignores further query bytes one at a time,
            // which would leak the tail of an ESC-prefixed binding into the
            // line as literal text. Restore the match line and run the whole
            // bound command instead; lone ESC and other bindings stay on
            // their existing paths.
            if let Some((len, binding)) = self.nul_frozen_bound_command(state, remaining) {
                let search = state
                    .search
                    .reverse_search
                    .take()
                    .expect("NUL-frozen search state");
                self.restore_isearch_line_for_editing(state, &search, true);
                let outcome = self.apply_binding(state, binding, &remaining[..len], hooks)?;
                rest = &remaining[len..];
                if state.input.applied_command || state.input.changed_display {
                    self.render(state)?;
                    drawn = true;
                }
                if !matches!(outcome, EditorOutcome::Continue) {
                    return Ok((outcome, rest.to_vec(), drawn));
                }
                continue;
            }
            // GNU's C-] character search runs on the established match line
            // when a NUL-frozen search ends. The ordinary pending-character
            // path decodes the target, including multibyte and split reads.
            if remaining.first() == Some(&0x1d)
                && state
                    .search
                    .reverse_search
                    .as_ref()
                    .is_some_and(|search| search.nul_pending)
                && let Some(search) = state.search.reverse_search.take()
            {
                self.restore_isearch_line_for_editing(state, &search, true);
                rest = &remaining[1..];
                state.vi.pending_char_search = Some(CharSearchMode::Forward);
                state.vi.pending_char_search_count = Some(1);
                state.vi.pending_char_search_operator = None;
                self.render(state)?;
                drawn = true;
                continue;
            }
            // An ESC that shares an input batch with following bytes starts
            // a key sequence in GNU Readline, so it ends the search as a
            // meta prefix rather than as an isearch terminator. Establish
            // the selected line at the match point and let the ESC continue
            // through ordinary dispatch with the rest of the batch. A lone
            // ESC, including one at the end of a batch, still terminates.
            if !state.search.quoted_pending
                && !rest.is_empty()
                && byte == b"\x1b"
                && self.is_isearch_terminator(byte)
                && state
                    .search
                    .reverse_search
                    .as_ref()
                    .is_some_and(|search| !search.nul_pending)
                && let Some(search) = state.search.reverse_search.take()
            {
                state.input.changed_display = true;
                state.search.quoted_pending = false;
                self.restore_isearch_line_for_editing(state, &search, false);
            }
            let outcome = if state.search.non_incremental_search.is_some() {
                self.handle_non_incremental_search(state, byte, hooks)?
            } else if state.search.reverse_search.is_some() {
                self.handle_reverse_search(state, byte, hooks)?
            } else {
                self.handle_bytes(state, byte, hooks)?
            };
            // GNU Readline defers the redisplay of typed text until the
            // input batch ends and redraws after every other key, so a
            // pasted line is drawn once and a command is drawn at once. A
            // key that changes the display without running a command, like
            // a search query, redraws as well. A finished paste is drawn
            // when its terminator arrives.
            let applied_command = state.input.applied_command;
            let changed_display = state.input.changed_display;
            let ends_the_read = !matches!(outcome, EditorOutcome::Continue);
            if ((applied_command || changed_display) && !ends_the_read)
                || (paste_active && !state.paste.bracketed_paste)
            {
                self.render(state)?;
                drawn = true;
            }
            if !matches!(outcome, EditorOutcome::Continue) {
                return Ok((outcome, rest.to_vec(), drawn));
            }
        }
        Ok((EditorOutcome::Continue, Vec::new(), drawn))
    }

    /// Longest complete control-prefixed binding in `bytes` while an
    /// incremental search is frozen by NUL. GNU runs the bound action after
    /// establishing the match line; only plain self-insert is excluded, so
    /// lone prefixes and incomplete sequences keep their existing paths.
    fn nul_frozen_bound_command(
        &self,
        state: &EditorState,
        bytes: &[u8],
    ) -> Option<(usize, KeyBinding)> {
        if !bytes.first().is_some_and(|byte| byte.is_ascii_control())
            || !state
                .search
                .reverse_search
                .as_ref()
                .is_some_and(|search| search.nul_pending)
        {
            return None;
        }
        let max = bytes.len().min(NUL_FROZEN_KEY_SEQUENCE_MAX);
        (2..=max).rev().find_map(|len| {
            let binding = self.keymap.lookup(self.keymap.current(), &bytes[..len])?;
            if matches!(binding, KeyBinding::Command(EditCommand::SelfInsert)) {
                None
            } else {
                Some((len, binding.clone()))
            }
        })
    }

    /// Dispatch keyboard-macro bytes one at a time.
    ///
    /// GNU Readline keeps macro bytes in the input stream, so a macro that
    /// accepts a line continues in the next prompt. Dispatching one byte at
    /// a time keeps the exact remainder when a macro ends the current read.
    pub(super) fn dispatch_macro_bytes(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<(EditorOutcome, Vec<u8>), ReadlineError> {
        let mut rest = bytes;
        let mut outcome = EditorOutcome::Continue;
        while !rest.is_empty() {
            let (byte, tail) = rest.split_at(1);
            rest = tail;
            let result = self.handle_bytes(state, byte, hooks)?;
            if !matches!(result, EditorOutcome::Continue) {
                outcome = result;
                break;
            }
        }
        Ok((outcome, rest.to_vec()))
    }

    /// Replay keyboard-macro bytes.
    ///
    /// The replay suppresses recording, and a macro that ends the current
    /// read continues in the next prompt, like GNU Readline, which keeps
    /// the macro bytes in the input stream.
    pub(super) fn replay_macro_bytes(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        let previous_replaying = state.macro_state.replaying_macro;
        state.macro_state.replaying_macro = true;
        let result = self.dispatch_macro_bytes(state, bytes, hooks);
        state.macro_state.replaying_macro = previous_replaying;
        let (outcome, remainder) = result?;
        if !matches!(outcome, EditorOutcome::Continue) {
            // The read ended inside the replay. GNU keeps the remaining
            // macro bytes in the input stream and continues them in the
            // next prompt.
            self.pending_macro_replay = remainder;
        }
        Ok(outcome)
    }

    fn handle_meta_prefix(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        state.input.prefix_meta = false;
        if bytes.is_empty() {
            return Ok(EditorOutcome::Continue);
        }

        let (next, rest) = split_first_unit(bytes);
        let mut prefixed = Vec::with_capacity(next.len() + 1);
        prefixed.push(0x1b);
        prefixed.extend_from_slice(next);
        let was_prefix_meta_command_len = state.input.prefix_meta_command_len;
        state.input.prefix_meta_command_len = prefixed.len();
        let result = self.handle_bytes(state, &prefixed, hooks);
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                state.input.prefix_meta_command_len = was_prefix_meta_command_len;
                return Err(error);
            }
        };
        if state.input.pending_key.is_empty() || !matches!(&outcome, EditorOutcome::Continue) {
            state.input.prefix_meta_command_len = was_prefix_meta_command_len;
        }
        if rest.is_empty() || !matches!(&outcome, EditorOutcome::Continue) {
            return Ok(outcome);
        }
        self.handle_bytes(state, rest, hooks)
    }

    fn handle_csi_skip(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        if state.input.arrow_key_prefix {
            state.input.skipping_csi = false;
            state.input.csi_sequence_started = false;
            state.input.arrow_key_prefix = false;
            if bytes.is_empty() {
                return Ok(EditorOutcome::Continue);
            }
            // GNU switches to its arrow-key-prefix keymap: lowercase
            // a/b/c/d mirror A/B/C/D for history and character movement,
            // even when the consumed introducer was a one-byte binding
            // such as `[`.
            let command = match bytes[0] {
                b'A' | b'a' => Some("previous-history"),
                b'B' | b'b' => Some("next-history"),
                b'C' | b'c' => Some("forward-char"),
                b'D' | b'd' => Some("backward-char"),
                _ => None,
            };
            if let Some(command) = command {
                self.apply_named_command(state, command, &bytes[..1], hooks)?;
            } else {
                state.consume_numeric_arg_unless_prefix();
                self.ding()?;
            }
            return self.handle_bytes(state, &bytes[1..], hooks);
        }
        let consumed = state.consume_csi_bytes(bytes);
        if consumed < bytes.len() {
            return self.handle_bytes(state, &bytes[consumed..], hooks);
        }
        Ok(EditorOutcome::Continue)
    }

    /// Number of continuation bytes in a UTF-8 sequence led by `byte`.
    fn utf8_continuation_count(byte: u8) -> u32 {
        match byte {
            0xc0..=0xdf => 1,
            0xe0..=0xef => 2,
            0xf0..=0xf7 => 3,
            _ => 0,
        }
    }

    fn handle_quoted_insert(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        let remaining = state.input.quoted_insert_remaining;
        if remaining == 0 {
            state.input.quoted_insert = false;
            state.input.quoted_single_byte = true;
            self.insert_literal(state, bytes, true);
            state.input.quoted_single_byte = false;
            // GNU quotes the lead byte of a multibyte key as a normal literal,
            // then applies bytewise overwrite to the continuation bytes that
            // follow it. Arm that tail only for the vi replace case.
            if state.overwrite_mode && self.char_motion_is_bytewise() && bytes.len() == 1 {
                let tail = Self::utf8_continuation_count(bytes[0]);
                if tail > 0 {
                    state.input.quoted_insert = true;
                    state.input.quoted_insert_remaining = tail;
                    state.input.quoted_multibyte_tail = true;
                }
            }
            return Ok(EditorOutcome::Continue);
        }
        if state.input.quoted_multibyte_tail
            && !bytes
                .first()
                .is_some_and(|byte| (0x80..=0xbf).contains(byte))
        {
            // A raw high byte not followed by a UTF-8 continuation was a
            // literal quoted byte, not a multibyte key; dispatch the rest
            // normally instead of consuming it as a character tail.
            state.input.quoted_insert = false;
            state.input.quoted_insert_remaining = 0;
            state.input.quoted_multibyte_tail = false;
            return self.handle_bytes(state, bytes, hooks);
        }
        let take = remaining.min(bytes.len() as u32) as usize;
        for byte in &bytes[..take] {
            self.insert_literal_with_count(state, &[*byte], 1, true);
            state.input.quoted_insert_remaining -= 1;
        }
        if state.input.quoted_insert_remaining == 0 {
            state.input.quoted_insert = false;
            state.input.quoted_multibyte_tail = false;
        }
        if take < bytes.len() {
            return self.handle_bytes(state, &bytes[take..], hooks);
        }
        Ok(EditorOutcome::Continue)
    }

    fn handle_bracketed_paste_input(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        let mut combined = std::mem::take(&mut state.paste.bracketed_paste_pending);
        let pending_len = combined.len();
        combined.extend_from_slice(bytes);
        let start = *state
            .paste
            .bracketed_paste_start
            .get_or_insert_with(|| state.buffer.point());
        let Some(end_pos) = combined
            .windows(BRACKETED_PASTE_END_SEQUENCE.len())
            .position(|window| window == BRACKETED_PASTE_END_SEQUENCE)
        else {
            // Do not expose or edit a partial paste before its complete end marker.
            normalize_bracketed_paste_carriage_returns(&mut combined[pending_len..]);
            state.paste.bracketed_paste_pending = combined;
            return Ok(EditorOutcome::Continue);
        };
        let mut payload = combined[..end_pos].to_vec();
        normalize_bracketed_paste_carriage_returns(&mut payload);
        // Bytes after the end marker are ordinary input, not paste payload.
        let remainder = combined[end_pos + BRACKETED_PASTE_END_SEQUENCE.len()..].to_vec();
        if !payload.is_empty() {
            self.insert_literal(state, &payload, false);
        }
        state.mark = Some(start);
        state.paste.bracketed_paste = false;
        state.paste.bracketed_paste_start = None;
        state.paste.bracketed_paste_pending.clear();
        if !remainder.is_empty() {
            return self.handle_bytes(state, &remainder, hooks);
        }
        Ok(EditorOutcome::Continue)
    }

    pub(super) fn finish_bracketed_paste(&mut self, state: &mut EditorState) {
        let start = state
            .paste
            .bracketed_paste_start
            .take()
            .unwrap_or_else(|| state.buffer.point());
        let pending = std::mem::take(&mut state.paste.bracketed_paste_pending);
        state.paste.bracketed_paste = false;
        if !pending.is_empty() {
            self.insert_literal(state, &pending, false);
        }
        state.mark = Some(start);
    }

    /// Consume a bare ESC run that followed an ignored replacement ESC.
    /// Returns `None` while the run is still open and `Some` once the rest of
    /// the event has been dispatched with a fresh sequence prefix.
    fn handle_replace_esc_run(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<Option<EditorOutcome>, ReadlineError> {
        let esc_run = bytes.iter().take_while(|byte| **byte == 0x1b).count();
        if esc_run == bytes.len() {
            state.input.pending_replace_esc_run = true;
            return Ok(None);
        }
        state.input.pending_replace_esc_run = false;
        let rest = &bytes[esc_run..];
        if matches!(rest.first(), Some(b'[' | b'O')) {
            state.input.pending_key.push(0x1b);
        }
        self.handle_bytes(state, rest, hooks).map(Some)
    }

    fn handle_replace_input(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        // GNU reads a whole character as the replacement key in a UTF-8
        // locale, but only one byte when byte motion is in effect. Hold an
        // incomplete character across split reads in the character path, or
        // a lead byte would be committed and its continuations ring as keys.
        let bytewise = self.char_motion_is_bytewise();
        let mut buffered = std::mem::take(&mut state.input.pending_char_bytes);
        buffered.extend_from_slice(bytes);
        if !bytewise && is_incomplete_utf8_prefix(&buffered) {
            state.input.pending_char_bytes = buffered;
            return Ok(EditorOutcome::Continue);
        }
        let bytes = buffered.as_slice();
        // Fragmentation-invariant: only the first unit is the replacement;
        // trailing bytes are separate input like split reads.
        if bytes.is_empty() {
            state.input.pending_replace = false;
            state.consume_numeric_arg_unless_prefix();
            return Ok(EditorOutcome::Continue);
        }
        let (first, rest) = if bytewise {
            split_first_byte(bytes)
        } else {
            split_first_unit(bytes)
        };
        let count = repeat_count(state.take_numeric_arg());
        state.input.pending_replace = false;
        let replacement = replacement_unit(first);
        if replacement.is_empty() && first == [0x1b] {
            // An ESC used as the replacement is ignored, but GNU still lets
            // it introduce the next key sequence, so a following arrow moves
            // the cursor. Decide on the next input event: an ESC there starts
            // a redundant ESC run and must not inherit this prefix, while any
            // other byte needs it to complete its sequence.
            state.input.pending_replace_esc = true;
            // Keep the ESC in the ordinary pending-key buffer so the
            // editor's keyseq-timeout expires this prefix; the flag above
            // only records how the next key should be interpreted.
            state.input.pending_key = first.to_vec();
        }
        if !replacement.is_empty() {
            // Vi command-mode `$` leaves the byte point just past the last
            // character; GNU replaces the last character under the cursor.
            if state.buffer.point() == state.buffer.len_chars() && !state.buffer.is_empty() {
                state.buffer.move_backward();
            }
            state.record_undo();
            let mut first_undo_point = None;
            let mut last_replaced = None;
            let mut last_displaced = None;
            for _ in 0..count {
                let start = state.buffer.point();
                if first_undo_point.is_none() {
                    first_undo_point = Some(if bytewise {
                        start + 1
                    } else {
                        state.buffer.next_boundary(start).unwrap_or(start + 1)
                    });
                }
                // In bytewise mode `r` overwrites one destination byte, not
                // the whole byte sequence that happens to contain point.
                let replaced = if bytewise {
                    if start < state.buffer.len_chars() {
                        last_displaced = Some(state.buffer.range_bytes(start, start + 1));
                        state
                            .buffer
                            .replace_range_bytes(start, start + 1, &replacement);
                        true
                    } else {
                        false
                    }
                } else {
                    let displaced = state
                        .buffer
                        .range_bytes(start, state.buffer.next_boundary(start).unwrap_or(start));
                    if state.buffer.replace_char_at_point_bytes(&replacement) {
                        last_displaced = Some(displaced);
                        true
                    } else {
                        false
                    }
                };
                if !replaced {
                    break;
                }
                last_replaced = Some(start);
            }
            state.undo.pending_replace_undo_point = first_undo_point;
            // GNU keeps an undo entry for an identical replacement, so the
            // next `u` consumes it silently instead of undoing an older edit.
            match last_replaced {
                Some(start) => {
                    state.buffer.set_point(start);
                    state.finish_noop_replace_undo();
                }
                None => {
                    state.record_noop_undo();
                    self.ding()?;
                }
            }
            // GNU keeps only the last displaced unit in the vi register;
            // a counted replace therefore makes that final unit pasteable.
            if let Some(displaced) = last_displaced {
                state.push_kill(displaced, KillDirection::Forward, false);
            }
            if let Some(mut change) = state.vi.vi_insert_change.take() {
                change.extend_from_slice(first);
                state.set_vi_redo(change);
            }
            state.after_non_kill_command();
            state.input.changed_display = true;
        }
        if rest.is_empty() {
            return Ok(EditorOutcome::Continue);
        }
        self.handle_bytes(state, rest, hooks)
    }

    fn insert_literal(&mut self, state: &mut EditorState, bytes: &[u8], record_macro: bool) {
        let count = repeat_count(state.take_numeric_arg());
        self.insert_literal_with_count(state, bytes, count, record_macro);
    }

    fn insert_literal_with_count(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        count: u32,
        record_macro: bool,
    ) {
        state.display.pending_text = true;
        state.record_insert_undo(
            count == 1 && bytes.len() == 1,
            self.keymap.current() == KeyMapName::ViInsert,
        );
        for _ in 0..count {
            let start = state.buffer.byte_point();
            // A quoted single byte is inserted, never overwritten, in vi
            // replace mode; backspace rings at the replace origin or removes
            // the trailing insertion after printable replace steps.
            if state.overwrite_mode && state.input.quoted_single_byte {
                state.buffer.insert_bytes(bytes);
                state.record_inserted_overwrite_bytes(start, bytes);
                continue;
            }
            // In bytewise overwrite mode unquoted literal bytes and the tail
            // of a quoted multibyte key overwrite raw bytes one at a time,
            // while a single quoted byte is inserted. Non-bytewise modes keep
            // whole-character overwrite.
            if state.overwrite_mode && self.char_motion_is_bytewise() {
                if state.input.quoted_multibyte_tail
                    || (!state.input.quoted_single_byte && bytes.iter().any(|byte| *byte >= 0x80))
                {
                    let end = (start + bytes.len()).min(state.buffer.as_bytes().len());
                    let replaced = state.buffer.as_bytes()[start..end].to_vec();
                    self.overwrite_bytes_bytewise(state, bytes);
                    state.record_overwrite_step(start, replaced, bytes);
                } else {
                    state.buffer.insert_bytes(bytes);
                    state.record_overwrite_step(start, Vec::new(), bytes);
                }
            } else if state.overwrite_mode {
                for byte in bytes {
                    let point = state.buffer.byte_point();
                    let end = state
                        .buffer
                        .next_boundary(point)
                        .unwrap_or(state.buffer.as_bytes().len());
                    let replaced = state.buffer.as_bytes()[point..end].to_vec();
                    if point < state.buffer.len_chars() {
                        let _ = state.buffer.delete_char_bytes();
                    }
                    state.buffer.replace_range_bytes(point, point, &[*byte]);
                    state.record_overwrite_step(point, replaced, &[*byte]);
                }
            } else {
                state.buffer.insert_bytes(bytes);
                state.record_overwrite_step(start, Vec::new(), bytes);
            }
        }
        if state.overwrite_mode
            && self.char_motion_is_bytewise()
            && (state.input.quoted_multibyte_tail
                || (!state.input.quoted_single_byte && bytes.iter().any(|byte| *byte >= 0x80)))
        {
            state.mark_noop_undo_if_unchanged();
        }
        if record_macro {
            state.record_macro_insert_bytes(bytes);
        }
        state.record_vi_insert_bytes(bytes);
        state.after_self_insert();
    }

    fn positive_zero_digit_is_literal(state: &EditorState, byte: u8) -> bool {
        state.numeric_arg == Some(0)
            && state.numeric_arg_sign_only
            && !state.numeric_arg_negative
            && byte.is_ascii_digit()
    }

    /// A bare argument key becomes literal text once a positive count is
    /// established: a minus after any positive count, or a digit after a
    /// positive sign-only count. Negative sign-only counts still compose bare
    /// digits, and universal arguments start fresh counts.
    pub(crate) fn bare_argument_key_is_literal(&self, state: &EditorState, byte: u8) -> bool {
        if state.numeric_arg_tail != NumericArgLiteralTail::None
            || state.numeric_arg_composition == NumericArgComposition::Pending
            || state.numeric_arg_universal
        {
            return false;
        }
        let Some(count) = state.numeric_arg else {
            return false;
        };
        (count > 0
            && (byte == b'-' || (state.numeric_arg_sign_only && byte.is_ascii_digit())))
            // A minus at a zero count starts a literal tail with zero repeats;
            // following argument keys then insert once.
            || (count == 0 && byte == b'-')
            || Self::positive_zero_digit_is_literal(state, byte)
    }

    /// GNU consumes a digit or minus after a universal-argument no-op as
    /// literal text. The first key repeats by the pending positive count (a
    /// Meta-prefixed key inserts once); later bare argument keys in the tail
    /// insert once without another bell, while Meta-prefixed argument keys
    /// start a fresh signed count.
    pub(crate) fn handle_noop_argument_key(
        &mut self,
        state: &mut EditorState,
        key: u8,
        meta_prefixed: bool,
        record_macro: bool,
    ) -> Result<(), ReadlineError> {
        if state.numeric_arg_tail != NumericArgLiteralTail::None {
            self.insert_literal_with_count(state, &[key], 1, record_macro);
            return Ok(());
        }
        let count = state.numeric_arg.unwrap_or(0);
        // A positive sign-only zero still inserts one bare argument key; a
        // plain zero repeats keys zero times, as GNU does.
        let repeats = if Self::positive_zero_digit_is_literal(state, key) {
            1
        } else {
            count.max(0) as u32
        };
        // A minus after a multiplied positive no-op chain repeats by that
        // count even when Meta-prefixed, with no bell; other Meta-prefixed
        // no-op argument keys insert once and ring twice.
        let repeat_by_count =
            state.numeric_arg_composition == NumericArgComposition::Chained && count > 0;
        state.take_numeric_arg();
        if meta_prefixed && !repeat_by_count {
            self.insert_literal_with_count(state, &[key], 1, record_macro);
            self.ding()?;
            self.ding()?;
            state.numeric_arg = Some(0);
        } else {
            self.insert_literal_with_count(state, &[key], repeats, record_macro);
            state.numeric_arg = None;
        }
        state.numeric_arg_tail = NumericArgLiteralTail::Literal;
        Ok(())
    }

    fn handle_numeric_argument_continuation(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<Option<EditorOutcome>, ReadlineError> {
        if (state.numeric_arg.is_none() && state.numeric_arg_tail == NumericArgLiteralTail::None)
            || !state.input.pending_key.is_empty()
        {
            return Ok(None);
        }
        // A count before `c0` does not turn the zero motion into another
        // count digit: GNU still runs `c0` and leaves the count on the change.
        if bytes.first() == Some(&b'0')
            && state.vi.vi_operator.is_some()
            && state
                .vi
                .vi_operator_key
                .as_deref()
                .is_some_and(|key| key.last() == Some(&b'c'))
        {
            return Ok(None);
        }
        // Fragmentation-invariant: a batched chunk may carry a digit run
        // plus the terminating sequence (e.g. `1\x1bl` after `M--`).
        // Consume the leading run digit-by-digit like the one-byte path,
        // then dispatch any remainder so batched and split reads agree.
        // A buffered keymap prefix (e.g. a lone ESC awaiting its Meta
        // partner after one-byte reads) owns the incoming bytes: stripping
        // digits here would orphan the prefix and corrupt dispatch, so the
        // continuation only runs with no prefix pending.
        let run = bytes
            .iter()
            .take_while(|byte| matches!(byte, b'0'..=b'9' | b'-'))
            .count();
        if run == 0 {
            return Ok(None);
        }
        let positive_chain_minus = state.numeric_arg_composition == NumericArgComposition::Chained
            && state.numeric_arg.is_some_and(|count| count > 0)
            && bytes[0] == b'-';
        if state.numeric_arg_tail != NumericArgLiteralTail::None
            || state.numeric_arg_composition == NumericArgComposition::Pending
            || positive_chain_minus
        {
            for byte in &bytes[..run] {
                self.handle_noop_argument_key(state, *byte, false, true)?;
            }
            if run < bytes.len() {
                let remainder = bytes[run..].to_vec();
                let outcome = self.handle_bytes(state, &remainder, hooks)?;
                return Ok(Some(outcome));
            }
            return Ok(Some(EditorOutcome::Continue));
        }
        let mut overflowed = false;
        for byte in &bytes[..run] {
            if state.numeric_arg_tail != NumericArgLiteralTail::None
                || self.bare_argument_key_is_literal(state, *byte)
            {
                self.handle_noop_argument_key(state, *byte, false, true)?;
            } else if update_numeric_argument(state, &[*byte]) {
                overflowed = true;
            } else if state.vi.vi_operator.is_some() {
                if let Some(change) = state.vi.vi_operator_key.as_mut() {
                    change.push(*byte);
                }
            } else {
                // Continuation digits are part of the same vi count and must
                // be saved for dot-replay, just like the first digit.
                state.vi.vi_count_keys.push(*byte);
            }
        }
        if overflowed {
            self.ding()?;
        }
        if run < bytes.len() {
            let remainder = bytes[run..].to_vec();
            let outcome = self.handle_bytes(state, &remainder, hooks)?;
            return Ok(Some(outcome));
        }
        Ok(Some(EditorOutcome::Continue))
    }

    fn handle_key_dispatch(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        state.input.pending_key.extend_from_slice(bytes);
        let pending = std::mem::take(&mut state.input.pending_key);
        // GNU `rl_insert_text` groups only single-byte inserts: buffer an
        // incomplete UTF-8 lead across events (like a keymap prefix) while it
        // takes the default SelfInsert path, so torn reads assemble into one
        // insert before undo grouping; complete bindings dispatch below.
        if is_incomplete_utf8_prefix(&pending)
            && matches!(
                self.keymap.lookup(self.keymap.current(), &pending[..1]),
                Some(KeyBinding::Command(EditCommand::SelfInsert))
            )
        {
            state.input.pending_key = pending;
            return Ok(EditorOutcome::Continue);
        }

        // In vi replace mode GNU dispatches ESC as vi-movement-mode before a
        // CSI/SS3 arrow can match, so the remaining bytes become command-mode
        // keys. A bracketed-paste start still owns its longer binding: wait
        // while the pending bytes can become that sequence, then split an
        // ESC-prefixed key that cannot.
        if state.overwrite_tracking
            && pending.len() > 1
            && pending[0] == 0x1b
            && !BRACKETED_PASTE_START_SEQUENCE.starts_with(&pending)
            && let Some(binding) = self.keymap.lookup(self.keymap.current(), &[0x1b]).cloned()
        {
            state.input.prefix_meta_command_len = 0;
            let outcome = self.apply_binding(state, binding, &[0x1b], hooks)?;
            if !matches!(outcome, EditorOutcome::Continue) {
                return Ok(outcome);
            }
            return self.handle_bytes(state, &pending[1..], hooks);
        }
        // A binding on a prefix is the fallback, not a replacement for its
        // submap. In particular, a direct ESC/C-x binding must not disconnect
        // explicit emacs-meta/emacs-ctlx bindings.
        if self.keymap.has_prefix(self.keymap.current(), &pending) {
            state.input.pending_key = pending;
            return Ok(EditorOutcome::Continue);
        }

        if let Some(binding) = self.keymap.lookup(self.keymap.current(), &pending).cloned() {
            state.input.prefix_meta_command_len = 0;
            return self.apply_binding(state, binding, &pending, hooks);
        }

        if let Some((len, binding)) = self
            .keymap
            .longest_matching_prefix(self.keymap.current(), &pending)
            .map(|(len, binding)| (len, binding.clone()))
        {
            // Insert a complete multibyte char as one SelfInsert so it opens
            // its own undo entry like GNU; single bytes keep per-byte dispatch.
            let mut use_len = len;
            if matches!(binding, KeyBinding::Command(EditCommand::SelfInsert))
                && len == 1
                && pending[0] >= 0x80
            {
                let char_len = split_first_unit(&pending).0.len();
                if char_len > 1 && std::str::from_utf8(&pending[..char_len]).is_ok() {
                    use_len = char_len;
                }
            }
            state.input.prefix_meta_command_len = 0;
            let outcome = self.apply_binding(state, binding, &pending[..use_len], hooks)?;
            if !matches!(outcome, EditorOutcome::Continue) {
                return Ok(outcome);
            }
            if use_len < pending.len() {
                return self.handle_bytes(state, &pending[use_len..], hooks);
            }
            return Ok(EditorOutcome::Continue);
        }

        // A bracketed-paste end marker outside a paste is not payload. GNU
        // rings once, then processes the marker's final `~` as ordinary input.
        // Buffer its otherwise-unbound prefixes so split reads behave alike;
        // normal key bindings and prefixes take precedence above.
        if pending.len() > 1
            && pending.len() < BRACKETED_PASTE_END_SEQUENCE.len()
            && BRACKETED_PASTE_END_SEQUENCE.starts_with(&pending)
        {
            state.input.pending_key = pending;
            return Ok(EditorOutcome::Continue);
        }
        if let Some(remainder) = pending.strip_prefix(BRACKETED_PASTE_END_SEQUENCE) {
            state.input.prefix_meta_command_len = 0;
            self.ding()?;
            let outcome = self.handle_bytes(state, b"~", hooks)?;
            if !matches!(outcome, EditorOutcome::Continue) {
                return Ok(outcome);
            }
            if !remainder.is_empty() {
                return self.handle_bytes(state, remainder, hooks);
            }
            return Ok(EditorOutcome::Continue);
        }

        self.handle_unbound(state, &pending, hooks)
    }

    /// A zero-width vi change is replayed without its trailing ESC: in vi
    /// command mode the ESC backs point onto the inserted text, but GNU
    /// leaves the redo point after it, so a later dot edits the next span.
    fn vi_change_replay_omits_escape(bytes: &[u8]) -> bool {
        let Some(body) = bytes.strip_suffix(b"\x1b") else {
            return false;
        };
        let motion = body
            .iter()
            .position(|byte| !byte.is_ascii_digit())
            .unwrap_or(body.len());
        body.get(motion) == Some(&b'c')
            && matches!(body.get(motion + 1), Some(b'0' | b'h' | b'b' | b'B' | b'^'))
    }

    pub(super) fn replay_vi_change(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        let original_change = bytes.to_vec();
        let omit_escape = Self::vi_change_replay_omits_escape(bytes);
        if omit_escape {
            state.vi.zero_width_redo_replays += 1;
            state.vi.replaying_vi_change = true;
        }
        let replay = if omit_escape {
            &bytes[..bytes.len() - 1]
        } else {
            bytes
        };
        if let Ok(text) = std::str::from_utf8(replay) {
            for ch in text.chars() {
                let mut buf = [0_u8; 4];
                let outcome =
                    self.handle_bytes(state, ch.encode_utf8(&mut buf).as_bytes(), hooks)?;
                if !matches!(outcome, EditorOutcome::Continue) {
                    state.vi.replaying_vi_change = omit_escape;
                    state.set_vi_redo(original_change);
                    return Ok(outcome);
                }
            }
        } else {
            for byte in replay {
                let outcome = self.handle_bytes(state, &[*byte], hooks)?;
                if !matches!(outcome, EditorOutcome::Continue) {
                    state.vi.replaying_vi_change = omit_escape;
                    state.set_vi_redo(original_change);
                    return Ok(outcome);
                }
            }
        }
        if omit_escape {
            state.finish_vi_insert_change(&[]);
            self.keymap.set_current(KeyMapName::ViCommand);
            state.after_non_kill_command();
            if state.vi.zero_width_redo_replays > 1 {
                state.record_noop_undo();
            }
        }
        state.set_vi_redo(original_change);
        if omit_escape {
            state.vi.replaying_vi_change = false;
        }
        Ok(EditorOutcome::Continue)
    }

    pub(super) fn translate_meta_input(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        if !self.flag(BoolVariable::ConvertMeta) || !bytes.iter().any(|byte| byte & 0x80 != 0) {
            return None;
        }

        let mut translated = Vec::with_capacity(bytes.len());
        for byte in bytes {
            if byte & 0x80 != 0 {
                translated.extend_from_slice(&[0x1b, byte & 0x7f]);
            } else {
                translated.push(*byte);
            }
        }
        Some(translated)
    }

    pub(super) fn handle_pending_vi_mark(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<Option<EditorOutcome>, ReadlineError> {
        let Some(action) = state.vi.pending_vi_mark.take() else {
            return Ok(None);
        };

        // GNU Readline's vi mark commands read one byte, so a multibyte
        // name is not a name: the byte rings and the rest of the character
        // arrives as ordinary input.
        // Fragmentation-invariant: only the first byte answers the pending
        // mark; trailing bytes are separate input like split reads.
        let (first, rest) = split_first_byte(bytes);
        let outcome = self.handle_pending_vi_mark_single(state, action, first)?;
        if !rest.is_empty() {
            let remainder_outcome = self.handle_bytes(state, rest, hooks)?;
            return Ok(Some(remainder_outcome));
        }
        Ok(Some(outcome))
    }

    fn handle_pending_vi_mark_single(
        &mut self,
        state: &mut EditorState,
        action: ViMarkAction,
        bytes: &[u8],
    ) -> Result<EditorOutcome, ReadlineError> {
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.ding()?;
            return Ok(EditorOutcome::Continue);
        };

        // GNU Readline names a vi mark by a lowercase ASCII letter, and
        // rings for any other name.
        if let Some(ch) = text.chars().find(|ch| ch.is_ascii_lowercase()) {
            match action {
                ViMarkAction::Set => {
                    state.vi.vi_marks.insert(ch, state.buffer.point());
                }
                ViMarkAction::Goto => {
                    let op_start = state.vi.pending_mark_operator.take();
                    if let Some(point) = state.vi.vi_marks.get(&ch).copied() {
                        state.buffer.set_point(point);
                        self.finish_vi_motion_operator(state, op_start, bytes, false, None);
                        state.input.changed_display = true;
                    } else {
                        state.cancel_pending_command();
                        self.ding()?;
                    }
                }
            }
            state.after_non_kill_command();
            state.consume_numeric_arg_unless_prefix();
        } else {
            self.ding()?;
            state.consume_numeric_arg_unless_prefix();
        }

        Ok(EditorOutcome::Continue)
    }

    pub(super) fn handle_pending_vi_register(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<Option<EditorOutcome>, ReadlineError> {
        if !state.vi.pending_vi_register {
            return Ok(None);
        }
        state.vi.pending_vi_register = false;

        // The register is named by one byte, like the vi mark commands:
        // a multibyte name is not a name and the rest of the character
        // arrives as ordinary input.
        // Fragmentation-invariant: only the first byte selects the register;
        // trailing bytes are separate input like split reads.
        let (first, rest) = split_first_byte(bytes);
        let outcome = self.handle_pending_vi_register_single(state, first)?;
        if !rest.is_empty() {
            let remainder_outcome = self.handle_bytes(state, rest, hooks)?;
            return Ok(Some(remainder_outcome));
        }
        Ok(Some(outcome))
    }

    fn handle_pending_vi_register_single(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
    ) -> Result<EditorOutcome, ReadlineError> {
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.ding()?;
            state.consume_numeric_arg_unless_prefix();
            return Ok(EditorOutcome::Continue);
        };

        if let Some(ch) = text.chars().find(|ch| !ch.is_control()) {
            state.vi.active_vi_register = Some(ch);
            state.after_non_kill_command();
            state.consume_numeric_arg_unless_prefix();
        } else {
            self.ding()?;
            state.consume_numeric_arg_unless_prefix();
        }

        Ok(EditorOutcome::Continue)
    }

    pub(super) fn handle_pending_char_search(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<Option<EditorOutcome>, ReadlineError> {
        if state.vi.pending_char_search.is_none() {
            return Ok(None);
        }
        // A byte-oriented search takes one raw byte; otherwise the bytes of
        // an incomplete character wait for the rest of it and the search
        // stays pending.
        let bytewise = self.variables.is_on("byte-oriented");
        let Some((first, rest)) = take_pending_character(state, bytes, bytewise) else {
            return Ok(Some(EditorOutcome::Continue));
        };
        let Some(search) = state.vi.pending_char_search.take() else {
            return Ok(None);
        };
        let count = state.vi.pending_char_search_count.take().unwrap_or(1);
        let search = if count < 0 { search.reversed() } else { search };
        let op_start = state.vi.pending_char_search_operator.take();

        // Fragmentation-invariant: only the first character is the search
        // key; trailing bytes are separate input like split reads.
        if let Some(ch) = char_search_key(&first) {
            let success = self.apply_char_search(
                state,
                search,
                ch,
                count.unsigned_abs() as usize,
                bytewise,
                crate::command::CharSearchBehavior {
                    repeat: false,
                    keep_partial: op_start.is_some(),
                },
            )?;
            // GNU remembers the target even when the first search fails:
            // `,` can then repeat it in the opposite direction.
            state.vi.last_char_search = Some((search, ch));
            if success {
                let widen = matches!(
                    search,
                    crate::state::CharSearchMode::Forward | crate::state::CharSearchMode::Backward
                );
                self.finish_vi_motion_operator(state, op_start, &first, true, widen.then_some(ch));
                state.after_non_kill_command();
                state.consume_numeric_arg_unless_prefix();
                state.input.changed_display = true;
            } else {
                // GNU rings again when a pending operator loses its motion.
                if op_start.is_some() {
                    self.ding()?;
                }
                state.cancel_pending_command();
            }
        } else {
            state.cancel_pending_command();
            if op_start.is_some() {
                self.ding()?;
            }
            self.ding()?;
            if first == [0x1b] {
                // The failed search consumed ESC, but GNU still lets that ESC
                // start a fresh key sequence: a following CSI/SS3 arrow moves
                // the cursor. Re-inject it as a keymap prefix.
                self.handle_key_dispatch(state, &first, hooks)?;
            }
        }

        if !rest.is_empty() {
            let remainder_outcome = self.handle_bytes(state, &rest, hooks)?;
            return Ok(Some(remainder_outcome));
        }
        Ok(Some(EditorOutcome::Continue))
    }

    pub(super) fn handle_unbound(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        let prefix_meta_command_len = state.input.prefix_meta_command_len;
        state.input.prefix_meta_command_len = 0;
        if state.input.pending_replace {
            return self.handle_replace_input(state, bytes, hooks);
        }

        if matches!(self.keymap.current(), KeyMapName::ViCommand) {
            // GNU cancels a pending vi operator silently when the next key is
            // unbound; an ordinary unbound key would ring here.
            let pending_operator = state.vi.vi_operator.is_some();
            state.cancel_pending_command();
            if pending_operator {
                state.mark_vi_redo_cancelled_operator();
            }
            // Redundant bare ESCs are silent no-ops in vi command mode:
            // Bash accepts after them without ringing. Drop the whole
            // leading ESC run, then keep processing what follows so that
            // batched reads cannot swallow trailing input.
            let esc_run = bytes.iter().take_while(|byte| **byte == 0x1b).count();
            if esc_run > 0 {
                let rest = &bytes[esc_run..];
                if rest.is_empty() {
                    return Ok(EditorOutcome::Continue);
                }
                return self.handle_bytes(state, rest, hooks);
            }
            if !pending_operator {
                self.ding()?;
            }
            state.consume_numeric_arg_unless_prefix();
            let (_, rest) = split_first_unit(bytes);
            if rest.is_empty() {
                return Ok(EditorOutcome::Continue);
            }
            return self.handle_bytes(state, rest, hooks);
        }

        // Unknown Meta-style ESC keys ring; CSI and SS3 keep their literal fallback.
        let unbound_meta_prefix = bytes.len() > 1
            && bytes.first() == Some(&0x1b)
            && !matches!(bytes.get(1), Some(&b'[') | Some(&b'O'));
        if prefix_meta_command_len > 0 && bytes.len() > prefix_meta_command_len {
            // Only the generated prefix gets literal fallback; redispatch later bytes normally.
            state.input.prefix_meta_command_len = prefix_meta_command_len;
            let outcome = self.handle_unbound(state, &bytes[..prefix_meta_command_len], hooks)?;
            if !matches!(&outcome, EditorOutcome::Continue) {
                return Ok(outcome);
            }
            return self.handle_bytes(state, &bytes[prefix_meta_command_len..], hooks);
        }

        let unbound_ctlx_prefix = prefix_meta_command_len == 0
            && bytes.len() > 1
            && bytes.first() == Some(&0x18)
            && self.keymap.has_prefix(self.keymap.current(), &bytes[..1]);
        if (unbound_meta_prefix && prefix_meta_command_len == 0) || unbound_ctlx_prefix {
            // An unbound ESC sequence or leaf under an active C-x prefix rings
            // once, consumes the prefix and first complete input unit, then
            // processes any trailing bytes normally.
            self.ding()?;
            state.consume_numeric_arg_unless_prefix();
            let (_, rest) = split_first_unit(&bytes[1..]);
            if rest.is_empty() {
                return Ok(EditorOutcome::Continue);
            }
            return self.handle_bytes(state, rest, hooks);
        }

        // Unknown CSI/SS3 keys under a live key-sequence prefix take the same
        // keymap-miss path as other unbound sequences: ring once, consume the
        // live prefix plus the byte that failed lookup, then process the rest.
        let unbound_csi_prefix = bytes.len() > 2
            && bytes.first() == Some(&0x1b)
            && matches!(bytes.get(1), Some(&b'[') | Some(&b'O'))
            && self.keymap.has_prefix(self.keymap.current(), &bytes[..2]);
        if unbound_csi_prefix {
            let mut prefix_len = 2;
            while prefix_len < bytes.len()
                && self
                    .keymap
                    .has_prefix(self.keymap.current(), &bytes[..=prefix_len])
            {
                prefix_len += 1;
            }
            let consumed = (prefix_len + 1).min(bytes.len());
            self.ding()?;
            state.consume_numeric_arg_unless_prefix();
            return self.handle_bytes(state, &bytes[consumed..], hooks);
        }

        if let Ok(text) = std::str::from_utf8(bytes) {
            let mut inserted = false;
            let bytes = text
                .chars()
                .filter(|ch| !ch.is_control())
                .flat_map(|ch| {
                    let mut buf = [0; 4];
                    ch.encode_utf8(&mut buf).as_bytes().to_vec()
                })
                .collect::<Vec<_>>();
            if !bytes.is_empty() {
                self.insert_literal(state, &bytes, true);
                inserted = true;
            }
            if inserted {
                return Ok(EditorOutcome::Continue);
            }
        } else if bytes.iter().any(|byte| !byte.is_ascii_control()) {
            // Fragmentation-invariant: preserve ASCII non-controls in invalid
            // chunks, matching split reads that see each byte separately.
            let insertable = bytes
                .iter()
                .copied()
                .filter(|byte| !byte.is_ascii_control())
                .collect::<Vec<_>>();
            if !insertable.is_empty() {
                self.insert_literal(state, &insertable, true);
            }
        }
        // Unbound input with nothing insertable still ends the argument:
        // insert_literal already consumed it on the insert paths above.
        state.consume_numeric_arg_unless_prefix();
        Ok(EditorOutcome::Continue)
    }

    pub(super) fn handle_reverse_search(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        if bytes.len() > 1 {
            return self.handle_reverse_search_batched(state, bytes, hooks);
        }
        self.handle_reverse_search_single(state, bytes, hooks)
    }

    fn handle_reverse_search_batched(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        // Fragmentation-invariant: a batched chunk mixing query bytes with
        // search controls must agree with split reads. Dispatch per byte,
        // keeping multi-byte non-SelfInsert bindings atomic so batched
        // escape sequences still terminate-and-execute as one unit.
        let mut pos = 0;
        while pos < bytes.len() {
            if state.search.reverse_search.is_none() {
                return self.handle_bytes(state, &bytes[pos..], hooks);
            }
            if state.search.quoted_pending {
                let single = [bytes[pos]];
                let outcome = self.handle_reverse_search_single(state, &single, hooks)?;
                if !matches!(outcome, EditorOutcome::Continue) {
                    return Ok(outcome);
                }
                pos += 1;
                continue;
            }
            let suffix = &bytes[pos..];
            if let Some((len, binding)) = self
                .keymap
                .longest_matching_prefix(self.keymap.current(), suffix)
                .map(|(len, binding)| (len, binding.clone()))
            {
                let first_is_search_control =
                    matches!(suffix[0], b'\r' | b'\n' | 0x07 | 0x12 | 0x13 | 0x7f)
                        || self.is_isearch_terminator(&suffix[0..1]);
                if len > 1
                    && !first_is_search_control
                    && !matches!(binding, KeyBinding::Command(EditCommand::SelfInsert))
                {
                    let chunk = suffix[..len].to_vec();
                    let outcome = self.handle_reverse_search_single(state, &chunk, hooks)?;
                    if !matches!(outcome, EditorOutcome::Continue) {
                        return Ok(outcome);
                    }
                    pos += len;
                    continue;
                }
            } else if self.keymap.has_prefix(self.keymap.current(), suffix) {
                return self.handle_reverse_search_single(state, suffix, hooks);
            }
            let single = [bytes[pos]];
            let outcome = self.handle_reverse_search_single(state, &single, hooks)?;
            if !matches!(outcome, EditorOutcome::Continue) {
                return Ok(outcome);
            }
            pos += 1;
        }
        Ok(EditorOutcome::Continue)
    }

    fn refresh_reverse_search_match(&self, search: &mut ReverseSearchState, repeat: bool) -> bool {
        let ignore_case = self.flag(BoolVariable::SearchIgnoreCase);
        let locale_lowercase = self.search_locale_lowercase(ignore_case);
        let displayed = search.match_line.is_some() || search.retain_match_on_failure;
        let found = update_reverse_search_match(
            search,
            &self.history,
            repeat,
            ignore_case,
            locale_lowercase.as_ref(),
        );
        // A failed query edit leaves the last displayed match in the buffer;
        // only the accept path uses it. The query prompt still reports the
        // failure, and Ctrl-G restores the original line independently.
        if !found && !repeat && displayed && std::str::from_utf8(&search.query).is_ok() {
            search.retain_failed_query_line = true;
        }
        found
    }

    /// Separators that make GNU retain an incremental-search query on Ctrl-W
    /// on the measured Readline 8.3 host: ASCII whitespace, `-`, `,`, `_`
    /// and `.`. These are the measured bytes, not a general word-rubout model.
    fn isearch_ctrl_w_separator(byte: u8) -> bool {
        byte.is_ascii_whitespace() || matches!(byte, b'-' | b',' | b'_' | b'.')
    }

    /// Restore the selected incremental-search line for ordinary editing.
    /// Both the terminator and Ctrl-U leave search state with the line
    /// established here; their bell and execute decisions stay separate.
    fn restore_isearch_line_for_editing(
        &mut self,
        state: &mut EditorState,
        search: &ReverseSearchState,
        keep_failed_display: bool,
    ) {
        let accepted =
            if keep_failed_display && search.match_line.is_none() && !search.query.is_empty() {
                // Ctrl-U keeps the line displayed by a failed query, unlike
                // terminate-without-execute which restores the original line.
                state.buffer.as_bytes().to_vec()
            } else {
                accept_search_line(state, search, false)
            };
        let mut buffer = LineBuffer::from_bytes(accepted);
        if search.query.is_empty() {
            buffer.set_point(search.original_point.min(buffer.len_chars()));
        } else if search.match_line.is_some() {
            let ignore_case = self.flag(BoolVariable::SearchIgnoreCase);
            let locale_lowercase = self.search_locale_lowercase(ignore_case);
            buffer.set_point(isearch_terminate_point(
                buffer.as_bytes(),
                &search.query,
                ignore_case,
                locale_lowercase.as_ref(),
            ));
        } else if matches!(search.direction, SearchDirection::Forward)
            && search.original_history_pos >= self.history.entries().len()
        {
            buffer.set_point(search.original_point.min(buffer.len_chars()));
        } else {
            buffer.set_point(0);
        }
        state.buffer = buffer;
        state.history_entry_index = if search.match_line.is_some() {
            search.match_index
        } else {
            search.original_history_entry_index
        };
        if search.match_line.is_none() {
            state.undo = search.original_undo.clone();
        }
        if let Some(index) = search.match_index {
            self.history.set_pos(index);
        }
        save_incremental_search(state, search);
        state.after_non_kill_command();
    }

    fn handle_reverse_search_single(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        // An unquoted NUL freezes the query: after the freeze, following
        // query-editing input is ignored, whether printable, quoted or
        // control. Accept, abort, Ctrl-C, Ctrl-U and configured isearch
        // terminators keep their normal paths. The measured cursor commands
        // instead leave the search and run on the established line. The
        // measured editing, history, completion and display keys listed
        // below also leave the search. C-r and the measured query-editing
        // control prefixes ring once while staying frozen; other controls
        // and repeated NUL are silent, and printable or quoted literal bytes
        // ring once only for a failed freeze.
        let nul_frozen = state
            .search
            .reverse_search
            .as_ref()
            .is_some_and(|search| search.nul_pending);
        if nul_frozen && state.search.quoted_pending {
            state.search.quoted_pending = false;
            if state
                .search
                .reverse_search
                .as_ref()
                .is_some_and(|search| search.nul_failed)
            {
                self.ding()?;
            }
            return Ok(EditorOutcome::Continue);
        }
        if nul_frozen && self.is_quoted_insert_binding(bytes) {
            state.search.quoted_pending = true;
            return Ok(EditorOutcome::Continue);
        }
        if nul_frozen
            && !matches!(bytes, [0x0d] | [0x0a] | [0x07] | [0x03] | [0x15])
            && !self.is_isearch_terminator(bytes)
        {
            // GNU applies the bound cursor commands after the freeze, with
            // point where the search left it. Only the measured keys are
            // routed here; unmeasured controls keep the frozen behavior.
            if bytes.len() == 1
                && NUL_FROZEN_LINE_COMMAND_KEYS.contains(&bytes[0])
                && let Some(binding) = self.keymap.lookup(self.keymap.current(), bytes).cloned()
            {
                let search = state
                    .search
                    .reverse_search
                    .take()
                    .expect("NUL-frozen search state");
                self.restore_isearch_line_for_editing(state, &search, true);
                return self.apply_binding(state, binding, bytes, hooks);
            }
            let failed = state
                .search
                .reverse_search
                .as_ref()
                .is_some_and(|search| search.nul_failed);
            // GNU rings once for each of these frozen query-editing control
            // bytes unless the NUL freeze itself already failed and rang. C-r
            // rings in either case.
            if bytes.len() == 1 && NUL_FROZEN_RING_KEYS.contains(&bytes[0]) {
                if !failed {
                    self.ding()?;
                }
                return Ok(EditorOutcome::Continue);
            }
            if bytes == [0x12] {
                self.ding()?;
                return Ok(EditorOutcome::Continue);
            }
            // C-^ has no default binding, yet GNU ends the frozen search and
            // rings; the following bytes then edit the established line.
            if bytes == [0x1e]
                && let Some(search) = state.search.reverse_search.take()
            {
                self.restore_isearch_line_for_editing(state, &search, true);
                self.ding()?;
                return Ok(EditorOutcome::Continue);
            }
            if failed && !bytes.iter().all(u8::is_ascii_control) {
                self.ding()?;
            }
            return Ok(EditorOutcome::Continue);
        }
        // Search-scoped quoted-insert: a pending quote consumes the next
        // byte literally into the query, even controls/terminators.
        if state.search.quoted_pending {
            state.search.quoted_pending = false;
            let Some(mut search) = state.search.reverse_search.take() else {
                return Ok(EditorOutcome::Continue);
            };
            search.query.extend_from_slice(bytes);
            search.match_index = None;
            let found = self.refresh_reverse_search_match(&mut search, false);
            self.apply_search_match(state, &search);
            // GNU per-keystroke bell (patch 0 Bash 5.3 PTY oracle): each
            // failing emacs incremental query extension rings once.
            // Vi `/`/`?` are non-incremental (separate state) and bell
            // once at execute instead.
            if !found && !search.query.is_empty() && !search.exclude_cursor {
                self.ding()?;
            }
            state.search.reverse_search = Some(search);
            state.input.changed_display = true;
            return Ok(EditorOutcome::Continue);
        }
        if let Some(translated) = self.translate_meta_input(bytes) {
            // Keep the Meta sequence atomic for terminator and binding lookup;
            // a pending quote above consumes the raw byte instead.
            return self.handle_reverse_search_single(state, &translated, hooks);
        }
        // Incremental searches allow anything bound to quoted-insert to
        // start a quote (CHANGES 8.3 2.i inside the patch 0 baseline).
        if self.is_quoted_insert_binding(bytes) {
            let Some(search) = state.search.reverse_search.take() else {
                return Ok(EditorOutcome::Continue);
            };
            state.search.reverse_search = Some(search);
            state.search.quoted_pending = true;
            return Ok(EditorOutcome::Continue);
        }
        // NUL is set-mark in the default keymap. Inside a search GNU leaves
        // the query and the selected match in place and freezes the query.
        // The measured freeze is failed when the query has no match or a
        // repeat in the same direction would move off the current match;
        // a selected match with no further match is a successful freeze,
        // and an empty query never fails.
        if bytes == [0x00] {
            let Some(mut search) = state.search.reverse_search.take() else {
                return Ok(EditorOutcome::Continue);
            };
            let repeat_moves = !search.query.is_empty() && {
                let mut repeated = search.clone();
                self.refresh_reverse_search_match(&mut repeated, true)
            };
            let failed = !search.query.is_empty() && (search.match_line.is_none() || repeat_moves);
            search.nul_pending = true;
            search.nul_failed = failed;
            state.search.reverse_search = Some(search);
            state.input.changed_display = true;
            if failed {
                self.ding()?;
            }
            return Ok(EditorOutcome::Continue);
        }
        let Some(mut search) = state.search.reverse_search.take() else {
            return Ok(EditorOutcome::Continue);
        };
        // GNU Readline redraws the search display for every key of an
        // incremental search, so the query and the match are drawn as
        // they change.
        state.input.changed_display = true;

        let outcome = match bytes {
            bytes if self.is_isearch_terminator(bytes) => {
                state.search.quoted_pending = false;
                // GNU point after terminate-without-execute (patch 0 Bash 5.3
                // PTY oracle): empty query restores the original point;
                // a match leaves point at the match start (`alpha` + C-J +
                // `!` yields `!alpha two`, `two` + C-J + `!` yields
                // `alpha !two`); a non-empty query with no match leaves
                // point at 0, except a forward search started at the end of
                // history keeps the original point (`draft` + C-s + `zzz` +
                // C-J + `!` yields `draft!`, while the same query from a
                // history line yields `!beta`). The history cursor stays on
                // the match so a later `n`/`N` steps past it.
                self.restore_isearch_line_for_editing(state, &search, false);
                // No-match terminate keeps the original line (no
                // query-as-line); emacs already belled per keystroke.
                if search.match_line.is_none() && !search.query.is_empty() && search.exclude_cursor
                {
                    self.ding()?;
                }
                EditorOutcome::Continue
            }
            b"\r" | b"\n" => {
                state.search.quoted_pending = false;
                let accepted = accept_search_line(state, &search, true);
                state.buffer = LineBuffer::from_bytes(accepted.clone());
                state.history_entry_index = if search.match_line.is_some() {
                    search.match_index
                } else {
                    search.original_history_entry_index
                };
                if search.match_line.is_none() {
                    state.undo = search.original_undo.clone();
                }
                if search.match_line.is_none() && !search.query.is_empty() && search.exclude_cursor
                {
                    self.ding()?;
                }
                save_incremental_search(state, &search);
                state.after_non_kill_command();
                EditorOutcome::Accepted(accepted)
            }
            &[0x07] => {
                state.search.quoted_pending = false;
                state.buffer = LineBuffer::from_bytes(search.original_line.clone());
                state.history_entry_index = search.original_history_entry_index;
                state.undo = search.original_undo.clone();
                state.after_non_kill_command();
                EditorOutcome::Continue
            }
            // Ctrl-W applies to the query, not to the line already selected
            // by the current match: it clears a query without a measured
            // separator (ringing once, and again with no match); with one of
            // those separators it retains the query, keeping the selected
            // line when a later extension fails. Ctrl-U instead ends the
            // search and applies its measured binding rule on the
            // established line. The bell counts below follow measured cases,
            // not a general rubout model.
            &[0x15] => {
                // Ctrl-U ends the search and establishes the selected line.
                // GNU honors a macro/string rebinding here, but a command or
                // named-command rebinding still performs unix-line-discard
                // from the match start; that command rings only when the
                // range is empty, which is the measured negative control.
                state.search.quoted_pending = false;
                self.restore_isearch_line_for_editing(state, &search, true);
                return match self.keymap.lookup(self.keymap.current(), bytes).cloned() {
                    Some(binding @ KeyBinding::Macro(_)) => {
                        self.apply_binding(state, binding, bytes, hooks)
                    }
                    _ => self.apply_binding(
                        state,
                        KeyBinding::Command(EditCommand::UnixLineDiscard),
                        bytes,
                        hooks,
                    ),
                };
            }
            &[0x17] => {
                let has_separator = search
                    .query
                    .iter()
                    .copied()
                    .any(Self::isearch_ctrl_w_separator);
                if !has_separator {
                    let no_match = search.match_line.is_none();
                    search.query.clear();
                    self.ding()?;
                    if no_match {
                        self.ding()?;
                    }
                } else {
                    // Even a retained query that never matched keeps the
                    // displayed line when a later query extension fails.
                    search.retain_match_on_failure = true;
                    let no_match = search.match_line.is_none();
                    let without_trailing_whitespace = search.query.trim_ascii_end();
                    let silent = !no_match
                        && without_trailing_whitespace.len() != search.query.len()
                        && !without_trailing_whitespace
                            .iter()
                            .copied()
                            .any(Self::isearch_ctrl_w_separator);
                    if no_match {
                        // A failed retained query rings twice, the extra bell
                        // matching the single-word no-match case.
                        self.ding()?;
                        self.ding()?;
                    } else if !silent {
                        self.ding()?;
                    }
                }
                state.search.reverse_search = Some(search);
                EditorOutcome::Continue
            }
            &[0x12] | &[0x13] => {
                let direction = if bytes == [0x12] {
                    SearchDirection::Backward
                } else {
                    SearchDirection::Forward
                };
                return self.repeat_reverse_search(state, search, direction);
            }
            &[0x7f] => {
                search.query.pop();
                search.match_index = None;
                self.refresh_reverse_search_match(&mut search, false);
                if !self.apply_search_match(state, &search) {
                    state.buffer = LineBuffer::from_bytes(search.original_line.clone());
                    state.history_entry_index = search.original_history_entry_index;
                    state.undo = search.original_undo.clone();
                }
                state.search.reverse_search = Some(search);
                EditorOutcome::Continue
            }
            _ => return self.update_search_with_input(state, search, bytes, hooks),
        };
        Ok(outcome)
    }

    fn update_search_with_input(
        &mut self,
        state: &mut EditorState,
        mut search: ReverseSearchState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        // In a non-UTF-8 C/POSIX raw-meta configuration, a high-bit byte
        // leaves search and is dispatched as ordinary input, not added to
        // the query. An opted-in UTF-8 byte-I/O backend overrides the process
        // locale here. Pending quoted input was already consumed by
        // handle_reverse_search_single.
        let raw_meta = !self.variables.utf8_locale()
            && matches!(self.variables.lc_ctype(), Some("C" | "POSIX"))
            && !self.flag(BoolVariable::ConvertMeta)
            && self.flag(BoolVariable::InputMeta)
            && bytes.first().is_some_and(|byte| byte & 0x80 != 0);
        // GNU incremental search appends printable input to the query even
        // when the active keymap binds it: only controls/escape sequences
        // terminate and execute. Bypass the keymap for chunks without
        // ASCII controls.
        if !raw_meta && !bytes.is_empty() && !bytes.iter().any(|byte| byte.is_ascii_control()) {
            let input = bytes.to_vec();
            search.query.extend(input);
            search.match_index = None;
            let found = self.refresh_reverse_search_match(&mut search, false);
            self.apply_search_match(state, &search);
            if !found && !search.query.is_empty() && !search.exclude_cursor {
                self.ding()?;
            }
            state.search.reverse_search = Some(search);
            return Ok(EditorOutcome::Continue);
        }
        let command_binding = self
            .keymap
            .lookup(self.keymap.current(), bytes)
            .filter(|binding| !matches!(binding, KeyBinding::Command(EditCommand::SelfInsert)))
            .cloned();
        // A key invoking a search command while a search is active repeats
        // (switching direction) instead of accepting and restarting.
        let repeat_direction = match &command_binding {
            Some(KeyBinding::Command(EditCommand::ReverseSearchHistory)) => {
                Some(SearchDirection::Backward)
            }
            Some(KeyBinding::NamedCommand(name)) if name == "forward-search-history" => {
                Some(SearchDirection::Forward)
            }
            _ => None,
        };
        if !raw_meta && let Some(direction) = repeat_direction {
            return self.repeat_reverse_search(state, search, direction);
        }
        if raw_meta
            || command_binding.is_some()
            || self.keymap.has_prefix(self.keymap.current(), bytes)
        {
            // Leaving search with a command key still restores the original
            // line after a failed query; only Enter keeps the displayed line.
            let accepted = accept_search_line(state, &search, false);
            state.buffer = LineBuffer::from_bytes(accepted);
            if raw_meta && search.match_line.is_some() {
                // Like explicit termination, raw-meta exit leaves point at
                // the match start before dispatching the ordinary input.
                let ignore_case = self.flag(BoolVariable::SearchIgnoreCase);
                let locale_lowercase = self.search_locale_lowercase(ignore_case);
                state.buffer.set_point(isearch_terminate_point(
                    state.buffer.as_bytes(),
                    &search.query,
                    ignore_case,
                    locale_lowercase.as_ref(),
                ));
            }
            if search.match_line.is_none() {
                state.history_entry_index = search.original_history_entry_index;
                state.undo = search.original_undo.clone();
            }
            if let Some(index) = search.match_index {
                self.history.set_pos(index);
            }
            save_incremental_search(state, &search);
            state.after_non_kill_command();
            if let Some(binding) = command_binding {
                return self.apply_binding(state, binding, bytes, hooks);
            }
            return self.handle_bytes(state, bytes, hooks);
        }
        // Fragmentation-invariant: keep ASCII non-controls even when the
        // chunk as a whole is invalid UTF-8, matching split reads.
        let input = bytes
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_control())
            .collect::<Vec<_>>();
        if !input.is_empty() {
            search.query.extend(input);
            search.match_index = None;
            let found = self.refresh_reverse_search_match(&mut search, false);
            self.apply_search_match(state, &search);
            if !found && !search.query.is_empty() && !search.exclude_cursor {
                self.ding()?;
            }
        }
        state.search.reverse_search = Some(search);
        Ok(EditorOutcome::Continue)
    }

    fn is_quoted_insert_binding(&self, bytes: &[u8]) -> bool {
        matches!(
            self.keymap.lookup(self.keymap.current(), bytes),
            Some(KeyBinding::Command(EditCommand::QuotedInsert))
        )
    }

    /// Repeat an active incremental search, switching direction.
    ///
    /// GNU repeats (rather than accepts and restarts) when the key invokes
    /// a search command while a search is active. A repeat with an empty
    /// query recalls the previous completed incremental search; with no match index the
    /// recalled query searches inclusively from the cursor like a fresh
    /// search, and a repeat with no further match keeps the line and bells.
    pub(super) fn repeat_reverse_search(
        &mut self,
        state: &mut EditorState,
        mut search: ReverseSearchState,
        direction: SearchDirection,
    ) -> Result<EditorOutcome, ReadlineError> {
        search.direction = direction;
        if search.query.is_empty()
            && let Some(previous) = state.search.last_incremental_search.clone()
        {
            search.query = previous;
        }
        let found = self.refresh_reverse_search_match(&mut search, true);
        self.apply_search_match(state, &search);
        if !found && !search.query.is_empty() {
            self.ding()?;
        }
        state.search.reverse_search = Some(search);
        Ok(EditorOutcome::Continue)
    }

    fn apply_search_match(&mut self, state: &mut EditorState, search: &ReverseSearchState) -> bool {
        if let Some(line) = &search.match_line {
            self.replace_from_history_at(state, search.match_index, line);
            true
        } else {
            false
        }
    }

    pub(super) fn handle_non_incremental_search(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        if bytes.len() > 1 {
            let mut pos = 0;
            while pos < bytes.len() {
                if state.search.non_incremental_search.is_none() {
                    return self.handle_bytes(state, &bytes[pos..], hooks);
                }
                if state.search.quoted_pending {
                    let single = [bytes[pos]];
                    let outcome = self.handle_non_incremental_single(state, &single)?;
                    if !matches!(outcome, EditorOutcome::Continue) {
                        return Ok(outcome);
                    }
                    pos += 1;
                    continue;
                }
                let single = [bytes[pos]];
                let outcome = self.handle_non_incremental_single(state, &single)?;
                if !matches!(outcome, EditorOutcome::Continue) {
                    return Ok(outcome);
                }
                pos += 1;
            }
            return Ok(EditorOutcome::Continue);
        }
        self.handle_non_incremental_single(state, bytes)
    }

    fn handle_non_incremental_single(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
    ) -> Result<EditorOutcome, ReadlineError> {
        // Search-scoped quote: consume the next byte literally.
        if state.search.quoted_pending {
            state.search.quoted_pending = false;
            let Some(mut search) = state.search.non_incremental_search.take() else {
                return Ok(EditorOutcome::Continue);
            };
            search.query.extend_from_slice(bytes);
            state.search.non_incremental_search = Some(search);
            state.input.changed_display = true;
            return Ok(EditorOutcome::Continue);
        }
        // Non-incremental searches quote only ^V/^Q (CHANGES 8.3 2.i:
        // "in the former case" = incremental allows any binding).
        if matches!(bytes, [0x16] | [0x11]) {
            let Some(search) = state.search.non_incremental_search.take() else {
                return Ok(EditorOutcome::Continue);
            };
            state.search.non_incremental_search = Some(search);
            state.search.quoted_pending = true;
            return Ok(EditorOutcome::Continue);
        }
        let Some(search) = state.search.non_incremental_search.take() else {
            return Ok(EditorOutcome::Continue);
        };
        // The query is drawn in the search prompt, and the match replaces
        // the line when the search executes, so every key redraws.
        state.input.changed_display = true;
        match bytes {
            b"\r" | b"\n" => {
                state.search.quoted_pending = false;
                let query = if search.query.is_empty() {
                    state
                        .search
                        .last_non_incremental_search
                        .clone()
                        .unwrap_or_default()
                } else {
                    search.query.clone()
                };
                // GNU bells once on empty or failed execute (patch 0 Bash
                // 5.3 PTY oracle: `:`, `/`, `?` with `zzz` or empty all
                // bell and clear back to the original line).
                if query.is_empty() {
                    self.ding()?;
                    state.after_non_kill_command();
                    return Ok(EditorOutcome::Continue);
                }
                // GNU vi `/` and `?` searches remain case-sensitive even
                // when `search-ignore-case` is on (Bash 5.3 PTY oracle).
                let ignore_case = !search.vi && self.flag(BoolVariable::SearchIgnoreCase);
                // Vi skips the history cursor like the incremental
                // cursor-skip oracle: backward covers
                // entries[..original_pos] and forward covers
                // entries[original_pos + 1..], so a matching cursor finds
                // its neighbor. Emacs stays inclusive via the shared
                // history search in both directions.
                let found = if search.vi {
                    match search.direction {
                        SearchDirection::Backward => {
                            let end = search
                                .original_history_pos
                                .min(self.history.entries().len());
                            search_history_backward(
                                &self.history,
                                &query,
                                Some(end),
                                ignore_case,
                                None,
                            )
                        }
                        SearchDirection::Forward => {
                            let start = search
                                .original_history_pos
                                .saturating_add(1)
                                .min(self.history.entries().len());
                            search_history_forward_from(
                                &self.history,
                                &query,
                                start,
                                ignore_case,
                                None,
                            )
                        }
                    }
                } else {
                    let direction = match search.direction {
                        SearchDirection::Backward => HistoryDirection::Previous,
                        SearchDirection::Forward => HistoryDirection::Next,
                    };
                    let locale_lowercase = self.search_locale_lowercase(ignore_case);
                    history_search_with_locale(
                        &mut self.history,
                        &query,
                        direction,
                        ignore_case,
                        locale_lowercase.as_ref(),
                    )
                    .map(|found| (found.entry_index, found.line_bytes.clone()))
                };
                let found_index = found.as_ref().map(|(index, _)| *index);
                if let Some((index, line)) = found {
                    // GNU emacs non-incremental search started on a history
                    // entry displays the match without moving the history
                    // cursor; search started from the current-line position
                    // moves it to the match. Keep vi's existing cursor rule.
                    let history_len = self.history.entries().len();
                    let cursor = if !search.vi && search.original_history_pos < history_len {
                        search.original_history_pos
                    } else {
                        index
                    };
                    self.history.set_pos(cursor);
                    self.replace_from_history_at(state, Some(index), &line);
                } else {
                    self.history.set_pos(search.original_history_pos);
                    self.ding()?;
                }
                state.search.last_non_incremental_search = Some(query);
                state.search.last_non_incremental_search_direction = Some(search.direction);
                state.search.last_non_incremental_search_match_index = found_index;
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            &[0x07] => {
                state.search.quoted_pending = false;
                state.buffer = LineBuffer::from_bytes(search.original_line.clone());
                state.history_entry_index = search.original_history_entry_index;
                state.undo = search.original_undo.clone();
                self.history.set_pos(search.original_history_pos);
                // GNU vi `/`/`?` rings the bell when C-g aborts the search;
                // incremental and emacs non-incremental searches stay quiet.
                if search.vi {
                    self.ding()?;
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            &[0x1b] if search.vi => {
                // GNU vi non-incremental search treats ESC as query text, not
                // an abort key: a history entry containing ESC is matchable.
                let mut search = search;
                search.query.extend_from_slice(bytes);
                state.search.non_incremental_search = Some(search);
                state.input.changed_display = true;
                Ok(EditorOutcome::Continue)
            }
            &[0x1b] => {
                state.search.quoted_pending = false;
                state.buffer = LineBuffer::from_bytes(search.original_line.clone());
                state.history_entry_index = search.original_history_entry_index;
                state.undo = search.original_undo.clone();
                self.history.set_pos(search.original_history_pos);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            &[0x7f] => {
                let mut search = search;
                search.query.pop();
                state.search.non_incremental_search = Some(search);
                Ok(EditorOutcome::Continue)
            }
            _ => {
                // Fragmentation-invariant: keep ASCII non-controls even for
                // invalid chunks, matching split reads.
                let mut search = search;
                search.query.extend(
                    bytes
                        .iter()
                        .copied()
                        .filter(|byte| !byte.is_ascii_control()),
                );
                state.search.non_incremental_search = Some(search);
                Ok(EditorOutcome::Continue)
            }
        }
    }

    pub(super) fn handle_named_command(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        // Fragmentation-invariant: a batched chunk may mix query text with
        // the terminating Enter/ESC/DEL, so dispatch per unit like split
        // reads instead of swallowing embedded controls as query text.
        if bytes.len() > 1 {
            let mut pos = 0;
            while pos < bytes.len() {
                if state.input.named_command.is_none() {
                    return self.handle_bytes(state, &bytes[pos..], hooks);
                }
                let remaining = &bytes[pos..];
                // Single-byte controls first, matching the one-byte path.
                if remaining.len() == 1 || matches!(remaining[0], b'\r' | b'\n' | 0x07 | 0x7f) {
                    let single = [remaining[0]];
                    let outcome = self.handle_named_command_single(state, &single, hooks)?;
                    if !matches!(outcome, EditorOutcome::Continue) {
                        return Ok(outcome);
                    }
                    pos += 1;
                    continue;
                }
                let (first, _) = split_first_unit(remaining);
                let outcome = self.handle_named_command_single(state, first, hooks)?;
                if !matches!(outcome, EditorOutcome::Continue) {
                    return Ok(outcome);
                }
                pos += first.len();
            }
            return Ok(EditorOutcome::Continue);
        }
        self.handle_named_command_single(state, bytes, hooks)
    }

    fn handle_named_command_single(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        // Ctrl-V quotes just the next unit: consume the pending flag before
        // any query control is interpreted, so even a normally active control
        // reaches the query as literal text.
        if state
            .input
            .named_command
            .as_ref()
            .is_some_and(|command| command.quoted_pending)
        {
            if let Some(command) = state.input.named_command.as_mut() {
                command.quoted_pending = false;
            }
            match bytes {
                b"\x00" | b"\x1a" => return Ok(EditorOutcome::Continue),
                // A quoted LF still submits the query through the Enter arm
                // below; a quoted CR is literal text and renders as ^M.
                b"\n" => {}
                _ => {
                    if let Some(command) = state.input.named_command.as_mut() {
                        state.input.changed_display |=
                            append_named_query_bytes(command, bytes, true);
                    }
                    return Ok(EditorOutcome::Continue);
                }
            }
        }
        match bytes {
            // GNU treats an unquoted NUL inside the query like Enter: the
            // name is submitted and the remaining bytes edit the main line.
            b"\x00" | b"\r" | b"\n" => {
                let mut pending = state.input.named_command.take().unwrap_or_default();
                flush_named_query(&mut pending);
                let command = pending.query.trim();
                if command.is_empty() {
                    // GNU treats an empty name as a silent no-op that restores
                    // the line; only an unknown non-empty name rings. Mirror a
                    // named command's redraw and count consumption without the
                    // catch-all's bell.
                    state.input.applied_command = true;
                    state.after_non_kill_command();
                    state.consume_numeric_arg_unless_prefix();
                    return Ok(EditorOutcome::Continue);
                }
                if let Some(key) = pending.invoking_key {
                    match command {
                        "digit-argument" => {
                            if state.numeric_arg.is_none() {
                                state.take_numeric_arg();
                                if key.is_ascii_digit() {
                                    state.numeric_arg = Some((key - b'0') as i32);
                                    state.input.applied_command = true;
                                    state.after_non_kill_command();
                                    return Ok(EditorOutcome::Continue);
                                }
                                state.numeric_arg = Some(1);
                            }
                            // Dispatch the invoking byte, not the Meta
                            // sequence or the Enter that submitted the name.
                            return self.handle_bytes(state, &[key], hooks);
                        }
                        "self-insert" => {
                            // GNU inserts the invoking byte as a literal
                            // self-insert even when that byte is bound to
                            // another command. Applying SelfInsert directly
                            // keeps the count and undo/redraw semantics but
                            // avoids re-running the keymap on the byte.
                            return self.apply_binding(
                                state,
                                KeyBinding::Command(EditCommand::SelfInsert),
                                &[key],
                                hooks,
                            );
                        }
                        _ => {}
                    }
                }
                self.apply_named_command(state, command, b"", hooks)
            }
            &[0x07] => {
                // GNU rings once when abort dismisses the named-command
                // prompt, whether or not a query has been typed. Dismissing
                // the prompt without submitting a command must not carry a
                // pending count into the next ordinary key. Submitted named
                // commands (digit-argument/self-insert) keep it.
                self.ding()?;
                state.take_numeric_arg();
                if let Some(command) = state.input.named_command.as_mut() {
                    command.pending_bytes.clear();
                }
                state.input.named_command = None;
                state.input.changed_display = true;
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            &[0x7f] => {
                if let Some(command) = state.input.named_command.as_mut() {
                    let removed =
                        command.pending_bytes.pop().is_some() || command.query.pop().is_some();
                    state.input.changed_display |= removed;
                }
                Ok(EditorOutcome::Continue)
            }
            &[0x15] | &[0x17] => {
                // GNU clears the whole query for both unix-line-discard and
                // unix-word-rubout here; only an empty query rings.
                let empty = state.input.named_command.as_ref().is_some_and(|command| {
                    command.query.is_empty() && command.pending_bytes.is_empty()
                });
                if let Some(command) = state.input.named_command.as_mut() {
                    command.query.clear();
                    command.pending_bytes.clear();
                    command.quoted_pending = false;
                }
                state.input.changed_display = true;
                if empty {
                    self.ding()?;
                }
                Ok(EditorOutcome::Continue)
            }
            &[0x16] => {
                if let Some(command) = state.input.named_command.as_mut() {
                    command.quoted_pending = true;
                }
                Ok(EditorOutcome::Continue)
            }
            &[b'\t'] | &[b' '] => {
                let before = state
                    .input
                    .named_command
                    .as_ref()
                    .map(|command| command.query.clone());
                self.complete_named_command_query(state, hooks)?;
                if before
                    != state
                        .input
                        .named_command
                        .as_ref()
                        .map(|command| command.query.clone())
                {
                    state.input.changed_display = true;
                }
                Ok(EditorOutcome::Continue)
            }
            _ => {
                if state.input.named_command.is_none() {
                    return self.handle_bytes(state, bytes, hooks);
                }
                if let Some(command) = state.input.named_command.as_mut() {
                    state.input.changed_display |= append_named_query_bytes(command, bytes, false);
                }
                Ok(EditorOutcome::Continue)
            }
        }
    }

    fn complete_named_command_query(
        &mut self,
        state: &mut EditorState,
        hooks: &mut impl Hooks,
    ) -> Result<(), ReadlineError> {
        let Some(pending) = state.input.named_command.as_ref() else {
            return Ok(());
        };
        let prefix = pending.query.as_bytes().to_vec();
        let mut candidates: Vec<_> =
            crate::keymap::bind_function_names(self.variable_is_on("sushline-vi-registers"))
                .into_iter()
                .map(str::as_bytes)
                .filter(|name| name.starts_with(&prefix))
                .map(<[u8]>::to_vec)
                .collect();
        candidates.extend(
            hooks
                .named_command_names()
                .into_iter()
                .filter(|name| name.starts_with(&prefix)),
        );
        // GNU rings once for a completion with no candidate, as ordinary
        // completion does. An empty query has every name as a candidate, so
        // it stays silent.
        if candidates.is_empty() {
            self.ding()?;
            return Ok(());
        }
        let common = crate::completion::display::longest_common_bytes_prefix(
            candidates.iter().map(Vec::as_slice),
        );
        if let Some(common) = common
            && common.len() > prefix.len()
            && let Some(pending) = state.input.named_command.as_mut()
        {
            pending.query = String::from_utf8_lossy(&common).into_owned();
        } else if candidates.len() > 1 {
            // An ambiguous query that cannot extend must reveal the names it
            // matches, just as ordinary completion lists its candidates.
            let response = crate::completion::CompletionResponse {
                candidates: candidates
                    .into_iter()
                    .map(crate::completion::CompletionCandidate::plain)
                    .collect(),
                options: crate::completion::CompletionOptions::default(),
            };
            self.display_completions_for_word_without_query(state, &response, &prefix, hooks)?;
        }
        Ok(())
    }
}

fn utf8_expected_len(lead: u8) -> Option<usize> {
    match lead {
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

/// True while `bytes` could still become one valid UTF-8 char with more
/// input: a valid lead byte plus only continuation bytes so far, shorter
/// than the char length.
fn is_incomplete_utf8_prefix(bytes: &[u8]) -> bool {
    let Some(first) = bytes.first() else {
        return false;
    };
    let Some(expected) = utf8_expected_len(*first) else {
        return false;
    };
    bytes.len() < expected && bytes[1..].iter().all(|byte| (0x80..=0xBF).contains(byte))
}

/// Splits the first input unit (one ASCII byte or one complete multibyte
/// char, falling back to one byte) from the remainder, so batched chunks
/// take the same path as split reads. Empty input splits as empty/empty.
fn split_first_unit(bytes: &[u8]) -> (&[u8], &[u8]) {
    if bytes.is_empty() {
        return (&[], &[]);
    }
    let len = first_unit_len(bytes)
        .min(bytes.len())
        .max(1)
        .min(bytes.len());
    bytes.split_at(len)
}

/// Splits the first byte of the input from the remainder, so a command
/// that reads one byte takes it whether the input arrives in one batch or
/// one byte at a time.
fn split_first_byte(bytes: &[u8]) -> (&[u8], &[u8]) {
    if bytes.is_empty() {
        return (&[], &[]);
    }
    bytes.split_at(1)
}

/// Assembles the key a pending character search consumes.
///
/// In byte-oriented mode the target is exactly one raw byte and the rest of
/// the input is redispatched. Otherwise GNU Readline reads a whole character
/// before a command that takes one consumes it, so a multibyte search
/// target is one character: the search matches it and the rest of it never
/// reaches the line. The bytes of an incomplete character wait here for the
/// rest of it.
fn take_pending_character(
    state: &mut EditorState,
    bytes: &[u8],
    bytewise: bool,
) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut buffered = std::mem::take(&mut state.input.pending_char_bytes);
    buffered.extend_from_slice(bytes);
    if bytewise {
        if buffered.is_empty() {
            return Some((Vec::new(), Vec::new()));
        }
        let rest = buffered.split_off(1);
        return Some((buffered, rest));
    }
    if is_incomplete_utf8_prefix(&buffered) {
        state.input.pending_char_bytes = buffered;
        return None;
    }
    let (first, rest) = split_first_unit(&buffered);
    Some((first.to_vec(), rest.to_vec()))
}

fn first_unit_len(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    if bytes[0] < 0x80 {
        return 1;
    }
    for len in 1..=4.min(bytes.len()) {
        if let Ok(text) = std::str::from_utf8(&bytes[..len])
            && let Some(ch) = text.chars().next()
        {
            // Earliest valid prefix ends exactly at the first char boundary;
            // longer valid prefixes still start with the same char.
            if text.chars().count() >= 1 {
                return ch.len_utf8();
            }
        }
    }
    1
}

fn replacement_unit(bytes: &[u8]) -> Vec<u8> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        text.chars()
            .find(|ch| !ch.is_control())
            .map(|ch| {
                let mut buf = [0; 4];
                ch.encode_utf8(&mut buf).as_bytes().to_vec()
            })
            .unwrap_or_default()
    } else {
        bytes
            .iter()
            .copied()
            .filter(|byte| *byte >= 0x80)
            .take(1)
            .collect()
    }
}

fn char_search_key(bytes: &[u8]) -> Option<char> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        text.chars().find(|ch| !ch.is_control())
    } else {
        bytes
            .iter()
            .copied()
            .find(|byte| *byte >= 0x80)
            .map(LineBuffer::search_char_for_byte)
    }
}

fn isearch_terminate_point(
    line: &[u8],
    query: &[u8],
    ignore_case: bool,
    locale_lowercase: Option<&crate::locale::LocaleLowercase>,
) -> usize {
    if query.is_empty() {
        return 0;
    }
    let Some(byte_offset) = search_bytes_offset(line, query, ignore_case, locale_lowercase, false)
    else {
        return 0;
    };
    byte_offset
}

fn accept_search_line(
    state: &EditorState,
    search: &ReverseSearchState,
    keep_failed_query: bool,
) -> Vec<u8> {
    if search.match_line.is_some()
        || search.retain_match_on_failure
        || (keep_failed_query && search.retain_failed_query_line)
    {
        // A match on the buffer's own entry preserves its live edit, which
        // may be newer than the line stored in the search result. A failed
        // query extension likewise keeps the displayed line on accept;
        // Ctrl-G still restores the original line.
        state.buffer.as_bytes().to_vec()
    } else {
        search.original_line.clone()
    }
}

fn save_incremental_search(state: &mut EditorState, search: &ReverseSearchState) {
    state.search.last_incremental_search = (!search.query.is_empty()).then(|| search.query.clone());
    state.search.last_incremental_search_direction = Some(search.direction);
}

/// ASCII control bytes GNU echoes into an execute-named-command query as
/// caret notation. Command and editing controls are handled before the query
/// text path; the measured set here is echoed rather than interpreted.
fn named_query_control_is_text(ch: char) -> bool {
    matches!(
        ch as u32,
        0x01 | 0x02
            | 0x04
            | 0x05
            | 0x06
            | 0x0b
            | 0x0c
            | 0x0e
            | 0x0f
            | 0x10
            | 0x12
            | 0x14
            | 0x18
            | 0x19
            | 0x1b
            | 0x1d
            | 0x1e
            | 0x1f
    )
}

/// Append query bytes, buffering an incomplete UTF-8 sequence until its
/// remaining bytes arrive. Editing controls are handled before this path;
/// only the measured caret-echoed controls stay in the command name. A quoted
/// byte bypasses that filter and is appended literally.
fn append_named_query_bytes(
    pending: &mut PendingNamedCommand,
    bytes: &[u8],
    keep_controls: bool,
) -> bool {
    fn push_chars(query: &mut String, text: &str, keep_controls: bool) -> bool {
        let before = query.len();
        query
            .extend(text.chars().filter(|ch| {
                keep_controls || !ch.is_control() || named_query_control_is_text(*ch)
            }));
        query.len() != before
    }

    let mut raw = std::mem::take(&mut pending.pending_bytes);
    raw.extend_from_slice(bytes);
    let mut consumed = 0;
    let mut changed = false;
    while consumed < raw.len() {
        let slice = &raw[consumed..];
        match std::str::from_utf8(slice) {
            Ok(text) => {
                changed |= push_chars(&mut pending.query, text, keep_controls);
                consumed = raw.len();
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if valid > 0 {
                    let text = std::str::from_utf8(&slice[..valid]).expect("valid UTF-8 prefix");
                    changed |= push_chars(&mut pending.query, text, keep_controls);
                    consumed += valid;
                }
                match error.error_len() {
                    Some(len) => {
                        consumed += len;
                        // Keep the byte in the name as a replacement scalar:
                        // GNU does not silently drop it and dispatch a name
                        // that would have been valid without it.
                        changed |= push_chars(&mut pending.query, "\u{fffd}", keep_controls);
                    }
                    None => break,
                }
            }
        }
    }
    pending.pending_bytes.extend_from_slice(&raw[consumed..]);
    changed
}

/// Complete a query ending in an unfinished UTF-8 sequence before dispatch.
fn flush_named_query(pending: &mut PendingNamedCommand) {
    if !pending.pending_bytes.is_empty() {
        pending.pending_bytes.clear();
        pending.query.push('\u{fffd}');
    }
}

#[cfg(test)]
mod named_query_tests {
    use super::{append_named_query_bytes, flush_named_query};
    use crate::state::PendingNamedCommand;

    #[test]
    fn echoed_controls_stay_in_query_and_command_controls_are_dropped() {
        let mut pending = PendingNamedCommand::default();
        assert!(append_named_query_bytes(
            &mut pending,
            b"\x1b\x01\x06\x0f\x12\x19\x1d\x1e\x1fa",
            false
        ));
        assert_eq!(
            pending.query,
            "\u{1b}\u{1}\u{6}\u{f}\u{12}\u{19}\u{1d}\u{1e}\u{1f}a"
        );
        assert!(append_named_query_bytes(&mut pending, b"\x07b\x7fc", false));
        assert_eq!(
            pending.query,
            "\u{1b}\u{1}\u{6}\u{f}\u{12}\u{19}\u{1d}\u{1e}\u{1f}abc"
        );
    }

    #[test]
    fn invalid_query_bytes_survive_as_replacement_scalars() {
        let mut pending = PendingNamedCommand::default();
        assert!(append_named_query_bytes(
            &mut pending,
            b"\xffaccept-line",
            false
        ));
        assert_eq!(pending.query, "\u{fffd}accept-line");
        assert!(!append_named_query_bytes(&mut pending, &[0xe2], false));
        flush_named_query(&mut pending);
        assert_eq!(pending.query, "\u{fffd}accept-line\u{fffd}");
        assert!(pending.pending_bytes.is_empty());
    }

    #[test]
    fn quoted_controls_stay_in_query_and_fragmented_utf8_still_joins() {
        let mut pending = PendingNamedCommand::default();
        assert!(append_named_query_bytes(
            &mut pending,
            b"\x16\x15\x07\x0d\x7f",
            true
        ));
        assert_eq!(pending.query, "\u{16}\u{15}\u{7}\u{d}\u{7f}");

        assert!(!append_named_query_bytes(&mut pending, &[0xe2], true));
        assert!(!append_named_query_bytes(&mut pending, &[0x80], true));
        assert!(append_named_query_bytes(&mut pending, &[0x8b], true));
        assert_eq!(pending.query, "\u{16}\u{15}\u{7}\u{d}\u{7f}\u{200b}");
        assert!(pending.pending_bytes.is_empty());
    }

    #[test]
    fn fragmented_utf8_query_bytes_form_one_character() {
        let mut pending = PendingNamedCommand::default();
        assert!(!append_named_query_bytes(&mut pending, &[0xe2], false));
        assert!(!append_named_query_bytes(&mut pending, &[0x80], false));
        assert!(append_named_query_bytes(&mut pending, &[0x8b], false));
        assert_eq!(pending.query, "\u{200b}");
        assert!(pending.pending_bytes.is_empty());

        assert!(append_named_query_bytes(&mut pending, b"ab", false));
        assert_eq!(pending.query, "\u{200b}ab");
    }
}

#[cfg(test)]
mod search_tests {
    use super::isearch_terminate_point;
    use crate::locale::LocaleLowercase;

    #[test]
    fn incremental_search_cursor_uses_original_byte_match_start() {
        let Some(locale) = LocaleLowercase::new("C.UTF-8") else {
            return;
        };
        assert_eq!(
            isearch_terminate_point(
                "Ä x ÄLPHA".as_bytes(),
                "älp".as_bytes(),
                true,
                Some(&locale),
            ),
            5
        );
    }
}
