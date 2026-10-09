//! Shared terminal cell-width helpers.
//!
//! This module intentionally keeps three different meanings separate:
//! ANSI-aware output measurement, buffer render measurement, and prompt marker
//! parsing. The rules differ and should not be collapsed into one function.

/// Returns the terminal cell width of a Unicode scalar value.
///
/// Control characters are explicitly zero width so measurement does not
/// depend on upstream width-table churn (`None` vs `Some(1)`).
pub(crate) fn char_width(ch: char) -> usize {
    if ch.is_control() {
        return 0;
    }
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
        if ch == '\x1b' {
            consume_escape_tail(&mut chars);
        } else {
            width += char_width(ch);
        }
    }
    width
}

/// Returns visible width of the last `\\n`-separated line.
pub(crate) fn last_line_width(value: &str) -> usize {
    value.rsplit('\n').next().map(visible_width).unwrap_or(0)
}

/// Consumes the tail of a terminal escape sequence after the leading ESC.
/// CSI (`ESC [`) runs to the first `@`..=`~` byte, OSC (`ESC ]`) runs to BEL
/// or `ESC \`, and any other ESC consumes one following byte when present.
fn consume_escape_tail(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek().copied() {
        Some('[') => {
            chars.next();
            for ch in chars.by_ref() {
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        }
        Some(']') => {
            chars.next();
            let mut previous = '\0';
            for ch in chars.by_ref() {
                if ch == '\x07' || (previous == '\x1b' && ch == '\\') {
                    break;
                }
                previous = ch;
            }
        }
        _ => {
            chars.next();
        }
    }
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

/// Measures rendered output once, returning terminal rows and whether output
/// ends exactly at the wrap boundary.
pub(crate) fn measured_rows_for_output(output: &str, columns: usize) -> (u16, bool) {
    let columns = columns.max(1);
    let mut row = 0usize;
    let mut col = 0usize;
    let mut saw_visible_cell = false;
    let mut ended_with_newline = false;
    let mut chars = output.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            consume_escape_tail(&mut chars);
            continue;
        }
        if ch == '\n' {
            row += 1;
            col = 0;
            ended_with_newline = true;
            continue;
        }
        ended_with_newline = false;
        let width = char_width(ch);
        let (added, next) = advance_cell(col, width, columns);
        row += added;
        col = next;
        saw_visible_cell |= width > 0;
    }
    (
        row as u16,
        saw_visible_cell && !ended_with_newline && col == 0,
    )
}

/// Returns how many terminal rows a rendered output string occupies.
pub(crate) fn rendered_rows_for_output(output: &str, columns: usize) -> u16 {
    measured_rows_for_output(output, columns).0
}
