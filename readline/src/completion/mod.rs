pub(crate) mod builtin;
pub(crate) mod display;
pub(crate) mod engine;
pub(crate) mod export;
pub(crate) mod filename;
mod insert;
mod menu;
pub(crate) mod quoting;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// CompletionType.
pub enum CompletionType {
    /// Complete.
    Complete,
    /// Command.
    Command,
    /// Filename.
    Filename,
    /// Hostname.
    Hostname,
    /// Username.
    Username,
    /// Variable.
    Variable,
    /// PossibleCompletions.
    PossibleCompletions,
    /// PossibleCommandCompletions.
    PossibleCommandCompletions,
    /// PossibleFilenameCompletions.
    PossibleFilenameCompletions,
    /// PossibleHostnameCompletions.
    PossibleHostnameCompletions,
    /// PossibleUsernameCompletions.
    PossibleUsernameCompletions,
    /// PossibleVariableCompletions.
    PossibleVariableCompletions,
    /// MenuComplete.
    MenuComplete,
    /// MenuCompleteBackward.
    MenuCompleteBackward,
    /// InsertCompletions.
    InsertCompletions,
    /// GlobCompleteWord.
    GlobCompleteWord,
    /// GlobExpandWord.
    GlobExpandWord,
    /// GlobListExpansions.
    GlobListExpansions,
    /// DynamicHistory.
    DynamicHistory,
    /// ViComplete.
    ViComplete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// CompletionContext.
pub struct CompletionContext {
    /// Line.
    pub line: Vec<u8>,
    /// Point.
    pub point: usize,
    /// Word start.
    pub word_start: usize,
    /// Word end.
    pub word_end: usize,
    /// Word.
    pub word: Vec<u8>,
    /// Key.
    pub key: Vec<u8>,
    /// Completion type.
    pub completion_type: CompletionType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// CompletionRequest.
pub struct CompletionRequest {
    /// Context.
    pub context: CompletionContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// CompletionCandidate.
pub struct CompletionCandidate {
    /// Replacement.
    pub replacement: Vec<u8>,
    /// Display.
    pub display: Option<String>,
}

impl CompletionCandidate {
    /// Replacement bytes.
    pub fn replacement_bytes(&self) -> &[u8] {
        &self.replacement
    }

    /// Replacement string.
    pub fn replacement_string(&self) -> String {
        String::from_utf8_lossy(&self.replacement).into_owned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// CompletionAction.
pub enum CompletionAction {
    /// Replace.
    Replace,
    /// DisplayOnly.
    DisplayOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// CompletionOptions.
pub struct CompletionOptions {
    /// Nospace.
    pub nospace: bool,
    /// Filenames.
    pub filenames: bool,
    /// Fullquote.
    pub fullquote: bool,
    /// Noquote.
    pub noquote: bool,
    /// Nosort.
    pub nosort: bool,
    /// Bashdefault.
    pub bashdefault: bool,
    /// Default.
    pub default: bool,
    /// Dirnames.
    pub dirnames: bool,
    /// Plusdirs.
    pub plusdirs: bool,
    /// Append character.
    pub append_character: Option<char>,
    /// Suppress append.
    pub suppress_append: bool,
    /// Replacement prefix.
    pub replacement_prefix: Option<Vec<u8>>,
    /// Replacement suffix.
    pub replacement_suffix: Option<Vec<u8>>,
    /// Filter prefix.
    pub filter_prefix: Option<Vec<u8>>,
    /// Filter suffix.
    pub filter_suffix: Option<Vec<u8>>,
    /// Action.
    pub action: Option<CompletionAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// CompletionResponse.
pub struct CompletionResponse {
    /// Candidates.
    pub candidates: Vec<CompletionCandidate>,
    /// Options.
    pub options: CompletionOptions,
}

impl CompletionOptions {
    pub(crate) fn quote_filename(&self) -> bool {
        (self.filenames || self.fullquote) && !self.noquote
    }
}
