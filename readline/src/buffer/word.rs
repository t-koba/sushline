use super::LineBuffer;

/// Unicode decimal digits (general category Nd). Rust's `is_numeric` also
/// includes other number categories such as U+00B2 and U+00BC, which GNU's
/// Emacs word class leaves outside a word.
const DECIMAL_DIGIT_RANGES: &[(u32, u32)] = &[
    (0x0030, 0x0039),
    (0x0660, 0x0669),
    (0x06F0, 0x06F9),
    (0x07C0, 0x07C9),
    (0x0966, 0x096F),
    (0x09E6, 0x09EF),
    (0x0A66, 0x0A6F),
    (0x0AE6, 0x0AEF),
    (0x0B66, 0x0B6F),
    (0x0BE6, 0x0BEF),
    (0x0C66, 0x0C6F),
    (0x0CE6, 0x0CEF),
    (0x0D66, 0x0D6F),
    (0x0DE6, 0x0DEF),
    (0x0E50, 0x0E59),
    (0x0ED0, 0x0ED9),
    (0x0F20, 0x0F29),
    (0x1040, 0x1049),
    (0x1090, 0x1099),
    (0x17E0, 0x17E9),
    (0x1810, 0x1819),
    (0x1946, 0x194F),
    (0x19D0, 0x19D9),
    (0x1A80, 0x1A89),
    (0x1A90, 0x1A99),
    (0x1B50, 0x1B59),
    (0x1BB0, 0x1BB9),
    (0x1C40, 0x1C49),
    (0x1C50, 0x1C59),
    (0xA620, 0xA629),
    (0xA8D0, 0xA8D9),
    (0xA900, 0xA909),
    (0xA9D0, 0xA9D9),
    (0xA9F0, 0xA9F9),
    (0xAA50, 0xAA59),
    (0xABF0, 0xABF9),
    (0xFF10, 0xFF19),
    (0x104A0, 0x104A9),
    (0x10D30, 0x10D39),
    (0x11066, 0x1106F),
    (0x110F0, 0x110F9),
    (0x11136, 0x1113F),
    (0x111D0, 0x111D9),
    (0x112F0, 0x112F9),
    (0x11450, 0x11459),
    (0x114D0, 0x114D9),
    (0x11650, 0x11659),
    (0x116C0, 0x116C9),
    (0x11730, 0x11739),
    (0x118E0, 0x118E9),
    (0x11950, 0x11959),
    (0x11C50, 0x11C59),
    (0x11D50, 0x11D59),
    (0x11DA0, 0x11DA9),
    (0x16A60, 0x16A69),
    (0x16AC0, 0x16AC9),
    (0x16B50, 0x16B59),
    (0x1D7CE, 0x1D7FF),
    (0x1E140, 0x1E149),
    (0x1E2F0, 0x1E2F9),
    (0x1E950, 0x1E959),
    (0x1FBF0, 0x1FBF9),
];

fn is_decimal_digit(ch: char) -> bool {
    let value = ch as u32;
    DECIMAL_DIGIT_RANGES
        .binary_search_by(|&(start, end)| {
            if value < start {
                std::cmp::Ordering::Greater
            } else if value > end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

pub(super) fn is_word_char(ch: char) -> bool {
    if private_byte_value(ch).is_some() {
        return false;
    }
    ch.is_alphabetic() || is_decimal_digit(ch)
}

/// GNU's Emacs word class omits `_` in both UTF-8 and C locales, unlike the
/// vi word class used by vi motions.
fn emacs_ascii_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
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

fn ascii_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// GNU's emacs word commands use the Unicode class in a UTF-8 locale, but
/// classify raw non-ASCII bytes as punctuation in a C locale or byte-oriented
/// mode. A caller-supplied break class keeps its own character test.
fn word_matcher(break_chars: Option<&str>, unicode: bool) -> impl Fn(char) -> bool + Copy + '_ {
    move |ch| match break_chars {
        Some(break_chars) => is_word_char_with_breaks(ch, break_chars),
        None if unicode => is_word_char(ch),
        None => emacs_ascii_word_char(ch),
    }
}

/// GNU's vi word commands classify only ASCII letters, digits and underscore
/// as word characters in a UTF-8 locale; emacs word commands keep the Unicode
/// class there, so vi motions need their own matcher.
fn vi_word_matcher(break_chars: Option<&str>) -> impl Fn(char) -> bool + Copy + '_ {
    move |ch| match break_chars {
        Some(break_chars) => is_word_char_with_breaks(ch, break_chars),
        None => ascii_word_char(ch),
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
            | 0x1160..=0x11ff
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
    pub fn copy_backward_word(&mut self, break_chars: Option<&str>, unicode: bool) -> Vec<u8> {
        self.copy_backward_word_by(word_matcher(break_chars, unicode))
    }

    pub(crate) fn copy_backward_words_by_count(
        &self,
        count: u32,
        break_chars: Option<&str>,
        unicode: bool,
    ) -> Vec<u8> {
        if count == 0 {
            return Vec::new();
        }

        let is_word = word_matcher(break_chars, unicode);
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
    pub fn copy_forward_word(&mut self, break_chars: Option<&str>, unicode: bool) -> Vec<u8> {
        self.copy_forward_word_by(word_matcher(break_chars, unicode))
    }

    pub(crate) fn copy_forward_words_by_count(
        &self,
        count: u32,
        break_chars: Option<&str>,
        unicode: bool,
    ) -> Vec<u8> {
        if count == 0 {
            return Vec::new();
        }

        let is_word = word_matcher(break_chars, unicode);
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
    pub fn backward_kill_word(&mut self, break_chars: Option<&str>, unicode: bool) -> Vec<u8> {
        self.backward_kill_word_by(word_matcher(break_chars, unicode))
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
    pub fn kill_word(&mut self, break_chars: Option<&str>, unicode: bool) -> Vec<u8> {
        self.kill_word_by(word_matcher(break_chars, unicode))
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
    pub fn upcase_word(&mut self, break_chars: Option<&str>, unicode: bool) -> bool {
        self.map_next_word_by(word_matcher(break_chars, unicode), |_, ch| {
            ch.to_uppercase().collect()
        })
    }

    /// Upcase previous word preserving point.
    pub fn upcase_previous_word_preserving_point(
        &mut self,
        break_chars: Option<&str>,
        unicode: bool,
    ) -> bool {
        self.upcase_previous_words(1, break_chars, unicode)
    }

    pub(crate) fn upcase_previous_words(
        &mut self,
        count: u32,
        break_chars: Option<&str>,
        unicode: bool,
    ) -> bool {
        self.map_previous_words_by(count, word_matcher(break_chars, unicode), |_, ch| {
            ch.to_uppercase().collect()
        })
    }

    /// Downcase word.
    pub fn downcase_word(&mut self, break_chars: Option<&str>, unicode: bool) -> bool {
        self.map_next_word_by(word_matcher(break_chars, unicode), |_, ch| {
            ch.to_lowercase().collect()
        })
    }

    /// Downcase previous word preserving point.
    pub fn downcase_previous_word_preserving_point(
        &mut self,
        break_chars: Option<&str>,
        unicode: bool,
    ) -> bool {
        self.downcase_previous_words(1, break_chars, unicode)
    }

    pub(crate) fn downcase_previous_words(
        &mut self,
        count: u32,
        break_chars: Option<&str>,
        unicode: bool,
    ) -> bool {
        self.map_previous_words_by(count, word_matcher(break_chars, unicode), |_, ch| {
            ch.to_lowercase().collect()
        })
    }

    /// Capitalize word.
    pub fn capitalize_word(&mut self, break_chars: Option<&str>, unicode: bool) -> bool {
        self.map_next_word_by(word_matcher(break_chars, unicode), capitalize_word_char)
    }

    /// Capitalize previous word preserving point.
    pub fn capitalize_previous_word_preserving_point(
        &mut self,
        break_chars: Option<&str>,
        unicode: bool,
    ) -> bool {
        self.capitalize_previous_words(1, break_chars, unicode)
    }

    pub(crate) fn capitalize_previous_words(
        &mut self,
        count: u32,
        break_chars: Option<&str>,
        unicode: bool,
    ) -> bool {
        self.map_previous_words_by(
            count,
            word_matcher(break_chars, unicode),
            capitalize_word_char,
        )
    }

    /// Transpose words.
    pub fn transpose_words(&mut self, break_chars: Option<&str>, unicode: bool) -> bool {
        self.transpose_words_by_count(1, break_chars, unicode)
    }

    pub(crate) fn transpose_words_by_count(
        &mut self,
        count: i32,
        break_chars: Option<&str>,
        unicode: bool,
    ) -> bool {
        if count == 0 {
            return false;
        }
        let is_word = word_matcher(break_chars, unicode);

        // Resolve GNU's four word boundaries on a copy so an invalid
        // selection leaves the original text and point unchanged.
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
    /// of the line GNU rings and leaves point there. In character mode an
    /// uncased GNU character is skipped the same way, and a cased scalar maps
    /// its whole character, dropping trailing marks. A cased character with
    /// no single-character mapping (such as `ß`) is left alone without
    /// ringing.
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
            // An uncased GNU character is skipped silently when text remains
            // after the skip. A leading mark/Jamo/joiner run skips through its
            // base when the cluster ends before the line end, or through the
            // whole run to the base when the base is final; a run with no base
            // rings, as does a final uncased character.
            let first_end = self.point + ch.len_utf8();
            let mut skip_end = first_end;
            if is_grapheme_extend(ch) || ch == '\u{200d}' {
                let cluster_end = self
                    .next_grapheme_boundary_checked(self.point)
                    .unwrap_or(first_end);
                if cluster_end < self.bytes.len() {
                    skip_end = cluster_end;
                } else {
                    let mut base = first_end;
                    while let Some(next) = self.char_at(base) {
                        if is_grapheme_extend(next) || next == '\u{200d}' {
                            base = self.next_char_boundary(base);
                        } else {
                            break;
                        }
                    }
                    if base >= self.bytes.len() {
                        return true;
                    }
                    skip_end = base;
                }
            }
            if skip_end >= self.bytes.len() {
                return true;
            }
            self.point = skip_end;
            return false;
        }
        let replacement = if lower != ch.to_string() {
            lower
        } else {
            upper
        };
        if replacement.chars().count() != 1 {
            let skip_end = self
                .next_grapheme_boundary_checked(self.point)
                .unwrap_or(self.point + ch.len_utf8());
            if skip_end < self.bytes.len() {
                self.point = skip_end;
            }
            return false;
        }
        // GNU maps the cased scalar at point but replaces its whole GNU
        // character, dropping the combining marks or joiner that follow it.
        let end = self
            .next_grapheme_boundary_checked(self.point)
            .unwrap_or_else(|| self.point + ch.len_utf8());
        let replacement_len = replacement.len();
        self.bytes
            .splice(self.point..end, replacement.as_bytes().iter().copied());
        if self.point + replacement_len < self.bytes.len() {
            self.move_forward();
        }
        false
    }

    /// Forward word.
    pub fn forward_word(&mut self, break_chars: Option<&str>, unicode: bool) -> bool {
        self.forward_word_by(word_matcher(break_chars, unicode))
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
        self.vi_word_motion_by(vi_word_matcher(break_chars), true)
    }

    /// Vi change-word motion (lowercase `cw`, with a numeric argument).
    ///
    /// GNU Readline runs `rl_vi_fword` for the motion, then backs the end
    /// point up to the end of the current run. The cursor itself stays at
    /// the start of the change, so `cw` never crosses the whitespace after
    /// a word on its first repetition.
    pub fn vi_change_words(&mut self, count: u32, break_chars: Option<&str>) -> bool {
        let is_word = vi_word_matcher(break_chars);
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
        let leading_grapheme = self.leading_grapheme_word_end();
        if let Some(end) = leading_grapheme {
            point = end;
        }

        // GNU appends a blank for the motion so the last character can be
        // reached; our boundary helpers already stop at the buffer end.
        // A leading cluster counts as the first word. When only one scalar
        // follows it before whitespace, the first remaining loop iteration
        // just crosses that whitespace, so counted forms need one more step.
        let separated_word = leading_grapheme.is_some_and(|end| class_at(end) == Some(0));
        let remaining = count.saturating_sub(u32::from(leading_grapheme.is_some()))
            + u32::from(count > 1 && separated_word);
        for _ in 0..remaining {
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
        // A mark or joiner after whitespace belongs to that whitespace
        // grapheme. GNU includes the whole grapheme in the changed range, so
        // leave the point at the first character of the following word.
        let prev_is_whitespace_grapheme = self
            .prev_char(point)
            .zip(self.prev_grapheme_boundary_checked(point))
            .is_some_and(|((prev, _), grapheme_start)| {
                grapheme_start < prev && class_at(grapheme_start) == Some(0)
            });
        if !prev_is_whitespace_grapheme {
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
    /// End of the leading mark/ZWJ cluster, when point starts at byte 0 with
    /// a grapheme extend or joiner. GNU groups that first cluster as one vi
    /// word before the following characters.
    fn leading_grapheme_word_end(&self) -> Option<usize> {
        if self.point != 0 {
            return None;
        }
        let (_, first) = self.decoded_char_at(0)?;
        if !is_grapheme_extend(first) && first != '\u{200d}' {
            return None;
        }
        let mut at = self.next_char_boundary(0);
        while let Some((_, ch)) = self.decoded_char_at(at) {
            if is_grapheme_extend(ch) || ch == '\u{200d}' {
                at = self.next_char_boundary(at);
                continue;
            }
            return Some(self.next_char_boundary(at));
        }
        None
    }

    fn vi_word_base_char(&self, idx: usize) -> Option<char> {
        let (_, original) = self.decoded_char_at(idx)?;
        let (_, mut ch) = self.decoded_char_at(idx)?;
        if ch == '\u{200d}' {
            // A joiner follows the base before it when one exists; only a
            // leading joiner run takes its class from the character it joins.
            let mut back = idx;
            while let Some((prev, previous)) = self.decoded_prev_char(back) {
                if !is_grapheme_extend(previous) && previous != '\u{200d}' {
                    return Some(previous);
                }
                back = prev;
            }
            let mut forward = self.next_char_boundary(idx);
            while let Some((_, next)) = self.decoded_char_at(forward) {
                if next != '\u{200d}' {
                    return Some(next);
                }
                forward = self.next_char_boundary(forward);
            }
        }
        let mut at = idx;
        while is_grapheme_extend(ch) || ch == '\u{200d}' {
            match self.decoded_prev_char(at) {
                Some((prev, previous)) => {
                    at = prev;
                    ch = previous;
                }
                None => return Some(original),
            }
        }
        Some(ch)
    }

    fn vi_word_motion_by<F>(&mut self, is_word: F, skip_whitespace: bool) -> bool
    where
        F: Fn(char) -> bool + Copy,
    {
        let start = self.point;
        if let Some(end) = self.leading_grapheme_word_end() {
            self.point = end;
        } else {
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
        self.vi_backward_word_by(vi_word_matcher(break_chars))
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
            self.backup_word_point();
            if self.point == 0 {
                return true;
            }
        }
        if self.point == 0 {
            return false;
        }

        // At the start of a word backed by whitespace, step into the gap.
        if self
            .prev_grapheme_boundary_checked(self.point)
            .is_some_and(|prev| self.word_class_at(prev, &is_word) == 0)
            && !matches!(self.word_class_at(self.point, &is_word), 0 | 3)
        {
            self.point = self.prev_grapheme_boundary_checked(self.point).unwrap();
            if self.point == 0 {
                return true;
            }
        }

        let cur = self.word_class_at(self.point, &is_word);
        let opoint = self.backup_word_point();
        let prev = self.word_class_at(self.point, &is_word);
        if !((cur == 1 && prev != 1) || (cur != 1 && prev == 1)) {
            self.point = opoint;
        }

        while self.point > 0 && self.word_class_at(self.point, &is_word) == 0 {
            self.backup_word_point();
        }

        if self.point > 0 {
            let mut opoint;
            if self.word_class_at(self.point, &is_word) == 1 {
                loop {
                    opoint = self.backup_word_point();
                    if self.point == 0 || self.word_class_at(self.point, &is_word) != 1 {
                        break;
                    }
                }
            } else {
                loop {
                    opoint = self.backup_word_point();
                    if self.point == 0 || self.word_class_at(self.point, &is_word) <= 1 {
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

    fn word_class_at<F>(&self, idx: usize, is_word: &F) -> u8
    where
        F: Fn(char) -> bool,
    {
        match self.vi_word_base_char(idx) {
            Some(ch) if ch.is_whitespace() => 0,
            Some(ch) if is_word(ch) => 1,
            Some(_) => 2,
            None => 3,
        }
    }

    fn backup_word_point(&mut self) -> usize {
        let old = self.point;
        if let Some(prev) = self.prev_grapheme_boundary_checked(self.point) {
            self.point = prev;
        }
        old
    }

    /// Vi end word (lowercase `e`).
    ///
    /// As in the underlying vi word motion, a contiguous punctuation run is
    /// its own word class. From inside a run `e` stops at its end; from the
    /// end of a run it advances to the end of the following run, skipping
    /// whitespace on the way.
    pub fn vi_end_word(&mut self, break_chars: Option<&str>) -> bool {
        self.vi_end_word_by(vi_word_matcher(break_chars))
    }

    fn vi_end_word_by<F>(&mut self, is_word: F) -> bool
    where
        F: Fn(char) -> bool,
    {
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
        // A leading mark/Jamo/ZWJ run ending at the line boundary absorbs its
        // only following base in one grapheme step. GNU's end-word stops on
        // that base, not on the leading run. A trailing mark or joiner on the
        // base must not be mistaken for the base itself.
        let end = self.bytes.len();
        if self.point < end
            && self
                .char_at(self.point)
                .is_some_and(|ch| is_grapheme_extend(ch) || ch == '\u{200d}')
            && self.next_grapheme_boundary_checked(self.point) == Some(end)
            && let Some((mut base, mut base_ch)) = self.decoded_prev_char_backward(end)
        {
            while (is_grapheme_extend(base_ch) || base_ch == '\u{200d}')
                && let Some((previous, previous_ch)) = self.decoded_prev_char_backward(base)
            {
                base = previous;
                base_ch = previous_ch;
            }
            if base > self.point && !is_grapheme_extend(base_ch) && base_ch != '\u{200d}' {
                self.point = base;
                if let Some(end) = self.next_boundary(base) {
                    self.set_operator_grapheme_end(end);
                }
                return self.point != start;
            }
        }
        let Some(base) = self.vi_word_base_char(self.point) else {
            return false;
        };
        let current = class(base);
        if current != 0
            && self
                .next_grapheme_boundary_checked(self.point)
                .and_then(|next| self.vi_word_base_char(next))
                .is_some_and(|ch| class(ch) == current)
        {
            self.advance_run_in(&class, current);
        } else {
            // At the end of a run, `e` moves to the end of the following
            // run; from whitespace it moves to the end of the next run
            // without the extra step.
            if current != 0
                && let Some(next) = self.next_grapheme_boundary_checked(self.point)
                && self.char_at(next).is_some()
            {
                self.point = next;
            }
            self.skip_whitespace_in(&class);
            if let Some(target) = self.vi_word_base_char(self.point) {
                let target = class(target);
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
        self.vi_end_byte_class_bytewise(|byte| {
            crate::buffer::vi_byte_class(byte, break_chars, false)
        })
    }

    /// Byte-oriented `E` ends at the last byte of a non-whitespace run.
    pub fn end_bigword_bytewise(&mut self) -> bool {
        self.vi_end_byte_class_bytewise(|byte| crate::buffer::vi_byte_class(byte, None, true))
    }

    fn vi_end_byte_class_bytewise<F>(&mut self, class: F) -> bool
    where
        F: Fn(u8) -> u8,
    {
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
        while self
            .vi_word_base_char(self.point)
            .is_some_and(|ch| class(ch) == 0)
        {
            let Some(next) = self.next_grapheme_boundary_checked(self.point) else {
                break;
            };
            self.point = next;
        }
    }

    fn advance_run_in<F>(&mut self, class: &F, target: u8)
    where
        F: Fn(char) -> u8,
    {
        while let Some(next) = self.next_grapheme_boundary_checked(self.point) {
            if self
                .vi_word_base_char(next)
                .is_some_and(|ch| class(ch) == target)
            {
                self.point = next;
            } else {
                break;
            }
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
    pub fn end_word(&mut self, break_chars: Option<&str>, unicode: bool) -> bool {
        self.end_word_by(word_matcher(break_chars, unicode))
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
        self.vi_end_word_by(|ch| !ch.is_whitespace())
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
    pub fn backward_word(&mut self, break_chars: Option<&str>, unicode: bool) -> bool {
        self.backward_word_by(word_matcher(break_chars, unicode))
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
