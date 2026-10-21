//! A terminal screen model for screen-level oracle assertions.
//!
//! The model renders the bytes a program wrote to a PTY onto a grid of rows
//! and columns, so an assertion can observe what a user sees on the terminal
//! instead of what the program wrote to the stream. The model covers the
//! control sequences the editor and GNU Readline use; it treats every
//! character as one cell wide, so it observes the single-width cases the
//! screen-level cases use. The insert and delete characters sequences are
//! covered because GNU Readline uses them to draw the pending numeric
//! argument over the prompt.

use std::iter::Peekable;
use std::str::Chars;

/// A rendered terminal screen.
pub(super) struct Screen {
    rows: usize,
    columns: usize,
    cells: Vec<Vec<char>>,
    row: usize,
    column: usize,
    saved: (usize, usize),
}

impl Screen {
    /// A screen of `rows` rows and `columns` columns.
    pub(super) fn new(rows: usize, columns: usize) -> Self {
        Self {
            rows,
            columns,
            cells: vec![vec![' '; columns]; rows],
            row: 0,
            column: 0,
            saved: (0, 0),
        }
    }

    /// Renders the bytes a program wrote to the terminal.
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        let mut characters = text.chars().peekable();
        while let Some(character) = characters.next() {
            match character {
                '\x1b' => self.escape(&mut characters),
                '\r' => self.column = 0,
                '\n' => self.row = (self.row + 1).min(self.rows - 1),
                '\x08' => self.column = self.column.saturating_sub(1),
                '\t' => self.column = ((self.column / 8) + 1) * 8,
                '\x07' => {}
                _ => self.put(character),
            }
        }
    }

    /// Whether any row of the screen shows `text`.
    pub(super) fn shows(&self, text: &str) -> bool {
        (0..self.rows).any(|row| self.row_text(row).contains(text))
    }

    /// The rendered rows at the last point where a row starts with `text`.
    ///
    /// A display that is drawn and then replaced (the pending numeric
    /// argument indicator, which the prompt replaces) is on the screen only
    /// between two writes, so every prefix of the stream is rendered. The
    /// indicator takes the place of the prompt, so it starts the row, and
    /// the last prefix whose row starts with it is the settled display: the
    /// prompt that follows starts the row again. A prefix that ends inside
    /// a multibyte character renders that character as a replacement
    /// character, so the text a case looks for has to be ASCII.
    pub(super) fn last_rows_starting_with(&self, bytes: &[u8], text: &str) -> Option<Vec<String>> {
        let mut rows = None;
        for end in 1..=bytes.len() {
            let mut screen = Screen::new(self.rows, self.columns);
            screen.feed(&bytes[..end]);
            if screen.rows_text().iter().any(|row| row.starts_with(text)) {
                rows = Some(screen.rows_text());
            }
        }
        rows
    }

    /// The rendered rows, for a failure message.
    pub(super) fn rows_text(&self) -> Vec<String> {
        (0..self.rows).map(|row| self.row_text(row)).collect()
    }

    /// The rendered text of a row, without its trailing spaces.
    fn row_text(&self, row: usize) -> String {
        self.cells[row]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn put(&mut self, character: char) {
        if self.column >= self.columns {
            self.row = (self.row + 1).min(self.rows - 1);
            self.column = 0;
        }
        self.cells[self.row][self.column] = character;
        self.column += 1;
    }

    fn escape(&mut self, characters: &mut Peekable<Chars<'_>>) {
        match characters.next() {
            Some('[') => self.control_sequence(characters),
            Some(']') => {
                // An operating system command ends with BEL or with the
                // string terminator.
                while let Some(character) = characters.next() {
                    if character == '\x07' {
                        break;
                    }
                    if character == '\x1b' {
                        characters.next();
                        break;
                    }
                }
            }
            Some('7') => self.saved = (self.row, self.column),
            Some('8') => {
                self.row = self.saved.0;
                self.column = self.saved.1;
            }
            Some('(') | Some(')') => {
                // A character set selection ends with a final byte.
                characters.next();
            }
            _ => {}
        }
    }

    fn control_sequence(&mut self, characters: &mut Peekable<Chars<'_>>) {
        let mut parameters = String::new();
        let mut final_byte = None;
        for character in characters.by_ref() {
            if ('\x40'..='\x7e').contains(&character) {
                final_byte = Some(character);
                break;
            }
            parameters.push(character);
        }
        let Some(final_byte) = final_byte else {
            return;
        };
        self.apply(&parameters, final_byte);
    }

    fn apply(&mut self, parameters: &str, final_byte: char) {
        let numbers: Vec<usize> = parameters
            .trim_start_matches('?')
            .split(';')
            .map(|value| value.parse().unwrap_or(0))
            .collect();
        let count = |index: usize| -> usize {
            match numbers.get(index) {
                Some(0) | None => 1,
                Some(value) => *value,
            }
        };
        match final_byte {
            'A' => self.row = self.row.saturating_sub(count(0)),
            'B' => self.row = (self.row + count(0)).min(self.rows - 1),
            'C' => self.column = (self.column + count(0)).min(self.columns - 1),
            'D' => self.column = self.column.saturating_sub(count(0)),
            'G' => self.column = (count(0) - 1).min(self.columns - 1),
            'H' | 'f' => {
                self.row = (count(0) - 1).min(self.rows - 1);
                self.column = (count(1) - 1).min(self.columns - 1);
            }
            '@' => self.insert_characters(count(0)),
            'P' => self.delete_characters(count(0)),
            'J' => self.erase_display(numbers.first().copied().unwrap_or(0)),
            'K' => self.erase_line(numbers.first().copied().unwrap_or(0)),
            'm' | 'h' | 'l' => {}
            's' => self.saved = (self.row, self.column),
            'u' => {
                self.row = self.saved.0;
                self.column = self.saved.1;
            }
            _ => {}
        }
    }

    /// Inserts blank characters at the cursor, pushing the rest of the row
    /// to the right.
    fn insert_characters(&mut self, count: usize) {
        let row = self.row;
        for _ in 0..count {
            self.cells[row].insert(self.column, ' ');
        }
        self.cells[row].truncate(self.columns);
    }

    /// Deletes characters at the cursor, pulling the rest of the row to the
    /// left.
    fn delete_characters(&mut self, count: usize) {
        let row = self.row;
        for _ in 0..count {
            if self.column < self.cells[row].len() {
                self.cells[row].remove(self.column);
                self.cells[row].push(' ');
            }
        }
    }

    fn erase_display(&mut self, mode: usize) {
        match mode {
            0 => {
                self.erase_line(0);
                for row in (self.row + 1)..self.rows {
                    self.cells[row] = vec![' '; self.columns];
                }
            }
            1 => {
                for row in 0..self.row {
                    self.cells[row] = vec![' '; self.columns];
                }
                self.erase_line(1);
            }
            _ => {
                for row in 0..self.rows {
                    self.cells[row] = vec![' '; self.columns];
                }
            }
        }
    }

    fn erase_line(&mut self, mode: usize) {
        let row = self.row;
        match mode {
            0 => {
                for column in self.column..self.columns {
                    self.cells[row][column] = ' ';
                }
            }
            1 => {
                for column in 0..=self.column.min(self.columns - 1) {
                    self.cells[row][column] = ' ';
                }
            }
            _ => {
                self.cells[row] = vec![' '; self.columns];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Screen;

    #[test]
    fn deleted_characters_pull_the_row_left() {
        let mut screen = Screen::new(2, 10);
        screen.feed(b"abcdef\x1b[3D\x1b[2P");
        assert_eq!(screen.row_text(0), "abcf");
    }

    #[test]
    fn inserted_characters_push_the_row_right() {
        let mut screen = Screen::new(2, 10);
        screen.feed(b"abcdef\x1b[3D\x1b[2@");
        assert_eq!(screen.row_text(0), "abc  def");
    }
}
