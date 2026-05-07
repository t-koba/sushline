use crate::completion::{CompletionCandidate, CompletionResponse};
use std::cmp::Ordering;
use std::ffi::CString;

pub(crate) fn common_prefix_bytes(candidates: &[CompletionCandidate]) -> Option<Vec<u8>> {
    let first = candidates.first()?.replacement_bytes().to_vec();
    let mut prefix = first;
    for candidate in &candidates[1..] {
        let bytes = candidate.replacement_bytes();
        while !bytes.starts_with(&prefix) {
            prefix.pop()?;
        }
    }
    (!prefix.is_empty()).then_some(prefix)
}

pub(crate) fn abbreviate_completion_prefix(items: &mut [String], prefix: &str, filenames: bool) {
    let marker = if filenames && prefix.starts_with('.') {
        "___"
    } else {
        "..."
    };
    for item in items {
        if let Some(rest) = item.strip_prefix(prefix) {
            *item = format!("{marker}{rest}");
        }
    }
}

pub(crate) fn sort_completion_response(response: &mut CompletionResponse) {
    if response.options.nosort {
        return;
    }
    response
        .candidates
        .sort_by(|a, b| match (a.display.as_deref(), b.display.as_deref()) {
            (Some(a), Some(b)) => compare_with_current_locale(a.as_bytes(), b.as_bytes()),
            (Some(a), None) => compare_with_current_locale(a.as_bytes(), b.replacement_bytes()),
            (None, Some(b)) => compare_with_current_locale(a.replacement_bytes(), b.as_bytes()),
            (None, None) => {
                compare_with_current_locale(a.replacement_bytes(), b.replacement_bytes())
            }
        });
    response
        .candidates
        .dedup_by(|a, b| a.replacement_bytes() == b.replacement_bytes());
}

fn compare_with_current_locale(a: &[u8], b: &[u8]) -> Ordering {
    let ordering = match (CString::new(a), CString::new(b)) {
        (Ok(a), Ok(b)) => {
            let result = unsafe { libc::strcoll(a.as_ptr(), b.as_ptr()) };
            result.cmp(&0)
        }
        _ => return a.cmp(b),
    };
    ordering.then_with(|| a.cmp(b))
}

pub(crate) fn merge_extended_completion_options(
    target: &mut crate::completion::CompletionOptions,
    source: crate::completion::CompletionOptions,
) {
    target.replacement_prefix = target
        .replacement_prefix
        .take()
        .or(source.replacement_prefix);
    target.replacement_suffix = target
        .replacement_suffix
        .take()
        .or(source.replacement_suffix);
    target.filter_prefix = target.filter_prefix.take().or(source.filter_prefix);
    target.filter_suffix = target.filter_suffix.take().or(source.filter_suffix);
    target.action = target.action.or(source.action);
}

pub(crate) fn apply_extended_completion_options(response: &mut CompletionResponse) {
    if let Some(prefix) = &response.options.filter_prefix {
        response
            .candidates
            .retain(|candidate| candidate.replacement_bytes().starts_with(prefix));
    }
    if let Some(suffix) = &response.options.filter_suffix {
        response
            .candidates
            .retain(|candidate| candidate.replacement_bytes().ends_with(suffix));
    }
    if response.options.replacement_prefix.is_some()
        || response.options.replacement_suffix.is_some()
    {
        let prefix_bytes = response
            .options
            .replacement_prefix
            .as_deref()
            .unwrap_or(b"");
        let suffix_bytes = response
            .options
            .replacement_suffix
            .as_deref()
            .unwrap_or(b"");
        for candidate in &mut response.candidates {
            let mut replacement_bytes = Vec::with_capacity(
                prefix_bytes.len() + candidate.replacement_bytes().len() + suffix_bytes.len(),
            );
            replacement_bytes.extend_from_slice(prefix_bytes);
            replacement_bytes.extend_from_slice(candidate.replacement_bytes());
            replacement_bytes.extend_from_slice(suffix_bytes);
            candidate.replacement = replacement_bytes;
        }
    }
}

pub(crate) fn format_completion_items_with_trailing(
    items: &[String],
    display_width: usize,
    horizontally: bool,
    keep_trailing_padding: bool,
) -> Vec<String> {
    if items.is_empty() {
        return Vec::new();
    }
    let item_width = items
        .iter()
        .map(|item| visible_width(item))
        .max()
        .unwrap_or(0)
        + 2;
    let columns = (display_width / item_width.max(1)).max(1);
    let rows = items.len().div_ceil(columns);
    let mut lines = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut line = String::new();
        for col in 0..columns {
            let idx = if horizontally {
                row * columns + col
            } else {
                col * rows + row
            };
            let Some(item) = items.get(idx) else {
                continue;
            };
            line.push_str(item);
            let padding = item_width.saturating_sub(visible_width(item));
            if col + 1 < columns || keep_trailing_padding {
                line.push_str(&" ".repeat(padding));
            }
        }
        if keep_trailing_padding {
            lines.push(line);
        } else {
            lines.push(line.trim_end().to_string());
        }
    }
    lines
}

pub(crate) fn color_completion_prefix(item: &str, replacement: &str, prefix: &str) -> String {
    if prefix.is_empty() || !replacement.starts_with(prefix) {
        return item.to_string();
    }
    let color = ls_color_named_code("readline-colored-completion-prefix")
        .unwrap_or_else(|| "1".to_string());
    let prefix_chars = prefix.chars().count();
    let split = item
        .char_indices()
        .nth(prefix_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(item.len());
    format!("\x1b[{color}m{}\x1b[0m{}", &item[..split], &item[split..])
}

fn ls_color_named_code(name: &str) -> Option<String> {
    let colors = std::env::var("LS_COLORS").ok()?;
    colors.split(':').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

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
            for ch in chars.by_ref() {
                if ch.is_ascii_alphabetic() {
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
            width += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }
    width
}

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
        let width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width > 0 && col + width > columns {
            row += 1;
            col = 0;
        }
        col += width;
        if col >= columns {
            row += col / columns;
            col %= columns;
        }
    }
    row as u16
}

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

use crate::completion::builtin::visible_stats_marker;
use crate::completion::filename::{
    FilenameOptions, filename_directory_completion, filename_display_name,
};
use crate::editor::{Editor, ReadlineError};
#[cfg(test)]
use crate::prompt::Prompt;
use crate::state::EditorState;
use crate::terminal::{TerminalEvent, TerminalIo};

impl<T> Editor<T>
where
    T: TerminalIo,
{
    #[cfg(test)]
    pub(crate) fn display_completions(
        &mut self,
        response: &CompletionResponse,
    ) -> Result<(), ReadlineError> {
        let mut state = EditorState::new(Prompt::new(""), None);
        self.display_completions_for_word(&mut state, response, b"")
    }

    pub(crate) fn display_completions_for_word(
        &mut self,
        state: &mut EditorState,
        response: &CompletionResponse,
        word: &[u8],
    ) -> Result<(), ReadlineError> {
        if response.candidates.is_empty() {
            self.ding()?;
            return Ok(());
        }
        let query_items = self
            .variables
            .get("completion-query-items")
            .and_then(|value| value.parse::<isize>().ok())
            .unwrap_or(100);
        let displayed_query = self.variable_is_on("page-completions")
            && query_items > 0
            && response.candidates.len() >= query_items as usize;
        if displayed_query {
            self.move_below_rendered_line(state)?;
            self.write_tracked(
                state,
                &format!(
                    "Display all {} possibilities? (y or n)",
                    response.candidates.len()
                ),
            )?;
            self.terminal.flush()?;
            match self.terminal.read_event(None)? {
                TerminalEvent::Bytes(bytes)
                    if matches!(bytes.as_slice(), b"y" | b"Y" | b" " | b"\t" | b"\r" | b"\n") => {}
                TerminalEvent::Bytes(_) => {
                    self.write_tracked_newline(state)?;
                    return Ok(());
                }
                TerminalEvent::Resize(_) | TerminalEvent::Timeout => {}
                TerminalEvent::Signal(signal) => {
                    let _ = self.handle_terminal_signal(state, signal)?;
                    self.write_tracked_newline(state)?;
                    return Ok(());
                }
            }
        }

        let mut items = response
            .candidates
            .iter()
            .map(|candidate| {
                let mut item = candidate
                    .display
                    .as_deref()
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        if response.options.filenames {
                            filename_display_name(candidate.replacement_bytes())
                        } else {
                            self.render_completion_bytes(candidate.replacement_bytes())
                        }
                    });
                if response.options.filenames
                    && !item.contains("\x1b[")
                    && let Some(directory) = filename_directory_completion(
                        word,
                        candidate.replacement_bytes(),
                        &FilenameOptions::from_variables(&self.variables),
                    )
                    && directory.display_slash
                    && !item.ends_with('/')
                {
                    item.push('/');
                }
                if self.variable_is_on("visible-stats")
                    && response.options.filenames
                    && !item.contains("\x1b[")
                    && let Some(marker) = visible_stats_marker(&candidate.replacement_string())
                    && !item.ends_with(marker)
                {
                    item.push(marker);
                }
                item
            })
            .collect::<Vec<_>>();
        let common_prefix = common_prefix_bytes(&response.candidates)
            .map(|bytes| self.render_completion_bytes(&bytes));
        if let Some(prefix) = common_prefix.as_deref() {
            let limit = self
                .variables
                .get("completion-prefix-display-length")
                .and_then(|value| value.parse::<isize>().ok())
                .filter(|value| *value > 0)
                .map(|value| value as usize)
                .unwrap_or(0);
            if limit > 0 && prefix.chars().count() > limit {
                abbreviate_completion_prefix(&mut items, prefix, response.options.filenames);
            }
        }
        if self.variable_is_on("colored-completion-prefix")
            && let Some(prefix) = common_prefix.as_deref()
        {
            for (item, candidate) in items.iter_mut().zip(response.candidates.iter()) {
                *item = color_completion_prefix(item, &candidate.replacement_string(), prefix);
            }
        }
        if self.variable_is_on("visible-stats") && !response.options.filenames {
            for item in &mut items {
                if !item.contains("\x1b[") {
                    item.push(' ');
                }
            }
        }

        if displayed_query {
            self.write_tracked_newline(state)?;
        } else {
            self.move_below_rendered_line(state)?;
        }
        let lines = format_completion_items_with_trailing(
            &items,
            self.completion_display_width(),
            self.variable_is_on("print-completions-horizontally"),
            false,
        );
        let page_rows = self.terminal_screen_rows().saturating_sub(1).max(1);
        let mut idx = 0;
        let mut page_remaining = page_rows;
        while idx < lines.len() {
            if self.variable_is_on("page-completions") && idx > 0 && page_remaining == 0 {
                let more_prompt = "--More--";
                self.terminal.write(more_prompt)?;
                self.terminal.flush()?;
                match self.terminal.read_event(None)? {
                    TerminalEvent::Bytes(bytes) if matches!(bytes.as_slice(), b"q" | b"Q") => {
                        self.write_tracked_newline(state)?;
                        return Ok(());
                    }
                    TerminalEvent::Bytes(bytes) if matches!(bytes.as_slice(), b"\r" | b"\n") => {
                        page_remaining = 1;
                    }
                    TerminalEvent::Bytes(bytes) if matches!(bytes.as_slice(), b" " | b"\t") => {
                        page_remaining = page_rows;
                    }
                    TerminalEvent::Bytes(_) | TerminalEvent::Resize(_) | TerminalEvent::Timeout => {
                        page_remaining = page_rows;
                    }
                    TerminalEvent::Signal(signal) => {
                        let _ = self.handle_terminal_signal(state, signal)?;
                        self.write_tracked_newline(state)?;
                        return Ok(());
                    }
                }
                let columns = state
                    .display
                    .last_terminal_size
                    .or_else(|| self.terminal.size().ok())
                    .map(|size| size.columns as usize)
                    .unwrap_or(80);
                let more_rows = rendered_rows_for_output(more_prompt, columns);
                if more_rows > 0 {
                    self.terminal.move_up(more_rows)?;
                }
                self.terminal.move_to_column(0)?;
                self.terminal.clear_to_screen_end()?;
            }
            let line_bytes = crate::buffer::rendered_string_to_bytes(&lines[idx]);
            self.write_tracked_bytes(state, &line_bytes)?;
            self.write_tracked_newline(state)?;
            idx += 1;
            page_remaining = page_remaining.saturating_sub(1);
        }
        Ok(())
    }

    fn render_completion_bytes(&self, bytes: &[u8]) -> String {
        crate::buffer::LineBuffer::from_bytes(bytes.to_vec())
            .render_text(None, self.render_options())
            .0
    }
}
