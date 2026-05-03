use super::*;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_kill_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        _key: &[u8],
        hooks: &impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            EditCommand::BackwardKillLine => {
                state.record_undo();
                if state.numeric_arg.take().unwrap_or(1) < 0 {
                    let killed = state.buffer.kill_to_end();
                    state.push_kill(killed, KillDirection::Forward);
                } else {
                    let killed = state.buffer.kill_to_start();
                    state.push_kill(killed, KillDirection::Backward);
                };
                Ok(EditorOutcome::Continue)
            }
            EditCommand::UnixWordRubout => {
                state.record_undo();
                let killed = state.buffer.unix_word_rubout();
                state.push_kill(killed, KillDirection::Backward);
                Ok(EditorOutcome::Continue)
            }
            EditCommand::BackwardKillWord => {
                state.record_undo();
                let mut killed = Vec::new();
                let word_breaks = self.editing_word_breaks(hooks);
                let direction = repeat_signed_collect_bytes(
                    state,
                    |state| state.buffer.backward_kill_word(word_breaks.as_deref()),
                    |state| state.buffer.kill_word(word_breaks.as_deref()),
                    |part, out| {
                        out.splice(0..0, part);
                    },
                    |part, out| out.extend(part),
                    (KillDirection::Backward, KillDirection::Forward),
                    &mut killed,
                );
                state.push_kill(killed, direction);
                Ok(EditorOutcome::Continue)
            }
            EditCommand::CopyRegionAsKill => {
                if let Some((start, end)) = state.region_bounds() {
                    state.push_kill(state.buffer.range_bytes(start, end), KillDirection::Forward);
                } else {
                    state.after_non_kill_command();
                }
                Ok(EditorOutcome::Continue)
            }
            EditCommand::KillLine => {
                state.record_undo();
                if state.numeric_arg.take().unwrap_or(1) < 0 {
                    let killed = state.buffer.kill_to_start();
                    state.push_kill(killed, KillDirection::Backward);
                } else {
                    let killed = state.buffer.kill_to_end();
                    state.push_kill(killed, KillDirection::Forward);
                }
                Ok(EditorOutcome::Continue)
            }
            EditCommand::KillRegion => {
                if let Some((start, end)) = state.region_bounds() {
                    state.record_undo();
                    let killed = state.buffer.delete_range_bytes(start, end);
                    state.mark = None;
                    state.push_kill(killed, KillDirection::Forward);
                } else {
                    state.after_non_kill_command();
                }
                Ok(EditorOutcome::Continue)
            }
            EditCommand::KillWholeLine => {
                state.record_undo();
                let killed = state.buffer.kill_whole_line();
                state.push_kill(killed, KillDirection::Forward);
                Ok(EditorOutcome::Continue)
            }
            EditCommand::KillWord => {
                state.record_undo();
                let mut killed = Vec::new();
                let word_breaks = self.editing_word_breaks(hooks);
                let direction = repeat_signed_collect_bytes(
                    state,
                    |state| state.buffer.kill_word(word_breaks.as_deref()),
                    |state| state.buffer.backward_kill_word(word_breaks.as_deref()),
                    |part, out| out.extend(part),
                    |part, out| {
                        out.splice(0..0, part);
                    },
                    (KillDirection::Forward, KillDirection::Backward),
                    &mut killed,
                );
                state.push_kill(killed, direction);
                Ok(EditorOutcome::Continue)
            }
            EditCommand::UnixLineDiscard => {
                state.record_undo();
                let killed = state.buffer.kill_to_start();
                state.push_kill(killed, KillDirection::Backward);
                Ok(EditorOutcome::Continue)
            }
            EditCommand::Yank => {
                state.record_undo();
                state.yank();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::YankPop => {
                state.record_undo();
                state.yank_pop();
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
    pub(super) fn apply_named_kill_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        _key: &[u8],
        hooks: &impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "copy-backward-word" => {
                let word_breaks = self.editing_word_breaks(hooks);
                let text = state.buffer.copy_backward_word(word_breaks.as_deref());
                state.push_kill(text, KillDirection::Backward);
            }
            "copy-forward-word" => {
                let word_breaks = self.editing_word_breaks(hooks);
                let text = state.buffer.copy_forward_word(word_breaks.as_deref());
                state.push_kill(text, KillDirection::Forward);
            }
            "insert-last-argument" | "yank-last-arg" => {
                state.record_undo();
                let arg = state.numeric_arg.take();
                self.yank_history_arg(state, arg, hooks)?;
                state.after_non_kill_command();
            }
            "yank-nth-arg" | "vi-yank-arg" => {
                state.record_undo();
                let n = state.numeric_arg.take().unwrap_or(1);
                self.yank_history_arg(state, Some(n), hooks)?;
                state.after_non_kill_command();
            }
            "shell-backward-kill-word" => {
                state.record_undo();
                let mut killed = Vec::new();
                let direction = repeat_signed_collect_bytes(
                    state,
                    |state| state.buffer.backward_kill_command_word(),
                    |state| state.buffer.kill_command_word(),
                    |part, out| {
                        out.splice(0..0, part);
                    },
                    |part, out| out.extend(part),
                    (KillDirection::Backward, KillDirection::Forward),
                    &mut killed,
                );
                state.push_kill(killed, direction);
            }
            "shell-kill-word" => {
                state.record_undo();
                let mut killed = Vec::new();
                let direction = repeat_signed_collect_bytes(
                    state,
                    |state| state.buffer.kill_command_word(),
                    |state| state.buffer.backward_kill_command_word(),
                    |part, out| out.extend(part),
                    |part, out| {
                        out.splice(0..0, part);
                    },
                    (KillDirection::Forward, KillDirection::Backward),
                    &mut killed,
                );
                state.push_kill(killed, direction);
            }
            "unix-filename-rubout" => {
                state.record_undo();
                let killed = state.buffer.backward_kill_filename_word();
                state.push_kill(killed, KillDirection::Backward);
            }
            "vi-unix-word-rubout" => {
                state.record_undo();
                let killed = state.buffer.unix_word_rubout();
                state.push_kill(killed, KillDirection::Backward);
            }
            _ => unreachable!("named command group mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }
}
