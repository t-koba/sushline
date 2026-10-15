use crate::state::EditorState;

#[derive(Clone, Debug)]
pub(crate) struct CompletionEdit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) word_bytes: Vec<u8>,
    pub(crate) quote: Option<char>,
    pub(crate) line: Vec<u8>,
    pub(crate) point: usize,
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
    let (word_bytes, quote) = dequote_completion_word_bytes(&raw_bytes);
    CompletionEdit {
        start,
        end,
        word_bytes,
        quote,
        line: bytes.to_vec(),
        point: state.buffer.byte_point(),
    }
}

pub(super) fn dequote_completion_word_bytes(raw: &[u8]) -> (Vec<u8>, Option<char>) {
    let mut idx = 0;
    let quote = match raw.first().copied() {
        Some(b'\'') => {
            idx = 1;
            Some('\'')
        }
        Some(b'"') => {
            idx = 1;
            Some('"')
        }
        _ => None,
    };
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
            continue;
        }
        out.push(byte);
    }
    if escaped {
        out.push(b'\\');
    }
    (out, quote)
}
pub(super) fn quote_filename_bytes(
    bytes: &[u8],
    escape_leading_tilde: bool,
    single_quote_line_feeds: bool,
) -> Vec<u8> {
    if single_quote_line_feeds && bytes.contains(&b'\n') {
        return quote_single_quoted_bytes(bytes);
    }
    let mut out = Vec::with_capacity(bytes.len());
    if escape_leading_tilde && bytes.first() == Some(&b'~') {
        out.push(b'\\');
    }
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
                | b']'
                | b'{'
                | b'}'
                | b'*'
                | b'?'
        ) {
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

pub(super) fn completion_suffix_bytes(edit: &CompletionEdit, state: &EditorState) -> Vec<u8> {
    let bytes = state.buffer.as_bytes();
    bytes
        .get(edit.end..)
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
            quote_filename_bytes(b"safe-lf\nitem", true, true),
            b"'safe-lf\nitem'",
        );
        assert_eq!(
            quote_filename_bytes(b"~/safe-lf\nitem", false, false),
            b"~/safe-lf\\\nitem",
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
    fn filename_quoting_escapes_only_a_leading_tilde() {
        assert_eq!(quote_filename_bytes(b"~", true, true), b"\\~");
        assert_eq!(quote_filename_bytes(b"~Alpha", true, true), b"\\~Alpha");
        assert_eq!(
            quote_filename_bytes(b"alpha~beta", true, true),
            b"alpha~beta"
        );
        assert_eq!(
            quote_filename_bytes(b"alpha/~beta", true, true),
            b"alpha/~beta"
        );
        assert_eq!(quote_filename_bytes(b"~user/", false, true), b"~user/");
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
    }
}
