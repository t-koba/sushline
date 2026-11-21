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

fn capitalize_word_char(index: usize, ch: char) -> String {
    if index == 0 {
        ch.to_uppercase().collect()
    } else {
        ch.to_lowercase().collect()
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
        if byte == b'$'
            && matches!(bytes.get(idx + 1), Some(b'(' | b'{' | b'['))
            && let Some(end) = shell_command_group_end(bytes, idx)
        {
            idx = end;
            continue;
        }
        if matches!(byte, b'<' | b'>')
            && bytes.get(idx + 1) == Some(&b'(')
            && let Some(end) = paired_command_group_end(bytes, idx + 2, b'(', b')')
        {
            idx = end;
            continue;
        }
        if byte == b'`' {
            idx = backtick_command_group_end(bytes, idx);
            continue;
        }
        if is_command_word_separator_byte(byte) {
            break;
        }
        idx += 1;
    }
    idx
}

fn shell_command_group_end(bytes: &[u8], idx: usize) -> Option<usize> {
    let (open, close) = match bytes.get(idx + 1).copied()? {
        b'(' => (b'(', b')'),
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        _ => return None,
    };
    paired_command_group_end(bytes, idx + 2, open, close)
}

fn paired_command_group_end(bytes: &[u8], mut cursor: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 1usize;
    let mut quote = None;
    let mut escape = false;
    while let Some(byte) = bytes.get(cursor).copied() {
        if escape {
            escape = false;
            cursor += 1;
            continue;
        }
        if byte == b'\\' && quote != Some(b'\'') {
            escape = true;
            cursor += 1;
            continue;
        }
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
            cursor += 1;
            continue;
        }
        if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return Some(cursor + 1);
            }
        }
        cursor += 1;
    }
    Some(bytes.len())
}

fn backtick_command_group_end(bytes: &[u8], idx: usize) -> usize {
    let mut escape = false;
    let mut cursor = idx + 1;
    while let Some(byte) = bytes.get(cursor).copied() {
        if escape {
            escape = false;
            cursor += 1;
            continue;
        }
        if byte == b'\\' {
            escape = true;
            cursor += 1;
            continue;
        }
        cursor += 1;
        if byte == b'`' {
            break;
        }
    }
    cursor
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
    /// Copy backward word.
    pub fn copy_backward_word(&mut self, break_chars: Option<&str>) -> Vec<u8> {
        self.copy_backward_word_by(word_matcher(break_chars))
    }

    pub(crate) fn copy_backward_words_by_count(
        &self,
        count: u32,
        break_chars: Option<&str>,
    ) -> Vec<u8> {
        if count == 0 {
            return Vec::new();
        }

        let is_word = word_matcher(break_chars);
        let point = self.point;
        let mut end = point;
        if self.char_at(point).is_some_and(is_word) {
            while let Some(ch) = self.char_at(end) {
                if !is_word(ch) {
                    break;
                }
                end = self.next_char_boundary(end);
            }
        }

        let mut cursor = self.clone();
        for _ in 0..count {
            if !cursor.backward_word_by(is_word) {
                break;
            }
        }
        self.range_bytes(cursor.point, end)
    }

    fn copy_backward_word_by<F>(&mut self, is_word: F) -> Vec<u8>
    where
        F: Fn(char) -> bool + Copy,
    {
        let point = self.point;
        let mut end = point;
        if self.char_at(point).is_some_and(is_word) {
            while let Some(ch) = self.char_at(end) {
                if !is_word(ch) {
                    break;
                }
                end = self.next_char_boundary(end);
            }
        }
        self.backward_word_by(is_word);
        let start = self.point;
        self.point = point;
        self.range_bytes(start, end)
    }

    /// Copy forward word.
    pub fn copy_forward_word(&mut self, break_chars: Option<&str>) -> Vec<u8> {
        self.copy_forward_word_by(word_matcher(break_chars))
    }

    pub(crate) fn copy_forward_words_by_count(
        &self,
        count: u32,
        break_chars: Option<&str>,
    ) -> Vec<u8> {
        if count == 0 {
            return Vec::new();
        }

        let is_word = word_matcher(break_chars);
        let point = self.point;
        let mut start = point;
        if self.char_at(point).is_some_and(is_word) {
            while let Some((prev, ch)) = self.prev_char(start) {
                if !is_word(ch) {
                    break;
                }
                start = prev;
            }
        } else {
            while self.char_at(start).is_some_and(char::is_whitespace) {
                start = self.next_char_boundary(start);
            }
        }

        let mut cursor = point;
        let mut end = point;
        let mut copied_any = false;
        for _ in 0..count {
            let Some((_, word_end)) = self.next_word_bounds_by(cursor, is_word) else {
                break;
            };
            cursor = word_end;
            end = word_end;
            copied_any = true;
        }
        if copied_any {
            self.range_bytes(start, end)
        } else {
            Vec::new()
        }
    }

    fn copy_forward_word_by<F>(&mut self, is_word: F) -> Vec<u8>
    where
        F: Fn(char) -> bool + Copy,
    {
        let point = self.point;
        let mut start = point;
        if self.char_at(point).is_some_and(is_word) {
            while let Some((prev, ch)) = self.prev_char(start) {
                if !is_word(ch) {
                    break;
                }
                start = prev;
            }
        } else {
            while self.char_at(start).is_some_and(char::is_whitespace) {
                start = self.next_char_boundary(start);
            }
        }
        self.forward_word_by(is_word);
        let end = self.point;
        self.point = point;
        self.range_bytes(start, end)
    }

    /// Kill to start.
    pub fn kill_to_start(&mut self) -> Vec<u8> {
        let killed = self.bytes[..self.point].to_vec();
        self.bytes.drain(..self.point);
        self.point = 0;
        killed
    }

    /// Kill to end.
    pub fn kill_to_end(&mut self) -> Vec<u8> {
        let killed = self.bytes[self.point..].to_vec();
        self.bytes.truncate(self.point);
        killed
    }

    /// Kill whole line.
    pub fn kill_whole_line(&mut self) -> Vec<u8> {
        let killed = self.bytes.clone();
        self.bytes.clear();
        self.point = 0;
        killed
    }

    /// Backward kill word.
    pub fn backward_kill_word(&mut self, break_chars: Option<&str>) -> Vec<u8> {
        self.backward_kill_word_by(word_matcher(break_chars))
    }

    /// Unix word rubout.
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

    /// Backward kill filename word.
    pub fn backward_kill_filename_word(&mut self) -> Vec<u8> {
        let end = self.point;
        self.backward_filename_word();
        self.delete_range_bytes(self.point, end)
    }

    /// Kill word.
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

    /// Delete horizontal space.
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

    /// Upcase word.
    pub fn upcase_word(&mut self, break_chars: Option<&str>) -> bool {
        self.map_next_word_by(word_matcher(break_chars), |_, ch| {
            ch.to_uppercase().collect()
        })
    }

    /// Upcase previous word preserving point.
    pub fn upcase_previous_word_preserving_point(&mut self, break_chars: Option<&str>) -> bool {
        self.upcase_previous_words(1, break_chars)
    }

    pub(crate) fn upcase_previous_words(&mut self, count: u32, break_chars: Option<&str>) -> bool {
        self.map_previous_words_by(count, word_matcher(break_chars), |_, ch| {
            ch.to_uppercase().collect()
        })
    }

    /// Downcase word.
    pub fn downcase_word(&mut self, break_chars: Option<&str>) -> bool {
        self.map_next_word_by(word_matcher(break_chars), |_, ch| {
            ch.to_lowercase().collect()
        })
    }

    /// Downcase previous word preserving point.
    pub fn downcase_previous_word_preserving_point(&mut self, break_chars: Option<&str>) -> bool {
        self.downcase_previous_words(1, break_chars)
    }

    pub(crate) fn downcase_previous_words(
        &mut self,
        count: u32,
        break_chars: Option<&str>,
    ) -> bool {
        self.map_previous_words_by(count, word_matcher(break_chars), |_, ch| {
            ch.to_lowercase().collect()
        })
    }

    /// Capitalize word.
    pub fn capitalize_word(&mut self, break_chars: Option<&str>) -> bool {
        self.map_next_word_by(word_matcher(break_chars), capitalize_word_char)
    }

    /// Capitalize previous word preserving point.
    pub fn capitalize_previous_word_preserving_point(&mut self, break_chars: Option<&str>) -> bool {
        self.capitalize_previous_words(1, break_chars)
    }

    pub(crate) fn capitalize_previous_words(
        &mut self,
        count: u32,
        break_chars: Option<&str>,
    ) -> bool {
        self.map_previous_words_by(count, word_matcher(break_chars), capitalize_word_char)
    }

    /// Transpose words.
    pub fn transpose_words(&mut self, break_chars: Option<&str>) -> bool {
        self.transpose_words_by(word_matcher(break_chars))
    }

    pub(crate) fn transpose_words_by_count(
        &mut self,
        count: i32,
        break_chars: Option<&str>,
    ) -> bool {
        if count == 0 {
            return false;
        }
        let is_word = word_matcher(break_chars);
        if count == 1 {
            return self.transpose_words_by(is_word);
        }

        // Resolve both endpoints on a copy so an invalid selection leaves
        // the original text and point unchanged.
        let mut cursor = self.clone();
        cursor.move_words_by_count(count, true, is_word);
        let right_end = cursor.point;
        cursor.move_words_by_count(1, false, is_word);
        let right_start = cursor.point;
        cursor.move_words_by_count(count, false, is_word);
        let left_start = cursor.point;
        cursor.move_words_by_count(1, true, is_word);
        let left_end = cursor.point;

        if left_start == right_start || right_start < left_end {
            return false;
        }

        let left = self.bytes[left_start..left_end].to_vec();
        let middle = self.bytes[left_end..right_start].to_vec();
        let right = self.bytes[right_start..right_end].to_vec();
        let mut replacement = Vec::with_capacity(right.len() + middle.len() + left.len());
        replacement.extend_from_slice(&right);
        replacement.extend_from_slice(&middle);
        replacement.extend_from_slice(&left);
        self.bytes.splice(left_start..right_end, replacement);
        self.point = right_end;
        true
    }

    fn move_words_by_count<F>(&mut self, count: i32, forward: bool, is_word: F)
    where
        F: Fn(char) -> bool + Copy,
    {
        // A negative count reverses the requested movement direction.
        let move_forward = forward != count.is_negative();
        for _ in 0..count.unsigned_abs() {
            let moved = if move_forward {
                self.forward_word_by(is_word)
            } else {
                self.backward_word_by(is_word)
            };
            if !moved {
                break;
            }
        }
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

    pub(crate) fn command_word_spans(&self) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut from = 0;
        while let Some((start, end)) = self.next_command_word_bounds_from(from) {
            if end <= from {
                break;
            }
            spans.push((start, end));
            from = end;
        }
        spans
    }

    pub(crate) fn transpose_word_ranges(
        &mut self,
        left_start: usize,
        left_end: usize,
        right_start: usize,
        right_end: usize,
    ) -> bool {
        if left_start >= left_end
            || left_end > right_start
            || right_start >= right_end
            || right_end > self.bytes.len()
        {
            return false;
        }
        let left = self.bytes[left_start..left_end].to_vec();
        let middle = self.bytes[left_end..right_start].to_vec();
        let right = self.bytes[right_start..right_end].to_vec();
        let mut replacement = Vec::with_capacity(right.len() + middle.len() + left.len());
        replacement.extend_from_slice(&right);
        replacement.extend_from_slice(&middle);
        replacement.extend_from_slice(&left);
        self.bytes.splice(left_start..right_end, replacement);
        self.point = right_end;
        true
    }

    /// Transpose command words.
    pub fn transpose_command_words(&mut self) -> bool {
        let Some((left_start, left_end)) = self.previous_command_word_bounds(self.point) else {
            return false;
        };
        let (left_start, left_end, right_start, right_end) =
            if let Some((right_start, right_end)) = self.next_command_word_bounds_from(left_end) {
                (left_start, left_end, right_start, right_end)
            } else {
                let Some((previous_start, previous_end)) =
                    self.previous_command_word_bounds(left_start)
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

    /// Toggle case at point, returning whether GNU would ring for it.
    ///
    /// Byte-oriented mode sees raw bytes: only ASCII letters change. An
    /// uncased byte is skipped silently when another byte follows; at the end
    /// of the line GNU rings and leaves point there. In character mode GNU
    /// rings for an uncased character, while a cased character with no
    /// single-character mapping (such as `ß`) is left alone without ringing.
    pub fn toggle_case_at_point(&mut self, bytewise: bool) -> bool {
        if bytewise {
            let Some(byte) = self.as_bytes().get(self.point).copied() else {
                return false;
            };
            if !byte.is_ascii_alphabetic() {
                if self.point + 1 < self.bytes.len() {
                    self.point += 1;
                    return false;
                }
                return true;
            }
            self.bytes[self.point] = if byte.is_ascii_lowercase() {
                byte.to_ascii_uppercase()
            } else {
                byte.to_ascii_lowercase()
            };
            if self.point + 1 < self.bytes.len() {
                self.point += 1;
            }
            return false;
        }
        let Some(ch) = self.char_at_point() else {
            return false;
        };
        let upper: String = ch.to_uppercase().collect();
        let lower: String = ch.to_lowercase().collect();
        if upper == ch.to_string() && lower == ch.to_string() {
            return true;
        }
        let replacement = if lower != ch.to_string() {
            lower
        } else {
            upper
        };
        if replacement.chars().count() != 1 {
            return false;
        }
        let Some(end) = self.next_char_boundary_checked(self.point) else {
            return false;
        };
        let replacement_len = replacement.len();
        self.bytes
            .splice(self.point..end, replacement.as_bytes().iter().copied());
        // GNU keeps point on the final character. A count then toggles that
        // same character again instead of leaving it after one change.
        if self.point + replacement_len < self.bytes.len() {
            self.move_forward();
        }
        false
    }

    /// Forward word.
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

    /// Forward bigword.
    pub fn forward_bigword(&mut self) -> bool {
        let start = self.point;
        while let Some(ch) = self.char_at(self.point) {
            if ch.is_whitespace() {
                break;
            }
            self.point = self.next_char_boundary(self.point);
        }
        while let Some(ch) = self.char_at(self.point) {
            if !ch.is_whitespace() {
                break;
            }
            self.point = self.next_char_boundary(self.point);
        }
        self.point != start
    }

    /// Forward bigword end.
    pub fn forward_bigword_end(&mut self) -> bool {
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

    /// Vi forward word.
    pub fn vi_forward_word(&mut self, break_chars: Option<&str>) -> bool {
        self.vi_word_motion_by(word_matcher(break_chars), true)
    }

    /// Vi change-word motion (lowercase `cw`, with a numeric argument).
    ///
    /// GNU Readline runs `rl_vi_fword` for the motion, then backs the end
    /// point up to the end of the current run. The cursor itself stays at
    /// the start of the change, so `cw` never crosses the whitespace after
    /// a word on its first repetition.
    pub fn vi_change_words(&mut self, count: u32, break_chars: Option<&str>) -> bool {
        let is_word = word_matcher(break_chars);
        let class = |ch: char| {
            if ch.is_whitespace() {
                0u8
            } else if is_word(ch) {
                1
            } else {
                2
            }
        };
        let class_at = |idx: usize| self.vi_word_base_char(idx).map(class);
        let start = self.point;
        let end = self.len_chars();
        if start >= end {
            return false;
        }
        let mut point = start;

        // GNU appends a blank for the motion so the last character can be
        // reached; our boundary helpers already stop at the buffer end.
        for _ in 0..count {
            if point >= end {
                break;
            }
            let current = class_at(point).unwrap();
            if current != 0 {
                while class_at(point) == Some(current) {
                    point = self.next_char_boundary(point);
                }
            }
            while class_at(point) == Some(0) {
                point = self.next_char_boundary(point);
            }
        }

        // Back up from the first character of the next word, then reduce the
        // range to the current run. Starting in whitespace or at a run end
        // leaves a single character to change; starting earlier in a run
        // advances over its last character so `cw` behaves like `ce`.
        if point < end && class_at(point).is_some_and(|current| current != 0) {
            point = self.prev_char(point).map(|(prev, _)| prev).unwrap_or(point);
        }
        while point > start && class_at(point) == Some(0) {
            point = self.prev_char(point).map(|(prev, _)| prev).unwrap_or(point);
        }
        let advance = point == start
            || (point < end.saturating_sub(1)
                && class_at(point).is_some_and(|current| current != 0));
        if advance {
            point = self.next_char_boundary(point);
        }
        self.point = point;
        point != start
    }

    /// Byte-oriented `cw` classifies each raw byte: high bytes are
    /// punctuation, so a word starting before a multibyte scalar stops at its
    /// first byte and a point inside one changes the rest of that run.
    pub fn vi_change_words_bytewise(&mut self, count: u32, break_chars: Option<&str>) -> bool {
        let class = |byte| crate::buffer::vi_byte_class(byte, break_chars, false);
        let start = self.point;
        let end = self.bytes.len();
        if start >= end {
            return false;
        }
        let mut point = start;
        for _ in 0..count {
            if point >= end {
                break;
            }
            let current = class(self.bytes[point]);
            if current != 0 {
                while point < end && class(self.bytes[point]) == current {
                    point += 1;
                }
            }
            while point < end && class(self.bytes[point]) == 0 {
                point += 1;
            }
        }
        if point < end && class(self.bytes[point]) != 0 {
            point -= 1;
        }
        while point > start && point < end && class(self.bytes[point]) == 0 {
            point -= 1;
        }
        let advance = point == start
            || (point < end && point < end.saturating_sub(1) && class(self.bytes[point]) != 0);
        if advance && point < end {
            point += 1;
        }
        self.point = point;
        point != start
    }

    /// Base character governing a combining mark's vi word class. GNU treats
    /// a mark as part of the preceding grapheme, not as a separate scalar.
    fn vi_word_base_char(&self, idx: usize) -> Option<char> {
        let (_, mut ch) = self.decoded_char_at(idx)?;
        let mut at = idx;
        while is_grapheme_extend(ch) || ch == '\u{200d}' {
            (at, ch) = self.decoded_prev_char(at)?;
        }
        Some(ch)
    }

    fn vi_word_motion_by<F>(&mut self, is_word: F, skip_whitespace: bool) -> bool
    where
        F: Fn(char) -> bool + Copy,
    {
        let start = self.point;
        match self.vi_word_base_char(self.point) {
            Some(ch) if ch.is_whitespace() => {
                while let Some(ch) = self.vi_word_base_char(self.point) {
                    if !ch.is_whitespace() {
                        break;
                    }
                    self.point = self.next_char_boundary(self.point);
                }
            }
            Some(ch) if is_word(ch) => {
                while let Some(ch) = self.vi_word_base_char(self.point) {
                    if !is_word(ch) {
                        break;
                    }
                    self.point = self.next_char_boundary(self.point);
                }
            }
            Some(_) => {
                while let Some(ch) = self.vi_word_base_char(self.point) {
                    if ch.is_whitespace() || is_word(ch) {
                        break;
                    }
                    self.point = self.next_char_boundary(self.point);
                }
            }
            None => return false,
        }
        if skip_whitespace {
            while let Some(ch) = self.vi_word_base_char(self.point) {
                if !ch.is_whitespace() {
                    break;
                }
                self.point = self.next_char_boundary(self.point);
            }
        }
        self.point != start
    }

    /// Vi backward word (lowercase `b`).
    ///
    /// Mirrors GNU Readline's `rl_vi_bword`: a word starts at an identifier
    /// run or a non-identifier, non-whitespace run, and the boundary between
    /// the two classes counts as its own stopping point. A contiguous
    /// punctuation run is therefore handled without treating every single
    /// character as a separate word.
    pub fn vi_backward_word(&mut self, break_chars: Option<&str>) -> bool {
        self.vi_backward_word_by(word_matcher(break_chars))
    }

    fn vi_backward_word_by<F>(&mut self, is_word: F) -> bool
    where
        F: Fn(char) -> bool + Copy,
    {
        let start = self.point;
        // GNU Readline steps off end-of-line before scanning backward. A
        // one-character final word is then treated as the starting word
        // rather than skipped, and punctuation/space runs keep their normal
        // boundaries.
        if self.point == self.as_bytes().len() && self.point > 0 {
            self.backup_point();
            if self.point == 0 {
                return true;
            }
        }
        if self.point == 0 {
            return false;
        }

        // At the start of a word backed by whitespace, step into the gap.
        if let Some((prev, ch)) = self.prev_char(self.point)
            && ch.is_whitespace()
            && self.char_at(self.point).is_none_or(|c| !c.is_whitespace())
        {
            self.point = prev;
            if self.point == 0 {
                return true;
            }
        }

        let cur_ident = self.char_at(self.point).is_some_and(is_word);
        let opoint = self.backup_point();
        let prev_ident = self.char_at(self.point).is_some_and(is_word);
        if !((cur_ident && !prev_ident) || (!cur_ident && prev_ident)) {
            self.point = opoint;
        }

        while self.point > 0 && self.char_at(self.point).is_some_and(char::is_whitespace) {
            self.backup_point();
        }

        if self.point > 0 {
            let mut opoint;
            if self.char_at(self.point).is_some_and(is_word) {
                loop {
                    opoint = self.backup_point();
                    if self.point == 0 || !self.char_at(self.point).is_some_and(is_word) {
                        break;
                    }
                }
            } else {
                loop {
                    opoint = self.backup_point();
                    if self.point == 0
                        || self
                            .char_at(self.point)
                            .is_some_and(|ch| ch.is_whitespace() || is_word(ch))
                    {
                        break;
                    }
                }
            }
            if self.point > 0 {
                self.point = opoint;
            }
        }
        self.point != start
    }

    fn backup_point(&mut self) -> usize {
        let old = self.point;
        if let Some((prev, _)) = self.prev_char(self.point) {
            self.point = prev;
        }
        old
    }

    /// Vi end word (lowercase `e`).
    ///
    /// Like [`vi_word_motion_by`](LineBuffer::vi_word_motion_by), a
    /// contiguous punctuation run is its own word class. From inside a run
    /// `e` stops at its end; from the end of a run it advances to the end of
    /// the following run, skipping whitespace on the way.
    pub fn vi_end_word(&mut self, break_chars: Option<&str>) -> bool {
        let is_word = word_matcher(break_chars);
        let class = |ch: char| {
            if ch.is_whitespace() {
                0u8
            } else if is_word(ch) {
                1
            } else {
                2
            }
        };

        let start = self.point;
        if self.char_at(self.point).is_none() {
            return false;
        }
        let current = class(self.char_at(self.point).unwrap());
        if current != 0
            && self
                .next_char(self.point)
                .is_some_and(|(_, ch)| class(ch) == current)
        {
            self.advance_run_in(&class, current);
        } else {
            // At the end of a run, `e` moves to the end of the following
            // run; from whitespace it moves to the end of the next run
            // without the extra step.
            if current != 0
                && let Some((next, _)) = self.next_char(self.point)
            {
                self.point = next;
            }
            self.skip_whitespace_in(&class);
            if let Some(ch) = self.char_at(self.point) {
                let target = class(ch);
                if target != 0 {
                    self.advance_run_in(&class, target);
                }
            }
        }
        self.point != start
    }

    /// Byte-oriented `ce` ends at the last byte of the current raw-byte run.
    /// A high byte is punctuation, so a point on its first byte changes only
    /// the bytes of that scalar rather than the following ASCII word.
    pub fn vi_end_word_bytewise(&mut self, break_chars: Option<&str>) -> bool {
        let class = |byte| crate::buffer::vi_byte_class(byte, break_chars, false);
        let start = self.point;
        let end = self.bytes.len();
        if start >= end {
            return false;
        }
        let current = class(self.bytes[start]);
        if current != 0
            && self
                .bytes
                .get(start + 1)
                .is_some_and(|b| class(*b) == current)
        {
            while self.point + 1 < end && class(self.bytes[self.point + 1]) == current {
                self.point += 1;
            }
        } else {
            if current != 0 && start + 1 < end {
                self.point = start + 1;
            }
            while self.point < end && class(self.bytes[self.point]) == 0 {
                self.point += 1;
            }
            if self.point < end {
                let target = class(self.bytes[self.point]);
                if target != 0 {
                    while self.point + 1 < end && class(self.bytes[self.point + 1]) == target {
                        self.point += 1;
                    }
                }
            }
        }
        self.point != start
    }

    fn skip_whitespace_in<F>(&mut self, class: &F)
    where
        F: Fn(char) -> u8,
    {
        while self.char_at(self.point).is_some_and(|ch| class(ch) == 0) {
            let Some((next, _)) = self.next_char(self.point) else {
                break;
            };
            self.point = next;
        }
    }

    fn advance_run_in<F>(&mut self, class: &F, target: u8)
    where
        F: Fn(char) -> u8,
    {
        while let Some((next, ch)) = self.next_char(self.point) {
            if class(ch) != target {
                break;
            }
            self.point = next;
        }
    }

    /// Backward bigword.
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

    /// End word.
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

    /// End bigword.
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

    /// Backward filename word.
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

    /// Forward command word.
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

    /// Backward command word.
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

    /// Kill command word.
    pub fn kill_command_word(&mut self) -> Vec<u8> {
        let start = self.point;
        self.forward_command_word();
        self.delete_range_bytes(start, self.point)
    }

    /// Backward kill command word.
    pub fn backward_kill_command_word(&mut self) -> Vec<u8> {
        let end = self.point;
        self.backward_command_word();
        self.delete_range_bytes(self.point, end)
    }

    /// Move to first nonblank.
    pub fn move_to_first_nonblank(&mut self) {
        self.point = self
            .decoded_char_indices()
            .into_iter()
            .find_map(|(idx, ch)| (!ch.is_whitespace()).then_some(idx))
            .unwrap_or(self.bytes.len());
    }

    /// Find forward.
    pub fn find_forward(&self, target: char, from_next: bool) -> Option<usize> {
        let start = if from_next {
            self.next_char_boundary(self.point)
        } else {
            self.point
        };
        self.find_forward_from(target, start)
    }

    /// Find forward from.
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

    /// Find the nth raw byte at or after `start`, advancing one byte past
    /// each match. A short count still reports the last match reached.
    pub(crate) fn find_nth_byte_forward_from(
        &self,
        target: u8,
        count: usize,
        start: usize,
    ) -> (Option<usize>, bool) {
        let mut from = start.min(self.bytes.len());
        let mut found = None;
        for _ in 0..count {
            match self.bytes[from..].iter().position(|byte| *byte == target) {
                Some(offset) => {
                    let idx = from + offset;
                    found = Some(idx);
                    from = idx + 1;
                }
                None => return (found, false),
            }
        }
        (found, true)
    }

    /// Find the nth raw byte at or before `end`, stepping one byte before
    /// each match. A short count still reports the last match reached.
    pub(crate) fn find_nth_byte_backward_from(
        &self,
        target: u8,
        count: usize,
        end: usize,
    ) -> (Option<usize>, bool) {
        let mut limit = end.saturating_add(1).min(self.bytes.len());
        let mut found = None;
        for taken in 1..=count {
            match self.bytes[..limit].iter().rposition(|byte| *byte == target) {
                Some(idx) => {
                    found = Some(idx);
                    if idx == 0 {
                        return (found, taken == count);
                    }
                    limit = idx;
                }
                None => return (found, false),
            }
        }
        (found, true)
    }

    /// Find the nth raw byte strictly before point, as vi `F`/`T` require.
    pub(crate) fn find_nth_byte_backward_before(
        &self,
        target: u8,
        count: usize,
    ) -> (Option<usize>, bool) {
        let Some(end) = self.point.checked_sub(1) else {
            return (None, false);
        };
        self.find_nth_byte_backward_from(target, count, end)
    }

    /// Find the nth target at or after `start`, advancing past each match.
    /// A short count still reports the last match reached.
    pub(crate) fn find_nth_forward_from(
        &self,
        target: char,
        count: usize,
        start: usize,
    ) -> (Option<usize>, bool) {
        let mut from = start;
        let mut found = None;
        for _ in 0..count {
            match self.find_forward_from(target, from) {
                Some(idx) => {
                    found = Some(idx);
                    from = self.next_char_boundary(idx);
                }
                None => return (found, false),
            }
        }
        (found, true)
    }

    /// Find the nth target at or before `end`, stepping before each match.
    /// A short count still reports the last match reached.
    pub(crate) fn find_nth_backward_from(
        &self,
        target: char,
        count: usize,
        end: usize,
    ) -> (Option<usize>, bool) {
        let mut limit = self.clamp_boundary(end);
        let mut found = None;
        for taken in 1..=count {
            let candidate = self
                .decoded_char_indices()
                .into_iter()
                .take_while(|(idx, _)| *idx <= limit)
                .filter(|(_, ch)| *ch == target)
                .last();
            match candidate {
                Some((idx, _)) => {
                    found = Some(idx);
                    if idx == 0 {
                        return (found, taken == count);
                    }
                    limit = idx - 1;
                }
                None => return (found, false),
            }
        }
        (found, true)
    }

    /// Find the nth target strictly before point, as vi `F`/`T` require.
    pub(crate) fn find_nth_backward_before(
        &self,
        target: char,
        count: usize,
    ) -> (Option<usize>, bool) {
        let Some(end) = self.prev_char_boundary_checked(self.point) else {
            return (None, false);
        };
        self.find_nth_backward_from(target, count, end)
    }

    /// Find backward.
    pub fn find_backward(&self, target: char, from_previous: bool) -> Option<usize> {
        let end = if from_previous {
            self.prev_char_boundary_checked(self.point).unwrap_or(0)
        } else {
            self.point
        };
        self.find_backward_from(target, end)
    }

    /// Find backward from.
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

    /// Word before point.
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

    /// Completion word bounds.
    pub fn completion_word_bounds(&self, break_chars: Option<&[u8]>) -> (usize, usize) {
        self.completion_word_bounds_at(self.point, break_chars)
    }

    /// Vi completion word bounds.
    pub fn vi_completion_word_bounds(&self, break_chars: Option<&[u8]>) -> (usize, usize) {
        let break_chars = break_chars.unwrap_or(DEFAULT_COMPLETION_BREAK_CHARS);
        let mut point = self.point;
        if self
            .char_at(point)
            .is_some_and(|ch| !is_break_byte(ch, break_chars))
        {
            point = self.next_char_boundary(point);
        }
        self.completion_word_bounds_at(point, Some(break_chars))
    }

    fn completion_word_bounds_at(
        &self,
        point: usize,
        break_chars: Option<&[u8]>,
    ) -> (usize, usize) {
        let break_chars = break_chars.unwrap_or(DEFAULT_COMPLETION_BREAK_CHARS);
        let mut start = 0;
        let mut quote = None;
        let mut escaped = false;
        for (idx, ch) in self.decoded_char_indices() {
            if idx >= point {
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
        (start, point.min(self.bytes.len()))
    }

    /// Backward word.
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

    fn map_next_word_by<P, F>(&mut self, is_word: P, f: F) -> bool
    where
        P: Fn(char) -> bool + Copy,
        F: FnMut(usize, char) -> String,
    {
        let Some((start, end)) = self.next_word_bounds_by(self.point, is_word) else {
            return false;
        };
        self.replace_mapped_chars(start, end, f);
        self.point = end;
        true
    }

    fn map_previous_words_by<P, F>(&mut self, count: u32, is_word: P, mut f: F) -> bool
    where
        P: Fn(char) -> bool + Copy,
        F: FnMut(usize, char) -> String,
    {
        // Walk backwards without moving the editor's point.
        let mut cursor = self.point;
        for _ in 0..count {
            let Some((start, end)) = self.previous_word_bounds_by(cursor, is_word) else {
                break;
            };
            self.replace_mapped_chars(start, end, &mut f);
            cursor = start;
        }
        cursor != self.point
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

    fn next_command_word_bounds_from(&self, from: usize) -> Option<(usize, usize)> {
        let mut start = from.min(self.bytes.len());
        while start < self.bytes.len() && is_command_word_separator_byte(self.bytes[start]) {
            start += 1;
        }
        if start == self.bytes.len() {
            return None;
        }
        Some((start, command_word_end(&self.bytes, start)))
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

    fn previous_command_word_bounds(&self, before: usize) -> Option<(usize, usize)> {
        let mut end = before.min(self.bytes.len());
        while end > 0 && is_command_word_separator_byte(self.bytes[end - 1]) {
            end -= 1;
        }
        if end == 0 {
            return None;
        }
        let start = command_word_start(&self.bytes, end);
        Some((start, end))
    }
}
