//! Shared terminal cell-width helpers.
//!
//! This module intentionally keeps three different meanings separate:
//! ANSI-aware output measurement, buffer render measurement, and prompt marker
//! parsing. The rules differ and should not be collapsed into one function.

/// Returns the terminal cell width of a Unicode scalar value.
pub(crate) fn char_width(ch: char) -> usize {
    unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0)
}

/// Returns visible width after removing readline hidden prompt markers and
/// terminal escape sequences.
pub(crate) fn visible_width(value: &str) -> usize {
    let mut width = 0;
    let mut chars = value.chars().peekable();
    let mut hidden = false;
    while let Some(ch) = chars.next() {
        if ch == '\x01' {
            hidden = true;
            continue;
        }
        if ch == '\x02' {
            hidden = false;
            continue;
        }
        if hidden {
            continue;
        }
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for ch in chars.by_ref() {
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        } else if ch == '\x1b' && chars.peek() == Some(&']') {
            chars.next();
            let mut previous = '\0';
            for ch in chars.by_ref() {
                if ch == '\x07' || (previous == '\x1b' && ch == '\\') {
                    break;
                }
                previous = ch;
            }
        } else if ch == '\x1b' {
            chars.next();
        } else {
            width += char_width(ch);
        }
    }
    width
}

/// Returns terminal-visible characters after removing CSI, OSC, and bare ESC
/// escape sequences.
pub(crate) fn terminal_visible_chars(output: &str) -> Vec<char> {
    let mut chars = output.chars().peekable();
    let mut visible = Vec::new();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for ch in chars.by_ref() {
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        } else if ch == '\x1b' && chars.peek() == Some(&']') {
            chars.next();
            let mut previous = '\0';
            for ch in chars.by_ref() {
                if ch == '\x07' || (previous == '\x1b' && ch == '\\') {
                    break;
                }
                previous = ch;
            }
        } else if ch == '\x1b' {
            let _ = chars.next();
        } else {
            visible.push(ch);
        }
    }
    visible
}

/// Advances one cell across a wrapped line, returning added rows and new column.
pub(crate) fn advance_cell(col: usize, width: usize, columns: usize) -> (usize, usize) {
    let mut col = col;
    let mut rows = 0usize;
    if width > 0 && col + width > columns {
        rows += 1;
        col = 0;
    }
    col += width;
    if col >= columns {
        rows += col / columns;
        col %= columns;
    }
    (rows, col)
}

/// Returns how many terminal rows a rendered output string occupies.
pub(crate) fn rendered_rows_for_output(output: &str, columns: usize) -> u16 {
    let columns = columns.max(1);
    let mut row = 0usize;
    let mut col = 0usize;
    for ch in terminal_visible_chars(output) {
        if ch == '\n' {
            row += 1;
            col = 0;
            continue;
        }
        let (added, next) = advance_cell(col, char_width(ch), columns);
        row += added;
        col = next;
    }
    row as u16
}

/// Returns whether rendered output ends exactly at the terminal wrap boundary.
pub(crate) fn output_ends_at_wrap_boundary(output: &str, columns: usize) -> bool {
    let columns = columns.max(1);
    let mut col = 0usize;
    let mut saw_visible_cell = false;
    let mut ended_with_newline = false;
    for ch in terminal_visible_chars(output) {
        if ch == '\n' {
            col = 0;
            ended_with_newline = true;
            continue;
        }
        ended_with_newline = false;
        let width = char_width(ch);
        (_, col) = advance_cell(col, width, columns);
        saw_visible_cell |= width > 0;
    }
    saw_visible_cell && !ended_with_newline && col == 0
}
