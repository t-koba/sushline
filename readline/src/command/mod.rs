use crate::buffer::LineBuffer;
use crate::editor::{Editor, EditorOutcome, ReadlineError};
use crate::hooks::{CommandContext, Hooks};
use crate::keymap::{EditCommand, KeyBinding, KeyMapName};
use crate::state::*;
use crate::terminal::TerminalIo;
use history::HistoryDirection;

mod completion_cmd;
mod editing;
mod history_nav;
mod kill;
mod misc;
mod movement;
mod named;
mod typed;
mod vi;

use completion_cmd::named_completion_command;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn apply_binding(
        &mut self,
        state: &mut EditorState,
        binding: KeyBinding,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        state.record_macro_binding(key, &binding);
        match binding {
            KeyBinding::Command(command) => self.apply_command(state, command, key, hooks),
            KeyBinding::NamedCommand(command) => {
                self.apply_named_command(state, &command, key, hooks)
            }
            KeyBinding::Macro(text) => {
                state.macro_state.replaying_macro = true;
                let outcome = self.handle_bytes(state, &text, hooks)?;
                state.macro_state.replaying_macro = false;
                Ok(outcome)
            }
            KeyBinding::ApplicationCommand(command) => {
                let context = CommandContext {
                    command: &command,
                    line: state.buffer.as_bytes(),
                    point: state.buffer.point(),
                    mark: state.mark,
                    argument: state.numeric_arg.take(),
                    key,
                    keymap: self.keymap.current(),
                };
                if let Some(edit) = hooks.on_command(context) {
                    state.record_undo();
                    if let Some(line) = edit.line {
                        state.buffer = LineBuffer::from_bytes(line);
                    }
                    if let Some(point) = edit.point {
                        state.buffer.set_point(point);
                    }
                    if let Some(mark) = edit.mark {
                        state.mark = mark;
                    }
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
        }
    }
}
