use crate::completion::quoting::CompletionEdit;
use crate::completion::{CompletionResponse, CompletionType};

use super::LastYankArgState;

#[derive(Debug, Default)]
pub(crate) struct CompletionState {
    pub(crate) last_completion: Option<CompletionResponse>,
    pub(crate) last_attempt: Option<CompletionAttemptState>,
    pub(crate) menu_completion: Option<MenuCompletionState>,
    pub(crate) last_yank_arg: Option<LastYankArgState>,
}

#[derive(Debug)]
pub(crate) struct CompletionAttemptState {
    pub(crate) completion_type: CompletionType,
    pub(crate) line: Vec<u8>,
    pub(crate) point: usize,
    pub(crate) unmodified: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct MenuCompletionState {
    pub(crate) index: usize,
    pub(crate) edit: CompletionEdit,
    pub(crate) end: usize,
    pub(crate) original: Vec<u8>,
    pub(crate) response: CompletionResponse,
}
