pub(crate) mod builtin;
pub(crate) mod display;
pub(crate) mod engine;
pub(crate) mod export;
pub(crate) mod filename;
pub(crate) mod quoting;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionType {
    Complete,
    Command,
    Filename,
    Hostname,
    Username,
    Variable,
    PossibleCompletions,
    PossibleCommandCompletions,
    PossibleFilenameCompletions,
    PossibleHostnameCompletions,
    PossibleUsernameCompletions,
    PossibleVariableCompletions,
    MenuComplete,
    MenuCompleteBackward,
    InsertCompletions,
    GlobCompleteWord,
    GlobExpandWord,
    GlobListExpansions,
    DynamicHistory,
    ViComplete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionContext {
    pub line: Vec<u8>,
    pub point: usize,
    pub word_start: usize,
    pub word_end: usize,
    pub word: Vec<u8>,
    pub key: Vec<u8>,
    pub completion_type: CompletionType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRequest {
    pub context: CompletionContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionCandidate {
    pub replacement: Vec<u8>,
    pub display: Option<String>,
}

impl CompletionCandidate {
    pub fn replacement_bytes(&self) -> &[u8] {
        &self.replacement
    }

    pub fn replacement_string(&self) -> String {
        String::from_utf8_lossy(&self.replacement).into_owned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionAction {
    Replace,
    DisplayOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompletionOptions {
    pub nospace: bool,
    pub filenames: bool,
    pub fullquote: bool,
    pub noquote: bool,
    pub nosort: bool,
    pub bashdefault: bool,
    pub default: bool,
    pub dirnames: bool,
    pub plusdirs: bool,
    pub append_character: Option<char>,
    pub suppress_append: bool,
    pub replacement_prefix: Option<Vec<u8>>,
    pub replacement_suffix: Option<Vec<u8>>,
    pub filter_prefix: Option<Vec<u8>>,
    pub filter_suffix: Option<Vec<u8>>,
    pub action: Option<CompletionAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompletionResponse {
    pub candidates: Vec<CompletionCandidate>,
    pub options: CompletionOptions,
}

impl CompletionOptions {
    pub(crate) fn quote_filename(&self) -> bool {
        (self.filenames || self.fullquote) && !self.noquote
    }
}
