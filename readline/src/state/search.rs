use history::History;

#[derive(Debug, Clone, Default)]
pub(crate) struct ReverseSearchState {
    pub(crate) query: Vec<u8>,
    pub(crate) match_line: Option<Vec<u8>>,
    pub(crate) match_index: Option<usize>,
    pub(crate) direction: SearchDirection,
    pub(crate) original_line: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct NonIncrementalSearchState {
    pub(crate) query: Vec<u8>,
    pub(crate) direction: SearchDirection,
    pub(crate) original_line: Vec<u8>,
    pub(crate) original_history_pos: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum SearchDirection {
    Forward,
    #[default]
    Backward,
}

pub(crate) fn update_reverse_search_match(
    search: &mut ReverseSearchState,
    history: &History,
    repeat: bool,
    ignore_case: bool,
) {
    let pivot = repeat.then_some(search.match_index).flatten();
    let found = match search.direction {
        SearchDirection::Backward => {
            search_history_backward(history, &search.query, pivot, ignore_case)
        }
        SearchDirection::Forward => {
            search_history_forward(history, &search.query, pivot, ignore_case)
        }
    };
    if let Some((idx, line)) = found {
        search.match_index = Some(idx);
        search.match_line = Some(line);
    } else if !repeat {
        search.match_index = None;
        search.match_line = None;
    }
}

pub(crate) fn search_history_backward(
    history: &History,
    needle: &[u8],
    before: Option<usize>,
    ignore_case: bool,
) -> Option<(usize, Vec<u8>)> {
    if needle.is_empty() {
        return None;
    }
    let needle = normalize_search_bytes(needle, ignore_case);
    let end = before
        .unwrap_or(history.entries().len())
        .min(history.entries().len());
    history.entries()[..end]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, entry)| {
            contains_bytes(
                &normalize_search_bytes(&entry.line_bytes, ignore_case),
                &needle,
            )
        })
        .map(|(idx, entry)| (idx, entry.line_bytes.clone()))
}

pub(crate) fn search_history_forward(
    history: &History,
    needle: &[u8],
    after: Option<usize>,
    ignore_case: bool,
) -> Option<(usize, Vec<u8>)> {
    if needle.is_empty() {
        return None;
    }
    let needle = normalize_search_bytes(needle, ignore_case);
    let start = after
        .map(|idx| idx + 1)
        .unwrap_or(0)
        .min(history.entries().len());
    history.entries()[start..]
        .iter()
        .enumerate()
        .find(|(_, entry)| {
            contains_bytes(
                &normalize_search_bytes(&entry.line_bytes, ignore_case),
                &needle,
            )
        })
        .map(|(offset, entry)| (start + offset, entry.line_bytes.clone()))
}

pub(crate) fn normalize_search_bytes(value: &[u8], ignore_case: bool) -> Vec<u8> {
    if ignore_case {
        value.iter().map(|byte| byte.to_ascii_lowercase()).collect()
    } else {
        value.to_vec()
    }
}

pub(crate) fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}
