use crate::locale::LocaleLowercase;
use history::{History, HistoryDirection, HistorySearchMatch};

#[derive(Debug, Default)]
pub(crate) struct SearchState {
    pub(crate) reverse_search: Option<ReverseSearchState>,
    pub(crate) non_incremental_search: Option<NonIncrementalSearchState>,
    pub(crate) last_incremental_search: Option<Vec<u8>>,
    pub(crate) last_incremental_search_direction: Option<SearchDirection>,
    pub(crate) last_non_incremental_search: Option<Vec<u8>>,
    pub(crate) last_non_incremental_search_direction: Option<SearchDirection>,
    pub(crate) last_non_incremental_search_match_index: Option<usize>,
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
    /// True for vi `/`/`?` (prompts `/`/`?`, cursor-exclusive backward
    /// search); false for emacs non-incremental search (prompt `:`).
    pub(crate) vi: bool,
}

impl NonIncrementalSearchState {
    pub(crate) fn prompt_char(&self) -> char {
        if !self.vi {
            return ':';
        }
        match self.direction {
            SearchDirection::Backward => '/',
            SearchDirection::Forward => '?',
        }
    }

    pub(crate) fn prompt_text(&self) -> String {
        format!(
            "{}{}",
            self.prompt_char(),
            String::from_utf8_lossy(&self.query)
        )
    }
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
    locale_lowercase: Option<&LocaleLowercase>,
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
        (SearchDirection::Backward, true, Some(idx)) => search_history_backward(
            history,
            &search.query,
            Some(idx),
            ignore_case,
            locale_lowercase,
        ),
        (SearchDirection::Forward, true, Some(idx)) => search_history_forward(
            history,
            &search.query,
            Some(idx),
            ignore_case,
            locale_lowercase,
        ),
        (SearchDirection::Backward, _, _) => {
            let end = if search.exclude_cursor {
                search.original_history_pos.min(history.entries().len())
            } else {
                search
                    .original_history_pos
                    .saturating_add(1)
                    .min(history.entries().len())
            };
            search_history_backward(
                history,
                &search.query,
                Some(end),
                ignore_case,
                locale_lowercase,
            )
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
            search_history_forward_from(
                history,
                &search.query,
                start,
                ignore_case,
                locale_lowercase,
            )
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
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<(usize, Vec<u8>)> {
    if needle.is_empty() {
        return None;
    }
    let end = before
        .unwrap_or(history.entries().len())
        .min(history.entries().len());
    history.entries()[..end]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, entry)| {
            search_bytes_offset(
                &entry.line_bytes,
                needle,
                ignore_case,
                locale_lowercase,
                false,
            )
            .is_some()
        })
        .map(|(idx, entry)| (idx, entry.line_bytes.clone()))
}

pub(crate) fn search_history_forward(
    history: &History,
    needle: &[u8],
    after: Option<usize>,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<(usize, Vec<u8>)> {
    let start = after
        .map(|idx| idx.saturating_add(1))
        .unwrap_or(0)
        .min(history.entries().len());
    search_history_forward_from(history, needle, start, ignore_case, locale_lowercase)
}

pub(crate) fn search_history_forward_from(
    history: &History,
    needle: &[u8],
    start: usize,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<(usize, Vec<u8>)> {
    if needle.is_empty() {
        return None;
    }
    let start = start.min(history.entries().len());
    history.entries()[start..]
        .iter()
        .enumerate()
        .find(|(_, entry)| {
            search_bytes_offset(
                &entry.line_bytes,
                needle,
                ignore_case,
                locale_lowercase,
                false,
            )
            .is_some()
        })
        .map(|(offset, entry)| (start + offset, entry.line_bytes.clone()))
}

pub(crate) fn search_bytes_offset(
    haystack: &[u8],
    needle: &[u8],
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
    from_end: bool,
) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if let Some(locale_lowercase) = locale_lowercase
        && ignore_case
        && let (Ok(haystack_text), Ok(needle_text)) =
            (std::str::from_utf8(haystack), std::str::from_utf8(needle))
    {
        let lowered_needle = needle_text
            .chars()
            .map(|character| locale_lowercase.lowercase(character))
            .collect::<Vec<_>>();
        if lowered_needle.is_empty() {
            return Some(0);
        }
        let lowered_haystack = haystack_text
            .char_indices()
            .map(|(offset, character)| (locale_lowercase.lowercase(character), offset))
            .collect::<Vec<_>>();
        let mut starts = lowered_haystack
            .windows(lowered_needle.len())
            .enumerate()
            .filter_map(|(index, window)| {
                window
                    .iter()
                    .map(|(character, _)| *character)
                    .eq(lowered_needle.iter().copied())
                    .then_some(lowered_haystack[index].1)
            });
        return if from_end {
            starts.next_back()
        } else {
            starts.next()
        };
    }
    if needle.len() > haystack.len() {
        return None;
    }
    let mut matches = haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(offset, window)| {
            let equal = if ignore_case {
                window
                    .iter()
                    .zip(needle)
                    .all(|(left, right)| left.eq_ignore_ascii_case(right))
            } else {
                window == needle
            };
            equal.then_some(offset)
        });
    if from_end {
        matches.next_back()
    } else {
        matches.next()
    }
}

pub(crate) fn search_prefix_matches(
    haystack: &[u8],
    needle: &[u8],
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> bool {
    if let Some(locale_lowercase) = locale_lowercase
        && ignore_case
        && let (Ok(haystack_text), Ok(needle_text)) =
            (std::str::from_utf8(haystack), std::str::from_utf8(needle))
    {
        let mut haystack_chars = haystack_text.chars();
        return needle_text.chars().all(|needle_char| {
            haystack_chars.next().is_some_and(|haystack_char| {
                locale_lowercase.lowercase(haystack_char) == locale_lowercase.lowercase(needle_char)
            })
        });
    }
    haystack.get(..needle.len()).is_some_and(|prefix| {
        if ignore_case {
            prefix
                .iter()
                .zip(needle)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        } else {
            prefix == needle
        }
    })
}

pub(crate) fn history_search_with_locale(
    history: &mut History,
    needle: &[u8],
    direction: HistoryDirection,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<HistorySearchMatch> {
    if !ignore_case || locale_lowercase.is_none() {
        return history.history_search_bytes_with_case(needle, direction, ignore_case);
    }
    if needle.is_empty() || history.entries().is_empty() {
        return None;
    }
    let position = history.where_history().min(history.entries().len());
    let found = match direction {
        HistoryDirection::Previous => {
            let end = position.min(history.entries().len().saturating_sub(1));
            history.entries()[..=end]
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, entry)| {
                    search_bytes_offset(&entry.line_bytes, needle, true, locale_lowercase, true)
                        .map(|offset| (index, offset, entry.line_bytes.clone()))
                })
        }
        HistoryDirection::Next => {
            history.entries()[position..]
                .iter()
                .enumerate()
                .find_map(|(offset, entry)| {
                    search_bytes_offset(&entry.line_bytes, needle, true, locale_lowercase, false)
                        .map(|byte_offset| {
                            (position + offset, byte_offset, entry.line_bytes.clone())
                        })
                })
        }
    }?;
    history.set_pos(found.0);
    Some(HistorySearchMatch {
        entry_index: found.0,
        byte_offset: found.1,
        line_bytes: found.2,
    })
}

pub(crate) fn history_prefix_backward_with_locale(
    history: &mut History,
    prefix: &[u8],
    current: Vec<u8>,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<Vec<u8>> {
    if !ignore_case || locale_lowercase.is_none() {
        return history.search_prefix_backward_bytes_with_case(prefix, current, ignore_case);
    }
    history_prefix_backward_matching(history, prefix, current, |line| {
        search_prefix_matches(line, prefix, true, locale_lowercase)
    })
}

fn history_prefix_backward_matching(
    history: &mut History,
    prefix: &[u8],
    current: Vec<u8>,
    matches: impl Fn(&[u8]) -> bool,
) -> Option<Vec<u8>> {
    let start = history.where_history().min(history.entries().len());
    // Preserve the public API's saved-current-edit side effect, but do not
    // trust its ASCII match: the locale matcher is authoritative per entry.
    history.search_prefix_backward_bytes_with_case(prefix, current, true);
    let found = history.entries()[..start]
        .iter()
        .enumerate()
        .rev()
        .find(|(_, entry)| matches(&entry.line_bytes))
        .map(|(index, entry)| (index, entry.line_bytes.clone()));
    if let Some((index, line)) = found {
        history.set_pos(index);
        Some(line)
    } else {
        history.set_pos(start);
        None
    }
}

pub(crate) fn history_prefix_forward_with_locale(
    history: &mut History,
    prefix: &[u8],
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<Vec<u8>> {
    if !ignore_case || locale_lowercase.is_none() {
        return history.search_prefix_forward_bytes_with_case(prefix, ignore_case);
    }
    history_prefix_forward_matching(history, |line| {
        search_prefix_matches(line, prefix, true, locale_lowercase)
    })
}

fn history_prefix_forward_matching(
    history: &mut History,
    matches: impl Fn(&[u8]) -> bool,
) -> Option<Vec<u8>> {
    let start = history.where_history();
    if start >= history.entries().len() {
        return None;
    }
    let found = history.entries()[start + 1..]
        .iter()
        .enumerate()
        .find(|(_, entry)| matches(&entry.line_bytes))
        .map(|(offset, entry)| (start + 1 + offset, entry.line_bytes.clone()));
    if let Some((index, line)) = found {
        history.set_pos(index);
        Some(line)
    } else if start + 1 >= history.entries().len() {
        // Past the newest match, return to the saved current edit; otherwise
        // preserve the current line (see Bash 5.3 PTY fixtures above).
        history.end_bytes()
    } else {
        None
    }
}

pub(crate) fn history_substring_forward_with_locale(
    history: &mut History,
    needle: &[u8],
    current: Vec<u8>,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<Vec<u8>> {
    if !ignore_case || locale_lowercase.is_none() {
        return history.search_containing_forward_from_cursor_bytes_with_case(
            needle,
            current,
            ignore_case,
        );
    }
    history_substring_forward_matching(history, needle, current, |line| {
        search_bytes_offset(line, needle, true, locale_lowercase, false).is_some()
    })
}

fn history_substring_forward_matching(
    history: &mut History,
    needle: &[u8],
    current: Vec<u8>,
    matches: impl Fn(&[u8]) -> bool,
) -> Option<Vec<u8>> {
    let start = history.where_history();
    // Keep current-edit bookkeeping while rejecting ASCII-only matches below.
    history.search_containing_forward_from_cursor_bytes_with_case(needle, current, true);
    if needle.is_empty() || start >= history.entries().len() {
        return None;
    }
    let current_matches = matches(&history.entries()[start].line_bytes);
    let first = start + usize::from(current_matches);
    let found = history.entries()[first..]
        .iter()
        .enumerate()
        .find(|(_, entry)| matches(&entry.line_bytes))
        .map(|(offset, entry)| (first + offset, entry.line_bytes.clone()));
    if let Some((index, line)) = found {
        history.set_pos(index);
        Some(line)
    } else {
        history.set_pos(start);
        None
    }
}

pub(crate) fn history_substring_backward_with_locale(
    history: &mut History,
    needle: &[u8],
    current: Vec<u8>,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<Vec<u8>> {
    if !ignore_case || locale_lowercase.is_none() {
        return history.search_containing_backward_from_cursor_bytes_with_case(
            needle,
            current,
            ignore_case,
        );
    }
    history_substring_backward_matching(history, needle, current, |line| {
        search_bytes_offset(line, needle, true, locale_lowercase, false).is_some()
    })
}

fn history_substring_backward_matching(
    history: &mut History,
    needle: &[u8],
    current: Vec<u8>,
    matches: impl Fn(&[u8]) -> bool,
) -> Option<Vec<u8>> {
    let start = history.where_history().min(history.entries().len());
    // Keep current-edit bookkeeping while rejecting ASCII-only matches below.
    history.search_containing_backward_from_cursor_bytes_with_case(needle, current, true);
    let found = if needle.is_empty() {
        None
    } else {
        history.entries()[..start]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, entry)| matches(&entry.line_bytes))
            .map(|(index, entry)| (index, entry.line_bytes.clone()))
    };
    if let Some((index, line)) = found {
        history.set_pos(index);
        Some(line)
    } else {
        history.set_pos(start);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf8_locale() -> Option<LocaleLowercase> {
        LocaleLowercase::new("C.UTF-8")
    }

    // Synthetic valid-UTF-8 matchers deliberately distinguish I from i.
    // These exercise locale rejection independently of installed locales; they
    // are not evidence about a Turkish runtime or oracle.
    fn identity_prefix(line: &[u8]) -> bool {
        std::str::from_utf8(line).unwrap().starts_with('i')
    }

    fn identity_substring(line: &[u8]) -> bool {
        std::str::from_utf8(line).unwrap().contains('i')
    }

    #[test]
    fn locale_prefix_rejection_restores_cursor_and_preserves_current_edit() {
        for start in [1, 2] {
            let mut history = History::new();
            history.push("I older");
            history.push("I newer");
            history.set_pos(start);
            assert_eq!(
                history_prefix_backward_matching(
                    &mut history,
                    b"i",
                    b"saved edit".to_vec(),
                    identity_prefix,
                ),
                None
            );
            assert_eq!(history.where_history(), start);
            if start == 2 {
                // The failed backward search must still save the current edit.
                // A failed forward prefix search preserves the current line
                // (Bash 5.3 PTY: forward past the newest match stays instead
                // of returning to the saved edit).
                history.set_pos(0);
                assert_eq!(
                    history_prefix_forward_matching(&mut history, identity_prefix),
                    None
                );
                assert_eq!(history.where_history(), 0);
                history.set_pos(2);
                assert_eq!(
                    history_prefix_forward_matching(&mut history, identity_prefix),
                    None
                );
            }
        }
    }

    #[test]
    fn locale_substring_rejection_restores_cursor_and_preserves_current_edit() {
        for start in [1, 2] {
            let mut history = History::new();
            history.push("x I older");
            history.push("x I newer");
            history.set_pos(start);
            assert_eq!(
                history_substring_backward_matching(
                    &mut history,
                    b"i",
                    b"saved edit".to_vec(),
                    identity_substring,
                ),
                None
            );
            assert_eq!(history.where_history(), start);
            if start == 2 {
                history.set_pos(0);
                assert_eq!(history.end_bytes(), Some(b"saved edit".to_vec()));
            }
        }

        let mut history = History::new();
        history.push("x I older");
        history.push("x I newer");
        assert_eq!(
            history_substring_forward_matching(
                &mut history,
                b"i",
                b"saved edit".to_vec(),
                identity_substring,
            ),
            None
        );
        assert_eq!(history.where_history(), 2);
        history.set_pos(0);
        assert_eq!(
            history_substring_forward_matching(
                &mut history,
                b"i",
                b"ignored edit".to_vec(),
                identity_substring,
            ),
            None
        );
        assert_eq!(history.where_history(), 0);
        assert_eq!(history.end_bytes(), Some(b"saved edit".to_vec()));
    }

    #[test]
    fn locale_matcher_skips_ascii_only_matches_in_both_directions() {
        let mut history = History::new();
        history.push("i older");
        history.push("I center");
        history.push("i newer");
        assert_eq!(
            history_prefix_backward_matching(&mut history, b"i", Vec::new(), identity_prefix),
            Some(b"i newer".to_vec())
        );
        assert_eq!(history.where_history(), 2);
        assert_eq!(
            history_substring_backward_matching(&mut history, b"i", Vec::new(), identity_substring),
            Some(b"i older".to_vec())
        );
        assert_eq!(history.where_history(), 0);
        assert_eq!(
            history_prefix_forward_matching(&mut history, identity_prefix),
            Some(b"i newer".to_vec())
        );
        assert_eq!(history.where_history(), 2);
        history.set_pos(0);
        assert_eq!(
            history_substring_forward_matching(&mut history, b"i", Vec::new(), identity_substring),
            Some(b"i newer".to_vec())
        );
        assert_eq!(history.where_history(), 2);
    }

    #[test]
    fn locale_wrappers_keep_per_input_ascii_fallback() {
        let locale = utf8_locale();
        for locale in [None, locale.as_ref()] {
            for (line, query) in [
                (b"I\xff".as_slice(), b"i".as_slice()),
                (b"I\xff".as_slice(), b"i\xff".as_slice()),
                (b"I".as_slice(), b"i".as_slice()),
            ] {
                let mut history = History::new();
                history.push_bytes(line.to_vec());
                history.push("unrelated");
                history.push_bytes(line.to_vec());
                assert_eq!(
                    history_prefix_backward_with_locale(
                        &mut history,
                        query,
                        Vec::new(),
                        true,
                        locale,
                    ),
                    Some(line.to_vec())
                );
                assert_eq!(history.where_history(), 2);
                assert_eq!(
                    history_substring_backward_with_locale(
                        &mut history,
                        query,
                        Vec::new(),
                        true,
                        locale,
                    ),
                    Some(line.to_vec())
                );
                assert_eq!(history.where_history(), 0);
                assert_eq!(
                    history_prefix_forward_with_locale(&mut history, query, true, locale),
                    Some(line.to_vec())
                );
                assert_eq!(history.where_history(), 2);
                history.set_pos(0);
                assert_eq!(
                    history_substring_forward_with_locale(
                        &mut history,
                        query,
                        Vec::new(),
                        true,
                        locale,
                    ),
                    Some(line.to_vec())
                );
                assert_eq!(history.where_history(), 2);
            }
        }
    }

    #[test]
    fn locale_matching_follows_the_measured_one_to_one_matrix() {
        let Some(locale) = utf8_locale() else {
            return;
        };
        for (uppercase, lowercase) in [("Ä", "ä"), ("Σ", "σ"), ("K", "k"), ("İ", "i"), ("ẞ", "ß")]
        {
            assert_eq!(
                search_bytes_offset(
                    uppercase.as_bytes(),
                    lowercase.as_bytes(),
                    true,
                    Some(&locale),
                    false,
                ),
                Some(0),
                "{uppercase}/{lowercase}"
            );
        }
        assert_eq!(
            search_bytes_offset("Σ".as_bytes(), "ς".as_bytes(), true, Some(&locale), false),
            None
        );
        assert_eq!(
            search_bytes_offset("ß".as_bytes(), b"ss", true, Some(&locale), false),
            None
        );
        assert_eq!(
            search_bytes_offset("ss".as_bytes(), "ß".as_bytes(), true, Some(&locale), false),
            None
        );
    }

    #[test]
    fn locale_search_returns_original_byte_offset_and_history_path_uses_it() {
        let Some(locale) = utf8_locale() else {
            return;
        };
        let line = "x ÄLPHA";
        assert_eq!(
            search_bytes_offset(
                line.as_bytes(),
                "älp".as_bytes(),
                true,
                Some(&locale),
                false
            ),
            Some(2)
        );

        let mut history = History::new();
        history.push(line);
        history.set_pos(1);
        let found = history_search_with_locale(
            &mut history,
            "älp".as_bytes(),
            HistoryDirection::Previous,
            true,
            Some(&locale),
        )
        .unwrap();
        assert_eq!(found.entry_index, 0);
        assert_eq!(found.byte_offset, 2);

        let mut prefix = History::new();
        prefix.push("ÄLPHA");
        assert_eq!(
            history_prefix_backward_with_locale(
                &mut prefix,
                "älp".as_bytes(),
                Vec::new(),
                true,
                Some(&locale),
            ),
            Some("ÄLPHA".as_bytes().to_vec())
        );

        let mut forward = History::new();
        forward.push("ÄLPHA old");
        forward.push("ÄLPHA new");
        forward.set_pos(0);
        assert_eq!(
            history_prefix_forward_with_locale(&mut forward, "älp".as_bytes(), true, Some(&locale),),
            Some("ÄLPHA new".as_bytes().to_vec())
        );
        forward.set_pos(0);
        assert_eq!(
            history_substring_forward_with_locale(
                &mut forward,
                "älp".as_bytes(),
                Vec::new(),
                true,
                Some(&locale),
            ),
            Some("ÄLPHA new".as_bytes().to_vec())
        );
        forward.set_pos(2);
        assert_eq!(
            history_substring_backward_with_locale(
                &mut forward,
                "älp".as_bytes(),
                Vec::new(),
                true,
                Some(&locale),
            ),
            Some("ÄLPHA new".as_bytes().to_vec())
        );
    }

    #[test]
    fn missing_or_non_utf8_locale_and_invalid_input_fall_back_to_ascii_bytes() {
        assert_eq!(
            search_bytes_offset(b"ALPHA", b"alpha", true, None, false),
            Some(0)
        );
        assert_eq!(
            search_bytes_offset("Ä".as_bytes(), "ä".as_bytes(), true, None, false),
            None
        );
        if let Some(c_locale) = LocaleLowercase::new("C") {
            assert_eq!(
                search_bytes_offset("Ä".as_bytes(), "ä".as_bytes(), true, Some(&c_locale), false),
                None
            );
            assert_eq!(
                search_bytes_offset(b"ALPHA", b"alpha", true, Some(&c_locale), false),
                Some(0)
            );
        }
        if let Some(locale) = utf8_locale() {
            assert_eq!(
                search_bytes_offset(b"\xff ALPHA", b"\xff alpha", true, Some(&locale), false),
                Some(0)
            );
            let invalid_haystack = [0xff, b' ', 0xc3, 0x84];
            let invalid_needle = [0xff, b' ', 0xc3, 0xa4];
            assert_eq!(
                search_bytes_offset(
                    &invalid_haystack,
                    &invalid_needle,
                    true,
                    Some(&locale),
                    false
                ),
                None
            );
        }
    }
}
