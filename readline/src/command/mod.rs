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
mod shell_words;
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
                // Inputrc macro bindings ignore numeric arguments; clear the
                // prefix before any command in the expansion can consume it.
                state.take_numeric_arg();
                state.completion.menu_completion = None;
                state.record_macro_insert_bytes(&text);
                if state.macro_state.inputrc_macro_depth >= MAX_INPUTRC_MACRO_DEPTH {
                    return Ok(EditorOutcome::MacroOverflow);
                }
                let previous_replaying = state.macro_state.replaying_macro;
                state.macro_state.inputrc_macro_depth += 1;
                state.macro_state.replaying_macro = true;
                let result = self.handle_bytes(state, &text, hooks);
                state.macro_state.inputrc_macro_depth -= 1;
                state.macro_state.replaying_macro = previous_replaying;
                let outcome = result?;
                match outcome {
                    EditorOutcome::MacroOverflow => {
                        if state.macro_state.inputrc_macro_depth == 0 {
                            // Outermost inputrc macro frame: abort every
                            // active macro suffix, keep physical bytes queued
                            // by the caller, and report the limit once.
                            // Only the buffered key prefix can belong to the
                            // discarded macro suffix, so clear that pending
                            // dispatch state and leave other state intact.
                            state.input.pending_key.clear();
                            state.input.prefix_meta_command_len = 0;
                            self.write_below_rendered_line(
                                state,
                                "readline: maximum macro execution nesting level exceeded",
                            )?;
                            self.write_tracked_newline(state)?;
                            self.ding()?;
                            Ok(EditorOutcome::Continue)
                        } else {
                            Ok(EditorOutcome::MacroOverflow)
                        }
                    }
                    outcome => Ok(outcome),
                }
            }
            KeyBinding::ApplicationCommand(command) => {
                state.completion.menu_completion = None;
                let argument = state.take_numeric_arg();
                let context = CommandContext {
                    command: &command,
                    line: state.buffer.as_bytes(),
                    point: state.buffer.point(),
                    mark: state.mark,
                    argument,
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
