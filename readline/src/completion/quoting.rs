use crate::state::EditorState;

pub(super) struct CompletionEdit {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) word_bytes: Vec<u8>,
    pub(super) quote: Option<char>,
}

pub(super) fn completion_edit(state: &EditorState, break_chars: &[u8]) -> CompletionEdit {
    let (mut start, end) = state.buffer.completion_word_bounds(Some(break_chars));
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
            escaped = true;
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
pub(super) fn quote_filename_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
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
                | b'#'
        ) {
            out.push(b'\\');
        }
        out.push(*byte);
    }
    out
}

pub(super) fn skip_completed_suffix_bytes(
    replacement: &[u8],
    edit: &CompletionEdit,
    state: &EditorState,
) -> Vec<u8> {
    let suffix = completion_suffix_bytes(edit, state);
    if suffix.is_empty() {
        return replacement.to_vec();
    }
    let mut end = replacement.len();
    for count in (1..=suffix.len()).rev() {
        if replacement.ends_with(&suffix[..count]) {
            end = replacement.len().saturating_sub(count);
            break;
        }
    }
    replacement[..end].to_vec()
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
