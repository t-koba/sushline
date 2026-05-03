use super::*;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_misc_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            EditCommand::Abort => {
                self.ding()?;
                state.cancel_pending_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::AcceptLine => {
                Ok(EditorOutcome::Accepted(state.buffer.as_bytes().to_vec()))
            }
            EditCommand::CallLastKbdMacro => {
                if let Some(macro_bytes) = state.macro_state.last_keyboard_macro.clone() {
                    state.macro_state.replaying_macro = true;
                    let outcome = self.handle_bytes(state, &macro_bytes, hooks)?;
                    state.macro_state.replaying_macro = false;
                    Ok(outcome)
                } else {
                    state.after_non_kill_command();
                    Ok(EditorOutcome::Continue)
                }
            }
            EditCommand::ClearScreen => {
                if state.numeric_arg.take().is_none() {
                    self.terminal.clear_display()?;
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::DigitArgument => {
                update_numeric_argument(state, key);
                Ok(EditorOutcome::Continue)
            }
            EditCommand::EndKbdMacro => {
                state.end_keyboard_macro();
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::UniversalArgument => {
                state.numeric_arg = Some(state.numeric_arg.unwrap_or(1) * 4);
                Ok(EditorOutcome::Continue)
            }
            EditCommand::PrintLastKbdMacro => {
                if let Some(macro_bytes) = &state.macro_state.last_keyboard_macro {
                    let display =
                        crate::keymap::KeySequence::new(macro_bytes.clone()).display_inputrc();
                    self.terminal.write(&display)?;
                    self.terminal.write("\r\n")?;
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::StartKbdMacro => {
                state.start_keyboard_macro();
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::PrefixMeta => {
                state.input.prefix_meta = true;
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::Unknown => {
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
    pub(super) fn apply_named_misc_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "arrow-key-prefix" | "skip-csi-sequence" => {
                state.input.skipping_csi = true;
                state.after_non_kill_command();
            }
            "clear-display" | "redraw-current-line" => {
                self.terminal.clear_display()?;
                state.after_non_kill_command();
            }
            "display-shell-version" => {
                if let Some(version) = hooks.version() {
                    self.terminal.write("\r\n")?;
                    self.terminal.write(&version)?;
                    self.terminal.write("\r\n")?;
                } else {
                    self.ding()?;
                }
                state.after_non_kill_command();
            }
            "do-lowercase-version" => {
                if let Some(last) = key.last().copied() {
                    let lower = last.to_ascii_lowercase();
                    return self.handle_bytes(state, &[lower], hooks);
                }
                state.after_non_kill_command();
            }
            "dump-functions" => {
                let query = if state.numeric_arg.take().is_some() {
                    crate::bind::BindQuery::PrintReusable
                } else {
                    crate::bind::BindQuery::PrintFunctions
                };
                let output = self.bind_api().print(query);
                self.terminal.write(&output)?;
                state.after_non_kill_command();
            }
            "dump-macros" => {
                let query = if state.numeric_arg.take().is_some() {
                    crate::bind::BindQuery::PrintMacrosReusable
                } else {
                    crate::bind::BindQuery::PrintMacros
                };
                let output = self.bind_api().print(query);
                self.terminal.write(&output)?;
                state.after_non_kill_command();
            }
            "dump-variables" => {
                let query = if state.numeric_arg.take().is_some() {
                    crate::bind::BindQuery::PrintVariablesReusable
                } else {
                    crate::bind::BindQuery::PrintVariables
                };
                let output = self.bind_api().print(query);
                self.terminal.write(&output)?;
                state.after_non_kill_command();
            }
            "emacs-editing-mode" => {
                self.keymap.set_current(KeyMapName::EmacsStandard);
                self.variables
                    .insert("editing-mode".to_string(), "emacs".to_string());
                self.variables
                    .insert("keymap".to_string(), "emacs".to_string());
                state.after_non_kill_command();
            }
            "execute-named-command" => {
                state.input.named_command = Some(String::new());
            }
            "re-read-init-file" => {
                self.reload_inputrc()?;
                state.after_non_kill_command();
            }
            "tty-status" => {
                if let Some(status) = hooks.tty_status() {
                    self.terminal.write("\r\n")?;
                    self.terminal.write(&status)?;
                    self.terminal.write("\r\n")?;
                } else {
                    self.ding()?;
                }
                state.after_non_kill_command();
            }
            _ => {
                state.after_non_kill_command();
            }
        }
        Ok(EditorOutcome::Continue)
    }
}
