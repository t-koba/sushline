use crate::locale::LocaleLowercase;
use crate::width::visible_width;
use history::{History, HistoryDirection, HistorySearchMatch};

use super::{EditorState, UndoState};

#[derive(Debug, Default)]
pub(crate) struct SearchState {
    /// Original prefix for a consecutive run of history navigation searches.
    pub(crate) history_navigation_needle: Option<Vec<u8>>,
    pub(crate) reverse_search: Option<ReverseSearchState>,
    pub(crate) non_incremental_search: Option<NonIncrementalSearchState>,
    pub(crate) last_incremental_search: Option<Vec<u8>>,
    pub(crate) last_incremental_search_direction: Option<SearchDirection>,
    pub(crate) last_non_incremental_search: Option<Vec<u8>>,
    pub(crate) last_non_incremental_search_direction: Option<SearchDirection>,
    pub(crate) last_non_incremental_search_match_index: Option<usize>,
    pub(crate) quoted_pending: bool,
}

impl EditorState {
    pub(crate) fn history_navigation_needle(&mut self) -> Vec<u8> {
        self.search
            .history_navigation_needle
            .get_or_insert_with(|| self.buffer.range_bytes(0, self.buffer.point()))
            .clone()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ReverseSearchState {
    pub(crate) query: Vec<u8>,
    pub(crate) match_line: Option<Vec<u8>>,
    pub(crate) match_index: Option<usize>,
    /// Byte offset of the match inside `match_line`, so a repeat can look for
    /// another occurrence in the same entry before moving to another entry.
    pub(crate) match_offset: Option<usize>,
    /// The displayed match is the unsaved line the search started on, which
    /// is a virtual entry ahead of history and has no history index.
    pub(crate) unsaved_line_match: bool,
    pub(crate) direction: SearchDirection,
    pub(crate) original_line: Vec<u8>,
    pub(crate) original_point: usize,
    pub(crate) original_history_pos: usize,
    pub(crate) original_history_entry_index: Option<usize>,
    pub(crate) original_undo: UndoState,
    pub(crate) exclude_cursor: bool,
    /// NUL (set-mark) froze the query: later printable, quoted and control
    /// query-editing bytes are ignored.
    pub(crate) nul_pending: bool,
    /// The frozen search was marked failed, so ignored printable and quoted
    /// literal bytes ring the bell.
    pub(crate) nul_failed: bool,
    /// A multi-word Ctrl-W kept the query and a selected match; accepting a
    /// later failing query keeps the displayed line instead of restoring the
    /// pre-search line.
    pub(crate) retain_match_on_failure: bool,
    /// A query extension failed after a match was displayed. Enter then
    /// accepts that displayed line; Ctrl-G still restores the original line.
    pub(crate) retain_failed_query_line: bool,
    /// GNU resets `history_pos` to `last_found_line` on a failed query edit
    /// but leaves `sline` at the exhausted oldest line with `sline_index` 0.
    /// The next edit first re-checks the query at offset 0 of that stale line
    /// (keeping the retained match line), then searches below the retained
    /// index. This reproduces the extra SF_FAILED bell and the match selected
    /// when a failed extension is deleted.
    pub(crate) failed_search_line: Option<Vec<u8>>,
    /// A direction switch kept the displayed match (GNU case -2): the query
    /// re-matched at the unchanged sline_index, so the match is confirmed
    /// live and later direction switches keep it too. Cleared by a query edit
    /// or a re-search, which can establish a new forward-failed fallback.
    pub(crate) match_survived_switch: bool,
    /// The search began on a recalled entry whose edit was committed to
    /// history at entry; its first match still uses the displayed line and
    /// the point the user searched from.
    pub(crate) edited_entry_at_start: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct NonIncrementalSearchState {
    pub(crate) query: Vec<u8>,
    pub(crate) direction: SearchDirection,
    pub(crate) original_line: Vec<u8>,
    pub(crate) original_history_pos: usize,
    pub(crate) original_history_entry_index: Option<usize>,
    pub(crate) original_undo: UndoState,
    /// True for vi `/`/`?` (prompts `/`/`?`, cursor-exclusive backward
    /// search); false for emacs non-incremental search (prompt `:`).
    pub(crate) vi: bool,
}

/// Render search query bytes once, using the buffer's printable control
/// notation. The result is already in display domain: a genuine raw-byte
/// marker is escaped here and must not be escaped again by the caller. The
/// second value is the physical column advance a raw TAB and its cursor moves
/// add beyond the text's visible width.
pub(crate) fn render_search_query(query: &[u8], start_col: usize) -> (String, usize) {
    let (text, columns) = crate::buffer::display_search_query_bytes(query, start_col);
    let extra = columns.saturating_sub(visible_width(&text));
    (text, extra)
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

    pub(crate) fn prompt_text(&self) -> (String, usize) {
        let (query, extra) = render_search_query(&self.query, 1);
        (format!("{}{}", self.prompt_char(), query), extra)
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
    // newer match). Repeats first look for another occurrence of the query
    // in the displayed entry and only then step to an adjacent entry. A
    // repeat with no current match restarts from the cursor. A backward
    // search begun on an unsaved line sees that line ahead of history; a
    // forward search from the end of history also sees that virtual entry
    // (Bash 5.3 PTY oracle).
    let from_end = matches!(search.direction, SearchDirection::Backward);
    let stale_line = search.failed_search_line.clone();
    // A failed query edit keeps the reached index so the next extension
    // continues from the same match, even though its displayed line is
    // dropped for accept and point handling.
    let had_match = search.match_index.is_some() || search.unsaved_line_match;
    if !repeat {
        search.unsaved_line_match = false;
    }
    let unsaved_candidate = search.original_history_entry_index.is_none()
        && !search.exclude_cursor
        && !search.query.is_empty()
        && (from_end || search.original_history_pos >= history.entries().len());
    let repeated = if repeat && had_match {
        // An unsaved match keeps match_index None so history.set_pos never
        // sees an out-of-range index; the in-line helper only needs its
        // line and offset.
        if search.match_line.is_some() {
            repeated_match_offset(search, ignore_case, locale_lowercase).map(|offset| {
                (
                    search.match_index,
                    search.match_line.clone().unwrap(),
                    offset,
                )
            })
        } else {
            None
        }
    } else {
        None
    };
    // An unsaved line is a virtual entry: backward sees it ahead of history
    // with the last occurrence at or before the point; forward sees its first
    // occurrence at or after the point. Neither assigns a history index.
    if unsaved_candidate && !repeat {
        let point = search.original_point.min(search.original_line.len());
        let offset = if from_end {
            unsaved_initial_match_offset(
                &search.original_line,
                &search.query,
                search.original_point,
                ignore_case,
                locale_lowercase,
            )
        } else {
            search
                .original_line
                .get(point..)
                .and_then(|line| {
                    search_bytes_offset(line, &search.query, ignore_case, locale_lowercase, false)
                })
                .map(|delta| point + delta)
        };
        if let Some(offset) = offset {
            search.match_index = None;
            search.match_line = Some(search.original_line.clone());
            search.match_offset = Some(offset);
            search.unsaved_line_match = true;
            return true;
        }
    }
    // A committed edit still belongs to the entry the search started on, so
    // its first reverse match is chosen from the displayed line and the
    // original point, not from the last occurrence in the stored entry.
    let edited_start = search.edited_entry_at_start
        && !repeat
        && from_end
        && !search.exclude_cursor
        && !search.query.is_empty()
        && search.original_history_pos < history.entries().len();
    if edited_start
        && let Some(offset) = unsaved_initial_match_offset(
            &search.original_line,
            &search.query,
            search.original_point,
            ignore_case,
            locale_lowercase,
        )
    {
        search.match_index = Some(search.original_history_pos);
        search.match_line = Some(search.original_line.clone());
        search.match_offset = Some(offset);
        search.unsaved_line_match = false;
        search.failed_search_line = None;
        return true;
    }
    let mut in_line = None;
    let found: Option<(Option<usize>, Vec<u8>)> = if let Some((idx, line, offset)) = repeated {
        in_line = Some(offset);
        Some((idx, line))
    } else {
        match (search.direction, repeat, search.match_index) {
            (SearchDirection::Backward, true, _) if search.unsaved_line_match => {
                // No earlier in-line occurrence remains; a bounded `None`
                // steps from the newest history entry without leaving the
                // history range.
                search_history_backward(history, &search.query, None, ignore_case, locale_lowercase)
                    .map(|(idx, line)| (Some(idx), line))
            }
            (SearchDirection::Backward, true, Some(idx)) => search_history_backward(
                history,
                &search.query,
                Some(idx),
                ignore_case,
                locale_lowercase,
            )
            .map(|(idx, line)| (Some(idx), line)),
            (SearchDirection::Forward, true, Some(idx)) => search_history_forward(
                history,
                &search.query,
                Some(idx),
                ignore_case,
                locale_lowercase,
            )
            .map(|(idx, line)| (Some(idx), line)),
            (SearchDirection::Backward, _, _) => {
                // A failed edit leaves GNU's `sline` on the exhausted oldest
                // line with `sline_index` clamped to 0. The next edit first
                // checks the query at that line's offset 0; a hit there is
                // found at the retained `last_found_line`, so the displayed
                // and accepted line stays the retained match and no bell
                // rings. Only when this check fails does GNU step below the
                // retained index and eventually ding.
                if !repeat
                    && !search.query.is_empty()
                    && let (Some(stale), Some(idx)) = (&stale_line, search.match_index)
                    && search_bytes_offset(
                        stale,
                        &search.query,
                        ignore_case,
                        locale_lowercase,
                        false,
                    ) == Some(0)
                    && let Some(line) = history.entries().get(idx)
                {
                    // GNU finds the query on the stale line but then redisplays
                    // `lines[history_pos]`, the retained last-found line, and
                    // keeps that index. Restore the retained line and offset so
                    // accept and the retained-line Ctrl-Y bell still see a match.
                    search.match_line = Some(line.line_bytes.clone());
                    search.match_offset = Some(0);
                    search.unsaved_line_match = false;
                    search.failed_search_line = None;
                    return true;
                }
                // An ordinary query edit continues from the displayed match
                // instead of restarting at the cursor: after a repeat reached
                // an older entry, a longer query must not jump to a newer one.
                // Only a fresh search without a prior match uses the
                // cursor-based bound; query edits, including deletion after
                // a match, continue from the reached index.
                let end = if edited_start {
                    // No occurrence at or before the point in the displayed
                    // entry: continue below it instead of re-finding a later
                    // occurrence in the same entry.
                    search.original_history_pos.min(history.entries().len())
                } else if search.exclude_cursor {
                    search.original_history_pos.min(history.entries().len())
                } else {
                    let found_end = search
                        .match_index
                        .unwrap_or(search.original_history_pos)
                        .saturating_add(1);
                    // After a failed edit GNU re-searches strictly below the
                    // retained last-found index, not from the retained match
                    // itself. The stale line's offset 0 was already checked
                    // above; GNU never re-scans the retained entry at other
                    // offsets, so the bound may drop to zero. `None`, not
                    // `Some(0)`, is the full-range case in the search helper.
                    let end = if stale_line.is_some() && search.match_index.is_some() {
                        search.match_index.unwrap_or(0)
                    } else {
                        found_end
                    };
                    end.min(history.entries().len())
                };
                // GNU skips a short line only when moving to another entry.
                // Exempt the entry the search actually starts on: a displayed
                // match from a query edit, or a selected history entry. From
                // the unsaved line the newest history entry is moved-to, so
                // it stays subject to the skip.
                let history_len = history.entries().len();
                let start_exempt = search
                    .match_index
                    .or_else(|| {
                        (search.original_history_pos < history_len)
                            .then_some(search.original_history_pos)
                    })
                    .filter(|idx| *idx < history_len);
                search_history_backward_with_start(
                    history,
                    &search.query,
                    Some(end),
                    ignore_case,
                    locale_lowercase,
                    start_exempt,
                )
                .map(|(idx, line)| (Some(idx), line))
            }
            (SearchDirection::Forward, _, _) => {
                let start = if search.exclude_cursor {
                    search
                        .original_history_pos
                        .saturating_add(1)
                        .min(history.entries().len())
                } else if stale_line.is_some() && search.match_index.is_some() {
                    // After a failed forward extension GNU re-searches
                    // strictly above the retained last-found index. Unlike a
                    // backward scan, forward exhaustion leaves the stale line
                    // at the newest entry, so its offset 0 is not the
                    // synthetic index-0 line the backward branch re-probes.
                    search
                        .match_index
                        .unwrap_or(0)
                        .saturating_add(1)
                        .min(history.entries().len())
                } else {
                    search
                        .match_index
                        .unwrap_or(search.original_history_pos)
                        .min(history.entries().len())
                };
                let mut first = start;
                // The start-point skip belongs to the entry the search began
                // in; a query edit already resolved to a newer entry keeps
                // that entry as its first candidate.
                if !search.exclude_cursor && !repeat && start == search.original_history_pos {
                    // Readline 8.3's first forward search ignores occurrences
                    // behind the start point in the cursor entry. With none at
                    // or after that point it takes the first newer matching
                    // entry, falling back to the cursor entry only when no
                    // newer entry matches. Vi `?` and repeats keep their start.
                    let point = search.original_point.min(search.original_line.len());
                    if search.original_line.get(point..).is_some_and(|line| {
                        search_bytes_offset(
                            line,
                            &search.query,
                            ignore_case,
                            locale_lowercase,
                            false,
                        )
                        .is_none()
                    }) {
                        first = start.saturating_add(1);
                    }
                }
                // GNU skips a short line only when moving to another entry:
                // the starting/cursor entry stays reachable in both the
                // point-skipped and fallback scans.
                let found = search_history_forward_from_with_start(
                    history,
                    &search.query,
                    first,
                    ignore_case,
                    locale_lowercase,
                    Some(start),
                )
                .map(|(idx, line)| (Some(idx), line));
                if found.is_none() && first != start {
                    search_history_forward_from_with_start(
                        history,
                        &search.query,
                        start,
                        ignore_case,
                        locale_lowercase,
                        Some(start),
                    )
                    .map(|(idx, line)| (Some(idx), line))
                } else {
                    found
                }
            }
        }
    };
    if let Some((idx, line)) = found {
        let offset = in_line.unwrap_or_else(|| {
            search_bytes_offset(
                &line,
                &search.query,
                ignore_case,
                locale_lowercase,
                from_end,
            )
            .unwrap_or(0)
        });
        // A repeat that stays inside the unsaved line keeps match_index None
        // so history.set_pos never sees an out-of-range index.
        search.match_index = idx;
        search.match_line = Some(line);
        search.match_offset = Some(offset);
        search.unsaved_line_match = idx.is_none();
        search.failed_search_line = None;
        // Leaving the starting entry ends the edited-entry rule: later query
        // edits and failed extensions use the ordinary match rules.
        search.edited_entry_at_start = false;
        return true;
    }
    if !had_match {
        search.match_index = None;
    }
    if !had_match || !repeat {
        // A failed query edit drops the displayed line (so accept and point
        // follow the original line) but keeps the match index as the bound
        // for the next extension. GNU's Ctrl-Y also still extends the query
        // from the last successful match offset; every other consumer needs
        // both the offset and the displayed line, so retaining the offset is
        // otherwise inert.
        search.match_line = None;
        if !had_match {
            search.match_offset = None;
        }
        search.unsaved_line_match = false;
    }
    // GNU leaves `sline` at the line where the scan exhausted. A backward
    // scan over the history fails after examining the oldest entry, so that
    // is the stale line the next edit re-checks at offset 0.
    search.failed_search_line = history.entries().first().map(|e| e.line_bytes.clone());
    false
}

/// Initial offset for an unsaved line: the last occurrence starting at or
/// before the search point wins. When no occurrence starts there, the unsaved
/// line has no reverse match and the caller searches stored history instead.
fn unsaved_initial_match_offset(
    line: &[u8],
    query: &[u8],
    original_point: usize,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<usize> {
    if query.is_empty() {
        return None;
    }
    let point = original_point.min(line.len());
    match_offsets_from_end(line, query, ignore_case, locale_lowercase)
        .into_iter()
        .find(|offset| *offset <= point)
}

/// Find the next occurrence inside the currently displayed entry. A repeat
/// advances one byte from the current match, so occurrences may overlap: GNU
/// steps by one byte rather than by the query length. A failed query keeps
/// the displayed line and its offset so the next repeat tries again before
/// moving on.
fn repeated_match_offset(
    search: &ReverseSearchState,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<usize> {
    if search.query.is_empty() {
        return None;
    }
    let line = search.match_line.as_deref()?;
    let offset = search.match_offset?;
    if matches!(search.direction, SearchDirection::Backward) {
        // Include the query's last byte before `offset`, so an overlapping
        // occurrence ending at `offset` can still be selected. One pass over
        // candidate starts keeps a miss linear in the line length.
        let end = char_boundary_after(line, offset.saturating_add(search.query.len() - 1));
        last_match_start_before(
            line,
            &search.query,
            offset,
            end,
            ignore_case,
            locale_lowercase,
        )
    } else {
        let after = char_boundary_after(line, offset.saturating_add(1));
        search_bytes_offset(
            &line[after.min(line.len())..],
            &search.query,
            ignore_case,
            locale_lowercase,
            false,
        )
        .map(|delta| after + delta)
    }
}

/// Find the nearest earlier match whose whole extent lies inside `line[..end]`.
/// Scanning candidate starts once preserves overlapping matches even when
/// locale lowercase folds a multibyte query into fewer bytes.
fn last_match_start_before(
    line: &[u8],
    query: &[u8],
    before: usize,
    end: usize,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<usize> {
    if query.is_empty() || before == 0 || end == 0 {
        return None;
    }
    let end = end.min(line.len());
    if let Some(locale) = locale_lowercase
        && ignore_case
        && let (Ok(haystack_text), Ok(needle_text)) = (
            std::str::from_utf8(&line[..end]),
            std::str::from_utf8(query),
        )
    {
        return locale_match_offsets(haystack_text, needle_text, locale)
            .into_iter()
            .rev()
            .find(|&start| start < before);
    }
    if query.len() > end {
        return None;
    }
    (0..before.min(end - query.len() + 1)).rev().find(|&start| {
        let window = &line[start..start + query.len()];
        if ignore_case {
            window
                .iter()
                .zip(query)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        } else {
            window == query
        }
    })
}

/// Compare a query with byte stepping that matches the paired U+0130 search
/// observations: its two-byte query character can match `i` at alternating
/// ASCII byte offsets. This is a narrow measured case, not general Unicode
/// case-fold compatibility.
fn locale_match_offsets(
    haystack: &str,
    needle: &str,
    locale_lowercase: &LocaleLowercase,
) -> Vec<usize> {
    if needle.is_empty() {
        return vec![];
    }
    haystack
        .char_indices()
        .filter_map(|(start, _)| {
            locale_streqn_at(
                &haystack.as_bytes()[start..],
                needle.as_bytes(),
                locale_lowercase,
            )
            .then_some(start)
        })
        .collect()
}

fn locale_streqn_at(
    mut haystack: &[u8],
    mut needle: &[u8],
    locale_lowercase: &LocaleLowercase,
) -> bool {
    let mut remaining = needle.len();
    while remaining > 0 {
        let needle_char = first_utf8_character(needle);
        let haystack_char = first_utf8_character(haystack);
        let (Some(needle_char), Some(haystack_char)) = (needle_char, haystack_char) else {
            if haystack.first().map(u8::to_ascii_lowercase)
                != needle.first().map(u8::to_ascii_lowercase)
            {
                return false;
            }
            needle = &needle[1..];
            haystack = &haystack[1..];
            remaining -= 1;
            continue;
        };
        let width = needle_char.len_utf8();
        if locale_lowercase.lowercase(needle_char) != locale_lowercase.lowercase(haystack_char) {
            return false;
        }
        needle = &needle[width..];
        haystack = haystack.get(width..).unwrap_or(&[]);
        remaining -= width;
    }
    true
}

fn first_utf8_character(bytes: &[u8]) -> Option<char> {
    let prefix = &bytes[..bytes.len().min(4)];
    let valid_prefix = match std::str::from_utf8(prefix) {
        Ok(text) => text,
        Err(error) if error.valid_up_to() > 0 => {
            std::str::from_utf8(&prefix[..error.valid_up_to()]).ok()?
        }
        Err(_) => return None,
    };
    valid_prefix.chars().next()
}

/// Lift a byte index to a UTF-8 character boundary, so locale-aware matching
/// still sees whole characters when a one-byte step lands inside one.
fn char_boundary_after(line: &[u8], index: usize) -> usize {
    let Some(text) = std::str::from_utf8(line).ok() else {
        return index.min(line.len());
    };
    let mut boundary = index.min(line.len());
    while boundary < line.len() && !text.is_char_boundary(boundary) {
        boundary += 1;
    }
    boundary
}

/// Drop a byte index to a UTF-8 character boundary, so a prefix never ends
/// inside the last matched character.
fn char_boundary_before(line: &[u8], index: usize) -> usize {
    let Ok(text) = std::str::from_utf8(line) else {
        return index.min(line.len());
    };
    let mut boundary = index.min(line.len());
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

/// Fixture-scoped reachability rule for incremental locale-aware search: a
/// moved-to history line shorter than the first query character's UTF-8 width
/// is skipped (the Kelvin sign needs three bytes, dotted I needs two). This
/// does not define a general Unicode-folding algorithm.
fn incremental_history_line_reachable(
    line: &[u8],
    needle: &[u8],
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> bool {
    if !ignore_case || locale_lowercase.is_none() {
        return true;
    }
    let (Ok(_), Ok(_)) = (std::str::from_utf8(line), std::str::from_utf8(needle)) else {
        return true;
    };
    first_utf8_character(needle).is_none_or(|first| line.len() >= first.len_utf8())
}

pub(crate) fn search_history_backward(
    history: &History,
    needle: &[u8],
    before: Option<usize>,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<(usize, Vec<u8>)> {
    search_history_backward_with_start(history, needle, before, ignore_case, locale_lowercase, None)
}

pub(crate) fn search_history_backward_with_start(
    history: &History,
    needle: &[u8],
    before: Option<usize>,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
    start_index: Option<usize>,
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
        .find(|(idx, entry)| {
            (Some(*idx) == start_index
                || incremental_history_line_reachable(
                    &entry.line_bytes,
                    needle,
                    ignore_case,
                    locale_lowercase,
                ))
                && search_bytes_offset(
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
    search_history_forward_from_with_start(
        history,
        needle,
        start,
        ignore_case,
        locale_lowercase,
        None,
    )
}

pub(crate) fn search_history_forward_from_with_start(
    history: &History,
    needle: &[u8],
    start: usize,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
    start_index: Option<usize>,
) -> Option<(usize, Vec<u8>)> {
    if needle.is_empty() {
        return None;
    }
    let start = start.min(history.entries().len());
    history.entries()[start..]
        .iter()
        .enumerate()
        .find(|(offset, entry)| {
            (Some(start + *offset) == start_index
                || incremental_history_line_reachable(
                    &entry.line_bytes,
                    needle,
                    ignore_case,
                    locale_lowercase,
                ))
                && search_bytes_offset(
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

/// Match starts in `haystack`, last occurrence first. Each step searches a
/// shorter prefix through the shared case/locale-aware matcher; the prefix
/// ends on the character boundary at or before the current match's last byte,
/// which keeps overlapping occurrences without cutting a multibyte character.
pub(crate) fn match_offsets_from_end(
    haystack: &[u8],
    needle: &[u8],
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Vec<usize> {
    let mut offsets = Vec::new();
    if needle.is_empty() {
        return offsets;
    }
    let mut end = haystack.len();
    while let Some(found) = search_bytes_offset(
        &haystack[..end],
        needle,
        ignore_case,
        locale_lowercase,
        true,
    ) {
        offsets.push(found);
        let next = char_boundary_before(
            haystack,
            found.saturating_add(needle.len().saturating_sub(1)),
        );
        if next >= end {
            break;
        }
        end = next;
    }
    offsets
}

/// Zero for the last occurrence, one for the one before it, and so on.
pub(crate) fn match_rank_from_end(
    haystack: &[u8],
    needle: &[u8],
    offset: usize,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<usize> {
    match_offsets_from_end(haystack, needle, ignore_case, locale_lowercase)
        .iter()
        .position(|&candidate| candidate == offset)
}

pub(crate) fn match_offset_at_rank_from_end(
    haystack: &[u8],
    needle: &[u8],
    rank: usize,
    ignore_case: bool,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<usize> {
    match_offsets_from_end(haystack, needle, ignore_case, locale_lowercase)
        .into_iter()
        .nth(rank)
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
    // GNU passes `line_len - query_len + 1` as the first `mbrtowc` byte
    // budget. A budget of exactly zero compares the first query character
    // with NUL and fails; a negative budget wraps to a large `size_t` and
    // still lets later folded characters compare at their byte positions.
    // This only encodes that measured boundary, not a general fold rule.
    if needle.len() > haystack.len() && needle.len() - haystack.len() == 1 {
        return None;
    }
    if let Some(locale_lowercase) = locale_lowercase
        && ignore_case
        && let (Ok(haystack_text), Ok(needle_text)) =
            (std::str::from_utf8(haystack), std::str::from_utf8(needle))
    {
        let offsets = locale_match_offsets(haystack_text, needle_text, locale_lowercase);
        return if from_end {
            offsets.last().copied()
        } else {
            offsets.first().copied()
        };
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
    if start >= history.entries().len() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn utf8_locale() -> Option<LocaleLowercase> {
        LocaleLowercase::new("C.UTF-8")
    }

    fn repeated_state(line: &str, query: &str, offset: usize) -> ReverseSearchState {
        ReverseSearchState {
            query: query.as_bytes().to_vec(),
            match_line: Some(line.as_bytes().to_vec()),
            match_index: Some(0),
            match_offset: Some(offset),
            unsaved_line_match: false,
            direction: SearchDirection::Backward,
            original_line: Vec::new(),
            original_point: 0,
            original_history_pos: 0,
            original_history_entry_index: None,
            original_undo: UndoState::default(),
            exclude_cursor: false,
            nul_pending: false,
            nul_failed: false,
            retain_match_on_failure: false,
            retain_failed_query_line: false,
            failed_search_line: None,
            match_survived_switch: false,
            edited_entry_at_start: false,
        }
    }

    fn initial_forward_state(
        line: &str,
        query: &str,
        point: usize,
        exclude_cursor: bool,
    ) -> ReverseSearchState {
        ReverseSearchState {
            query: query.as_bytes().to_vec(),
            match_line: None,
            match_index: None,
            match_offset: None,
            unsaved_line_match: false,
            direction: SearchDirection::Forward,
            original_line: line.as_bytes().to_vec(),
            original_point: point,
            original_history_pos: 0,
            original_history_entry_index: None,
            original_undo: UndoState::default(),
            exclude_cursor,
            nul_pending: false,
            nul_failed: false,
            retain_match_on_failure: false,
            retain_failed_query_line: false,
            failed_search_line: None,
            match_survived_switch: false,
            edited_entry_at_start: false,
        }
    }

    #[test]
    fn initial_backward_search_matches_unsaved_line_before_history() {
        let mut history = History::new();
        history.push("abcabc");
        history.push("mid");
        history.push("xabc");
        let mut unsaved = ReverseSearchState {
            query: b"abc".to_vec(),
            original_line: b"abcabc".to_vec(),
            original_history_pos: history.entries().len(),
            ..Default::default()
        };
        assert!(update_reverse_search_match(
            &mut unsaved,
            &history,
            false,
            false,
            None
        ));
        assert_eq!(unsaved.match_index, None);
        assert_eq!(unsaved.match_line.as_deref(), Some(&b"abcabc"[..]));
        assert!(unsaved.unsaved_line_match);
        assert!(update_reverse_search_match(
            &mut unsaved,
            &history,
            true,
            false,
            None
        ));
        assert_eq!(unsaved.match_index, Some(2));
        assert_eq!(unsaved.match_line.as_deref(), Some(&b"xabc"[..]));
        assert!(!unsaved.unsaved_line_match);

        // An entry-owned line keeps the recalled history ordering.
        let mut recalled = ReverseSearchState {
            original_history_entry_index: Some(2),
            unsaved_line_match: false,
            ..unsaved
        };
        recalled.match_index = None;
        recalled.match_line = None;
        recalled.match_offset = None;
        assert!(update_reverse_search_match(
            &mut recalled,
            &history,
            false,
            false,
            None
        ));
        assert_eq!(recalled.match_index, Some(2));
    }

    #[test]
    fn initial_forward_search_skips_the_point_entry_without_a_later_occurrence() {
        let mut history = History::new();
        history.push("abcabc");
        history.push("mid");
        history.push("xxxabc");
        for (point, expected_index, expected_line) in [
            (3, Some(0), "abcabc"),
            (4, Some(2), "xxxabc"),
            (6, Some(2), "xxxabc"),
        ] {
            let mut search = initial_forward_state("abcabc", "abc", point, false);
            assert!(
                update_reverse_search_match(&mut search, &history, false, false, None),
                "point {point}"
            );
            assert_eq!(search.match_index, expected_index, "point {point}");
            assert_eq!(
                search.match_line.as_deref(),
                Some(expected_line.as_bytes()),
                "point {point}"
            );
        }

        // With no newer match the cursor entry is still searched from its
        // start, as GNU does when the point has no occurrence ahead of it.
        let mut shorter = History::new();
        shorter.push("abcabc");
        shorter.push("mid");
        let mut search = initial_forward_state("abcabc", "abc", 6, false);
        assert!(update_reverse_search_match(
            &mut search,
            &shorter,
            false,
            false,
            None
        ));
        assert_eq!(search.match_index, Some(0));
        assert_eq!(search.match_line.as_deref(), Some("abcabc".as_bytes()));

        // Vi `?` keeps its cursor-exclusive start regardless of the point.
        let mut search = initial_forward_state("abcabc", "abc", 3, true);
        assert!(update_reverse_search_match(
            &mut search,
            &history,
            false,
            false,
            None
        ));
        assert_eq!(search.match_index, Some(2));
    }

    #[test]
    fn repeated_match_offset_finds_overlaps_without_rescanning_prefixes() {
        let line = format!("{}Q", "x".repeat(8_000));
        let state = repeated_state(&line, "Q", 8_000);
        assert_eq!(repeated_match_offset(&state, false, None), None);

        for (line, query, offset, expected) in [
            ("aaaa", "aa", 2, Some(1)),
            ("aaaa", "aa", 1, Some(0)),
            ("aXaXa", "aXa", 2, Some(0)),
        ] {
            let state = repeated_state(line, query, offset);
            assert_eq!(repeated_match_offset(&state, false, None), expected);
        }

        let locale = utf8_locale().expect("C.UTF-8 locale");
        for (line, query, offset, expected) in
            [("iii", "İİ", 1, Some(0)), ("iiii", "İİ", 2, Some(1))]
        {
            let state = repeated_state(line, query, offset);
            assert_eq!(repeated_match_offset(&state, true, Some(&locale)), expected);
        }
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
        history.reset_cursor();
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
        history.reset_cursor();
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
                history.reset_cursor();
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
        prefix.reset_cursor();
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

    #[test]
    fn match_offsets_from_end_keeps_overlapping_and_multibyte_occurrences() {
        let no_locale = None;
        assert_eq!(
            match_offsets_from_end(b"aaaa", b"aa", false, no_locale),
            vec![2, 1, 0]
        );
        assert_eq!(
            match_offsets_from_end("äää".as_bytes(), "ä".as_bytes(), false, no_locale),
            vec![4, 2, 0]
        );
        assert_eq!(
            match_offsets_from_end("äxäxäxZ".as_bytes(), "ä".as_bytes(), false, no_locale),
            vec![6, 3, 0]
        );
    }
}
