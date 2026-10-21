use crate::terminal::TerminalSize;

#[derive(Debug, Default)]
pub(crate) struct DisplayState {
    pub(crate) horizontal: crate::display::horizontal::HorizontalViewport,
    pub(crate) rendered_rows: u16,
    pub(crate) rendered_cursor_row: u16,
    pub(crate) last_terminal_size: Option<TerminalSize>,
    /// Text that was inserted but not drawn yet.
    ///
    /// GNU Readline defers the redisplay of typed text until the input
    /// batch ends. The editor draws the pending text before a key that
    /// interrupts it, so the text comes before what the key writes.
    pub(crate) pending_text: bool,
}
