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
    pub output_meta: bool,
    pub byte_oriented: bool,
    pub utf8_locale: bool,
}

impl Default for RenderOptions<'static> {
    fn default() -> Self {
        Self {
            active_region: false,
            active_region_start: Cow::Borrowed(b""),
            active_region_end: Cow::Borrowed(b""),
            output_meta: true,
            byte_oriented: false,
            utf8_locale: true,
        }
    }
}

/// Spaces rendering a TAB at unwrapped absolute column `col`: terminal tab
/// stops every 8 columns.
pub(crate) fn tab_expansion(col: usize) -> &'static str {
    const SPACES: &str = "        ";
    &SPACES[..8 - col % 8]
}

pub(super) fn display_char(
    ch: char,
    output_meta: bool,
    byte_oriented: bool,
    utf8_locale: bool,
    col: usize,
) -> String {
    if let Some(byte) = private_byte_value(ch) {
        if !output_meta || byte.is_ascii_control() {
            return format!("\\{byte:03o}");
        }
        return private_byte_char(byte).to_string();
    }
    // UTF-8 locales preserve valid multibyte characters in character mode.
    // Byte-oriented mode or a non-UTF-8 locale renders constituent bytes;
    // `output-meta` selects raw bytes versus octal notation.
    if !ch.is_ascii() && (byte_oriented || !utf8_locale) {
        return ch
            .to_string()
            .as_bytes()
            .iter()
            .map(|byte| {
                if output_meta {
                    private_byte_char(*byte).to_string()
                } else {
                    format!("\\{byte:03o}")
                }
            })
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
    // Bash 5.3 PTY probes keep quoted-inserted SOH and DEL caret-visible
    // regardless of `echo-control-characters`.
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

/// Whether `ch` renders as a per-byte expansion under these options: a valid
/// non-ASCII scalar with byte-oriented mode or a non-UTF-8 locale.
fn renders_byte_split(ch: char, byte_oriented: bool, utf8_locale: bool) -> bool {
    !ch.is_ascii() && private_byte_value(ch).is_none() && (byte_oriented || !utf8_locale)
}

/// Display prefix for the first `prefix_len` bytes of `ch`'s UTF-8 encoding
/// under byte-split rendering. The caller guarantees the byte-split case and
/// `prefix_len <= ch.len_utf8()`; the full-length prefix equals the
/// whole-character `display_char` expansion, so rendered bytes never change.
fn byte_split_prefix(ch: char, prefix_len: usize, output_meta: bool) -> String {
    let mut buf = [0; 4];
    let bytes = ch.encode_utf8(&mut buf).as_bytes().to_vec();
    bytes[..prefix_len.min(bytes.len())]
        .iter()
        .map(|byte| {
            if output_meta {
                private_byte_char(*byte).to_string()
            } else {
                format!("\\{byte:03o}")
            }
        })
        .collect()
}

/// Cell width of a byte-split prefix without changing rendered bytes.
fn byte_split_prefix_width(ch: char, prefix_len: usize, output_meta: bool) -> usize {
    byte_split_prefix(ch, prefix_len, output_meta)
        .chars()
        .map(char_width)
        .sum()
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
        let mut best: Option<(usize, usize)> = None;
        for (idx, (row, col)) in positions {
            if row != target_row {
                continue;
            }
            if col >= current_col {
                self.point = idx;
                return;
            }
            // Lower-column fallback keeps the nearest cell: interior bytes
            // of a valid scalar share the scalar-start cell, so only a
            // strictly nearer column replaces the candidate. This retains
            // the first byte index at each cell, which `screen_positions`
            // orders as the scalar boundary.
            match best {
                Some((best_col, _)) if col <= best_col => {}
                _ => best = Some((col, idx)),
            }
        }
        if let Some((_, idx)) = best {
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
            let start_row = row;
            let start_col = col;
            let display = display_char(
                ch,
                options.output_meta,
                options.byte_oriented,
                options.utf8_locale,
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
            let next = self.next_char_boundary(idx);
            // Byte-wise motion can rest inside a valid UTF-8 sequence. Record
            // each interior byte so point lookups stay exact; the rendered
            // bytes are unchanged, only the position table gains entries.
            // UTF-8 scalar rendering keeps the scalar start (matching
            // `render_text` and `horizontal_line`); byte-split rendering
            // advances by the rendered prefix.
            if next > idx + 1 {
                if renders_byte_split(ch, options.byte_oriented, options.utf8_locale) {
                    let byte_len = next - idx;
                    for prefix_len in 1..byte_len {
                        let prefix = byte_split_prefix(ch, prefix_len, options.output_meta);
                        let mut row_k = start_row;
                        let mut col_k = start_col;
                        for rendered in prefix.chars() {
                            let (added, next_col) =
                                advance_cell(col_k, char_width(rendered), columns);
                            row_k += added;
                            col_k = next_col;
                        }
                        positions.push((idx + prefix_len, (row_k, col_k)));
                    }
                } else {
                    for interior in (idx + 1)..next {
                        positions.push((interior, (start_row, start_col)));
                    }
                }
            }
            positions.push((next, (row, col)));
        }
        positions
    }

    /// Indivisible display units on the prompt's absolute tab-stop basis.
    pub(crate) fn horizontal_line(
        &self,
        mark: Option<usize>,
        options: RenderOptions<'_>,
        base_col: usize,
    ) -> crate::display::horizontal::DisplayLine {
        let mut line = crate::display::horizontal::DisplayLine::default();
        let region = self.region(mark, options.active_region);
        for (idx, ch) in self.decoded_char_indices() {
            if Some(idx) == region.map(|(start, _)| start) {
                line.push_hidden(bytes_lossless(options.active_region_start.as_ref()));
            }
            if Some(idx) == region.map(|(_, end)| end) {
                line.push_hidden(bytes_lossless(options.active_region_end.as_ref()));
            }
            if idx == self.point {
                line.point = line.width;
            } else {
                let next = self.next_char_boundary(idx);
                if self.point > idx && self.point < next {
                    // Interior of a valid sequence: UTF-8 scalar mode keeps
                    // the cell start; byte-split mode advances by the rendered
                    // prefix so the cursor sits between the constituent bytes.
                    line.point = line.width
                        + if renders_byte_split(ch, options.byte_oriented, options.utf8_locale) {
                            byte_split_prefix_width(ch, self.point - idx, options.output_meta)
                        } else {
                            0
                        };
                }
            }
            // Keep horizontal mode on one row even for quoted newlines.
            let display = if ch == '\n' {
                "^J".to_string()
            } else {
                display_char(
                    ch,
                    options.output_meta,
                    options.byte_oriented,
                    options.utf8_locale,
                    base_col + line.width,
                )
            };
            let width = display.chars().map(char_width).sum();
            line.push_cell(display, width);
        }
        if self.point == self.bytes.len() {
            line.point = line.width;
        }
        if region.is_some_and(|(_, end)| end == self.bytes.len()) {
            line.push_hidden(bytes_lossless(options.active_region_end.as_ref()));
        }
        line
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
            } else {
                let next = self.next_char_boundary(idx);
                if self.point > idx && self.point < next {
                    // Interior of a valid sequence without changing rendered
                    // bytes: UTF-8 scalar mode keeps the unit start (only
                    // reachable via byte commands); byte-split mode advances
                    // by the rendered prefix.
                    point_width = width
                        + if renders_byte_split(ch, options.byte_oriented, options.utf8_locale) {
                            byte_split_prefix_width(ch, self.point - idx, options.output_meta)
                        } else {
                            0
                        };
                }
            }
            if ch == '\n' {
                let display = display_char(
                    ch,
                    options.output_meta,
                    options.byte_oriented,
                    options.utf8_locale,
                    tab_col,
                );
                out.push_str(&display);
                tab_col = 0;
                continue;
            }
            let display = display_char(
                ch,
                options.output_meta,
                options.byte_oriented,
                options.utf8_locale,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_meta_uses_locale_and_preserves_raw_invalid_bytes() {
        let invalid = private_byte_char(0xff);
        let raw_invalid = display_char(invalid, true, false, true, 0);
        assert_eq!(rendered_string_to_bytes(&raw_invalid), vec![0xff]);
        assert_eq!(display_char(invalid, false, false, true, 0), "\\377");
        let raw_byte_oriented = display_char(invalid, true, true, true, 0);
        assert_eq!(rendered_string_to_bytes(&raw_byte_oriented), vec![0xff]);

        assert_eq!(display_char('é', false, false, true, 0), "é");
        assert_eq!(display_char('é', false, false, false, 0), "\\303\\251");
        let raw_c_locale = display_char('é', true, false, false, 0);
        assert_eq!(rendered_string_to_bytes(&raw_c_locale), "é".as_bytes());
        let raw_byte_oriented = display_char('é', true, true, true, 0);
        assert_eq!(rendered_string_to_bytes(&raw_byte_oriented), "é".as_bytes());
        assert_eq!(display_char('é', false, true, true, 0), "\\303\\251");
        assert_eq!(display_char('\x01', false, false, true, 0), "^A");
        assert_eq!(display_char('\x7f', false, false, true, 0), "^?");
    }
}
