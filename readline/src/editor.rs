use crate::bind::BindApi;
use crate::buffer::LineBuffer;
use crate::config::{Config, InputrcPath};
use crate::hooks::{HistoryExpansionContext, Hooks};
use crate::inputrc::{InputrcDiagnostic, InputrcParser, discover_inputrc_path};
use crate::keymap::{EditCommand, KeyBinding, KeyMap, KeyMapName};
use crate::prompt::Prompt;
use crate::state::*;
use crate::terminal::{SigintPolicy, TerminalEvent, TerminalIo, escape};
use crate::variables::{BoolVariable, Variables};
use history::History;
use history::expansion::{
    HistoryChars, HistoryExpansion, HistoryExpansionPolicy, HistoryExpansionState,
    HistoryExpansionStatus,
};
use std::fmt;
use std::io;
use std::path::Path;
use std::time::Duration;

/// Outcome of one [`Editor::read_line`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadlineResult {
    /// A line accepted by the user.
    Line(Vec<u8>),
    /// The active read was interrupted before accepting a line, such as by Ctrl-C.
    ///
    /// Selected by [`SigintPolicy::Interrupted`]; the built-in terminal instead
    /// continues the read by default. The built-in terminal restores terminal
    /// and signal state before applying this policy. If it caught SIGINT with a
    /// saved non-default caller handler, it re-delivers the signal to that handler
    /// after restoration; with `SIG_DFL`, the configured policy handles the read
    /// without re-raising the signal. The embedder decides whether to retry an
    /// interrupted result.
    Interrupted,
    /// End-of-file was reached before a line was accepted.
    Eof,
}

#[derive(Debug)]
/// ReadlineError.
pub enum ReadlineError {
    /// Io.
    Io(io::Error),
    /// Inputrc.
    Inputrc(String),
}

impl fmt::Display for ReadlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Inputrc(err) => f.write_str(err),
        }
    }
}

impl std::error::Error for ReadlineError {}

impl From<io::Error> for ReadlineError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Format an inputrc diagnostic like GNU Readline's startup message.
fn format_inputrc_diagnostic(diagnostic: &InputrcDiagnostic) -> String {
    let mut text = String::from("readline: ");
    if let Some(path) = &diagnostic.path {
        text.push_str(&path.display().to_string());
        text.push_str(": ");
    }
    text.push_str(&format!(
        "line {}: {}: {}",
        diagnostic.line, diagnostic.variable, diagnostic.message
    ));
    text
}

/// Editor.
pub struct Editor<T>
where
    T: TerminalIo,
{
    config: Config,
    pub(crate) terminal: T,
    pub(crate) sigint_policy: SigintPolicy,
    pub(crate) history: History,
    pub(crate) keymap: KeyMap,
    pub(crate) variables: Variables,
    pub(crate) pending_initial_line: Option<Vec<u8>>,
    pub(crate) history_expansion_state: HistoryExpansionState,
    pub(crate) kill_ring: Vec<Vec<u8>>,
    pub(crate) last_keyboard_macro: Option<Vec<u8>>,
    pub(crate) keyboard_macro_recording: Option<Vec<u8>>,
    pub(crate) pending_macro_replay: Vec<u8>,
    pub(crate) last_incremental_search: Option<Vec<u8>>,
    pub(crate) last_incremental_search_direction: Option<SearchDirection>,
    pub(crate) last_non_incremental_search: Option<Vec<u8>>,
    pub(crate) last_non_incremental_search_direction: Option<SearchDirection>,
    pub(crate) last_non_incremental_search_match_index: Option<usize>,
    pub(crate) last_char_search: Option<(CharSearchMode, char)>,
    pub(crate) last_vi_change: Option<Vec<u8>>,
    initial_inputrc_error: Option<String>,
    inputrc_diagnostics: Vec<InputrcDiagnostic>,
    inputrc_report_pending: bool,
    /// Input that was read but not consumed yet.
    ///
    /// The bytes that follow an accepted line stay here, like GNU
    /// Readline's input stream, so the next read continues with them.
    pending_input: Vec<u8>,
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    /// New.
    pub fn new(config: Config, terminal: T, history: History) -> Self {
        let mut line = Self::new_without_inputrc(config, terminal, history);
        if let Err(err) = line.reload_inputrc() {
            line.initial_inputrc_error = Some(err.to_string());
        }
        line
    }

    /// Try new.
    pub fn try_new(config: Config, terminal: T, history: History) -> Result<Self, ReadlineError> {
        let mut line = Self::new_without_inputrc(config, terminal, history);
        line.reload_inputrc()?;
        Ok(line)
    }

    fn new_without_inputrc(config: Config, terminal: T, history: History) -> Self {
        let mut keymap = KeyMap::emacs_default();
        let variables = Variables::default_for_config(&config);
        keymap.set_current(match config.editing_mode {
            crate::config::EditingMode::Emacs => crate::keymap::KeyMapName::EmacsStandard,
            crate::config::EditingMode::Vi => crate::keymap::KeyMapName::ViInsert,
        });
        Self {
            config,
            sigint_policy: terminal.sigint_policy(),
            terminal,
            history,
            keymap,
            variables,
            pending_initial_line: None,
            history_expansion_state: HistoryExpansionState::new(),
            kill_ring: Vec::new(),
            last_keyboard_macro: None,
            keyboard_macro_recording: None,
            pending_macro_replay: Vec::new(),
            last_incremental_search: None,
            last_incremental_search_direction: None,
            last_non_incremental_search: None,
            last_non_incremental_search_direction: None,
            last_non_incremental_search_match_index: None,
            last_char_search: None,
            last_vi_change: None,
            initial_inputrc_error: None,
            inputrc_diagnostics: Vec::new(),
            inputrc_report_pending: false,
            pending_input: Vec::new(),
        }
    }

    /// Returns the current SIGINT and unquoted Ctrl-C policy.
    pub fn sigint_policy(&self) -> SigintPolicy {
        self.sigint_policy
    }

    /// Sets the SIGINT and unquoted Ctrl-C policy for subsequent reads.
    ///
    /// The initial value comes from [`TerminalIo::sigint_policy`]: `Continue`
    /// for the built-in terminal and `Interrupted` for custom backends unless
    /// they opt in. This does not change caller-owned OS signal dispositions
    /// or how non-SIGINT signals are handled.
    pub fn set_sigint_policy(&mut self, policy: SigintPolicy) {
        self.sigint_policy = policy;
    }

    /// Initial inputrc error.
    pub fn initial_inputrc_error(&self) -> Option<&str> {
        self.initial_inputrc_error.as_deref()
    }

    /// Bind api.
    pub fn bind_api(&mut self) -> BindApi<'_> {
        BindApi::with_config(&mut self.keymap, &mut self.variables, &self.config)
    }

    /// Terminal.
    pub fn terminal(&self) -> &T {
        &self.terminal
    }

    /// Terminal mut.
    pub fn terminal_mut(&mut self) -> &mut T {
        &mut self.terminal
    }

    /// History.
    pub fn history(&self) -> &History {
        &self.history
    }

    /// History mut.
    pub fn history_mut(&mut self) -> &mut History {
        &mut self.history
    }

    /// Variables.
    pub fn variables(&self) -> &Variables {
        &self.variables
    }

    /// Variables mut.
    pub fn variables_mut(&mut self) -> &mut Variables {
        &mut self.variables
    }

    /// Load inputrc str.
    pub fn load_inputrc_str(&mut self, source: &str) -> Result<(), ReadlineError> {
        let diagnostics = InputrcParser::new()
            .parse_str(source, &self.config, &mut self.keymap, &mut self.variables)
            .map_err(|e| ReadlineError::Inputrc(format!("line {}: {}", e.line, e.message)))?;
        self.set_inputrc_diagnostics(diagnostics);
        Ok(())
    }

    /// Load inputrc file.
    pub fn load_inputrc_file(&mut self, path: &Path) -> Result<(), ReadlineError> {
        let diagnostics = InputrcParser::new()
            .parse_file(path, &self.config, &mut self.keymap, &mut self.variables)
            .map_err(|e| ReadlineError::Inputrc(format!("line {}: {}", e.line, e.message)))?;
        self.set_inputrc_diagnostics(diagnostics);
        self.config.inputrc_path = InputrcPath::Path(path.to_path_buf());
        Ok(())
    }

    /// Inputrc diagnostics.
    ///
    /// The non-fatal problems of the last inputrc load, in the order GNU
    /// Readline reports them. The editor also prints them before the next
    /// prompt, like GNU Readline prints them while it starts up.
    pub fn inputrc_diagnostics(&self) -> &[InputrcDiagnostic] {
        &self.inputrc_diagnostics
    }

    fn set_inputrc_diagnostics(&mut self, diagnostics: Vec<InputrcDiagnostic>) {
        self.inputrc_report_pending = !diagnostics.is_empty();
        self.inputrc_diagnostics = diagnostics;
    }

    /// Print the pending inputrc diagnostics, like GNU Readline's startup
    /// messages, and keep them available through
    /// [`Self::inputrc_diagnostics`].
    fn report_inputrc_diagnostics(&mut self) -> Result<(), ReadlineError> {
        if !self.inputrc_report_pending {
            return Ok(());
        }
        self.inputrc_report_pending = false;
        for diagnostic in &self.inputrc_diagnostics {
            self.terminal
                .write_diagnostic(&format_inputrc_diagnostic(diagnostic))?;
            self.terminal.write_diagnostic("\n")?;
        }
        self.terminal.flush()?;
        Ok(())
    }

    /// Reload inputrc.
    pub fn reload_inputrc(&mut self) -> Result<(), ReadlineError> {
        let path = match &self.config.inputrc_path {
            InputrcPath::Disabled => return Ok(()),
            InputrcPath::Path(path) => path.clone(),
            InputrcPath::Discover => discover_inputrc_path(),
        };
        match self.load_inputrc_file(&path) {
            Ok(()) => Ok(()),
            Err(ReadlineError::Io(err)) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }

    /// Cleanup after signal.
    pub fn cleanup_after_signal(&mut self) -> Result<(), ReadlineError> {
        if self.flag(BoolVariable::EnableBracketedPaste) {
            let _ = self.terminal.write(escape::BRACKETED_PASTE_OFF);
            let _ = self.terminal.flush();
        }
        if self.flag(BoolVariable::EnableMetaKey) {
            let _ = self.terminal.set_meta_key_enabled(false);
            let _ = self.terminal.flush();
        }
        if self.flag(BoolVariable::EnableKeypad) {
            let _ = self.terminal.set_application_keypad_enabled(false);
            let _ = self.terminal.flush();
        }
        self.terminal.restore_mode().map_err(ReadlineError::Io)
    }

    /// Reset after signal.
    pub fn reset_after_signal(&mut self) -> Result<(), ReadlineError> {
        self.terminal
            .set_input_meta_enabled(self.flag(BoolVariable::InputMeta))?;
        self.terminal.enter_raw_mode()?;
        self.terminal
            .set_meta_key_enabled(self.flag(BoolVariable::EnableMetaKey))?;
        if self.flag(BoolVariable::EnableKeypad) {
            self.terminal.set_application_keypad_enabled(true)?;
        }
        if self.flag(BoolVariable::EnableBracketedPaste) {
            self.terminal.write(escape::BRACKETED_PASTE_ON)?;
            self.terminal.flush()?;
        }
        Ok(())
    }

    fn restore_persistent_state(&self, state: &mut EditorState) {
        // Completed kill, macro, incremental/non-incremental search memories,
        // and vi state live on `Editor` across reads while active searches,
        // pending operators, counts,
        // marks, and insert recording stay per-line in the fresh state.
        // An unfinished keyboard-macro recording also lives on `Editor`:
        // GNU Readline keeps recording across prompts, so a definition
        // started in one read continues in the next.
        state.kill.kill_ring = self.kill_ring.clone();
        state.macro_state.last_keyboard_macro = self.last_keyboard_macro.clone();
        state.macro_state.keyboard_macro = self.keyboard_macro_recording.clone();
        state.search.last_incremental_search = self.last_incremental_search.clone();
        state.search.last_incremental_search_direction = self.last_incremental_search_direction;
        state.search.last_non_incremental_search = self.last_non_incremental_search.clone();
        state.search.last_non_incremental_search_direction =
            self.last_non_incremental_search_direction;
        state.search.last_non_incremental_search_match_index =
            self.last_non_incremental_search_match_index;
        state.vi.last_char_search = self.last_char_search;
        state.vi.last_vi_change = self.last_vi_change.clone();
    }

    fn save_persistent_state(&mut self, state: &EditorState) {
        self.kill_ring = state.kill.kill_ring.clone();
        self.last_keyboard_macro = state.macro_state.last_keyboard_macro.clone();
        self.keyboard_macro_recording = state.macro_state.keyboard_macro.clone();
        self.last_incremental_search = state.search.last_incremental_search.clone();
        self.last_incremental_search_direction = state.search.last_incremental_search_direction;
        self.last_non_incremental_search = state.search.last_non_incremental_search.clone();
        self.last_non_incremental_search_direction =
            state.search.last_non_incremental_search_direction;
        self.last_non_incremental_search_match_index =
            state.search.last_non_incremental_search_match_index;
        self.last_char_search = state.vi.last_char_search;
        self.last_vi_change = state.vi.last_vi_change.clone();
    }

    /// Read line.
    pub fn read_line(
        &mut self,
        prompt: Prompt,
        hooks: &mut impl Hooks,
    ) -> Result<ReadlineResult, ReadlineError> {
        self.bind_tty_special_chars();
        if let Err(setup_err) = self.reset_after_signal() {
            let _ = self.cleanup_after_signal();
            return Err(setup_err);
        }
        let result = self.read_line_raw(prompt, hooks);
        let restore_result = self.cleanup_after_signal();
        match (result, restore_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(err), _) => Err(err),
            (Ok(_), Err(err)) => Err(err),
        }
    }

    fn read_line_raw(
        &mut self,
        prompt: Prompt,
        hooks: &mut impl Hooks,
    ) -> Result<ReadlineResult, ReadlineError> {
        self.reset_runtime_keymap_for_new_line();
        let mut state = EditorState::new(prompt, self.pending_initial_line.take());
        self.restore_persistent_state(&mut state);
        self.report_inputrc_diagnostics()?;
        self.render(&mut state)?;

        // A keyboard-macro replay that accepted a line continues in this
        // prompt, like GNU Readline, which keeps the macro bytes in the
        // input stream across the accept-line that ended the previous read.
        if !self.pending_macro_replay.is_empty() {
            let remaining = std::mem::take(&mut self.pending_macro_replay);
            let outcome = self.replay_macro_bytes(&mut state, &remaining, hooks)?;
            if !matches!(outcome, EditorOutcome::Continue) {
                return self.finish_outcome(&mut state, outcome);
            }
            self.render(&mut state)?;
        }

        loop {
            if let Some(signal) = hooks.check_signals() {
                if let Some(result) = self.handle_checked_signal(&mut state, signal)? {
                    self.save_persistent_state(&state);
                    return Ok(result);
                }
                self.render(&mut state)?;
                continue;
            }

            let event = match self.read_input_event(self.keyseq_timeout()) {
                Ok(event) => event,
                Err(error)
                    if state.paste.bracketed_paste
                        && error.kind() != io::ErrorKind::Interrupted =>
                {
                    // The nested Readline paste read ends on a negative key, so
                    // expose its accumulated bytes before reporting a fatal error.
                    self.finish_bracketed_paste(&mut state);
                    self.render(&mut state)?;
                    if error.kind() == io::ErrorKind::UnexpectedEof {
                        continue;
                    }
                    return Err(error.into());
                }
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                    return self.finish_outcome(&mut state, EditorOutcome::Eof);
                }
                Err(error) => return Err(error.into()),
            };

            match event {
                TerminalEvent::Timeout => {
                    if !state.input.pending_key.is_empty() {
                        let pending = std::mem::take(&mut state.input.pending_key);
                        let outcome = self.handle_unbound(&mut state, &pending, hooks)?;
                        if !matches!(outcome, EditorOutcome::Continue) {
                            return self.finish_outcome(&mut state, outcome);
                        }
                        self.render(&mut state)?;
                    }
                    continue;
                }
                TerminalEvent::Resize(size) => {
                    state.display.last_terminal_size = Some(size);
                    self.render(&mut state)?;
                    continue;
                }
                TerminalEvent::Signal(signal) => {
                    if let Some(result) = self.handle_terminal_signal(&mut state, signal)? {
                        self.save_persistent_state(&state);
                        return Ok(result);
                    }
                    self.render(&mut state)?;
                    continue;
                }
                TerminalEvent::Bytes(bytes) if bytes.is_empty() => continue,
                TerminalEvent::Bytes(bytes) => {
                    let (outcome, remainder, drawn) =
                        self.dispatch_input_bytes(&mut state, &bytes, hooks)?;
                    if !remainder.is_empty() {
                        // The bytes that follow an accepted line stay in
                        // the input stream, like GNU Readline, so the next
                        // read continues with them.
                        self.pending_input.extend(remainder);
                    }
                    if state.input.interrupted {
                        self.save_persistent_state(&state);
                        return Ok(ReadlineResult::Interrupted);
                    }
                    if matches!(outcome, EditorOutcome::Continue) {
                        // The batch's text is drawn when the batch ends,
                        // unless a key already drew it.
                        if !drawn || state.display.pending_text {
                            self.render(&mut state)?;
                        }
                    } else {
                        return self.finish_outcome(&mut state, outcome);
                    }
                }
            }
        }
    }

    fn handle_checked_signal(
        &mut self,
        state: &mut EditorState,
        signal: i32,
    ) -> Result<Option<ReadlineResult>, ReadlineError> {
        // Hook-observed signals take the same restore/echo/reraise path as
        // terminal signals so `^C` output and raw-mode re-entry agree.
        self.handle_terminal_signal(state, signal)
    }

    pub(crate) fn echo_signal_interrupt(
        &mut self,
        state: &mut EditorState,
    ) -> Result<(), ReadlineError> {
        if state.display.rendered_cursor_row > 0 {
            self.terminal.move_up(state.display.rendered_cursor_row)?;
        }
        self.terminal.move_to_column(0)?;
        self.terminal.clear_to_screen_end()?;
        let (prompt, _) = self.effective_prompt(state);
        self.terminal
            .write_bytes(&crate::buffer::rendered_string_to_bytes(&prompt))?;
        self.terminal.write("^C\r\n")?;
        self.terminal.flush()?;
        state.display.rendered_rows = 0;
        state.display.rendered_cursor_row = 0;
        Ok(())
    }

    fn finish_outcome(
        &mut self,
        state: &mut EditorState,
        outcome: EditorOutcome,
    ) -> Result<ReadlineResult, ReadlineError> {
        self.save_persistent_state(state);
        match outcome {
            EditorOutcome::Continue
            | EditorOutcome::MacroOverflow
            | EditorOutcome::MacroAborted => {
                unreachable!("continue outcomes are handled in the read loop")
            }
            EditorOutcome::Accepted(bytes) => {
                self.move_below_rendered_line(state)?;
                self.terminal.flush()?;
                if self.flag(BoolVariable::RevertAllAtNewline) {
                    self.history.revert_current_edit();
                }
                // Explicit History insertions allow empty entries; automatic
                // editor history keeps its separate empty-line suppression.
                if self.config.auto_add_history {
                    if bytes.is_empty() {
                        self.history.reset_cursor();
                    } else {
                        self.history.push_bytes(bytes.clone());
                        self.history.reset_cursor();
                    }
                    self.history.enforce_max_len(self.history_size_limit());
                }
                Ok(ReadlineResult::Line(bytes))
            }
            EditorOutcome::Eof => {
                self.move_below_rendered_line(state)?;
                self.terminal.flush()?;
                Ok(ReadlineResult::Eof)
            }
        }
    }

    fn bind_tty_special_chars(&mut self) {
        if !self.flag(BoolVariable::BindTtySpecialChars) {
            return;
        }
        for (byte, command_name) in self.terminal.tty_special_bindings() {
            let command = if command_name == "end-of-file" {
                EditCommand::DeleteChar
            } else if let Some(command) = EditCommand::parse(command_name) {
                command
            } else {
                continue;
            };
            let sequence = crate::keymap::KeySequence::new(vec![byte]);
            for map in self.tty_special_binding_maps() {
                if self.keymap.is_user_binding(map, &sequence) {
                    continue;
                }
                let binding = if command_name == "end-of-file"
                    && matches!(map, KeyMapName::ViCommand | KeyMapName::ViInsert)
                {
                    KeyBinding::NamedCommand("vi-eof-maybe".to_string())
                } else {
                    KeyBinding::Command(command)
                };
                self.keymap.bind(map, sequence.clone(), binding);
            }
        }
    }

    fn reset_runtime_keymap_for_new_line(&mut self) {
        let mode = self
            .variables
            .get("editing-mode")
            .map(String::as_str)
            .unwrap_or(match self.config.editing_mode {
                crate::config::EditingMode::Emacs => "emacs",
                crate::config::EditingMode::Vi => "vi",
            });
        self.keymap.set_current(match mode {
            "vi" => KeyMapName::ViInsert,
            _ => KeyMapName::EmacsStandard,
        });
    }

    fn tty_special_binding_maps(&self) -> Vec<KeyMapName> {
        if self
            .variables
            .get("editing-mode")
            .map(String::as_str)
            .unwrap_or(match self.config.editing_mode {
                crate::config::EditingMode::Emacs => "emacs",
                crate::config::EditingMode::Vi => "vi",
            })
            == "vi"
        {
            vec![KeyMapName::ViInsert, KeyMapName::ViCommand]
        } else {
            vec![KeyMapName::EmacsStandard]
        }
    }

    pub(crate) fn effective_editing_mode_is_vi(&self) -> bool {
        self.variables
            .get("editing-mode")
            .map(String::as_str)
            .unwrap_or(match self.config.editing_mode {
                crate::config::EditingMode::Emacs => "emacs",
                crate::config::EditingMode::Vi => "vi",
            })
            == "vi"
    }

    /// Whether consecutive kills may coalesce; GNU appends only outside vi mode.
    pub(crate) fn kill_append_allowed(&self) -> bool {
        !self.effective_editing_mode_is_vi()
    }

    pub(crate) fn variable_is_on(&self, name: &str) -> bool {
        self.variables.is_on(name)
    }

    pub(crate) fn flag(&self, variable: BoolVariable) -> bool {
        self.variables.flag(variable)
    }

    pub(crate) fn search_locale_lowercase(
        &self,
        ignore_case: bool,
    ) -> Option<crate::locale::LocaleLowercase> {
        if ignore_case && self.variables.utf8_locale() {
            self.variables
                .lc_ctype()
                .and_then(crate::locale::LocaleLowercase::new)
        } else {
            None
        }
    }

    fn histchars(&self) -> HistoryChars {
        // GNU has no `histchars` inputrc variable; the characters are fixed.
        HistoryChars::parse("!^#")
    }

    fn history_expansion_policy(&self) -> HistoryExpansionPolicy {
        // GNU has no inputrc variables for the remaining policy fields;
        // embedders configure them per call through `HistoryExpansionPolicy`.
        // The Readline editor keeps single-quote inhibition for the measured
        // Bash prompt path; the pure History API has a distinct direct-library
        // default that expands the pinned `echo '!!` input.
        HistoryExpansionPolicy {
            quick_substitution: false,
            quotes_inhibit_expansion: true,
            ..HistoryExpansionPolicy::default()
        }
    }

    pub(crate) fn ding(&mut self) -> Result<(), ReadlineError> {
        match self
            .variables
            .get("bell-style")
            .map(String::as_str)
            .unwrap_or("audible")
        {
            "none" => {}
            "visible" => self.terminal.visible_bell()?,
            "audible" => self.terminal.write("\x07")?,
            _ if self.variable_is_on("prefer-visible-bell") => self.terminal.visible_bell()?,
            _ => self.terminal.write("\x07")?,
        }
        Ok(())
    }

    pub(crate) fn expand_history_line(
        &mut self,
        line: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<HistoryExpansion, String> {
        let histchars = self.histchars();
        let policy = self.history_expansion_policy();
        // Split disjoint `Editor` fields so substitution memory persists
        // across expansions and reads while history stays shared.
        let Editor {
            history,
            history_expansion_state,
            ..
        } = self;
        hooks.expand_history(HistoryExpansionContext {
            line,
            history,
            histchars,
            policy: &policy,
            expansion_state: history_expansion_state,
        })
    }

    pub(crate) fn try_expand_history(
        &mut self,
        state: &mut EditorState,
        line: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<Option<Vec<u8>>, ReadlineError> {
        match self.expand_history_line(line, hooks) {
            Ok(expanded) if expanded.status == HistoryExpansionStatus::PrintOnly => {
                // The print-only result is not error text, but GNU writes it
                // to standard error as well, so it takes the same sink.
                let result = String::from_utf8_lossy(&expanded.line);
                self.report_history_expansion_message(state, &result)?;
                state.after_non_kill_command();
                Ok(None)
            }
            Ok(expanded) => Ok(Some(expanded.line)),
            Err(message) => {
                self.report_history_expansion_message(state, &message)?;
                state.after_non_kill_command();
                Ok(None)
            }
        }
    }

    /// Report a history expansion message.
    ///
    /// A failure is error text, so it goes to the diagnostic sink with the
    /// other error families. The print-only result is not error text, but
    /// GNU writes it to standard error as well, so it takes the same sink.
    pub(crate) fn report_history_expansion_message(
        &mut self,
        state: &mut EditorState,
        message: &str,
    ) -> Result<(), ReadlineError> {
        self.write_diagnostic_below_rendered_line(state, message)?;
        self.terminal.flush()?;
        Ok(())
    }

    fn history_size_limit(&self) -> Option<usize> {
        self.variables
            .get("history-size")
            .and_then(|value| value.parse::<isize>().ok())
            .and_then(|value| (value >= 0).then_some(value as usize))
    }

    pub(crate) fn read_input_event(
        &mut self,
        timeout: Option<Duration>,
    ) -> io::Result<TerminalEvent> {
        if !self.pending_input.is_empty() {
            return Ok(TerminalEvent::Bytes(std::mem::take(
                &mut self.pending_input,
            )));
        }
        self.terminal.read_event(timeout)
    }

    pub(crate) fn read_completion_response(&mut self) -> io::Result<TerminalEvent> {
        let event = self.read_input_event(None)?;
        match event {
            TerminalEvent::Bytes(bytes) if bytes.len() > 1 => {
                let first = bytes[0];
                self.pending_input.extend(bytes[1..].iter().copied());
                Ok(TerminalEvent::Bytes(vec![first]))
            }
            event => Ok(event),
        }
    }

    fn keyseq_timeout(&self) -> Option<Duration> {
        let timeout = self
            .variables
            .get("keyseq-timeout")
            .and_then(|value| value.parse::<isize>().ok())
            .unwrap_or(self.config.keyseq_timeout_ms as isize);
        (timeout > 0).then(|| Duration::from_millis(timeout as u64))
    }

    /// Replace the buffer with a history line, preserving the edit left behind.
    ///
    /// The current history cursor normally identifies the entry represented by
    /// `line`. Search overlays may display another entry without moving that
    /// cursor, so callers with an overlay use `replace_from_history_at`.
    pub(crate) fn replace_from_history(&mut self, state: &mut EditorState, line: &[u8]) {
        let target = self.history.current_index();
        self.replace_from_history_at(state, target, line);
    }

    /// Replace the buffer with a line from an explicit history entry.
    ///
    /// Before changing the displayed entry, persist the current buffer and its
    /// undo state to the entry that owns it. This remains distinct from the
    /// history cursor when an incremental or non-incremental search overlays
    /// a match without moving the cursor.
    pub(crate) fn replace_from_history_at(
        &mut self,
        state: &mut EditorState,
        target: Option<usize>,
        line: &[u8],
    ) {
        let target = target.filter(|index| self.history.get(*index).is_some());
        let source = state.history_entry_index;
        let source_is_current = source.is_some_and(|index| self.history.get(index).is_some());
        let same_source = source_is_current && source == target;

        if !self.flag(BoolVariable::RevertAllAtNewline) && source_is_current {
            state.commit_pending_undo();
            if !same_source && let Some(index) = source {
                self.history
                    .set_line_bytes(index, state.buffer.as_bytes().to_vec());
            }
            if let Some(index) = source {
                self.history
                    .set_undo_list(index, state.undo_snapshot_lines());
            }
        }

        // A line already owned by this entry is a no-op even when the buffer
        // contains an edit newer than the stored line.
        if same_source {
            return;
        }

        let point = state.buffer.point();
        state.buffer = LineBuffer::from_bytes(line.to_vec());
        state.original_line = line.to_vec();
        state.history_entry_index = target;
        state.undo.undo_stack.clear();
        state.undo.pending_undo = None;
        if !self.flag(BoolVariable::RevertAllAtNewline)
            && let Some(index) = target
            && let Some(undo_list) = self.history.undo_list(index)
        {
            state.restore_undo_snapshot_lines(undo_list);
        }
        if self.flag(BoolVariable::HistoryPreservePoint) {
            state.buffer.set_point(point);
        }
    }

    pub(crate) fn is_isearch_terminator(&self, bytes: &[u8]) -> bool {
        let terminators = self
            .variables
            .get_bytes("isearch-terminators")
            .map(Vec::as_slice)
            .unwrap_or(b"\x1b\n");
        bytes.len() == 1 && terminators.contains(&bytes[0])
    }
}

pub(crate) enum EditorOutcome {
    /// Continue.
    Continue,
    /// Accepted.
    Accepted(Vec<u8>),
    /// Eof.
    Eof,
    /// Inputrc macro nesting limit reached; unwinds active macro suffixes.
    MacroOverflow,
    /// Abort inside an inputrc macro; unwinds active macro suffixes.
    MacroAborted,
}

#[cfg(test)]
mod tests;
