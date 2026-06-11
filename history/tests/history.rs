use std::fs;

use history::expansion::{
    HistoryChars, HistoryExpansionError, HistoryExpansionPolicy, expand_history,
    expand_history_with_status, get_history_event, history_arg_extract, history_tokenize,
};
use history::{History, HistoryDirection};

#[test]
fn navigates_history_and_restores_current_edit() {
    let mut h = History::new();
    h.push("one");
    h.push("two");
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
fn forward_history_search_starts_after_current_position() {
    let mut h = History::new();
    h.push("needle one");
    h.push("needle two");
    h.set_pos(0);

    let found = h
        .history_search_bytes(b"needle", HistoryDirection::Next)
        .expect("next match");
    assert_eq!(found.entry_index, 1);
    assert_eq!(found.line_bytes, b"needle two");
}

#[test]
fn searches_history_by_prefix() {
    let mut h = History::new();
    h.push("alpha one");
    h.push("beta");
    h.push("alpha two");
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
    loaded.append_file_with_timestamps(&path, 3, true).unwrap();
    History::truncate_file(&path, 2).unwrap();

    let truncated = History::read_file(&path).unwrap();
    assert_eq!(
        truncated
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (None, "printf two".to_string()),
            (None, "printf three".to_string())
        ]
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "printf two\nprintf three\n"
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
fn reads_history_file_ranges_and_controls_timestamp_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    fs::write(&path, "#1700000000\none\n#1700000001\ntwo\nthree\n").unwrap();

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

    let no_timestamps = dir.path().join("no-timestamps");
    ranged
        .write_file_with_timestamps(&no_timestamps, false)
        .unwrap();
    assert_eq!(fs::read_to_string(no_timestamps).unwrap(), "two\nthree\n");
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
    assert!(printed.print_only);

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
fn supports_history_library_state_and_stifle_operations() {
    let mut h = History::new();
    h.push("one");
    h.push("two");
    h.push("three");
    assert!(h.add_time("#1700000000"));
    assert_eq!(h.entries()[2].timestamp.as_deref(), Some("#1700000000"));
    assert_eq!(h.total_bytes(), "one".len() + "two".len() + "three".len());

    h.stifle(2);
    assert!(h.is_stifled());
    assert_eq!(h.max_entries(), Some(2));
    assert_eq!(h.len(), 2);
    assert_eq!(h.get(0).map(|entry| entry.line()), Some("two".into()));
    let state = h.state();
    assert_eq!(state.offset, 2);
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
fn supports_history_library_position_and_search_operations() {
    let mut h = History::new();
    h.push("alpha one");
    h.push("beta two");
    h.push("alpha three");

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
