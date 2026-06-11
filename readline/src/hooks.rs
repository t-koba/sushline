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
pub struct ApplicationLineExpansionContext<'a> {
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

    fn expand_application_line_with_context(
        &mut self,
        context: ApplicationLineExpansionContext<'_>,
    ) -> Option<Edit> {
        self.expand_application_line(context.line).map(|line| Edit {
            line: Some(line),
            point: None,
            mark: None,
        })
    }

    fn expand_history(
        &mut self,
        _context: HistoryExpansionContext<'_>,
    ) -> Option<Result<Vec<u8>, String>> {
        None
    }

    fn expand_history_with_status(
        &mut self,
        context: HistoryExpansionContext<'_>,
    ) -> Option<Result<HistoryExpansion, String>> {
        if let Some(result) = self.expand_history(context) {
            return Some(result.map(|line| HistoryExpansion {
                line,
                print_only: false,
            }));
        }
        Some(
            expand_history_with_status(
                context.line,
                context.history,
                context.histchars,
                context.policy,
                |_| false,
            )
            .map_err(|err| err.message()),
        )
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

    fn spell_correct_with_context(
        &mut self,
        context: SpellCorrectionContext<'_>,
    ) -> Option<Vec<u8>> {
        self.spell_correct(context.word)
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

    fn glob_expand_bytes(&self, pattern: &[u8]) -> Option<Vec<Vec<u8>>> {
        let pattern = std::str::from_utf8(pattern).ok()?;
        self.glob_expand(pattern)
            .map(|matches| matches.into_iter().map(String::into_bytes).collect())
    }

    /// Returns shell/application words for history-word commands.
    ///
    /// The default derives words from `tokenize_with_spans` when the embedder
    /// provides byte ranges.
    fn tokenize(&self, line: &[u8]) -> Option<Vec<Vec<u8>>> {
        let spans = self.tokenize_with_spans(line)?;
        let mut previous_end = 0;
        let mut words = Vec::with_capacity(spans.len());
        for (start, end) in spans {
            if start >= end || end > line.len() || start < previous_end {
                return None;
            }
            words.push(line[start..end].to_vec());
            previous_end = end;
        }
        Some(words)
    }

    /// Returns shell/application word byte ranges for commands that need to
    /// edit by shell word boundaries.
    ///
    /// Ranges must be non-empty, sorted, non-overlapping, and within `line`.
    /// Invalid ranges are ignored and Sushline falls back to its built-in
    /// command-word parser.
    fn tokenize_with_spans(&self, _line: &[u8]) -> Option<Vec<(usize, usize)>> {
        None
    }

    fn quote(&self, _value: &[u8]) -> Option<Vec<u8>> {
        None
    }

    /// Quotes a completion replacement in the current editor context.
    ///
    /// Embedders that need GNU-equivalent shell quoting should implement this
    /// method instead of `quote`; it is called for quoted and unquoted
    /// completion replacements and carries the original line state.
    fn quote_completion(&self, context: QuoteContext<'_>) -> Option<Vec<u8>> {
        if context.quote.is_none() && context.quote_filename {
            self.quote(context.value)
        } else {
            None
        }
    }

    fn completion_word_breaks(&self) -> Option<Vec<u8>> {
        None
    }

    fn editing_word_breaks(&self) -> Option<Vec<u8>> {
        None
    }
}

impl Hooks for () {}
