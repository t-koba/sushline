use std::fs;

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
fn history_files_write_bash_compatible_raw_multiline_entries() {
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
    loaded.append_file(&path, 3).unwrap();
    History::truncate_file(&path, 2).unwrap();

    let truncated = History::read_file(&path).unwrap();
    assert_eq!(
        truncated
            .entries()
            .iter()
            .map(|entry| (entry.timestamp.as_deref(), entry.line().into_owned()))
            .collect::<Vec<_>>(),
        vec![
            (Some("#1700000001"), "printf two".to_string()),
            (Some("#1700000002"), "printf three".to_string()),
        ]
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "#1700000001\nprintf two\n#1700000002\nprintf three\n"
    );
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
