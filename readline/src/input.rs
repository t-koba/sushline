use crate::buffer::LineBuffer;
use crate::editor::{Editor, EditorOutcome, ReadlineError, ReadlineResult};
use crate::hooks::Hooks;
use crate::keymap::{EditCommand, KeyBinding, KeyMapName};
use crate::state::*;
use crate::terminal::TerminalIo;
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
        #[cfg(all(unix, not(test)))]
        unsafe {
            libc::raise(signal);
        }
        #[cfg(unix)]
        if signal == libc::SIGINT {
            self.echo_signal_interrupt(state)?;
            return Ok(Some(ReadlineResult::Interrupted));
        }
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
            return Ok(self.handle_bracketed_paste_input(state, bytes));
        }
        if let Some(outcome) = self.handle_pending_vi_mark(state, bytes)? {
            return Ok(outcome);
        }
        if let Some(outcome) = self.handle_pending_vi_register(state, bytes)? {
            return Ok(outcome);
        }
        if let Some(outcome) = self.handle_pending_char_search(state, bytes)? {
            return Ok(outcome);
        }
        if state.input.pending_replace {
            return Ok(self.handle_replace_input(state, bytes));
        }
        if self.handle_multibyte_insert(state, bytes) {
            return Ok(EditorOutcome::Continue);
        }
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
    ) -> EditorOutcome {
        let mut combined = std::mem::take(&mut state.paste.bracketed_paste_pending);
        combined.extend_from_slice(bytes);
        let end_seq = b"\x1b[201~";
        let paste_ends = combined
            .windows(end_seq.len())
            .any(|window| window == end_seq);
        let payload_len = combined
            .windows(end_seq.len())
            .position(|window| window == end_seq)
            .unwrap_or_else(|| {
                combined
                    .len()
                    .saturating_sub(end_seq.len().saturating_sub(1))
            });
        let payload = combined[..payload_len].to_vec();
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
        }
        EditorOutcome::Continue
    }

    fn handle_replace_input(&mut self, state: &mut EditorState, bytes: &[u8]) -> EditorOutcome {
        state.input.pending_replace = false;
        let replacement = replacement_unit(bytes);
        if !replacement.is_empty() {
            state.record_undo();
            state.buffer.replace_char_at_point_bytes(&replacement);
            if let Some(mut change) = state.vi.vi_insert_change.take() {
                change.extend_from_slice(bytes);
                state.vi.last_vi_change = Some(change);
            }
            state.after_non_kill_command();
        }
        EditorOutcome::Continue
    }

    fn insert_literal(&mut self, state: &mut EditorState, bytes: &[u8], record_macro: bool) {
        let count = state.numeric_arg.take().unwrap_or(1).unsigned_abs().max(1);
        if !state.undo.last_undo_was_insert {
            state.record_undo();
        }
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
        if record_macro
            && !state.macro_state.replaying_macro
            && let Some(macro_bytes) = state.macro_state.keyboard_macro.as_mut()
        {
            macro_bytes.extend_from_slice(bytes);
        }
        state.record_vi_insert_bytes(bytes);
        state.after_self_insert();
    }

    fn handle_multibyte_insert(&mut self, state: &mut EditorState, bytes: &[u8]) -> bool {
        if bytes.len() <= 1 || matches!(self.keymap.current(), KeyMapName::ViCommand) {
            return false;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return false;
        };
        if text.is_ascii() {
            return false;
        }
        if !state.undo.last_undo_was_insert {
            state.record_undo();
        }
        for ch in text.chars().filter(|ch| !ch.is_control()) {
            state.buffer.insert_char(ch);
        }
        state.record_vi_insert_bytes(bytes);
        state.after_self_insert();
        true
    }

    fn handle_key_dispatch(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        state.input.pending_key.extend_from_slice(bytes);
        let pending = std::mem::take(&mut state.input.pending_key);
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
            let outcome = self.apply_binding(state, binding, &pending[..len], hooks)?;
            if !matches!(outcome, EditorOutcome::Continue) {
                return Ok(outcome);
            }
            if len < pending.len() {
                return self.handle_bytes(state, &pending[len..], hooks);
            }
            return Ok(EditorOutcome::Continue);
        }

        self.handle_unbound(state, &pending)
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
        if self.variable_is_on("convert-meta") {
            return Some(vec![0x1b, stripped]);
        }
        if !self.variable_is_on("input-meta") && !self.variable_is_on("meta-flag") {
            return Some(vec![stripped]);
        }
        None
    }

    pub(super) fn handle_pending_vi_mark(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
    ) -> Result<Option<EditorOutcome>, ReadlineError> {
        let Some(action) = state.vi.pending_vi_mark.take() else {
            return Ok(None);
        };

        let Ok(text) = std::str::from_utf8(bytes) else {
            self.ding()?;
            return Ok(Some(EditorOutcome::Continue));
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
        } else {
            self.ding()?;
        }

        Ok(Some(EditorOutcome::Continue))
    }

    pub(super) fn handle_pending_vi_register(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
    ) -> Result<Option<EditorOutcome>, ReadlineError> {
        if !state.vi.pending_vi_register {
            return Ok(None);
        }
        state.vi.pending_vi_register = false;

        let Ok(text) = std::str::from_utf8(bytes) else {
            self.ding()?;
            return Ok(Some(EditorOutcome::Continue));
        };

        if let Some(ch) = text.chars().find(|ch| !ch.is_control()) {
            state.vi.active_vi_register = Some(ch);
            state.after_non_kill_command();
        } else {
            self.ding()?;
        }

        Ok(Some(EditorOutcome::Continue))
    }

    pub(super) fn handle_pending_char_search(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
    ) -> Result<Option<EditorOutcome>, ReadlineError> {
        let Some(search) = state.vi.pending_char_search.take() else {
            return Ok(None);
        };
        let op_start = state.vi.pending_char_search_operator.take();

        if let Some(ch) = char_search_key(bytes) {
            if self.apply_char_search(state, search, ch)? {
                self.finish_vi_motion_operator(state, op_start, bytes, true);
                state.vi.last_char_search = Some((search, ch));
                state.after_non_kill_command();
            } else {
                state.cancel_pending_command();
            }
        } else {
            state.cancel_pending_command();
            self.ding()?;
        }

        Ok(Some(EditorOutcome::Continue))
    }

    pub(super) fn handle_unbound(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
    ) -> Result<EditorOutcome, ReadlineError> {
        if state.input.pending_replace {
            state.input.pending_replace = false;
            let replacement = replacement_unit(bytes);
            if !replacement.is_empty() {
                state.record_undo();
                state.buffer.replace_char_at_point_bytes(&replacement);
                state.after_non_kill_command();
            }
            return Ok(EditorOutcome::Continue);
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
        } else if bytes.iter().any(|byte| *byte >= 0x80) {
            let insertable = bytes
                .iter()
                .copied()
                .filter(|byte| *byte >= 0x80)
                .collect::<Vec<_>>();
            if !insertable.is_empty() {
                self.insert_literal(state, &insertable, true);
            }
        }
        Ok(EditorOutcome::Continue)
    }

    pub(super) fn handle_reverse_search(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        let Some(mut search) = state.search.reverse_search.take() else {
            return Ok(EditorOutcome::Continue);
        };

        let outcome = match bytes {
            &[0x1b] if self.is_isearch_terminator(bytes) => {
                let accepted = accept_search_line(&search);
                state.buffer = LineBuffer::from_bytes(accepted);
                save_last_search(state, &search);
                state.after_non_kill_command();
                EditorOutcome::Continue
            }
            b"\r" | b"\n" => {
                let accepted = accept_search_line(&search);
                state.buffer = LineBuffer::from_bytes(accepted.clone());
                save_last_search(state, &search);
                state.after_non_kill_command();
                EditorOutcome::Accepted(accepted)
            }
            &[0x07] => {
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
                    self.variable_is_on("search-ignore-case"),
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
                    self.variable_is_on("search-ignore-case"),
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
        let input = if let Ok(text) = std::str::from_utf8(bytes) {
            text.bytes()
                .filter(|byte| !byte.is_ascii_control())
                .collect::<Vec<_>>()
        } else {
            bytes
                .iter()
                .copied()
                .filter(|byte| *byte >= 0x80)
                .collect::<Vec<_>>()
        };
        if !input.is_empty() {
            search.query.extend(input);
            search.match_index = None;
            update_reverse_search_match(
                &mut search,
                &self.history,
                false,
                self.variable_is_on("search-ignore-case"),
            );
            self.apply_search_match(state, &search);
        }
        state.search.reverse_search = Some(search);
        Ok(EditorOutcome::Continue)
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
    ) -> EditorOutcome {
        let Some(mut search) = state.search.non_incremental_search.take() else {
            return EditorOutcome::Continue;
        };
        match bytes {
            b"\r" | b"\n" => {
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
                        self.variable_is_on("search-ignore-case"),
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
                if let Ok(text) = std::str::from_utf8(bytes) {
                    search
                        .query
                        .extend(text.bytes().filter(|byte| !byte.is_ascii_control()));
                } else {
                    search
                        .query
                        .extend(bytes.iter().copied().filter(|byte| *byte >= 0x80));
                }
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
