//! Pure buffer rendering.
//!
//! This module owns conversion from buffer bytes to display strings and point
//! widths, including control-character and meta-byte display semantics.

use super::{LineBuffer, private_byte_char, private_byte_value};
use crate::width::{advance_cell, char_width};
use std::borrow::Cow;

#[derive(Debug, Clone)]
pub struct RenderOptions<'a> {
    pub active_region: bool,
    pub active_region_start: Cow<'a, [u8]>,
    pub active_region_end: Cow<'a, [u8]>,
    pub echo_control: bool,
    pub output_meta: bool,
    pub byte_oriented: bool,
}

impl Default for RenderOptions<'static> {
    fn default() -> Self {
        Self {
            active_region: false,
            active_region_start: Cow::Borrowed(b""),
            active_region_end: Cow::Borrowed(b""),
            echo_control: true,
            output_meta: true,
            byte_oriented: false,
        }
    }
}

/// Spaces rendering a TAB at unwrapped absolute column `col`: terminal tab
/// stops every 8 columns.
pub(crate) fn tab_expansion(col: usize) -> &'static str {
    const SPACES: &str = "        ";
    &SPACES[..8 - col % 8]
}

pub(super) fn rendered_char_width(ch: char, col: usize, options: &RenderOptions<'_>) -> usize {
    display_char(
        ch,
        options.echo_control,
        options.output_meta,
        options.byte_oriented,
        col,
    )
    .chars()
    .map(|ch| if ch == '\n' { 0 } else { char_width(ch) })
    .sum()
}

pub(super) fn display_char(
    ch: char,
    echo_control: bool,
    output_meta: bool,
    byte_oriented: bool,
    col: usize,
) -> String {
    if let Some(byte) = private_byte_value(ch) {
        if byte_oriented || !output_meta || byte.is_ascii_control() {
            return format!("\\{byte:03o}");
        }
        return (byte as char).to_string();
    }
    if (byte_oriented || !output_meta) && !ch.is_ascii() {
        return ch
            .to_string()
            .as_bytes()
            .iter()
            .map(|byte| format!("\\{byte:03o}"))
            .collect();
    }
    // GNU expands TAB to spaces up to the next multiple-of-8 tab stop
    // (patch 0 Bash 5.3 PTY oracle: `a<TAB>b` after the 15-column
    // `SUSHLINE_READY>` prompt renders 8 spaces; a lone TAB one space).
    // Unconditional: the oracle expands even with `echo-control-characters`
    // off. Tab stops use the unwrapped absolute column from line start so
    // the buffer render and screen-position math share one basis.
    if ch == '\t' {
        return tab_expansion(col).to_string();
    }
    if !echo_control {
        return ch.to_string();
    }
    match ch {
        '\n' => "\r\n".to_string(),
        '\x00'..='\x1f' => {
            let caret = char::from_u32((ch as u32) + 0x40).unwrap_or('@');
            format!("^{caret}")
        }
        '\x7f' => "^?".to_string(),
        _ => ch.to_string(),
    }
}

pub(crate) fn append_bytes_lossless(out: &mut String, bytes: &[u8]) {
    for byte in bytes {
        if byte.is_ascii() {
            out.push(*byte as char);
        } else {
            out.push(private_byte_char(*byte));
        }
    }
}

pub(crate) fn bytes_lossless(bytes: &[u8]) -> String {
    let mut out = String::new();
    append_bytes_lossless(&mut out, bytes);
    out
}

fn break_sequence_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 1,
    }
}

pub(crate) fn append_bytes_decoded_lossless(out: &mut String, bytes: &[u8]) {
    let mut idx = 0;
    while idx < bytes.len() {
        let first = bytes[idx];
        let needed = break_sequence_len(first);
        if needed > 1
            && idx + needed <= bytes.len()
            && let Ok(text) = std::str::from_utf8(&bytes[idx..idx + needed])
            && let Some(ch) = text.chars().next()
        {
            out.push(ch);
            idx += needed;
            continue;
        }
        if first.is_ascii() {
            out.push(first as char);
        } else {
            out.push(private_byte_char(first));
        }
        idx += 1;
    }
}

pub(crate) fn bytes_decoded_lossless(bytes: &[u8]) -> String {
    let mut out = String::new();
    append_bytes_decoded_lossless(&mut out, bytes);
    out
}

pub(crate) fn rendered_string_to_bytes(rendered: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(rendered.len());
    for ch in rendered.chars() {
        if let Some(byte) = private_byte_value(ch) {
            out.push(byte);
        } else {
            let mut buf = [0; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }
    out
}

impl LineBuffer {
    /// Move screen line.
    pub(crate) fn move_screen_line(
        &mut self,
        prompt_width: usize,
        columns: usize,
        rows: isize,
        options: RenderOptions<'_>,
    ) {
        let columns = columns.max(1);
        let positions = self.screen_positions(prompt_width, columns, options);
        let Some(&(current_row, current_col)) = positions
            .iter()
            .find_map(|(idx, pos)| (*idx == self.point).then_some(pos))
        else {
            return;
        };
        let target_row = if rows < 0 {
            current_row.saturating_sub(rows.unsigned_abs())
        } else {
            current_row.saturating_add(rows as usize)
        };
        let mut best = None;
        for (idx, (row, col)) in positions {
            if row != target_row {
                continue;
            }
            if col >= current_col {
                self.point = idx;
                return;
            }
            best = Some(idx);
        }
        if let Some(idx) = best {
            self.point = idx;
        }
    }

    fn screen_positions(
        &self,
        prompt_width: usize,
        columns: usize,
        options: RenderOptions<'_>,
    ) -> Vec<(usize, (usize, usize))> {
        let mut positions = Vec::new();
        let mut row = prompt_width / columns;
        let mut col = prompt_width % columns;
        // Unwrapped absolute column from line start; TAB expansion shares
        // this basis with `render_text` so cursor math and output agree.
        let mut abs_col = prompt_width;
        positions.push((0, (row, col)));
        for (idx, ch) in self.decoded_char_indices() {
            let display = display_char(
                ch,
                options.echo_control,
                options.output_meta,
                options.byte_oriented,
                abs_col,
            );
            for rendered in display.chars() {
                if rendered == '\n' {
                    row += 1;
                    col = 0;
                    // A newline also restarts the tab-stop basis like a row.
                    abs_col = 0;
                    continue;
                }
                let (added, next) = advance_cell(col, char_width(rendered), columns);
                row += added;
                col = next;
                abs_col += char_width(rendered);
            }
            positions.push((self.next_char_boundary(idx), (row, col)));
        }
        positions
    }

    /// Horizontal window with options.
    pub(crate) fn horizontal_window_with_options(
        &self,
        max_width: usize,
        mark: Option<usize>,
        options: RenderOptions<'_>,
    ) -> (String, usize) {
        if max_width == 0 {
            return (String::new(), 0);
        }
        // Cumulative window-relative sizing: each candidate width is
        // measured from the candidate window start so per-grapheme TAB
        // stops agree with the cumulative `rel_col` render below.
        let mut start = self.point;
        while let Some(prev) = self.prev_grapheme_boundary_checked(start) {
            if self.window_slice_width(prev, self.point, &options) > max_width.saturating_sub(1) {
                break;
            }
            start = prev;
        }
        let width = self.window_slice_width(start, self.point, &options);
        let mut end = self.point;
        while end < self.bytes.len() {
            let next = self.next_grapheme_boundary(end);
            if self.window_slice_width(start, next, &options) > max_width {
                break;
            }
            end = next;
        }
        let region = self.region(mark, options.active_region);
        let mut visible = String::new();
        // Window-relative tab stops: the scrolled window has no prompt
        // context, so stops restart at the window start (self-consistent
        // within horizontal-scroll mode).
        let mut rel_col = 0usize;
        for (idx, ch) in self.decoded_char_indices_in_range(start, end) {
            if Some(idx) == region.map(|(region_start, _)| region_start) {
                append_bytes_lossless(&mut visible, options.active_region_start.as_ref());
            }
            if Some(idx) == region.map(|(_, region_end)| region_end) {
                append_bytes_lossless(&mut visible, options.active_region_end.as_ref());
            }
            let display = display_char(
                ch,
                options.echo_control,
                options.output_meta,
                options.byte_oriented,
                rel_col,
            );
            if ch == '\n' {
                rel_col = 0;
            } else {
                rel_col += display.chars().map(char_width).sum::<usize>();
            }
            visible.push_str(&display);
        }
        if region.is_some_and(|(_, region_end)| region_end == end) {
            append_bytes_lossless(&mut visible, options.active_region_end.as_ref());
        }
        (visible, width)
    }

    /// Render text.
    ///
    /// `base_col` is the unwrapped absolute column where buffer display
    /// starts (the prompt last-line width); TAB stops count from it, sharing
    /// the basis with `screen_positions`.
    pub(crate) fn render_text(
        &self,
        mark: Option<usize>,
        options: RenderOptions<'_>,
        base_col: usize,
    ) -> (String, usize) {
        let region = self.region(mark, options.active_region);
        let mut out = String::new();
        let mut width = 0;
        let mut point_width = 0;
        // TAB stops share the `screen_positions` basis: the unwrapped
        // absolute column from line start, restarted after each newline.
        let mut tab_col = base_col;
        for (idx, ch) in self.decoded_char_indices() {
            if Some(idx) == region.map(|(start, _)| start) {
                append_bytes_lossless(&mut out, options.active_region_start.as_ref());
            }
            if Some(idx) == region.map(|(_, end)| end) {
                append_bytes_lossless(&mut out, options.active_region_end.as_ref());
            }
            if idx == self.point {
                point_width = width;
            }
            if ch == '\n' {
                let display = display_char(
                    ch,
                    options.echo_control,
                    options.output_meta,
                    options.byte_oriented,
                    tab_col,
                );
                out.push_str(&display);
                tab_col = 0;
                continue;
            }
            let display = display_char(
                ch,
                options.echo_control,
                options.output_meta,
                options.byte_oriented,
                tab_col,
            );
            let w = display.chars().map(char_width).sum::<usize>();
            width += w;
            tab_col += w;
            out.push_str(&display);
        }
        if self.point == self.bytes.len() {
            point_width = width;
        }
        if region.is_some_and(|(_, end)| end == self.bytes.len()) {
            append_bytes_lossless(&mut out, options.active_region_end.as_ref());
        }
        (out, point_width)
    }

    /// Rendered rows and point.
    pub(crate) fn rendered_rows_and_point(
        &self,
        prompt_width: usize,
        columns: usize,
        options: RenderOptions<'_>,
    ) -> (usize, usize, usize) {
        let positions = self.screen_positions(prompt_width, columns.max(1), options);
        let (point_row, point_col) = positions
            .iter()
            .find_map(|(idx, pos)| (*idx == self.point).then_some(*pos))
            .unwrap_or((0, 0));
        let total_row = positions.last().map(|(_, (row, _))| *row).unwrap_or(0);
        (total_row, point_row, point_col)
    }

    /// Screen (row, column) for a buffer byte index, wrapping at `columns`.
    pub(crate) fn rendered_position(
        &self,
        target: usize,
        prompt_width: usize,
        columns: usize,
        options: RenderOptions<'_>,
    ) -> (usize, usize) {
        let positions = self.screen_positions(prompt_width, columns.max(1), options);
        positions
            .iter()
            .find_map(|(idx, pos)| (*idx == target.min(self.bytes.len())).then_some(*pos))
            .unwrap_or((0, prompt_width % columns.max(1)))
    }

    pub(crate) fn rendered_width_until(&self, end: usize, options: &RenderOptions<'_>) -> usize {
        self.rendered_slice_width(0, end.min(self.bytes.len()), options)
    }

    fn rendered_slice_width(&self, start: usize, end: usize, options: &RenderOptions<'_>) -> usize {
        // Slice-relative tab stops (horizontal-scroll windowing has no
        // prompt context); matches the window-relative render above.
        // `tab_col` restarts after each newline like `screen_positions`
        // so a TAB after `\\n` uses the new line as its basis, while the
        // returned width stays cumulative (newlines are zero width).
        let mut width = 0usize;
        let mut tab_col = 0usize;
        for ch in self.decoded_chars_in_range(start, end) {
            if ch == '\n' {
                tab_col = 0;
                continue;
            }
            let w = rendered_char_width(ch, tab_col, options);
            width += w;
            tab_col += w;
        }
        width
    }

    /// Cumulative window width from `start` to `end` on a window-relative
    /// tab basis (stops restart at `start` and after each newline).
    fn window_slice_width(&self, start: usize, end: usize, options: &RenderOptions<'_>) -> usize {
        self.rendered_slice_width(start, end, options)
    }

    fn region(&self, mark: Option<usize>, active: bool) -> Option<(usize, usize)> {
        if !active {
            return None;
        }
        let mark = self.clamp_boundary(mark?);
        let start = mark.min(self.point);
        let end = mark.max(self.point);
        (start != end).then_some((start, end))
    }
}
