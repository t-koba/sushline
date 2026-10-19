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
                let op_start = state.take_vi_operator();
                let bytewise = self.char_motion_is_bytewise();
                if !move_by_char(state, false, bytewise) {
                    self.ding()?;
                }
                self.finish_vi_motion_operator(state, op_start, key, false);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::BackwardWord => {
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
                let op_start = state.take_vi_operator();
                state.buffer.move_beginning();
                self.finish_vi_motion_operator(state, op_start, key, false);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::EndOfLine => {
                let op_start = state.take_vi_operator();
                state.buffer.move_end();
                self.finish_vi_motion_operator(state, op_start, key, true);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ExchangePointAndMark => {
                if let Some(mark) = state.mark {
                    let point = state.buffer.point();
                    state.buffer.set_point(mark);
                    state.mark = Some(point);
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ForwardChar => {
                let op_start = state.take_vi_operator();
                let bytewise = self.char_motion_is_bytewise();
                if !move_by_char(state, true, bytewise) {
                    self.ding()?;
                }
                self.finish_vi_motion_operator(state, op_start, key, false);
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::ForwardWord => {
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
    fn char_motion_is_bytewise(&self) -> bool {
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

/// Whether character motion steps bytes rather than UTF-8 scalars/graphemes.
/// Pure helper so the locale/setting policy is unit-testable without a PTY.
fn char_motion_is_bytewise(byte_oriented: bool, utf8_locale: bool) -> bool {
    byte_oriented || !utf8_locale
}

/// Move point by characters honoring a signed numeric argument, like
/// `repeat_signed`: a negative argument moves the opposite way. Returns
/// false when the count is nonzero and any step could not move, in which
/// case the caller rings once (blocked movement rings in the tested Bash
/// fixtures); a zero count or a fully satisfied count stays silent.
fn move_by_char(state: &mut EditorState, forward: bool, bytewise: bool) -> bool {
    let arg = state.take_numeric_arg().unwrap_or(1);
    for _ in 0..arg.unsigned_abs() {
        let stepped = if forward != (arg < 0) {
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
            return false;
        }
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
