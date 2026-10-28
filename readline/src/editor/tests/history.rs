use super::*;

#[test]
fn auto_add_history_skips_empty_accepted_lines() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"\r".to_vec())]);
    let config = Config {
        auto_add_history: true,
        ..Config::default()
    };
    let mut history = History::new();
    history.push("one");
    history.push("two");
    assert!(history.set_pos(0));
    let mut editor = Editor::new(config, terminal, history);

    assert_eq!(
        editor
            .read_line(Prompt::new("> "), &mut TestHistoryHook)
            .unwrap(),
        ReadlineResult::Line(Vec::new())
    );
    assert_eq!(editor.history.len(), 2);
    assert_eq!(editor.history.where_history(), 2);
}

#[test]
fn auto_add_history_resets_cursor_after_nonempty_accepted_line() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"new\r".to_vec()),
        TerminalEvent::Bytes(b"\x1b[A\r".to_vec()),
    ]);
    let config = Config {
        auto_add_history: true,
        ..Config::default()
    };
    let mut history = History::new();
    history.push("old");
    let mut editor = Editor::new(config, terminal, history);

    assert_eq!(
        editor
            .read_line(Prompt::new("> "), &mut TestHistoryHook)
            .unwrap(),
        ReadlineResult::Line(b"new".to_vec())
    );
    assert_eq!(editor.history.where_history(), 2);
    assert!(editor.history.current_history().is_none());
    assert_eq!(
        editor
            .read_line(Prompt::new("> "), &mut TestHistoryHook)
            .unwrap(),
        ReadlineResult::Line(b"new".to_vec())
    );
}

#[test]
fn history_beginning_loads_entry_for_fresh_state_at_current_cursor() {
    use crate::prompt::Prompt;
    use crate::state::EditorState;

    let terminal = MemoryTerminal::with_events(vec![]);
    let mut history = History::new();
    history.push("alpha old");
    assert!(history.set_pos(0));
    let mut editor = Editor::new(Config::default(), terminal, history);
    let mut state = EditorState::new(Prompt::new("> "), None);
    let line = editor
        .history
        .beginning_bytes(state.buffer.as_bytes().to_vec())
        .expect("history beginning");

    assert_eq!(editor.history.current_index(), Some(0));
    editor.replace_from_history(&mut state, &line);
    assert_eq!(state.buffer.as_bytes(), b"alpha old");
}

#[test]
fn history_navigation_does_not_replace_an_entry_from_a_fresh_read() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x10\x05 EDIT\r".to_vec()),
        TerminalEvent::Bytes(b"\x10\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push("alpha old");
    history.push("alpha new");
    history.reset_cursor();
    let mut editor = Editor::new(Config::default(), terminal, history);
    editor
        .load_inputrc_str("set revert-all-at-newline off")
        .unwrap();
    let mut hooks = TestHistoryHook;

    assert_eq!(
        editor.read_line(Prompt::new("> "), &mut hooks).unwrap(),
        ReadlineResult::Line(b"alpha new EDIT".to_vec())
    );
    assert_eq!(
        editor.read_line(Prompt::new("> "), &mut hooks).unwrap(),
        ReadlineResult::Line(b"alpha old".to_vec())
    );
    // A new read starts with a fresh line, not the entry left selected by the
    // previous call. Its first history motion must not overwrite that entry.
    assert_eq!(editor.history.entries()[1].line_bytes, b"alpha new");
}

#[test]
fn history_expansion_supports_quick_substitution_and_event_search() {
    let mut history = History::new();
    history.push("echo src/lib.rs src/main.rs");
    history.push("git checkout main");

    assert_eq!(
        expand_history_for_test("^main^feature^", &history),
        "git checkout feature"
    );
    assert_eq!(
        expand_history_for_test("^main^feature", &history),
        "git checkout feature"
    );
    assert_eq!(
        expand_history_for_test("!?checkout?:2", &history),
        "main".to_string()
    );
    assert_eq!(
        expand_history_for_test("!?checkout?:%", &history),
        "checkout"
    );
    assert_eq!(
        expand_history_for_test("!-2:$:r", &history),
        "src/main".to_string()
    );
    assert_eq!(
        expand_history_for_test("!-2:1-$", &history),
        "src/lib.rs src/main.rs".to_string()
    );
    assert_eq!(
        expand_history_for_test("!-2:1-2", &history),
        "src/lib.rs src/main.rs".to_string()
    );
}

#[test]
fn history_expansion_honors_custom_histchars() {
    // GNU has no `histchars` inputrc variable, so custom characters only
    // apply through the direct expansion API, not editor inputrc.
    let mut history = History::new();
    history.push("git checkout main");
    assert_eq!(
        expand_history_with_chars_for_test("~main~dev~", &history, HistoryChars::parse("%~#")),
        Ok("git checkout dev".to_string())
    );
}

#[test]
fn history_expansion_reports_event_and_word_errors() {
    let mut history = History::new();
    history.push("echo one");

    assert_eq!(
        expand_history_with_chars_for_test("!missing", &history, HistoryChars::parse("!^#")),
        Err(HistoryExpansionError::EventNotFound("!missing".to_string()))
    );
    assert_eq!(
        expand_history_with_chars_for_test("!!:9", &history, HistoryChars::parse("!^#")),
        Err(HistoryExpansionError::BadWordSpecifier("9".to_string()))
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"!missing".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": history-expand-line")
        .unwrap();
    let mut hooks = TestHistoryHook;
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("!missing".as_bytes().to_vec()));
    // The failure is error text, so it goes to the diagnostic sink instead
    // of the display stream.
    assert!(
        line.terminal
            .diagnostics
            .contains("!missing: event not found\n")
    );
    assert!(!line.terminal.out.contains("!missing: event not found"));
}

#[test]
fn history_expansion_print_only_result_goes_to_the_diagnostic_sink() {
    // The print-only result is not error text, but GNU writes it to standard
    // error as well, so it takes the diagnostic sink.
    let mut history = History::new();
    history.push("echo one");

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"!echo:p".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": history-expand-line")
        .unwrap();
    let mut hooks = TestHistoryHook;
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();

    // The print-only result does not become the line.
    assert_eq!(result, ReadlineResult::Line("!echo:p".as_bytes().to_vec()));
    assert!(line.terminal.diagnostics.contains("echo one\n"));
    assert!(!line.terminal.out.contains("echo one"));
}

#[test]
fn incremental_search_abort_restores_history_buffer_ownership() {
    let keys = b"\x10\x10\x05 EDIT\x13BBB\x07\x0e\x10\x0e\r";
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys.to_vec())]);
    let mut history = History::new();
    history.push("AAA");
    history.push("BBB");
    let mut editor = Editor::new(Config::default(), terminal, history);
    editor
        .load_inputrc_str("set revert-all-at-newline off")
        .unwrap();
    let mut hooks = TestHistoryHook;

    assert_eq!(
        editor.read_line(Prompt::new("> "), &mut hooks).unwrap(),
        ReadlineResult::Line(b"BBB".to_vec())
    );
    assert_eq!(editor.history.entries()[0].line_bytes, b"AAA EDIT");
    assert!(!editor.history.undo_list(0).unwrap().is_empty());
}

#[test]
fn nonincremental_search_abort_restores_history_buffer_ownership() {
    let keys = b"\x10\x10\x05 EDIT\x0fBBB\x1b\x0e\x10\x0e\r";
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys.to_vec())]);
    let mut history = History::new();
    history.push("AAA");
    history.push("BBB");
    let mut editor = Editor::new(Config::default(), terminal, history);
    editor
        .load_inputrc_str(
            "set revert-all-at-newline off\n\"\\C-o\": non-incremental-forward-search-history",
        )
        .unwrap();
    let mut hooks = TestHistoryHook;

    assert_eq!(
        editor.read_line(Prompt::new("> "), &mut hooks).unwrap(),
        ReadlineResult::Line(b"BBB".to_vec())
    );
    assert_eq!(editor.history.entries()[0].line_bytes, b"AAA EDIT");
    assert!(!editor.history.undo_list(0).unwrap().is_empty());
}

#[test]
fn nonincremental_search_overlay_preserves_the_origin_edit() {
    let keys = b"\x10\x10\x05 EDIT\x0fBBB\r\r";
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys.to_vec())]);
    let mut history = History::new();
    history.push("AAA");
    history.push("BBB");
    let mut editor = Editor::new(Config::default(), terminal, history);
    editor
        .load_inputrc_str(
            "set revert-all-at-newline off\n\"\\C-o\": non-incremental-forward-search-history",
        )
        .unwrap();
    let mut hooks = TestHistoryHook;

    assert_eq!(
        editor.read_line(Prompt::new("> "), &mut hooks).unwrap(),
        ReadlineResult::Line(b"BBB".to_vec())
    );
    assert_eq!(editor.history.entries()[0].line_bytes, b"AAA EDIT");
    assert!(!editor.history.undo_list(0).unwrap().is_empty());
}

#[test]
fn history_search_overlay_preserves_the_origin_edit_across_navigation() {
    use crate::buffer::LineBuffer;
    use crate::prompt::Prompt;
    use crate::state::EditorState;
    use ::history::HistoryDirection;

    let terminal = MemoryTerminal::with_events(vec![]);
    let mut history = History::new();
    history.push("AAA");
    history.push("BBB");
    let mut editor = Editor::new(Config::default(), terminal, history);
    editor
        .load_inputrc_str("set revert-all-at-newline off")
        .unwrap();
    assert!(editor.history.set_pos(0));

    let mut state = EditorState::new(Prompt::new("> "), None);
    state.original_line = b"AAA".to_vec();
    state.history_entry_index = Some(0);
    state.buffer = LineBuffer::from_bytes(b"AAA".to_vec());
    state.record_undo();
    state.buffer = LineBuffer::from_bytes(b"AAA EDIT".to_vec());
    state.commit_pending_undo();
    assert!(!state.undo_snapshot_lines().is_empty());

    // A search overlay displays another entry without moving the history cursor.
    editor.replace_from_history_at(&mut state, Some(1), b"BBB");
    assert_eq!(state.buffer.as_bytes(), b"BBB");
    assert_eq!(editor.history.current_index(), Some(0));
    assert_eq!(editor.history.entries()[0].line_bytes, b"AAA EDIT");
    assert!(!editor.history.undo_list(0).unwrap().is_empty());

    // Moving after the overlay must not associate its text with the origin.
    let line = editor
        .history
        .navigate_bytes(HistoryDirection::Next, state.buffer.as_bytes().to_vec())
        .expect("next history");
    editor.replace_from_history(&mut state, &line);
    assert_eq!(editor.history.entries()[0].line_bytes, b"AAA EDIT");

    // Returning to the origin restores both its edit and its undo state.
    let line = editor
        .history
        .navigate_bytes(HistoryDirection::Previous, state.buffer.as_bytes().to_vec())
        .expect("previous history");
    editor.replace_from_history(&mut state, &line);
    assert_eq!(state.buffer.as_bytes(), b"AAA EDIT");
    assert!(state.undo());
    assert_eq!(state.buffer.as_bytes(), b"AAA");
}

#[test]
fn history_navigation_preserves_edited_entry_and_undo_ownership() {
    use crate::buffer::LineBuffer;
    use crate::prompt::Prompt;
    use crate::state::EditorState;
    use ::history::HistoryDirection;

    let terminal = MemoryTerminal::with_events(vec![]);
    let mut history = History::new();
    history.push("alpha old");
    history.push("alpha new");
    let mut editor = Editor::new(Config::default(), terminal, history);
    editor
        .load_inputrc_str("set revert-all-at-newline off")
        .unwrap();
    assert!(editor.history.set_pos(1));

    let mut state = EditorState::new(Prompt::new("> "), None);
    state.original_line = b"alpha new".to_vec();
    state.history_entry_index = Some(1);
    state.buffer = LineBuffer::from_bytes(b"alpha new".to_vec());
    state.record_undo();
    state.buffer = LineBuffer::from_bytes(b"alpha new EDIT".to_vec());
    state.commit_pending_undo();
    assert!(!state.undo_snapshot_lines().is_empty());

    // Leave the newest entry for the older one.
    let line = editor
        .history
        .navigate_bytes(HistoryDirection::Previous, state.buffer.as_bytes().to_vec())
        .expect("previous history");
    editor.replace_from_history(&mut state, &line);
    assert_eq!(state.buffer.as_bytes(), b"alpha old");
    // The edited text and its undo state stay on the entry left behind.
    assert_eq!(
        editor.history.entries()[1].line_bytes,
        b"alpha new EDIT".to_vec()
    );
    assert!(!editor.history.undo_list(1).unwrap().is_empty());
    // The destination keeps its own (empty) undo state.
    assert!(editor.history.undo_list(0).unwrap().is_empty());
    assert!(state.undo_snapshot_lines().is_empty());

    // Returning restores the preserved edit and its undo state.
    let line = editor
        .history
        .navigate_bytes(HistoryDirection::Next, state.buffer.as_bytes().to_vec())
        .expect("next history");
    assert_eq!(line, b"alpha new EDIT".to_vec());
    editor.replace_from_history(&mut state, &line);
    assert_eq!(state.buffer.as_bytes(), b"alpha new EDIT");
    assert!(!state.undo_snapshot_lines().is_empty());
    assert!(state.undo());
    assert_eq!(state.buffer.as_bytes(), b"alpha new");
}

#[test]
fn history_search_restores_origin_undo_after_overlay_navigation() {
    for (case, search_keys) in [
        ("incremental", &b"\x1dBBB\n\x10"[..]),
        ("nonincremental", &b"\x0fBBB\r\x0e\x10"[..]),
        ("incremental abort", &b"\x1dBBB\x07\x0e\x10"[..]),
        ("nonincremental abort", &b"\x0fBBB\x07\x0e\x10"[..]),
        ("failed incremental command", &b"\x1dBBBZ\x06\x0e\x10"[..]),
        (
            "erased incremental query",
            &b"\x1dBBB\x7f\x7f\x7f\n\x0e\x10"[..],
        ),
    ] {
        let keys = [b"\x10\x10\x05 EDIT".as_slice(), search_keys, b"\x1f\r"].concat();
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys)]);
        let mut history = History::new();
        history.push("AAA");
        history.push("BBB");
        let mut editor = Editor::new(Config::default(), terminal, history);
        editor
            .load_inputrc_str(
                "set revert-all-at-newline off\n\"\\C-o\": non-incremental-forward-search-history\n\"\\C-]\": forward-search-history",
            )
            .unwrap();
        assert_eq!(
            editor
                .read_line(Prompt::new("> "), &mut TestHistoryHook)
                .unwrap(),
            ReadlineResult::Line(b"AAA".to_vec()),
            "{case}"
        );
        assert_eq!(
            editor.history.entries()[0].line_bytes,
            b"AAA EDIT",
            "{case}"
        );
        assert_eq!(editor.history.entries()[1].line_bytes, b"BBB", "{case}");
    }
}

#[test]
fn incremental_history_search_keeps_the_edit_when_matching_its_own_entry() {
    for terminator in *b"\n\x06\r" {
        let keys = [b"\x10\x05 EDIT\x12A".as_slice(), &[terminator], b"\r"].concat();
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys)]);
        let mut history = History::new();
        history.push("AAA");
        let mut editor = Editor::new(Config::default(), terminal, history);
        editor
            .load_inputrc_str("set revert-all-at-newline off")
            .unwrap();
        assert_eq!(
            editor
                .read_line(Prompt::new("> "), &mut TestHistoryHook)
                .unwrap(),
            ReadlineResult::Line(b"AAA EDIT".to_vec()),
            "terminator {terminator}"
        );
    }
}

#[test]
fn reverse_incremental_search_matches_invalid_utf8_history_fixture() {
    let terminal =
        MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"\x12alpha\n\r".to_vec())]);
    let mut history = History::new();
    history.push_bytes(b"pre\xffALPHA".to_vec());
    let mut editor = Editor::new(Config::default(), terminal, history);
    editor
        .load_inputrc_str("set search-ignore-case on")
        .unwrap();

    assert_eq!(
        editor
            .read_line(Prompt::new("> "), &mut TestHistoryHook)
            .unwrap(),
        ReadlineResult::Line(b"pre\xffALPHA".to_vec())
    );
}

#[test]
fn prefix_search_matches_invalid_utf8_history_fixture() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"alpha\x0f\r".to_vec())]);
    let mut history = History::new();
    history.push_bytes(b"ALPHA\xfftail".to_vec());
    assert!(history.set_pos(history.len()));
    let mut editor = Editor::new(Config::default(), terminal, history);
    editor
        .load_inputrc_str("set search-ignore-case on\n\"\\C-o\": history-search-backward")
        .unwrap();

    assert_eq!(
        editor
            .read_line(Prompt::new("> "), &mut TestHistoryHook)
            .unwrap(),
        ReadlineResult::Line(b"ALPHA\xfftail".to_vec())
    );
}
