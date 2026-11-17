use super::*;
use crate::variables::BoolVariable;

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
                let prefix = state.history_navigation_needle();
                let ignore_case = self.flag(BoolVariable::SearchIgnoreCase);
                let locale_lowercase = self.search_locale_lowercase(ignore_case);
                let arg = state.take_numeric_arg().unwrap_or(1);
                let count = arg.unsigned_abs() as usize;
                let mut matched = false;
                if arg < 0 {
                    for _ in 0..count {
                        let before_buffer = state.buffer.clone();
                        let before_pos = self.history.where_history();
                        if let Some(line) = history_prefix_forward_with_locale(
                            &mut self.history,
                            &prefix,
                            ignore_case,
                            locale_lowercase.as_ref(),
                        ) {
                            // An excessive repeat past the newest entry restores the
                            // saved edit; after prior successes in this command,
                            // stay at the newest match instead (Bash 5.3 PTY).
                            if matched
                                && self.history.where_history() >= self.history.entries().len()
                            {
                                self.history.set_pos(before_pos);
                                state.buffer = before_buffer;
                                break;
                            }
                            self.replace_from_history(state, &line);
                            matched = true;
                        } else {
                            if !matched {
                                self.ding()?;
                            }
                            break;
                        }
                    }
                } else {
                    let current = state.buffer.as_bytes().to_vec();
                    for _ in 0..count {
                        if let Some(line) = history_prefix_backward_with_locale(
                            &mut self.history,
                            &prefix,
                            current.clone(),
                            ignore_case,
                            locale_lowercase.as_ref(),
                        ) {
                            self.replace_from_history(state, &line);
                            matched = true;
                        } else {
                            if !matched {
                                self.ding()?;
                            }
                            break;
                        }
                    }
                }
                // Only successful nonempty prefix recalls change point and mark;
                // keep saved-edit restoration and preserve-point policy unchanged.
                if matched
                    && !prefix.is_empty()
                    && self.history.current_index().is_some()
                    && !self.flag(BoolVariable::HistoryPreservePoint)
                {
                    state.buffer.set_point(prefix.len());
                    state.mark = Some(state.buffer.len_chars());
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::HistorySearchForward => {
                let prefix = state.history_navigation_needle();
                let ignore_case = self.flag(BoolVariable::SearchIgnoreCase);
                let locale_lowercase = self.search_locale_lowercase(ignore_case);
                let arg = state.take_numeric_arg().unwrap_or(1);
                let count = arg.unsigned_abs() as usize;
                let mut matched = false;
                if arg < 0 {
                    let current = state.buffer.as_bytes().to_vec();
                    for _ in 0..count {
                        if let Some(line) = history_prefix_backward_with_locale(
                            &mut self.history,
                            &prefix,
                            current.clone(),
                            ignore_case,
                            locale_lowercase.as_ref(),
                        ) {
                            self.replace_from_history(state, &line);
                            matched = true;
                        } else {
                            if !matched {
                                self.ding()?;
                            }
                            break;
                        }
                    }
                } else {
                    for _ in 0..count {
                        let before_pos = self.history.where_history();
                        if let Some(line) = history_prefix_forward_with_locale(
                            &mut self.history,
                            &prefix,
                            ignore_case,
                            locale_lowercase.as_ref(),
                        ) {
                            // Do not load a nonmatching saved edit as a search
                            // result. After prior successes, stop at the newest
                            // match silently even if the saved edit matches.
                            if self.history.where_history() >= self.history.entries().len()
                                && (matched
                                    || (state.history_entry_index.is_some()
                                        && !search_prefix_matches(
                                            &line,
                                            &prefix,
                                            ignore_case,
                                            locale_lowercase.as_ref(),
                                        )))
                            {
                                self.history.set_pos(before_pos);
                                if !matched {
                                    self.ding()?;
                                }
                                break;
                            }
                            self.replace_from_history(state, &line);
                            matched = true;
                        } else {
                            if !matched {
                                self.ding()?;
                            }
                            break;
                        }
                    }
                }
                // Only successful nonempty prefix recalls change point and mark;
                // keep saved-edit restoration and preserve-point policy unchanged.
                if matched
                    && !prefix.is_empty()
                    && self.history.current_index().is_some()
                    && !self.flag(BoolVariable::HistoryPreservePoint)
                {
                    state.buffer.set_point(prefix.len());
                    state.mark = Some(state.buffer.len_chars());
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::PreviousHistory | EditCommand::NextHistory => {
                let arg = state.take_numeric_arg().unwrap_or(1);
                let direction = if (command == EditCommand::NextHistory) != (arg < 0) {
                    HistoryDirection::Next
                } else {
                    HistoryDirection::Previous
                };
                for step in 0..arg.unsigned_abs() {
                    let before_pos = self.history.where_history();
                    if let Some(line) = self
                        .history
                        .navigate_bytes(direction, state.buffer.as_bytes().to_vec())
                    {
                        self.replace_from_history(state, &line);
                        if direction == HistoryDirection::Previous
                            && self.history.where_history() == before_pos
                        {
                            break;
                        }
                    } else {
                        if direction == HistoryDirection::Next && step == 0 {
                            self.ding()?;
                        }
                        break;
                    }
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ReverseSearchHistory => {
                if state.search.reverse_search.is_some() {
                    let Some(search) = state.search.reverse_search.take() else {
                        return Ok(EditorOutcome::Continue);
                    };
                    return self.repeat_reverse_search(state, search, SearchDirection::Backward);
                }
                state.search.quoted_pending = false;
                state.search.non_incremental_search = None;
                let original_point = state.buffer.point();
                let original_history_pos = self.history.where_history();
                state.search.reverse_search = Some(ReverseSearchState {
                    direction: SearchDirection::Backward,
                    original_line: state.buffer.as_bytes().to_vec(),
                    original_point,
                    original_history_pos,
                    original_history_entry_index: state.history_entry_index,
                    original_undo: state.undo.clone(),
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
                let index = state.take_numeric_arg().unwrap_or(1).unsigned_abs() as usize;
                // GNU fetches the first entry for an out-of-range number
                // (tested 0/4/5/6 against 3 entries, silently); an empty
                // history stays put.
                let line = self
                    .history
                    .get_1_based_entry(index)
                    .or_else(|| {
                        if self.history.entries().is_empty() {
                            None
                        } else {
                            self.history.get_1_based_entry(1)
                        }
                    })
                    .map(|entry| entry.line_bytes.clone());
                if let Some(line) = line {
                    self.replace_from_history(state, &line);
                }
                state.after_non_kill_command();
            }
            "forward-search-history" => {
                if state.search.reverse_search.is_some() {
                    let Some(search) = state.search.reverse_search.take() else {
                        return Ok(EditorOutcome::Continue);
                    };
                    return self.repeat_reverse_search(state, search, SearchDirection::Forward);
                }
                state.search.quoted_pending = false;
                state.search.non_incremental_search = None;
                let original_point = state.buffer.point();
                let original_history_pos = self.history.where_history();
                state.search.reverse_search = Some(ReverseSearchState {
                    direction: SearchDirection::Forward,
                    original_line: state.buffer.as_bytes().to_vec(),
                    original_point,
                    original_history_pos,
                    original_history_entry_index: state.history_entry_index,
                    original_undo: state.undo.clone(),
                    ..Default::default()
                });
                state.kill.last_was_kill = false;
                state.kill.last_yank = None;
            }
            "history-substring-search-forward" => {
                let needle = state.history_navigation_needle();
                let current = state.buffer.as_bytes().to_vec();
                let ignore_case = self.flag(BoolVariable::SearchIgnoreCase);
                let locale_lowercase = self.search_locale_lowercase(ignore_case);
                let arg = state.take_numeric_arg().unwrap_or(1);
                let count = arg.unsigned_abs() as usize;
                let mut matched = false;
                for _ in 0..count {
                    let found = if arg < 0 {
                        history_substring_backward_with_locale(
                            &mut self.history,
                            &needle,
                            current.clone(),
                            ignore_case,
                            locale_lowercase.as_ref(),
                        )
                    } else {
                        history_substring_forward_with_locale(
                            &mut self.history,
                            &needle,
                            current.clone(),
                            ignore_case,
                            locale_lowercase.as_ref(),
                        )
                    };
                    if let Some(line) = found {
                        self.replace_from_history(state, &line);
                        matched = true;
                    } else {
                        if !matched {
                            self.ding()?;
                        }
                        break;
                    }
                }
                // A negative argument makes the forward command search backward.
                let from_end = arg < 0;
                if matched
                    && !needle.is_empty()
                    && let Some(offset) = search_bytes_offset(
                        state.buffer.as_bytes(),
                        &needle,
                        ignore_case,
                        locale_lowercase.as_ref(),
                        from_end,
                    )
                {
                    state.buffer.set_point(offset);
                    state.mark = Some(state.buffer.len_chars());
                }
                state.after_non_kill_command();
            }
            "history-substring-search-backward" => {
                let needle = state.history_navigation_needle();
                let current = state.buffer.as_bytes().to_vec();
                let ignore_case = self.flag(BoolVariable::SearchIgnoreCase);
                let locale_lowercase = self.search_locale_lowercase(ignore_case);
                let arg = state.take_numeric_arg().unwrap_or(1);
                let count = arg.unsigned_abs() as usize;
                let mut matched = false;
                for _ in 0..count {
                    let found = if arg < 0 {
                        history_substring_forward_with_locale(
                            &mut self.history,
                            &needle,
                            current.clone(),
                            ignore_case,
                            locale_lowercase.as_ref(),
                        )
                    } else {
                        history_substring_backward_with_locale(
                            &mut self.history,
                            &needle,
                            current.clone(),
                            ignore_case,
                            locale_lowercase.as_ref(),
                        )
                    };
                    if let Some(line) = found {
                        self.replace_from_history(state, &line);
                        matched = true;
                    } else {
                        if !matched {
                            self.ding()?;
                        }
                        break;
                    }
                }
                // A negative argument makes the backward command search forward.
                let from_end = arg >= 0;
                if matched
                    && !needle.is_empty()
                    && let Some(offset) = search_bytes_offset(
                        state.buffer.as_bytes(),
                        &needle,
                        ignore_case,
                        locale_lowercase.as_ref(),
                        from_end,
                    )
                {
                    state.buffer.set_point(offset);
                    state.mark = Some(state.buffer.len_chars());
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
                state.search.quoted_pending = false;
                state.search.reverse_search = None;
                state.search.non_incremental_search = Some(NonIncrementalSearchState {
                    query: Vec::new(),
                    direction: if command == "non-incremental-forward-search-history" {
                        SearchDirection::Forward
                    } else {
                        SearchDirection::Backward
                    },
                    original_line: state.buffer.as_bytes().to_vec(),
                    original_history_pos: self.history.where_history(),
                    original_history_entry_index: state.history_entry_index,
                    original_undo: state.undo.clone(),
                    vi: false,
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
                if let Some(query) = state.search.last_non_incremental_search.clone() {
                    let history_direction = match direction {
                        SearchDirection::Backward => HistoryDirection::Previous,
                        SearchDirection::Forward => HistoryDirection::Next,
                    };
                    let ignore_case = self.flag(BoolVariable::SearchIgnoreCase);
                    // GNU `history_search` includes the positioned entry. Step
                    // exclusively past the last nonincremental match, which is
                    // distinct from the preserved history cursor for emacs
                    // searches begun on an existing entry. Restore the actual
                    // cursor if this repeat fails.
                    let len = self.history.entries().len();
                    let saved_pos = self.history.where_history().min(len);
                    let match_pos = state
                        .search
                        .last_non_incremental_search_match_index
                        .unwrap_or(saved_pos)
                        .min(len);
                    let stepped = match direction {
                        SearchDirection::Backward => match_pos.checked_sub(1),
                        SearchDirection::Forward => match_pos.checked_add(1),
                    };
                    let mut moved = false;
                    if let Some(stepped) = stepped {
                        moved = true;
                        self.history.set_pos(stepped.min(len));
                    }
                    let locale_lowercase = self.search_locale_lowercase(ignore_case);
                    let found = if moved {
                        history_search_with_locale(
                            &mut self.history,
                            &query,
                            history_direction,
                            ignore_case,
                            locale_lowercase.as_ref(),
                        )
                    } else {
                        None
                    };
                    if found.is_none() && moved {
                        self.history.set_pos(saved_pos);
                    }
                    if let Some(found) = found {
                        state.search.last_non_incremental_search_match_index =
                            Some(found.entry_index);
                        self.replace_from_history(state, &found.line_bytes);
                    } else {
                        self.ding()?;
                    }
                    state.search.last_non_incremental_search_direction = Some(direction);
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
