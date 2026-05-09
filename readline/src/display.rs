use crate::buffer::{RenderOptions, rendered_string_to_bytes};
use crate::completion::display::{
    output_ends_at_wrap_boundary, rendered_rows_for_output, visible_width,
};
use crate::editor::{Editor, ReadlineError};
use crate::keymap::KeyMapName;
use crate::prompt::Prompt;
use crate::state::{EditorState, SearchDirection};
use crate::terminal::{TerminalIo, TerminalSize};
use std::borrow::Cow;
use std::io;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn completion_display_width(&self) -> usize {
        let screen_width = self
            .terminal
            .size()
            .ok()
            .map(|size| size.columns as usize)
            .unwrap_or(80);
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
        self.terminal
            .size()
            .ok()
            .map(|size| size.rows as usize)
            .unwrap_or(24)
    }

    pub(super) fn current_prompt_width(&self, state: &EditorState) -> usize {
        self.effective_prompt(state).1
    }

    fn effective_prompt(&self, state: &EditorState) -> (String, usize) {
        if let Some(prompt) = active_search_prompt(state) {
            let width = visible_width(&prompt);
            return (prompt, width);
        }
        let mut mode = self.mode_prompt_prefix();
        if self.variable_is_on("mark-modified-lines")
            && self
                .history
                .current_history()
                .is_some_and(|entry| entry.line_bytes != state.buffer.as_bytes())
        {
            mode.push('*');
        }
        if self.variable_is_on("show-mode-in-prompt")
            && let Some(operator) = state.vi_operator_prompt()
        {
            mode.push_str(operator);
        }
        let width = visible_width(&mode) + state.prompt.width();
        (format!("{mode}{}", state.prompt.visible()), width)
    }

    pub(super) fn render_options(&self) -> RenderOptions<'_> {
        RenderOptions {
            active_region: self.variable_is_on("enable-active-region"),
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
            echo_control: self.variable_is_on("echo-control-characters"),
            output_meta: self.variable_is_on("output-meta"),
            byte_oriented: self.variable_is_on("byte-oriented"),
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
        let size = state
            .display
            .last_terminal_size
            .or_else(|| self.terminal.size().ok())
            .unwrap_or(TerminalSize {
                columns: 80,
                rows: 24,
            });
        let columns = size.columns as usize;
        let (buffer, point_width) = if self.variable_is_on("horizontal-scroll-mode") {
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
        let ends_at_wrap_boundary = output_ends_at_wrap_boundary(&rendered_output, columns);
        let rendered_rows = rendered_rows_for_output(&rendered_output, columns);
        if ends_at_wrap_boundary {
            self.terminal.write("\r\n")?;
        }
        if self.variable_is_on("horizontal-scroll-mode") {
            let column = if columns > 0 {
                (prompt_width + point_width) % columns
            } else {
                prompt_width + point_width
            };
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

    pub(crate) fn write_tracked(&mut self, state: &mut EditorState, text: &str) -> io::Result<()> {
        self.terminal.write(text)?;
        let columns = state
            .display
            .last_terminal_size
            .or_else(|| self.terminal.size().ok())
            .map(|size| size.columns as usize)
            .unwrap_or(80);
        state.display.rendered_cursor_row = state
            .display
            .rendered_cursor_row
            .saturating_add(rendered_rows_for_output(text, columns));
        Ok(())
    }

    pub(crate) fn write_tracked_bytes(
        &mut self,
        state: &mut EditorState,
        bytes: &[u8],
    ) -> io::Result<()> {
        self.terminal.write_bytes(bytes)?;
        let columns = state
            .display
            .last_terminal_size
            .or_else(|| self.terminal.size().ok())
            .map(|size| size.columns as usize)
            .unwrap_or(80);
        let text = String::from_utf8_lossy(bytes);
        state.display.rendered_cursor_row = state
            .display
            .rendered_cursor_row
            .saturating_add(rendered_rows_for_output(&text, columns));
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

    pub(super) fn mode_prompt_prefix(&self) -> String {
        if !self.variable_is_on("show-mode-in-prompt") {
            return String::new();
        }
        let raw = match self.keymap.current() {
            KeyMapName::ViCommand => self
                .variables
                .get_bytes("vi-cmd-mode-string")
                .map(Vec::as_slice)
                .map(prompt_bytes_lossless)
                .unwrap_or_else(|| "(cmd)".to_string()),
            KeyMapName::ViInsert => self
                .variables
                .get_bytes("vi-ins-mode-string")
                .map(Vec::as_slice)
                .map(prompt_bytes_lossless)
                .unwrap_or_else(|| "(ins)".to_string()),
            _ => self
                .variables
                .get_bytes("emacs-mode-string")
                .map(Vec::as_slice)
                .map(prompt_bytes_lossless)
                .unwrap_or_else(|| "@".to_string()),
        };
        Prompt::new(raw).visible().to_string()
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

fn prompt_bytes_lossless(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        if byte.is_ascii() {
            out.push(*byte as char);
        } else {
            out.push(char::from_u32(0xe000 + *byte as u32).unwrap());
        }
    }
    out
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
        let prompt_width = visible_width(self.mode_prompt_prefix().as_str()) + state.prompt.width();
        let column = prompt_width + state.buffer.display_width_until(match_pos);
        self.terminal.write("\x1b[s")?;
        self.terminal.move_to_column(column as u16)?;
        self.terminal.flush()?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        self.terminal.write("\x1b[u")?;
        Ok(())
    }
}
