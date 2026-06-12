use crate::completion::{CompletionRequest, CompletionResponse, CompletionType};
use crate::keymap::KeyMapName;
use history::History;
use history::expansion::{
    HistoryChars, HistoryExpansion, HistoryExpansionPolicy, expand_history_with_status,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Edit.
pub struct Edit {
    /// Line.
    pub line: Option<Vec<u8>>,
    /// Point.
    pub point: Option<usize>,
    /// Mark.
    pub mark: Option<Option<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// CommandContext.
pub struct CommandContext<'a> {
    /// Command.
    pub command: &'a str,
    /// Line.
    pub line: &'a [u8],
    /// Point.
    pub point: usize,
    /// Mark.
    pub mark: Option<usize>,
    /// Argument.
    pub argument: Option<i32>,
    /// Key.
    pub key: &'a [u8],
    /// Keymap.
    pub keymap: KeyMapName,
}

#[derive(Debug, Clone, Copy)]
/// HistoryExpansionContext.
pub struct HistoryExpansionContext<'a> {
    /// Line.
    pub line: &'a [u8],
    /// History.
    pub history: &'a History,
    /// Histchars.
    pub histchars: HistoryChars,
    /// Policy.
    pub policy: &'a HistoryExpansionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// LineExpansionContext.
pub struct LineExpansionContext<'a> {
    /// Line.
    pub line: &'a [u8],
    /// Point.
    pub point: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// SpellCorrectionContext.
pub struct SpellCorrectionContext<'a> {
    /// Line.
    pub line: &'a [u8],
    /// Point.
    pub point: usize,
    /// Word start.
    pub word_start: usize,
    /// Word end.
    pub word_end: usize,
    /// Word.
    pub word: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// QuoteContext.
pub struct QuoteContext<'a> {
    /// Value.
    pub value: &'a [u8],
    /// Line.
    pub line: &'a [u8],
    /// Point.
    pub point: usize,
    /// Word start.
    pub word_start: usize,
    /// Word end.
    pub word_end: usize,
    /// Word.
    pub word: &'a [u8],
    /// Quote.
    pub quote: Option<char>,
    /// Completion type.
    pub completion_type: CompletionType,
    /// Quote filename.
    pub quote_filename: bool,
}

/// Hooks.
pub trait Hooks {
    /// Checks for pending application signals.
    fn check_signals(&mut self) -> Option<i32> {
        None
    }

    /// Returns the embedding application's version string.
    fn version(&mut self) -> Option<String> {
        None
    }

    /// Returns tty status text for display-oriented commands.
    fn tty_status(&mut self) -> Option<String> {
        None
    }

    /// Allows the embedder to intercept an editing command.
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

    /// Expands aliases in a command line.
    fn expand_aliases(&mut self, _line: &[u8]) -> Option<Vec<u8>> {
        None
    }

    /// Opens the current line in an external editor and returns replacement text.
    fn edit_and_execute(&mut self, _line: &[u8]) -> Option<Vec<u8>> {
        None
    }

    /// Returns a corrected spelling for the word in context.
    fn spell_correct(&mut self, _context: SpellCorrectionContext<'_>) -> Option<Vec<u8>> {
        None
    }

    /// Expands history references in a line.
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

    /// Quotes a completion candidate for insertion.
    fn quote_completion(&mut self, _context: QuoteContext<'_>) -> Option<Vec<u8>> {
        None
    }

    /// Expands a glob pattern into candidate paths.
    fn glob_expand(&mut self, _pattern: &[u8]) -> Option<Vec<Vec<u8>>> {
        None
    }

    /// Returns command names for completion.
    fn command_names(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    /// Returns user names for completion.
    fn user_names(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    /// Returns host names for completion.
    fn host_names(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    /// Returns variable names for completion.
    fn variable_names(&mut self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    /// Returns completion word-break bytes.
    fn completion_word_breaks(&mut self) -> Option<Vec<u8>> {
        None
    }

    /// Returns editing word-break bytes.
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
    // Some.
    Some(
        spans
            .into_iter()
            .map(|(start, end)| line[start..end].to_vec())
            .collect(),
    )
}
