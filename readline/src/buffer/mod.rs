mod render;
mod word;
pub use render::RenderOptions;
pub(crate) use render::{append_bytes_lossless, rendered_string_to_bytes};
use word::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordStyle {
    Readline,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineBuffer {
    bytes: Vec<u8>,
    point: usize,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from(text: &str) -> Self {
        Self {
            bytes: text.as_bytes().to_vec(),
            point: text.len(),
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let point = bytes.len();
        Self { bytes, point }
    }

    pub fn as_string(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn point(&self) -> usize {
        self.point
    }

    pub fn byte_index_for_char_index(&self, index: usize) -> usize {
        if index >= self.bytes.len() {
            self.bytes.len()
        } else {
            index
        }
    }

    pub fn byte_point(&self) -> usize {
        self.point
    }

    pub fn len_chars(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn set_point(&mut self, point: usize) {
        self.point = self.clamp_boundary(point);
    }

    pub fn insert_char(&mut self, ch: char) {
        let mut buf = [0; 4];
        self.insert_bytes(ch.encode_utf8(&mut buf).as_bytes());
    }

    pub fn insert_str(&mut self, text: &str) {
        self.insert_bytes(text.as_bytes());
    }

    pub fn insert_bytes(&mut self, bytes: &[u8]) {
        let point = self.point.min(self.bytes.len());
        self.bytes.splice(point..point, bytes.iter().copied());
        self.point = point + bytes.len();
    }

    pub fn replace_char_at_point_bytes(&mut self, bytes: &[u8]) -> bool {
        let Some(end) = self.next_char_boundary_checked(self.point) else {
            return false;
        };
        self.bytes.splice(self.point..end, bytes.iter().copied());
        self.point += bytes.len();
        true
    }

    pub fn replace_range(&mut self, start: usize, end: usize, text: &str) {
        self.replace_range_bytes(start, end, text.as_bytes());
    }

    pub fn replace_range_bytes(&mut self, start: usize, end: usize, bytes: &[u8]) {
        let (start, end) = self.normalized_range(start, end);
        self.bytes.splice(start..end, bytes.iter().copied());
        self.point = start + bytes.len();
    }

    pub fn char_at_point(&self) -> Option<char> {
        self.char_at(self.point)
    }

    pub fn char_before_point(&self) -> Option<char> {
        self.prev_char(self.point).map(|(_, ch)| ch)
    }

    pub fn next_nonblank_from(&self, from: usize) -> usize {
        let mut idx = self.clamp_boundary(from);
        while let Some(ch) = self.char_at(idx) {
            if !ch.is_whitespace() {
                break;
            }
            idx = self.next_char_boundary(idx);
        }
        idx
    }

    pub fn set_char_at_point(&mut self, ch: char) -> bool {
        let Some(end) = self.next_char_boundary_checked(self.point) else {
            return false;
        };
        let mut bytes = [0; 4];
        self.bytes.splice(
            self.point..end,
            ch.encode_utf8(&mut bytes).as_bytes().iter().copied(),
        );
        true
    }

    pub(crate) fn search_char_for_byte(byte: u8) -> char {
        if byte < 0x80 {
            byte as char
        } else {
            private_byte_char(byte)
        }
    }

    pub fn set_char_before_point(&mut self, ch: char) -> bool {
        let Some(start) = self.prev_char_boundary_checked(self.point) else {
            return false;
        };
        let mut bytes = [0; 4];
        self.bytes.splice(
            start..self.point,
            ch.encode_utf8(&mut bytes).as_bytes().iter().copied(),
        );
        self.point = start + ch.len_utf8();
        true
    }

    pub fn insert_comment(&mut self, comment: &str) {
        self.point = 0;
        self.insert_str(comment);
    }

    pub fn toggle_comment(&mut self, comment: &str) {
        self.point = 0;
        if !comment.is_empty() && self.bytes.starts_with(comment.as_bytes()) {
            self.delete_range_bytes(0, comment.len());
        } else {
            self.insert_str(comment);
        }
    }

    pub fn range_bytes(&self, start: usize, end: usize) -> Vec<u8> {
        let (start, end) = self.normalized_range(start, end);
        self.bytes[start..end].to_vec()
    }

    pub fn delete_range_bytes(&mut self, start: usize, end: usize) -> Vec<u8> {
        let (start, end) = self.normalized_range(start, end);
        let deleted = self.bytes[start..end].to_vec();
        self.bytes.drain(start..end);
        self.point = start;
        deleted
    }

    fn decoded_char_at(&self, idx: usize) -> Option<(usize, char)> {
        let idx = idx.min(self.bytes.len());
        let first = *self.bytes.get(idx)?;
        let needed = utf8_sequence_len(first);
        if needed > 1
            && idx + needed <= self.bytes.len()
            && let Ok(text) = std::str::from_utf8(&self.bytes[idx..idx + needed])
            && let Some(ch) = text.chars().next()
        {
            return Some((idx + needed, ch));
        }
        if first < 0x80 {
            Some((idx + 1, first as char))
        } else {
            Some((idx + 1, private_byte_char(first)))
        }
    }

    fn decoded_prev_char(&self, idx: usize) -> Option<(usize, char)> {
        let target = idx.min(self.bytes.len());
        let mut current = 0;
        let mut previous = None;
        while current < target {
            let Some((next, ch)) = self.decoded_char_at(current) else {
                break;
            };
            if next > target {
                break;
            }
            previous = Some((current, ch));
            current = next;
        }
        previous
    }

    fn decoded_char_indices(&self) -> Vec<(usize, char)> {
        self.decoded_char_indices_in_range(0, self.bytes.len())
    }

    fn decoded_char_indices_in_range(&self, start: usize, end: usize) -> Vec<(usize, char)> {
        let mut out = Vec::new();
        let mut idx = self.clamp_boundary(start);
        let end = end.min(self.bytes.len());
        while idx < end {
            let Some((next, ch)) = self.decoded_char_at(idx) else {
                break;
            };
            out.push((idx, ch));
            idx = next;
        }
        out
    }

    fn decoded_chars_in_range(&self, start: usize, end: usize) -> Vec<char> {
        self.decoded_char_indices_in_range(start, end)
            .into_iter()
            .map(|(_, ch)| ch)
            .collect()
    }

    pub(crate) fn display_width_until(&self, end: usize) -> usize {
        self.decoded_char_indices_in_range(0, end.min(self.bytes.len()))
            .into_iter()
            .map(|(_, ch)| unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum()
    }

    pub(crate) fn matching_open_paren_before_point(&self) -> Option<usize> {
        if self.point == 0 || self.char_before_point() != Some(')') {
            return None;
        }
        let mut depth = 0_usize;
        for (idx, ch) in self
            .decoded_char_indices_in_range(0, self.point)
            .into_iter()
            .rev()
        {
            match ch {
                ')' => depth += 1,
                '(' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn find_matching_bracket_forward(
        &self,
        start: usize,
        open: char,
        close: char,
    ) -> Option<usize> {
        let mut depth = 0usize;
        for (idx, ch) in self.decoded_char_indices_in_range(start, self.bytes.len()) {
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
        }
        None
    }

    pub(crate) fn find_matching_bracket_backward(
        &self,
        start: usize,
        open: char,
        close: char,
    ) -> Option<usize> {
        let mut depth = 0usize;
        for (idx, ch) in self
            .decoded_char_indices_in_range(0, start.saturating_add(1))
            .into_iter()
            .rev()
        {
            if ch == close {
                depth += 1;
            } else if ch == open {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
        }
        None
    }

    pub fn delete_char(&mut self) -> bool {
        self.delete_char_bytes().is_some()
    }

    pub fn delete_char_bytes(&mut self) -> Option<Vec<u8>> {
        let end = self.next_grapheme_boundary_checked(self.point)?;
        let deleted = self.bytes[self.point..end].to_vec();
        self.bytes.drain(self.point..end);
        Some(deleted)
    }

    pub fn backward_delete_char(&mut self) -> bool {
        self.backward_delete_char_bytes().is_some()
    }

    pub fn backward_delete_char_bytes(&mut self) -> Option<Vec<u8>> {
        let start = self.prev_grapheme_boundary_checked(self.point)?;
        let deleted = self.bytes[start..self.point].to_vec();
        self.bytes.drain(start..self.point);
        self.point = start;
        Some(deleted)
    }

    pub fn backward_replace_char_with_space(&mut self) -> bool {
        let Some(start) = self.prev_grapheme_boundary_checked(self.point) else {
            return false;
        };
        self.bytes.splice(start..self.point, [b' ']);
        self.point = start;
        true
    }

    pub fn move_beginning(&mut self) {
        self.point = 0;
    }

    pub fn move_end(&mut self) {
        self.point = self.bytes.len();
    }

    pub fn move_forward(&mut self) -> bool {
        let Some(next) = self.next_grapheme_boundary_checked(self.point) else {
            return false;
        };
        self.point = next;
        true
    }

    pub fn move_backward(&mut self) -> bool {
        let Some(prev) = self.prev_grapheme_boundary_checked(self.point) else {
            return false;
        };
        self.point = prev;
        true
    }

    pub fn move_forward_byte(&mut self) -> bool {
        if self.point >= self.bytes.len() {
            return false;
        }
        self.point += 1;
        true
    }

    pub fn move_backward_byte(&mut self) -> bool {
        if self.point == 0 {
            return false;
        }
        self.point -= 1;
        true
    }

    pub fn transpose_chars(&mut self) -> bool {
        let bounds = self.grapheme_boundaries();
        if bounds.len() < 3 || self.point == 0 {
            return false;
        }
        let right_end = if self.point == self.bytes.len() {
            self.point
        } else {
            self.next_grapheme_boundary(self.point)
        };
        let right_start = self.prev_grapheme_boundary_checked(right_end).unwrap_or(0);
        let left_start = self
            .prev_grapheme_boundary_checked(right_start)
            .unwrap_or(0);
        if left_start == right_start {
            return false;
        }
        let left = self.bytes[left_start..right_start].to_vec();
        let right = self.bytes[right_start..right_end].to_vec();
        let mut replacement = Vec::with_capacity(left.len() + right.len());
        replacement.extend_from_slice(&right);
        replacement.extend_from_slice(&left);
        self.bytes.splice(left_start..right_end, replacement);
        self.point = right_end;
        true
    }

    fn normalized_range(&self, start: usize, end: usize) -> (usize, usize) {
        let start = start.min(self.bytes.len());
        let end = end.min(self.bytes.len()).max(start);
        (start, end)
    }

    fn clamp_boundary(&self, idx: usize) -> usize {
        idx.min(self.bytes.len())
    }

    fn char_at(&self, idx: usize) -> Option<char> {
        self.decoded_char_at(idx).map(|(_, ch)| ch)
    }

    fn prev_char(&self, idx: usize) -> Option<(usize, char)> {
        self.decoded_prev_char(idx)
    }

    fn next_char(&self, idx: usize) -> Option<(usize, char)> {
        let idx = self.next_char_boundary(idx);
        self.decoded_char_at(idx).map(|(_, ch)| (idx, ch))
    }

    fn next_char_boundary(&self, idx: usize) -> usize {
        let idx = self.clamp_boundary(idx);
        self.decoded_char_at(idx)
            .map(|(next, _)| next)
            .unwrap_or(self.bytes.len())
    }

    fn next_char_boundary_checked(&self, idx: usize) -> Option<usize> {
        (idx < self.bytes.len()).then(|| self.next_char_boundary(idx))
    }

    fn prev_char_boundary_checked(&self, idx: usize) -> Option<usize> {
        self.prev_char(idx).map(|(idx, _)| idx)
    }

    fn grapheme_boundaries(&self) -> Vec<usize> {
        let mut out = vec![0];
        let mut idx = 0;
        while idx < self.bytes.len() {
            idx = self.next_grapheme_boundary(idx);
            out.push(idx);
        }
        out
    }

    fn next_grapheme_boundary(&self, idx: usize) -> usize {
        let idx = self.clamp_boundary(idx);
        if self.char_at(idx).is_none() {
            return self.bytes.len();
        };
        let mut end = self.next_char_boundary(idx);
        let mut after_zwj = false;
        while let Some(ch) = self.char_at(end) {
            if is_grapheme_extend(ch) || after_zwj {
                after_zwj = ch == '\u{200d}';
                end = self.next_char_boundary(end);
                continue;
            }
            if ch == '\u{200d}' {
                after_zwj = true;
                end = self.next_char_boundary(end);
                continue;
            }
            break;
        }
        end
    }

    fn next_grapheme_boundary_checked(&self, idx: usize) -> Option<usize> {
        (idx < self.bytes.len()).then(|| self.next_grapheme_boundary(idx))
    }

    fn prev_grapheme_boundary_checked(&self, idx: usize) -> Option<usize> {
        let idx = self.clamp_boundary(idx);
        if idx == 0 {
            return None;
        }
        self.grapheme_boundaries()
            .into_iter()
            .take_while(|boundary| *boundary < idx)
            .last()
    }

    fn replace_mapped_chars<F>(&mut self, start: usize, end: usize, mut f: F)
    where
        F: FnMut(usize, char) -> String,
    {
        let mut replacement = String::new();
        for (idx, ch) in self
            .decoded_chars_in_range(start, end)
            .into_iter()
            .enumerate()
        {
            replacement.push_str(&f(idx, ch));
        }
        self.bytes
            .splice(start..end, replacement.as_bytes().iter().copied());
    }
}

#[cfg(test)]
mod tests;
