use super::*;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_history_nav_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        _key: &[u8],
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            EditCommand::HistoryBeginning => {
                if let Some(line) = self
                    .history
                    .beginning_bytes(state.buffer.as_bytes().to_vec())
                {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::HistoryEnd => {
                if let Some(line) = self.history.end_bytes() {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::HistorySearchBackward => {
                let prefix = state.buffer.range_bytes(0, state.buffer.point());
                if let Some(line) = self
                    .history
                    .search_prefix_backward_bytes(&prefix, state.buffer.as_bytes().to_vec())
                {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::HistorySearchForward => {
                let prefix = state.buffer.range_bytes(0, state.buffer.point());
                if let Some(line) = self.history.search_prefix_forward_bytes(&prefix) {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::NextHistory => {
                if let Some(line) = self
                    .history
                    .navigate_bytes(HistoryDirection::Next, state.buffer.as_bytes().to_vec())
                {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::PreviousHistory => {
                if let Some(line) = self
                    .history
                    .navigate_bytes(HistoryDirection::Previous, state.buffer.as_bytes().to_vec())
                {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ReverseSearchHistory => {
                state.search.reverse_search = Some(ReverseSearchState {
                    direction: SearchDirection::Backward,
                    original_line: state.buffer.as_bytes().to_vec(),
                    ..Default::default()
                });
                state.kill.last_was_kill = false;
                state.kill.last_yank = None;
                Ok(EditorOutcome::Continue)
            }
            _ => unreachable!("command group mismatch"),
        }
    }
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_named_history_nav_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        _key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "alias-expand-line" => {
                state.record_undo();
                if let Some(expanded) = hooks.expand_aliases(state.buffer.as_bytes()) {
                    state.buffer = LineBuffer::from_bytes(expanded);
                }
                state.after_non_kill_command();
            }
            "history-and-alias-expand-line" | "history-expand-line" => {
                state.record_undo();
                let line = state.buffer.as_bytes().to_vec();
                if let Some(expanded) = self.try_expand_history(state, &line, hooks)? {
                    let expanded = if command == "history-and-alias-expand-line" {
                        hooks.expand_aliases(&expanded).unwrap_or(expanded)
                    } else {
                        expanded
                    };
                    state.buffer = LineBuffer::from_bytes(expanded);
                    state.after_non_kill_command();
                }
            }
            "fetch-history" | "vi-fetch-history" => {
                if let Some(line) =
                    self.history
                        .get_1_based_entry(
                            state.numeric_arg.take().unwrap_or(1).unsigned_abs() as usize
                        )
                        .map(|entry| entry.line_bytes.clone())
                {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
            }
            "forward-search-history" => {
                state.search.reverse_search = Some(ReverseSearchState {
                    direction: SearchDirection::Forward,
                    original_line: state.buffer.as_bytes().to_vec(),
                    ..Default::default()
                });
                state.kill.last_was_kill = false;
                state.kill.last_yank = None;
            }
            "history-substring-search-forward" => {
                let needle = state.buffer.range_bytes(0, state.buffer.point());
                if let Some(line) = self.history.search_containing_forward_from_cursor_bytes(
                    &needle,
                    state.buffer.as_bytes().to_vec(),
                ) {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
            }
            "history-substring-search-backward" => {
                let needle = state.buffer.range_bytes(0, state.buffer.point());
                if let Some(line) = self.history.search_containing_backward_from_cursor_bytes(
                    &needle,
                    state.buffer.as_bytes().to_vec(),
                ) {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
            }
            "magic-space" => {
                state.record_undo();
                let line = state.buffer.as_bytes().to_vec();
                if let Some(expanded) = self.try_expand_history(state, &line, hooks)? {
                    state.buffer = LineBuffer::from_bytes(expanded);
                    state.buffer.insert_char(' ');
                    state.after_self_insert();
                }
            }
            "non-incremental-forward-search-history" | "non-incremental-reverse-search-history" => {
                state.search.non_incremental_search = Some(NonIncrementalSearchState {
                    query: Vec::new(),
                    direction: if command == "non-incremental-forward-search-history" {
                        SearchDirection::Forward
                    } else {
                        SearchDirection::Backward
                    },
                    original_line: state.buffer.as_bytes().to_vec(),
                    original_history_pos: self.history.where_history(),
                });
                state.after_non_kill_command();
            }
            "non-incremental-forward-search-history-again"
            | "non-incremental-reverse-search-history-again" => {
                let direction = if command == "non-incremental-forward-search-history-again" {
                    SearchDirection::Forward
                } else {
                    SearchDirection::Backward
                };
                if let Some(query) = state.search.last_search.clone() {
                    let history_direction = match direction {
                        SearchDirection::Backward => HistoryDirection::Previous,
                        SearchDirection::Forward => HistoryDirection::Next,
                    };
                    if let Some(found) = self.history.history_search_bytes_with_case(
                        &query,
                        history_direction,
                        self.variable_is_on("search-ignore-case"),
                    ) {
                        self.replace_from_history(state, &found.line_bytes);
                    } else {
                        self.ding()?;
                    }
                    state.search.last_search_direction = Some(direction);
                } else {
                    self.ding()?;
                }
                state.after_non_kill_command();
            }
            "operate-and-get-next" => {
                self.pending_initial_line = state
                    .numeric_arg
                    .take()
                    .and_then(|arg| {
                        self.history
                            .get_1_based_entry(arg.unsigned_abs() as usize)
                            .map(|entry| entry.line_bytes.clone())
                    })
                    .or_else(|| self.history.next_after_current_cursor_bytes());
                return Ok(EditorOutcome::Accepted(state.buffer.as_bytes().to_vec()));
            }
            _ => unreachable!("named command group mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }
}
