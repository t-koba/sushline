use crate::bind::BindApi;
use crate::buffer::LineBuffer;
use crate::config::{Config, InputrcPath};
use crate::hooks::{HistoryExpansionContext, Hooks};
use crate::inputrc::{InputrcParser, discover_inputrc_path};
use crate::keymap::{EditCommand, KeyBinding, KeyMap, KeyMapName};
use crate::prompt::Prompt;
use crate::state::*;
use crate::terminal::{TerminalEvent, TerminalIo};
use crate::variables::Variables;
use history::History;
use history::expansion::{HistoryChars, HistoryExpansion, HistoryExpansionPolicy};
use std::fmt;
use std::io;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadlineResult {
    Line(Vec<u8>),
    Interrupted,
    Eof,
}

#[derive(Debug)]
pub enum ReadlineError {
    Io(io::Error),
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

pub struct Editor<T>
where
    T: TerminalIo,
{
    config: Config,
    pub(crate) terminal: T,
    pub(crate) history: History,
    pub(crate) keymap: KeyMap,
    pub(crate) variables: Variables,
    pub(crate) pending_initial_line: Option<Vec<u8>>,
    initial_inputrc_error: Option<String>,
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub fn new(config: Config, terminal: T, history: History) -> Self {
        let mut line = Self::new_without_inputrc(config, terminal, history);
        if let Err(err) = line.reload_inputrc() {
            line.initial_inputrc_error = Some(err.to_string());
        }
        line
    }

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
            terminal,
            history,
            keymap,
            variables,
            pending_initial_line: None,
            initial_inputrc_error: None,
        }
    }

    pub fn initial_inputrc_error(&self) -> Option<&str> {
        self.initial_inputrc_error.as_deref()
    }

    pub fn bind_api(&mut self) -> BindApi<'_> {
        BindApi::with_config(&mut self.keymap, &mut self.variables, &self.config)
    }

    pub fn terminal(&self) -> &T {
        &self.terminal
    }

    pub fn terminal_mut(&mut self) -> &mut T {
        &mut self.terminal
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn history_mut(&mut self) -> &mut History {
        &mut self.history
    }

    pub fn variables(&self) -> &Variables {
        &self.variables
    }

    pub fn variables_mut(&mut self) -> &mut Variables {
        &mut self.variables
    }

    pub fn load_inputrc_str(&mut self, source: &str) -> Result<(), ReadlineError> {
        InputrcParser::new()
            .parse_str(source, &self.config, &mut self.keymap, &mut self.variables)
            .map_err(|e| ReadlineError::Inputrc(format!("line {}: {}", e.line, e.message)))
    }

    pub fn load_inputrc_file(&mut self, path: &Path) -> Result<(), ReadlineError> {
        InputrcParser::new()
            .parse_file(path, &self.config, &mut self.keymap, &mut self.variables)
            .map_err(|e| ReadlineError::Inputrc(format!("line {}: {}", e.line, e.message)))?;
        self.config.inputrc_path = InputrcPath::Path(path.to_path_buf());
        Ok(())
    }

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

    pub fn cleanup_after_signal(&mut self) -> Result<(), ReadlineError> {
        if self.variable_is_on("enable-bracketed-paste") {
            let _ = self.terminal.write("\x1b[?2004l");
            let _ = self.terminal.flush();
        }
        if self.variable_is_on("enable-meta-key") {
            let _ = self.terminal.set_meta_key_enabled(false);
            let _ = self.terminal.flush();
        }
        if self.variable_is_on("enable-keypad") {
            let _ = self.terminal.set_application_keypad_enabled(false);
            let _ = self.terminal.flush();
        }
        self.terminal.restore_mode().map_err(ReadlineError::Io)
    }

    pub fn reset_after_signal(&mut self) -> Result<(), ReadlineError> {
        self.terminal.enter_raw_mode()?;
        self.terminal
            .set_meta_key_enabled(self.variable_is_on("enable-meta-key"))?;
        self.terminal
            .set_application_keypad_enabled(self.variable_is_on("enable-keypad"))?;
        if self.variable_is_on("enable-bracketed-paste") {
            self.terminal.write("\x1b[?2004h")?;
            self.terminal.flush()?;
        }
        Ok(())
    }

    pub fn read_line(
        &mut self,
        prompt: Prompt,
        hooks: &mut impl Hooks,
    ) -> Result<ReadlineResult, ReadlineError> {
        self.bind_tty_special_chars();
        self.reset_after_signal()?;
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
        self.render(&mut state)?;

        loop {
            if let Some(signal) = hooks.check_signals() {
                if let Some(result) = self.handle_checked_signal(&mut state, signal)? {
                    return Ok(result);
                }
                self.render(&mut state)?;
                continue;
            }

            let event = self.terminal.read_event(self.keyseq_timeout())?;

            match event {
                TerminalEvent::Timeout => {
                    if !state.input.pending_key.is_empty() {
                        let pending = std::mem::take(&mut state.input.pending_key);
                        let outcome = self.handle_unbound(&mut state, &pending)?;
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
                        return Ok(result);
                    }
                    self.render(&mut state)?;
                    continue;
                }
                TerminalEvent::Bytes(bytes) if bytes.is_empty() => continue,
                TerminalEvent::Bytes(bytes) => {
                    let outcome = if state.search.non_incremental_search.is_some() {
                        self.handle_non_incremental_search(&mut state, &bytes)
                    } else if state.search.reverse_search.is_some() {
                        self.handle_reverse_search(&mut state, &bytes, hooks)?
                    } else {
                        self.handle_bytes(&mut state, &bytes, hooks)?
                    };
                    if matches!(outcome, EditorOutcome::Continue) {
                        self.render(&mut state)?;
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
        #[cfg(unix)]
        if signal == libc::SIGINT {
            self.echo_signal_interrupt(state)?;
            return Ok(Some(ReadlineResult::Interrupted));
        }
        let _ = signal;
        Ok(None)
    }

    pub(crate) fn echo_signal_interrupt(
        &mut self,
        state: &mut EditorState,
    ) -> Result<(), ReadlineError> {
        self.write_below_rendered_line(state, "^C\r\n")?;
        self.terminal.flush()?;
        Ok(())
    }

    fn finish_outcome(
        &mut self,
        state: &mut EditorState,
        outcome: EditorOutcome,
    ) -> Result<ReadlineResult, ReadlineError> {
        match outcome {
            EditorOutcome::Continue => {
                unreachable!("continue outcomes are handled in the read loop")
            }
            EditorOutcome::Accepted(bytes) => {
                self.move_below_rendered_line(state)?;
                self.terminal.flush()?;
                if self.variable_is_on("revert-all-at-newline") {
                    self.history.revert_current_edit();
                }
                if self.config.auto_add_history {
                    self.history.push_bytes(bytes.clone());
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
        if !self.variable_is_on("bind-tty-special-chars") {
            return;
        }
        for (byte, command) in self.terminal.tty_special_bindings() {
            let command = if command == "end-of-file" {
                EditCommand::Eof
            } else if let Some(command) = EditCommand::parse(command) {
                command
            } else {
                continue;
            };
            for map in self.tty_special_binding_maps() {
                let binding = if command == EditCommand::Eof
                    && matches!(map, KeyMapName::ViCommand | KeyMapName::ViInsert)
                {
                    KeyBinding::NamedCommand("vi-eof-maybe".to_string())
                } else {
                    KeyBinding::Command(command)
                };
                self.keymap
                    .bind(map, crate::keymap::KeySequence::new(vec![byte]), binding);
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

    pub(crate) fn variable_is_on(&self, name: &str) -> bool {
        self.variables.is_on(name)
    }

    fn histchars(&self) -> HistoryChars {
        HistoryChars::parse(
            self.variables
                .get("histchars")
                .map(String::as_str)
                .unwrap_or("!^#"),
        )
    }

    fn history_expansion_policy(&self) -> HistoryExpansionPolicy {
        let mut policy = HistoryExpansionPolicy::default();
        if let Some(value) = self.variables.get_bytes("history-word-delimiters") {
            policy.word_delimiters = value.clone();
        }
        if let Some(value) = self.variables.get_bytes("history-search-delimiter-chars") {
            policy.search_delimiters = value.clone();
        }
        if let Some(value) = self.variables.get_bytes("history-no-expand-chars") {
            policy.no_expand_chars = value.clone();
        }
        policy.quotes_inhibit_expansion = self.variable_is_on("history-quotes-inhibit-expansion");
        policy
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
        let policy = self.history_expansion_policy();
        hooks
            .expand_history_with_status(HistoryExpansionContext {
                line,
                history: &self.history,
                histchars: self.histchars(),
                policy: &policy,
            })
            .unwrap_or_else(|| {
                Ok(HistoryExpansion {
                    line: line.to_vec(),
                    print_only: false,
                })
            })
    }

    pub(crate) fn try_expand_history(
        &mut self,
        state: &mut EditorState,
        line: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<Option<Vec<u8>>, ReadlineError> {
        match self.expand_history_line(line, hooks) {
            Ok(expanded) if expanded.print_only => {
                let message = String::from_utf8_lossy(&expanded.line);
                self.report_history_expansion_message(state, &message)?;
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

    pub(crate) fn report_history_expansion_message(
        &mut self,
        state: &mut EditorState,
        message: &str,
    ) -> Result<(), ReadlineError> {
        self.move_below_rendered_line(state)?;
        self.write_tracked(state, message)?;
        self.write_tracked_newline(state)?;
        self.terminal.flush()?;
        Ok(())
    }

    fn history_size_limit(&self) -> Option<usize> {
        self.variables
            .get("history-size")
            .and_then(|value| value.parse::<isize>().ok())
            .and_then(|value| (value >= 0).then_some(value as usize))
    }

    fn keyseq_timeout(&self) -> Option<Duration> {
        let timeout = self
            .variables
            .get("keyseq-timeout")
            .and_then(|value| value.parse::<isize>().ok())
            .unwrap_or(self.config.keyseq_timeout_ms as isize);
        (timeout > 0).then(|| Duration::from_millis(timeout as u64))
    }

    pub(crate) fn replace_from_history(&mut self, state: &mut EditorState, line: &[u8]) {
        if !self.variable_is_on("revert-all-at-newline")
            && let Some(index) = self.history.current_index()
        {
            self.history
                .set_undo_list(index, state.undo_snapshot_lines());
        }
        let point = state.buffer.point();
        state.buffer = LineBuffer::from_bytes(line.to_vec());
        state.original_line = line.to_vec();
        state.undo.undo_stack.clear();
        state.undo.pending_undo = None;
        if !self.variable_is_on("revert-all-at-newline")
            && let Some(index) = self.history.current_index()
            && let Some(undo_list) = self.history.undo_list(index)
        {
            state.restore_undo_snapshot_lines(undo_list);
        }
        if self.variable_is_on("history-preserve-point") {
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
    Continue,
    Accepted(Vec<u8>),
    Eof,
}

#[cfg(test)]
mod tests;
