use super::*;

#[test]
fn editor_new_retains_initial_inputrc_error_and_try_new_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inputrc");
    std::fs::write(&path, "$else\n").unwrap();
    let config = Config {
        inputrc_path: crate::config::InputrcPath::Path(path),
        ..Config::default()
    };

    let line = Editor::new(config.clone(), MemoryTerminal::default(), History::new());
    assert!(
        line.initial_inputrc_error()
            .is_some_and(|err| err.contains("$else without $if")),
        "{:?}",
        line.initial_inputrc_error()
    );

    let err = match Editor::try_new(config, MemoryTerminal::default(), History::new()) {
        Ok(_) => panic!("try_new unexpectedly accepted invalid inputrc"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("$else without $if"), "{err:?}");
}

#[test]
fn every_readline_command_is_in_typed_or_named_dispatch_table() {
    for command in crate::keymap::BIND_FUNCTION_NAMES {
        assert!(
            EditCommand::parse(command).is_some()
                || NAMED_READLINE_COMMAND_DISPATCH
                    .binary_search(command)
                    .is_ok(),
            "{command} must have an explicit dispatch classification"
        );
    }
}

#[test]
fn effective_prompt_width_uses_last_line_only() {
    let mut line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    line.load_inputrc_str("set show-mode-in-prompt on").unwrap();
    line.variables_mut()
        .insert("emacs-mode-string".to_string(), "ab\ncde".to_string());

    // Multiline prompt: mode lines do not contribute to the last line.
    let state = EditorState::new(Prompt::new("12\n345"), None);
    let (text, width) = line.effective_prompt(&state);
    assert_eq!(text, "ab\ncde12\n345");
    assert_eq!(width, 3);

    // Single-line prompt: only the mode last line contributes.
    let state = EditorState::new(Prompt::new("XY"), None);
    let (_, width) = line.effective_prompt(&state);
    assert_eq!(width, 3 + 2);
}

#[test]
fn do_lowercase_version_self_binding_does_not_recurse() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"1".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"a\": do-lowercase-version\n\"1\": do-lowercase-version")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"".to_vec()));
    assert!(
        line.terminal.out.contains("\x07"),
        "self-binding without case difference must ding, got {:?}",
        line.terminal.out
    );
}

#[test]
fn do_lowercase_version_still_lowercases_uppercase_key() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x1b, b'A']),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"a".to_vec()));
}

#[test]
fn resize_columns_change_recomputes_prompt_wrap() {
    let line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let output = "a".repeat(30);

    let mut narrow = EditorState::new(Prompt::new("> "), None);
    narrow.display.last_terminal_size = Some(TerminalSize {
        columns: 20,
        rows: 24,
    });
    let mut wide = EditorState::new(Prompt::new("> "), None);
    wide.display.last_terminal_size = Some(TerminalSize {
        columns: 80,
        rows: 24,
    });

    // Resize plumbing follows the latest size for both shrink and grow.
    assert_eq!(line.tracked_terminal_columns(&narrow), 20);
    assert_eq!(line.tracked_terminal_columns(&wide), 80);

    // Per-render measurement recomputes wrap from those columns, matching
    // unconditional recompute semantics without a cached newlines array.
    let (narrow_rows, _) =
        crate::width::measured_rows_for_output(&output, line.tracked_terminal_columns(&narrow));
    let (wide_rows, _) =
        crate::width::measured_rows_for_output(&output, line.tracked_terminal_columns(&wide));
    assert_eq!(narrow_rows, 1);
    assert_eq!(wide_rows, 0);
}

#[test]
fn batched_numeric_continuation_matches_fragmented_reads() {
    // Live batching may deliver `1\x1bl` (digit run plus terminator)
    // in one `Bytes` chunk after `M--`; the one-byte path saw `1` then
    // `ESC l` separately. Both must yield `FOO bar`.
    for terminal in [
        // Fragmented (one `read` per byte, pre-batch shape).
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"FOO BAR".to_vec()),
            TerminalEvent::Bytes(vec![0x1b, b'-']),
            TerminalEvent::Bytes(b"1".to_vec()),
            TerminalEvent::Bytes(vec![0x1b, b'l']),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
        // Batched (one `read` returns run plus terminator).
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"FOO BAR".to_vec()),
            TerminalEvent::Bytes(vec![0x1b, b'-', b'1', 0x1b, b'l']),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
    ] {
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"FOO bar".to_vec()),
            "batched and fragmented numeric continuations must agree"
        );
    }
}

#[test]
fn batched_invalid_multibyte_chunk_preserves_both_bytes() {
    // Pin-independent lock: a single `Bytes([0xFF, 0x41])` chunk through the
    // live dispatch preserves both bytes. Each byte is bound to `self-insert`
    // in the default map, so `longest_matching_prefix` splits the chunk and
    // the invalid-byte `insert_bytes` fallback keeps `0xFF`. The
    // `handle_unbound` `>= 0x80` filter is not reached for this chunk; the
    // same filter in search/replace accumulation still needs oracle review.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0xFF, 0x41]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line(vec![0xFF, 0x41]),
        "batched invalid chunk must preserve both bytes via split self-insert"
    );
}
