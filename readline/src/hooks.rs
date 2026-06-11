use crate::completion::{CompletionRequest, CompletionResponse, CompletionType};
use crate::keymap::KeyMapName;
use history::History;
use history::expansion::{
    HistoryChars, HistoryExpansion, HistoryExpansionPolicy, expand_history_with_status,
};

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
    pub policy: &'a HistoryExpansionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineExpansionContext<'a> {
    pub line: &'a [u8],
    pub point: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellCorrectionContext<'a> {
    pub line: &'a [u8],
    pub point: usize,
    pub word_start: usize,
    pub word_end: usize,
    pub word: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuoteContext<'a> {
    pub value: &'a [u8],
    pub line: &'a [u8],
    pub point: usize,
    pub word_start: usize,
    pub word_end: usize,
    pub word: &'a [u8],
    pub quote: Option<char>,
    pub completion_type: CompletionType,
    pub quote_filename: bool,
}

pub trait Hooks {
    fn check_signals(&mut self) -> Option<i32> {
        None
    }

    fn version(&mut self) -> Option<String> {
        None
    }

    fn tty_status(&mut self) -> Option<String> {
        None
    }

    fn on_command(&mut self, _context: CommandContext<'_>) -> Option<Edit> {
        None
    }

    /// Performs application-owned whole-line expansion for commands such as
    /// `shell-expand-line`.
    ///
    /// Returning `None` means the embedding program has no expansion result;
    /// sushline must not invent application semantics on its behalf.
    fn expand_line(&mut self, _context: LineExpansionContext<'_>) -> Option<Edit> {
        None
    }

    fn expand_aliases(&mut self, _line: &[u8]) -> Option<Vec<u8>> {
        None
    }

    fn edit_and_execute(&mut self, _line: &[u8]) -> Option<Vec<u8>> {
        None
    }

    fn spell_correct(&mut self, _context: SpellCorrectionContext<'_>) -> Option<Vec<u8>> {
        None
    }

    fn expand_history(
        &mut self,
        context: HistoryExpansionContext<'_>,
    ) -> Result<HistoryExpansion, String> {
        expand_history_with_status(
            context.line,
            context.history,
            context.histchars,
            context.policy,
            |_| false,
        )
        .map_err(|err| err.message())
    }

    /// Performs programmable completion. `None` means there is no compspec for
    /// this request and sushline should continue with built-in default paths.
    fn complete(&mut self, _request: CompletionRequest) -> Option<CompletionResponse> {
        None
    }

    /// Performs application-owned default completion, including
    /// `bashdefault` fallback and command-position completion.
    fn default_complete(&mut self, _request: &CompletionRequest) -> Option<CompletionResponse> {
        None
    }

    fn quote_completion(&mut self, _context: QuoteContext<'_>) -> Option<Vec<u8>> {
        None
    }

    fn glob_expand(&mut self, _pattern: &[u8]) -> Option<Vec<Vec<u8>>> {
        None
    }

    fn command_names(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    fn user_names(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    fn host_names(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    fn variable_names(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    fn completion_word_breaks(&mut self) -> Option<Vec<u8>> {
        None
    }

    fn editing_word_breaks(&mut self) -> Option<Vec<u8>> {
        None
    }

    /// Returns shell/application word byte ranges for commands that need to
    /// edit by shell word boundaries.
    ///
    /// Ranges must be non-empty, sorted, non-overlapping, and within `line`.
    /// Invalid ranges are ignored and Sushline falls back to its built-in
    /// command-word parser.
    fn shell_word_spans(&mut self, _line: &[u8]) -> Option<Vec<(usize, usize)>> {
        None
    }

    /// Returns shell/application words for history-word commands.
    ///
    /// The default derives words from `shell_word_spans` when the embedder
    /// provides byte ranges.
    fn shell_words(&mut self, line: &[u8]) -> Option<Vec<Vec<u8>>> {
        derive_words_from_spans(line, self.shell_word_spans(line)?)
    }
}

impl Hooks for () {}

pub(crate) fn validate_token_spans(line: &[u8], spans: &[(usize, usize)]) -> bool {
    let mut previous_end = 0;
    for &(start, end) in spans {
        if start >= end || end > line.len() || start < previous_end {
            return false;
        }
        previous_end = end;
    }
    true
}

pub(crate) fn derive_words_from_spans(
    line: &[u8],
    spans: Vec<(usize, usize)>,
) -> Option<Vec<Vec<u8>>> {
    if !validate_token_spans(line, &spans) {
        return None;
    }
    Some(
        spans
            .into_iter()
            .map(|(start, end)| line[start..end].to_vec())
            .collect(),
    )
}
