//! Redisplay orchestration.
//!
//! This module converts prompt and buffer render output into terminal writes.
//! Cell-width calculations go through `crate::width`; byte rendering remains
//! owned by `buffer::render`.

pub(crate) mod horizontal;

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

    /// Terminal columns, always at least one (normalized sizes never report zero).
    pub(crate) fn tracked_terminal_columns(&self, state: &EditorState) -> usize {
        (state
            .display
            .last_terminal_size
            .map(normalize_terminal_size)
            .unwrap_or_else(|| self.usable_terminal_size())
            .columns as usize)
            .max(1)
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
        if let Some(search) = state.search.non_incremental_search.as_ref() {
            let prompt = search.prompt_text();
            let width = last_line_width(&prompt);
            return (prompt, width);
        }
        let mode = self.mode_prompt();
        let decorations = self.prompt_decorations(state);
        let width = state
            .prompt
            .width_after_prefix(mode.width() + last_line_width(&decorations));
        (
            format!("{}{decorations}{}", mode.visible(), state.prompt.visible()),
            width,
        )
    }

    fn prompt_decorations(&self, state: &EditorState) -> String {
        let mut decorations = String::new();
        if self.flag(BoolVariable::MarkModifiedLines)
            && self
                .history
                .current_history()
                .is_some_and(|entry| entry.line_bytes != state.buffer.as_bytes())
        {
            decorations.push('*');
        }
        if self.flag(BoolVariable::ShowModeInPrompt)
            && let Some(operator) = state.vi_operator_prompt()
        {
            decorations.push_str(operator);
        }
        decorations
    }

    fn horizontal_prompt(&self, state: &EditorState) -> horizontal::DisplayLine {
        if let Some(prompt) = active_search_prompt(state) {
            return Prompt::new(prompt).horizontal_line();
        }
        // Parse the raw mode Prompt separately, just like the normal Prompt:
        // marker-stripped output loses the hidden spans' cell annotations.
        let mut line = self.mode_prompt().horizontal_line();
        line.push_prompt_text(&self.prompt_decorations(state));
        line.append(state.prompt.horizontal_line());
        line
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
            output_meta: self.flag(BoolVariable::OutputMeta),
            byte_oriented: self.flag(BoolVariable::ByteOriented),
            utf8_locale: self.variables.utf8_locale(),
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
        let columns = self.tracked_terminal_columns(state);
        // GNU non-incremental search (emacs `:` and vi `/`/`?`, patch 0
        // Bash 5.3 PTY oracle) replaces the line with the search prompt
        // while the query is entered; the match appears only after Enter
        // executes. Render prompt-only here so the original line stays
        // hidden until execute/abort.
        if state.search.non_incremental_search.is_some() {
            self.terminal
                .write_bytes(&rendered_string_to_bytes(&prompt))?;
            self.terminal.clear_after_cursor()?;
            let rendered_output = prompt.clone();
            let (rendered_rows, ends_at_wrap_boundary) =
                measured_rows_for_output(&rendered_output, columns);
            if ends_at_wrap_boundary {
                self.terminal.write("\r\n")?;
            }
            state.display.rendered_rows = rendered_rows;
            state.display.rendered_cursor_row = state.display.rendered_rows;
            self.terminal
                .move_to_column((prompt_width % columns.max(1)) as u16)?;
            return self.terminal.flush();
        }
        if self.flag(BoolVariable::HorizontalScrollMode) {
            let mut line = self.horizontal_prompt(state);
            let buffer =
                state
                    .buffer
                    .horizontal_line(state.mark, self.render_options(), prompt_width);
            line.append(buffer);
            let (output, column) = state.display.horizontal.layout(&line, columns);
            self.terminal
                .write_bytes(&rendered_string_to_bytes(&output))?;
            self.terminal.clear_after_cursor()?;
            state.display.rendered_rows = rendered_rows_for_output(&output, columns);
            state.display.rendered_cursor_row = state.display.rendered_rows;
            self.terminal.move_to_column(column as u16)?;
            return self.terminal.flush();
        }
        state.display.horizontal = horizontal::HorizontalViewport::default();
        self.terminal
            .write_bytes(&rendered_string_to_bytes(&prompt))?;
        let (buffer, _) = state
            .buffer
            .render_text(state.mark, self.render_options(), prompt_width);
        self.terminal
            .write_bytes(&rendered_string_to_bytes(&buffer))?;
        self.terminal.clear_after_cursor()?;
        let rendered_output = format!("{prompt}{buffer}");
        let (rendered_rows, ends_at_wrap_boundary) =
            measured_rows_for_output(&rendered_output, columns);
        if ends_at_wrap_boundary {
            self.terminal.write("\r\n")?;
        }
        let (last_row, point_row, point_col) =
            state
                .buffer
                .rendered_rows_and_point(prompt_width, columns, self.render_options());
        state.display.rendered_rows = rendered_rows;
        let rows_back = last_row.saturating_sub(point_row) as u16;
        if rows_back > 0 {
            self.terminal.move_up(rows_back)?;
        }
        state.display.rendered_cursor_row = state.display.rendered_rows.saturating_sub(rows_back);
        self.terminal.move_to_column(point_col as u16)?;
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

    fn mode_prompt(&self) -> Prompt {
        if !self.flag(BoolVariable::ShowModeInPrompt) {
            return Prompt::new("");
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
        Prompt::new(raw)
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
        state: &mut EditorState,
        inserted: &str,
    ) -> Result<(), ReadlineError> {
        if !inserted.ends_with(')') {
            return Ok(());
        }
        let Some(match_pos) = state.buffer.matching_open_paren_before_point() else {
            return Ok(());
        };
        let prompt_width = self.current_prompt_width(state);
        let columns = self.tracked_terminal_columns(state);
        if self.flag(BoolVariable::HorizontalScrollMode) {
            // Self-insert calls blink before its ordinary redisplay. First
            // update the horizontal viewport so the mapping and the screen
            // describe the same prompt, buffer, and scroll margin.
            self.render(state)?;
            let mut buffer = state.buffer.clone();
            buffer.set_point(match_pos);
            let position = prompt_width
                + buffer
                    .horizontal_line(None, self.render_options(), prompt_width)
                    .point;
            let Some(column) = state.display.horizontal.visible_column(position) else {
                // Offscreen GNU blink semantics remain unconfirmed. Do not
                // move to a marker, clamp to another glyph, or leave the row.
                return Ok(());
            };
            self.terminal.write(escape::SAVE_CURSOR)?;
            self.terminal.move_to_column(column as u16)?;
            self.terminal.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            self.terminal.write(escape::RESTORE_CURSOR)?;
            return Ok(());
        }
        let (match_row, match_col) =
            state
                .buffer
                .rendered_position(match_pos, prompt_width, columns, self.render_options());
        let (_, point_row, _) =
            state
                .buffer
                .rendered_rows_and_point(prompt_width, columns, self.render_options());
        self.terminal.write(escape::SAVE_CURSOR)?;
        if point_row > match_row {
            self.terminal.move_up((point_row - match_row) as u16)?;
        }
        self.terminal.move_to_column(match_col as u16)?;
        self.terminal.flush()?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        self.terminal.write(escape::RESTORE_CURSOR)?;
        Ok(())
    }
}
