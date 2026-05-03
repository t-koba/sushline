use super::*;

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
            self.apply_vi_operator_range(state, op, start, inclusive);
            change.extend_from_slice(key);
            state.finish_vi_operator_change(op, change);
        }
    }

    pub(super) fn apply_movement_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        key: &[u8],
        hooks: &impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            EditCommand::BackwardChar => {
                let op_start = state.take_vi_operator();
                repeat_signed(
                    state,
                    |state| {
                        state.buffer.move_backward();
                    },
                    |state| {
                        state.buffer.move_forward();
                    },
                );
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
                repeat_signed(
                    state,
                    |state| {
                        state.buffer.move_forward();
                    },
                    |state| {
                        state.buffer.move_backward();
                    },
                );
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
    pub(super) fn apply_named_movement_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        _key: &[u8],
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
                let columns = state
                    .display
                    .last_terminal_size
                    .or_else(|| self.terminal.size().ok())
                    .map(|size| size.columns as usize)
                    .unwrap_or(80)
                    .max(1);
                let prompt_width = self.current_prompt_width(state);
                let count = state.numeric_arg.take().unwrap_or(1).unsigned_abs().max(1) as isize;
                state
                    .buffer
                    .move_screen_line(prompt_width, columns, count, self.render_options());
                state.after_non_kill_command();
            }
            "previous-screen-line" => {
                let columns = state
                    .display
                    .last_terminal_size
                    .or_else(|| self.terminal.size().ok())
                    .map(|size| size.columns as usize)
                    .unwrap_or(80)
                    .max(1);
                let prompt_width = self.current_prompt_width(state);
                let count = state.numeric_arg.take().unwrap_or(1).unsigned_abs().max(1) as isize;
                state
                    .buffer
                    .move_screen_line(prompt_width, columns, -count, self.render_options());
                state.after_non_kill_command();
            }
            "shell-backward-word" => {
                repeat_signed(
                    state,
                    |state| {
                        state.buffer.backward_command_word();
                    },
                    |state| {
                        state.buffer.forward_command_word();
                    },
                );
                state.after_non_kill_command();
            }
            "shell-forward-word" => {
                repeat_signed(
                    state,
                    |state| {
                        state.buffer.forward_command_word();
                    },
                    |state| {
                        state.buffer.backward_command_word();
                    },
                );
                state.after_non_kill_command();
            }
            _ => unreachable!("named command group mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }
}
