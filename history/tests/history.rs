use std::fs;

use history::expansion::{
    HistoryChars, HistoryExpansionError, HistoryExpansionPolicy, HistoryExpansionState,
    HistoryExpansionStatus, expand_history, expand_history_with_state, expand_history_with_status,
    expand_history_with_status_with_state, get_history_event, history_arg_extract,
    history_tokenize,
};
use history::{
    History, HistoryDirection, HistoryEntry, HistoryMultiline, HistoryNulPolicy,
    HistoryReadOptions, HistoryUndoEntry,
};

#[test]
fn navigates_history_and_restores_current_edit() {
    let mut h = History::new();
    h.push("one");
    h.push("two");
    h.reset_cursor();
    assert_eq!(
        h.navigate_bytes(HistoryDirection::Previous, b"draft".to_vec()),
        Some(b"two".to_vec())
    );
    assert_eq!(
        h.navigate_bytes(HistoryDirection::Previous, b"two".to_vec()),
        Some(b"one".to_vec())
    );
    assert_eq!(
        h.navigate_bytes(HistoryDirection::Next, b"one".to_vec()),
        Some(b"two".to_vec())
    );
    assert_eq!(
        h.navigate_bytes(HistoryDirection::Next, b"two".to_vec()),
        Some(b"draft".to_vec())
    );
}

#[test]
fn fresh_appends_preserve_position_zero() {
    for bytes in [false, true] {
        let mut history = History::new();
        for (index, line) in ["alpha", "beta", "alpha gamma", "delta"].iter().enumerate() {
            if bytes {
                history.push_bytes(line.as_bytes().to_vec());
            } else {
                history.push(*line);
            }
            assert_eq!(history.len(), index + 1);
            assert_eq!(history.where_history(), 0);
            assert_eq!(history.current_history().unwrap().line_bytes, b"alpha");
        }
        assert!(history.previous_history().is_none());
        assert_eq!(history.where_history(), 0);
    }
}

#[test]
fn set_pos_accepts_end_and_preserves_position_on_invalid_target() {
    // The pinned GNU 8.3 fixture tests each target from a fresh history.
    for start in 0..=3 {
        for target in [3, 4] {
            let mut history = History::new();
            for line in ["A", "B", "C"] {
                history.push(line);
            }
            assert!(history.set_pos(start));
            assert_eq!(history.set_pos(target), target == 3);
            let expected = if target == 3 { 3 } else { start };
            assert_eq!(history.where_history(), expected);
            assert_eq!(
                history
                    .current_history()
                    .map(|entry| entry.line_bytes.as_slice()),
                [Some(&b"A"[..]), Some(&b"B"[..]), Some(&b"C"[..]), None][expected]
            );
        }
    }

    let mut empty = History::new();
    empty.push("A");
    empty.clear();
    assert!(empty.set_pos(0));
    assert!(!empty.set_pos(1));
    assert_eq!(empty.where_history(), 0);
    assert!(empty.current_history().is_none());
}

#[test]
fn previous_and_next_match_gnu_boundary_positions() {
    // Each call starts in a fresh A/B/C history, as in the pinned GNU 8.3 fixture.
    for (previous, expected) in [
        (
            true,
            [(None, 0), (Some("A"), 0), (Some("B"), 1), (Some("C"), 2)],
        ),
        (
            false,
            [(Some("B"), 1), (Some("C"), 2), (None, 3), (None, 3)],
        ),
    ] {
        for (start, (entry, position)) in expected.into_iter().enumerate() {
            let mut history = History::new();
            for line in ["A", "B", "C"] {
                history.push(line);
            }
            assert!(history.set_pos(start));
            let actual = if previous {
                history.previous_history()
            } else {
                history.next_history()
            };
            assert_eq!(
                actual.map(|entry| entry.line_bytes.as_slice()),
                entry.map(str::as_bytes),
                "previous={previous} start={start}"
            );
            assert_eq!(
                history.where_history(),
                position,
                "previous={previous} start={start}"
            );
        }
    }
}

#[test]
fn removal_preserves_numeric_position() {
    for position in 0..=4 {
        for index in 0..4 {
            let mut history = History::new();
            for line in ["A", "B", "C", "D"] {
                history.push(line);
            }
            assert!(history.set_pos(position));
            let mut expected = vec!["A", "B", "C", "D"];
            let removed = expected.remove(index);
            assert_eq!(
                history.remove(index).unwrap().line_bytes,
                removed.as_bytes()
            );
            assert_eq!(history.where_history(), position);
            assert_eq!(
                history
                    .entries()
                    .iter()
                    .map(|entry| entry.line_bytes.as_slice())
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|line| line.as_bytes())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                history
                    .current_history()
                    .map(|entry| entry.line_bytes.as_slice()),
                expected.get(position).map(|line| line.as_bytes())
            );
            assert!(history.remove(3).is_none());
            assert_eq!(history.where_history(), position);
        }
    }

    for (position, index, remaining) in [(1, 0, "D"), (2, 0, "D"), (4, 0, "D"), (2, 1, "C")] {
        let mut history = History::new();
        for line in ["A", "B", "C", "D"] {
            history.push(line);
        }
        assert!(history.set_pos(position));
        history.stifle(2);
        assert!(history.remove(index).is_some());
        assert_eq!(history.where_history(), position);
        assert_eq!(history.len(), 1);
        assert_eq!(history.entries()[0].line_bytes, remaining.as_bytes());
        assert!(history.current_history().is_none());
    }
}

#[test]
fn adding_history_preserves_explicit_middle_and_old_end_positions() {
    for bytes in [false, true] {
        for position in [1, 3] {
            let mut history = History::new();
            for line in ["alpha", "beta", "alpha gamma"] {
                history.push(line);
            }
            assert!(history.set_pos(position));
            if bytes {
                history.push_bytes(b"delta".to_vec());
            } else {
                history.push("delta");
            }
            assert_eq!(history.len(), 4);
            assert_eq!(history.where_history(), position);
            let current = history.current_history().unwrap();
            assert_eq!(
                current.line_bytes,
                if position == 1 {
                    &b"beta"[..]
                } else {
                    &b"delta"[..]
                }
            );
            let previous = history.previous_history().unwrap();
            assert_eq!(
                previous.line_bytes,
                if position == 1 {
                    &b"alpha"[..]
                } else {
                    &b"alpha gamma"[..]
                }
            );
            assert_eq!(history.where_history(), position - 1);
        }
    }
}

#[test]
fn stifled_raw_appends_preserve_numeric_positions() {
    for bytes in [false, true] {
        let append = |history: &mut History, line: &str| {
            if bytes {
                history.push_bytes(line.as_bytes().to_vec());
            } else {
                history.push(line);
            }
        };
        for limit in [0, 1, 2] {
            let mut history = History::new();
            history.stifle(limit);
            for (index, line) in ["alpha", "beta", "gamma"].iter().enumerate() {
                append(&mut history, line);
                assert_eq!(history.where_history(), 0);
                assert_eq!(history.len(), (index + 1).min(limit));
                let expected_current = match limit {
                    0 => None,
                    1 => Some(line.as_bytes()),
                    2 => Some(["alpha", "beta", "gamma"][index.saturating_sub(1)].as_bytes()),
                    _ => unreachable!(),
                };
                assert_eq!(
                    history
                        .current_history()
                        .map(|entry| entry.line_bytes.as_slice()),
                    expected_current
                );
                assert!(history.previous_history().is_none());
            }
        }

        let mut history = History::new();
        for line in ["alpha", "beta", "gamma"] {
            history.push(line);
        }
        history.stifle(1);
        assert_eq!(history.entries()[0].line_bytes, b"gamma");
        assert!(history.set_pos(1));
        append(&mut history, "delta");
        assert_eq!(history.where_history(), 1);
        assert_eq!(history.entries()[0].line_bytes, b"delta");
        assert!(history.current_history().is_none());
        assert_eq!(history.previous_history().unwrap().line_bytes, b"delta");
        assert_eq!(history.where_history(), 0);

        for position in 0..=2 {
            let mut history = History::new();
            for line in ["alpha", "beta", "gamma"] {
                history.push(line);
            }
            history.stifle(2);
            assert_eq!(
                history
                    .entries()
                    .iter()
                    .map(|entry| entry.line_bytes.as_slice())
                    .collect::<Vec<_>>(),
                vec![&b"beta"[..], &b"gamma"[..]]
            );
            assert!(history.set_pos(position));
            append(&mut history, "delta");
            assert_eq!(
                history
                    .entries()
                    .iter()
                    .map(|entry| entry.line_bytes.as_slice())
                    .collect::<Vec<_>>(),
                vec![&b"gamma"[..], &b"delta"[..]]
            );
            assert_eq!(history.where_history(), position);
            assert_eq!(
                history
                    .current_history()
                    .map(|entry| entry.line_bytes.as_slice()),
                [Some(&b"gamma"[..]), Some(&b"delta"[..]), None][position]
            );
            assert_eq!(
                history
                    .previous_history()
                    .map(|entry| entry.line_bytes.as_slice()),
                [None, Some(&b"gamma"[..]), Some(&b"delta"[..])][position]
            );
            assert_eq!(history.where_history(), position.saturating_sub(1));
        }

        // Stifled pushes also discard the saved edit, not just the cursor.
        let mut history = History::new();
        history.stifle(2);
        append(&mut history, "old");
        history.reset_cursor();
        assert_eq!(
            history.navigate_bytes(HistoryDirection::Previous, b"draft".to_vec()),
            Some(b"old".to_vec())
        );
        append(&mut history, "new");
        assert_eq!(
            history.navigate_bytes(HistoryDirection::Next, b"old".to_vec()),
            Some(b"new".to_vec())
        );
        assert_eq!(
            history.navigate_bytes(HistoryDirection::Next, b"new".to_vec()),
            Some(Vec::new())
        );
    }
}

#[test]
fn history_expansion_absolute_numbers_follow_stifled_append_base() {
    // Pinned GNU History 8.3's `stifle-after-add` and `stifle-before-add`
    // fixtures distinguish resident ordinals from absolute event numbers once
    // appending while capped advances history_base.
    let policy = HistoryExpansionPolicy::default();
    let expand = |history: &History, line: &[u8]| {
        expand_history(line, history, HistoryChars::parse("!^#"), &policy, |_| {
            false
        })
    };

    let mut stifled_after_add = History::new();
    for line in ["echo alpha", "echo bravo", "echo charlie"] {
        stifled_after_add.push(line);
    }
    stifled_after_add.stifle(2);
    assert_eq!(expand(&stifled_after_add, b"!1").unwrap(), b"echo bravo");
    assert_eq!(expand(&stifled_after_add, b"!2").unwrap(), b"echo charlie");
    assert!(expand(&stifled_after_add, b"!3").is_err());
    stifled_after_add.push("echo delta");
    assert!(expand(&stifled_after_add, b"!1").is_err());
    assert_eq!(expand(&stifled_after_add, b"!2").unwrap(), b"echo charlie");
    assert_eq!(expand(&stifled_after_add, b"!3").unwrap(), b"echo delta");
    assert!(expand(&stifled_after_add, b"!4").is_err());
    assert_eq!(expand(&stifled_after_add, b"!-1").unwrap(), b"echo delta");
    assert_eq!(expand(&stifled_after_add, b"!-2").unwrap(), b"echo charlie");
    assert_eq!(
        stifled_after_add.get_1_based_entry(1).unwrap().line_bytes,
        b"echo charlie"
    );

    let mut stifled_before_add = History::new();
    stifled_before_add.stifle(2);
    for line in ["echo alpha", "echo bravo", "echo charlie"] {
        stifled_before_add.push(line);
    }
    assert!(expand(&stifled_before_add, b"!1").is_err());
    assert_eq!(expand(&stifled_before_add, b"!2").unwrap(), b"echo bravo");
    assert_eq!(expand(&stifled_before_add, b"!3").unwrap(), b"echo charlie");
    assert_eq!(
        expand(&stifled_before_add, b"!-1").unwrap(),
        b"echo charlie"
    );
    assert_eq!(expand(&stifled_before_add, b"!-2").unwrap(), b"echo bravo");
    stifled_before_add.push("echo delta");
    assert!(expand(&stifled_before_add, b"!1").is_err());
    assert!(expand(&stifled_before_add, b"!2").is_err());
    assert_eq!(expand(&stifled_before_add, b"!3").unwrap(), b"echo charlie");
    assert_eq!(expand(&stifled_before_add, b"!4").unwrap(), b"echo delta");
    assert_eq!(expand(&stifled_before_add, b"!-1").unwrap(), b"echo delta");
    assert_eq!(
        expand(&stifled_before_add, b"!-2").unwrap(),
        b"echo charlie"
    );

    stifled_before_add.clear();
    stifled_before_add.push("echo fresh");
    assert_eq!(expand(&stifled_before_add, b"!1").unwrap(), b"echo fresh");
}

#[test]
fn append_clears_current_edit_and_preserves_import_cursor() {
    let mut history = History::new();
    history.push("old");
    history.reset_cursor();
    assert_eq!(
        history.navigate_bytes(HistoryDirection::Previous, b"draft".to_vec()),
        Some(b"old".to_vec())
    );
    history.push_bytes(b"new".to_vec());
    assert_eq!(history.where_history(), 0);
    assert_eq!(
        history.navigate_bytes(HistoryDirection::Next, b"old".to_vec()),
        Some(b"new".to_vec())
    );
    assert_eq!(
        history.navigate_bytes(HistoryDirection::Next, b"new".to_vec()),
        Some(Vec::new())
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "imported\n").unwrap();
    let mut history = History::new();
    history.push("old");
    history.load_file(&path, None).unwrap();
    assert_eq!(history.where_history(), 0);
    assert_eq!(history.current_history().unwrap().line(), "old");
}

#[test]
fn forward_history_search_includes_current_position() {
    // Like GNU `history_search`, the positioned entry is included in both
    // directions; repeat commands step past it at their own call sites.
    let mut h = History::new();
    h.push("needle one");
    h.push("needle two");
    h.set_pos(0);

    let found = h
        .history_search_bytes(b"needle", HistoryDirection::Next)
        .expect("next match");
    assert_eq!(found.entry_index, 0);
    assert_eq!(found.line_bytes, b"needle one");
}

#[test]
fn history_search_offsets_match_pinned_single_entry_fixture() {
    // Mirrors history-search-offset.sh's primary/fresh-replica GNU 8.3 fixture.
    let mut h = History::new();
    let line = b"echo alpha-first alpha-last";
    h.push_bytes(line.to_vec());
    assert_eq!(h.len(), 1);
    assert_eq!(line.len(), 27);

    for (direction, offset) in [
        (HistoryDirection::Previous, 17),
        (HistoryDirection::Next, 5),
    ] {
        assert!(h.set_pos(0));
        assert_eq!(h.where_history(), 0);
        let found = h.history_search_bytes(b"alpha", direction).expect("match");
        assert_eq!(found.byte_offset, offset);
        assert_eq!(found.entry_index, 0);
        assert_eq!(found.line_bytes, line);
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.current_history().unwrap().line_bytes, line);
    }
}

#[test]
fn failed_nonempty_history_search_matches_pinned_single_entry_fixture() {
    // GNU 8.3 returns -1 in both directions and preserves this cursor and entry.
    let mut h = History::new();
    let line = b"echo alpha-first alpha-last";
    h.push_bytes(line.to_vec());
    assert_eq!(h.len(), 1);
    assert_eq!(line.len(), 27);

    for direction in [HistoryDirection::Previous, HistoryDirection::Next] {
        assert!(h.set_pos(0));
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.history_search_bytes(b"missing", direction), None);
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.current_history().unwrap().line_bytes, line);
    }
}

#[test]
fn history_search_is_case_sensitive_in_pinned_single_entry_fixture() {
    // GNU 8.3 does not match uppercase ALPHA against lowercase history text.
    let mut h = History::new();
    let line = b"echo alpha-first alpha-last";
    h.push_bytes(line.to_vec());

    for direction in [HistoryDirection::Previous, HistoryDirection::Next] {
        assert!(h.set_pos(0));
        assert_eq!(h.history_search_bytes(b"ALPHA", direction), None);
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.current_history().unwrap().line_bytes, line);
    }
}

#[test]
fn history_search_offsets_match_pinned_utf8_single_entry_fixture() {
    // Mirrors history-search-offset.sh's primary/fresh-replica GNU 8.3 fixture.
    let mut h = History::new();
    let line = b"\xce\xbb alpha \xce\xbb alpha";
    h.push_bytes(line.to_vec());
    assert_eq!(h.len(), 1);
    assert_eq!(line.len(), 17);

    for (direction, offset) in [
        (HistoryDirection::Previous, 12),
        (HistoryDirection::Next, 3),
    ] {
        assert!(h.set_pos(0));
        assert_eq!(h.where_history(), 0);
        let found = h.history_search_bytes(b"alpha", direction).expect("match");
        assert_eq!(found.byte_offset, offset);
        assert_eq!(found.entry_index, 0);
        assert_eq!(found.line_bytes, line);
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.current_history().unwrap().line_bytes, line);
    }
}

#[test]
fn empty_history_search_matches_pinned_single_entry_fixture() {
    // GNU 8.3 returns -1 in both directions for this positioned-entry fixture.
    let mut h = History::new();
    let line = b"echo alpha-first alpha-last";
    h.push_bytes(line.to_vec());
    assert_eq!(h.len(), 1);
    assert_eq!(line.len(), 27);

    for direction in [HistoryDirection::Previous, HistoryDirection::Next] {
        assert!(h.set_pos(0));
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.history_search_bytes(b"", direction), None);
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.current_history().unwrap().line_bytes, line);
    }
}

#[test]
fn history_search_nul_query_uses_full_byte_slice_in_single_entry_fixture() {
    // Rust byte-slice behavior, not the GNU C-string result in the NUL-query probe.
    let mut h = History::new();
    let line = b"echo alpha-first alpha-last";
    h.push_bytes(line.to_vec());
    assert_eq!(h.len(), 1);
    assert_eq!(line.len(), 27);

    for direction in [HistoryDirection::Previous, HistoryDirection::Next] {
        assert!(h.set_pos(0));
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.history_search_bytes(b"alpha\0suffix", direction), None);
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.current_history().unwrap().line_bytes, line);
    }
}

#[test]
fn history_search_includes_positioned_entry_with_directional_offsets() {
    // GNU `history_search` fixtures: the positioned entry is included in
    // both directions, forward offsets are the first occurrence, and
    // backward offsets are the last occurrence in the line.
    let mut h = History::new();
    h.push("echo alpha-first alpha-last");
    h.push("echo second");

    // End position 2, backward alpha: entry 0, last-occurrence offset 17.
    let backward = h
        .history_search_bytes(b"alpha", HistoryDirection::Previous)
        .expect("backward match");
    assert_eq!(backward.entry_index, 0);
    assert_eq!(backward.byte_offset, 17);
    assert_eq!(backward.line_bytes, b"echo alpha-first alpha-last");

    // Position 0, forward alpha: entry 0, first-occurrence offset 5.
    h.set_pos(0);
    let forward = h
        .history_search_bytes(b"alpha", HistoryDirection::Next)
        .expect("forward match");
    assert_eq!(forward.entry_index, 0);
    assert_eq!(forward.byte_offset, 5);

    // Position 1, forward second: entry 1, offset 5.
    h.set_pos(1);
    let ahead = h
        .history_search_bytes(b"second", HistoryDirection::Next)
        .expect("forward match");
    assert_eq!(ahead.entry_index, 1);
    assert_eq!(ahead.byte_offset, 5);

    // Position 0, backward alpha: entry 0, last-occurrence offset 17.
    h.set_pos(0);
    let behind = h
        .history_search_bytes(b"alpha", HistoryDirection::Previous)
        .expect("backward match");
    assert_eq!(behind.entry_index, 0);
    assert_eq!(behind.byte_offset, 17);
}

#[test]
fn history_search_traversal_matches_pinned_multi_entry_fixture() {
    // Mirrors the primary and fresh-replica GNU 8.3 fixture in
    // history-search-offset.sh: the middle entry does not match, so each
    // direction finds the nearest matching entry and moves the cursor there.
    let mut history = History::new();
    let lines: [&[u8]; 3] = [b"echo needle first", b"middle", b"echo needle last"];
    for line in lines {
        history.push_bytes(line.to_vec());
    }
    assert_eq!(history.len(), 3);

    for (direction, entry_index, line) in [
        (HistoryDirection::Previous, 0, lines[0]),
        (HistoryDirection::Next, 2, lines[2]),
    ] {
        assert!(history.set_pos(1));
        assert_eq!(history.where_history(), 1);
        let found = history
            .history_search_bytes(b"needle", direction)
            .expect("match in the requested direction");
        assert_eq!(found.entry_index, entry_index);
        assert_eq!(found.byte_offset, 5);
        assert_eq!(found.line_bytes, line);
        assert_eq!(history.where_history(), entry_index);
        assert_eq!(history.current_history().unwrap().line_bytes, line);
    }
}

#[test]
fn failed_multi_entry_history_search_matches_pinned_fixture() {
    // Mirrors the pinned GNU 8.3 failed-search rows for the same three-entry
    // fixture: both directions return no match and preserve position 1 on `middle`.
    let mut history = History::new();
    let lines: [&[u8]; 3] = [b"echo needle first", b"middle", b"echo needle last"];
    for line in lines {
        history.push_bytes(line.to_vec());
    }
    assert_eq!(history.len(), 3);
    assert_eq!(lines[1].len(), 6);

    for direction in [HistoryDirection::Previous, HistoryDirection::Next] {
        assert!(history.set_pos(1));
        assert_eq!(history.where_history(), 1);
        assert_eq!(history.history_search_bytes(b"missing", direction), None);
        assert_eq!(history.where_history(), 1);
        assert_eq!(history.current_history().unwrap().line_bytes, lines[1]);
    }
}

#[test]
fn history_search_current_match_matches_pinned_multi_entry_fixture() {
    // Mirrors the primary and fresh-replica GNU 8.3 current-match fixture in
    // history-search-offset.sh: all entries match, so both directions retain
    // the explicitly positioned middle entry and return its byte offset.
    let mut history = History::new();
    let lines: [&[u8]; 3] = [
        b"a needle before",
        b"echo needle current",
        b"longer needle after",
    ];
    for line in lines {
        history.push_bytes(line.to_vec());
    }
    assert_eq!(history.len(), 3);

    for direction in [HistoryDirection::Previous, HistoryDirection::Next] {
        assert!(history.set_pos(1));
        assert_eq!(history.where_history(), 1);
        let found = history
            .history_search_bytes(b"needle", direction)
            .expect("the positioned entry matches");
        assert_eq!(found.entry_index, 1);
        assert_eq!(found.byte_offset, 5);
        assert_eq!(found.line_bytes, lines[1]);
        assert_eq!(history.where_history(), 1);
        assert_eq!(history.current_history().unwrap().line_bytes, lines[1]);
    }
}

#[test]
fn history_search_matches_pinned_invalid_byte_needle_fixture() {
    // Mirrors the primary and fresh-replica GNU 8.3 raw-byte fixture in
    // history-search-offset.sh: offsets count bytes even for a non-UTF-8 needle.
    let mut history = History::new();
    let line: &[u8] = b"pre\xffx\xfftail";
    history.push_bytes(line.to_vec());
    assert_eq!(history.len(), 1);

    for (direction, offset) in [(HistoryDirection::Previous, 5), (HistoryDirection::Next, 3)] {
        assert!(history.set_pos(0));
        assert_eq!(history.where_history(), 0);
        let found = history
            .history_search_bytes(&[0xff], direction)
            .expect("raw-byte needle match");
        assert_eq!(found.entry_index, 0);
        assert_eq!(found.byte_offset, offset);
        assert_eq!(found.line_bytes, line);
        assert_eq!(history.where_history(), 0);
        assert_eq!(history.current_history().unwrap().line_bytes, line);
    }
}

#[test]
fn history_search_ignore_case_folds_ascii_bytes_only() {
    let mut h = History::new();
    h.push("ÄLPHA");

    assert!(
        h.history_search_bytes_with_case("älpha".as_bytes(), HistoryDirection::Previous, true)
            .is_none()
    );

    h.push("ALPHA");
    h.reset_cursor();
    let found = h
        .history_search_bytes_with_case(b"alpha", HistoryDirection::Previous, true)
        .expect("ASCII case-insensitive search");
    assert_eq!(found.entry_index, 1);
    assert_eq!(found.byte_offset, 0);
}

#[test]
fn history_search_pos_preserves_in_range_cursor_for_pinned_cases() {
    // Mirrors the pinned GNU 8.3 history-position-search-pos fixture.
    let mut h = History::new();
    h.push("needle first");
    h.push("middle");
    h.push("needle last");
    h.set_pos(1);

    let cases = [
        (
            "pos0_previous",
            "needle",
            HistoryDirection::Previous,
            0,
            Some(0),
        ),
        ("pos0_next", "needle", HistoryDirection::Next, 0, Some(0)),
        (
            "pos3_previous",
            "needle",
            HistoryDirection::Previous,
            3,
            Some(2),
        ),
        ("pos3_next_miss", "needle", HistoryDirection::Next, 3, None),
        (
            "pos99_previous_fallback",
            "needle",
            HistoryDirection::Previous,
            99,
            Some(0),
        ),
        (
            "pos99_next_fallback",
            "needle",
            HistoryDirection::Next,
            99,
            Some(2),
        ),
        (
            "pos99_previous_miss",
            "missing",
            HistoryDirection::Previous,
            99,
            None,
        ),
    ];
    for (name, needle, direction, pos, expected) in cases {
        assert_eq!(h.where_history(), 1, "{name}: before");
        assert_eq!(
            h.history_search_pos(needle, direction, pos)
                .map(|found| found.entry_index),
            expected,
            "{name}: result"
        );
        assert_eq!(h.where_history(), 1, "{name}: after");
    }
}

#[test]
fn history_search_pos_matches_beginning_and_end_boundaries() {
    // GNU Readline 8.3 histsearch.c starts at the requested position: position
    // zero is searchable in both directions; one-past-end searches the newest
    // entry backward and has no forward entries. history_search_pos restores
    // the caller's cursor after either result.
    let mut h = History::new();
    h.push("needle first");
    h.push("middle");
    h.push("needle last");
    h.set_pos(1);

    assert_eq!(
        h.history_search_pos("needle", HistoryDirection::Previous, 0)
            .map(|found| found.entry_index),
        Some(0)
    );
    assert_eq!(
        h.history_search_pos("needle", HistoryDirection::Next, 0)
            .map(|found| found.entry_index),
        Some(0)
    );
    assert_eq!(
        h.history_search_pos("needle", HistoryDirection::Previous, 3)
            .map(|found| found.entry_index),
        Some(2)
    );
    assert!(
        h.history_search_pos("needle", HistoryDirection::Next, 3)
            .is_none()
    );
    assert_eq!(
        h.where_history(),
        1,
        "positional search must not move the cursor"
    );
}

#[test]
fn history_search_pos_beyond_end_falls_back_to_cursor() {
    // GNU History's `history_search_pos` ignores a failed `history_set_pos`,
    // leaving the search anchored at the pre-existing cursor.
    let mut h = History::new();
    h.push("needle first");
    h.push("middle");
    h.push("needle last");
    h.set_pos(1);

    assert_eq!(
        h.history_search_pos("needle", HistoryDirection::Previous, 99)
            .map(|found| found.entry_index),
        Some(0)
    );
    assert_eq!(
        h.history_search_pos("needle", HistoryDirection::Next, 99)
            .map(|found| found.entry_index),
        Some(2)
    );
    assert!(
        h.history_search_pos("missing", HistoryDirection::Next, usize::MAX)
            .is_none()
    );
    assert_eq!(h.where_history(), 1);

    h.set_pos(3);
    assert_eq!(
        h.history_search_pos("needle", HistoryDirection::Previous, 99)
            .map(|found| found.entry_index),
        Some(2)
    );
    assert!(
        h.history_search_pos("needle", HistoryDirection::Next, 99)
            .is_none()
    );
    assert_eq!(h.where_history(), 3);
}

#[test]
fn history_search_prefix_matches_beginning_and_end_boundaries() {
    // GNU Readline 8.3 histsearch.c implements anchored prefix search through
    // the same position-inclusive history search as substring search.
    let mut h = History::new();
    h.push("alpha first");
    h.push("middle");
    h.push("alpha last");

    h.set_pos(0);
    assert_eq!(
        h.history_search_prefix("alpha", HistoryDirection::Previous)
            .map(|found| found.entry_index),
        Some(0)
    );
    h.set_pos(0);
    assert_eq!(
        h.history_search_prefix("alpha", HistoryDirection::Next)
            .map(|found| found.entry_index),
        Some(0)
    );
    h.set_pos(3);
    assert_eq!(
        h.history_search_prefix("alpha", HistoryDirection::Previous)
            .map(|found| found.entry_index),
        Some(2)
    );
    h.set_pos(3);
    assert!(
        h.history_search_prefix("alpha", HistoryDirection::Next)
            .is_none()
    );
    assert_eq!(
        h.where_history(),
        3,
        "a failed forward prefix search leaves the end cursor unchanged"
    );

    h.set_pos(1);
    assert!(
        h.history_search_prefix("missing", HistoryDirection::Previous)
            .is_none()
    );
    assert_eq!(
        h.where_history(),
        1,
        "a failed prefix search preserves the cursor"
    );
}

#[test]
fn searches_history_by_prefix() {
    let mut h = History::new();
    h.push("alpha one");
    h.push("beta");
    h.push("alpha two");
    h.reset_cursor();
    assert_eq!(
        h.search_prefix_backward_bytes(b"alp", b"alp".to_vec()),
        Some(b"alpha two".to_vec())
    );
    assert_eq!(
        h.search_prefix_backward_bytes(b"alp", b"alpha two".to_vec()),
        Some(b"alpha one".to_vec())
    );
    assert_eq!(
        h.search_prefix_forward_bytes(b"alp"),
        Some(b"alpha two".to_vec())
    );
    assert_eq!(h.search_prefix_forward_bytes(b"alp"), Some(b"alp".to_vec()));
}

#[test]
fn case_insensitive_byte_search_variants_fold_ascii_only() {
    let mut h = History::new();
    h.push("ALPHA old");
    h.push("beta");
    h.push("ALPHA new");
    h.reset_cursor();

    assert_eq!(
        h.search_prefix_backward_bytes_with_case(b"alpha", b"alpha".to_vec(), true),
        Some(b"ALPHA new".to_vec())
    );
    assert_eq!(
        h.search_prefix_backward_bytes_with_case(b"alpha", b"alpha".to_vec(), true),
        Some(b"ALPHA old".to_vec())
    );
    assert_eq!(
        h.search_prefix_forward_bytes_with_case(b"alpha", true),
        Some(b"ALPHA new".to_vec())
    );

    let mut substring = History::new();
    substring.push("ALPHA old");
    substring.push("beta");
    substring.push("ALPHA new");
    substring.reset_cursor();
    assert_eq!(
        substring.search_containing_backward_from_cursor_bytes_with_case(
            b"alpha",
            b"alpha".to_vec(),
            true,
        ),
        Some(b"ALPHA new".to_vec())
    );
    assert_eq!(
        substring.search_containing_backward_from_cursor_bytes_with_case(
            b"alpha",
            b"alpha".to_vec(),
            true,
        ),
        Some(b"ALPHA old".to_vec())
    );

    let mut forward = History::new();
    forward.push("alpha old");
    forward.push("ALPHA new");
    forward.set_pos(0);
    assert_eq!(
        forward.search_containing_forward_from_cursor_bytes_with_case(
            b"alpha",
            b"alpha old".to_vec(),
            true,
        ),
        Some(b"ALPHA new".to_vec())
    );

    let mut non_ascii = History::new();
    non_ascii.push("ÄLPHA old");
    assert!(
        non_ascii
            .search_prefix_backward_bytes_with_case("älpha".as_bytes(), Vec::new(), true)
            .is_none()
    );
}

#[test]
fn forward_substring_search_at_end_preserves_current_edit_and_cursor() {
    let mut h = History::new();
    h.push("needle old");
    h.push("other");
    h.push("needle new");
    h.reset_cursor();

    assert!(
        h.search_containing_forward_from_cursor_bytes(b"needle", b"needle".to_vec())
            .is_none()
    );
    assert_eq!(
        h.where_history(),
        3,
        "forward search at history end must not wrap"
    );
    assert_eq!(
        h.navigate_bytes(HistoryDirection::Previous, b"needle".to_vec()),
        Some(b"needle new".to_vec())
    );
    assert_eq!(
        h.navigate_bytes(HistoryDirection::Next, b"needle new".to_vec()),
        Some(b"needle".to_vec()),
        "navigating back to the end must restore the current edit"
    );
}

#[test]
fn searches_history_by_substring() {
    let mut h = History::new();
    h.push("alpha one");
    h.push("beta");
    h.push("alpha two");
    assert_eq!(
        h.search_containing_backward_index_bytes(b"two", None),
        Some((2, b"alpha two".to_vec()))
    );
    assert_eq!(
        h.search_containing_backward_index_bytes(b"alp", None),
        Some((2, b"alpha two".to_vec()))
    );
}

#[test]
fn forward_index_substring_search_handles_after_boundaries_without_overflow() {
    let mut h = History::new();
    h.push("needle first");
    h.push("middle");
    h.push("needle last");

    assert_eq!(
        h.search_containing_forward_index_bytes(b"needle", None),
        Some((0, b"needle first".to_vec()))
    );
    assert_eq!(
        h.search_containing_forward_index_bytes(b"needle", Some(0)),
        Some((2, b"needle last".to_vec()))
    );
    assert!(
        h.search_containing_forward_index_bytes(b"needle", Some(2))
            .is_none()
    );
    assert!(
        h.search_containing_forward_index_bytes(b"needle", Some(3))
            .is_none()
    );
    assert!(
        h.search_containing_forward_index_bytes(b"needle", Some(usize::MAX))
            .is_none()
    );
    assert!(
        h.search_containing_forward_index_bytes(b"", Some(usize::MAX))
            .is_none()
    );
}

#[test]
fn reads_writes_appends_and_truncates_history_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");

    let mut h = History::new();
    h.push("one");
    h.push("two");
    h.write_file(&path).unwrap();

    let mut loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| entry.line())
            .collect::<Vec<_>>(),
        vec!["one", "two"]
    );

    loaded.push("three");
    loaded.append_new_to_file(&path).unwrap();
    History::truncate_file(&path, 2).unwrap();

    let truncated = History::read_file(&path).unwrap();
    assert_eq!(
        truncated
            .entries()
            .iter()
            .map(|entry| entry.line())
            .collect::<Vec<_>>(),
        vec!["two", "three"]
    );

    let appended_last = dir.path().join("append-last");
    loaded.append_last_to_file(&appended_last, 2).unwrap();
    assert_eq!(fs::read_to_string(appended_last).unwrap(), "two\nthree\n");
}

#[test]
fn truncate_file_preserves_noop_bytes_and_truncates_when_needed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let cases: &[(&[u8], usize, &[u8])] = &[
        (b"", 0, b""),
        (b"unterminated", 0, b"unterminated"),
        (b"unterminated", usize::MAX, b"unterminated"),
        (b"A\nB\nC", usize::MAX, b"A\nB\nC"),
        (b"A\n\nB\n", 10, b"A\n\nB\n"),
        (b"A\n\nB\n", 1, b"B\n"),
        (b"A\nB", 10, b"A\nB"),
        (b"A\nB", 1, b"A\nB"),
        (b"A\nB", 0, b""),
        (b"A\nB\nC", 1, b"B\nC"),
        (b"A\nB\nC", 2, b"A\nB\nC"),
    ];

    for &(input, max_len, expected) in cases {
        fs::write(&path, input).unwrap();
        History::truncate_file(&path, max_len).unwrap();
        let output = fs::read(&path).unwrap();
        assert_eq!(
            output.as_slice(),
            expected,
            "fixture {input:?}, max_len {max_len}"
        );
    }
}

#[test]
fn truncate_file_streams_across_buffers_with_exact_suffixes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let mut inputs = vec![vec![b'\n'; 20_000], vec![0xff; 40_000]];
    // Put LF and CRLF boundaries on either side of the 64 KiB scan buffer,
    // including a CRLF split across buffers; retain a long non-text tail too.
    for prefix_len in (64 * 1024 - 2)..=(64 * 1024 + 2) {
        let mut input = vec![b'x'; prefix_len];
        input.extend_from_slice(b"\r\n#1700000000\r\n\ncommand\r\n");
        input.extend(vec![0xff; 40_000]);
        input.extend_from_slice(b"\0unterminated\r");
        inputs.push(input);
    }
    for input in inputs {
        let lf_count = input.iter().filter(|&&byte| byte == b'\n').count();
        for limit in [0, 1, 2, 3, lf_count, lf_count + 1, usize::MAX] {
            // The original in-memory definition is the oracle, independent
            // of scan buffer sizes or the length of the unterminated tail.
            let start = if lf_count <= limit {
                0
            } else if limit == 0 {
                input.len()
            } else {
                input
                    .iter()
                    .enumerate()
                    .filter(|&(_, byte)| *byte == b'\n')
                    .nth(lf_count - limit - 1)
                    .unwrap()
                    .0
                    + 1
            };
            fs::write(&path, &input).unwrap();
            History::truncate_file(&path, limit).unwrap();
            assert_eq!(fs::read(&path).unwrap(), input[start..], "limit {limit}");
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}

#[test]
fn truncate_crlf_history_preserves_measured_gnu_bytes() {
    // Truncation is raw-byte based, unlike CRLF-normalizing history imports.
    let cases: &[(&[u8], &[&[u8]])] = &[
        (
            b"A0\r\nA1\r\nB0\r\n",
            &[b"", b"B0\r\n", b"A1\r\nB0\r\n", b"A0\r\nA1\r\nB0\r\n"],
        ),
        (
            b"#1700000000\r\nA0\r\nA1\r\n#1700000001\r\nB0\r\nB1\r\n",
            &[
                b"",
                b"B1\r\n",
                b"B0\r\nB1\r\n",
                b"#1700000001\r\nB0\r\nB1\r\n",
                b"A1\r\n#1700000001\r\nB0\r\nB1\r\n",
                b"A0\r\nA1\r\n#1700000001\r\nB0\r\nB1\r\n",
                b"#1700000000\r\nA0\r\nA1\r\n#1700000001\r\nB0\r\nB1\r\n",
            ],
        ),
    ];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    for &(input, expected_by_limit) in cases {
        for (limit, &expected) in expected_by_limit.iter().enumerate() {
            fs::write(&path, input).unwrap();
            History::truncate_file(&path, limit).unwrap();
            assert_eq!(
                fs::read(&path).unwrap(),
                expected,
                "fixture {input:?}, limit {limit}"
            );
        }
    }
}

#[test]
fn appends_to_unterminated_history_file_tails() {
    // GNU History 8.3 append_history counts the last N entries; these APIs
    // select the same suffix by its starting index.
    type Fixture = (&'static [u8], bool, [&'static [u8]; 3]);
    let fixtures: &[Fixture] = &[
        (
            b"old-tail",
            false,
            [b"old-tail", b"old-tailnew2\n", b"old-tailnew1\nnew2\n"],
        ),
        (
            b"#1700000000\nold-tail",
            true,
            [
                b"#1700000000\nold-tail",
                b"#1700000000\nold-tail#1700000002\nnew2\n",
                b"#1700000000\nold-tail#1700000001\nnew1\n#1700000002\nnew2\n",
            ],
        ),
    ];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let mut history = History::new();
    history.push("new1");
    history.add_time("#1700000001");
    history.push("new2");
    history.add_time("#1700000002");
    for &(source, timestamps, expected) in fixtures {
        for (count, bytes) in expected.iter().enumerate() {
            fs::write(&path, source).unwrap();
            if timestamps {
                history
                    .append_file_with_timestamps(&path, 2 - count, true)
                    .unwrap();
            } else {
                history.append_file(&path, 2 - count).unwrap();
            }
            assert_eq!(
                fs::read(&path).unwrap(),
                *bytes,
                "timestamps={timestamps} count={count}"
            );
        }
    }
}

#[test]
fn oversized_append_history_count_matches_pinned_fixture() {
    // GNU History 8.3 append_history(3) selects both available entries and
    // does not separate the first appended line from this unterminated tail.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, b"old-tail").unwrap();

    let mut history = History::new();
    history.push("new1");
    history.push("new2");
    history.append_last_to_file(&path, 3).unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"old-tailnew1\nnew2\n");
}

#[test]
fn failed_full_write_cleans_up_sibling_tmp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::create_dir(&path).unwrap();

    let mut h = History::new();
    h.push("one");
    assert!(h.write_file(&path).is_err());
    assert!(
        !dir.path().join("history.tmp").exists(),
        "failed write must not leak sibling tmp"
    );
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .all(|entry| {
                !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("history.tmp.")
            }),
        "failed write must not leak unique sibling tmp"
    );
}

#[test]
fn append_file_creates_missing_path_with_or_without_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut history = History::new();
    history.push("alpha");
    for (from, expected) in [(0, &b"alpha\n"[..]), (1, &b""[..])] {
        let path = dir.path().join(format!("history-{from}"));
        assert!(!path.exists());
        history.append_file(&path, from).unwrap();
        assert_eq!(fs::read(&path).unwrap(), expected);
    }
}

#[test]
#[cfg(unix)]
fn append_file_preserves_existing_wider_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, b"old\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let mut history = History::new();
    history.push("new");
    history.append_file(&path, 0).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"old\nnew\n");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[test]
#[cfg(unix)]
fn append_file_follows_relative_symlink_to_existing_target() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("history-target");
    let link = dir.path().join("history-link");
    fs::write(&target, b"old\n").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    symlink("history-target", &link).unwrap();

    let mut history = History::new();
    history.push("new");
    history.append_file(&link, 0).unwrap();

    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(&link).unwrap(),
        std::path::Path::new("history-target")
    );
    assert_eq!(fs::read(&target).unwrap(), b"old\nnew\n");
    assert_eq!(fs::read(&link).unwrap(), b"old\nnew\n");
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[test]
#[cfg_attr(not(unix), ignore = "requires Unix file modes")]
fn full_write_creates_restrictive_file_mode_for_missing_path() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let mut h = History::new();
    h.push("two");
    h.write_file(&path).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "two\n");
    let append_only = dir.path().join("append-only");
    let mut a = History::new();
    a.push("appended secret");
    a.append_file(&append_only, 0).unwrap();
    assert_eq!(
        fs::metadata(&append_only).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .all(|entry| {
                !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("history.tmp.")
            }),
        "successful write must not leak sibling tmp"
    );
}

#[test]
#[cfg_attr(not(unix), ignore = "requires Unix file modes")]
fn full_write_preserves_restrictive_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let mut h = History::new();
    h.push("one");
    h.write_file(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut rewritten = History::new();
    rewritten.push("two");
    rewritten.write_file(&path).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "two\n");
    History::truncate_file(&path, 1).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
#[cfg_attr(not(unix), ignore = "requires Unix file modes")]
fn full_write_resets_wider_existing_file_mode_to_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "one\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let mut h = History::new();
    h.push("two");
    h.write_file(&path).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600,
        "full write resets wider pre-existing mode to owner-only 0600 (pinned GNU 8.3 fixture)"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "two\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    History::truncate_file(&path, 1).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
#[cfg(unix)]
fn truncate_file_sets_owner_only_mode_for_measured_noop_and_rewrite() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    for (limit, expected) in [(2, &b"A\nB\n"[..]), (1, &b"B\n"[..])] {
        fs::write(&path, b"A\nB\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        History::truncate_file(&path, limit).unwrap();
        assert_eq!(fs::read(&path).unwrap(), expected);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn load_file_limits_entries_and_append_new_tracks_loaded_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");

    let mut initial = History::new();
    initial.push("one");
    initial.push("two");
    initial.push("three");
    initial.write_file(&path).unwrap();

    let mut loaded = History::new();
    loaded.load_file(&path, Some(2)).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| entry.line())
            .collect::<Vec<_>>(),
        vec!["two", "three"]
    );

    loaded.push("four");
    loaded.append_new_to_file(&path).unwrap();
    let appended = History::read_file(&path).unwrap();
    assert_eq!(
        appended
            .entries()
            .iter()
            .map(|entry| entry.line())
            .collect::<Vec<_>>(),
        vec!["one", "two", "three", "four"]
    );
}

#[test]
fn push_preserves_adjacent_duplicates() {
    let mut h = History::new();
    h.push("same");
    h.push("same");
    assert_eq!(h.entries().len(), 2);
}

#[test]
fn history_files_write_readline_compatible_raw_multiline_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let mut h = History::new();
    h.push("printf 'one\ntwo'");
    h.write_file(&path).unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"printf 'one\ntwo'\n");
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| entry.line())
            .collect::<Vec<_>>(),
        vec!["printf 'one", "two'"]
    );
}

#[test]
fn history_files_join_timestamp_delimited_multiline_and_preserve_blanks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(
        &path,
        "#1700000000\nline one\n\nline two\n#1700000001\nnext\n",
    )
    .unwrap();

    // The default grouping is one entry per physical line: the first line
    // after a timestamp record carries the timestamp, the lines after it
    // carry none, and a blank line carries no entry.
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (Some("#1700000000"), "line one".to_string()),
            (None, "line two".to_string()),
            (Some("#1700000001"), "next".to_string()),
        ]
    );

    // The joined grouping makes the physical lines of a timestamp block one
    // entry and keeps the blank line inside it.
    let joined = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (Some("#1700000000"), "line one\n\nline two".to_string()),
            (Some("#1700000001"), "next".to_string()),
        ]
    );

    // Round-trip preserves the delimited byte layout.
    let reread = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(reread.entries().len(), 2);
    assert_eq!(reread.entries()[0].line_bytes, b"line one\n\nline two");

    // Default per-line grouping drops blanks in plain files.
    fs::write(&path, "line one\n\nline two\nnext\n").unwrap();
    let plain = History::read_file(&path).unwrap();
    assert_eq!(
        plain
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec![
            "line one".to_string(),
            "line two".to_string(),
            "next".to_string()
        ]
    );
    // Joined grouping preserves the selected plain physical lines.
    let plain_joined = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(
        plain_joined
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["line one\n\nline two\nnext".to_string()]
    );
}

#[test]
fn history_files_group_physical_lines_by_multiline_policy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#123\none\ntwo\n#456\nthree\nfour\n").unwrap();

    // The default grouping is one entry per physical line, GNU's default
    // `history_multiline_entries` mode: the first line of a timestamp block
    // carries the timestamp, the lines after it carry none.
    let per_line = History::read_file(&path).unwrap();
    assert_eq!(
        per_line
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (Some("#123"), "one".to_string()),
            (None, "two".to_string()),
            (Some("#456"), "three".to_string()),
            (None, "four".to_string()),
        ]
    );

    // The joined grouping is GNU's `history_multiline_entries` mode 1: one
    // entry per timestamp block.
    let joined = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (Some("#123"), "one\ntwo".to_string()),
            (Some("#456"), "three\nfour".to_string()),
        ]
    );
}

#[test]
fn history_multiline_joined_joins_plain_range_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "C0\nC1\nC2\nC3\n").unwrap();

    let joined =
        History::read_file_range_with_multiline(&path, 0, Some(3), HistoryMultiline::Joined)
            .unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["C0\nC1\nC2".to_string()]
    );
}

#[test]
fn load_file_variants_take_the_multiline_policy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#123\none\ntwo\n").unwrap();

    let mut loaded = History::new();
    loaded.load_file(&path, None).unwrap();
    assert_eq!(loaded.entries().len(), 2);
    assert_eq!(loaded.entries()[0].timestamp.as_deref(), Some("#123"));

    let mut joined = History::new();
    joined
        .load_file_with_multiline(&path, None, HistoryMultiline::Joined)
        .unwrap();
    assert_eq!(joined.entries().len(), 1);
    assert_eq!(joined.entries()[0].line_bytes, b"one\ntwo");

    let mut ranged = History::new();
    ranged
        .load_file_range_with_multiline(&path, 1, Some(2), None, HistoryMultiline::Joined)
        .unwrap();
    assert_eq!(ranged.entries().len(), 1);
    assert_eq!(ranged.entries()[0].line_bytes, b"two");
}

#[test]
fn joined_timestamped_import_with_one_entry_cap_keeps_last_group() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, b"#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n").unwrap();

    // Pinned GNU History 8.3 primary/replica retain this exact second group.
    let mut loaded = History::new();
    loaded
        .load_file_with_multiline(&path, Some(1), HistoryMultiline::Joined)
        .unwrap();
    assert_eq!(loaded.entries().len(), 1);
    assert_eq!(loaded.entries()[0].line_bytes, b"B0\nB1");
}

#[test]
fn plain_joined_full_file_import_matches_pinned_eof_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");

    // Pinned GNU History 8.3 primary/replica agree on both exact EOF fixtures.
    for (input, expected) in [
        (b"A\nB\nunterminated".as_slice(), b"A\nB".as_slice()),
        (
            b"A\nB\nunterminated\n".as_slice(),
            b"A\nB\nunterminated".as_slice(),
        ),
    ] {
        fs::write(&path, input).unwrap();
        let loaded = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
        assert_eq!(loaded.entries().len(), 1);
        assert_eq!(loaded.entries()[0].line_bytes, expected);
    }
}

#[test]
fn plain_joined_loads_selected_physical_lines_and_handles_empty_ranges() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, b"\nA\n\nB\n\nunterminated").unwrap();

    let mut loaded = History::new();
    loaded
        .load_file_with_multiline(&path, None, HistoryMultiline::Joined)
        .unwrap();
    assert_eq!(loaded.entries().len(), 1);
    assert_eq!(loaded.entries()[0].line_bytes, b"\nA\n\nB\n");
    assert_eq!(loaded.entries()[0].timestamp, None);

    let mut ranged = History::new();
    ranged
        .load_file_range_with_multiline(&path, 2, Some(5), None, HistoryMultiline::Joined)
        .unwrap();
    assert_eq!(ranged.entries().len(), 1);
    assert_eq!(ranged.entries()[0].line_bytes, b"\nB\n");
    assert!(
        History::read_file_range_with_multiline(&path, 5, None, HistoryMultiline::Joined)
            .unwrap()
            .entries()
            .is_empty()
    );
    fs::write(&path, b"").unwrap();
    assert!(
        History::read_file_with_multiline(&path, HistoryMultiline::Joined)
            .unwrap()
            .entries()
            .is_empty()
    );
}

#[test]
fn history_files_preserve_non_utf8_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let mut h = History::new();
    h.push_bytes(vec![b'a', 0xff, b'\n', b'b']);
    h.write_file(&path).unwrap();

    let loaded = History::read_file(&path).unwrap();
    assert_eq!(loaded.entries()[0].line_bytes, vec![b'a', 0xff]);
    assert_eq!(
        fs::read(&path).unwrap(),
        vec![b'a', 0xff, b'\n', b'b', b'\n']
    );
}

#[test]
fn history_files_preserve_embedded_nul_bytes() {
    // The pinned GNU History 8.3 direct-library fixtures truncate each tested
    // command at its first NUL, skip a leading-NUL command, and read later
    // lines. The default Rust reader preserves the raw file bytes.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");

    std::fs::write(&path, b"one\0two\nthree\n").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| entry.line_bytes.clone())
            .collect::<Vec<_>>(),
        vec![b"one\0two".to_vec(), b"three".to_vec()]
    );

    std::fs::write(&path, b"\0hidden\nnext\n").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| entry.line_bytes.clone())
            .collect::<Vec<_>>(),
        vec![b"\0hidden".to_vec(), b"next".to_vec()]
    );

    std::fs::write(&path, b"#123\nalpha\0tail\nbeta\n#456\nlast\n").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(loaded.entries().len(), 3);
    assert_eq!(loaded.entries()[0].timestamp.as_deref(), Some("#123"));
    assert_eq!(loaded.entries()[0].line_bytes, b"alpha\0tail");
    assert_eq!(loaded.entries()[1].timestamp.as_deref(), None);
    assert_eq!(loaded.entries()[1].line_bytes, b"beta");
    assert_eq!(loaded.entries()[2].timestamp.as_deref(), Some("#456"));
    assert_eq!(loaded.entries()[2].line_bytes, b"last");

    // The joined grouping keeps the physical lines of the block together.
    let joined = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(joined.entries().len(), 2);
    assert_eq!(joined.entries()[0].timestamp.as_deref(), Some("#123"));
    assert_eq!(joined.entries()[0].line_bytes, b"alpha\0tail\nbeta");
    assert_eq!(joined.entries()[1].timestamp.as_deref(), Some("#456"));
    assert_eq!(joined.entries()[1].line_bytes, b"last");

    // Retain timestamp metadata while exposing the pinned GNU 8.3 text view;
    // command-line NUL bytes remain covered separately above.
    std::fs::write(&path, b"#1700000000\0suffix\nalpha\nbeta\n").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(loaded.entries().len(), 2);
    assert_eq!(
        loaded.entries()[0].timestamp.as_deref(),
        Some("#1700000000\0suffix")
    );
    assert_eq!(
        loaded.entries()[0].gnu_timestamp_text(),
        Some("#1700000000")
    );
    assert_eq!(loaded.entries()[0].timestamp_seconds(), Some(1_700_000_000));
    assert_eq!(loaded.entries()[0].line_bytes, b"alpha");
    assert_eq!(loaded.entries()[1].timestamp.as_deref(), None);
    assert_eq!(loaded.entries()[1].line_bytes, b"beta");

    loaded.write_file_with_timestamps(&path, true).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"#1700000000\0suffix\nalpha\nbeta\n"
    );
    let roundtrip = History::read_file(&path).unwrap();
    assert_eq!(roundtrip.entries().len(), 2);
    assert_eq!(
        roundtrip.entries()[0].timestamp.as_deref(),
        Some("#1700000000\0suffix")
    );
    assert_eq!(roundtrip.entries()[0].line_bytes, b"alpha");
    assert_eq!(roundtrip.entries()[1].timestamp.as_deref(), None);
    assert_eq!(roundtrip.entries()[1].line_bytes, b"beta");
}

#[test]
fn explicit_nul_truncation_matches_pinned_full_file_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let options = HistoryReadOptions {
        nul_policy: HistoryNulPolicy::TruncateAtNul,
        ..Default::default()
    };
    assert_eq!(
        HistoryReadOptions::default().multiline,
        HistoryMultiline::PerLine
    );
    assert_eq!(
        HistoryReadOptions::default().nul_policy,
        HistoryNulPolicy::Preserve
    );
    // Pinned GNU History 8.3 full-file command and timestamp NUL fixtures.
    type ExpectedEntry<'a> = (&'a [u8], Option<&'a str>);
    let fixtures: &[(&[u8], &[ExpectedEntry<'_>])] = &[
        (
            b"one\0hidden\nnext\0tail\nfinal\n",
            &[(b"one", None), (b"next", None), (b"final", None)],
        ),
        (b"\0hidden\nnext\n", &[(b"next", None)]),
        (
            b"#123\nalpha\0tail\nbeta\n#456\nlast\n",
            &[
                (b"alpha", Some("#123")),
                (b"beta", None),
                (b"last", Some("#456")),
            ],
        ),
        (
            b"#1700000000\0suffix\nalpha\nbeta\n",
            &[(b"alpha", Some("#1700000000")), (b"beta", None)],
        ),
    ];
    for &(bytes, expected) in fixtures {
        fs::write(&path, bytes).unwrap();
        let loaded = History::read_file_with_options(&path, options).unwrap();
        assert_eq!(loaded.entries().len(), expected.len());
        for (entry, &(line, timestamp)) in loaded.entries().iter().zip(expected) {
            assert_eq!(entry.line_bytes, line);
            assert_eq!(entry.timestamp.as_deref(), timestamp);
        }
        let preserved = History::read_file(&path).unwrap();
        assert_eq!(
            preserved.entries(),
            History::read_file_with_options(&path, HistoryReadOptions::default())
                .unwrap()
                .entries()
        );
        // Writing remains independent: stored command and timestamp NUL bytes
        // from a preserving read are emitted unchanged.
        preserved.write_file_with_timestamps(&path, true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    // Additional product cases, not GNU fixture claims: truncation precedes
    // recognition and decoding, but does not make #\0digits a timestamp.
    fs::write(&path, b"#\x00123\n#456\nraw\xff\0tail\nignored\0tail").unwrap();
    let loaded = History::read_file_with_options(&path, options).unwrap();
    assert_eq!(loaded.entries()[0].line_bytes, b"#");
    assert_eq!(loaded.entries()[1].line_bytes, b"#456");
    assert_eq!(loaded.entries()[2].line_bytes, b"raw\xff");
    assert_eq!(loaded.entries().len(), 3); // unterminated fragment still ignored
    assert!(
        loaded
            .entries()
            .iter()
            .all(|entry| entry.timestamp.is_none())
    );
    fs::write(&path, b"#1\0\xff\nalpha\n").unwrap();
    let loaded = History::read_file_with_options(&path, options).unwrap();
    assert_eq!(loaded.entries()[0].timestamp.as_deref(), Some("#1"));
    let preserved = History::read_file(&path).unwrap();
    assert_eq!(
        preserved.entries()[0].timestamp.as_deref(),
        Some("#1\0\u{fffd}")
    );
}

#[test]
fn pinned_gnu_history_8_3_nul_eof_fixture_matches_sushline_policies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let preserve_options = HistoryReadOptions {
        multiline: HistoryMultiline::PerLine,
        nul_policy: HistoryNulPolicy::Preserve,
    };
    let truncate_options = HistoryReadOptions {
        nul_policy: HistoryNulPolicy::TruncateAtNul,
        ..preserve_options
    };
    // The workflow asserts GNU 8.3's result in history-full-nul-eof.expected:
    // it ignores the unfinished fragment and truncates the final-LF command
    // at NUL. Default Preserve keeps that suffix; TruncateAtNul matches it.
    struct Fixture {
        bytes: &'static [u8],
        preserved: &'static [&'static [u8]],
        truncated: &'static [&'static [u8]],
    }
    let fixtures = [
        Fixture {
            bytes: b"one\nignored\0tail",
            preserved: &[b"one"],
            truncated: &[b"one"],
        },
        Fixture {
            bytes: b"one\nignored\0tail\n",
            preserved: &[b"one", b"ignored\0tail"],
            truncated: &[b"one", b"ignored"],
        },
    ];
    for fixture in fixtures {
        fs::write(&path, fixture.bytes).unwrap();
        let preserved = History::read_file_with_options(&path, preserve_options).unwrap();
        assert_eq!(
            preserved
                .entries()
                .iter()
                .map(|entry| entry.line_bytes.as_slice())
                .collect::<Vec<_>>(),
            fixture.preserved
        );
        assert_eq!(
            History::read_file(&path).unwrap().entries(),
            preserved.entries()
        );
        let truncated = History::read_file_with_options(&path, truncate_options).unwrap();
        assert_eq!(
            truncated
                .entries()
                .iter()
                .map(|entry| entry.line_bytes.as_slice())
                .collect::<Vec<_>>(),
            fixture.truncated
        );
    }
}

#[test]
fn joined_nul_eof_inputs_match_both_sushline_policies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let preserve_options = HistoryReadOptions {
        multiline: HistoryMultiline::Joined,
        nul_policy: HistoryNulPolicy::Preserve,
    };
    let truncate_options = HistoryReadOptions {
        nul_policy: HistoryNulPolicy::TruncateAtNul,
        ..preserve_options
    };
    // Product-side expectations for the exact byte sequences sent to the
    // pinned GNU probe; this test does not assert a GNU result.
    struct Fixture {
        bytes: &'static [u8],
        preserved: &'static [&'static [u8]],
        truncated: &'static [&'static [u8]],
    }
    let fixtures = [
        Fixture {
            bytes: b"one\nignored\0tail",
            preserved: &[b"one"],
            truncated: &[b"one"],
        },
        Fixture {
            bytes: b"one\nignored\0tail\n",
            preserved: &[b"one\nignored\0tail"],
            truncated: &[b"one\nignored"],
        },
    ];
    for fixture in fixtures {
        fs::write(&path, fixture.bytes).unwrap();
        let preserved_history = History::read_file_with_options(&path, preserve_options).unwrap();
        assert_eq!(
            preserved_history
                .entries()
                .iter()
                .map(|entry| entry.line_bytes.as_slice())
                .collect::<Vec<_>>(),
            fixture.preserved
        );
        let truncated_history = History::read_file_with_options(&path, truncate_options).unwrap();
        assert_eq!(
            truncated_history
                .entries()
                .iter()
                .map(|entry| entry.line_bytes.as_slice())
                .collect::<Vec<_>>(),
            fixture.truncated
        );
    }
}

#[test]
fn nul_policies_apply_to_range_capped_and_joined_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(
        &path,
        b"#1\0suffix\nalpha\0tail\n\0hidden\nbeta\0tail\n#2\nlast\0tail\n",
    )
    .unwrap();
    for nul_policy in [HistoryNulPolicy::Preserve, HistoryNulPolicy::TruncateAtNul] {
        for multiline in [HistoryMultiline::PerLine, HistoryMultiline::Joined] {
            let options = HistoryReadOptions {
                multiline,
                nul_policy,
            };
            let full = History::read_file_with_options(&path, options).unwrap();
            if nul_policy == HistoryNulPolicy::TruncateAtNul {
                let expected: &[&[u8]] = match multiline {
                    HistoryMultiline::PerLine => &[b"alpha", b"beta", b"last"],
                    HistoryMultiline::Joined => &[b"alpha\n\nbeta", b"last"],
                };
                assert_eq!(
                    full.entries()
                        .iter()
                        .map(|entry| entry.line_bytes.as_slice())
                        .collect::<Vec<_>>(),
                    expected
                );
            }
            let mut uncapped = History::new();
            uncapped
                .load_file_with_options(&path, None, options)
                .unwrap();
            assert_eq!(uncapped.entries(), full.entries());
            for cap in [0, 1, 2, 10] {
                let mut capped = History::new();
                capped
                    .load_file_with_options(&path, Some(cap), options)
                    .unwrap();
                assert_eq!(
                    capped.entries(),
                    &full.entries()[full.len().saturating_sub(cap)..]
                );
            }
            let range = History::read_file_range_with_options(&path, 0, Some(2), options).unwrap();
            let expected: &[&[u8]] = match (nul_policy, multiline) {
                (HistoryNulPolicy::Preserve, HistoryMultiline::PerLine) => {
                    &[b"alpha\0tail", b"\0hidden"]
                }
                (HistoryNulPolicy::Preserve, HistoryMultiline::Joined) => {
                    &[b"alpha\0tail\n\0hidden"]
                }
                (HistoryNulPolicy::TruncateAtNul, HistoryMultiline::PerLine) => &[b"alpha"],
                (HistoryNulPolicy::TruncateAtNul, HistoryMultiline::Joined) => &[b"alpha"],
            };
            assert_eq!(
                range
                    .entries()
                    .iter()
                    .map(|entry| entry.line_bytes.as_slice())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                range.entries()[0].timestamp.as_deref(),
                Some(if nul_policy == HistoryNulPolicy::Preserve {
                    "#1\0suffix"
                } else {
                    "#1"
                })
            );
            for cap in [None, Some(0), Some(1)] {
                let mut loaded = History::new();
                loaded
                    .load_file_range_with_options(&path, 0, Some(2), cap, options)
                    .unwrap();
                let start = cap.map_or(0, |cap| range.len().saturating_sub(cap));
                assert_eq!(loaded.entries(), &range.entries()[start..]);
            }
            if nul_policy == HistoryNulPolicy::Preserve {
                assert_eq!(
                    full.entries(),
                    History::read_file_with_multiline(&path, multiline)
                        .unwrap()
                        .entries()
                );
                assert_eq!(
                    range.entries(),
                    History::read_file_range_with_multiline(&path, 0, Some(2), multiline)
                        .unwrap()
                        .entries()
                );
            }
        }
    }
}

#[test]
fn gnu_timestamp_text_views_first_nul_or_absent_timestamp() {
    let mut entry = HistoryEntry {
        line_bytes: Vec::new(),
        timestamp: None,
        modified: false,
        undo_list: Vec::new(),
    };
    assert_eq!(entry.gnu_timestamp_text(), None);
    entry.timestamp = Some("#123".into());
    assert_eq!(entry.gnu_timestamp_text(), Some("#123"));
    entry.timestamp = Some("#123\0suffix\0more".into());
    assert_eq!(entry.gnu_timestamp_text(), Some("#123"));
}

#[test]
fn history_entry_timestamp_seconds_parse_decimal_prefixes_and_overflow() {
    let cases = [
        ("#0", Some(0)),
        ("#1700000000", Some(1_700_000_000)),
        ("#+17", Some(17)),
        ("# -17tail", Some(-17)),
        ("# \t+17tail", Some(17)),
        // The C library's number conversion counts the vertical tab as
        // whitespace, so a record whose text carries one after the marker
        // still carries a time.
        ("#\u{b}1700000000", Some(1_700_000_000)),
        ("#\u{b}-17", Some(-17)),
        ("#17tail", Some(17)),
        ("#9223372036854775807", Some(i64::MAX)),
        ("#-9223372036854775808", Some(i64::MIN)),
        ("#9223372036854775808", None),
        ("#-9223372036854775809", None),
        ("#18446744073709551616", None),
        ("#", None),
        ("#+", None),
        ("#not-a-number", None),
        ("1700000000", None),
    ];

    for (timestamp, expected) in cases {
        let entry = HistoryEntry {
            line_bytes: Vec::new(),
            timestamp: Some(timestamp.to_string()),
            modified: false,
            undo_list: Vec::new(),
        };
        assert_eq!(
            entry.timestamp_seconds(),
            expected,
            "timestamp {timestamp:?}"
        );
    }

    let entry_without_timestamp = HistoryEntry {
        line_bytes: Vec::new(),
        timestamp: None,
        modified: false,
        undo_list: Vec::new(),
    };
    assert_eq!(entry_without_timestamp.timestamp_seconds(), None);
}

#[test]
fn gnu_timestamp_seconds_respects_explicit_comment_marker() {
    let cases = [
        (Some("#1700000000"), b'#', 1_700_000_000),
        (Some("#1700000000"), b'!', 0),
        (Some("#1700000000"), 0, 0),
        (Some("! -17tail"), b'!', -17),
        (Some("#\u{b}1700000000"), b'#', 1_700_000_000),
        (Some("#9223372036854775808"), b'#', 0),
        (Some("#1700000000\0suffix"), b'#', 1_700_000_000),
        (None, b'#', 0),
    ];

    for (timestamp, comment_char, expected) in cases {
        let entry = HistoryEntry {
            line_bytes: Vec::new(),
            timestamp: timestamp.map(str::to_owned),
            modified: false,
            undo_list: Vec::new(),
        };
        assert_eq!(
            entry.gnu_timestamp_seconds(comment_char),
            expected,
            "timestamp {timestamp:?} with comment character {comment_char:?}"
        );
    }
}

#[test]
fn history_file_entries_expose_their_timestamp_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#0\nepoch\n#1700000000\nlater\n").unwrap();

    let loaded = History::read_file(&path).unwrap();
    assert_eq!(loaded.entries()[0].timestamp_seconds(), Some(0));
    assert_eq!(loaded.entries()[1].timestamp_seconds(), Some(1_700_000_000));
}

#[test]
fn timestamp_records_survive_a_write_and_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let mut history = History::new();
    history.push("echo one");
    assert!(history.add_time("#1700000000"));
    history.write_file_with_timestamps(&path, true).unwrap();

    let loaded = History::read_file(&path).unwrap();
    assert_eq!(loaded.entries().len(), 1);
    assert_eq!(
        loaded.entries()[0].timestamp.as_deref(),
        Some("#1700000000")
    );
    assert_eq!(loaded.entries()[0].line(), "echo one");
}

#[test]
fn treats_digits_with_trailing_junk_as_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#1700000000abc\nfoo\n# not timestamp\n").unwrap();

    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (Some("#1700000000abc"), "foo".to_string()),
            (None, "# not timestamp".to_string()),
        ]
    );

    // The joined grouping keeps the block together.
    let joined = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#1700000000abc"), "foo\n# not timestamp".to_string()),]
    );
}

#[test]
fn recognizes_timestamp_delimiters_before_utf8_decoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, b"#123\xff\none\ntwo\n#456\nlast\n").unwrap();

    let loaded = History::read_file(&path).unwrap();
    assert_eq!(loaded.entries().len(), 3);
    assert_eq!(
        loaded.entries()[0].timestamp.as_deref(),
        Some("#123\u{fffd}")
    );
    assert_eq!(loaded.entries()[0].line_bytes, b"one");
    assert_eq!(loaded.entries()[1].timestamp.as_deref(), None);
    assert_eq!(loaded.entries()[1].line_bytes, b"two");
    assert_eq!(loaded.entries()[2].timestamp.as_deref(), Some("#456"));
    assert_eq!(loaded.entries()[2].line_bytes, b"last");

    // The joined grouping keeps the block together.
    let joined = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(joined.entries().len(), 2);
    assert_eq!(joined.entries()[0].line_bytes, b"one\ntwo");
    assert_eq!(joined.entries()[1].timestamp.as_deref(), Some("#456"));
    assert_eq!(joined.entries()[1].line_bytes, b"last");

    let output = dir.path().join("roundtrip");
    loaded.write_file_with_timestamps(&output, true).unwrap();
    assert_eq!(
        fs::read(output).unwrap(),
        b"#123\xef\xbf\xbd\none\ntwo\n#456\nlast\n"
    );
}

#[test]
fn preserves_timestamped_history_file_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(
        &path,
        "#1700000000\necho one\n# not timestamp\n#1700000001\nprintf two\n",
    )
    .unwrap();

    let mut loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (Some("#1700000000"), "echo one".to_string()),
            (None, "# not timestamp".to_string()),
            (Some("#1700000001"), "printf two".to_string()),
        ]
    );

    loaded.push("printf three");
    loaded.add_time("#1700000002");
    loaded.append_file_with_timestamps(&path, 2, true).unwrap();
    History::truncate_file(&path, 2).unwrap();

    let truncated = History::read_file(&path).unwrap();
    assert_eq!(
        truncated
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#1700000002"), "printf three".to_string())]
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "#1700000002\nprintf three\n"
    );

    let timestamped = dir.path().join("timestamped");
    loaded
        .write_file_with_timestamps(&timestamped, true)
        .unwrap();
    assert_eq!(
        fs::read_to_string(timestamped).unwrap(),
        "#1700000000\necho one\n# not timestamp\n#1700000001\nprintf two\n#1700000002\nprintf three\n"
    );
}

#[test]
#[cfg_attr(not(unix), ignore = "requires Unix symlinks")]
fn full_write_preserves_symlinked_history_path() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target-history");
        fs::write(&target, "target-v1\n").unwrap();
        let link = dir.path().join("link-history");
        symlink("target-history", &link).unwrap();

        let mut h = History::new();
        h.push("target-v2");
        h.write_file(&link).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "full write must preserve symlink instead of replacing it"
        );
        assert_eq!(
            fs::read_link(&link).unwrap(),
            std::path::Path::new("target-history")
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "target-v2\n");
        assert_eq!(fs::read_to_string(&link).unwrap(), "target-v2\n");
    }
}

#[test]
#[cfg_attr(not(unix), ignore = "requires Unix symlinks")]
fn full_write_preserves_relative_symlink_chain() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        use std::path::PathBuf;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target-history");
        fs::write(&target, "target-v1\n").unwrap();
        let intermediate = dir.path().join("intermediate-history");
        symlink("target-history", &intermediate).unwrap();
        let link = dir.path().join("link-history");
        symlink("intermediate-history", &link).unwrap();

        let mut history = History::new();
        history.push("target-v2");
        history.write_file(&link).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::symlink_metadata(&intermediate)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&link).unwrap(),
            PathBuf::from("intermediate-history")
        );
        assert_eq!(
            fs::read_link(&intermediate).unwrap(),
            PathBuf::from("target-history")
        );
        assert_eq!(fs::read_to_string(target).unwrap(), "target-v2\n");
    }
}

#[test]
#[cfg_attr(not(unix), ignore = "requires Unix symlinks")]
fn truncate_preserves_relative_symlink_chain() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target-history");
        fs::write(&target, "one\ntwo\nthree\n").unwrap();
        let intermediate = dir.path().join("intermediate-history");
        symlink("target-history", &intermediate).unwrap();
        let link = dir.path().join("link-history");
        symlink("intermediate-history", &link).unwrap();

        History::truncate_file(&link, 2).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::symlink_metadata(&intermediate)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(target).unwrap(), "two\nthree\n");
    }
}

#[test]
#[cfg_attr(not(unix), ignore = "requires Unix symlinks")]
fn full_write_rejects_symlink_loops_without_replacing_links() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        use std::path::PathBuf;
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        symlink("second", &first).unwrap();
        symlink("first", &second).unwrap();

        let mut history = History::new();
        history.push("new");
        let error = history.write_file(&first).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(
            fs::symlink_metadata(&first)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::symlink_metadata(&second)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&first).unwrap(), PathBuf::from("second"));
        assert_eq!(fs::read_link(&second).unwrap(), PathBuf::from("first"));
    }
}

#[test]
#[cfg_attr(not(unix), ignore = "requires Unix symlinks and /dev/null")]
fn symlinked_special_history_discards_and_preserves_link() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("history");
        symlink("/dev/null", &link).unwrap();

        let mut h = History::new();
        h.push("hello");
        h.write_file(&link).unwrap();
        h.append_file(&link, 0).unwrap();
        assert!(
            History::truncate_file(&link, 10).is_err(),
            "truncate of a non-regular destination must error like GNU"
        );

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "special-file writes must preserve the symlink"
        );
        assert!(!dir.path().join("history.tmp").exists());
    }
}

#[test]
#[cfg_attr(not(unix), ignore = "requires /dev/null and symlinks")]
fn non_regular_history_reads_error_like_gnu() {
    use std::io::ErrorKind;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "one\ntwo\n").unwrap();

    // Regular positive control for all four read/load paths.
    assert_eq!(History::read_file(&path).unwrap().len(), 2);
    assert_eq!(History::read_file_range(&path, 0, None).unwrap().len(), 2);
    let mut loaded = History::new();
    loaded.load_file(&path, None).unwrap();
    assert_eq!(loaded.len(), 2);
    let mut ranged = History::new();
    ranged.load_file_range(&path, 0, None, None).unwrap();
    assert_eq!(ranged.len(), 2);

    // Non-regular reads reject like GNU (EINVAL) instead of loading as empty.
    for result in [
        History::read_file("/dev/null").map(|history| history.len()),
        History::read_file_range("/dev/null", 0, None).map(|history| history.len()),
        History::read_file_range("/dev/null", 0, Some(1)).map(|history| history.len()),
    ] {
        let err = result.expect_err("reading /dev/null must error like GNU");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
    let mut history = History::new();
    history.push("kept");
    assert_eq!(
        history
            .load_file("/dev/null", None)
            .expect_err("loading /dev/null must error")
            .kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(history.len(), 1, "failed load must keep existing entries");
    assert_eq!(
        history
            .load_file_range("/dev/null", 0, None, None)
            .expect_err("range loading /dev/null must error")
            .kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(history.len(), 1, "failed range load must keep entries");

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = dir.path().join("link-history");
        symlink(&path, &link).unwrap();
        assert_eq!(History::read_file(&link).unwrap().len(), 2);
        assert_eq!(History::read_file_range(&link, 0, None).unwrap().len(), 2);
        let mut linked = History::new();
        linked.load_file(&link, None).unwrap();
        assert_eq!(linked.len(), 2);
        let mut linked_range = History::new();
        linked_range.load_file_range(&link, 0, None, None).unwrap();
        assert_eq!(linked_range.len(), 2);

        let null_link = dir.path().join("null-history");
        symlink("/dev/null", &null_link).unwrap();
        assert!(
            History::read_file(&null_link).is_err(),
            "reading through a symlink to a special file must error"
        );
        assert!(
            fs::symlink_metadata(&null_link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "failed read must preserve the symlink"
        );
    }
}

#[test]
#[cfg_attr(not(unix), ignore = "requires /dev/null")]
fn truncate_non_regular_destination_errors() {
    assert!(
        History::truncate_file("/dev/null", 10).is_err(),
        "truncate of /dev/null must error like GNU history_truncate_file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("history");
        symlink("/dev/null", &link).unwrap();
        assert!(
            History::truncate_file(&link, 10).is_err(),
            "truncate through a symlink to a special file must error"
        );
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "failed truncate must preserve the symlink"
        );
    }
}

#[test]
fn reads_history_file_ranges_and_controls_timestamp_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#1700000000\none\n#1700000001\ntwo\nthree\n").unwrap();

    // The range bounds count physical command lines: "one", "two" and
    // "three".
    let ranged = History::read_file_range(&path, 1, Some(3)).unwrap();
    assert_eq!(
        ranged
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (Some("#1700000001"), "two".to_string()),
            (None, "three".to_string()),
        ]
    );

    let single = History::read_file_range(&path, 1, Some(2)).unwrap();
    assert_eq!(
        single
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#1700000001"), "two".to_string())]
    );

    let to_end = History::read_file_range(&path, 1, None).unwrap();
    assert_eq!(
        to_end
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["two".to_string(), "three".to_string()]
    );

    let reversed_range = History::read_file_range(&path, 1, Some(0)).unwrap();
    assert_eq!(
        reversed_range
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["two".to_string(), "three".to_string()]
    );

    // The joined grouping applies to the selected lines.
    let joined =
        History::read_file_range_with_multiline(&path, 1, Some(3), HistoryMultiline::Joined)
            .unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#1700000001"), "two\nthree".to_string())]
    );

    let no_timestamps = dir.path().join("no-timestamps");
    ranged
        .write_file_with_timestamps(&no_timestamps, false)
        .unwrap();
    assert_eq!(fs::read_to_string(no_timestamps).unwrap(), "two\nthree\n");
}

#[test]
fn finite_ranges_preserve_carriers_and_grouping_at_crlf_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    // Blanks count toward the bound, consecutive timestamps do not, and
    // the final unterminated fragment is never a command line.
    fs::write(
        &path,
        b"#1\r\n\r\nA\r\n\r\nB\r\n#2\r\n#3\r\n\r\nC\r\n\r\nD\r\nfragment",
    )
    .unwrap();
    type Entries = &'static [(&'static str, Option<&'static str>)];
    let cases: &[(usize, Option<usize>, Entries, Entries)] = &[
        (0, Some(1), &[], &[("", None)]),
        (0, Some(3), &[("A", Some("#1"))], &[("A", Some("#1"))]),
        // The #1 carrier A was discarded. The block must still use
        // timestamped Joined rules: trim boundary blanks, do not inherit
        // the timestamp, and emit an empty entry only for a one-line cut.
        (2, Some(2), &[], &[("", None)]),
        (2, Some(4), &[("B", None)], &[("B", None)]),
        (
            2,
            Some(8),
            &[("B", None), ("C", Some("#3")), ("D", None)],
            &[("B", None), ("C\n\nD", Some("#3"))],
        ),
        (4, Some(6), &[("C", Some("#3"))], &[("C", Some("#3"))]),
        (5, Some(7), &[("C", Some("#3"))], &[("C", Some("#3"))]),
        (6, Some(6), &[], &[("", None)]),
        (
            5,
            Some(8),
            &[("C", Some("#3")), ("D", None)],
            &[("C\n\nD", Some("#3"))],
        ),
        (8, Some(9), &[], &[]),
        (7, Some(0), &[("D", None)], &[("D", None)]),
        (7, None, &[("D", None)], &[("D", None)]),
    ];
    for &(from, to, per_line, joined) in cases {
        for (mode, expected) in [
            (HistoryMultiline::PerLine, per_line),
            (HistoryMultiline::Joined, joined),
        ] {
            let history = History::read_file_range_with_multiline(&path, from, to, mode).unwrap();
            let actual: Vec<_> = history
                .entries()
                .iter()
                .map(|entry| (entry.line().into_owned(), entry.timestamp.as_deref()))
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|&(line, timestamp)| (line.to_owned(), timestamp))
                .collect();
            assert_eq!(actual, expected, "{from}..{to:?} {mode:?}");
        }
    }
}

#[test]
fn finite_timestamped_ranges_discard_carriers_but_not_block_grouping() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, b"#1\nold\n\n\nB\n\n#2\n#3\n\nC\n\n#4\nfragment").unwrap();
    type Entries = &'static [(&'static str, Option<&'static str>)];
    let cases: &[(usize, usize, Entries, Entries)] = &[
        (1, 3, &[], &[]), // Multiple blanks do not make a timestamped entry.
        (1, 5, &[("B", None)], &[("B", None)]),
        (
            1,
            8,
            &[("B", None), ("C", Some("#3"))],
            &[("B", None), ("C", Some("#3"))],
        ),
        (5, 8, &[("C", Some("#3"))], &[("C", Some("#3"))]),
        (8, 9, &[], &[]), // Empty trailing block and unterminated fragment.
    ];
    for &(from, to, per_line, joined) in cases {
        for (mode, expected) in [
            (HistoryMultiline::PerLine, per_line),
            (HistoryMultiline::Joined, joined),
        ] {
            let history =
                History::read_file_range_with_multiline(&path, from, Some(to), mode).unwrap();
            let actual: Vec<_> = history
                .entries()
                .iter()
                .map(|entry| (entry.line().into_owned(), entry.timestamp.as_deref()))
                .collect();
            let expected: Vec<_> = expected
                .iter()
                .map(|&(line, timestamp)| (line.to_owned(), timestamp))
                .collect();
            assert_eq!(actual, expected, "{from}..{to} {mode:?}");
        }
    }
}

#[test]
fn finite_plain_joined_ranges_keep_blanks_and_later_timestamp_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    // A blank first line selects plain format, even when the selected
    // range starts at a later timestamp-shaped command line.
    fs::write(&path, b"\r\nA\r\n#2\r\n\r\nB\r\nfragment").unwrap();
    for (from, to, expected) in [
        (1, 3, Some("A\n#2")),
        (2, 2, Some("#2")),
        (3, 4, Some("")),
        (1, 5, Some("A\n#2\n\nB")),
        (4, 9, Some("B")),
        (5, 9, None),
    ] {
        let history = History::read_file_range_with_multiline(
            &path,
            from,
            Some(to),
            HistoryMultiline::Joined,
        )
        .unwrap();
        let actual: Vec<_> = history
            .entries()
            .iter()
            .map(|entry| (entry.line().into_owned(), entry.timestamp.as_deref()))
            .collect();
        let expected: Vec<_> = expected
            .into_iter()
            .map(|line| (line.to_owned(), None))
            .collect();
        assert_eq!(actual, expected, "{from}..{to}");
    }
}

#[test]
fn suffix_timestamped_ranges_preserve_carriers_and_grouping() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    type Entries = &'static [(&'static str, Option<&'static str>)];
    let cases: &[(usize, Entries, Entries)] = &[
        // Skipping only a blank leaves the first command's carrier intact.
        (
            1,
            &[
                ("old", Some("#1")),
                ("B", None),
                ("C", None),
                ("D", Some("#3")),
                ("E", None),
            ],
            &[("old\n\nB\n\nC", Some("#1")), ("D\n\nE", Some("#3"))],
        ),
        // Skipping the old carrier must not turn its block into plain grouping.
        (
            2,
            &[("B", None), ("C", None), ("D", Some("#3")), ("E", None)],
            &[("B\n\nC", None), ("D\n\nE", Some("#3"))],
        ),
        // Consecutive markers at the selected boundary replace the old carrier.
        (
            6,
            &[("D", Some("#3")), ("E", None)],
            &[("D\n\nE", Some("#3"))],
        ),
        (
            7,
            &[("D", Some("#3")), ("E", None)],
            &[("D\n\nE", Some("#3"))],
        ),
        (8, &[("E", None)], &[("E", None)]),
        (10, &[], &[]), // An open-ended blank-only block creates no entry.
        (11, &[], &[]), // EOF fragment is ignored, including beyond EOF.
        (20, &[], &[]),
        (usize::MAX, &[], &[]),
    ];
    for newline in ["\n", "\r\n"] {
        let contents = "#1\n\nold\n\nB\n\nC\n#2\n#3\n\nD\n\nE\n\n#4\nfragment";
        fs::write(&path, contents.replace('\n', newline)).unwrap();
        for &(from, per_line, joined) in cases {
            for to in [None, Some(0), Some(from - 1)] {
                for (mode, expected) in [
                    (HistoryMultiline::PerLine, per_line),
                    (HistoryMultiline::Joined, joined),
                ] {
                    let history =
                        History::read_file_range_with_multiline(&path, from, to, mode).unwrap();
                    let actual: Vec<_> = history
                        .entries()
                        .iter()
                        .map(|entry| (entry.line().into_owned(), entry.timestamp.as_deref()))
                        .collect();
                    let expected: Vec<_> = expected
                        .iter()
                        .map(|&(line, timestamp)| (line.to_owned(), timestamp))
                        .collect();
                    assert_eq!(actual, expected, "{from}..{to:?} {mode:?} {newline:?}");
                }
            }
        }
    }
}

#[test]
fn suffix_plain_ranges_preserve_blanks_and_timestamp_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, b"\r\nA\r\n#2\r\n\r\nB\r\n\r\nfragment").unwrap();
    for (from, per_line, joined) in [
        (2, vec!["#2", "B"], vec!["#2\n\nB\n"]),
        (3, vec!["B"], vec!["\nB\n"]),
        (5, vec![], vec![""]),
        (6, vec![], vec![]),
        (20, vec![], vec![]),
        (usize::MAX, vec![], vec![]),
    ] {
        for to in [None, Some(0), Some(from - 1)] {
            for (mode, expected) in [
                (HistoryMultiline::PerLine, per_line.clone()),
                (HistoryMultiline::Joined, joined.clone()),
            ] {
                let history =
                    History::read_file_range_with_multiline(&path, from, to, mode).unwrap();
                let actual: Vec<_> = history
                    .entries()
                    .iter()
                    .map(|entry| (entry.line().into_owned(), entry.timestamp.as_deref()))
                    .collect();
                let expected: Vec<_> = expected
                    .into_iter()
                    .map(|line| (line.to_owned(), None))
                    .collect();
                assert_eq!(actual, expected, "{from}..{to:?} {mode:?}");
            }
        }
    }
}

#[test]
fn range_bounds_match_the_measured_gnu_range_call() {
    // Measured against GNU's own range call: the bounds count the file's
    // command lines and the selected lines are grouped afterwards, so a
    // range can end with a partial block. The oracle job measures the same
    // against the pinned release, and there the pinned release's own range
    // call answers and selects the same command lines.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#1700000000\nA0\nA1\n#1700000001\nB0\nB1\nB2\n").unwrap();

    // The bounds count the command lines A0, A1 and B0: the TO bound is
    // exclusive, so B1 is outside the range.
    let per_line = History::read_file_range(&path, 0, Some(3)).unwrap();
    assert_eq!(
        per_line
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["A0".to_string(), "A1".to_string(), "B0".to_string()]
    );

    // The joined grouping applies to the selected lines, so the first
    // block is whole and the second one is partial.
    let joined =
        History::read_file_range_with_multiline(&path, 0, Some(3), HistoryMultiline::Joined)
            .unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["A0\nA1".to_string(), "B0".to_string()]
    );

    // A range that starts inside a block selects the rest of that block
    // alone, and the block that follows it as a partial one.
    let partial =
        History::read_file_range_with_multiline(&path, 1, Some(3), HistoryMultiline::Joined)
            .unwrap();
    assert_eq!(
        partial
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["A1".to_string(), "B0".to_string()]
    );
}

#[test]
fn timestamped_range_bounds_count_blank_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#1700000000\nA0\n\nA1\n#1700000001\nB0\nB1\n").unwrap();

    let blank_only = History::read_file_range(&path, 1, Some(1)).unwrap();
    assert!(blank_only.entries().is_empty());

    let history = History::read_file_range(&path, 0, Some(3)).unwrap();
    assert_eq!(
        history
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["A0".to_string(), "A1".to_string()]
    );

    let joined =
        History::read_file_range_with_multiline(&path, 0, Some(3), HistoryMultiline::Joined)
            .unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["A0\n\nA1".to_string()]
    );
}

#[test]
fn joined_blank_only_ranges_preserve_one_untimestamped_entry() {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in [
        (
            "timestamped",
            "#1700000000\nA0\n\nA1\n#1700000001\nB0\nB1\n",
        ),
        ("plain", "A0\n\nA1\nB0\nB1\n"),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        for to in [1, 2] {
            let per_line = History::read_file_range_with_multiline(
                &path,
                1,
                Some(to),
                HistoryMultiline::PerLine,
            )
            .unwrap();
            assert!(per_line.entries().is_empty(), "{name}: to={to}");
            let joined = History::read_file_range_with_multiline(
                &path,
                1,
                Some(to),
                HistoryMultiline::Joined,
            )
            .unwrap();
            assert_eq!(joined.entries().len(), 1, "{name}: to={to}");
            assert!(joined.entries()[0].line_bytes.is_empty(), "{name}: to={to}");
            assert_eq!(joined.entries()[0].timestamp, None, "{name}: to={to}");
        }
    }

    let mut loaded = History::new();
    loaded
        .load_file_range_with_multiline(
            dir.path().join("timestamped"),
            1,
            Some(2),
            None,
            HistoryMultiline::Joined,
        )
        .unwrap();
    assert_eq!(loaded.entries().len(), 1);
    assert!(loaded.entries()[0].line_bytes.is_empty());
    assert_eq!(loaded.entries()[0].timestamp, None);
}

#[test]
fn timestamped_joined_ranges_do_not_create_entries_for_multiple_blanks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#1700000000\nA0\n\n\nA1\n").unwrap();
    let joined =
        History::read_file_range_with_multiline(&path, 1, Some(3), HistoryMultiline::Joined)
            .unwrap();
    assert!(joined.entries().is_empty());
}

#[test]
fn range_edges_at_blank_physical_lines_keep_the_current_rule() {
    let dir = tempfile::tempdir().unwrap();
    let fixtures = [
        (
            "timestamped",
            "#1700000000\nA0\n\nA1\n#1700000001\nB0\nB1\n",
            "A1",
            vec!["A1", "B0", "B1"],
        ),
        ("plain", "C0\n\nC1\nC2\n", "C1", vec!["C1", "C2"]),
    ];

    for (name, contents, after_blank, from_blank_to_eof) in fixtures {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        let lines = |from, to| {
            History::read_file_range(&path, from, Some(to))
                .unwrap()
                .entries()
                .iter()
                .map(|entry| entry.line().into_owned())
                .collect::<Vec<_>>()
        };

        assert!(lines(1, 1).is_empty(), "{name}: equal blank-line bound");
        assert_eq!(lines(1, 3), vec![after_blank], "{name}: forward from blank");
        assert_eq!(
            lines(1, 0),
            from_blank_to_eof,
            "{name}: reversed bounds from blank"
        );
        assert_eq!(
            lines(2, 1),
            from_blank_to_eof,
            "{name}: reversed bounds ending at blank"
        );
        assert_eq!(
            lines(2, 2),
            vec![after_blank],
            "{name}: equal nonblank bound"
        );
    }
}

#[test]
fn plain_range_bounds_count_blank_lines_in_both_modes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "A0\n\nA1\nB0\nB1\n").unwrap();

    let per_line = History::read_file_range(&path, 0, Some(3)).unwrap();
    assert_eq!(
        per_line
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["A0".to_string(), "A1".to_string()]
    );

    let joined =
        History::read_file_range_with_multiline(&path, 0, Some(3), HistoryMultiline::Joined)
            .unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["A0\n\nA1".to_string()]
    );
}

#[test]
fn the_bounds_at_or_below_from_are_the_products_own_rule() {
    // Pinned GNU History 8.3 returns no entries for these tested timestamped
    // equal/reversed bounds, but returns the FROM line for the tested plain
    // file bounds. Sushline retains its product rule: equal bounds select the
    // command line at FROM and a TO below FROM leaves the range open to EOF.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#1700000000\nA0\nA1\n#1700000001\nB0\nB1\nB2\n").unwrap();

    let lines = |history: &History| -> Vec<String> {
        history
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect()
    };

    // Equal bounds select the command line at FROM.
    let equal = History::read_file_range(&path, 0, Some(0)).unwrap();
    assert_eq!(lines(&equal), vec!["A0".to_string()]);
    let equal = History::read_file_range(&path, 2, Some(2)).unwrap();
    assert_eq!(lines(&equal), vec!["B0".to_string()]);

    // A TO below FROM leaves the range open to the end of the file.
    let reversed = History::read_file_range(&path, 2, Some(1)).unwrap();
    assert_eq!(
        lines(&reversed),
        vec!["B0".to_string(), "B1".to_string(), "B2".to_string()]
    );

    // The joined grouping applies to the selected lines, so a range that
    // starts inside a block selects the rest of that block alone.
    let joined =
        History::read_file_range_with_multiline(&path, 3, Some(3), HistoryMultiline::Joined)
            .unwrap();
    assert_eq!(lines(&joined), vec!["B1".to_string()]);

    // Without timestamp records, equal bounds select FROM. The pinned GNU
    // fixture returns only C2 for reversed (2,1); Sushline retains C2 and C3.
    fs::write(&path, "C0\nC1\nC2\nC3\n").unwrap();
    let equal = History::read_file_range(&path, 0, Some(0)).unwrap();
    assert_eq!(lines(&equal), vec!["C0".to_string()]);
    let equal = History::read_file_range(&path, 2, Some(2)).unwrap();
    assert_eq!(lines(&equal), vec!["C2".to_string()]);
    let reversed = History::read_file_range(&path, 2, Some(1)).unwrap();
    assert_eq!(lines(&reversed), vec!["C2".to_string(), "C3".to_string()]);
}

#[test]
fn ranges_beyond_eof_match_the_pinned_gnu_fixtures() {
    // The pinned GNU History 8.3 out-of-file fixtures record these bounds;
    // FROM beyond EOF is empty, while TO beyond EOF selects the remaining
    // command lines, for both timestamped and plain files.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let lines = |history: &History| -> Vec<String> {
        history
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect()
    };

    fs::write(&path, "#1700000000\nA0\nA1\n#1700000001\nB0\nB1\nB2\n").unwrap();
    let beyond_from = History::read_file_range(&path, 5, Some(9)).unwrap();
    assert!(lines(&beyond_from).is_empty());
    let beyond_to = History::read_file_range(&path, 3, Some(9)).unwrap();
    assert_eq!(lines(&beyond_to), vec!["B1".to_string(), "B2".to_string()]);

    fs::write(&path, "C0\nC1\nC2\nC3\n").unwrap();
    let beyond_from = History::read_file_range(&path, 4, Some(9)).unwrap();
    assert!(lines(&beyond_from).is_empty());
    let beyond_to = History::read_file_range(&path, 2, Some(9)).unwrap();
    assert_eq!(lines(&beyond_to), vec!["C2".to_string(), "C3".to_string()]);
}

#[test]
fn range_bounds_count_physical_command_lines() {
    // FROM/TO bounds count physical command lines, including blanks; timestamp
    // records are metadata and do not count. The fixture has three nonempty
    // commands and one blank line across two timestamped blocks.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(
        &path,
        "#1700000000\nline one\n\nline two\n#1700000001\nnext\n",
    )
    .unwrap();

    // The blank is at physical line index 1, so "line two" is at range index 2.
    let second = History::read_file_range(&path, 2, Some(3)).unwrap();
    assert_eq!(
        second
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(None, "line two".to_string())]
    );

    let third = History::read_file_range(&path, 3, Some(4)).unwrap();
    assert_eq!(
        third
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#1700000001"), "next".to_string())]
    );

    // Equal bounds select the command line at FROM, and a TO below FROM
    // leaves the range open to the end of the file.
    let equal = History::read_file_range(&path, 2, Some(2)).unwrap();
    assert_eq!(
        equal
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(None, "line two".to_string())]
    );

    let reversed = History::read_file_range(&path, 2, Some(1)).unwrap();
    assert_eq!(
        reversed
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (None, "line two".to_string()),
            (Some("#1700000001"), "next".to_string())
        ]
    );

    // The joined grouping applies to the selected lines, so the blank inside
    // the selected physical range remains inside the grouped entry.
    let joined =
        History::read_file_range_with_multiline(&path, 0, Some(3), HistoryMultiline::Joined)
            .unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#1700000000"), "line one\n\nline two".to_string())]
    );

    let partial =
        History::read_file_range_with_multiline(&path, 1, Some(4), HistoryMultiline::Joined)
            .unwrap();
    assert_eq!(
        partial
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (None, "line two".to_string()),
            (Some("#1700000001"), "next".to_string()),
        ]
    );

    // Truncation counts all physical LF-delimited lines, including timestamps.
    History::truncate_file(&path, 1).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "next\n");
}

#[test]
fn truncates_timestamped_history_with_measured_gnu_bytes() {
    // Exact GNU History 8.3 fixtures with history_multiline_entries=0.
    let cases: &[(&[u8], usize, &[u8])] = &[
        (
            b"#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n",
            10,
            b"#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n",
        ),
        (b"#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n", 1, b"B1\n"),
        (
            b"#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n",
            2,
            b"B0\nB1\n",
        ),
        (
            b"#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n",
            3,
            b"#1700000001\nB0\nB1\n",
        ),
        (
            b"#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n",
            4,
            b"A1\n#1700000001\nB0\nB1\n",
        ),
        (b"#1700000000\nA0\nA1\n", 1, b"A1\n"),
        (b"#1700000000\nA0\nA1\n", 2, b"A0\nA1\n"),
        (b"#1700000000\nA0\n\nB0\n", 1, b"B0\n"),
        (b"#1700000000\nA0\n\nB0\n", 2, b"\nB0\n"),
        (b"#1700000000\nA0\n#1700000001\n", 0, b""),
        (b"#1700000000\nA0\n#1700000001\n", 1, b"#1700000001\n"),
        (b"#1700000000\nA0\n#1700000001\n", 2, b"A0\n#1700000001\n"),
        (
            b"#1700000000\n#1700000001\nA0\n#1700000002\nA1\n",
            1,
            b"A1\n",
        ),
        (
            b"#1700000000\n#1700000001\nA0\n#1700000002\nA1\n",
            2,
            b"#1700000002\nA1\n",
        ),
        (b"#1700000000\nA\n#1700000001\nB", 1, b"#1700000001\nB"),
        (b"#1700000000\nA\n#1700000001\nB", 2, b"A\n#1700000001\nB"),
        (b"#1700000000\nA\n#1700000001", 1, b"A\n#1700000001"),
        (
            b"#1700000000\nA\n#1700000001",
            2,
            b"#1700000000\nA\n#1700000001",
        ),
    ];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    for &(input, limit, expected) in cases {
        fs::write(&path, input).unwrap();
        History::truncate_file(&path, limit).unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            expected,
            "fixture {input:?}, limit {limit}"
        );
    }
}

#[test]
fn late_timestamp_line_is_command_text_in_an_untimestamped_file() {
    // The pinned GNU History 8.3 full-file and range probes both read this
    // input's late timestamp-looking line as command text: the first physical
    // line selects the file format.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "A0\n#1700000000\nB0\n").unwrap();

    let loaded = History::read_file(&path).unwrap();
    // Keep the full-file result available for the pinned CI comparison.
    println!("RUST_BASELINE|entry_count={}", loaded.entries().len());
    for (index, entry) in loaded.entries().iter().enumerate() {
        println!(
            "RUST_BASELINE|entry_{index}={}",
            entry.line().replace('\n', "\\n")
        );
    }
    let expected = vec![
        (None, "A0".to_string()),
        (None, "#1700000000".to_string()),
        (None, "B0".to_string()),
    ];
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        expected
    );

    let ranged = History::read_file_range(&path, 0, Some(3)).unwrap();
    assert_eq!(
        ranged
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn append_new_can_suppress_timestamp_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#1700000000\none\n").unwrap();

    let mut loaded = History::read_file(&path).unwrap();
    loaded.push("two");
    loaded.add_time("#1700000001");
    loaded
        .append_new_to_file_with_timestamps(&path, false)
        .unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "#1700000000\none\ntwo\n"
    );

    loaded.push("three");
    loaded.add_time("#1700000002");
    loaded.append_new_to_file(&path).unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "#1700000000\none\ntwo\nthree\n"
    );
}

#[test]
fn history_expansion_supports_line_so_far_status_and_policy() {
    let mut history = History::new();
    history.push("echo alpha beta");

    assert_eq!(
        expand_history(
            b"printf !#",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        b"printf printf ".to_vec()
    );

    let printed = expand_history_with_status(
        b"!!:p",
        &history,
        HistoryChars::parse("!^#"),
        &HistoryExpansionPolicy::default(),
        |_| false,
    )
    .unwrap();
    assert_eq!(printed.line, b"echo alpha beta");
    assert_eq!(printed.status, HistoryExpansionStatus::PrintOnly);

    let policy = HistoryExpansionPolicy {
        quotes_inhibit_expansion: true,
        quote_state: Some(b'\''),
        ..HistoryExpansionPolicy::default()
    };
    assert_eq!(
        expand_history(b"!!", &history, HistoryChars::parse("!^#"), &policy, |_| {
            false
        },)
        .unwrap(),
        b"!!".to_vec()
    );
}

#[test]
fn history_expansion_inhibit_index_zero_matches_pinned_fixture() {
    // Paired with history_expand_probe's index-zero callback and index-one
    // control in the pinned GNU History 8.3 CI fixture.
    let mut history = History::new();
    for entry in ["echo previous", "echo a b", "echo a b"] {
        history.push(entry);
    }
    for (input, expected, status) in [
        (
            b"!!".as_slice(),
            b"!!".as_slice(),
            HistoryExpansionStatus::Unchanged,
        ),
        (
            b"x!!".as_slice(),
            b"xecho a b".as_slice(),
            HistoryExpansionStatus::Expanded,
        ),
    ] {
        let result = expand_history_with_status(
            input,
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |index| index == 0,
        )
        .unwrap();
        assert_eq!(result.line, expected, "input={input:?}");
        assert_eq!(result.status, status, "input={input:?}");
    }
}

#[test]
fn history_expansion_inherited_quote_state_matches_pinned_fixture() {
    // Paired with history_expand_probe's quote_state_single and
    // quote_state_unquoted_control fixtures in pinned GNU History 8.3 CI.
    let mut history = History::new();
    for entry in ["echo previous", "echo a b", "echo a b"] {
        history.push(entry);
    }
    for (quote_state, expected, status) in [
        (
            Some(b'\''),
            b"!!".as_slice(),
            HistoryExpansionStatus::Unchanged,
        ),
        (
            None,
            b"echo a b".as_slice(),
            HistoryExpansionStatus::Expanded,
        ),
    ] {
        let policy = HistoryExpansionPolicy {
            quotes_inhibit_expansion: true,
            quote_state,
            ..HistoryExpansionPolicy::default()
        };
        let result = expand_history_with_status(
            b"!!",
            &history,
            HistoryChars::parse("!^#"),
            &policy,
            |_| false,
        )
        .unwrap();
        assert_eq!(result.line, expected, "quote_state={quote_state:?}");
        assert_eq!(result.status, status, "quote_state={quote_state:?}");
    }
}

#[test]
fn history_expansion_current_line_event_supports_shared_suffixes() {
    // Paired with the five current_line* rows in CI's history_expand_probe.
    // Local GNU 8.2 measurements are directional; pinned 8.3 CI must confirm.
    let mut history = History::new();
    history.push("echo previous");
    history.push("echo a b");
    history.push("echo a b");
    let policy = HistoryExpansionPolicy::default();
    for (input, expected) in [
        (b"printf !#".as_slice(), b"printf printf ".as_slice()),
        (
            b"echo a b !#:q".as_slice(),
            b"echo a b 'echo a b '".as_slice(),
        ),
        (b"echo a b !#:0".as_slice(), b"echo a b echo".as_slice()),
        (
            b"echo a b !#:s/a/X/".as_slice(),
            b"echo a b echo X b ".as_slice(),
        ),
        (b"echo a b !#:$".as_slice(), b"echo a b b".as_slice()),
    ] {
        let result = expand_history_with_status(
            input,
            &history,
            HistoryChars::parse("!^#"),
            &policy,
            |_| false,
        )
        .unwrap();
        assert_eq!(result.line, expected, "input={input:?}");
        assert_eq!(
            result.status,
            HistoryExpansionStatus::Expanded,
            "input={input:?}"
        );
    }
}

#[test]
fn history_expansion_tracks_escaped_quotes_in_context() {
    let mut history = History::new();
    history.push("echo previous");
    let policy = HistoryExpansionPolicy::default();
    let cases = [
        (
            br"echo \' !!".as_slice(),
            br"echo \' echo previous".as_slice(),
        ),
        (
            br#"echo \"'!!"#.as_slice(),
            br#"echo \"'echo previous"#.as_slice(),
        ),
        (
            br#"echo "a\"'!!"#.as_slice(),
            br#"echo "a\"'echo previous"#.as_slice(),
        ),
        (
            br"echo 'a\' !!".as_slice(),
            br"echo 'a\' echo previous".as_slice(),
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(
            expand_history(input, &history, HistoryChars::parse("!^#"), &policy, |_| {
                false
            },)
            .unwrap(),
            expected,
            "input={input:?}"
        );
    }
}

#[test]
fn history_root_and_extension_modifiers_search_the_entire_selected_text() {
    let mut history = History::new();
    for (event, expected_root, expected_extension) in [
        ("a.b/c", "a", ".b/c"),
        ("foo", "foo", "foo"),
        ("foo/", "foo/", "foo/"),
        ("/", "/", "/"),
        ("echo foo", "echo foo", "echo foo"),
    ] {
        history.push(event);
        for (modifier, expected) in [
            (b"!!:r".as_slice(), expected_root.as_bytes()),
            (b"!!:e".as_slice(), expected_extension.as_bytes()),
        ] {
            assert_eq!(
                expand_history(
                    modifier,
                    &history,
                    HistoryChars::parse("!^#"),
                    &HistoryExpansionPolicy::default(),
                    |_| false,
                )
                .unwrap(),
                expected,
                "event={event:?} modifier={modifier:?}"
            );
        }
    }
}

#[test]
fn history_head_and_tail_modifiers_use_byte_string_slash_rules() {
    let mut history = History::new();
    for (event, expected_head, expected_tail) in [
        ("foo", "foo", "foo"),
        ("/foo", "", "foo"),
        ("foo/", "foo", ""),
        ("/", "", ""),
        ("a//", "a/", ""),
        ("a//b", "a/", "b"),
    ] {
        history.push(event);
        for (modifier, expected) in [
            (b"!!:h".as_slice(), expected_head.as_bytes()),
            (b"!!:t".as_slice(), expected_tail.as_bytes()),
        ] {
            assert_eq!(
                expand_history(
                    modifier,
                    &history,
                    HistoryChars::parse("!^#"),
                    &HistoryExpansionPolicy::default(),
                    |_| false,
                )
                .unwrap(),
                expected,
                "event={event:?} modifier={modifier:?}"
            );
        }
    }
}

#[test]
fn history_expansion_matches_readline_search_and_substitution_failures() {
    let mut history = History::new();
    history.push("grep needle middle needle-last tail");

    assert_eq!(
        expand_history(
            b"!?needle middle?",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        b"grep needle middle needle-last tail".to_vec()
    );

    history.clear();
    history.push("printf /a/b/c.txt alpha alpha beta");
    for expansion in [
        b"!!:s/missing/MISSING/".as_slice(),
        b"!!:gs/missing/MISSING/".as_slice(),
        b"!!:Gs/missing/MISSING/".as_slice(),
        b"!!:&".as_slice(),
        b"^missing^MISSING^".as_slice(),
        b"^^MISSING^".as_slice(),
    ] {
        assert_eq!(
            expand_history(
                expansion,
                &history,
                HistoryChars::parse("!^#"),
                &HistoryExpansionPolicy::default(),
                |_| false,
            ),
            Err(HistoryExpansionError::SubstitutionFailed),
            "{}",
            String::from_utf8_lossy(expansion)
        );
    }
}

#[test]
fn history_expansion_only_allows_readline_colonless_word_designators() {
    let mut history = History::new();
    history.push("echo zero one two three");

    assert_eq!(
        expand_history(
            b"!!2",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        b"echo zero one two three2".to_vec()
    );
    assert_eq!(
        expand_history(
            b"!!0",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        b"echo zero one two three0".to_vec()
    );
    assert_eq!(
        expand_history(
            b"!!-2",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        b"echo zero one".to_vec()
    );
}

#[test]
fn caret_word_designator_errors_on_single_word_event() {
    let mut history = History::new();
    history.push("ls");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();

    // GNU defines `^` as word 1 with no single-word fallback (the zeroth-word
    // fallback is documented only for `$`), so `^` errors like numeric `:1`.
    for (typed, spec) in [
        (b"!!:^".as_slice(), "^"),
        (b"!!^".as_slice(), "^"),
        (b"!!:1".as_slice(), "1"),
    ] {
        let error = expand_history(typed, &history, histchars, &policy, |_| false).unwrap_err();
        assert_eq!(
            error,
            HistoryExpansionError::BadWordSpecifier(spec.to_string()),
            "{}",
            String::from_utf8_lossy(typed)
        );
        assert_eq!(error.message(), format!("{spec}: bad word specifier"));
    }

    // Documented single-word successes stay intact.
    for (typed, expected) in [
        ("!!:$", b"ls".as_slice()),
        ("!!:0", b"ls".as_slice()),
        ("!!:*", b"".as_slice()),
    ] {
        assert_eq!(
            expand_history(typed.as_bytes(), &history, histchars, &policy, |_| false).unwrap(),
            expected.to_vec(),
            "{typed}"
        );
    }

    // Two-word control: `^`, `:1`, and `$` agree on the first argument.
    history.clear();
    history.push("echo hi");
    for typed in [b"!!:^".as_slice(), b"!!:1".as_slice(), b"!!:$".as_slice()] {
        assert_eq!(
            expand_history(typed, &history, histchars, &policy, |_| false).unwrap(),
            b"hi".to_vec(),
            "{}",
            String::from_utf8_lossy(typed)
        );
    }
}

#[test]
fn word_designator_stops_at_the_end_of_the_designator() {
    // GNU Bash 5.3 reads the designator and stops where the designator
    // ends, so the text that follows is literal: `!!:1/2` is the first word
    // with `/2` appended, and `!!:1h` keeps the letter with it.
    let mut history = History::new();
    history.push("echo zero one two three");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();

    for (typed, expected) in [
        ("!!:1/2", b"zero/2".as_slice()),
        ("!!:1.txt", b"zero.txt".as_slice()),
        ("!!:1,2", b"zero,2".as_slice()),
        ("!!:1]", b"zero]".as_slice()),
        ("!!:1=", b"zero=".as_slice()),
        ("!!:1/2/3", b"zero/2/3".as_slice()),
        ("!!:1h", b"zeroh".as_slice()),
        ("!!:1&", b"zero&".as_slice()),
    ] {
        assert_eq!(
            expand_history(typed.as_bytes(), &history, histchars, &policy, |_| false).unwrap(),
            expected.to_vec(),
            "{typed}"
        );
    }
}

#[test]
fn word_designator_forms_select_the_words_gnu_selects() {
    // The designator forms GNU Bash 5.3 accepts, with the words it selects
    // from the entry `echo zero one two three`.
    let mut history = History::new();
    history.push("echo zero one two three");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();

    for (typed, expected) in [
        ("!!:0", b"echo".as_slice()),
        ("!!:1", b"zero".as_slice()),
        ("!!:^", b"zero".as_slice()),
        ("!!:$", b"three".as_slice()),
        ("!!:*", b"zero one two three".as_slice()),
        ("!!:1-2", b"zero one".as_slice()),
        ("!!:1-", b"zero one two".as_slice()),
        ("!!:-2", b"echo zero one".as_slice()),
        ("!!:1*", b"zero one two three".as_slice()),
        ("!!:2*", b"one two three".as_slice()),
        ("!!:0*", b"echo zero one two three".as_slice()),
        ("!!:1-$", b"zero one two three".as_slice()),
        ("!!:1-^", b"zero".as_slice()),
        ("!!:-$", b"echo zero one two three".as_slice()),
        ("!!:-^", b"echo zero".as_slice()),
    ] {
        assert_eq!(
            expand_history(typed.as_bytes(), &history, histchars, &policy, |_| false).unwrap(),
            expected.to_vec(),
            "{typed}"
        );
    }
}

#[test]
fn history_expansion_honors_readline_backslash_inhibition() {
    let mut history = History::new();
    history.push("echo alpha");

    assert_eq!(
        expand_history(
            br"\!!",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        br"\!!".to_vec()
    );
    assert_eq!(
        expand_history(
            br"\\!!",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        br"\\echo alpha".to_vec()
    );
}

#[test]
fn history_quote_modifiers_apply_once_after_the_modifier_chain() {
    let mut history = History::new();
    history.push("echo one two");
    for (typed, expected) in [
        ("!!:q:q", b"'echo one two'".as_slice()),
        ("!!:x:x", b"'echo' 'one' 'two'".as_slice()),
        ("!!:q:x", b"'echo' 'one' 'two'".as_slice()),
        ("!!:x:q", b"'echo one two'".as_slice()),
        ("^one^new^:q:x", b"'echo' 'new' 'two'".as_slice()),
        ("^one^new^:x:q", b"'echo new two'".as_slice()),
    ] {
        assert_eq!(
            expand_history(
                typed.as_bytes(),
                &history,
                HistoryChars::parse("!^#"),
                &HistoryExpansionPolicy::default(),
                |_| false,
            )
            .unwrap(),
            expected,
            "{typed}"
        );
    }

    history.clear();
    history.push("echo /tmp/a.txt");
    for (typed, expected) in [
        ("!!:q:t", b"'a.txt'".as_slice()),
        ("!!:t:q", b"'a.txt'".as_slice()),
    ] {
        assert_eq!(
            expand_history(
                typed.as_bytes(),
                &history,
                HistoryChars::parse("!^#"),
                &HistoryExpansionPolicy::default(),
                |_| false,
            )
            .unwrap(),
            expected,
            "{typed}"
        );
    }
}

#[test]
fn history_x_modifier_preserves_separators_empty_fields_and_other_whitespace() {
    let mut history = History::new();
    let policy = HistoryExpansionPolicy::default();
    let histchars = HistoryChars::parse("!^#");
    let cases: &[(&[u8], &[u8], &[u8])] = &[
        (b"echo  one   two", b"!!:x", b"'echo' '' 'one' '' '' 'two'"),
        (b"  echo  one ", b"!!:x", b"'' '' 'echo' '' 'one' ''"),
        (
            b"echo\tone\t\ttwo\nlast",
            b"!!:x",
            b"'echo'\t'one'\t''\t'two'\n'last'",
        ),
        (
            b"echo\rone\x0c two\x0bone",
            b"!!:x",
            b"'echo\rone\x0c' 'two\x0bone'",
        ),
        (b"don't stop", b"!!:x", b"'don'\\''t' 'stop'"),
        (b"", b"!!:x", b"''"),
        (b"   ", b"!!:x", b"'' '' '' ''"),
        (
            b"echo  one   two",
            b"^one^new^:x",
            b"'echo' '' 'new' '' '' 'two'",
        ),
    ];

    for (entry, typed, expected) in cases {
        history.clear();
        history.push_bytes(entry.to_vec());
        assert_eq!(
            expand_history(typed, &history, histchars, &policy, |_| false).unwrap(),
            *expected,
            "entry={entry:?}, typed={typed:?}"
        );
    }
}

#[test]
fn history_expansion_preserves_quoted_history_words() {
    let mut history = History::new();
    history.push("printf \"two words\" $'ansi word' tail");

    assert_eq!(
        expand_history(
            b"!!:1",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        br#""two words""#.to_vec()
    );
    assert_eq!(
        expand_history(
            b"!!:2",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        b"$'ansi word'".to_vec()
    );
    assert_eq!(
        expand_history(
            b"!!:1:q",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        br#"'"two words"'"#.to_vec()
    );
    assert_eq!(
        expand_history(
            b"!!:1:x",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        b"'\"two' 'words\"'".to_vec()
    );

    history.clear();
    history
        .push("echo $foo foo=bar a@b a\\ b {a,b} $(printf hi) `printf hi` <(echo hi) >(cat) tail");
    for (typed, expected) in [
        ("!!:1", b"$foo".as_slice()),
        ("!!:2", b"foo=bar".as_slice()),
        ("!!:3", b"a@b".as_slice()),
        ("!!:4", b"a\\ b".as_slice()),
        ("!!:5", b"{a,b}".as_slice()),
        ("!!:6", b"$(printf hi)".as_slice()),
        ("!!:7", b"`printf hi`".as_slice()),
        ("!!:8", b"<(echo hi)".as_slice()),
        ("!!:9", b">(cat)".as_slice()),
    ] {
        assert_eq!(
            expand_history(
                typed.as_bytes(),
                &history,
                HistoryChars::parse("!^#"),
                &HistoryExpansionPolicy::default(),
                |_| false,
            )
            .unwrap(),
            expected.to_vec(),
            "{typed}"
        );
    }

    history.clear();
    history.push("echo arr=(one two) tail");
    for (typed, expected) in [
        ("!!:1", b"arr=".as_slice()),
        ("!!:2", b"(".as_slice()),
        ("!!:3", b"one".as_slice()),
        ("!!:4", b"two".as_slice()),
        ("!!:5", b")".as_slice()),
    ] {
        assert_eq!(
            expand_history(
                typed.as_bytes(),
                &history,
                HistoryChars::parse("!^#"),
                &HistoryExpansionPolicy::default(),
                |_| false,
            )
            .unwrap(),
            expected.to_vec(),
            "{typed}"
        );
    }

    history.clear();
    history.push("cat <in >out 2>&1 |& sed s/a/b/ && echo done");
    for (typed, expected) in [
        ("!!:1", b"<".as_slice()),
        ("!!:2", b"in".as_slice()),
        ("!!:3", b">".as_slice()),
        ("!!:4", b"out".as_slice()),
        ("!!:5", b"2>&1".as_slice()),
        ("!!:6", b"|".as_slice()),
        ("!!:7", b"&".as_slice()),
        ("!!:8", b"sed".as_slice()),
        ("!!:9", b"s/a/b/".as_slice()),
        ("!!:10", b"&&".as_slice()),
        ("!!:11", b"echo".as_slice()),
    ] {
        assert_eq!(
            expand_history(
                typed.as_bytes(),
                &history,
                HistoryChars::parse("!^#"),
                &HistoryExpansionPolicy::default(),
                |_| false,
            )
            .unwrap(),
            expected.to_vec(),
            "{typed}"
        );
    }

    history.clear();
    history.push("cmd 2>file 12>>file 3<in 4<&0 >&2 <&0 &>>file ;& ;;& <>rw");
    for (typed, expected) in [
        ("!!:1", b"2>".as_slice()),
        ("!!:2", b"file".as_slice()),
        ("!!:3", b"12>>".as_slice()),
        ("!!:4", b"file".as_slice()),
        ("!!:5", b"3<".as_slice()),
        ("!!:6", b"in".as_slice()),
        ("!!:7", b"4<&0".as_slice()),
        ("!!:8", b">&2".as_slice()),
        ("!!:9", b"<&0".as_slice()),
        ("!!:10", b"&>".as_slice()),
        ("!!:11", b">".as_slice()),
        ("!!:12", b"file".as_slice()),
        ("!!:13", b";".as_slice()),
        ("!!:14", b"&".as_slice()),
        ("!!:15", b";;".as_slice()),
        ("!!:16", b"&".as_slice()),
        ("!!:17", b"<".as_slice()),
        ("!!:18", b">".as_slice()),
        ("!!:19", b"rw".as_slice()),
    ] {
        assert_eq!(
            expand_history(
                typed.as_bytes(),
                &history,
                HistoryChars::parse("!^#"),
                &HistoryExpansionPolicy::default(),
                |_| false,
            )
            .unwrap(),
            expected.to_vec(),
            "{typed}"
        );
    }
}

#[test]
fn custom_history_word_delimiters_preserve_punctuation_fields() {
    let policy = HistoryExpansionPolicy {
        word_delimiters: b" \t\n;&()|<>,".to_vec(),
        ..HistoryExpansionPolicy::default()
    };
    // Paired with history_expand_probe's default/comma delimiter rows.
    // The selected punctuation field is a word, not a discarded separator.
    let mut history = History::new();
    history.push("echo alpha,beta gamma");
    for (selected_policy, expected) in [
        (&HistoryExpansionPolicy::default(), b"gamma".as_slice()),
        (&policy, b",".as_slice()),
    ] {
        let expanded = expand_history_with_status(
            b"!!:2",
            &history,
            HistoryChars::parse("!^#"),
            selected_policy,
            |_| false,
        )
        .unwrap();
        assert_eq!(expanded.line, expected);
        assert_eq!(expanded.status, HistoryExpansionStatus::Expanded);
    }

    for (line, expected) in [
        (
            b"echo alpha,beta gamma".as_slice(),
            vec![b"echo".as_slice(), b"alpha", b",", b"beta", b"gamma"],
        ),
        (
            b"echo alpha,,beta".as_slice(),
            vec![b"echo".as_slice(), b"alpha", b",,", b"beta"],
        ),
        (
            b"echo alpha,;beta".as_slice(),
            vec![b"echo".as_slice(), b"alpha", b",;", b"beta"],
        ),
        (
            b"echo alpha, & beta".as_slice(),
            vec![b"echo".as_slice(), b"alpha", b", & ", b"beta"],
        ),
        (
            b"echo alpha, beta".as_slice(),
            vec![b"echo".as_slice(), b"alpha", b", ", b"beta"],
        ),
        (
            b"echo alpha,beta,".as_slice(),
            vec![b"echo".as_slice(), b"alpha", b",", b"beta", b","],
        ),
        (
            b"echo 'alpha,beta' escaped\\,comma".as_slice(),
            vec![b"echo".as_slice(), b"'alpha,beta'", b"escaped\\,comma"],
        ),
        (
            b"cmd,2>&1 | echo out,".as_slice(),
            vec![
                b"cmd".as_slice(),
                b",",
                b"2>&1",
                b"|",
                b"echo",
                b"out",
                b",",
            ],
        ),
        (
            b"echo ; 2>&1".as_slice(),
            vec![b"echo".as_slice(), b";", b"2>&1"],
        ),
    ] {
        assert_eq!(
            history_tokenize(line, &policy),
            expected.into_iter().map(<[u8]>::to_vec).collect::<Vec<_>>(),
            "{}",
            String::from_utf8_lossy(line)
        );
    }

    // With punctuation as the only configured delimiter, whitespace after
    // the comma is skipped rather than absorbed into its field.
    let punctuation_only = HistoryExpansionPolicy {
        word_delimiters: b",".to_vec(),
        ..HistoryExpansionPolicy::default()
    };
    assert_eq!(
        history_tokenize(b"echo alpha, beta", &punctuation_only),
        vec![
            b"echo".to_vec(),
            b"alpha".to_vec(),
            b",".to_vec(),
            b"beta".to_vec()
        ],
    );

    let mut history = History::new();
    history.push("echo alpha,beta gamma");
    let histchars = HistoryChars::parse("!^#");
    assert_eq!(
        expand_history(b"!!:2", &history, histchars, &policy, |_| false).unwrap(),
        b",".to_vec(),
    );
    assert_eq!(
        expand_history(b"!?,?:%", &history, histchars, &policy, |_| false).unwrap(),
        b",".to_vec(),
    );
}

#[test]
fn history_tokenize_uses_gnu_history_default_whitespace_classes() {
    let policy = HistoryExpansionPolicy::default();
    for (line, expected) in [
        (
            b"echo a\x0cb".as_slice(),
            vec![b"echo".as_slice(), b"a\x0cb".as_slice()],
        ),
        (
            b"echo a\rb".as_slice(),
            vec![b"echo".as_slice(), b"a\rb".as_slice()],
        ),
        (
            b"echo a\x0bb".as_slice(),
            vec![b"echo".as_slice(), b"a\x0bb".as_slice()],
        ),
        (b"echo a b".as_slice(), vec![b"echo".as_slice(), b"a", b"b"]),
        (
            b"echo a\tb".as_slice(),
            vec![b"echo".as_slice(), b"a", b"b"],
        ),
        (
            b"echo a\nb".as_slice(),
            vec![b"echo".as_slice(), b"a", b"b"],
        ),
    ] {
        assert_eq!(
            history_tokenize(line, &policy),
            expected.into_iter().map(<[u8]>::to_vec).collect::<Vec<_>>(),
            "line={line:?}"
        );
    }

    let configurable_form_feed = HistoryExpansionPolicy {
        word_delimiters: b"\x0c".to_vec(),
        ..policy.clone()
    };
    assert_eq!(
        history_tokenize(b"echo a\x0cb", &configurable_form_feed),
        vec![
            b"echo".to_vec(),
            b"a".to_vec(),
            b"\x0c".to_vec(),
            b"b".to_vec()
        ]
    );

    let event = b"echo a\x0cb other";
    let mut history = History::new();
    history.push(std::str::from_utf8(event).unwrap());
    assert_eq!(
        expand_history(
            b"!!:1",
            &history,
            HistoryChars::parse("!^#"),
            &policy,
            |_| false,
        )
        .unwrap(),
        b"a\x0cb"
    );
    assert_eq!(
        history_arg_extract(1, 1, event, &policy),
        Some(b"a\x0cb".to_vec())
    );
}

#[test]
fn history_tokenize_retains_comment_looking_words() {
    let policy = HistoryExpansionPolicy::default();
    for (line, expected) in [
        (
            b"echo #comment trailing".as_slice(),
            vec![b"echo".as_slice(), b"#comment", b"trailing"],
        ),
        (
            b"#comment trailing".as_slice(),
            vec![b"#comment".as_slice(), b"trailing"],
        ),
        (
            b"echo foo#bar tail".as_slice(),
            vec![b"echo".as_slice(), b"foo#bar", b"tail"],
        ),
    ] {
        assert_eq!(
            history_tokenize(line, &policy),
            expected.into_iter().map(<[u8]>::to_vec).collect::<Vec<_>>(),
            "line={line:?}"
        );
    }

    let event = b"echo #comment trailing";
    let mut history = History::new();
    history.push(std::str::from_utf8(event).unwrap());
    assert_eq!(
        expand_history(
            b"!!:1",
            &history,
            HistoryChars::parse("!^#"),
            &policy,
            |_| false,
        )
        .unwrap(),
        b"#comment"
    );
    assert_eq!(
        history_arg_extract(1, 2, event, &policy),
        Some(b"#comment trailing".to_vec())
    );
}

#[test]
fn history_tokenize_splits_parameter_and_arithmetic_expansions_at_spaces() {
    let policy = HistoryExpansionPolicy::default();
    for (line, expected) in [
        (
            b"echo ${v:-a b} z".as_slice(),
            vec![b"echo".as_slice(), b"${v:-a", b"b}", b"z"],
        ),
        (
            b"echo $[1 + 2] z".as_slice(),
            vec![b"echo".as_slice(), b"$[1", b"+", b"2]", b"z"],
        ),
        (
            b"echo ${v:-${w:-a b}} z".as_slice(),
            vec![b"echo".as_slice(), b"${v:-${w:-a", b"b}}", b"z"],
        ),
        (
            b"echo $(echo a b) z".as_slice(),
            vec![b"echo".as_slice(), b"$(echo a b)", b"z"],
        ),
        (
            b"echo <(echo a b) z".as_slice(),
            vec![b"echo".as_slice(), b"<(echo a b)", b"z"],
        ),
    ] {
        assert_eq!(
            history_tokenize(line, &policy),
            expected.into_iter().map(<[u8]>::to_vec).collect::<Vec<_>>(),
            "line={line:?}"
        );
    }

    let event = b"echo ${v:-a b} z";
    let mut history = History::new();
    history.push(std::str::from_utf8(event).unwrap());
    assert_eq!(
        expand_history(
            b"!!:1",
            &history,
            HistoryChars::parse("!^#"),
            &policy,
            |_| false,
        )
        .unwrap(),
        b"${v:-a"
    );
    assert_eq!(
        history_arg_extract(1, 2, event, &policy),
        Some(b"${v:-a b}".to_vec())
    );
}

#[test]
fn history_tokenize_matches_gnu_nested_command_delimiter_scanning() {
    let policy = HistoryExpansionPolicy::default();
    for (line, expected) in [
        (
            br#"echo $(echo ")" a) z"#.as_slice(),
            vec![b"echo".as_slice(), br#"$(echo ")" a) z"#],
        ),
        (
            br#"echo $(echo `printf ")"` a) z"#.as_slice(),
            vec![b"echo".as_slice(), br#"$(echo `printf ")"` a) z"#],
        ),
        (
            br#"echo $(echo "a b") z"#.as_slice(),
            vec![b"echo".as_slice(), br#"$(echo "a b")"#, b"z"],
        ),
        (
            b"echo $(echo a b) z".as_slice(),
            vec![b"echo".as_slice(), b"$(echo a b)", b"z"],
        ),
        (
            b"echo <(echo a b) z".as_slice(),
            vec![b"echo".as_slice(), b"<(echo a b)", b"z"],
        ),
        (
            b"echo `echo a b` z".as_slice(),
            vec![b"echo".as_slice(), b"`echo a b`", b"z"],
        ),
    ] {
        assert_eq!(
            history_tokenize(line, &policy),
            expected.into_iter().map(<[u8]>::to_vec).collect::<Vec<_>>(),
            "line={line:?}"
        );
    }

    let event = br#"echo $(echo `printf ")"` a) z"#;
    let mut history = History::new();
    history.push(std::str::from_utf8(event).unwrap());
    assert_eq!(
        expand_history(
            b"!!:1",
            &history,
            HistoryChars::parse("!^#"),
            &policy,
            |_| false,
        )
        .unwrap(),
        br#"$(echo `printf ")"` a) z"#.to_vec()
    );
    assert_eq!(
        history_arg_extract(1, 1, event, &policy),
        Some(br#"$(echo `printf ")"` a) z"#.to_vec())
    );
}

#[test]
fn history_extended_glob_word_selection_matches_pinned_history_83() {
    // Pinned GNU History 8.3 keeps these unquoted, balanced forms together.
    // Quoted, escaped, command/process substitution, and arithmetic cases are
    // controls; this checks lexical word behavior, not glob expansion.
    let policy = HistoryExpansionPolicy::default();
    type LexicalCase = (&'static [u8], &'static [&'static [u8]], &'static [u8]);
    let cases: &[LexicalCase] = &[
        (b"echo @(a b) z", &[b"echo", b"@(a b)", b"z"], b"@(a b)"),
        (b"echo !(a b) z", &[b"echo", b"!(a b)", b"z"], b"!(a b)"),
        (b"echo ?(a b) z", &[b"echo", b"?(a b)", b"z"], b"?(a b)"),
        (b"echo +(a b) z", &[b"echo", b"+(a b)", b"z"], b"+(a b)"),
        (b"echo *(a b) z", &[b"echo", b"*(a b)", b"z"], b"*(a b)"),
        (
            b"echo @((a b) c) z",
            &[b"echo", b"@((a b) c)", b"z"],
            b"@((a b) c)",
        ),
        (
            b"echo '@(a b)' z",
            &[b"echo", b"'@(a b)'", b"z"],
            b"'@(a b)'",
        ),
        (
            b"echo \\@(a b) z",
            &[b"echo", b"\\@", b"(", b"a", b"b", b")", b"z"],
            b"\\@",
        ),
        (b"echo $(a b) z", &[b"echo", b"$(a b)", b"z"], b"$(a b)"),
        (b"echo <(a b) z", &[b"echo", b"<(a b)", b"z"], b"<(a b)"),
        (
            b"echo $((1 + 2)) z",
            &[b"echo", b"$((1 + 2))", b"z"],
            b"$((1 + 2))",
        ),
    ];
    for (input, expected_tokens, expected_arg) in cases {
        assert_eq!(
            history_tokenize(input, &policy),
            expected_tokens
                .iter()
                .map(|token| token.to_vec())
                .collect::<Vec<_>>(),
            "tokenization for {input:?}"
        );
        assert_eq!(
            history_arg_extract(1, 1, input, &policy).as_deref(),
            Some(*expected_arg),
            "argument extraction for {input:?}"
        );
        let mut history = History::new();
        history.push(std::str::from_utf8(input).unwrap());
        assert_eq!(
            expand_history(
                b"!!:1",
                &history,
                HistoryChars::parse("!^#"),
                &policy,
                |_| false,
            )
            .unwrap(),
            *expected_arg,
            "word-selection expansion for {input:?}"
        );
    }
}

#[test]
fn history_tokenize_keeps_here_string_redirections_together() {
    let policy = HistoryExpansionPolicy::default();
    assert_eq!(
        history_tokenize(b"echo 3<<<value", &policy),
        vec![b"echo".to_vec(), b"3<<<".to_vec(), b"value".to_vec()],
    );
    assert_eq!(
        history_tokenize(b"echo <<<value", &policy),
        vec![b"echo".to_vec(), b"<<<".to_vec(), b"value".to_vec()],
    );
    // GNU libhistory experiment 2d4373f5f4eb4578b36f89fbcbcd9cc1 keeps
    // an ampersand following the operator outside the `<<<` redirection word.
    assert_eq!(
        history_tokenize(b"echo 3<<<&1", &policy),
        vec![
            b"echo".to_vec(),
            b"3<<<".to_vec(),
            b"&".to_vec(),
            b"1".to_vec()
        ],
    );

    let mut history = History::new();
    history.push("echo 3<<<value tail");
    let histchars = HistoryChars::parse("!^#");
    for (typed, expected) in [
        ("!!:1", b"3<<<".as_slice()),
        ("!!:2", b"value".as_slice()),
        ("!!:$", b"tail".as_slice()),
    ] {
        assert_eq!(
            expand_history(typed.as_bytes(), &history, histchars, &policy, |_| false,).unwrap(),
            expected.to_vec(),
            "{typed}"
        );
    }
    assert_eq!(
        history_arg_extract(1, 2, b"echo 3<<<value tail", &policy),
        Some(b"3<<< value".to_vec()),
    );
}

#[test]
fn history_expansion_exposes_helper_functions() {
    let policy = HistoryExpansionPolicy::default();
    let line = b"echo arr=(one two) | sed 's/ /_/g'";

    assert_eq!(
        history_tokenize(line, &policy),
        vec![
            b"echo".to_vec(),
            b"arr=".to_vec(),
            b"(".to_vec(),
            b"one".to_vec(),
            b"two".to_vec(),
            b")".to_vec(),
            b"|".to_vec(),
            b"sed".to_vec(),
            b"'s/ /_/g'".to_vec(),
        ]
    );
    assert_eq!(
        history_arg_extract(2, 5, line, &policy),
        Some(b"( one two )".to_vec())
    );
    assert_eq!(history_arg_extract(4, 2, line, &policy), None);

    let mut history = History::new();
    history.push("printf alpha beta");
    let event = get_history_event(b"!$", &history, HistoryChars::parse("!^#"), &policy)
        .unwrap()
        .expect("event");
    assert_eq!(event.line, b"beta");
    assert_eq!(event.next_index, 2);
    assert_eq!(event.matched_word, None);
}

#[test]
fn explicit_empty_entries_affect_event_expansion_and_stifling() {
    let mut history = History::new();
    history.push("first");
    history.push("");
    assert_eq!(history.len(), 2);
    assert_eq!(history.entries()[0].line_bytes, b"first");
    assert!(history.entries()[1].line_bytes.is_empty());
    assert_eq!(
        expand_history(
            b"!!",
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
        Vec::<u8>::new()
    );
    assert!(history.add_time("#123"));
    assert_eq!(history.entries()[1].timestamp.as_deref(), Some("#123"));

    history.push_bytes(Vec::new());
    assert_eq!(history.len(), 3);
    assert!(history.entries()[2].line_bytes.is_empty());
    assert!(history.add_time("#456"));
    assert_eq!(history.entries()[2].timestamp.as_deref(), Some("#456"));

    let mut bounded = History::new();
    bounded.stifle(2);
    bounded.push("first");
    bounded.push("");
    bounded.push("last");
    assert_eq!(
        bounded
            .entries()
            .iter()
            .map(|entry| entry.line_bytes.clone())
            .collect::<Vec<_>>(),
        vec![Vec::new(), b"last".to_vec()]
    );

    let mut zero = History::new();
    zero.stifle(0);
    zero.push("");
    assert!(zero.is_empty());
}

#[test]
fn supports_history_library_state_and_stifle_operations() {
    let mut h = History::new();
    h.push("one");
    h.push("two");
    h.push("three");
    assert!(h.add_time("#1700000000"));
    assert_eq!(h.entries()[2].timestamp.as_deref(), Some("#1700000000"));
    assert_eq!(
        h.total_bytes(),
        "one".len() + "two".len() + "three".len() + "#1700000000".len()
    );

    h.stifle(2);
    assert!(h.is_stifled());
    assert_eq!(h.max_entries(), Some(2));
    assert_eq!(h.len(), 2);
    assert_eq!(h.get(0).map(|entry| entry.line()), Some("two".into()));
    let state = h.state();
    assert_eq!(state.offset, 0);
    assert_eq!(state.length, 2);
    assert!(state.size >= state.length);
    assert!(state.stifled);
    assert_eq!(state.max_entries, Some(2));
    assert_eq!(h.unstifle(), Some(2));
    assert!(!h.is_stifled());

    let old = h.replace(1, "THREE").unwrap();
    assert_eq!(old.line(), "three");
    assert!(h.entries()[1].modified);
    let removed = h.remove(0).unwrap();
    assert_eq!(removed.line(), "two");
    assert_eq!(h.get(0).map(|entry| entry.line()), Some("THREE".into()));
    h.clear();
    assert!(h.is_empty());
}

#[test]
fn total_bytes_counts_resident_timestamps() {
    let mut history = History::new();
    assert_eq!(history.total_bytes(), 0);

    // A line without timestamp counts only its own bytes.
    history.push("one");
    assert_eq!(history.total_bytes(), 3);

    // An explicitly empty timestamp adds nothing.
    assert!(history.add_time(""));
    assert_eq!(history.total_bytes(), 3);

    // An explicit timestamp adds its UTF-8 byte length.
    assert!(history.add_time("#123"));
    assert_eq!(history.total_bytes(), 3 + 4);

    // Multibyte lines count bytes, and empty timestamps stay free.
    history.push("\u{e9}");
    assert!(history.add_time(""));
    assert_eq!(history.total_bytes(), 3 + 4 + 2);

    assert!(history.add_time("#-45"));
    assert_eq!(history.total_bytes(), 3 + 4 + 2 + 4);

    // Replacement keeps the timestamp and recounts the new line.
    history.replace(0, "changed").unwrap();
    assert_eq!(history.total_bytes(), 7 + 4 + 2 + 4);

    // Removal subtracts the entry, clear returns to zero.
    history.remove(1).unwrap();
    assert_eq!(history.total_bytes(), 7 + 4);
    history.remove(0).unwrap();
    assert_eq!(history.total_bytes(), 0);
    history.push("last");
    history.clear();
    assert_eq!(history.total_bytes(), 0);

    history.push("aaa");
    history.push("bbbb");
    history.push("cc");
    assert_eq!(history.total_bytes(), 9);
    history.stifle(1);
    assert_eq!(history.entries()[0].line(), "cc");
    assert_eq!(history.total_bytes(), 2);
}
#[test]
fn set_line_bytes_preserves_metadata_and_marks_only_changed_content() {
    let mut history = History::new();
    history.push("old timed");
    assert!(history.add_time("#1700000000"));
    let undo = vec![HistoryUndoEntry {
        start: 1,
        deleted: b"old".to_vec(),
        inserted: b"new".to_vec(),
    }];
    assert!(history.set_undo_list(0, undo.clone()));

    assert!(history.set_line_bytes(0, b"old timed".to_vec()));
    assert!(!history.entries()[0].modified);
    assert!(history.set_line_bytes(0, b"new timed".to_vec()));
    assert!(history.entries()[0].modified);
    assert_eq!(history.entries()[0].line_bytes, b"new timed");
    assert_eq!(
        history.entries()[0].timestamp.as_deref(),
        Some("#1700000000")
    );
    assert_eq!(history.undo_list(0), Some(undo.as_slice()));
    assert!(!history.set_line_bytes(1, b"missing".to_vec()));
}

#[test]
fn replace_preserves_timestamps_undo_history_and_timestamped_serialization() {
    let mut history = History::new();
    history.push("old timed");
    assert!(history.add_time("#1700000000"));
    history.push("old untimed");
    let undo = vec![HistoryUndoEntry {
        start: 1,
        deleted: b"old".to_vec(),
        inserted: b"new".to_vec(),
    }];
    assert!(history.set_undo_list(0, undo.clone()));

    let previous = history.replace(0, "new timed").unwrap();
    assert_eq!(previous.line_bytes, b"old timed");
    assert_eq!(previous.timestamp.as_deref(), Some("#1700000000"));
    assert!(!previous.modified);
    assert_eq!(previous.undo_list, undo);
    assert_eq!(history.entries()[0].line_bytes, b"new timed");
    assert_eq!(
        history.entries()[0].timestamp.as_deref(),
        Some("#1700000000")
    );
    assert!(history.entries()[0].modified);
    assert_eq!(history.undo_list(0), Some(undo.as_slice()));

    let previous = history.replace(1, "new untimed").unwrap();
    assert_eq!(previous.line_bytes, b"old untimed");
    assert_eq!(previous.timestamp, None);
    assert_eq!(history.entries()[1].timestamp, None);
    assert!(history.entries()[1].modified);

    assert!(history.replace(2, "invalid").is_none());
    assert_eq!(history.entries().len(), 2);
    assert_eq!(history.entries()[1].line_bytes, b"new untimed");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    history.write_file_with_timestamps(&path, true).unwrap();
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "#1700000000\nnew timed\nnew untimed\n"
    );
}

// The pinned GNU 8.3 position fixture covers selected cases in this matrix.
#[test]
fn replacement_preserves_numeric_position_for_valid_and_invalid_indexes() {
    for (stifled, positions, indexes) in [
        (false, &[0, 1, 2, 3, 4][..], &[0, 1, 2, 3, 4][..]),
        (true, &[1, 2, 4][..], &[0, 1, 2][..]),
    ] {
        for &pos in positions {
            for &index in indexes {
                let mut history = History::new();
                for line in ["A", "B", "C", "D"] {
                    history.push(line);
                }
                assert!(history.set_pos(pos));
                if stifled {
                    history.stifle(2);
                }
                let mut expected = if stifled {
                    vec!["C", "D"]
                } else {
                    vec!["A", "B", "C", "D"]
                };
                let old = expected.get(index).copied();
                let previous = history.replace(index, "R");
                assert_eq!(previous.as_ref().map(|entry| entry.line()).as_deref(), old);
                if index < expected.len() {
                    expected[index] = "R";
                }
                assert_eq!(
                    history.where_history(),
                    pos,
                    "stifled={stifled} pos={pos} index={index}"
                );
                assert_eq!(
                    history
                        .current_history()
                        .map(|entry| entry.line())
                        .as_deref(),
                    expected.get(pos).copied()
                );
                assert_eq!(
                    history
                        .entries()
                        .iter()
                        .map(|entry| entry.line())
                        .collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }
}

#[test]
fn replace_preserves_cursor_and_invalid_remove_is_noop() {
    let mut history = History::new();
    history.push("first");
    history.push("second");

    // Replacing the positioned entry keeps the cursor in place.
    assert!(history.set_pos(0));
    let previous = history.replace(0, "changed").unwrap();
    assert_eq!(previous.line_bytes, b"first");
    assert_eq!(history.where_history(), 0);
    assert_eq!(history.current_history().unwrap().line_bytes, b"changed");
    assert_eq!(history.next_history().unwrap().line_bytes, b"second");
    assert_eq!(history.previous_history().unwrap().line_bytes, b"changed");

    // Replacing the middle entry keeps a middle cursor too.
    history.push("third");
    assert!(history.set_pos(1));
    history.replace(1, "middle").unwrap();
    assert_eq!(history.where_history(), 1);
    assert_eq!(history.current_history().unwrap().line_bytes, b"middle");

    // Removing an invalid index changes nothing, not even the cursor.
    assert!(history.set_pos(0));
    assert!(history.remove(99).is_none());
    assert_eq!(history.where_history(), 0);
    assert_eq!(history.current_history().unwrap().line_bytes, b"changed");
    assert_eq!(history.entries().len(), 3);
}

#[test]
fn supports_history_library_position_and_search_operations() {
    let mut h = History::new();
    h.push("alpha one");
    h.push("beta two");
    h.push("alpha three");
    h.reset_cursor();

    assert_eq!(h.where_history(), 3);
    assert!(h.set_pos(2));
    assert_eq!(
        h.current_history().map(|entry| entry.line()),
        Some("alpha three".into())
    );
    assert_eq!(
        h.previous_history().map(|entry| entry.line()),
        Some("beta two".into())
    );
    assert_eq!(
        h.next_history().map(|entry| entry.line()),
        Some("alpha three".into())
    );
    assert!(h.next_history().is_none());
    assert_eq!(h.where_history(), 3);
    assert!(!h.set_pos(4));

    let found = h
        .history_search_bytes(b"two", HistoryDirection::Previous)
        .expect("backward search");
    assert_eq!(found.entry_index, 1);
    assert_eq!(found.byte_offset, 5);
    assert_eq!(
        h.current_history().map(|entry| entry.line()),
        Some("beta two".into())
    );

    let prefix = h
        .history_search_prefix("alpha", HistoryDirection::Next)
        .expect("forward prefix search");
    assert_eq!(prefix.entry_index, 2);
    assert_eq!(prefix.byte_offset, 0);

    let pos = h
        .history_search_pos("one", HistoryDirection::Previous, 2)
        .expect("search from position");
    assert_eq!(pos.entry_index, 0);
    assert_eq!(pos.line_bytes, b"alpha one");
}

#[test]
fn quick_substitution_is_inhibited_by_single_quote_state() {
    let mut history = History::new();
    history.push("echo alpha");
    let histchars = HistoryChars::parse("!^#");

    // Baseline: no quote state expands, with and without trailing `^`.
    assert_eq!(
        expand_history(
            b"^alpha^beta^",
            &history,
            histchars,
            &HistoryExpansionPolicy::default(),
            |_| false
        )
        .unwrap(),
        b"echo beta".to_vec()
    );
    assert_eq!(
        expand_history(
            b"^alpha^beta",
            &history,
            histchars,
            &HistoryExpansionPolicy::default(),
            |_| false
        )
        .unwrap(),
        b"echo beta".to_vec()
    );

    // GNU Readline 8.3: single-quote state inhibits quick substitution even
    // when `quotes_inhibit_expansion` is off; the line passes through unchanged.
    for quotes_inhibit in [false, true] {
        let policy = HistoryExpansionPolicy {
            quote_state: Some(b'\''),
            quotes_inhibit_expansion: quotes_inhibit,
            ..HistoryExpansionPolicy::default()
        };
        for line in [b"^alpha^beta^".as_slice(), b"^alpha^beta".as_slice()] {
            assert_eq!(
                expand_history(line, &history, histchars, &policy, |_| false).unwrap(),
                line.to_vec(),
                "quotes_inhibit={quotes_inhibit}",
            );
        }
    }

    // Double-quote state does not inhibit quick substitution.
    let policy = HistoryExpansionPolicy {
        quote_state: Some(b'"'),
        ..HistoryExpansionPolicy::default()
    };
    assert_eq!(
        expand_history(b"^alpha^beta^", &history, histchars, &policy, |_| false).unwrap(),
        b"echo beta".to_vec()
    );
}

#[test]
fn quick_substitution_processes_suffix_modifiers_and_further_expansion() {
    // GNU treats `^old^new^` as `!!:s^old^new^`: the suffix after the closing
    // marker flows through modifier and history expansion handling.
    let mut history = History::new();
    history.push("echo old");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();

    for (typed, expected) in [
        ("^old^new^ trailing", b"echo new trailing".as_slice()),
        ("^old^new^:q trailing", b"'echo new' trailing".as_slice()),
        (
            "^old^new^:s/new/final/ trailing",
            b"echo final trailing".as_slice(),
        ),
        ("^old^new^; !!", b"echo new; echo old".as_slice()),
        (
            "^old^new^ !!:s/old/final/",
            b"echo new echo final".as_slice(),
        ),
    ] {
        assert_eq!(
            expand_history(typed.as_bytes(), &history, histchars, &policy, |_| false).unwrap(),
            expected.to_vec(),
            "{typed}"
        );
    }

    let printed =
        expand_history_with_status(b"^old^new^:p", &history, histchars, &policy, |_| false)
            .unwrap();
    assert_eq!(printed.line, b"echo new");
    assert_eq!(printed.status, HistoryExpansionStatus::PrintOnly);

    let printed_trailing = expand_history_with_status(
        b"^old^new^:p trailing",
        &history,
        histchars,
        &policy,
        |_| false,
    )
    .unwrap();
    assert_eq!(printed_trailing.line, b"echo new trailing");
    assert_eq!(printed_trailing.status, HistoryExpansionStatus::PrintOnly);

    assert_eq!(
        expand_history(
            b"^missing^new^ trailing",
            &history,
            histchars,
            &policy,
            |_| false
        ),
        Err(HistoryExpansionError::SubstitutionFailed),
    );
}

#[test]
fn quick_substitution_honors_escaped_delimiters() {
    // GNU libhistory experiment 399d1d1b3a4a41e8a28adb79d1c13546: only a
    // backslash followed by the active quick marker escapes the delimiter and
    // emits the marker as field data; other backslashes are preserved.
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();

    let mut history = History::new();
    history.push("echo old^part");
    assert_eq!(
        expand_history(b"^old\\^part^new^", &history, histchars, &policy, |_| false).unwrap(),
        b"echo new".to_vec(),
    );

    let mut history = History::new();
    history.push("echo old");
    assert_eq!(
        expand_history(b"^old^new\\^part^", &history, histchars, &policy, |_| false).unwrap(),
        b"echo new^part".to_vec(),
    );

    // GNU libhistory experiment 2aae18ed91614a5084148ff5a7873df9 confirms
    // backslashes before ordinary bytes remain data in either field.
    let mut history = History::new();
    history.push("echo old");
    assert_eq!(
        expand_history(br"^old^new\z^", &history, histchars, &policy, |_| false).unwrap(),
        br"echo new\z".to_vec(),
    );

    let mut history = History::new();
    history.push(r"echo f\oo");
    assert_eq!(
        expand_history(br"^f\oo^BAR^", &history, histchars, &policy, |_| false).unwrap(),
        b"echo BAR".to_vec(),
    );
}

#[test]
fn quick_substitution_expands_and_escapes_replacement_ampersands() {
    // Pinned GNU History 8.3 primary fixture: quick and ordinary substitution
    // both expand `&` to the old field, while `\&` is literal; the expanded
    // replacement is also reused by a following `!!:&`.
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut history = History::new();
    history.push("echo old");

    for (typed, expected) in [
        (b"^old^&tail^".as_slice(), b"echo oldtail".as_slice()),
        (b"!!:s^old^&tail^".as_slice(), b"echo oldtail".as_slice()),
        (b"^old^\\&tail^".as_slice(), b"echo &tail".as_slice()),
        (b"!!:s^old^\\&tail^".as_slice(), b"echo &tail".as_slice()),
    ] {
        assert_eq!(
            expand_history(typed, &history, histchars, &policy, |_| false).unwrap(),
            expected,
            "typed={typed:?}",
        );
    }

    let mut state = HistoryExpansionState::new();
    assert_eq!(
        expand_history_with_state(
            b"^old^&tail^",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        )
        .unwrap(),
        b"echo oldtail",
    );
    assert_eq!(
        expand_history_with_state(b"!!:&", &history, histchars, &policy, &mut state, |_| false,)
            .unwrap(),
        b"echo oldtail",
    );
}

#[test]
fn timestamped_reads_strip_leading_blanks_and_drop_blank_only_gaps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");

    // Leading blank after a timestamp is stripped like Bash.
    std::fs::write(&path, "#1700000000\n\nfoo\n#1700000001\nbar\n").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (Some("#1700000000"), "foo".to_string()),
            (Some("#1700000001"), "bar".to_string()),
        ]
    );

    // A gap of only blank lines between timestamps drops the phantom entry.
    std::fs::write(&path, "#1700000000\n\n\n#1700000001\nfoo\n").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#1700000001"), "foo".to_string()),]
    );

    // Internal and trailing blanks are still preserved by the joined grouping.
    std::fs::write(&path, "#1700000000\nline one\n\nline two\n").unwrap();
    let joined = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(joined.entries()[0].line_bytes, b"line one\n\nline two");

    // The default grouping drops the blank line and splits the block.
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(loaded.entries().len(), 2);
    assert_eq!(loaded.entries()[0].line_bytes, b"line one");
    assert_eq!(loaded.entries()[1].line_bytes, b"line two");
}

#[test]
fn ignores_unterminated_trailing_fragment_like_gnu() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");

    // Newline-terminated controls load every line.
    std::fs::write(&path, "one\ntwo\n").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["one".to_string(), "two".to_string()]
    );

    // A trailing fragment without LF is ignored; completed lines stay.
    std::fs::write(&path, "one\ntwo").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["one".to_string()]
    );

    // A single unterminated line loads nothing.
    std::fs::write(&path, "two").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert!(loaded.entries().is_empty());

    // Timestamped control keeps one entry per physical line by default.
    std::fs::write(&path, "#123\none\ntwo\n").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#123"), "one".to_string()), (None, "two".to_string()),]
    );

    // The joined grouping joins the multiline entry.
    let joined = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#123"), "one\ntwo".to_string()),]
    );

    // An unterminated trailing line drops out of the delimited entry.
    std::fs::write(&path, "#123\none\ntwo").unwrap();
    let loaded = History::read_file(&path).unwrap();
    assert_eq!(
        loaded
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#123"), "one".to_string()),]
    );
    let joined = History::read_file_with_multiline(&path, HistoryMultiline::Joined).unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![(Some("#123"), "one".to_string()),]
    );
}

#[test]
fn remove_adjusts_append_new_boundary_for_loaded_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    std::fs::write(&path, "old1\nold2\n").unwrap();

    // Removing a loaded entry keeps the surviving unsaved entry in append-new.
    let mut loaded = History::read_file(&path).unwrap();
    loaded.push("unsaved");
    assert!(loaded.remove(0).is_some());
    loaded.push("fresh");
    loaded.append_new_to_file(&path).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "old1\nold2\nunsaved\nfresh\n"
    );

    // Control: removing the unsaved entry appends only the later entry.
    std::fs::write(&path, "old1\nold2\n").unwrap();
    let mut loaded = History::read_file(&path).unwrap();
    loaded.push("unsaved");
    assert!(loaded.remove(2).is_some());
    loaded.push("fresh");
    loaded.append_new_to_file(&path).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "old1\nold2\nfresh\n"
    );

    // Invalid removal appends everything after the loaded prefix.
    std::fs::write(&path, "old1\nold2\n").unwrap();
    let mut loaded = History::read_file(&path).unwrap();
    loaded.push("unsaved");
    assert!(loaded.remove(9).is_none());
    loaded.push("fresh");
    loaded.append_new_to_file(&path).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "old1\nold2\nunsaved\nfresh\n"
    );
}

#[test]
fn history_expansion_retains_substring_search_and_matched_word_across_calls() {
    // GNU most-recent-search memory: `!??` reuses the previous `!?` needle,
    // `%` reuses its matched word, and empty-old `s//new/` falls back to the
    // search when no substitution precedes it. Fresh sessions fail closed.
    let mut history = History::new();
    history.push("echo alpha");
    history.push("echo beta");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();

    // Fresh one-shot `!??` fails closed instead of matching any entry.
    assert_eq!(
        expand_history(b"!??", &history, histchars, &policy, |_| false),
        Err(HistoryExpansionError::EventNotFound("!??".to_string())),
    );

    let mut state = HistoryExpansionState::new();
    // Fresh session `!??` also fails closed.
    assert_eq!(
        expand_history_with_state(b"!??", &history, histchars, &policy, &mut state, |_| false),
        Err(HistoryExpansionError::EventNotFound("!??".to_string())),
    );
    assert_eq!(
        expand_history_with_state(
            b"!?alpha?",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        )
        .unwrap(),
        b"echo alpha".to_vec(),
    );
    // Empty search reuses `alpha`, finding `echo alpha` rather than the last entry.
    assert_eq!(
        expand_history_with_state(b"!??", &history, histchars, &policy, &mut state, |_| false)
            .unwrap(),
        b"echo alpha".to_vec(),
    );
    // `%` reuses the matched word from the previous substring search.
    assert_eq!(
        expand_history_with_state(b"!:%", &history, histchars, &policy, &mut state, |_| false)
            .unwrap(),
        b"alpha".to_vec(),
    );
    // Empty-old substitution falls back to the remembered search.
    assert_eq!(
        expand_history_with_state(
            b"!1:s//OMEGA/",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        )
        .unwrap(),
        b"echo OMEGA".to_vec(),
    );
    // A failed search keeps the remembered needle for the next `!??`.
    assert_eq!(
        expand_history_with_state(
            b"!?missing?",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        ),
        Err(HistoryExpansionError::EventNotFound(
            "!?missing?".to_string()
        )),
    );
    assert_eq!(
        expand_history_with_state(b"!??", &history, histchars, &policy, &mut state, |_| false)
            .unwrap(),
        b"echo alpha".to_vec(),
    );
}

#[test]
fn history_expansion_cold_start_percent_is_empty_in_pinned_fixture() {
    // Exact pinned GNU History 8.3 fixture: seed two entries and make !:% the
    // first expansion call in a fresh stateful session.
    let mut history = History::new();
    history.push("echo alpha");
    history.push("echo beta");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut state = HistoryExpansionState::new();

    let result = expand_history_with_status_with_state(
        b"!:%",
        &history,
        histchars,
        &policy,
        &mut state,
        |_| false,
    )
    .unwrap();
    assert_eq!(result.status, HistoryExpansionStatus::Expanded);
    assert_eq!(result.line, b"");
}

#[test]
fn history_expansion_percent_after_first_failed_search_is_empty_in_pinned_fixture() {
    // Exact pinned GNU History 8.3 sequence: a first failed substring search
    // leaves no matched word for the immediately following !:% designator.
    let mut history = History::new();
    history.push("echo alpha");
    history.push("echo beta");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut state = HistoryExpansionState::new();

    assert_eq!(
        expand_history_with_state(
            b"!?missing?",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        ),
        Err(HistoryExpansionError::EventNotFound(
            "!?missing?".to_string()
        )),
    );
    let result = expand_history_with_status_with_state(
        b"!:%",
        &history,
        histchars,
        &policy,
        &mut state,
        |_| false,
    )
    .unwrap();
    assert_eq!(result.status, HistoryExpansionStatus::Expanded);
    assert_eq!(result.line, b"");
}

#[test]
fn history_expansion_reuses_search_after_failure_in_pinned_sequence() {
    // Pinned GNU History 8.3 primary/replica: seed once per sequence, with
    // no expansion outputs added. A fresh success→reuse session is the control.
    for failed_search in [true, false] {
        let mut history = History::new();
        history.push("echo alpha");
        history.push("echo beta");
        let histchars = HistoryChars::parse("!^#");
        let policy = HistoryExpansionPolicy::default();
        let mut state = HistoryExpansionState::new();
        let mut expand = |input: &[u8]| {
            expand_history_with_state(input, &history, histchars, &policy, &mut state, |_| false)
        };
        assert_eq!(expand(b"!?alpha?").unwrap(), b"echo alpha");
        if failed_search {
            // Rust exposes a typed error, not GNU's status/error-output pair.
            assert_eq!(
                expand(b"!?missing?"),
                Err(HistoryExpansionError::EventNotFound(
                    "!?missing?".to_string()
                )),
            );
        }
        assert_eq!(expand(b"!??").unwrap(), b"echo alpha");
    }
}

#[test]
fn history_expansion_search_memory_survives_clear_and_reseed_in_pinned_fixture() {
    // Mirrors the pinned GNU 8.3 primary/fresh-replica fixture exactly: the
    // remembered substring is used against the new entries after clear/reseed.
    let mut history = History::new();
    history.push("echo alpha");
    history.push("echo beta");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut state = HistoryExpansionState::new();

    assert_eq!(
        expand_history_with_state(
            b"!?alpha?",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false
        )
        .unwrap(),
        b"echo alpha".to_vec(),
    );
    assert_eq!(history.len(), 2);
    history.clear();
    assert_eq!(history.len(), 0);
    history.push("echo alpha-new");
    history.push("echo gamma");
    assert_eq!(history.len(), 2);

    assert_eq!(
        expand_history_with_state(b"!??", &history, histchars, &policy, &mut state, |_| false)
            .unwrap(),
        b"echo alpha-new".to_vec(),
    );
}

#[test]
fn history_expansion_percent_reuses_matched_word_after_clear_and_reseed_in_pinned_fixture() {
    // Exact pinned GNU History 8.3 fixture: the matched word remains `alpha`
    // across clearing and reseeding, so !:% does not select from the new list.
    let mut history = History::new();
    history.push("echo alpha");
    history.push("echo beta");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut state = HistoryExpansionState::new();

    let matched = expand_history_with_status_with_state(
        b"!?alpha?",
        &history,
        histchars,
        &policy,
        &mut state,
        |_| false,
    )
    .unwrap();
    assert_eq!(matched.status, HistoryExpansionStatus::Expanded);
    assert_eq!(matched.line, b"echo alpha");
    assert_eq!(history.len(), 2);
    history.clear();
    assert_eq!(history.len(), 0);
    history.push("echo alpha-new");
    history.push("echo gamma");
    assert_eq!(history.len(), 2);

    let percent = expand_history_with_status_with_state(
        b"!:%",
        &history,
        histchars,
        &policy,
        &mut state,
        |_| false,
    )
    .unwrap();
    assert_eq!(percent.status, HistoryExpansionStatus::Expanded);
    assert_eq!(percent.line, b"alpha");
}

#[test]
fn history_expansion_empty_old_substitution_after_first_failed_search_fails_in_pinned_fixture() {
    // The pinned GNU History 8.3 fixture starts with a failed first search,
    // then adds the matching event before trying empty-old substitution.
    let mut history = History::new();
    history.push("echo alpha");
    history.push("echo beta");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut state = HistoryExpansionState::new();

    let failed_search = expand_history_with_status_with_state(
        b"!?gamma?",
        &history,
        histchars,
        &policy,
        &mut state,
        |_| false,
    );
    assert!(matches!(
        failed_search,
        Err(HistoryExpansionError::EventNotFound(message)) if message == "!?gamma?"
    ));
    assert_eq!(history.len(), 2);
    history.push("echo gamma-new");
    assert_eq!(history.len(), 3);
    assert_eq!(history.entries()[2].line_bytes, b"echo gamma-new");

    assert!(matches!(
        expand_history_with_status_with_state(
            b"!-1:s//OMEGA/",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        ),
        Err(HistoryExpansionError::SubstitutionFailed)
    ));
}

#[test]
fn history_expansion_empty_old_substitution_uses_search_after_clear_and_reseed_in_pinned_fixture() {
    // Exact pinned GNU History 8.3 sequence: the empty old substitution uses
    // the preceding `alpha` search after history is cleared and reseeded.
    let mut history = History::new();
    history.push("echo alpha");
    history.push("echo beta");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut state = HistoryExpansionState::new();

    let matched = expand_history_with_status_with_state(
        b"!?alpha?",
        &history,
        histchars,
        &policy,
        &mut state,
        |_| false,
    )
    .unwrap();
    assert_eq!(matched.status, HistoryExpansionStatus::Expanded);
    assert_eq!(matched.line, b"echo alpha");
    assert_eq!(history.len(), 2);
    history.clear();
    assert_eq!(history.len(), 0);
    history.push("echo alpha-new");
    history.push("echo gamma");
    assert_eq!(history.len(), 2);

    let substitution = expand_history_with_status_with_state(
        b"!-2:s//OMEGA/",
        &history,
        histchars,
        &policy,
        &mut state,
        |_| false,
    )
    .unwrap();
    assert_eq!(substitution.status, HistoryExpansionStatus::Expanded);
    assert_eq!(substitution.line, b"echo OMEGA-new");
}

#[test]
fn history_expansion_percent_reuses_matched_word_after_failed_search_in_pinned_fixture() {
    // Exact pinned GNU 8.3 sequence: the `%` expansion immediately follows a
    // failed substring search in the same stateful session.
    let mut history = History::new();
    history.push("echo alpha");
    history.push("echo beta");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut state = HistoryExpansionState::new();

    assert_eq!(
        expand_history_with_state(
            b"!?alpha?",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false
        )
        .unwrap(),
        b"echo alpha".to_vec(),
    );
    assert_eq!(
        expand_history_with_state(
            b"!?missing?",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        ),
        Err(HistoryExpansionError::EventNotFound(
            "!?missing?".to_string()
        )),
    );
    assert_eq!(
        expand_history_with_state(b"!:%", &history, histchars, &policy, &mut state, |_| false)
            .unwrap(),
        b"alpha".to_vec(),
    );
}

#[test]
fn history_expansion_selects_percent_word_by_match_position() {
    // GNU selects `%` from the substring match offset, so a needle spanning
    // word boundaries still yields the word holding the match start. The
    // backwards-search occurrence choice is retained: a repeated needle
    // yields the later word, so forward iteration would be wrong here.
    let mut history = History::new();
    history.push("echo alpha-first alpha-last");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut state = HistoryExpansionState::new();

    assert_eq!(
        expand_history_with_state(
            b"!?alpha-first alpha?:%",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        )
        .unwrap(),
        b"alpha-first".to_vec(),
    );
    // `%` reuses the stored match-position word on the next call.
    assert_eq!(
        expand_history_with_state(b"!:%", &history, histchars, &policy, &mut state, |_| false)
            .unwrap(),
        b"alpha-first".to_vec(),
    );
    assert_eq!(
        expand_history_with_state(
            b"!?echo alpha?:%",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        )
        .unwrap(),
        b"echo".to_vec(),
    );
    // Repeated-needle control: `alpha` occurs in both words and the later
    // word wins, matching GNU backwards search.
    assert_eq!(
        expand_history_with_state(
            b"!?alpha?:%",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        )
        .unwrap(),
        b"alpha-last".to_vec(),
    );
    // One-shot calls without session state behave the same per call.
    assert_eq!(
        expand_history(
            b"!?alpha-first alpha?:%",
            &history,
            histchars,
            &policy,
            |_| false
        )
        .unwrap(),
        b"alpha-first".to_vec(),
    );
}

#[test]
fn history_expansion_scans_event_prefixes_by_delimiters() {
    // GNU scans a `!string` prefix until a delimiter: dots, slashes, and
    // most punctuation belong to the prefix, while a dash terminates it.
    let mut history = History::new();
    history.push("foo wrong");
    history.push("foo.bar correct");
    history.push("./tool local");
    history.push("/usr/bin/tool absolute");
    history.push("foo/bar slash");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    for (typed, expected) in [
        ("!foo.bar", "foo.bar correct"),
        ("!./tool", "./tool local"),
        ("!/usr/bin/tool", "/usr/bin/tool absolute"),
        ("!foo/bar", "foo/bar slash"),
    ] {
        assert_eq!(
            expand_history(typed.as_bytes(), &history, histchars, &policy, |_| false).unwrap(),
            expected.as_bytes().to_vec(),
            "{typed}"
        );
    }

    // A dash terminates the prefix: `!foo` selects the event while `-`
    // applies the all-but-last-word range and `bar` stays literal.
    let mut dashed = History::new();
    dashed.push("foo wrong");
    assert_eq!(
        expand_history(b"!foo-bar", &dashed, histchars, &policy, |_| false).unwrap(),
        b"foobar".to_vec(),
    );
    // Dash without digits is not a relative event; GNU reports the token.
    assert_eq!(
        expand_history(b"!-bar", &dashed, histchars, &policy, |_| false),
        Err(HistoryExpansionError::EventNotFound("!-bar".to_string())),
    );

    // Shell metacharacters terminate the prefix: the event expands and
    // the metacharacter with the rest stays literal.
    let mut plain = History::new();
    plain.push("foo wrong");
    for (typed, expected) in [
        ("!foo;bar", "foo wrong;bar"),
        ("!foo&bar", "foo wrong&bar"),
        ("!foo|bar", "foo wrong|bar"),
        ("!foo<in", "foo wrong<in"),
        ("!foo>out", "foo wrong>out"),
        ("!foo(bar", "foo wrong(bar"),
        ("!foo)bar", "foo wrong)bar"),
    ] {
        assert_eq!(
            expand_history(typed.as_bytes(), &plain, histchars, &policy, |_| false).unwrap(),
            expected.as_bytes().to_vec(),
            "{typed}"
        );
    }

    // Open-ended ranges select nothing on single-word entries like GNU.
    let mut single = History::new();
    single.push("abcd");
    assert_eq!(
        expand_history(b"!ab-cd", &single, histchars, &policy, |_| false).unwrap(),
        b"cd".to_vec(),
    );
    assert_eq!(
        expand_history(b"!!:0-", &single, histchars, &policy, |_| false).unwrap(),
        b"".to_vec(),
    );
}

#[test]
fn history_expansion_consults_search_delimiters_for_string_prefixes() {
    // Paired with history_expand_probe's four search_delimiters_* rows.
    // An empty/default set mirrors GNU's NULL; dot terminates only `!string`
    // prefixes, not `!?` needles. Pinned GNU 8.3 confirmation is pending.
    let mut history = History::new();
    history.push("foo wrong");
    history.push("foo.bar correct");
    history.push("foobar");
    history.push("foo.bar qux");
    let histchars = HistoryChars::parse("!^#");
    let custom = HistoryExpansionPolicy {
        search_delimiters: b".".to_vec(),
        ..HistoryExpansionPolicy::default()
    };
    let empty = HistoryExpansionPolicy {
        search_delimiters: Vec::new(),
        ..HistoryExpansionPolicy::default()
    };
    for (input, policy, expected) in [
        (b"!foo.bar".as_slice(), &empty, b"foo.bar qux".as_slice()),
        (b"!foo.bar", &custom, b"foo.bar qux.bar"),
        (b"!?bar", &empty, b"foo.bar qux"),
        (b"!?bar", &custom, b"foo.bar qux"),
    ] {
        let result =
            expand_history_with_status(input, &history, histchars, policy, |_| false).unwrap();
        assert_eq!(result.line, expected, "input={input:?}");
        assert_eq!(result.status, HistoryExpansionStatus::Expanded);
    }
    // Retain the closed-needle controls beyond the four direct GNU fixtures.
    for policy in [&custom, &empty] {
        assert_eq!(
            expand_history(b"!?bar?", &history, histchars, policy, |_| false).unwrap(),
            b"foo.bar qux".to_vec(),
        );
    }
}

#[test]
fn history_expansion_scans_omitted_question_needle_to_end_of_line() {
    // Without a closing `?`, GNU scans the needle to the end of the line:
    // spaces, colons, and semicolons belong to the needle, so no
    // word-designator or modifier split follows for the event.
    let mut first = History::new();
    for entry in [
        "foobar",
        "foo-bar baz",
        "foo.bar qux",
        "foo/bar qux",
        "foo\"bar qux",
        "foo'bar qux",
        "myprog arg1 arg2",
    ] {
        first.push(entry);
    }
    let mut second = History::new();
    second.push("alpha beta");
    second.push("gamma delta here");
    second.push("/usr/local/bin/tool arg");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    for (history, typed, expected) in [
        (&first, "!?arg1", "myprog arg1 arg2"),
        (&second, "!?delta?:p", "gamma delta here"),
        (&second, "!?delta here", "gamma delta here"),
        (&second, "!?delta? here", "gamma delta here here"),
        (&second, "!?gamma delta", "gamma delta here"),
        (&second, "!?gamma delta? extra", "gamma delta here extra"),
    ] {
        assert_eq!(
            expand_history(typed.as_bytes(), history, histchars, &policy, |_| false).unwrap(),
            expected.as_bytes().to_vec(),
            "{typed}"
        );
    }
    for (history, typed, needle) in [
        (&first, "!?arg1 qux", "arg1 qux"),
        (&first, "!?arg1:qux", "arg1:qux"),
        (&first, "!?arg1;qux", "arg1;qux"),
        (&second, "!?delta:p", "delta:p"),
    ] {
        assert_eq!(
            expand_history(typed.as_bytes(), history, histchars, &policy, |_| false),
            Err(HistoryExpansionError::EventNotFound(format!("!?{needle}?"))),
            "{typed}"
        );
    }
}

#[test]
fn history_expansion_terminates_substring_needle_at_newline() {
    // GNU terminates a `!?` needle at a newline or closing `?`: a later `?`
    // after the newline is literal suffix, not the delimiter.
    let mut history = History::new();
    history.push("echo alpha");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();

    for (typed, next_index, expected) in [
        ("!?alpha\nTAIL", 7, "echo alpha\nTAIL"),
        ("!?alpha\nTAIL?", 7, "echo alpha\nTAIL?"),
        ("!?alpha?\nTAIL", 8, "echo alpha\nTAIL"),
    ] {
        let event = get_history_event(typed.as_bytes(), &history, histchars, &policy)
            .expect("event parse")
            .expect("event found");
        assert_eq!(event.next_index, next_index, "{typed:?}");
        assert_eq!(event.line, b"echo alpha".to_vec(), "{typed:?}");
        assert_eq!(
            expand_history(typed.as_bytes(), &history, histchars, &policy, |_| false).unwrap(),
            expected.as_bytes().to_vec(),
            "{typed:?}"
        );
    }
}

#[test]
fn history_expansion_retains_substitution_memory_across_calls() {
    // GNU History-library memory (`subst_lhs`/`subst_rhs`): `&` repeats the
    // previous substitution and empty-old reuses it across expansion calls.
    let mut history = History::new();
    history.push("echo alpha beta alpha");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let mut state = HistoryExpansionState::new();

    // Fresh single-call `&` fails closed without a prior substitution.
    assert_eq!(
        expand_history(b"!!:&", &history, histchars, &policy, |_| false),
        Err(HistoryExpansionError::SubstitutionFailed),
    );

    assert_eq!(
        expand_history_with_state(
            b"!!:s/alpha/OMEGA/",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        )
        .unwrap(),
        b"echo OMEGA beta alpha".to_vec(),
    );
    // `&` repeats the remembered substitution on the same event line.
    assert_eq!(
        expand_history_with_state(b"!!:&", &history, histchars, &policy, &mut state, |_| false)
            .unwrap(),
        b"echo OMEGA beta alpha".to_vec(),
    );
    // Empty-old reuses the remembered old with a new replacement.
    assert_eq!(
        expand_history_with_state(
            b"!!:s//Z/",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        )
        .unwrap(),
        b"echo Z beta alpha".to_vec(),
    );
    // A failed parsed substitution replaces memory (GNU comparison
    // `e1f2e9a262794e4ca618bdda1bc91242`): later `&` and empty-old reuse the
    // failed pair and fail instead of retaining stale state.
    assert_eq!(
        expand_history_with_state(
            b"!!:s/missing/MISSING/",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        ),
        Err(HistoryExpansionError::SubstitutionFailed),
    );
    assert_eq!(
        expand_history_with_state(b"!!:&", &history, histchars, &policy, &mut state, |_| false),
        Err(HistoryExpansionError::SubstitutionFailed),
    );
    assert_eq!(
        expand_history_with_state(
            b"!!:s//Z/",
            &history,
            histchars,
            &policy,
            &mut state,
            |_| false,
        ),
        Err(HistoryExpansionError::SubstitutionFailed),
    );

    // Quick substitution seeds the same session memory for a later `&`.
    let mut quick_state = HistoryExpansionState::new();
    assert_eq!(
        expand_history_with_status_with_state(
            b"^alpha^OMEGA^",
            &history,
            histchars,
            &policy,
            &mut quick_state,
            |_| false,
        )
        .unwrap()
        .line,
        b"echo OMEGA beta alpha".to_vec(),
    );
    assert_eq!(
        expand_history_with_state(
            b"!!:&",
            &history,
            histchars,
            &policy,
            &mut quick_state,
            |_| false
        )
        .unwrap(),
        b"echo OMEGA beta alpha".to_vec(),
    );
}

#[test]
fn history_expansion_failed_substitution_replaces_memory() {
    // GNU comparisons `e1f2e9a262794e4ca618bdda1bc91242` and
    // `167300f533954bbfb7f55dfb77cfd585`: a parsed ordinary (`:s`), global
    // (`:gs`/`:as`), per-word (`:Gs`), or leading quick
    // substitution is remembered before replacement is attempted, so a
    // no-match failure replaces stale state and later `&`/empty-old fail.
    let mut history = History::new();
    history.push("echo alpha beta alpha");
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();

    for failed in [
        b"!!:s/missing/MISSING/".as_slice(),
        b"!!:gs/missing/MISSING/".as_slice(),
        b"!!:as/missing/MISSING/".as_slice(),
        b"!!:Gs/missing/MISSING/".as_slice(),
    ] {
        let mut state = HistoryExpansionState::new();
        assert_eq!(
            expand_history_with_state(
                b"!!:s/alpha/OMEGA/",
                &history,
                histchars,
                &policy,
                &mut state,
                |_| false,
            )
            .unwrap(),
            b"echo OMEGA beta alpha".to_vec(),
            "{}",
            String::from_utf8_lossy(failed),
        );
        assert_eq!(
            expand_history_with_state(failed, &history, histchars, &policy, &mut state, |_| false),
            Err(HistoryExpansionError::SubstitutionFailed),
            "{}",
            String::from_utf8_lossy(failed),
        );
        assert_eq!(
            expand_history_with_state(b"!!:&", &history, histchars, &policy, &mut state, |_| false),
            Err(HistoryExpansionError::SubstitutionFailed),
            "{}",
            String::from_utf8_lossy(failed),
        );
        assert_eq!(
            expand_history_with_state(
                b"!!:s//Z/",
                &history,
                histchars,
                &policy,
                &mut state,
                |_| false
            ),
            Err(HistoryExpansionError::SubstitutionFailed),
            "{}",
            String::from_utf8_lossy(failed),
        );
    }

    // Leading quick substitution participates in the same lifecycle.
    let mut quick_state = HistoryExpansionState::new();
    assert_eq!(
        expand_history_with_state(
            b"!!:s/alpha/OMEGA/",
            &history,
            histchars,
            &policy,
            &mut quick_state,
            |_| false,
        )
        .unwrap(),
        b"echo OMEGA beta alpha".to_vec(),
    );
    assert_eq!(
        expand_history_with_state(
            b"^missing^MISSING^",
            &history,
            histchars,
            &policy,
            &mut quick_state,
            |_| false,
        ),
        Err(HistoryExpansionError::SubstitutionFailed),
    );
    assert_eq!(
        expand_history_with_state(
            b"!!:&",
            &history,
            histchars,
            &policy,
            &mut quick_state,
            |_| false
        ),
        Err(HistoryExpansionError::SubstitutionFailed),
    );
    assert_eq!(
        expand_history_with_state(
            b"!!:s//Z/",
            &history,
            histchars,
            &policy,
            &mut quick_state,
            |_| false
        ),
        Err(HistoryExpansionError::SubstitutionFailed),
    );
}

#[test]
fn history_expansion_preserves_backslashes_before_ordinary_bytes() {
    // GNU libhistory.so.8: with slash and colon delimiters, only
    // `\<delimiter>` escapes the delimiter; only replacement-side `\&`
    // is a literal ampersand. Other backslashes (including doubled ones) are
    // preserved, while bare replacement `&` still expands the old text.
    let histchars = HistoryChars::parse("!^#");
    let policy = HistoryExpansionPolicy::default();
    let expand = |event: &[u8], expansion: &[u8]| {
        let mut history = History::new();
        history.push(std::str::from_utf8(event).unwrap());
        expand_history(expansion, &history, histchars, &policy, |_| false)
    };

    // Replacement `\z` and `\q` preserve the slash.
    assert_eq!(
        expand(b"echo foo", b"!!:s/foo/\\z/").unwrap(),
        b"echo \\z".to_vec(),
    );
    assert_eq!(
        expand(b"echo foo", b"!!:s/foo/\\q/").unwrap(),
        b"echo \\q".to_vec(),
    );
    // `\\z` preserves both slashes.
    assert_eq!(
        expand(b"echo foo", b"!!:s/foo/\\\\z/").unwrap(),
        b"echo \\\\z".to_vec(),
    );
    // Replacement `\&` is a literal `&`; bare `&` expands the old text.
    assert_eq!(
        expand(b"echo foo", b"!!:s/foo/\\&/").unwrap(),
        b"echo &".to_vec(),
    );
    assert_eq!(
        expand(b"echo foo", b"!!:s/foo/&/").unwrap(),
        b"echo foo".to_vec(),
    );
    assert_eq!(
        expand(b"echo foo", b"!!:s/foo/[&]/").unwrap(),
        b"echo [foo]".to_vec(),
    );
    assert_eq!(
        expand(b"echo foo", b"!!:s/foo/[\\&]/").unwrap(),
        b"echo [&]".to_vec(),
    );
    // Escaped delimiter is literal delimiter data in either field.
    assert_eq!(
        expand(b"echo a/b", b"!!:s/a\\/b/X/").unwrap(),
        b"echo X".to_vec(),
    );
    assert_eq!(
        expand(b"echo foo", b"!!:s/foo/a\\/b/").unwrap(),
        b"echo a/b".to_vec(),
    );
    // Only the active delimiter is special: with `:` delimiters a backslash
    // before `/` is preserved, and with `/` delimiters one before `:` is.
    assert_eq!(
        expand(b"echo foo", b"!!:s:foo:\\/:").unwrap(),
        b"echo \\/".to_vec(),
    );
    assert_eq!(
        expand(b"echo foo", b"!!:s/foo/\\:/").unwrap(),
        b"echo \\:".to_vec(),
    );
    // An escaped active colon stays data, and a backslash can itself be the delimiter.
    assert_eq!(
        expand(b"echo foo", b"!!:s:foo:\\::").unwrap(),
        b"echo :".to_vec(),
    );
    assert_eq!(
        expand(b"echo foo", b"!!:s\\foo\\X\\").unwrap(),
        b"echo X".to_vec(),
    );
    // Search-side ordinary escapes are preserved: `f\oo` matches a literal
    // backslash entry, not `foo`.
    assert_eq!(
        expand(b"echo f\\oo", b"!!:s/f\\oo/BAR/").unwrap(),
        b"echo BAR".to_vec(),
    );
    assert_eq!(
        expand(b"echo foo", b"!!:s/f\\oo/BAR/"),
        Err(HistoryExpansionError::SubstitutionFailed),
    );
    // Search-side `\&` is ordinary and preserved, not a literal `&`.
    assert_eq!(
        expand(b"echo a&b", b"!!:s/\\&/X/"),
        Err(HistoryExpansionError::SubstitutionFailed),
    );
    assert_eq!(
        expand(b"echo a\\&b", b"!!:s/\\&/X/").unwrap(),
        b"echo aXb".to_vec(),
    );
}

#[test]
fn joined_grouping_joins_plain_command_lines() {
    // Plain command lines form one entry in joined mode.
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain");
    fs::write(&plain, "C0\nC1\nC2\nC3\n").unwrap();
    let joined = History::read_file_with_multiline(&plain, HistoryMultiline::Joined).unwrap();
    assert_eq!(
        joined
            .entries()
            .iter()
            .map(|entry| entry.line().into_owned())
            .collect::<Vec<_>>(),
        vec!["C0\nC1\nC2\nC3".to_string()]
    );
}

#[test]
fn history_expansion_answers_match_the_measured_c_library() {
    // Event and word designators, modifiers, substitutions, relative events,
    // and quick substitution were measured against installed GNU History 8.2.
    // Selected outputs are also recorded and asserted against pinned GNU
    // History 8.3 by the direct-library oracle step; that probe covers the
    // exact open-single-quote input below as well.
    let mut history = History::new();
    history.push("echo previous");
    history.push("echo a b");
    history.push("echo a b");
    let policy = HistoryExpansionPolicy::default();
    for (input, expected) in [
        ("!!", "echo a b"),
        ("!$", "b"),
        ("!:0", "echo"),
        ("!!:h", "echo a b"),
        ("!!:t", "echo a b"),
        ("!!:r", "echo a b"),
        ("!!:e", "echo a b"),
        ("!!:q", "'echo a b'"),
        ("!!:p", "echo a b"),
        ("!!:x", "'echo' 'a' 'b'"),
        ("!!:s/b/B/", "echo a B"),
        ("!-2", "echo a b"),
        ("^a^A^ trailing", "echo A b trailing"),
        ("echo ^a^b", "echo ^a^b"),
    ] {
        assert_eq!(
            expand_history(
                input.as_bytes(),
                &history,
                HistoryChars::parse("!^#"),
                &policy,
                |_| false,
            )
            .unwrap(),
            expected.as_bytes(),
            "input={input:?}"
        );
    }
    // The library rejects the modifiers it does not know and reports the
    // event it cannot find; this library rejects them too.
    // Pinned GNU History 8.3 returns this exact diagnostic for the
    // unrecognized `:u` modifier.
    let unknown_modifier = expand_history(
        b"!!:u",
        &history,
        HistoryChars::parse("!^#"),
        &policy,
        |_| false,
    )
    .unwrap_err();
    assert_eq!(
        unknown_modifier,
        HistoryExpansionError::UnrecognizedHistoryModifier("u".to_string())
    );
    assert_eq!(
        unknown_modifier.message(),
        "u: unrecognized history modifier"
    );
    assert_eq!(
        expand_history(
            b"!nonexistent",
            &history,
            HistoryChars::parse("!^#"),
            &policy,
            |_| false,
        ),
        Err(HistoryExpansionError::EventNotFound(
            "!nonexistent".to_string()
        ))
    );
    // The pinned GNU History 8.3 artifact distinguishes status 0 (no
    // expansion), status 1 (event expansion), and status 2 (print-only).
    for (input, expected, status) in [
        (
            b"echo ^a^b".as_slice(),
            b"echo ^a^b".as_slice(),
            HistoryExpansionStatus::Unchanged,
        ),
        (
            b"!!".as_slice(),
            b"echo a b".as_slice(),
            HistoryExpansionStatus::Expanded,
        ),
        (
            b"!!:p".as_slice(),
            b"echo a b".as_slice(),
            HistoryExpansionStatus::PrintOnly,
        ),
        (
            b"^a^A^ trailing".as_slice(),
            b"echo A b trailing".as_slice(),
            HistoryExpansionStatus::Expanded,
        ),
    ] {
        let result = expand_history_with_status(
            input,
            &history,
            HistoryChars::parse("!^#"),
            &policy,
            |_| false,
        )
        .unwrap();
        assert_eq!(result.line, expected, "input={input:?}");
        assert_eq!(result.status, status, "input={input:?}");
    }

    // Pinned GNU History 8.3 reports status 1 and bytes `echo 'echo a b`
    // for this direct-library input with the latest event seeded above.
    assert_eq!(
        expand_history(
            b"echo '!!",
            &history,
            HistoryChars::parse("!^#"),
            &policy,
            |_| false,
        )
        .unwrap(),
        b"echo 'echo a b"
    );
    let inhibited_policy = HistoryExpansionPolicy {
        quotes_inhibit_expansion: true,
        ..policy.clone()
    };
    assert_eq!(
        expand_history(
            b"echo '!!",
            &history,
            HistoryChars::parse("!^#"),
            &inhibited_policy,
            |_| false,
        )
        .unwrap(),
        b"echo '!!"
    );
}

#[test]
fn joined_range_edges_at_blank_physical_lines_keep_current_rule() {
    let dir = tempfile::tempdir().unwrap();
    let fixtures = [
        (
            "timestamped",
            "#1700000000\nA0\n\nA1\n#1700000001\nB0\nB1\n",
            vec!["A1", "B0\nB1"],
        ),
        ("plain", "A0\n\nA1\nB0\nB1\n", vec!["A1\nB0\nB1"]),
    ];

    for (name, contents, reversed_to_eof) in fixtures {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        let lines = |from, to| {
            History::read_file_range_with_multiline(&path, from, Some(to), HistoryMultiline::Joined)
                .unwrap()
                .entries()
                .iter()
                .map(|entry| entry.line().into_owned())
                .collect::<Vec<_>>()
        };

        let blank = vec![""];
        assert_eq!(lines(1, 1), blank, "{name}: equal blank-line bound");
        assert_eq!(lines(1, 2), blank, "{name}: exclusive TO leaves blank only");
        assert_eq!(lines(2, 1), reversed_to_eof, "{name}: TO blank");
        let from_blank = if name == "plain" {
            vec!["\nA1\nB0\nB1"]
        } else {
            reversed_to_eof
        };
        assert_eq!(lines(1, 0), from_blank, "{name}: FROM blank");
    }
}

#[test]
fn full_file_blank_line_modes_preserve_current_grouping_policy() {
    let dir = tempfile::tempdir().unwrap();
    let fixtures = [
        (
            "timestamped",
            "#1700000000\n\nA\n#1700000001\n\n\n#1700000002\nB\n\n",
            vec!["A", "B"],
            vec!["A", "B\n"],
        ),
        (
            "plain",
            "\nA\n\n\nB\n\n\nC\n\n",
            vec!["A", "B", "C"],
            vec!["\nA\n\n\nB\n\n\nC\n"],
        ),
    ];

    for (name, contents, per_line, joined) in fixtures {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        for (mode, expected) in [
            (HistoryMultiline::PerLine, per_line),
            (HistoryMultiline::Joined, joined),
        ] {
            let history = History::read_file_with_multiline(&path, mode).unwrap();
            let lines = history
                .entries()
                .iter()
                .map(|entry| entry.line().into_owned())
                .collect::<Vec<_>>();
            assert_eq!(lines, expected, "{name} {mode:?}");
        }
    }
}

#[test]
fn stifle_preserves_offsets_and_state_across_trimming_and_noop_caps() {
    for limit in 0..=4 {
        for position in 0..=3 {
            let mut h = History::new();
            for line in ["alpha", "beta", "gamma"] {
                h.push(line);
            }
            assert!(h.set_pos(position));
            h.stifle(limit);
            let expected = &["alpha", "beta", "gamma"][3 - limit.min(3)..];
            let lines: Vec<_> = h.entries().iter().map(|e| e.line()).collect();
            assert_eq!(lines, expected, "limit={limit} position={position}");
            assert_eq!(h.where_history(), position);
            assert_eq!(
                h.current_index(),
                (position < expected.len()).then_some(position)
            );
            assert_eq!(
                h.current_history().map(|e| e.line().into_owned()),
                expected.get(position).map(|s| s.to_string())
            );
            let state = h.state();
            assert_eq!(state.offset, position);
            assert_eq!(state.length, expected.len());
            assert_eq!(state.max_entries, Some(limit));
            assert!(state.stifled);
            assert!(h.set_pos(0));
            assert!(h.set_state(state.clone()));
            assert_eq!(h.state(), state);
        }
    }
}

#[test]
fn unstifle_preserves_measured_offsets_and_entries() {
    for (limit, positions, expected) in [
        (0, &[0, 2, 4][..], &[][..]),
        (2, &[0, 1, 2, 3, 4][..], &["C", "D"][..]),
    ] {
        for &position in positions {
            let mut history = History::new();
            for line in ["A", "B", "C", "D"] {
                history.push(line);
            }
            assert!(history.set_pos(position));
            history.stifle(limit);
            assert!(history.is_stifled());
            assert_eq!(history.unstifle(), Some(limit));
            assert!(!history.is_stifled());
            assert_eq!(history.max_entries(), None);
            assert_eq!(history.where_history(), position);
            assert_eq!(
                history
                    .entries()
                    .iter()
                    .map(|e| e.line())
                    .collect::<Vec<_>>(),
                expected,
                "limit={limit} position={position}"
            );
            assert_eq!(
                history.current_history().map(|e| e.line().into_owned()),
                expected.get(position).map(|s| s.to_string())
            );
        }
    }
}

#[test]
fn clear_resets_measured_offsets_and_preserves_stifling() {
    for (limit, positions) in [
        (None, &[0, 1, 2, 3, 4][..]),
        (Some(2), &[1, 4][..]),
        (Some(0), &[4][..]),
    ] {
        for &position in positions {
            let mut history = History::new();
            for line in ["A", "B", "C", "D"] {
                history.push(line);
            }
            assert!(history.set_pos(position));
            if let Some(limit) = limit {
                history.stifle(limit);
            }
            assert_eq!(history.where_history(), position);
            history.clear();
            assert_eq!(
                history.where_history(),
                0,
                "limit={limit:?} position={position}"
            );
            assert!(history.entries().is_empty());
            assert_eq!(history.is_stifled(), limit.is_some());
            assert_eq!(history.max_entries(), limit);
        }
    }
}

#[test]
fn post_stifle_append_preserves_even_beyond_end_offsets() {
    for bytes in [false, true] {
        for limit in 0..=2 {
            for position in [1, 3] {
                let mut h = History::new();
                for line in ["alpha", "beta", "gamma"] {
                    h.push(line);
                }
                assert!(h.set_pos(position));
                h.stifle(limit);
                if bytes {
                    h.push_bytes(b"delta".to_vec());
                } else {
                    h.push("delta");
                }
                let expected: &[&str] = match limit {
                    0 => &[],
                    1 => &["delta"],
                    _ => &["gamma", "delta"],
                };
                assert_eq!(
                    h.entries().iter().map(|e| e.line()).collect::<Vec<_>>(),
                    expected
                );
                assert_eq!(h.where_history(), position);
                assert_eq!(
                    h.current_history().map(|e| e.line().into_owned()),
                    expected.get(position).map(|s| s.to_string())
                );
            }
        }
    }
}

#[test]
fn beyond_end_cursor_operations_are_safe_and_recoverable() {
    for offset in [3, usize::MAX] {
        let mut h = History::new();
        for line in ["alpha", "beta", "gamma"] {
            h.push(line);
        }
        assert!(h.set_pos(3));
        h.stifle(2);
        let mut state = h.state();
        state.offset = offset;
        assert!(h.set_state(state.clone()));
        assert_eq!(h.current_index(), None);
        assert!(h.current_history().is_none());
        assert!(h.previous_history().is_none());
        assert!(h.next_history().is_none());
        for direction in [HistoryDirection::Previous, HistoryDirection::Next] {
            assert!(h.navigate_bytes(direction, b"edit".to_vec()).is_none());
            assert!(h.history_search_bytes(b"a", direction).is_none());
            assert!(h.history_search_prefix("a", direction).is_none());
            assert!(h.history_search_pos("a", direction, usize::MAX).is_none());
        }
        assert!(h.next_after_current_cursor_bytes().is_none());
        assert!(h.search_prefix_backward_bytes(b"a", vec![]).is_none());
        assert!(h.search_prefix_forward_bytes(b"a").is_none());
        assert!(
            h.search_containing_backward_from_cursor_bytes(b"a", vec![])
                .is_none()
        );
        assert!(
            h.search_containing_forward_from_cursor_bytes(b"a", vec![])
                .is_none()
        );
        assert_eq!(h.where_history(), offset);
        // An explicit valid search position is independent of the invalid cursor.
        assert!(
            h.history_search_pos("beta", HistoryDirection::Next, 0)
                .is_some()
        );
        assert_eq!(h.where_history(), offset);
        assert!(!h.set_pos(3));
        assert_eq!(h.where_history(), offset);
        assert_eq!(
            h.beginning_bytes(b"edit".to_vec())
                .map(|line| String::from_utf8(line).unwrap()),
            Some("beta".to_string())
        );
        assert_eq!(h.where_history(), 0);
        assert_eq!(h.end_bytes(), Some(Vec::new()));
        assert_eq!(h.where_history(), h.len());
        assert!(h.set_state(state));
        assert_eq!(h.where_history(), offset);
        h.push("delta");
        assert_eq!(h.where_history(), offset);
        assert!(h.set_pos(2));
        assert_eq!(
            h.previous_history().map(|e| e.line().into_owned()),
            Some("delta".to_string())
        );
    }
}

#[test]
fn beyond_end_of_empty_stifled_history_is_safe() {
    let mut h = History::new();
    h.push("alpha");
    assert!(h.set_pos(1));
    h.stifle(0);
    assert!(h.previous_history().is_none());
    assert!(h.next_history().is_none());
    assert!(
        h.history_search_bytes(b"a", HistoryDirection::Previous)
            .is_none()
    );
    assert_eq!(h.where_history(), 1);
    assert_eq!(h.end_bytes(), Some(Vec::new()));
    assert_eq!(h.where_history(), 0);
    assert!(h.set_pos(0));
    assert_eq!(h.where_history(), 0);
}

#[test]
fn file_import_preserves_numeric_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "file-alpha\nfile-beta\nfile-gamma\n").unwrap();

    for ranged in [false, true] {
        let fresh = if ranged {
            History::read_file_range(&path, 0, Some(3)).unwrap()
        } else {
            History::read_file(&path).unwrap()
        };
        assert_eq!(fresh.where_history(), 0);
        assert_eq!(fresh.current_history().unwrap().line(), "file-alpha");
        assert_eq!(fresh.len(), 3);

        for position in [0, 1, 2, 3] {
            let mut history = History::new();
            for line in ["old-a", "old-b", "old-c"] {
                history.push(line);
            }
            assert!(history.set_pos(position));
            if ranged {
                history.load_file_range(&path, 0, Some(3), None).unwrap();
            } else {
                history.load_file(&path, None).unwrap();
            }
            assert_eq!(history.where_history(), position);
            assert_eq!(
                history.current_history().unwrap().line(),
                ["old-a", "old-b", "old-c", "file-alpha"][position]
            );
            assert_eq!(
                history
                    .entries()
                    .iter()
                    .map(|e| e.line())
                    .collect::<Vec<_>>(),
                [
                    "old-a",
                    "old-b",
                    "old-c",
                    "file-alpha",
                    "file-beta",
                    "file-gamma"
                ]
            );
        }

        for position in [1, 3] {
            for explicit_cap in [false, true] {
                let mut history = History::new();
                for line in ["old-a", "old-b", "old-c"] {
                    history.push(line);
                }
                assert!(history.set_pos(position));
                if !explicit_cap {
                    history.stifle(2);
                }
                let cap = explicit_cap.then_some(2);
                if ranged {
                    history.load_file_range(&path, 0, Some(3), cap).unwrap();
                } else {
                    history.load_file(&path, cap).unwrap();
                }
                assert_eq!(history.where_history(), position);
                assert_eq!(
                    history
                        .entries()
                        .iter()
                        .map(|e| e.line())
                        .collect::<Vec<_>>(),
                    ["file-beta", "file-gamma"]
                );
                assert_eq!(
                    history.current_history().map(|e| e.line_bytes.as_slice()),
                    (position == 1).then_some(b"file-gamma".as_slice())
                );
                // Imported entries are part of the loaded watermark, even after trimming.
                let output = dir.path().join("appended");
                fs::write(&output, "").unwrap();
                history.append_new_to_file(&output).unwrap();
                assert_eq!(fs::read(&output).unwrap(), b"");
            }
        }
    }
}

#[test]
fn full_file_crlf_import_matches_measured_grouping_and_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    type Fixture<'a> = (&'a str, &'a [u8], &'a [&'a [u8]], &'a [&'a [u8]]);
    let fixtures: &[Fixture<'_>] = &[
        (
            "plain",
            b"A0\r\nA1\r\nB0\r\n",
            &[b"A0", b"A1", b"B0"],
            &[b"A0\nA1\nB0"],
        ),
        (
            "timestamped",
            b"#1700000000\r\nA0\r\nA1\r\n#1700000001\r\nB0\r\nB1\r\n",
            &[b"A0", b"A1", b"B0", b"B1"],
            &[b"A0\nA1", b"B0\nB1"],
        ),
    ];
    for &(name, contents, per_line, joined) in fixtures {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        for (mode, expected) in [
            (HistoryMultiline::PerLine, per_line),
            (HistoryMultiline::Joined, joined),
        ] {
            let history = History::read_file_with_multiline(&path, mode).unwrap();
            let entries = history.entries();
            assert_eq!(
                entries
                    .iter()
                    .map(|e| e.line_bytes.as_slice())
                    .collect::<Vec<_>>(),
                expected,
                "{name} {mode:?}"
            );
            if name == "timestamped" {
                let second = if mode == HistoryMultiline::PerLine {
                    2
                } else {
                    1
                };
                assert_eq!(entries[0].timestamp.as_deref(), Some("#1700000000"));
                assert_eq!(entries[second].timestamp.as_deref(), Some("#1700000001"));
            }
        }
    }
}
