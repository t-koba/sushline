#[derive(Debug, Clone)]
pub(crate) struct MenuCompletionState {
    pub(crate) index: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) original: Vec<u8>,
}
