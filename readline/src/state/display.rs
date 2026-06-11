use crate::terminal::TerminalSize;

#[derive(Debug, Default)]
pub(crate) struct DisplayState {
    pub(crate) rendered_rows: u16,
    pub(crate) rendered_cursor_row: u16,
    pub(crate) last_terminal_size: Option<TerminalSize>,
}
