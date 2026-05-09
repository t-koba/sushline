use crate::completion::CompletionResponse;

#[derive(Debug, Clone)]
pub(crate) struct MenuCompletionState {
    pub(crate) index: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) original: Vec<u8>,
    pub(crate) word_bytes: Vec<u8>,
    pub(crate) quote: Option<char>,
    pub(crate) response: CompletionResponse,
}
