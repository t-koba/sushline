pub(crate) const CLEAR_TO_LINE_END: &str = "\x1b[K";
pub(crate) const CLEAR_TO_SCREEN_END: &str = "\x1b[J";
pub(crate) const CLEAR_ALL: &str = "\x1b[2J";
pub(crate) const ERASE_SCROLLBACK: &str = "\x1b[3J";
pub(crate) const SAVE_CURSOR: &str = "\x1b[s";
pub(crate) const RESTORE_CURSOR: &str = "\x1b[u";
pub(crate) const BRACKETED_PASTE_ON: &str = "\x1b[?2004h";
pub(crate) const BRACKETED_PASTE_OFF: &str = "\x1b[?2004l";
pub(crate) const APPLICATION_KEYPAD_ON: &str = "\x1b=";
pub(crate) const APPLICATION_KEYPAD_OFF: &str = "\x1b>";
pub(crate) const XTERM_META_ON: &str = "\x1b[?1034h";
pub(crate) const XTERM_META_OFF: &str = "\x1b[?1034l";
pub(crate) const VISIBLE_BELL: &str = "\x1b[?5h\x1b[?5l";

pub(crate) fn move_up(rows: u16) -> String {
    if rows == 0 {
        String::new()
    } else {
        format!("\x1b[{rows}A")
    }
}

pub(crate) fn move_to_column(column: u16) -> String {
    format!("\x1b[{}G", column.saturating_add(1))
}
