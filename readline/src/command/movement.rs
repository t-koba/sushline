use super::*;
use crate::variables::BoolVariable;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn finish_vi_motion_operator(
        &mut self,
        state: &mut EditorState,
        op_start: Option<(ViOperator, usize, Vec<u8>)>,
        key: &[u8],
        inclusive: bool,
    ) {
        if let Some((op, start, mut change)) = op_start {
            if !self.apply_vi_operator_range(state, op, start, inclusive) {
                // GNU still enters insert mode when a change motion spans no
                // text; the typed replacement becomes the redo change. Other
                // operators keep the failed motion as the redo command: dot
                // repeats it, so a ringing motion rings again while a silent
                // one stays silent, and neither edits the line.
                if matches!(op, ViOperator::Change) {
                    change.extend_from_slice(key);
                    state.finish_vi_operator_change(op, change);
                    self.keymap.set_current(KeyMapName::ViInsert);
                } else {
                    change.extend_from_slice(key);
                    state.set_vi_redo(change);
                }
                return;
            }
            change.extend_from_slice(key);
            state.finish_vi_operator_change(op, change);
        }
    }

    pub(super) fn apply_movement_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            EditCommand::BackwardChar => {
                // Ctrl-H cancels a pending vi operator without moving point;
                // other bindings of backward-char still move normally.
                let cancel_without_motion = key == b"\x08" && state.vi.vi_operator.is_some();
                let op_start = take_vi_motion_operator(state, command, key);
                let bytewise = self.char_motion_is_bytewise();
                if !cancel_without_motion && !move_by_char(state, false, bytewise) {
                    self.ding()?;
                }
                self.finish_vi_motion_operator(state, op_start, key, false);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::BackwardWord => {
                let _ = take_vi_motion_operator(state, command, key);
                let word_breaks = self.editing_word_breaks(hooks);
                repeat_signed(
                    state,
                    |state| {
                        state.buffer.backward_word(word_breaks.as_deref());
                    },
                    |state| {
                        state.buffer.forward_word(word_breaks.as_deref());
                    },
                );
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::BeginningOfLine => {
                let op_start = take_vi_motion_operator(state, command, key);
                state.buffer.move_beginning();
                self.finish_vi_motion_operator(state, op_start, key, false);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::EndOfLine => {
                let op_start = take_vi_motion_operator(state, command, key);
                state.buffer.move_end();
                // Vi command mode keeps point on the final character; insert
                // mode and emacs keep it at the buffer end.
                if matches!(self.keymap.current(), KeyMapName::ViCommand)
                    && !state.buffer.is_empty()
                {
                    if self.char_motion_is_bytewise() {
                        state.buffer.move_backward_byte();
                    } else {
                        state.buffer.move_backward();
                    }
                }
                self.finish_vi_motion_operator(state, op_start, key, true);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ExchangePointAndMark => {
                let _ = take_vi_motion_operator(state, command, key);
                let mark = state.mark.unwrap_or(0);
                let point = state.buffer.point();
                state.buffer.set_point(mark);
                state.mark = Some(point);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ForwardChar => {
                let op_start = take_vi_motion_operator(state, command, key);
                let bytewise = self.char_motion_is_bytewise();
                // A pending operator needs the end-of-line position to build
                // its range. Arrow keys cancel the operator above and keep the
                // clamped vi-command motion.
                let clamp_to_last_char =
                    matches!(self.keymap.current(), KeyMapName::ViCommand) && op_start.is_none();
                let moved = if clamp_to_last_char {
                    move_by_char_vi_forward(state, bytewise)
                } else {
                    move_by_char(state, true, bytewise)
                };
                if !moved {
                    self.ding()?;
                }
                self.finish_vi_motion_operator(state, op_start, key, false);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ForwardWord => {
                let _ = take_vi_motion_operator(state, command, key);
                let word_breaks = self.editing_word_breaks(hooks);
                repeat_signed(
                    state,
                    |state| {
                        state.buffer.forward_word(word_breaks.as_deref());
                    },
                    |state| {
                        state.buffer.backward_word(word_breaks.as_deref());
                    },
                );
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::SetMark => {
                let _ = take_vi_motion_operator(state, command, key);
                let mark = state
                    .numeric_arg
                    .take()
                    .map(|arg| (arg.max(0) as usize).min(state.buffer.len_chars()))
                    .unwrap_or_else(|| state.buffer.point());
                state.mark = Some(mark);
                state.after_non_kill_command();
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
    /// Character motion steps a single byte when `byte-oriented` is on or
    /// the locale is non-UTF-8 (measured Bash 5.3 `LC_ALL=C` PTY behavior);
    /// otherwise it keeps UTF-8 scalar/grapheme motion.
    pub(crate) fn char_motion_is_bytewise(&self) -> bool {
        char_motion_is_bytewise(
            self.flag(BoolVariable::ByteOriented),
            self.variables.utf8_locale(),
        )
    }

    pub(super) fn apply_named_movement_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        _key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "backward-byte" => {
                repeat(state, |state| {
                    state.buffer.move_backward_byte();
                });
                state.after_non_kill_command();
            }
            "forward-byte" => {
                repeat(state, |state| {
                    state.buffer.move_forward_byte();
                });
                state.after_non_kill_command();
            }
            "next-screen-line" => {
                let columns = self.tracked_terminal_columns(state);
                let prompt_width = self.current_prompt_width(state);
                let count = repeat_count(state.take_numeric_arg()) as isize;
                state
                    .buffer
                    .move_screen_line(prompt_width, columns, count, self.render_options());
                state.after_non_kill_command();
            }
            "previous-screen-line" => {
                let columns = self.tracked_terminal_columns(state);
                let prompt_width = self.current_prompt_width(state);
                let count = repeat_count(state.take_numeric_arg()) as isize;
                state
                    .buffer
                    .move_screen_line(prompt_width, columns, -count, self.render_options());
                state.after_non_kill_command();
            }
            "shell-backward-word" => {
                let arg = state.take_numeric_arg().unwrap_or(1);
                for _ in 0..arg.unsigned_abs() {
                    if arg < 0 {
                        shell_words::move_shell_forward_word(&mut state.buffer, hooks);
                    } else {
                        shell_words::move_shell_backward_word(&mut state.buffer, hooks);
                    }
                }
                state.after_non_kill_command();
            }
            "shell-forward-word" => {
                let arg = state.take_numeric_arg().unwrap_or(1);
                for _ in 0..arg.unsigned_abs() {
                    if arg < 0 {
                        shell_words::move_shell_backward_word(&mut state.buffer, hooks);
                    } else {
                        shell_words::move_shell_forward_word(&mut state.buffer, hooks);
                    }
                }
                state.after_non_kill_command();
            }
            _ => unreachable!("named command group mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }
}

/// Whether a key is the vi motion for one of the shared character/line
/// movement commands. GNU applies a pending operator only for the vi motion;
/// arrows, Home/End and other bindings of the same command cancel it.
fn take_vi_motion_operator(
    state: &mut EditorState,
    command: EditCommand,
    key: &[u8],
) -> Option<(ViOperator, usize, Vec<u8>)> {
    let operator = state.take_vi_operator();
    if operator.is_some() && !is_vi_motion_key(command, key) {
        // GNU drops the count along with the cancelled operator.
        state.take_numeric_arg();
        state.mark_vi_redo_cancelled_operator();
        return None;
    }
    operator
}

fn is_vi_motion_key(command: EditCommand, key: &[u8]) -> bool {
    match command {
        EditCommand::BackwardChar => key == b"h",
        EditCommand::ForwardChar => key == b"l" || key == b" ",
        EditCommand::BeginningOfLine => key == b"0",
        EditCommand::EndOfLine => key == b"$",
        _ => false,
    }
}

/// Whether character motion steps bytes rather than UTF-8 scalars/graphemes.
/// Pure helper so the locale/setting policy is unit-testable without a PTY.
fn char_motion_is_bytewise(byte_oriented: bool, utf8_locale: bool) -> bool {
    byte_oriented || !utf8_locale
}

/// Move point by characters honoring a signed numeric argument, like
/// `repeat_signed`: a negative argument moves the opposite way. Returns
/// false when the caller should ring once. Paired Bash 5.3 / Readline 8.3
/// fixtures show a direction asymmetry: a backward walk that cannot satisfy
/// its count rings even after moving, while a forward walk rings only when
/// no step at all was possible. A zero count or a satisfied count is silent.
/// Vi command mode keeps point on the last character. A counted motion that
/// runs out after moving at least once is silent; starting on that character
/// rings once without moving.
fn move_by_char_vi_forward(state: &mut EditorState, bytewise: bool) -> bool {
    let count = state.take_numeric_arg().unwrap_or(1).unsigned_abs();
    let mut moved = false;
    for _ in 0..count {
        let before = state.buffer.byte_point();
        let stepped = if bytewise {
            state.buffer.move_forward_byte()
        } else {
            state.buffer.move_forward()
        };
        if !stepped || state.buffer.byte_point() >= state.buffer.len_chars() {
            state.buffer.set_point(before);
            return moved;
        }
        moved = true;
    }
    true
}

fn move_by_char(state: &mut EditorState, forward: bool, bytewise: bool) -> bool {
    let arg = state.take_numeric_arg().unwrap_or(1);
    let moving_forward = forward != (arg < 0);
    let mut moved = false;
    for _ in 0..arg.unsigned_abs() {
        let stepped = if moving_forward {
            if bytewise {
                state.buffer.move_forward_byte()
            } else {
                state.buffer.move_forward()
            }
        } else if bytewise {
            state.buffer.move_backward_byte()
        } else {
            state.buffer.move_backward()
        };
        if !stepped {
            return moving_forward && moved;
        }
        moved = true;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::char_motion_is_bytewise;

    #[test]
    fn char_motion_steps_bytes_unless_utf8_without_byte_oriented() {
        assert!(!char_motion_is_bytewise(false, true));
        assert!(char_motion_is_bytewise(true, true));
        assert!(char_motion_is_bytewise(false, false));
        assert!(char_motion_is_bytewise(true, false));
    }
}
