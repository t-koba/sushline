use super::*;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_vi_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        key: &[u8],
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            EditCommand::ViAppendEol => {
                state.buffer.move_end();
                self.keymap.set_current(KeyMapName::ViInsert);
                state.begin_vi_insert_change(key);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ViAppendMode => {
                state.buffer.move_forward();
                self.keymap.set_current(KeyMapName::ViInsert);
                state.begin_vi_insert_change(key);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ViInsertBeg => {
                state.buffer.move_beginning();
                self.keymap.set_current(KeyMapName::ViInsert);
                state.begin_vi_insert_change(key);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ViInsertionMode => {
                self.keymap.set_current(KeyMapName::ViInsert);
                state.begin_vi_insert_change(key);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ViMovementMode => {
                if state.buffer.point() > 0 {
                    state.buffer.move_backward();
                }
                state.overwrite_mode = false;
                state.finish_vi_insert_change(key);
                self.keymap.set_current(KeyMapName::ViCommand);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            _ => unreachable!("command group mismatch"),
        }
    }
}

#[derive(Clone, Copy)]
enum ViCommandGroup {
    Motion,
    Operator,
    Edit,
    Search,
    Mode,
}

fn named_vi_command_group(command: &str) -> Option<ViCommandGroup> {
    match command {
        "character-search" | "character-search-backward" | "vi-char-search" => {
            Some(ViCommandGroup::Search)
        }
        "edit-and-execute-command" | "vi-edit-and-execute-command" => Some(ViCommandGroup::Mode),
        "vi-arg-digit" => Some(ViCommandGroup::Search),
        "vi-bWord" | "vi-backward-bigword" => Some(ViCommandGroup::Motion),
        "vi-back-to-indent" | "vi-first-print" => Some(ViCommandGroup::Motion),
        "vi-backward-word" | "vi-bword" | "vi-prev-word" => Some(ViCommandGroup::Motion),
        "vi-change-case" => Some(ViCommandGroup::Operator),
        "vi-change-char" | "vi-replace" => Some(ViCommandGroup::Operator),
        "vi-change-to" => Some(ViCommandGroup::Operator),
        "vi-column" => Some(ViCommandGroup::Motion),
        "vi-delete" => Some(ViCommandGroup::Operator),
        "vi-delete-to" => Some(ViCommandGroup::Operator),
        "vi-eWord" | "vi-end-bigword" => Some(ViCommandGroup::Motion),
        "vi-end-word" | "vi-eword" => Some(ViCommandGroup::Motion),
        "vi-eof-maybe" => Some(ViCommandGroup::Mode),
        "vi-editing-mode" => Some(ViCommandGroup::Mode),
        "vi-fWord" | "vi-forward-bigword" => Some(ViCommandGroup::Motion),
        "vi-forward-word" | "vi-fword" | "vi-next-word" => Some(ViCommandGroup::Motion),
        "vi-goto-mark" => Some(ViCommandGroup::Mode),
        "vi-match" => Some(ViCommandGroup::Mode),
        "vi-overstrike-delete" | "vi-rubout" => Some(ViCommandGroup::Edit),
        "vi-put" => Some(ViCommandGroup::Edit),
        "vi-redo" => Some(ViCommandGroup::Edit),
        "vi-search" => Some(ViCommandGroup::Search),
        "vi-search-again" => Some(ViCommandGroup::Search),
        "vi-set-register" => Some(ViCommandGroup::Mode),
        "vi-set-mark" => Some(ViCommandGroup::Mode),
        "vi-subst" => Some(ViCommandGroup::Operator),
        "vi-undo" => Some(ViCommandGroup::Edit),
        "vi-yank-pop" => Some(ViCommandGroup::Edit),
        "vi-yank-to" => Some(ViCommandGroup::Operator),
        _ => None,
    }
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_vi_named_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match named_vi_command_group(command).expect("named vi command group") {
            ViCommandGroup::Motion => self.apply_vi_motion_command(state, command, key, hooks),
            ViCommandGroup::Operator => self.apply_vi_operator_command(state, command, key),
            ViCommandGroup::Edit => self.apply_vi_edit_command(state, command, key, hooks),
            ViCommandGroup::Search => self.apply_vi_search_command(state, command, key),
            ViCommandGroup::Mode => self.apply_vi_mode_command(state, command, key, hooks),
        }
    }

    fn apply_vi_motion_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
        hooks: &impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "vi-bWord" | "vi-backward-bigword" => {
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    repeat(state, |state| {
                        state.buffer.backward_bigword();
                    });
                });
            }
            "vi-back-to-indent" | "vi-first-print" => {
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    state.buffer.move_to_first_nonblank();
                });
            }
            "vi-backward-word" | "vi-bword" | "vi-prev-word" => {
                let word_breaks = self.editing_word_breaks(hooks);
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    repeat(state, |state| {
                        state.buffer.backward_word(word_breaks.as_deref());
                    });
                });
            }
            "vi-column" => {
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    let column = state.numeric_arg.take().unwrap_or(1).max(1) as usize - 1;
                    state.buffer.set_point(column);
                });
            }
            "vi-eWord" | "vi-end-bigword" => {
                self.apply_vi_motion_with_operator(state, key, true, |state| {
                    repeat(state, |state| {
                        state.buffer.end_bigword();
                    });
                });
            }
            "vi-end-word" | "vi-eword" => {
                let word_breaks = self.editing_word_breaks(hooks);
                self.apply_vi_motion_with_operator(state, key, true, |state| {
                    repeat(state, |state| {
                        state.buffer.end_word(word_breaks.as_deref());
                    });
                });
            }
            "vi-fWord" | "vi-forward-bigword" => {
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    repeat(state, |state| {
                        state.buffer.forward_bigword();
                    });
                });
            }
            "vi-forward-word" | "vi-fword" | "vi-next-word" => {
                let word_breaks = self.editing_word_breaks(hooks);
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    repeat(state, |state| {
                        state.buffer.forward_word(word_breaks.as_deref());
                    });
                });
            }
            _ => unreachable!("named vi motion command mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }

    fn apply_vi_motion_with_operator(
        &mut self,
        state: &mut EditorState,
        key: &[u8],
        inclusive: bool,
        motion: impl FnOnce(&mut EditorState),
    ) {
        let op_start = state.take_vi_operator();
        motion(state);
        self.finish_vi_motion_operator(state, op_start, key, inclusive);
        state.after_non_kill_command();
    }

    fn apply_vi_operator_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "vi-change-case" => {
                state.record_undo();
                repeat(state, |state| {
                    state.buffer.toggle_case_at_point();
                });
                state.vi.last_vi_change = Some(state.vi_key_sequence_for_change(key));
                state.after_non_kill_command();
            }
            "vi-change-char" | "vi-replace" => {
                state.input.pending_replace = true;
                state.vi.vi_insert_change = Some(state.vi_key_sequence_for_change(key));
                state.after_non_kill_command();
            }
            "vi-change-to" => {
                if key == b"C" {
                    self.apply_vi_operator_range_bounds(
                        state,
                        ViOperator::Change,
                        state.buffer.point(),
                        state.buffer.len_chars(),
                    );
                    state.begin_vi_insert_change(key);
                    state.after_non_kill_command();
                    return Ok(EditorOutcome::Continue);
                }
                self.handle_vi_doubled_operator(state, ViOperator::Change, key, |line, state| {
                    line.apply_vi_operator_range_bounds(
                        state,
                        ViOperator::Change,
                        0,
                        state.buffer.len_chars(),
                    );
                    state.begin_vi_insert_change(b"cc");
                });
                state.after_non_kill_command();
            }
            "vi-delete" => {
                state.record_undo();
                repeat(state, |state| {
                    state.buffer.delete_char();
                });
                state.vi.last_vi_change = Some(state.vi_key_sequence_for_change(key));
                state.after_non_kill_command();
            }
            "vi-delete-to" => {
                if key == b"D" {
                    self.apply_vi_operator_range_bounds(
                        state,
                        ViOperator::Delete,
                        state.buffer.point(),
                        state.buffer.len_chars(),
                    );
                    state.vi.last_vi_change = Some(state.vi_key_sequence_for_change(key));
                    state.after_non_kill_command();
                    return Ok(EditorOutcome::Continue);
                }
                self.handle_vi_doubled_operator(state, ViOperator::Delete, key, |line, state| {
                    line.apply_vi_operator_range_bounds(
                        state,
                        ViOperator::Delete,
                        0,
                        state.buffer.len_chars(),
                    );
                    state.vi.last_vi_change = Some(vec![b'd', b'd']);
                });
                state.after_non_kill_command();
            }
            "vi-subst" => {
                state.record_undo();
                if key == b"S" {
                    let killed = state.buffer.kill_whole_line();
                    state.push_kill(killed, KillDirection::Forward);
                } else if let Some(killed) = state.buffer.delete_char_bytes() {
                    state.push_kill(killed, KillDirection::Forward);
                }
                self.keymap.set_current(KeyMapName::ViInsert);
                state.begin_vi_insert_change(key);
                state.after_non_kill_command();
            }
            "vi-yank-to" => {
                if key == b"Y" {
                    let text = state
                        .buffer
                        .range_bytes(state.buffer.point(), state.buffer.len_chars());
                    state.push_kill(text, KillDirection::Forward);
                    state.after_non_kill_command();
                    return Ok(EditorOutcome::Continue);
                }
                self.handle_vi_doubled_operator(state, ViOperator::Yank, key, |_line, state| {
                    let text = state.buffer.as_bytes().to_vec();
                    state.push_kill(text, KillDirection::Forward);
                    state.buffer.move_beginning();
                });
                state.after_non_kill_command();
            }
            _ => unreachable!("named vi operator command mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }

    fn handle_vi_doubled_operator(
        &mut self,
        state: &mut EditorState,
        operator: ViOperator,
        key: &[u8],
        on_doubled: impl FnOnce(&mut Self, &mut EditorState),
    ) {
        if matches!(
            state.take_vi_operator().map(|(op, _, _)| op),
            Some(op) if op == operator
        ) {
            on_doubled(self, state);
        } else {
            state.set_vi_operator(operator, key);
        }
    }

    fn apply_vi_edit_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "vi-overstrike-delete" | "vi-rubout" => {
                state.record_undo();
                state.buffer.backward_delete_char();
                state.after_non_kill_command();
            }
            "vi-put" => {
                state.record_undo();
                if key == b"P" {
                    state.vi_put_before();
                } else {
                    state.buffer.move_forward();
                    state.vi_put();
                }
                state.vi.last_vi_change = Some(state.vi_key_sequence_for_change(key));
            }
            "vi-redo" => {
                if let Some(bytes) = state.vi.last_vi_change.clone() {
                    return self.replay_vi_change(state, &bytes, hooks);
                }
                state.after_non_kill_command();
            }
            "vi-undo" => {
                state.undo();
            }
            "vi-yank-pop" => {
                state.record_undo();
                state.yank_pop();
            }
            _ => unreachable!("named vi edit command mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }

    fn apply_vi_search_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "character-search" | "character-search-backward" | "vi-char-search" => {
                self.start_or_repeat_char_search(state, command, key)?;
                state.after_non_kill_command();
            }
            "vi-arg-digit" => {
                update_numeric_argument(state, key);
                if state.vi.vi_operator.is_some() {
                    if let Some(change) = state.vi.vi_operator_key.as_mut() {
                        change.extend_from_slice(key);
                    }
                } else {
                    state.vi.vi_count_keys.extend_from_slice(key);
                }
            }
            "vi-search" => {
                state.search.reverse_search = Some(ReverseSearchState {
                    query: Vec::new(),
                    match_line: None,
                    match_index: None,
                    direction: if key == b"?" {
                        SearchDirection::Forward
                    } else {
                        SearchDirection::Backward
                    },
                    original_line: state.buffer.as_bytes().to_vec(),
                });
                state.after_non_kill_command();
            }
            "vi-search-again" => {
                let query = state.search.last_search.clone();
                if let Some(query) = query {
                    let mut search_direction =
                        state.search.last_search_direction.unwrap_or_default();
                    if key == b"N" {
                        search_direction = match search_direction {
                            SearchDirection::Backward => SearchDirection::Forward,
                            SearchDirection::Forward => SearchDirection::Backward,
                        };
                    }
                    let direction = match search_direction {
                        SearchDirection::Backward => HistoryDirection::Previous,
                        SearchDirection::Forward => HistoryDirection::Next,
                    };
                    if let Some(found) = self.history.history_search_bytes_with_case(
                        &query,
                        direction,
                        self.variable_is_on("search-ignore-case"),
                    ) {
                        self.replace_from_history(state, &found.line_bytes);
                    }
                }
                state.after_non_kill_command();
            }
            _ => unreachable!("named vi search command mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }

    fn apply_vi_mode_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "edit-and-execute-command" | "vi-edit-and-execute-command" => {
                state.record_undo();
                if let Some(edited) = hooks.edit_and_execute(state.buffer.as_bytes()) {
                    state.buffer = LineBuffer::from_bytes(edited);
                    return Ok(EditorOutcome::Accepted(state.buffer.as_bytes().to_vec()));
                }
                self.ding()?;
                state.after_non_kill_command();
            }
            "vi-eof-maybe" => {
                if state.buffer.is_empty() {
                    return Ok(EditorOutcome::Eof);
                }
                state.after_non_kill_command();
            }
            "vi-editing-mode" => {
                self.keymap.set_current(KeyMapName::ViInsert);
                self.variables
                    .insert("editing-mode".to_string(), "vi".to_string());
                self.variables
                    .insert("keymap".to_string(), "vi".to_string());
                state.after_non_kill_command();
            }
            "vi-goto-mark" => {
                state.vi.pending_vi_mark = Some(ViMarkAction::Goto);
                state.vi.pending_mark_operator = state.take_vi_operator();
                state.after_non_kill_command();
            }
            "vi-match" => {
                let op_start = state.take_vi_operator();
                self.vi_match_bracket(state);
                self.finish_vi_motion_operator(state, op_start, key, true);
                state.after_non_kill_command();
            }
            "vi-set-register" => {
                state.vi.pending_vi_register = true;
                state.after_non_kill_command();
            }
            "vi-set-mark" => {
                state.vi.pending_vi_mark = Some(ViMarkAction::Set);
                state.after_non_kill_command();
            }
            _ => unreachable!("named vi mode command mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn vi_match_bracket(&mut self, state: &mut EditorState) {
        let pairs = [('(', ')'), ('[', ']'), ('{', '}')];
        let Some(ch) = state.buffer.char_at_point() else {
            return;
        };
        for (open, close) in pairs {
            if ch == open {
                if let Some(pos) =
                    state
                        .buffer
                        .find_matching_bracket_forward(state.buffer.point(), open, close)
                {
                    state.buffer.set_point(pos);
                }
                return;
            }
            if ch == close {
                if let Some(pos) =
                    state
                        .buffer
                        .find_matching_bracket_backward(state.buffer.point(), open, close)
                {
                    state.buffer.set_point(pos);
                }
                return;
            }
        }
    }

    pub(crate) fn start_or_repeat_char_search(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
    ) -> Result<(), ReadlineError> {
        let mode = match (command, key) {
            ("character-search", _) => Some(CharSearchMode::Forward),
            ("character-search-backward", _) => Some(CharSearchMode::Backward),
            ("vi-char-search", b"f") => Some(CharSearchMode::Forward),
            ("vi-char-search", b"F") => Some(CharSearchMode::Backward),
            ("vi-char-search", b"t") => Some(CharSearchMode::TillForward),
            ("vi-char-search", b"T") => Some(CharSearchMode::TillBackward),
            ("vi-char-search", b";") => {
                let op_start = state.take_vi_operator();
                if let Some((mode, ch)) = state.vi.last_char_search {
                    if self.apply_char_search_with_repeat(state, mode, ch, true)? {
                        self.finish_vi_motion_operator(state, op_start, key, true);
                    } else {
                        state.cancel_pending_command();
                    }
                } else {
                    state.cancel_pending_command();
                    self.ding()?;
                }
                return Ok(());
            }
            ("vi-char-search", b",") => {
                let op_start = state.take_vi_operator();
                if let Some((mode, ch)) = state.vi.last_char_search {
                    if self.apply_char_search_with_repeat(state, mode.reversed(), ch, true)? {
                        self.finish_vi_motion_operator(state, op_start, key, true);
                    } else {
                        state.cancel_pending_command();
                    }
                } else {
                    state.cancel_pending_command();
                    self.ding()?;
                }
                return Ok(());
            }
            _ => None,
        };

        if let Some(mode) = mode {
            state.vi.pending_char_search = Some(mode);
            state.vi.pending_char_search_operator = state.take_vi_operator();
        }
        Ok(())
    }
    pub(crate) fn apply_char_search(
        &mut self,
        state: &mut EditorState,
        mode: CharSearchMode,
        ch: char,
    ) -> Result<bool, ReadlineError> {
        self.apply_char_search_with_repeat(state, mode, ch, false)
    }

    fn apply_char_search_with_repeat(
        &mut self,
        state: &mut EditorState,
        mode: CharSearchMode,
        ch: char,
        repeat: bool,
    ) -> Result<bool, ReadlineError> {
        let found = match mode {
            CharSearchMode::Forward => state.buffer.find_forward(ch, true),
            CharSearchMode::Backward => state.buffer.find_backward(ch, true),
            CharSearchMode::TillForward if repeat => state
                .buffer
                .find_forward_from(ch, state.buffer.point().saturating_add(2)),
            CharSearchMode::TillForward => state.buffer.find_forward(ch, true),
            CharSearchMode::TillBackward if repeat => state
                .buffer
                .find_backward_from(ch, state.buffer.point().saturating_sub(2)),
            CharSearchMode::TillBackward => state.buffer.find_backward(ch, true),
        };

        let Some(mut pos) = found else {
            self.ding()?;
            return Ok(false);
        };

        match mode {
            CharSearchMode::TillForward => {
                pos = pos.saturating_sub(1);
            }
            CharSearchMode::TillBackward => {
                pos = (pos + 1).min(state.buffer.len_chars());
            }
            CharSearchMode::Forward | CharSearchMode::Backward => {}
        }
        state.buffer.set_point(pos);
        Ok(true)
    }

    pub(crate) fn apply_vi_operator_range(
        &mut self,
        state: &mut EditorState,
        op: ViOperator,
        start: usize,
        inclusive: bool,
    ) {
        let mut end = state.buffer.point();
        if inclusive {
            if end >= start {
                end = (end + 1).min(state.buffer.len_chars());
            }
        } else if end > start && matches!(op, ViOperator::Delete) {
            end = state.buffer.next_nonblank_from(end);
        }
        self.apply_vi_operator_range_bounds(state, op, start.min(end), start.max(end));
    }

    pub(crate) fn apply_vi_operator_range_bounds(
        &mut self,
        state: &mut EditorState,
        op: ViOperator,
        start: usize,
        end: usize,
    ) {
        if start == end {
            if matches!(op, ViOperator::Change) {
                state.record_undo();
                self.keymap.set_current(KeyMapName::ViInsert);
            }
            return;
        }
        match op {
            ViOperator::Delete | ViOperator::Change => {
                state.record_undo();
                let killed = state.buffer.delete_range_bytes(start, end);
                state.push_kill(killed, KillDirection::Forward);
                if matches!(op, ViOperator::Change) {
                    self.keymap.set_current(KeyMapName::ViInsert);
                }
            }
            ViOperator::Yank => {
                let text = state.buffer.range_bytes(start, end);
                state.push_kill(text, KillDirection::Forward);
                state.buffer.set_point(start);
            }
        }
    }
}
