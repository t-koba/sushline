use super::{LineBuffer, private_byte_char, private_byte_value};
use std::borrow::Cow;
use unicode_width::UnicodeWidthChar;

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

pub(super) fn rendered_char_width(ch: char, options: RenderOptions<'_>) -> usize {
    display_char(
        ch,
        options.echo_control,
        options.output_meta,
        options.byte_oriented,
    )
    .chars()
    .map(|ch| {
        if ch == '\n' {
            0
        } else {
            UnicodeWidthChar::width(ch).unwrap_or(0)
        }
    })
    .sum()
}

pub(super) fn display_char(
    ch: char,
    echo_control: bool,
    output_meta: bool,
    byte_oriented: bool,
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
    if !echo_control {
        return ch.to_string();
    }
    match ch {
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
    pub fn move_to_display_width(&mut self, target: usize) {
        let mut width = 0;
        for (idx, ch) in self.decoded_char_indices() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if width + ch_width > target {
                self.point = idx;
                return;
            }
            width += ch_width;
        }
        self.point = self.bytes.len();
    }

    pub fn move_screen_line(
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
        positions.push((0, (row, col)));
        for (idx, ch) in self.decoded_char_indices() {
            let display = display_char(
                ch,
                options.echo_control,
                options.output_meta,
                options.byte_oriented,
            );
            for rendered in display.chars() {
                if rendered == '\n' {
                    row += 1;
                    col = 0;
                    continue;
                }
                let width = UnicodeWidthChar::width(rendered).unwrap_or(0);
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
            positions.push((self.next_char_boundary(idx), (row, col)));
        }
        positions
    }

    pub fn display_width_before_point(&self) -> usize {
        self.decoded_chars_in_range(0, self.point)
            .into_iter()
            .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum()
    }

    pub fn display_width(&self) -> usize {
        self.decoded_char_indices()
            .into_iter()
            .map(|(_, ch)| ch)
            .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum()
    }

    pub fn horizontal_window(&self, max_width: usize) -> (String, usize) {
        self.horizontal_window_with_options(max_width, None, RenderOptions::default())
    }

    pub fn horizontal_window_with_options(
        &self,
        max_width: usize,
        mark: Option<usize>,
        options: RenderOptions<'_>,
    ) -> (String, usize) {
        if max_width == 0 {
            return (String::new(), 0);
        }
        let mut start = self.point;
        let mut width = 0;
        while let Some(prev) = self.prev_grapheme_boundary_checked(start) {
            let ch_width = self.rendered_slice_width(prev, start, options.clone());
            if width + ch_width > max_width.saturating_sub(1) {
                break;
            }
            start = prev;
            width += ch_width;
        }
        let mut end = self.point;
        let mut total_width = width;
        while end < self.bytes.len() {
            let next = self.next_grapheme_boundary(end);
            let ch_width = self.rendered_slice_width(end, next, options.clone());
            if total_width + ch_width > max_width {
                break;
            }
            end = next;
            total_width += ch_width;
        }
        let region = self.region(mark, options.active_region);
        let mut visible = String::new();
        for (idx, ch) in self.decoded_char_indices_in_range(start, end) {
            if Some(idx) == region.map(|(region_start, _)| region_start) {
                append_bytes_lossless(&mut visible, options.active_region_start.as_ref());
            }
            if Some(idx) == region.map(|(_, region_end)| region_end) {
                append_bytes_lossless(&mut visible, options.active_region_end.as_ref());
            }
            visible.push_str(&display_char(
                ch,
                options.echo_control,
                options.output_meta,
                options.byte_oriented,
            ));
        }
        if region.is_some_and(|(_, region_end)| region_end == end) {
            append_bytes_lossless(&mut visible, options.active_region_end.as_ref());
        }
        (visible, width)
    }

    pub fn as_string_with_active_region(&self, mark: Option<usize>) -> String {
        let Some((start, end)) = self.region(mark, true) else {
            return self.as_string();
        };
        let mut out = String::new();
        for (idx, ch) in self.decoded_char_indices() {
            if idx == start {
                out.push_str("\x1b[7m");
            }
            if idx == end {
                out.push_str("\x1b[0m");
            }
            out.push(ch);
        }
        if end == self.bytes.len() {
            out.push_str("\x1b[0m");
        }
        out
    }

    pub fn render_text(&self, mark: Option<usize>, options: RenderOptions<'_>) -> (String, usize) {
        let region = self.region(mark, options.active_region);
        let mut out = String::new();
        let mut width = 0;
        let mut point_width = 0;
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
            let display = display_char(
                ch,
                options.echo_control,
                options.output_meta,
                options.byte_oriented,
            );
            width += display
                .chars()
                .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
                .sum::<usize>();
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

    pub fn render_text_bytes(
        &self,
        mark: Option<usize>,
        options: RenderOptions<'_>,
    ) -> (Vec<u8>, usize) {
        let (rendered, point) = self.render_text(mark, options);
        (rendered_string_to_bytes(&rendered), point)
    }

    pub fn rendered_rows_and_point(
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

    fn rendered_slice_width(&self, start: usize, end: usize, options: RenderOptions<'_>) -> usize {
        self.decoded_chars_in_range(start, end)
            .into_iter()
            .map(|ch| rendered_char_width(ch, options.clone()))
            .sum()
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
