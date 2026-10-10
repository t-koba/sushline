use history::History;

#[derive(Debug, Default)]
pub(crate) struct SearchState {
    pub(crate) reverse_search: Option<ReverseSearchState>,
    pub(crate) non_incremental_search: Option<NonIncrementalSearchState>,
    pub(crate) last_search: Option<Vec<u8>>,
    pub(crate) last_search_direction: Option<SearchDirection>,
    pub(crate) quoted_pending: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ReverseSearchState {
    pub(crate) query: Vec<u8>,
    pub(crate) match_line: Option<Vec<u8>>,
    pub(crate) match_index: Option<usize>,
    pub(crate) direction: SearchDirection,
    pub(crate) original_line: Vec<u8>,
    pub(crate) original_point: usize,
    pub(crate) original_history_pos: usize,
    pub(crate) exclude_cursor: bool,
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
) -> bool {
    // GNU starts the first emacs search from the history cursor, inclusively:
    // backward covers entries[..original_pos + 1], forward covers
    // entries[original_pos..]. Vi `/` (backward) and `?` (forward) skip the
    // cursor entry: backward covers entries[..original_pos], forward covers
    // entries[original_pos + 1..] (patch 0 Bash 5.3 PTY oracle: from a
    // matching cursor `/alpha` finds the older match and `?alpha` finds the
    // newer match). Repeats step exclusively past the current match. A
    // repeat with no current match restarts from the cursor.
    let found = match (search.direction, repeat, search.match_index) {
        (SearchDirection::Backward, true, Some(idx)) => {
            search_history_backward(history, &search.query, Some(idx), ignore_case)
        }
        (SearchDirection::Forward, true, Some(idx)) => {
            search_history_forward(history, &search.query, Some(idx), ignore_case)
        }
        (SearchDirection::Backward, _, _) => {
            let end = if search.exclude_cursor {
                search.original_history_pos.min(history.entries().len())
            } else {
                search
                    .original_history_pos
                    .saturating_add(1)
                    .min(history.entries().len())
            };
            search_history_backward(history, &search.query, Some(end), ignore_case)
        }
        (SearchDirection::Forward, _, _) => {
            let start = if search.exclude_cursor {
                search
                    .original_history_pos
                    .saturating_add(1)
                    .min(history.entries().len())
            } else {
                search.original_history_pos.min(history.entries().len())
            };
            search_history_forward_from(history, &search.query, start, ignore_case)
        }
    };
    if let Some((idx, line)) = found {
        search.match_index = Some(idx);
        search.match_line = Some(line);
        return true;
    }
    if !(repeat && search.match_index.is_some()) {
        search.match_index = None;
        search.match_line = None;
    }
    false
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
    let start = after
        .map(|idx| idx.saturating_add(1))
        .unwrap_or(0)
        .min(history.entries().len());
    search_history_forward_from(history, needle, start, ignore_case)
}

pub(crate) fn search_history_forward_from(
    history: &History,
    needle: &[u8],
    start: usize,
    ignore_case: bool,
) -> Option<(usize, Vec<u8>)> {
    if needle.is_empty() {
        return None;
    }
    let needle = normalize_search_bytes(needle, ignore_case);
    let start = start.min(history.entries().len());
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
