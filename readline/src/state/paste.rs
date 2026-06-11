#[derive(Debug, Default)]
pub(crate) struct BracketedPasteState {
    pub(crate) bracketed_paste: bool,
    pub(crate) bracketed_paste_start: Option<usize>,
    pub(crate) bracketed_paste_pending: Vec<u8>,
}
