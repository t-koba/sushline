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
                if state.macro_state.keyboard_macro.is_some() {
                    state.abort_keyboard_macro_definition();
                }
                // Abort invalidates the completed definition in both the
                // active-recording and no-recording cases so a following call
                // cannot replay stale bytes. The helper above deliberately
                // preserves it for nested-start handling.
                state.macro_state.last_keyboard_macro = None;
                state.cancel_pending_command();
                if state.macro_state.inputrc_macro_depth > 0 {
                    return Ok(EditorOutcome::MacroAborted);
                }
                Ok(EditorOutcome::Continue)
            }
            EditCommand::AcceptLine => {
                Ok(EditorOutcome::Accepted(state.buffer.as_bytes().to_vec()))
            }
            EditCommand::CallLastKbdMacro => {
                if state.macro_state.keyboard_macro.is_some() {
                    state.macro_state.keyboard_macro = None;
                    state.macro_state.last_keyboard_macro = None;
                    state.macro_state.last_recorded_self_insert = false;
                    self.ding()?;
                    state.after_non_kill_command();
                    return Ok(EditorOutcome::Continue);
                }
                if let Some(macro_bytes) = state.macro_state.last_keyboard_macro.clone() {
                    let count = state.take_numeric_arg().unwrap_or(1);
                    if count <= 0 {
                        state.after_non_kill_command();
                        if count < 0 {
                            // The tested Bash 5.3 direct-call path reports the
                            // macro nesting diagnostic for a negative count.
                            self.write_diagnostic_below_rendered_line(
                                state,
                                "readline: maximum macro execution nesting level exceeded",
                            )?;
                            self.write_tracked_newline(state)?;
                            self.ding()?;
                        }
                        return Ok(EditorOutcome::Continue);
                    }
                    (|| -> Result<EditorOutcome, ReadlineError> {
                        let mut outcome = EditorOutcome::Continue;
                        for _ in 0..count as u32 {
                            outcome = self.replay_macro_bytes(state, &macro_bytes, hooks)?;
                            if !matches!(outcome, EditorOutcome::Continue) {
                                break;
                            }
                        }
                        Ok(outcome)
                    })()
                } else {
                    self.ding()?;
                    state.after_non_kill_command();
                    Ok(EditorOutcome::Continue)
                }
            }
            EditCommand::ClearScreen => {
                if state.take_numeric_arg().is_none() {
                    self.clear_display_and_reset(state)?;
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::DigitArgument => {
                if update_numeric_argument(state, key) {
                    self.ding()?;
                }
                Ok(EditorOutcome::Continue)
            }
            EditCommand::EndKbdMacro => {
                let count = state.numeric_arg.unwrap_or(1);
                let was_recording = state.macro_state.keyboard_macro.is_some();
                state.end_keyboard_macro();
                state.take_numeric_arg();
                state.after_non_kill_command();

                if !was_recording {
                    state.macro_state.last_keyboard_macro = None;
                    self.ding()?;
                    return Ok(EditorOutcome::Continue);
                }
                if count == 1 {
                    return Ok(EditorOutcome::Continue);
                }
                if count <= 0 {
                    state.macro_state.last_keyboard_macro = None;
                    self.write_diagnostic_below_rendered_line(
                        state,
                        "readline: maximum macro execution nesting level exceeded",
                    )?;
                    self.write_tracked_newline(state)?;
                    self.ding()?;
                    return Ok(EditorOutcome::Continue);
                }

                // The definition itself is the first execution. Reuse the
                // existing whole-macro replay path for the remaining count.
                state.numeric_arg = Some(count - 1);
                let previous_replaying = state.macro_state.replaying_macro;
                let outcome =
                    self.apply_misc_command(state, EditCommand::CallLastKbdMacro, key, hooks);
                state.macro_state.replaying_macro = previous_replaying;
                outcome
            }
            EditCommand::UniversalArgument => {
                // Explicit digit counts are already complete; only implicit
                // counts and sign-only prefixes are multiplied by four.
                if state.numeric_arg.is_none()
                    || state.numeric_arg_sign_only
                    || state.numeric_arg_universal
                {
                    state.numeric_arg_universal_noop = false;
                    match state.numeric_arg.unwrap_or(1).checked_mul(4) {
                        Some(next) => {
                            state.numeric_arg = Some(next);
                            state.numeric_arg_universal = true;
                        }
                        None => {
                            state.take_numeric_arg();
                            self.ding()?;
                        }
                    }
                } else {
                    state.numeric_arg_universal_noop = true;
                }
                Ok(EditorOutcome::Continue)
            }
            EditCommand::PrintLastKbdMacro => {
                if let Some(macro_bytes) = &state.macro_state.last_keyboard_macro {
                    let display =
                        crate::keymap::KeySequence::new(macro_bytes.clone()).display_inputrc_body();
                    self.write_below_rendered_line(state, &display)?;
                    self.write_tracked_newline(state)?;
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::StartKbdMacro => {
                // GNU `rl_start_kbd_macro` aborts when already defining
                // (`RL_STATE_MACRODEF`): ring once, drop the partial
                // definition, and consume the pending argument without
                // starting a fresh definition. A prior completed definition
                // stays intact; the measured nonempty nested-start fixture
                // leaves the following end stray and the call without a
                // definition. Explicit-argument appends only apply when not
                // already recording.
                if state.macro_state.keyboard_macro.is_some() {
                    self.ding()?;
                    state.abort_keyboard_macro_definition();
                    state.cancel_pending_command();
                    return Ok(EditorOutcome::Continue);
                }
                // Readline appends and replays the previous macro for an explicit
                // count or sign argument, but not for a universal-argument prefix.
                let append_previous = state.numeric_arg.is_some() && !state.numeric_arg_universal;
                state.take_numeric_arg();
                let previous_macro = state.start_keyboard_macro(append_previous);
                state.after_non_kill_command();
                if let Some(macro_bytes) = previous_macro {
                    // The replayed bytes belong to the new definition, so
                    // this dispatch keeps recording them like GNU Readline.
                    let (outcome, remainder) =
                        self.dispatch_macro_bytes(state, &macro_bytes, hooks)?;
                    if !matches!(outcome, EditorOutcome::Continue) {
                        self.pending_macro_replay = remainder;
                    }
                    state.macro_state.last_recorded_self_insert = false;
                    Ok(outcome)
                } else {
                    Ok(EditorOutcome::Continue)
                }
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
                self.clear_display_and_reset(state)?;
                state.after_non_kill_command();
            }
            "display-shell-version" => {
                if let Some(version) = hooks.version() {
                    self.write_below_rendered_line(state, &version)?;
                    self.write_tracked_newline(state)?;
                } else {
                    self.ding()?;
                }
                state.after_non_kill_command();
            }
            "do-lowercase-version" => {
                if let Some(last) = key.last().copied() {
                    let lower = last.to_ascii_lowercase();
                    if lower == last {
                        self.ding()?;
                        state.after_non_kill_command();
                        return Ok(EditorOutcome::Continue);
                    }
                    return self.handle_bytes(state, &[lower], hooks);
                }
                state.after_non_kill_command();
            }
            "dump-functions" => {
                let query = if state.take_numeric_arg().is_some() {
                    crate::bind::BindQuery::PrintReusable
                } else {
                    crate::bind::BindQuery::PrintFunctions
                };
                let output = self.bind_api().print(query);
                self.write_below_rendered_line(state, &output)?;
                state.after_non_kill_command();
            }
            "dump-macros" => {
                let query = if state.take_numeric_arg().is_some() {
                    crate::bind::BindQuery::PrintMacrosReusable
                } else {
                    crate::bind::BindQuery::PrintMacros
                };
                let output = self.bind_api().print(query);
                self.write_below_rendered_line(state, &output)?;
                state.after_non_kill_command();
            }
            "dump-variables" => {
                let query = if state.take_numeric_arg().is_some() {
                    crate::bind::BindQuery::PrintVariablesReusable
                } else {
                    crate::bind::BindQuery::PrintVariables
                };
                let output = self.bind_api().print(query);
                self.write_below_rendered_line(state, &output)?;
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
                    self.write_below_rendered_line(state, &status)?;
                    self.write_tracked_newline(state)?;
                } else {
                    self.ding()?;
                }
                state.after_non_kill_command();
            }
            _ => {
                self.ding()?;
                state.after_non_kill_command();
            }
        }
        Ok(EditorOutcome::Continue)
    }
}
