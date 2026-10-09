//! Redisplay orchestration.
//!
//! This module converts prompt and buffer render output into terminal writes.
//! Cell-width calculations go through `crate::width`; byte rendering remains
//! owned by `buffer::render`.

use crate::buffer::{RenderOptions, bytes_lossless, rendered_string_to_bytes};
use crate::editor::{Editor, ReadlineError};
use crate::keymap::KeyMapName;
use crate::prompt::Prompt;
use crate::state::{EditorState, SearchDirection};
use crate::terminal::{TerminalIo, TerminalSize, escape};
use crate::variables::BoolVariable;
use crate::width::{last_line_width, measured_rows_for_output, rendered_rows_for_output};
use std::borrow::Cow;
use std::io;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn usable_terminal_size(&self) -> TerminalSize {
        let size = self.terminal.size().unwrap_or(TerminalSize {
            columns: 80,
            rows: 24,
        });
        normalize_terminal_size(size)
    }

    pub(crate) fn tracked_terminal_columns(&self, state: &EditorState) -> usize {
        state
            .display
            .last_terminal_size
            .map(normalize_terminal_size)
            .unwrap_or_else(|| self.usable_terminal_size())
            .columns as usize
    }

    pub(crate) fn completion_display_width(&self) -> usize {
        let screen_width = self.usable_terminal_size().columns as usize;
        if let Some(width) = self
            .variables
            .get("completion-display-width")
            .and_then(|value| value.parse::<isize>().ok())
            .filter(|value| *value >= 0)
            .map(|value| value as usize)
            .filter(|value| *value <= screen_width)
        {
            return width;
        }
        std::env::var("COLUMNS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(screen_width)
    }

    pub(crate) fn terminal_screen_rows(&self) -> usize {
        self.usable_terminal_size().rows as usize
    }

    pub(super) fn current_prompt_width(&self, state: &EditorState) -> usize {
        self.effective_prompt(state).1
    }

    pub(crate) fn effective_prompt(&self, state: &EditorState) -> (String, usize) {
        if let Some(prompt) = active_search_prompt(state) {
            let width = last_line_width(&prompt);
            return (prompt, width);
        }
        let (mut mode, mut mode_width) = self.mode_prompt_prefix();
        if self.flag(BoolVariable::MarkModifiedLines)
            && self
                .history
                .current_history()
                .is_some_and(|entry| entry.line_bytes != state.buffer.as_bytes())
        {
            mode.push('*');
            mode_width += 1;
        }
        if self.flag(BoolVariable::ShowModeInPrompt)
            && let Some(operator) = state.vi_operator_prompt()
        {
            mode.push_str(operator);
            mode_width += last_line_width(operator);
        }
        let width = state.prompt.width_after_prefix(mode_width);
        (format!("{mode}{}", state.prompt.visible()), width)
    }

    pub(super) fn render_options(&self) -> RenderOptions<'_> {
        RenderOptions {
            active_region: self.flag(BoolVariable::EnableActiveRegion),
            active_region_start: self
                .variables
                .get_bytes("active-region-start-color")
                .map(|bytes| Cow::Borrowed(bytes.as_slice()))
                .unwrap_or(Cow::Borrowed(b"\x1b[7m")),
            active_region_end: self
                .variables
                .get_bytes("active-region-end-color")
                .map(|bytes| Cow::Borrowed(bytes.as_slice()))
                .unwrap_or(Cow::Borrowed(b"\x1b[0m")),
            echo_control: self.flag(BoolVariable::EchoControlCharacters),
            output_meta: self.flag(BoolVariable::OutputMeta),
            byte_oriented: self.flag(BoolVariable::ByteOriented),
        }
    }

    pub(super) fn render(&mut self, state: &mut EditorState) -> io::Result<()> {
        if state.display.rendered_rows > 0 {
            if state.display.rendered_cursor_row > 0 {
                self.terminal.move_up(state.display.rendered_cursor_row)?;
            }
            self.terminal.move_to_column(0)?;
            self.terminal.clear_to_screen_end()?;
        }
        self.terminal.move_to_column(0)?;
        let (prompt, prompt_width) = self.effective_prompt(state);
        self.terminal
            .write_bytes(&rendered_string_to_bytes(&prompt))?;
        let columns = self.tracked_terminal_columns(state);
        let (buffer, point_width) = if self.flag(BoolVariable::HorizontalScrollMode) {
            state.buffer.horizontal_window_with_options(
                columns.saturating_sub(prompt_width).max(1),
                state.mark,
                self.render_options(),
            )
        } else {
            state.buffer.render_text(state.mark, self.render_options())
        };
        self.terminal
            .write_bytes(&rendered_string_to_bytes(&buffer))?;
        self.terminal.clear_after_cursor()?;
        let rendered_output = format!("{prompt}{buffer}");
        let (rendered_rows, ends_at_wrap_boundary) =
            measured_rows_for_output(&rendered_output, columns);
        if ends_at_wrap_boundary {
            self.terminal.write("\r\n")?;
        }
        if self.flag(BoolVariable::HorizontalScrollMode) {
            let column = (prompt_width + point_width) % columns.max(1);
            state.display.rendered_rows = rendered_rows;
            state.display.rendered_cursor_row = state.display.rendered_rows;
            self.terminal.move_to_column(column as u16)?;
        } else {
            let (last_row, point_row, point_col) =
                state
                    .buffer
                    .rendered_rows_and_point(prompt_width, columns, self.render_options());
            state.display.rendered_rows = rendered_rows;
            let rows_back = last_row.saturating_sub(point_row) as u16;
            if rows_back > 0 {
                self.terminal.move_up(rows_back)?;
            }
            state.display.rendered_cursor_row =
                state.display.rendered_rows.saturating_sub(rows_back);
            self.terminal.move_to_column(point_col as u16)?;
        }
        self.terminal.flush()
    }

    pub(crate) fn write_tracked_newline(&mut self, state: &mut EditorState) -> io::Result<()> {
        self.terminal.write("\r\n")?;
        state.display.rendered_cursor_row = state.display.rendered_cursor_row.saturating_add(1);
        Ok(())
    }

    fn note_tracked_output(&self, state: &mut EditorState, text: &str) {
        let columns = self.tracked_terminal_columns(state);
        state.display.rendered_cursor_row = state
            .display
            .rendered_cursor_row
            .saturating_add(rendered_rows_for_output(text, columns));
    }

    pub(crate) fn write_tracked(&mut self, state: &mut EditorState, text: &str) -> io::Result<()> {
        self.terminal.write(text)?;
        self.note_tracked_output(state, text);
        Ok(())
    }

    pub(crate) fn write_tracked_bytes(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
    ) -> io::Result<()> {
        self.terminal.write_bytes(bytes)?;
        self.note_tracked_output(state, &String::from_utf8_lossy(bytes));
        Ok(())
    }

    pub(crate) fn write_below_rendered_line(
        &mut self,
        state: &mut EditorState,
        text: &str,
    ) -> io::Result<()> {
        self.move_below_rendered_line(state)?;
        self.write_tracked(state, text)
    }

    pub(crate) fn move_below_rendered_line(&mut self, state: &mut EditorState) -> io::Result<()> {
        let rows_down = state
            .display
            .rendered_rows
            .saturating_sub(state.display.rendered_cursor_row)
            .saturating_add(1);
        for _ in 0..rows_down {
            self.terminal.write("\r\n")?;
        }
        state.display.rendered_cursor_row = state.display.rendered_rows.saturating_add(1);
        Ok(())
    }

    pub(crate) fn clear_display_and_reset(&mut self, state: &mut EditorState) -> io::Result<()> {
        self.terminal.clear_display()?;
        state.display.rendered_rows = 0;
        state.display.rendered_cursor_row = 0;
        Ok(())
    }

    pub(super) fn mode_prompt_prefix(&self) -> (String, usize) {
        if !self.flag(BoolVariable::ShowModeInPrompt) {
            return (String::new(), 0);
        }
        let raw = match self.keymap.current() {
            KeyMapName::ViCommand => self
                .variables
                .get_bytes("vi-cmd-mode-string")
                .map(Vec::as_slice)
                .map(bytes_lossless)
                .unwrap_or_else(|| "(cmd)".to_string()),
            KeyMapName::ViInsert => self
                .variables
                .get_bytes("vi-ins-mode-string")
                .map(Vec::as_slice)
                .map(bytes_lossless)
                .unwrap_or_else(|| "(ins)".to_string()),
            _ => self
                .variables
                .get_bytes("emacs-mode-string")
                .map(Vec::as_slice)
                .map(bytes_lossless)
                .unwrap_or_else(|| "@".to_string()),
        };
        let prompt = Prompt::new(raw);
        (prompt.visible().to_string(), prompt.width())
    }
}

fn normalize_terminal_size(size: TerminalSize) -> TerminalSize {
    TerminalSize {
        columns: if size.columns == 0 { 80 } else { size.columns },
        rows: if size.rows == 0 { 24 } else { size.rows },
    }
}

fn active_search_prompt(state: &EditorState) -> Option<String> {
    let search = state.search.reverse_search.as_ref()?;
    let direction = match search.direction {
        SearchDirection::Backward => "reverse-i-search",
        SearchDirection::Forward => "i-search",
    };
    let failed = if search.query.is_empty() || search.match_line.is_some() {
        ""
    } else {
        "failed "
    };
    let query = String::from_utf8_lossy(&search.query);
    Some(format!("({failed}{direction})`{query}': "))
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn blink_matching_paren(
        &mut self,
        state: &EditorState,
        inserted: &str,
    ) -> Result<(), ReadlineError> {
        if !inserted.ends_with(')') {
            return Ok(());
        }
        let Some(match_pos) = state.buffer.matching_open_paren_before_point() else {
            return Ok(());
        };
        let prompt_width = self.current_prompt_width(state);
        let column = prompt_width
            + state
                .buffer
                .rendered_width_until(match_pos, &self.render_options());
        self.terminal.write(escape::SAVE_CURSOR)?;
        self.terminal.move_to_column(column as u16)?;
        self.terminal.flush()?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        self.terminal.write(escape::RESTORE_CURSOR)?;
        Ok(())
    }
}
