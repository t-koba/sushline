use crate::completion::{CompletionRequest, CompletionResponse};
use crate::keymap::KeyMapName;
use history::History;
use history::expansion::HistoryChars;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edit {
    pub line: Option<Vec<u8>>,
    pub point: Option<usize>,
    pub mark: Option<Option<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandContext<'a> {
    pub command: &'a str,
    pub line: &'a [u8],
    pub point: usize,
    pub mark: Option<usize>,
    pub argument: Option<i32>,
    pub key: &'a [u8],
    pub keymap: KeyMapName,
}

#[derive(Debug, Clone, Copy)]
pub struct HistoryExpansionContext<'a> {
    pub line: &'a [u8],
    pub history: &'a History,
    pub histchars: HistoryChars,
}

pub trait Hooks {
    fn on_command(&mut self, _context: CommandContext<'_>) -> Option<Edit> {
        None
    }

    fn get_variable(&self, _name: &str) -> Option<String> {
        None
    }

    fn set_variable(&mut self, _name: &str, _value: &str) {}

    fn expand_aliases(&mut self, _line: &[u8]) -> Option<Vec<u8>> {
        None
    }

    /// Performs application-owned whole-line expansion for commands such as
    /// `shell-expand-line`.
    ///
    /// Returning `None` means the embedding program has no expansion result;
    /// sushline must not invent application semantics on its behalf.
    fn expand_application_line(&mut self, _line: &[u8]) -> Option<Vec<u8>> {
        None
    }

    fn expand_history(
        &mut self,
        _context: HistoryExpansionContext<'_>,
    ) -> Option<Result<Vec<u8>, String>> {
        None
    }

    fn check_signals(&self) -> Option<i32> {
        None
    }

    fn version(&self) -> Option<String> {
        None
    }

    fn edit_and_execute(&mut self, _line: &[u8]) -> Option<Vec<u8>> {
        None
    }

    fn tty_status(&self) -> Option<String> {
        None
    }

    fn spell_correct(&mut self, _word: &[u8]) -> Option<Vec<u8>> {
        None
    }

    /// Performs application-owned default completion.
    ///
    /// Returning `None` means that the embedding program has no default for this request; it must
    /// not be treated as permission to invent application state inside sushline.
    fn default_complete(&mut self, _request: &CompletionRequest) -> Option<CompletionResponse> {
        None
    }

    fn complete(&mut self, _request: CompletionRequest) -> Option<CompletionResponse> {
        None
    }

    fn command_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn user_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn host_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn variable_names(&mut self) -> Vec<String> {
        Vec::new()
    }

    fn glob_expand(&self, _pattern: &str) -> Option<Vec<String>> {
        None
    }

    fn tokenize(&self, _line: &[u8]) -> Option<Vec<Vec<u8>>> {
        None
    }

    fn quote(&self, _value: &[u8]) -> Option<Vec<u8>> {
        None
    }

    fn completion_word_breaks(&self) -> Option<Vec<u8>> {
        None
    }

    fn editing_word_breaks(&self) -> Option<Vec<u8>> {
        None
    }
}

impl Hooks for () {}
