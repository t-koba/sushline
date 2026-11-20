use super::*;

/// GNU leaves vi point on the last remaining character after a delete ran
/// to the end of the line. Both operator deletes and `vi-delete` use this.
fn back_off_vi_delete_point(state: &mut EditorState, reached_end: bool) {
    if reached_end && !state.buffer.is_empty() {
        state.buffer.move_backward();
    }
}

/// GNU dot-replays lowercase `p` with its forward motion: a put leaves point
/// on the last inserted character, so replay steps forward before yanking.
/// Uppercase `P` replays at the current point without that motion.
fn vi_put_redo(change: &[u8]) -> Option<(u8, i32)> {
    let digit_len = change
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let command = *change.get(digit_len)?;
    if !matches!(command, b'p' | b'P') || change.len() != digit_len + 1 {
        return None;
    }
    let count = if digit_len == 0 {
        1
    } else {
        std::str::from_utf8(&change[..digit_len])
            .ok()?
            .parse()
            .ok()?
    };
    Some((command, count))
}

/// A saved vi insertion is replayed by stuffing its text at the current point
/// and backing up over the last inserted character. A count typed before
/// `i`/`I`/`a`/`A` does not multiply the insertion, so its saved repeat is one.
struct ViInsertRedo {
    command: u8,
    text: Vec<u8>,
}

fn vi_insert_redo(change: &[u8]) -> Option<ViInsertRedo> {
    let digit_len = change
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let rest = &change[digit_len..];
    let command = *rest.first()?;
    if !matches!(command, b'i' | b'I' | b'a' | b'A') {
        return None;
    }
    let end = if rest.last() == Some(&0x1b) {
        rest.len() - 1
    } else {
        rest.len()
    };
    if end <= 1 {
        return None;
    }
    Some(ViInsertRedo {
        command,
        text: rest[1..end].to_vec(),
    })
}

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
                // GNU frees the undo list on the first entry to command mode:
                // text typed before that point cannot be undone with `u`.
                if !state.vi.command_mode_entered {
                    state.undo.undo_stack.clear();
                    state.undo.pending_undo = None;
                    state.clear_vi_put_undo();
                    state.vi.command_mode_entered = true;
                    // The redo synthesis is separate from the undo reset: a
                    // cancelled operator persists across reads and must keep
                    // its redo, but GNU still frees the undo list each read.
                    if !state.vi.last_change_is_cancelled_operator {
                        // GNU keeps an uninterrupted initial insertion as the
                        // default vi redo change. It is not started by `i`, so
                        // synthesize the same key sequence from the line as typed,
                        // without making the initial text undoable. A persisted
                        // non-insert command (`x`, `cw`, ...) remains the redo
                        // command across reads.
                        let prior_is_insert = state
                            .vi
                            .last_vi_change
                            .as_ref()
                            .and_then(|change| {
                                change.iter().copied().find(|byte| !byte.is_ascii_digit())
                            })
                            .is_none_or(|byte| matches!(byte, b'i' | b'I' | b'a' | b'A'));
                        if prior_is_insert {
                            if state.vi.vi_insert_change.is_none()
                                && !state.vi.initial_insert_bytes.is_empty()
                                && state.vi.initial_insert_bytes == state.buffer.as_bytes()
                            {
                                let bytes = state.buffer.as_bytes();
                                let mut change = vec![b'i'];
                                change.extend_from_slice(&bytes[vi_initial_redo_start(bytes)..]);
                                change.push(b'\x1b');
                                state.set_initial_insert_vi_redo(change);
                            } else if !state.vi.initial_insert_bytes.is_empty()
                                || !state.vi.last_change_is_initial_insert
                            {
                                // Editing the initial text before this read's
                                // first ESC, or carrying an explicit insert redo
                                // into an empty read, drops GNU's redo change.
                                state.vi.last_vi_change = None;
                                state.vi.last_change_is_cancelled_operator = false;
                                state.vi.last_change_is_initial_insert = false;
                            }
                        }
                    }
                }
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
        "vi-change-char" => Some(ViCommandGroup::Operator),
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
        "vi-redo" | "vi-replace" => Some(ViCommandGroup::Edit),
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
        hooks: &mut impl Hooks,
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
                if command == "vi-prev-word" && key == b"B" {
                    self.apply_vi_motion_with_operator(state, key, false, |state| {
                        repeat(state, |state| {
                            state.buffer.backward_bigword();
                        });
                    });
                    return Ok(EditorOutcome::Continue);
                }
                let word_breaks = self.editing_word_breaks(hooks);
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    repeat(state, |state| {
                        state.buffer.vi_backward_word(word_breaks.as_deref());
                    });
                });
            }
            "vi-column" => {
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    let column = state.take_numeric_arg().unwrap_or(1).max(1) as usize - 1;
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
                if command == "vi-end-word" && key == b"E" {
                    self.apply_vi_motion_with_operator(state, key, true, |state| {
                        repeat(state, |state| {
                            state.buffer.end_bigword();
                        });
                    });
                    return Ok(EditorOutcome::Continue);
                }
                let word_breaks = self.editing_word_breaks(hooks);
                self.apply_vi_motion_with_operator(state, key, true, |state| {
                    repeat(state, |state| {
                        state.buffer.vi_end_word(word_breaks.as_deref());
                    });
                });
            }
            "vi-fWord" | "vi-forward-bigword" => {
                let change_operator = matches!(state.vi.vi_operator, Some(ViOperator::Change));
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    repeat(state, |state| {
                        if change_operator {
                            state.buffer.forward_bigword_end();
                        } else {
                            state.buffer.forward_bigword();
                        }
                    });
                });
            }
            "vi-forward-word" | "vi-fword" | "vi-next-word" => {
                if command == "vi-next-word" && key == b"W" {
                    let change_operator = matches!(state.vi.vi_operator, Some(ViOperator::Change));
                    self.apply_vi_motion_with_operator(state, key, false, |state| {
                        repeat(state, |state| {
                            if change_operator {
                                state.buffer.forward_bigword_end();
                            } else {
                                state.buffer.forward_bigword();
                            }
                        });
                    });
                    return Ok(EditorOutcome::Continue);
                }
                let word_breaks = self.editing_word_breaks(hooks);
                let change_operator = matches!(state.vi.vi_operator, Some(ViOperator::Change));
                let change_count =
                    change_operator.then(|| state.take_numeric_arg().unwrap_or(1).unsigned_abs());
                self.apply_vi_motion_with_operator(state, key, false, |state| {
                    if let Some(count) = change_count {
                        state.buffer.vi_change_words(count, word_breaks.as_deref());
                    } else {
                        repeat(state, |state| {
                            state.buffer.vi_forward_word(word_breaks.as_deref());
                        });
                    }
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
                let change = state.vi_key_sequence_for_change(key);
                state.set_vi_redo(change);
                state.after_non_kill_command();
            }
            "vi-change-char" => {
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
                let mut reached_end = false;
                let mut deleted = Vec::new();
                repeat(state, |state| {
                    if let Some(bytes) = state.buffer.delete_char_bytes() {
                        deleted.extend_from_slice(&bytes);
                        if state.buffer.point() == state.buffer.len_chars() {
                            reached_end = true;
                        }
                    }
                });
                back_off_vi_delete_point(state, reached_end);
                if deleted.is_empty() {
                    self.ding()?;
                } else {
                    state.push_kill(deleted, KillDirection::Forward, self.kill_append_allowed());
                }
                let change = state.vi_key_sequence_for_change(key);
                state.set_vi_redo(change);
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
                    let change = state.vi_key_sequence_for_change(key);
                    state.set_vi_redo(change);
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
                    state.set_vi_redo(vec![b'd', b'd']);
                });
                state.after_non_kill_command();
            }
            "vi-subst" => {
                state.record_undo();
                state.vi.defer_undo_commit = true;
                if key == b"S" {
                    let killed = state.buffer.kill_whole_line();
                    state.push_kill(killed, KillDirection::Forward, self.kill_append_allowed());
                } else {
                    let count = repeat_count(state.take_numeric_arg());
                    // Vi command-mode `$` leaves the byte point just past the
                    // last character; GNU treats the last character as the one
                    // under the cursor for `s`.
                    if state.buffer.point() == state.buffer.len_chars() && !state.buffer.is_empty()
                    {
                        state.buffer.move_backward();
                    }
                    let mut killed = Vec::new();
                    for _ in 0..count {
                        match state.buffer.delete_char_bytes() {
                            Some(bytes) => killed.extend_from_slice(&bytes),
                            None => break,
                        }
                    }
                    if killed.is_empty() {
                        self.ding()?;
                    } else {
                        state.push_kill(killed, KillDirection::Forward, self.kill_append_allowed());
                    }
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
                    state.push_kill(text, KillDirection::Forward, self.kill_append_allowed());
                    state.after_non_kill_command();
                    return Ok(EditorOutcome::Continue);
                }
                let allow_append = self.kill_append_allowed();
                self.handle_vi_doubled_operator(state, ViOperator::Yank, key, |_line, state| {
                    let text = state.buffer.as_bytes().to_vec();
                    state.push_kill(text, KillDirection::Forward, allow_append);
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
                let count = state.take_numeric_arg().unwrap_or(1);
                let mut deleted = 0;
                for _ in 0..count.max(0) {
                    if !state.buffer.backward_delete_char() {
                        break;
                    }
                    deleted += 1;
                }
                // GNU rings when the count exceeds the characters before point,
                // including a zero count or an empty line.
                if deleted != count {
                    self.ding()?;
                }
                let change = state.vi_key_sequence_for_change(key);
                state.set_vi_redo(change);
                state.after_non_kill_command();
            }
            "vi-put" => {
                let yank_len = state.vi_put_yank_len();
                let before_edit = state.buffer.clone();
                state.record_undo();
                let yanked = if key == b"P" {
                    state.vi_put_before()
                } else {
                    state.buffer.move_forward();
                    state.vi_put()
                };
                if yanked {
                    let change = state.vi_key_sequence_for_change(key);
                    state.set_vi_redo(change);
                    state.after_non_kill_command();
                    // GNU keeps a lowercase put and its dot-replays in one
                    // undo entry; an uppercase put starts a separate group.
                    if !(key == b"p"
                        && state.extend_vi_put_undo(&before_edit, state.buffer.point(), yank_len))
                    {
                        state.start_vi_put_undo(&before_edit, state.buffer.point(), yank_len);
                    }
                    return Ok(EditorOutcome::Continue);
                } else {
                    // GNU leaves the previous redo command intact when the
                    // unnamed register is empty.
                    self.ding()?;
                }
            }
            "vi-redo" => {
                if state.vi.last_change_is_cancelled_operator {
                    state.buffer.move_end();
                    state.buffer.insert_bytes(b" ");
                    state.after_non_kill_command();
                    self.ding()?;
                    return Ok(EditorOutcome::Continue);
                }
                if let Some(original) = state.vi.last_vi_change.clone() {
                    let bytes = original.clone();
                    let explicit = state.take_numeric_arg().filter(|count| *count != 0);
                    let bytes = match explicit {
                        Some(count) => EditorState::vi_change_with_count(&bytes, count),
                        None => bytes,
                    };
                    if let Some((command, saved_count)) = vi_put_redo(&bytes) {
                        let yank_len = state.vi_put_yank_len();
                        let before_edit = state.buffer.clone();
                        state.record_undo();
                        let count = explicit.unwrap_or(saved_count).max(1) as usize;
                        for _ in 0..count {
                            let yanked = if command == b'P' {
                                state.vi_put_before()
                            } else {
                                state.buffer.move_forward();
                                state.vi_put()
                            };
                            if !yanked {
                                break;
                            }
                        }
                        // GNU remembers a count typed for this put: a later
                        // plain dot repeats that count, not the older one.
                        state.set_vi_redo(bytes);
                        state.after_non_kill_command();
                        if command != b'p'
                            || !state.extend_vi_put_undo(
                                &before_edit,
                                state.buffer.point(),
                                yank_len,
                            )
                        {
                            state.start_vi_put_undo(&before_edit, state.buffer.point(), yank_len);
                        }
                        return Ok(EditorOutcome::Continue);
                    }
                    if let Some(insert) = vi_insert_redo(&bytes) {
                        state.record_undo();
                        match insert.command {
                            b'I' => state.buffer.move_beginning(),
                            b'a' => {
                                state.buffer.move_forward();
                            }
                            b'A' => state.buffer.move_end(),
                            _ => {}
                        }
                        let count = explicit.unwrap_or(1).max(1) as usize;
                        for _ in 0..count {
                            state.buffer.insert_bytes(&insert.text);
                        }
                        if state.buffer.point() > 0 {
                            state.buffer.move_backward();
                        }
                        state.set_vi_redo(original);
                        state.after_non_kill_command();
                        return Ok(EditorOutcome::Continue);
                    }
                    return self.replay_vi_change(state, &bytes, hooks);
                }
                state.after_non_kill_command();
            }
            "vi-replace" => {
                state.overwrite_mode = true;
                self.keymap.set_current(KeyMapName::ViInsert);
                state.begin_vi_insert_change(key);
                state.after_non_kill_command();
            }
            "vi-undo" => {
                let count = state.take_numeric_arg().unwrap_or(1).max(1) as usize;
                let mut exhausted = false;
                for _ in 0..count {
                    if !state.undo() {
                        exhausted = true;
                        break;
                    }
                }
                if exhausted {
                    self.ding()?;
                }
            }
            "vi-yank-pop" => {
                state.record_undo();
                if !state.yank_pop() {
                    self.ding()?;
                }
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
                if update_numeric_argument(state, key) {
                    self.ding()?;
                }
                if state.vi.vi_operator.is_some() {
                    if let Some(change) = state.vi.vi_operator_key.as_mut() {
                        change.extend_from_slice(key);
                    }
                } else {
                    state.vi.vi_count_keys.extend_from_slice(key);
                }
            }
            "vi-search" => {
                if key != b"/" && key != b"?" {
                    // GNU only starts a search for `/` (backward) and `?`
                    // (forward): any other invoking key (for example a
                    // remapped binding) rings the bell without entering
                    // search, matching the patch 0 Bash 5.3 PTY oracle
                    // (`C-o` mapped to `vi-search` in emacs mode bells and
                    // leaves the typed query literal).
                    self.ding()?;
                    state.after_non_kill_command();
                    return Ok(EditorOutcome::Continue);
                }
                // GNU vi `/`/`?` is non-incremental (patch 0 Bash 5.3 PTY
                // oracle): the prompt shows `/query`/`?query`, the line stays
                // hidden until Enter executes, and a second Enter accepts.
                state.search.quoted_pending = false;
                state.search.reverse_search = None;
                let original_history_pos = self.history.where_history();
                state.search.non_incremental_search = Some(NonIncrementalSearchState {
                    query: Vec::new(),
                    direction: if key == b"?" {
                        SearchDirection::Forward
                    } else {
                        SearchDirection::Backward
                    },
                    original_line: state.buffer.as_bytes().to_vec(),
                    original_history_pos,
                    original_history_entry_index: state.history_entry_index,
                    original_undo: state.undo.clone(),
                    vi: true,
                });
                state.kill.last_was_kill = false;
                state.kill.last_yank = None;
            }
            "vi-search-again" => {
                let Some(query) = state.search.last_non_incremental_search.clone() else {
                    self.ding()?;
                    state.after_non_kill_command();
                    return Ok(EditorOutcome::Continue);
                };
                let mut search_direction = state
                    .search
                    .last_non_incremental_search_direction
                    .unwrap_or_default();
                if key == b"N" {
                    search_direction = match search_direction {
                        SearchDirection::Backward => SearchDirection::Forward,
                        SearchDirection::Forward => SearchDirection::Backward,
                    };
                }
                // GNU `n`/`N` step exclusively past the current match, so a
                // repeat from a matching line moves on instead of re-finding
                // it; a repeat with no further match rings the bell and keeps
                // the line (patch 0 Bash 5.3 PTY oracle).
                let pos = self.history.where_history();
                // GNU vi `n`/`N` repeats are case-sensitive even when
                // `search-ignore-case` is on (Bash 5.3 PTY oracle).
                let ignore_case = false;
                let found = match search_direction {
                    SearchDirection::Backward => {
                        search_history_backward(&self.history, &query, Some(pos), ignore_case, None)
                    }
                    SearchDirection::Forward => {
                        search_history_forward(&self.history, &query, Some(pos), ignore_case, None)
                    }
                };
                if let Some((index, line)) = found {
                    state.search.last_non_incremental_search_match_index = Some(index);
                    self.history.set_pos(index);
                    self.replace_from_history(state, &line);
                } else {
                    self.ding()?;
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

// GNU's recorded initial insert starts at the last multibyte character of
// the typed line; ASCII lines are recorded in full.
fn vi_initial_redo_start(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(text) => text
            .char_indices()
            .filter(|(_, ch)| !ch.is_ascii())
            .map(|(index, _)| index)
            .next_back()
            .unwrap_or(0),
        Err(_) => 0,
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
                    if self.apply_char_search_with_repeat(state, mode, ch, true, false)? {
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
                    if self.apply_char_search_with_repeat(
                        state,
                        mode.reversed(),
                        ch,
                        true,
                        false,
                    )? {
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
            state.vi.pending_char_search_count =
                if matches!(command, "character-search" | "character-search-backward") {
                    Some(state.numeric_arg.unwrap_or(1))
                } else {
                    None
                };
            state.vi.pending_char_search_operator = state.take_vi_operator();
        }
        Ok(())
    }
    pub(crate) fn apply_char_search(
        &mut self,
        state: &mut EditorState,
        mode: CharSearchMode,
        ch: char,
        emacs: bool,
    ) -> Result<bool, ReadlineError> {
        self.apply_char_search_with_repeat(state, mode, ch, false, emacs)
    }

    fn apply_char_search_with_repeat(
        &mut self,
        state: &mut EditorState,
        mode: CharSearchMode,
        ch: char,
        repeat: bool,
        emacs: bool,
    ) -> Result<bool, ReadlineError> {
        let found = match mode {
            CharSearchMode::Forward => state.buffer.find_forward(ch, true),
            CharSearchMode::Backward if emacs => state.buffer.find_backward_before_point(ch),
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
    ) -> bool {
        let mut end = state.buffer.point();
        if inclusive {
            if end >= start {
                end = (end + 1).min(state.buffer.len_chars());
            }
        } else if end > start && matches!(op, ViOperator::Delete) {
            end = state.buffer.next_nonblank_from(end);
        }
        let (lo, hi) = (start.min(end), start.max(end));
        if lo == hi {
            return false;
        }
        self.apply_vi_operator_range_bounds(state, op, lo, hi);
        true
    }

    pub(crate) fn apply_vi_operator_range_bounds(
        &mut self,
        state: &mut EditorState,
        op: ViOperator,
        start: usize,
        end: usize,
    ) {
        if start == end {
            return;
        }
        // GNU leaves the cursor on the last remaining character after a
        // delete that ran to the end of the line. Change enters insert mode
        // at the deletion point instead, so only delete moves back.
        let delete_to_end = matches!(op, ViOperator::Delete) && end >= state.buffer.len_chars();
        match op {
            ViOperator::Delete | ViOperator::Change => {
                state.record_undo();
                let killed = state.buffer.delete_range_bytes(start, end);
                state.push_kill(killed, KillDirection::Forward, self.kill_append_allowed());
                if matches!(op, ViOperator::Change) {
                    // Keep the deletion pending so the following insert joins
                    // it in one undo entry, as GNU does for change commands.
                    state.vi.defer_undo_commit = true;
                    self.keymap.set_current(KeyMapName::ViInsert);
                } else {
                    back_off_vi_delete_point(state, delete_to_end);
                }
            }
            ViOperator::Yank => {
                let text = state.buffer.range_bytes(start, end);
                state.push_kill(text, KillDirection::Forward, self.kill_append_allowed());
                state.buffer.set_point(start);
            }
        }
    }
}
