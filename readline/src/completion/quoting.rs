use crate::state::EditorState;

#[derive(Clone, Debug)]
pub(crate) struct CompletionEdit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) word_bytes: Vec<u8>,
    pub(crate) quote: Option<char>,
    pub(crate) ansi_c_quote: bool,
    pub(crate) line: Vec<u8>,
    pub(crate) point: usize,
    // Built-in lookup may use a reopened segment, while callbacks and the
    // replacement range continue to describe the entire original word.
    pub(super) filename_segment: Option<FilenameSegment>,
    pub(super) builtin_filename_completion: bool,
}

#[derive(Clone, Debug)]
pub(super) struct FilenameSegment {
    pub(super) raw_start: usize,
    pub(super) word_bytes: Vec<u8>,
    pub(super) word_prefix: Vec<u8>,
}

impl CompletionEdit {
    pub(super) fn matching_word(&self) -> &[u8] {
        self.filename_segment
            .as_ref()
            .map_or(&self.word_bytes, |segment| &segment.word_bytes)
    }

    pub(super) fn has_initial_open_single_quote(&self) -> bool {
        if self.ansi_c_quote || self.quote != Some('\'') {
            return false;
        }
        let Some(raw) = self.line.get(self.start..self.end) else {
            return false;
        };
        // A later apostrophe may close/reopen the quote to the right of point;
        // keep those existing suffix cases on their original requoting path.
        raw.first() == Some(&b'\'')
            && !raw[1..].contains(&b'\'')
            && !self
                .line
                .get(self.end..)
                .is_some_and(|suffix| suffix.contains(&b'\''))
    }

    pub(super) fn reopened_filename_segment(&self) -> Option<FilenameSegment> {
        if self.ansi_c_quote {
            return None;
        }
        let raw = self.line.get(self.start..self.end)?;
        let (_, quote, opening) = dequote_completion_word_with_opening(raw);
        let opening = opening.filter(|opening| *opening > 0)?;
        if quote != Some('\'') || opening + 1 == raw.len() {
            return None;
        }
        Some(FilenameSegment {
            raw_start: self.start + opening,
            word_bytes: dequote_completion_word_bytes(&raw[opening..]).0,
            word_prefix: dequote_completion_word_bytes(&raw[..opening]).0,
        })
    }
}

pub(super) fn completion_edit(
    state: &EditorState,
    break_chars: &[u8],
    include_point_char: bool,
) -> CompletionEdit {
    let (mut start, end) = if include_point_char {
        state.buffer.vi_completion_word_bounds(Some(break_chars))
    } else {
        state.buffer.completion_word_bounds(Some(break_chars))
    };
    let bytes = state.buffer.as_bytes();
    if start > 0 && matches!(bytes.get(start - 1), Some(b'\'' | b'"')) {
        start -= 1;
    }
    let raw_bytes = state
        .buffer
        .as_bytes()
        .get(start..end)
        .unwrap_or_default()
        .to_vec();
    let ansi_c_quote = raw_bytes.starts_with(b"$'");
    let (word_bytes, quote) = dequote_completion_word_bytes(&raw_bytes);
    CompletionEdit {
        start,
        end,
        word_bytes,
        quote,
        ansi_c_quote,
        line: bytes.to_vec(),
        point: state.buffer.byte_point(),
        filename_segment: None,
        builtin_filename_completion: false,
    }
}

pub(super) fn dequote_completion_word_bytes(raw: &[u8]) -> (Vec<u8>, Option<char>) {
    let (word, quote, _) = dequote_completion_word_with_opening(raw);
    (word, quote)
}

fn dequote_completion_word_with_opening(raw: &[u8]) -> (Vec<u8>, Option<char>, Option<usize>) {
    let ansi_c_quote = raw.starts_with(b"$'");
    let mut idx = if ansi_c_quote { 2 } else { 0 };
    // A plain single-quoted word can continue through unquoted escaped bytes
    // and reopened quotes. Its quote at point, not its first delimiter, governs
    // both filename requoting and decoding the right-hand suffix.
    let track_quote_transitions = raw.first() == Some(&b'\'');
    let mut quote = if ansi_c_quote {
        Some('\'')
    } else {
        match raw.first().copied() {
            Some(b'\'') => {
                idx = 1;
                Some('\'')
            }
            Some(b'"') => {
                idx = 1;
                Some('"')
            }
            _ => None,
        }
    };
    let mut opening = quote.map(|_| 0);
    let mut out = Vec::with_capacity(raw.len().saturating_sub(idx));
    let mut escaped = false;
    while let Some(byte) = raw.get(idx).copied() {
        idx += 1;
        if escaped {
            out.push(byte);
            escaped = false;
            continue;
        }
        if byte == b'\\' && quote != Some('\'') {
            if quote == Some('"') {
                match raw.get(idx).copied() {
                    Some(b'$' | b'`' | b'"' | b'\\') => escaped = true,
                    Some(b'\n') => idx += 1,
                    Some(_) | None => out.push(b'\\'),
                }
            } else {
                escaped = true;
            }
            continue;
        }
        if quote == Some(byte as char) {
            if track_quote_transitions {
                quote = None;
                opening = None;
            }
            continue;
        }
        if track_quote_transitions && quote.is_none() && matches!(byte, b'\'' | b'"') {
            quote = Some(byte as char);
            opening = Some(idx - 1);
            continue;
        }
        out.push(byte);
    }
    // A pending unquoted escape after a single-quoted segment has no filename
    // byte yet. In particular, point between the backslash and apostrophe must
    // still match candidates for the decoded prefix, not a literal backslash.
    if escaped && !track_quote_transitions {
        out.push(b'\\');
    }
    (out, quote, opening)
}
pub(super) fn quote_filename_bytes(
    bytes: &[u8],
    escape_leading_tilde: bool,
    escape_leading_hash: bool,
    single_quote_line_feeds: bool,
) -> Vec<u8> {
    if single_quote_line_feeds && bytes.contains(&b'\n') {
        return quote_single_quoted_bytes(bytes);
    }
    let mut out = Vec::with_capacity(bytes.len());
    if escape_leading_tilde && bytes.first() == Some(&b'~') {
        out.push(b'\\');
    }
    if escape_leading_hash && bytes.first() == Some(&b'#') {
        out.push(b'\\');
    }
    let escape_closing_brackets = bytes.contains(&b'[');
    let escape_brace_expansion = bytes.contains(&b'{');
    for byte in bytes {
        if matches!(
            byte,
            b' ' | b'\t'
                | b'\n'
                | b'\\'
                | b'\''
                | b'"'
                | b'$'
                | b'`'
                | b'!'
                | b'&'
                | b'|'
                | b';'
                | b'<'
                | b'>'
                | b'('
                | b')'
                | b'['
                | b'{'
                | b'*'
                | b'?'
        ) || (*byte == b']' && escape_closing_brackets)
            || (escape_brace_expansion && matches!(*byte, b'}' | b','))
        {
            out.push(b'\\');
        }
        out.push(*byte);
    }
    out
}

pub(super) fn skip_completed_suffix_bytes(replacement: &[u8], suffix: &[u8]) -> Vec<u8> {
    let overlap = completed_suffix_overlap_len(replacement, suffix);
    replacement[..replacement.len().saturating_sub(overlap)].to_vec()
}

pub(super) fn completed_suffix_overlap_len(replacement: &[u8], suffix: &[u8]) -> usize {
    (1..=suffix.len())
        .rev()
        .find(|count| replacement.ends_with(&suffix[..*count]))
        .unwrap_or(0)
}

// Only skip text at the cursor when it matches the candidate's remaining prefix.
pub(super) fn completion_suffix_overlap_len(completion: &[u8], suffix: &[u8]) -> usize {
    completion
        .iter()
        .zip(suffix)
        .take_while(|(completion_byte, suffix_byte)| completion_byte == suffix_byte)
        .count()
}

pub(super) fn completion_suffix_bytes(edit: &CompletionEdit, state: &EditorState) -> Vec<u8> {
    completion_suffix_bytes_at(edit.end, state)
}

pub(super) fn completion_suffix_bytes_at(end: usize, state: &EditorState) -> Vec<u8> {
    state
        .buffer
        .as_bytes()
        .get(end..)
        .unwrap_or_default()
        .iter()
        .copied()
        .take_while(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>()
}

pub(super) fn insert_disabled_completion_key(state: &mut EditorState, key: &[u8]) -> bool {
    if key == b"\t" {
        state.buffer.insert_char('\t');
        return true;
    }
    let Ok(text) = std::str::from_utf8(key) else {
        return false;
    };
    let mut inserted = false;
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        state.buffer.insert_char(ch);
        inserted = true;
    }
    inserted
}

pub(super) fn quote_single_quoted_bytes(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() + 2);
    out.push(b'\'');
    for byte in value {
        if *byte == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(*byte);
        }
    }
    out.push(b'\'');
    out
}

// Bash PTY probes use `$'…'` spelling and leave backslashes raw; apostrophes
// use the same shell quote-concatenation form as ordinary single quotes.
pub(super) fn quote_ansi_c_quoted_filename_bytes(value: &[u8]) -> Vec<u8> {
    let quoted = quote_single_quoted_bytes(value);
    let mut out = Vec::with_capacity(quoted.len() + 1);
    out.push(b'$');
    out.extend_from_slice(&quoted);
    out
}

pub(super) fn quote_double_quoted_bytes(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() + 2);
    out.push(b'"');
    for byte in value {
        if matches!(*byte, b'"' | b'\\' | b'$' | b'`') {
            out.push(b'\\');
        }
        out.push(*byte);
    }
    out.push(b'"');
    out
}

// Bash filename completion leaves `$` and backtick bytes raw in double quotes.
pub(super) fn quote_double_quoted_filename_bytes(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() + 2);
    out.push(b'"');
    for byte in value {
        if matches!(*byte, b'"' | b'\\') {
            out.push(b'\\');
        }
        out.push(*byte);
    }
    out.push(b'"');
    out
}

#[cfg(test)]
mod tests {
    use super::{
        dequote_completion_word_bytes, quote_double_quoted_filename_bytes, quote_filename_bytes,
    };

    #[test]
    fn filename_quoting_uses_single_quotes_for_line_feeds() {
        assert_eq!(
            quote_filename_bytes(b"safe-lf\nitem", true, false, true),
            b"'safe-lf\nitem'",
        );
        assert_eq!(
            quote_filename_bytes(b"~/safe-lf\nitem", false, false, false),
            b"~/safe-lf\\\nitem",
        );
    }

    #[test]
    fn filename_quoting_escapes_only_a_shell_comment_leading_hash_when_requested() {
        assert_eq!(
            quote_filename_bytes(b"#safe-item", false, true, true),
            b"\\#safe-item"
        );
        assert_eq!(
            quote_filename_bytes(b"#safe-item", false, false, true),
            b"#safe-item"
        );
        assert_eq!(
            quote_filename_bytes(b"safe-#item", false, true, true),
            b"safe-#item"
        );
    }

    #[test]
    fn double_quoted_filename_quoting_leaves_dollar_and_backtick_unescaped() {
        assert_eq!(
            quote_double_quoted_filename_bytes(b"$`\"\\"),
            b"\"$`\\\"\\\\\""
        );
    }

    #[test]
    fn filename_quoting_escapes_close_brackets_only_with_open_brackets() {
        assert_eq!(
            quote_filename_bytes(b"safe-right]item", false, false, true),
            b"safe-right]item"
        );
        assert_eq!(
            quote_filename_bytes(b"safe-set[abc]", false, false, true),
            b"safe-set\\[abc\\]"
        );
        assert_eq!(
            quote_filename_bytes(b"safe-rev]open[", false, false, true),
            b"safe-rev\\]open\\["
        );
    }

    #[test]
    fn filename_quoting_escapes_brace_closers_and_commas_with_open_braces() {
        assert_eq!(
            quote_filename_bytes(b"safe-right}item", false, false, true),
            b"safe-right}item"
        );
        assert_eq!(
            quote_filename_bytes(b"safe-a,b", false, false, true),
            b"safe-a,b"
        );
        assert_eq!(
            quote_filename_bytes(b"safe-pair{a,b}", false, false, true),
            b"safe-pair\\{a\\,b\\}"
        );
        assert_eq!(
            quote_filename_bytes(b"safe-open{a,b", false, false, true),
            b"safe-open\\{a\\,b"
        );
        assert_eq!(
            quote_filename_bytes(b"safe-rev}a,b{", false, false, true),
            b"safe-rev\\}a\\,b\\{"
        );
    }

    #[test]
    fn filename_quoting_escapes_only_a_leading_tilde() {
        assert_eq!(quote_filename_bytes(b"~", true, false, true), b"\\~");
        assert_eq!(
            quote_filename_bytes(b"~Alpha", true, false, true),
            b"\\~Alpha"
        );
        assert_eq!(
            quote_filename_bytes(b"alpha~beta", true, false, true),
            b"alpha~beta"
        );
        assert_eq!(
            quote_filename_bytes(b"alpha/~beta", true, false, true),
            b"alpha/~beta"
        );
        assert_eq!(
            quote_filename_bytes(b"~user/", false, false, true),
            b"~user/"
        );
    }

    #[test]
    fn single_quoted_words_decode_concatenated_segments_and_track_quote_at_point() {
        for (raw, expected, quote) in [
            (b"'safe".as_slice(), b"safe".as_slice(), Some('\'')),
            (b"'safe'".as_slice(), b"safe".as_slice(), None),
            (br"'safe'\".as_slice(), b"safe".as_slice(), None),
            (br"'safe'\'''\".as_slice(), b"safe'".as_slice(), None),
            (br"'safe'\''".as_slice(), b"safe'".as_slice(), Some('\'')),
            (br"'safe'\'''".as_slice(), b"safe'".as_slice(), None),
            (
                br"'safe'\'''\''".as_slice(),
                b"safe''".as_slice(),
                Some('\''),
            ),
        ] {
            assert_eq!(
                dequote_completion_word_bytes(raw),
                (expected.to_vec(), quote)
            );
        }
        // A backslash inside the single-quoted segment is still literal.
        assert_eq!(
            dequote_completion_word_bytes(br"'safe\name"),
            (br"safe\name".to_vec(), Some('\'')),
        );
    }

    #[test]
    fn double_quotes_only_dequote_bash_escapable_bytes() {
        assert_eq!(
            dequote_completion_word_bytes(b"\"\\~/Al"),
            (b"\\~/Al".to_vec(), Some('"')),
        );
        assert_eq!(
            dequote_completion_word_bytes(b"\"\\x"),
            (b"\\x".to_vec(), Some('"')),
        );
        for (raw, expected) in [
            (b"\"\\$".as_slice(), b"$".as_slice()),
            (b"\"\\`".as_slice(), b"`".as_slice()),
            (b"\"\\\"".as_slice(), b"\"".as_slice()),
            (b"\"\\\\".as_slice(), b"\\".as_slice()),
            (b"\"\\\n".as_slice(), b"".as_slice()),
        ] {
            assert_eq!(
                dequote_completion_word_bytes(raw),
                (expected.to_vec(), Some('"'))
            );
        }
        assert_eq!(
            dequote_completion_word_bytes(b"$'safe-prefix"),
            (b"safe-prefix".to_vec(), Some('\'')),
        );
    }
}
