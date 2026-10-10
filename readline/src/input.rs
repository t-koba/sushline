use crate::buffer::LineBuffer;
use crate::editor::{Editor, EditorOutcome, ReadlineError, ReadlineResult};
use crate::hooks::Hooks;
use crate::keymap::{EditCommand, KeyBinding, KeyMapName};
use crate::state::*;
use crate::terminal::TerminalIo;
use crate::variables::BoolVariable;
use history::HistoryDirection;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn handle_terminal_signal(
        &mut self,
        state: &mut EditorState,
        signal: i32,
    ) -> Result<Option<ReadlineResult>, ReadlineError> {
        self.terminal.restore_mode()?;
        #[cfg(unix)]
        if signal == libc::SIGINT {
            self.echo_signal_interrupt(state)?;
            return Ok(Some(ReadlineResult::Interrupted));
        }
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

    pub(super) fn handle_bytes(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        if state.input.prefix_meta {
            return self.handle_meta_prefix(state, bytes, hooks);
        }
        if state.input.skipping_csi {
            return Ok(self.handle_csi_skip(state, bytes));
        }
        if state.input.quoted_insert {
            return Ok(self.handle_quoted_insert(state, bytes));
        }
        if let Some(translated) = self.translate_meta_input(bytes) {
            return self.handle_bytes(state, &translated, hooks);
        }
        if state.input.named_command.is_some() {
            return self.handle_named_command(state, bytes, hooks);
        }
        if state.paste.bracketed_paste {
            return self.handle_bracketed_paste_input(state, bytes, hooks);
        }
        if let Some(outcome) = self.handle_pending_vi_mark(state, bytes, hooks)? {
            return Ok(outcome);
        }
        if let Some(outcome) = self.handle_pending_vi_register(state, bytes, hooks)? {
            return Ok(outcome);
        }
        if let Some(outcome) = self.handle_pending_char_search(state, bytes, hooks)? {
            return Ok(outcome);
        }
        if state.input.pending_replace {
            return self.handle_replace_input(state, bytes, hooks);
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

    fn handle_meta_prefix(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        state.input.prefix_meta = false;
        let mut prefixed = Vec::with_capacity(bytes.len() + 1);
        prefixed.push(0x1b);
        prefixed.extend_from_slice(bytes);
        self.handle_bytes(state, &prefixed, hooks)
    }

    fn handle_csi_skip(&mut self, state: &mut EditorState, bytes: &[u8]) -> EditorOutcome {
        state.consume_csi_bytes(bytes);
        EditorOutcome::Continue
    }

    fn handle_quoted_insert(&mut self, state: &mut EditorState, bytes: &[u8]) -> EditorOutcome {
        state.input.quoted_insert = false;
        self.insert_literal(state, bytes, true);
        EditorOutcome::Continue
    }

    fn handle_bracketed_paste_input(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        let mut combined = std::mem::take(&mut state.paste.bracketed_paste_pending);
        combined.extend_from_slice(bytes);
        let end_seq = b"\x1b[201~";
        let end_pos = combined
            .windows(end_seq.len())
            .position(|window| window == end_seq);
        let paste_ends = end_pos.is_some();
        let payload_len = end_pos.unwrap_or_else(|| {
            combined
                .len()
                .saturating_sub(end_seq.len().saturating_sub(1))
        });
        let payload = combined[..payload_len].to_vec();
        let remainder = end_pos
            .map(|pos| combined[pos + end_seq.len()..].to_vec())
            .unwrap_or_default();
        if payload_len < combined.len() && !paste_ends {
            state
                .paste
                .bracketed_paste_pending
                .extend_from_slice(&combined[payload_len..]);
        }
        let start = *state
            .paste
            .bracketed_paste_start
            .get_or_insert_with(|| state.buffer.point());
        if !payload.is_empty() {
            self.insert_literal(state, &payload, false);
            state.mark = Some(start);
        }
        if paste_ends {
            state.paste.bracketed_paste = false;
            state.paste.bracketed_paste_start = None;
            state.paste.bracketed_paste_pending.clear();
            if !remainder.is_empty() {
                return self.handle_bytes(state, &remainder, hooks);
            }
        }
        Ok(EditorOutcome::Continue)
    }

    fn handle_replace_input(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        // Fragmentation-invariant: only the first unit is the replacement;
        // trailing bytes are separate input like split reads.
        if bytes.is_empty() {
            state.input.pending_replace = false;
            state.consume_numeric_arg_unless_prefix();
            return Ok(EditorOutcome::Continue);
        }
        let (first, rest) = split_first_unit(bytes);
        state.input.pending_replace = false;
        state.consume_numeric_arg_unless_prefix();
        let replacement = replacement_unit(first);
        if !replacement.is_empty() {
            let point = state.buffer.point();
            state.record_undo();
            state.buffer.replace_char_at_point_bytes(&replacement);
            state.buffer.set_point(point);
            if let Some(mut change) = state.vi.vi_insert_change.take() {
                change.extend_from_slice(first);
                state.vi.last_vi_change = Some(change);
            }
            state.after_non_kill_command();
        }
        if rest.is_empty() {
            return Ok(EditorOutcome::Continue);
        }
        self.handle_bytes(state, rest, hooks)
    }

    fn insert_literal(&mut self, state: &mut EditorState, bytes: &[u8], record_macro: bool) {
        let count = repeat_count(state.numeric_arg.take());
        state.record_insert_undo(count == 1 && bytes.len() == 1);
        for _ in 0..count {
            if state.overwrite_mode {
                for byte in bytes {
                    let point = state.buffer.point();
                    if point < state.buffer.len_chars() {
                        let _ = state.buffer.delete_char_bytes();
                    }
                    state.buffer.replace_range_bytes(point, point, &[*byte]);
                }
            } else {
                state.buffer.insert_bytes(bytes);
            }
        }
        if record_macro {
            state.record_macro_insert_bytes(bytes);
        }
        state.record_vi_insert_bytes(bytes);
        state.after_self_insert();
    }

    fn handle_numeric_argument_continuation(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<Option<EditorOutcome>, ReadlineError> {
        if state.numeric_arg.is_none() {
            return Ok(None);
        }
        // Fragmentation-invariant: a batched chunk may carry a digit run
        // plus the terminating sequence (e.g. `1\x1bl` after `M--`).
        // Consume the leading run digit-by-digit like the one-byte path,
        // then dispatch any remainder so batched and split reads agree.
        let run = bytes
            .iter()
            .take_while(|byte| matches!(byte, b'0'..=b'9' | b'-'))
            .count();
        if run == 0 {
            return Ok(None);
        }
        for index in 0..run {
            update_numeric_argument(state, &bytes[index..index + 1]);
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
        // insert before undo grouping; exact and custom bindings still fire
        // at once, ahead of keymap-prefix buffering.
        if is_incomplete_utf8_prefix(&pending)
            && matches!(
                self.keymap.lookup(self.keymap.current(), &pending[..1]),
                Some(KeyBinding::Command(EditCommand::SelfInsert))
            )
        {
            state.input.pending_key = pending;
            return Ok(EditorOutcome::Continue);
        }

        if let Some(binding) = self.keymap.lookup(self.keymap.current(), &pending).cloned() {
            return self.apply_binding(state, binding, &pending, hooks);
        }

        if self.keymap.has_prefix(self.keymap.current(), &pending) {
            state.input.pending_key = pending;
            return Ok(EditorOutcome::Continue);
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
            let outcome = self.apply_binding(state, binding, &pending[..use_len], hooks)?;
            if !matches!(outcome, EditorOutcome::Continue) {
                return Ok(outcome);
            }
            if use_len < pending.len() {
                return self.handle_bytes(state, &pending[use_len..], hooks);
            }
            return Ok(EditorOutcome::Continue);
        }

        self.handle_unbound(state, &pending, hooks)
    }

    pub(super) fn replay_vi_change(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        let original_change = bytes.to_vec();
        if let Ok(text) = std::str::from_utf8(bytes) {
            for ch in text.chars() {
                let mut buf = [0_u8; 4];
                let outcome =
                    self.handle_bytes(state, ch.encode_utf8(&mut buf).as_bytes(), hooks)?;
                if !matches!(outcome, EditorOutcome::Continue) {
                    state.vi.last_vi_change = Some(original_change);
                    return Ok(outcome);
                }
            }
        } else {
            for byte in bytes {
                let outcome = self.handle_bytes(state, &[*byte], hooks)?;
                if !matches!(outcome, EditorOutcome::Continue) {
                    state.vi.last_vi_change = Some(original_change);
                    return Ok(outcome);
                }
            }
        }
        state.vi.last_vi_change = Some(original_change);
        Ok(EditorOutcome::Continue)
    }

    pub(super) fn translate_meta_input(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        let [byte] = bytes else {
            return None;
        };
        if byte & 0x80 == 0 {
            return None;
        }

        let stripped = byte & 0x7f;
        if self.flag(BoolVariable::ConvertMeta) {
            return Some(vec![0x1b, stripped]);
        }
        if !self.flag(BoolVariable::InputMeta) && !self.flag(BoolVariable::MetaFlag) {
            return Some(vec![stripped]);
        }
        None
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

        // Fragmentation-invariant: only the first unit answers the pending
        // mark; trailing bytes are separate input like split reads.
        let (first, rest) = split_first_unit(bytes);
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

        if let Some(ch) = text.chars().find(|ch| !ch.is_control()) {
            match action {
                ViMarkAction::Set => {
                    state.vi.vi_marks.insert(ch, state.buffer.point());
                }
                ViMarkAction::Goto => {
                    let op_start = state.vi.pending_mark_operator.take();
                    if let Some(point) = state.vi.vi_marks.get(&ch).copied() {
                        state.buffer.set_point(point);
                        self.finish_vi_motion_operator(state, op_start, bytes, false);
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

        // Fragmentation-invariant: only the first unit selects the register;
        // trailing bytes are separate input like split reads.
        let (first, rest) = split_first_unit(bytes);
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
        let Some(search) = state.vi.pending_char_search.take() else {
            return Ok(None);
        };
        let op_start = state.vi.pending_char_search_operator.take();

        // Fragmentation-invariant: only the first unit is the search key;
        // trailing bytes are separate input like split reads.
        let (first, rest) = split_first_unit(bytes);
        if let Some(ch) = char_search_key(first) {
            if self.apply_char_search(state, search, ch)? {
                self.finish_vi_motion_operator(state, op_start, first, true);
                state.vi.last_char_search = Some((search, ch));
                state.after_non_kill_command();
                state.consume_numeric_arg_unless_prefix();
            } else {
                state.cancel_pending_command();
            }
        } else {
            state.cancel_pending_command();
            self.ding()?;
        }

        if !rest.is_empty() {
            let remainder_outcome = self.handle_bytes(state, rest, hooks)?;
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
        if state.input.pending_replace {
            return self.handle_replace_input(state, bytes, hooks);
        }

        if matches!(self.keymap.current(), KeyMapName::ViCommand) {
            state.cancel_pending_command();
            self.ding()?;
            return Ok(EditorOutcome::Continue);
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

    fn handle_reverse_search_single(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        // Search-scoped quoted-insert: a pending quote consumes the next
        // byte literally into the query, even controls/terminators.
        if state.search.quoted_pending {
            state.search.quoted_pending = false;
            let Some(mut search) = state.search.reverse_search.take() else {
                return Ok(EditorOutcome::Continue);
            };
            search.query.extend_from_slice(bytes);
            search.match_index = None;
            update_reverse_search_match(
                &mut search,
                &self.history,
                false,
                self.flag(BoolVariable::SearchIgnoreCase),
            );
            self.apply_search_match(state, &search);
            state.search.reverse_search = Some(search);
            return Ok(EditorOutcome::Continue);
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
        let Some(mut search) = state.search.reverse_search.take() else {
            return Ok(EditorOutcome::Continue);
        };

        let outcome = match bytes {
            bytes if self.is_isearch_terminator(bytes) => {
                state.search.quoted_pending = false;
                let accepted = accept_search_line(&search);
                // GNU point after terminate-without-execute (patch 0 Bash 5.3
                // PTY oracle): empty query restores the original point;
                // a match leaves point at the match start (`alpha` + C-J +
                // `!` yields `!alpha two`, `two` + C-J + `!` yields
                // `alpha !two`); a non-empty query with no match leaves
                // point at 0 (`zzz` + C-J + `!` on `draft` yields `!draft`).
                let mut buffer = LineBuffer::from_bytes(accepted);
                if search.query.is_empty() {
                    buffer.set_point(search.original_point.min(buffer.len_chars()));
                } else {
                    buffer.set_point(isearch_terminate_point(
                        buffer.as_bytes(),
                        &search.query,
                        self.flag(BoolVariable::SearchIgnoreCase),
                    ));
                }
                state.buffer = buffer;
                save_last_search(state, &search);
                state.after_non_kill_command();
                EditorOutcome::Continue
            }
            b"\r" | b"\n" => {
                state.search.quoted_pending = false;
                let accepted = accept_search_line(&search);
                state.buffer = LineBuffer::from_bytes(accepted.clone());
                save_last_search(state, &search);
                state.after_non_kill_command();
                EditorOutcome::Accepted(accepted)
            }
            &[0x07] => {
                state.search.quoted_pending = false;
                state.buffer = LineBuffer::from_bytes(search.original_line.clone());
                state.after_non_kill_command();
                EditorOutcome::Continue
            }
            &[0x12] | &[0x13] => {
                search.direction = if bytes == [0x12] {
                    SearchDirection::Backward
                } else {
                    SearchDirection::Forward
                };
                update_reverse_search_match(
                    &mut search,
                    &self.history,
                    true,
                    self.flag(BoolVariable::SearchIgnoreCase),
                );
                self.apply_search_match(state, &search);
                state.search.reverse_search = Some(search);
                EditorOutcome::Continue
            }
            &[0x7f] => {
                search.query.pop();
                search.match_index = None;
                update_reverse_search_match(
                    &mut search,
                    &self.history,
                    false,
                    self.flag(BoolVariable::SearchIgnoreCase),
                );
                if !self.apply_search_match(state, &search) {
                    state.buffer = LineBuffer::from_bytes(search.original_line.clone());
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
        let command_binding = self
            .keymap
            .lookup(self.keymap.current(), bytes)
            .filter(|binding| !matches!(binding, KeyBinding::Command(EditCommand::SelfInsert)))
            .cloned();
        if command_binding.is_some() || self.keymap.has_prefix(self.keymap.current(), bytes) {
            let accepted = accept_search_line(&search);
            state.buffer = LineBuffer::from_bytes(accepted);
            save_last_search(state, &search);
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
            update_reverse_search_match(
                &mut search,
                &self.history,
                false,
                self.flag(BoolVariable::SearchIgnoreCase),
            );
            self.apply_search_match(state, &search);
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

    fn apply_search_match(&mut self, state: &mut EditorState, search: &ReverseSearchState) -> bool {
        if let Some(line) = &search.match_line {
            self.replace_from_history(state, line);
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
                    let outcome = self.handle_non_incremental_single(state, &single);
                    if !matches!(outcome, EditorOutcome::Continue) {
                        return Ok(outcome);
                    }
                    pos += 1;
                    continue;
                }
                let single = [bytes[pos]];
                let outcome = self.handle_non_incremental_single(state, &single);
                if !matches!(outcome, EditorOutcome::Continue) {
                    return Ok(outcome);
                }
                pos += 1;
            }
            return Ok(EditorOutcome::Continue);
        }
        Ok(self.handle_non_incremental_single(state, bytes))
    }

    fn handle_non_incremental_single(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
    ) -> EditorOutcome {
        // Search-scoped quote: consume the next byte literally.
        if state.search.quoted_pending {
            state.search.quoted_pending = false;
            let Some(mut search) = state.search.non_incremental_search.take() else {
                return EditorOutcome::Continue;
            };
            search.query.extend_from_slice(bytes);
            state.search.non_incremental_search = Some(search);
            return EditorOutcome::Continue;
        }
        // Non-incremental searches quote only ^V/^Q (CHANGES 8.3 2.i:
        // "in the former case" = incremental allows any binding).
        if matches!(bytes, [0x16] | [0x11]) {
            let Some(search) = state.search.non_incremental_search.take() else {
                return EditorOutcome::Continue;
            };
            state.search.non_incremental_search = Some(search);
            state.search.quoted_pending = true;
            return EditorOutcome::Continue;
        }
        let Some(mut search) = state.search.non_incremental_search.take() else {
            return EditorOutcome::Continue;
        };
        match bytes {
            b"\r" | b"\n" => {
                state.search.quoted_pending = false;
                let query = if search.query.is_empty() {
                    state.search.last_search.clone().unwrap_or_default()
                } else {
                    search.query.clone()
                };
                if !query.is_empty() {
                    let direction = match search.direction {
                        SearchDirection::Backward => HistoryDirection::Previous,
                        SearchDirection::Forward => HistoryDirection::Next,
                    };
                    if let Some(found) = self.history.history_search_bytes_with_case(
                        &query,
                        direction,
                        self.flag(BoolVariable::SearchIgnoreCase),
                    ) {
                        self.replace_from_history(state, &found.line_bytes);
                    } else {
                        self.history.set_pos(search.original_history_pos);
                    }
                    state.search.last_search = Some(query);
                    state.search.last_search_direction = Some(search.direction);
                }
                state.after_non_kill_command();
                EditorOutcome::Continue
            }
            &[0x07] | &[0x1b] => {
                state.search.quoted_pending = false;
                state.buffer = LineBuffer::from_bytes(search.original_line.clone());
                self.history.set_pos(search.original_history_pos);
                state.after_non_kill_command();
                EditorOutcome::Continue
            }
            &[0x7f] => {
                search.query.pop();
                state.search.non_incremental_search = Some(search);
                EditorOutcome::Continue
            }
            _ => {
                // Fragmentation-invariant: keep ASCII non-controls even for
                // invalid chunks, matching split reads.
                search.query.extend(
                    bytes
                        .iter()
                        .copied()
                        .filter(|byte| !byte.is_ascii_control()),
                );
                state.search.non_incremental_search = Some(search);
                EditorOutcome::Continue
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
                if remaining.len() == 1
                    || matches!(remaining[0], b'\r' | b'\n' | 0x07 | 0x1b | 0x7f)
                {
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
        match bytes {
            b"\r" | b"\n" => {
                let command = state.input.named_command.take().unwrap_or_default();
                self.apply_named_command(state, command.trim(), b"", hooks)
            }
            &[0x07] | &[0x1b] => {
                state.input.named_command = None;
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            &[0x7f] => {
                if let Some(command) = state.input.named_command.as_mut() {
                    command.pop();
                }
                Ok(EditorOutcome::Continue)
            }
            _ => {
                if state.input.named_command.is_none() {
                    return self.handle_bytes(state, bytes, hooks);
                }
                if let Ok(text) = std::str::from_utf8(bytes)
                    && let Some(command) = state.input.named_command.as_mut()
                {
                    command.extend(text.chars().filter(|ch| !ch.is_control()));
                }
                Ok(EditorOutcome::Continue)
            }
        }
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

fn isearch_terminate_point(line: &[u8], query: &[u8], ignore_case: bool) -> usize {
    if query.is_empty() || query.len() > line.len() {
        return 0;
    }
    if ignore_case {
        let needle: Vec<u8> = query.iter().map(|byte| byte.to_ascii_lowercase()).collect();
        line.windows(needle.len())
            .position(|window| {
                window
                    .iter()
                    .map(|byte| byte.to_ascii_lowercase())
                    .eq(needle.iter().copied())
            })
            .unwrap_or(0)
    } else {
        line.windows(query.len())
            .position(|window| window == query)
            .unwrap_or(0)
    }
}

fn accept_search_line(search: &ReverseSearchState) -> Vec<u8> {
    search
        .match_line
        .clone()
        .unwrap_or_else(|| search.original_line.clone())
}

fn save_last_search(state: &mut EditorState, search: &ReverseSearchState) {
    state.search.last_search = (!search.query.is_empty()).then(|| search.query.clone());
    state.search.last_search_direction = Some(search.direction);
}
