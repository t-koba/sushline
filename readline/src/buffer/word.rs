use super::LineBuffer;
pub(super) fn is_word_char(ch: char) -> bool {
    if private_byte_value(ch).is_some() {
        return false;
    }
    ch.is_alphanumeric() || ch == '_'
}

pub(super) fn private_byte_char(byte: u8) -> char {
    char::from_u32(0xe000 + byte as u32).unwrap()
}

pub(super) fn private_byte_value(ch: char) -> Option<u8> {
    let value = ch as u32;
    if (0xe000..=0xe0ff).contains(&value) {
        Some((value - 0xe000) as u8)
    } else {
        None
    }
}

pub(super) fn utf8_sequence_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 1,
    }
}

pub(super) fn is_word_char_with_breaks(ch: char, break_chars: &str) -> bool {
    !ch.is_whitespace() && !break_chars.contains(ch)
}

fn word_matcher(break_chars: Option<&str>) -> impl Fn(char) -> bool + Copy + '_ {
    move |ch| match break_chars {
        Some(break_chars) => is_word_char_with_breaks(ch, break_chars),
        None => is_word_char(ch),
    }
}

pub(super) fn is_horizontal_space(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

pub(super) fn is_filename_separator(ch: char) -> bool {
    ch == '/' || ch.is_whitespace()
}

pub(super) fn is_command_word_separator(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '|' | '&' | ';' | '<' | '>' | '(' | ')')
}

fn is_command_word_separator_byte(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'|' | b'&' | b';' | b'<' | b'>' | b'(' | b')')
}

fn is_break_byte(ch: char, break_chars: &[u8]) -> bool {
    ch.is_ascii() && break_chars.contains(&(ch as u8))
}

pub(super) fn command_word_end(bytes: &[u8], mut idx: usize) -> usize {
    let mut quote = None;
    let mut escaped = false;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if escaped {
            escaped = false;
            idx += 1;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            idx += 1;
            continue;
        }
        if let Some(q) = quote {
            idx += 1;
            if byte == q {
                quote = None;
            }
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
            idx += 1;
            continue;
        }
        if is_command_word_separator_byte(byte) {
            break;
        }
        idx += 1;
    }
    idx
}

pub(super) fn command_word_start(bytes: &[u8], end: usize) -> usize {
    let mut idx = 0;
    let mut last_start = 0;
    while idx < end {
        while idx < end {
            if !is_command_word_separator_byte(bytes[idx]) {
                break;
            }
            idx += 1;
        }
        if idx >= end {
            break;
        }
        last_start = idx;
        let next = command_word_end(bytes, idx);
        if next >= end {
            break;
        }
        idx = next;
    }
    last_start
}

pub(super) fn is_grapheme_extend(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0300..=0x036f
            | 0x0483..=0x0489
            | 0x0591..=0x05bd
            | 0x05bf
            | 0x05c1..=0x05c2
            | 0x05c4..=0x05c5
            | 0x05c7
            | 0x0610..=0x061a
            | 0x064b..=0x065f
            | 0x0670
            | 0x06d6..=0x06dc
            | 0x06df..=0x06e4
            | 0x06e7..=0x06e8
            | 0x06ea..=0x06ed
            | 0x1ab0..=0x1aff
            | 0x1dc0..=0x1dff
            | 0x20d0..=0x20ff
            | 0xfe00..=0xfe0f
            | 0xfe20..=0xfe2f
            | 0xe0100..=0xe01ef
    )
}

pub(super) const DEFAULT_COMPLETION_BREAK_CHARS: &[u8] = b" \t\n'\"`$|&;<>(){}";

impl LineBuffer {
    pub fn copy_backward_word(&mut self, break_chars: Option<&str>) -> Vec<u8> {
        self.copy_backward_word_by(word_matcher(break_chars))
    }

    fn copy_backward_word_by<F>(&mut self, is_word: F) -> Vec<u8>
    where
        F: Fn(char) -> bool + Copy,
    {
        let end = self.point;
        self.backward_word_by(is_word);
        let start = self.point;
        self.point = end;
        self.range_bytes(start, end)
    }

    pub fn copy_forward_word(&mut self, break_chars: Option<&str>) -> Vec<u8> {
        self.copy_forward_word_by(word_matcher(break_chars))
    }

    fn copy_forward_word_by<F>(&mut self, is_word: F) -> Vec<u8>
    where
        F: Fn(char) -> bool + Copy,
    {
        let start = self.point;
        self.forward_word_by(is_word);
        let end = self.point;
        self.point = start;
        self.range_bytes(start, end)
    }

    pub fn kill_to_start(&mut self) -> Vec<u8> {
        let killed = self.bytes[..self.point].to_vec();
        self.bytes.drain(..self.point);
        self.point = 0;
        killed
    }

    pub fn kill_to_end(&mut self) -> Vec<u8> {
        let killed = self.bytes[self.point..].to_vec();
        self.bytes.truncate(self.point);
        killed
    }

    pub fn kill_whole_line(&mut self) -> Vec<u8> {
        let killed = self.bytes.clone();
        self.bytes.clear();
        self.point = 0;
        killed
    }

    pub fn backward_kill_word(&mut self, break_chars: Option<&str>) -> Vec<u8> {
        self.backward_kill_word_by(word_matcher(break_chars))
    }

    pub fn unix_word_rubout(&mut self) -> Vec<u8> {
        let end = self.point;
        while let Some((prev, ch)) = self.prev_char(self.point) {
            if !ch.is_whitespace() {
                break;
            }
            self.point = prev;
        }
        while let Some((prev, ch)) = self.prev_char(self.point) {
            if ch.is_whitespace() {
                break;
            }
            self.point = prev;
        }
        self.delete_range_bytes(self.point, end)
    }

    fn backward_kill_word_by<F>(&mut self, is_word: F) -> Vec<u8>
    where
        F: Fn(char) -> bool + Copy,
    {
        let end = self.point;
        self.backward_word_by(is_word);
        self.delete_range_bytes(self.point, end)
    }

    pub fn backward_kill_filename_word(&mut self) -> Vec<u8> {
        let end = self.point;
        self.backward_filename_word();
        self.delete_range_bytes(self.point, end)
    }

    pub fn kill_word(&mut self, break_chars: Option<&str>) -> Vec<u8> {
        self.kill_word_by(word_matcher(break_chars))
    }

    fn kill_word_by<F>(&mut self, is_word: F) -> Vec<u8>
    where
        F: Fn(char) -> bool + Copy,
    {
        let start = self.point;
        self.forward_word_by(is_word);
        self.delete_range_bytes(start, self.point)
    }

    pub fn delete_horizontal_space(&mut self) {
        let mut start = self.point;
        while let Some((prev, ch)) = self.prev_char(start) {
            if !is_horizontal_space(ch) {
                break;
            }
            start = prev;
        }
        let mut end = self.point;
        while let Some(ch) = self.char_at(end) {
            if !is_horizontal_space(ch) {
                break;
            }
            end = self.next_char_boundary(end);
        }
        let _ = self.delete_range_bytes(start, end);
    }

    pub fn upcase_word(&mut self, break_chars: Option<&str>) -> bool {
        self.map_next_word_by(word_matcher(break_chars), |ch| ch.to_uppercase().collect())
    }

    pub fn upcase_previous_word_preserving_point(&mut self, break_chars: Option<&str>) -> bool {
        self.map_previous_word_preserving_point_by(word_matcher(break_chars), |ch| {
            ch.to_uppercase().collect()
        })
    }

    pub fn downcase_word(&mut self, break_chars: Option<&str>) -> bool {
        self.map_next_word_by(word_matcher(break_chars), |ch| ch.to_lowercase().collect())
    }

    pub fn downcase_previous_word_preserving_point(&mut self, break_chars: Option<&str>) -> bool {
        self.map_previous_word_preserving_point_by(word_matcher(break_chars), |ch| {
            ch.to_lowercase().collect()
        })
    }

    pub fn capitalize_word(&mut self, break_chars: Option<&str>) -> bool {
        self.capitalize_word_by(word_matcher(break_chars))
    }

    pub fn capitalize_previous_word_preserving_point(&mut self, break_chars: Option<&str>) -> bool {
        self.capitalize_previous_word_preserving_point_by(word_matcher(break_chars))
    }

    fn capitalize_word_by<F>(&mut self, is_word: F) -> bool
    where
        F: Fn(char) -> bool + Copy,
    {
        let Some((start, end)) = self.next_word_bounds_by(self.point, is_word) else {
            return false;
        };
        self.replace_mapped_chars(start, end, |idx, ch| {
            if idx == 0 {
                ch.to_uppercase().collect()
            } else {
                ch.to_lowercase().collect()
            }
        });
        self.point = end;
        true
    }

    fn capitalize_previous_word_preserving_point_by<F>(&mut self, is_word: F) -> bool
    where
        F: Fn(char) -> bool + Copy,
    {
        let Some((start, end)) = self.previous_word_bounds_by(self.point, is_word) else {
            return false;
        };
        let point = self.point;
        self.replace_mapped_chars(start, end, |idx, ch| {
            if idx == 0 {
                ch.to_uppercase().collect()
            } else {
                ch.to_lowercase().collect()
            }
        });
        self.point = self.clamp_boundary(point);
        true
    }

    pub fn transpose_words(&mut self, break_chars: Option<&str>) -> bool {
        self.transpose_words_by(word_matcher(break_chars))
    }

    fn transpose_words_by<F>(&mut self, is_word: F) -> bool
    where
        F: Fn(char) -> bool + Copy,
    {
        let Some((left_start, left_end)) = self.previous_word_bounds_by(self.point, is_word) else {
            return false;
        };
        let (left_start, left_end, right_start, right_end) =
            if let Some((right_start, right_end)) = self.next_word_bounds_by(left_end, is_word) {
                (left_start, left_end, right_start, right_end)
            } else {
                let Some((previous_start, previous_end)) =
                    self.previous_word_bounds_by(left_start, is_word)
                else {
                    return false;
                };
                (previous_start, previous_end, left_start, left_end)
            };
        let left = self.bytes[left_start..left_end].to_vec();
        let middle = self.bytes[left_end..right_start].to_vec();
        let right = self.bytes[right_start..right_end].to_vec();
        let mut replacement = Vec::with_capacity(right.len() + middle.len() + left.len());
        replacement.extend_from_slice(&right);
        replacement.extend_from_slice(&middle);
        replacement.extend_from_slice(&left);
        let point = left_start + replacement.len();
        self.bytes.splice(left_start..right_end, replacement);
        self.point = point;
        true
    }

    pub fn transpose_command_words(&mut self) -> bool {
        let Some((left_start, left_end)) = self.previous_bigword_bounds(self.point) else {
            return false;
        };
        let (left_start, left_end, right_start, right_end) =
            if let Some((right_start, right_end)) = self.next_bigword_bounds_from(left_end) {
                (left_start, left_end, right_start, right_end)
            } else {
                let Some((previous_start, previous_end)) = self.previous_bigword_bounds(left_start)
                else {
                    return false;
                };
                (previous_start, previous_end, left_start, left_end)
            };
        let left = self.bytes[left_start..left_end].to_vec();
        let middle = self.bytes[left_end..right_start].to_vec();
        let right = self.bytes[right_start..right_end].to_vec();
        let mut replacement = Vec::with_capacity(right.len() + middle.len() + left.len());
        replacement.extend_from_slice(&right);
        replacement.extend_from_slice(&middle);
        replacement.extend_from_slice(&left);
        let point = left_start + replacement.len();
        self.bytes.splice(left_start..right_end, replacement);
        self.point = point;
        true
    }

    pub fn toggle_case_at_point(&mut self) -> bool {
        let Some(ch) = self.char_at_point() else {
            return false;
        };
        let replacement: String = if ch.is_lowercase() {
            ch.to_uppercase().collect()
        } else {
            ch.to_lowercase().collect()
        };
        let Some(end) = self.next_char_boundary_checked(self.point) else {
            return false;
        };
        self.bytes
            .splice(self.point..end, replacement.as_bytes().iter().copied());
        self.move_forward();
        true
    }

    pub fn forward_word(&mut self, break_chars: Option<&str>) -> bool {
        self.forward_word_by(word_matcher(break_chars))
    }

    fn forward_word_by<F>(&mut self, is_word: F) -> bool
    where
        F: Fn(char) -> bool + Copy,
    {
        let start = self.point;
        while let Some(ch) = self.char_at(self.point) {
            if is_word(ch) {
                break;
            }
            self.point = self.next_char_boundary(self.point);
        }
        while let Some(ch) = self.char_at(self.point) {
            if !is_word(ch) {
                break;
            }
            self.point = self.next_char_boundary(self.point);
        }
        self.point != start
    }

    pub fn forward_bigword(&mut self) -> bool {
        let start = self.point;
        while let Some(ch) = self.char_at(self.point) {
            if !ch.is_whitespace() {
                break;
            }
            self.point = self.next_char_boundary(self.point);
        }
        while let Some(ch) = self.char_at(self.point) {
            if ch.is_whitespace() {
                break;
            }
            self.point = self.next_char_boundary(self.point);
        }
        self.point != start
    }

    pub fn backward_bigword(&mut self) -> bool {
        let start = self.point;
        while let Some((prev, ch)) = self.prev_char(self.point) {
            if !ch.is_whitespace() {
                break;
            }
            self.point = prev;
        }
        while let Some((prev, ch)) = self.prev_char(self.point) {
            if ch.is_whitespace() {
                break;
            }
            self.point = prev;
        }
        self.point != start
    }

    pub fn end_word(&mut self, break_chars: Option<&str>) -> bool {
        self.end_word_by(word_matcher(break_chars))
    }

    fn end_word_by<F>(&mut self, is_word: F) -> bool
    where
        F: Fn(char) -> bool + Copy,
    {
        let start = self.point;
        if self.char_at(self.point).is_some_and(is_word) {
            self.point = self.next_char_boundary(self.point);
        }
        while let Some(ch) = self.char_at(self.point) {
            if is_word(ch) {
                break;
            }
            self.point = self.next_char_boundary(self.point);
        }
        while let Some((next, ch)) = self.next_char(self.point) {
            if !is_word(ch) {
                break;
            }
            self.point = next;
        }
        self.point != start
    }

    pub fn end_bigword(&mut self) -> bool {
        let start = self.point;
        if self
            .char_at(self.point)
            .is_some_and(|ch| !ch.is_whitespace())
        {
            self.point = self.next_char_boundary(self.point);
        }
        while let Some(ch) = self.char_at(self.point) {
            if !ch.is_whitespace() {
                break;
            }
            self.point = self.next_char_boundary(self.point);
        }
        while let Some((next, ch)) = self.next_char(self.point) {
            if ch.is_whitespace() {
                break;
            }
            self.point = next;
        }
        self.point != start
    }

    pub fn backward_filename_word(&mut self) -> bool {
        let start = self.point;
        while let Some((prev, ch)) = self.prev_char(self.point) {
            if !is_filename_separator(ch) {
                break;
            }
            self.point = prev;
        }
        while let Some((prev, ch)) = self.prev_char(self.point) {
            if is_filename_separator(ch) {
                break;
            }
            self.point = prev;
        }
        self.point != start
    }

    pub fn forward_command_word(&mut self) -> bool {
        let start = self.point;
        let mut idx = self.point.min(self.bytes.len());
        while idx < self.bytes.len() {
            if !is_command_word_separator_byte(self.bytes[idx]) {
                break;
            }
            idx += 1;
        }
        if idx < self.bytes.len() {
            idx = command_word_end(&self.bytes, idx).min(self.bytes.len());
        }
        self.point = idx;
        self.point != start
    }

    pub fn backward_command_word(&mut self) -> bool {
        let start = self.point;
        let mut idx = self.point.min(self.bytes.len());
        while let Some((prev, ch)) = self.prev_char(idx) {
            if !is_command_word_separator(ch) {
                break;
            }
            idx = prev;
        }
        if idx > 0 {
            idx = command_word_start(&self.bytes, idx).min(self.bytes.len());
        }
        self.point = idx;
        self.point != start
    }

    pub fn kill_command_word(&mut self) -> Vec<u8> {
        let start = self.point;
        self.forward_command_word();
        self.delete_range_bytes(start, self.point)
    }

    pub fn backward_kill_command_word(&mut self) -> Vec<u8> {
        let end = self.point;
        self.backward_command_word();
        self.delete_range_bytes(self.point, end)
    }

    pub fn move_to_first_nonblank(&mut self) {
        self.point = self
            .decoded_char_indices()
            .into_iter()
            .find_map(|(idx, ch)| (!ch.is_whitespace()).then_some(idx))
            .unwrap_or(self.bytes.len());
    }

    pub fn find_forward(&self, target: char, from_next: bool) -> Option<usize> {
        let start = if from_next {
            self.next_char_boundary(self.point)
        } else {
            self.point
        };
        self.find_forward_from(target, start)
    }

    pub fn find_forward_from(&self, target: char, start: usize) -> Option<usize> {
        let mut idx = self.clamp_boundary(start);
        while let Some((next, ch)) = self.decoded_char_at(idx) {
            if ch == target {
                return Some(idx);
            }
            idx = next;
        }
        None
    }

    pub fn find_backward(&self, target: char, from_previous: bool) -> Option<usize> {
        let end = if from_previous {
            self.prev_char_boundary_checked(self.point).unwrap_or(0)
        } else {
            self.point
        };
        self.find_backward_from(target, end)
    }

    pub fn find_backward_from(&self, target: char, end: usize) -> Option<usize> {
        let end = self.next_char_boundary(self.clamp_boundary(end));
        let mut found = None;
        for (idx, ch) in self.decoded_char_indices() {
            if idx > end {
                break;
            }
            if ch == target {
                found = Some(idx);
            }
        }
        found
    }

    pub fn word_before_point(&self, break_chars: Option<&[u8]>) -> Vec<u8> {
        let mut start = self.point;
        while let Some((prev, ch)) = self.prev_char(start) {
            let is_boundary = match break_chars {
                Some(break_chars) => is_break_byte(ch, break_chars),
                None => !is_word_char(ch),
            };
            if is_boundary {
                break;
            }
            start = prev;
        }
        self.range_bytes(start, self.point)
    }

    pub fn completion_word_bounds(&self, break_chars: Option<&[u8]>) -> (usize, usize) {
        let break_chars = break_chars.unwrap_or(DEFAULT_COMPLETION_BREAK_CHARS);
        let mut start = 0;
        let mut quote = None;
        let mut escaped = false;
        for (idx, ch) in self.decoded_char_indices() {
            if idx >= self.point {
                break;
            }
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' && quote != Some('\'') {
                escaped = true;
                continue;
            }
            if let Some(active_quote) = quote {
                if ch == active_quote {
                    quote = None;
                }
                continue;
            }
            if matches!(ch, '\'' | '"') {
                quote = Some(ch);
                continue;
            }
            if is_break_byte(ch, break_chars) {
                start = idx + ch.len_utf8();
            }
        }
        (start, self.point)
    }

    pub fn backward_word(&mut self, break_chars: Option<&str>) -> bool {
        self.backward_word_by(word_matcher(break_chars))
    }

    fn backward_word_by<F>(&mut self, is_word: F) -> bool
    where
        F: Fn(char) -> bool + Copy,
    {
        let start = self.point;
        while let Some((prev, ch)) = self.prev_char(self.point) {
            if is_word(ch) {
                break;
            }
            self.point = prev;
        }
        while let Some((prev, ch)) = self.prev_char(self.point) {
            if !is_word(ch) {
                break;
            }
            self.point = prev;
        }
        self.point != start
    }

    fn map_next_word_by<P, F>(&mut self, is_word: P, mut f: F) -> bool
    where
        P: Fn(char) -> bool + Copy,
        F: FnMut(char) -> String,
    {
        let Some((start, end)) = self.next_word_bounds_by(self.point, is_word) else {
            return false;
        };
        self.replace_mapped_chars(start, end, |_, ch| f(ch));
        self.point = end;
        true
    }

    fn map_previous_word_preserving_point_by<P, F>(&mut self, is_word: P, mut f: F) -> bool
    where
        P: Fn(char) -> bool + Copy,
        F: FnMut(char) -> String,
    {
        let Some((start, end)) = self.previous_word_bounds_by(self.point, is_word) else {
            return false;
        };
        let point = self.point;
        self.replace_mapped_chars(start, end, |_, ch| f(ch));
        self.point = self.clamp_boundary(point);
        true
    }

    fn next_word_bounds_by<F>(&self, from: usize, is_word: F) -> Option<(usize, usize)>
    where
        F: Fn(char) -> bool + Copy,
    {
        let mut start = self.clamp_boundary(from);
        while let Some(ch) = self.char_at(start) {
            if is_word(ch) {
                break;
            }
            start = self.next_char_boundary(start);
        }
        if start == self.bytes.len() {
            return None;
        }
        let mut end = start;
        while let Some(ch) = self.char_at(end) {
            if !is_word(ch) {
                break;
            }
            end = self.next_char_boundary(end);
        }
        Some((start, end))
    }

    fn next_bigword_bounds_from(&self, from: usize) -> Option<(usize, usize)> {
        let mut start = self.clamp_boundary(from);
        while let Some(ch) = self.char_at(start) {
            if !ch.is_whitespace() {
                break;
            }
            start = self.next_char_boundary(start);
        }
        if start == self.bytes.len() {
            return None;
        }
        let mut end = start;
        while let Some(ch) = self.char_at(end) {
            if ch.is_whitespace() {
                break;
            }
            end = self.next_char_boundary(end);
        }
        Some((start, end))
    }

    fn previous_word_bounds_by<F>(&self, before: usize, is_word: F) -> Option<(usize, usize)>
    where
        F: Fn(char) -> bool + Copy,
    {
        let mut end = self.clamp_boundary(before);
        while let Some((prev, ch)) = self.prev_char(end) {
            if is_word(ch) {
                break;
            }
            end = prev;
        }
        if end == 0 {
            return None;
        }
        let mut start = end;
        while let Some((prev, ch)) = self.prev_char(start) {
            if !is_word(ch) {
                break;
            }
            start = prev;
        }
        Some((start, end))
    }

    fn previous_bigword_bounds(&self, before: usize) -> Option<(usize, usize)> {
        let mut end = self.clamp_boundary(before);
        while let Some((prev, ch)) = self.prev_char(end) {
            if !ch.is_whitespace() {
                break;
            }
            end = prev;
        }
        if end == 0 {
            return None;
        }
        let mut start = end;
        while let Some((prev, ch)) = self.prev_char(start) {
            if ch.is_whitespace() {
                break;
            }
            start = prev;
        }
        Some((start, end))
    }
}
