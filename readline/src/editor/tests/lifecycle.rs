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

#[test]
fn batched_mixed_control_and_multibyte_matches_fragmented_reads() {
    // Live batching may deliver `a`, `C-a` (beginning-of-line) and `é`
    // in one `Bytes` chunk; the one-byte path saw them separately. Both
    // must dispatch `C-a` as a command instead of swallowing it in a
    // bulk multibyte insert, yielding `éa`.
    let mixed = [b"a".as_slice(), &[0x01], "é".as_bytes()].concat();
    for terminal in [
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"a".to_vec()),
            TerminalEvent::Bytes(vec![0x01]),
            TerminalEvent::Bytes("é".as_bytes().to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(mixed),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
    ] {
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line("éa".as_bytes().to_vec()),
            "batched and fragmented mixed control/multibyte must agree"
        );
    }
}

#[test]
fn batched_invalid_search_chunk_preserves_ascii_like_fragmented_reads() {
    // Incremental search accumulation must keep ASCII non-controls even
    // when the chunk as a whole is invalid UTF-8. History holds `[0xFF]`
    // and `A`; batched `[0xFF, A]` must query both bytes (no match,
    // original line) exactly like split `[0xFF]` + `[A]` reads.
    fn history_with_invalid_entry() -> History {
        let mut history = History::new();
        history.push_bytes(vec![0xFF]);
        history.push("A");
        history
    }
    for terminal in [
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(vec![0x12]),
            TerminalEvent::Bytes(vec![0xFF, b'A']),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(vec![0x12]),
            TerminalEvent::Bytes(vec![0xFF]),
            TerminalEvent::Bytes(vec![b'A']),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
    ] {
        let mut line = Editor::new(Config::default(), terminal, history_with_invalid_entry());
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"".to_vec()),
            "batched and fragmented invalid search chunks must agree"
        );
    }
}

#[test]
fn batched_invalid_non_incremental_search_chunk_matches_fragmented_reads() {
    // Same ASCII-preservation rule for non-incremental search accumulation.
    fn history_with_invalid_entry() -> History {
        let mut history = History::new();
        history.push_bytes(vec![0xFF]);
        history.push("A");
        history
    }
    for terminal in [
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(vec![0x1b, b'p']),
            TerminalEvent::Bytes(vec![0xFF, b'A']),
            TerminalEvent::Bytes(b"\r".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(vec![0x1b, b'p']),
            TerminalEvent::Bytes(vec![0xFF]),
            TerminalEvent::Bytes(vec![b'A']),
            TerminalEvent::Bytes(b"\r".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
    ] {
        let mut line = Editor::new(Config::default(), terminal, history_with_invalid_entry());
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"".to_vec()),
            "batched and fragmented non-incremental search must agree"
        );
    }
}

#[test]
fn batched_search_control_matches_fragmented_reads() {
    // Reviewer repro: history ["A_first", "A_second"], C-r then [A, C-r]
    // batched must agree with [A], [C-r] fragmented reads (both accept
    // "A_first"); the whole-chunk match previously swallowed the direction
    // toggle and stayed on "A_second".
    fn history() -> History {
        let mut history = History::new();
        history.push("A_first");
        history.push("A_second");
        history
    }
    for terminal in [
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(vec![0x12]),
            TerminalEvent::Bytes(vec![b'A', 0x12]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(vec![0x12]),
            TerminalEvent::Bytes(vec![b'A']),
            TerminalEvent::Bytes(vec![0x12]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
    ] {
        let mut line = Editor::new(Config::default(), terminal, history());
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"A_first".to_vec()),
            "batched and fragmented search direction toggles must agree"
        );
    }
}

#[test]
fn batched_non_incremental_query_plus_enter_matches_fragmented_reads() {
    // Non-incremental search must execute an embedded Enter instead of
    // swallowing it as query text: [A, CR] batched agrees with [A], [CR].
    fn history() -> History {
        let mut history = History::new();
        history.push("A_first");
        history.push("A_second");
        history
    }
    for terminal in [
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(vec![0x1b, b'p']),
            TerminalEvent::Bytes(vec![b'A', b'\r']),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
        super::MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(vec![0x1b, b'p']),
            TerminalEvent::Bytes(vec![b'A']),
            TerminalEvent::Bytes(b"\r".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]),
    ] {
        let mut line = Editor::new(Config::default(), terminal, history());
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"A_second".to_vec()),
            "batched and fragmented non-incremental Enter must agree"
        );
    }
}

#[test]
fn batched_vi_replace_remainder_matches_fragmented_reads() {
    // `r` consumes one replacement char; trailing bytes are separate input.
    // Batched [X,d,l] must replace with X then delete like split reads
    // (both accept "bc"), instead of dropping the trailing operator.
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set editing-mode vi").unwrap();
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"r".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"l".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"r".to_vec()),
        TerminalEvent::Bytes(b"Xdl".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"bc".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented vi replace remainder must agree"
    );
}

#[test]
fn batched_vi_char_search_remainder_matches_fragmented_reads() {
    // `f` consumes one search key; trailing bytes are separate input.
    // Batched [,,d,l] must move to `,` then delete like split reads.
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set editing-mode vi").unwrap();
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc,def".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"f".to_vec()),
        TerminalEvent::Bytes(b",".to_vec()),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"l".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc,def".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"f".to_vec()),
        TerminalEvent::Bytes(b",dl".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"abcdef".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented vi char-search remainder must agree"
    );
}

#[test]
fn batched_vi_mark_remainder_matches_fragmented_reads() {
    // `m` consumes one mark name; trailing bytes are separate input.
    // Batched [a,i,Z] must set mark a then enter insert like split reads.
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set editing-mode vi").unwrap();
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"m".to_vec()),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"m".to_vec()),
        TerminalEvent::Bytes(b"aiZ".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"Zabc".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented vi mark remainder must agree"
    );
}

#[test]
fn batched_vi_register_remainder_matches_fragmented_reads() {
    // `pending_vi_register` consumes one register name; trailing bytes are
    // separate input. `vi-set-register` is not a GNU-bindable name (bash
    // `bind -l` omits it, pinned by bind_golden), so bind `Q` directly
    // through the keymap instead of inputrc. Batched [a,i,Z] must select
    // register a then enter insert like split reads.
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set editing-mode vi").unwrap();
        line.keymap.bind(
            crate::keymap::KeyMapName::ViCommand,
            crate::keymap::KeySequence::new(b"Q".to_vec()),
            crate::keymap::KeyBinding::NamedCommand("vi-set-register".to_string()),
        );
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"Q".to_vec()),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"Q".to_vec()),
        TerminalEvent::Bytes(b"aiZ".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"Zabc".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented vi register remainder must agree"
    );
}

#[test]
fn batched_named_command_enter_matches_fragmented_reads() {
    // Named-command text mixed with Enter in one chunk must execute like
    // split reads instead of swallowing the terminator as query text.
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("\"\\C-o\": execute-named-command")
            .unwrap();
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"beginning-of-line".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"beginning-of-line\r".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"Xabc".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented named-command Enter must agree"
    );
}

#[test]
fn batched_vi_pending_invalid_chunk_matches_fragmented_reads() {
    // Pending vi mark consumes one unit; an invalid byte dings and trailing
    // bytes are separate input. Batched [0xFF,i,Z] must agree with split
    // [0xFF],[i],[Z] reads (both accept "Zabc").
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set editing-mode vi").unwrap();
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"m".to_vec()),
        TerminalEvent::Bytes(vec![0xFF]),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"m".to_vec()),
        TerminalEvent::Bytes(vec![0xFF, b'i', b'Z']),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"Zabc".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented vi pending invalid chunks must agree"
    );
}

#[test]
fn batched_vi_register_invalid_chunk_matches_fragmented_reads() {
    // Pending vi register consumes one unit; an invalid byte dings and
    // trailing bytes are separate input. Batched [0xFF,i,Z] must agree with
    // split [0xFF],[i],[Z] reads (both accept "Zabc").
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set editing-mode vi").unwrap();
        line.keymap.bind(
            crate::keymap::KeyMapName::ViCommand,
            crate::keymap::KeySequence::new(b"Q".to_vec()),
            crate::keymap::KeyBinding::NamedCommand("vi-set-register".to_string()),
        );
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"Q".to_vec()),
        TerminalEvent::Bytes(vec![0xFF]),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"Q".to_vec()),
        TerminalEvent::Bytes(vec![0xFF, b'i', b'Z']),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"Zabc".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented vi register invalid chunks must agree"
    );
}

#[test]
fn batched_vi_replace_invalid_chunk_matches_fragmented_reads() {
    // Pending vi replace consumes one unit; an invalid byte replaces with
    // that byte and trailing bytes are separate input. Batched [0xFF,d,l]
    // must agree with split [0xFF],[d],[l] reads (both accept "bc").
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set editing-mode vi").unwrap();
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"r".to_vec()),
        TerminalEvent::Bytes(vec![0xFF]),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"l".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"r".to_vec()),
        TerminalEvent::Bytes(vec![0xFF, b'd', b'l']),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"bc".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented vi replace invalid chunks must agree"
    );
}

#[test]
fn batched_vi_char_search_invalid_chunk_matches_fragmented_reads() {
    // Pending vi char-search consumes one unit; an unfound invalid key
    // cancels and trailing bytes are separate input. Batched [0xFF,d,l]
    // must agree with split [0xFF],[d],[l] reads (both accept "bc,def").
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set editing-mode vi").unwrap();
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc,def".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"f".to_vec()),
        TerminalEvent::Bytes(vec![0xFF]),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"l".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc,def".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"f".to_vec()),
        TerminalEvent::Bytes(vec![0xFF, b'd', b'l']),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"bc,def".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented vi char-search invalid chunks must agree"
    );
}

#[test]
fn batched_named_command_invalid_chunk_matches_fragmented_reads() {
    // Named-command query drops invalid bytes; trailing ASCII plus Enter
    // in one chunk must execute like split reads. Batched [0xFF,
    // "beginning-of-line", CR] must agree with split [0xFF],
    // ["beginning-of-line"], [CR] reads (both accept "Xabc").
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("\"\\C-o\": execute-named-command")
            .unwrap();
        line.read_line(Prompt::new("> "), &mut ()).unwrap()
    }
    let fragmented = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0xFF]),
        TerminalEvent::Bytes(b"beginning-of-line".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut batched_query = vec![0xFF];
    batched_query.extend_from_slice(b"beginning-of-line\r");
    let batched = run(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(batched_query),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(fragmented, ReadlineResult::Line(b"Xabc".to_vec()));
    assert_eq!(
        batched, fragmented,
        "batched and fragmented named-command invalid chunks must agree"
    );
}

#[test]
fn vi_search_again_steps_exclusively_past_matches_with_bell() {
    // Vi `/` lands on the newest match; each `n` must step exclusively to
    // the older match, and exhausting matches must bell and keep the line.
    fn run(events: Vec<TerminalEvent>) -> (ReadlineResult, String) {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut history = History::new();
        history.push("alpha one");
        history.push("alpha two");
        history.push("gamma");
        let mut line = Editor::new(Config::default(), terminal, history);
        line.load_inputrc_str("set editing-mode vi").unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        (result, line.terminal.out.clone())
    }
    // `n` steps from "alpha two" to "alpha one"; second `n` exhausts matches.
    // GNU vi `/` is non-incremental: the first Enter executes the query
    // (prompt `/alpha` hidden until then) and the final Enter accepts.
    let (result, out) = run(vec![
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"/".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"n".to_vec()),
        TerminalEvent::Bytes(b"n".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(result, ReadlineResult::Line(b"alpha one".to_vec()));
    assert!(
        out.contains("\x07"),
        "exhausted repeat must bell, got {out:?}"
    );
    // `N` flips to forward: from the newest alpha there is no newer match,
    // so it must bell and keep "alpha two".
    let (result, out) = run(vec![
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"/".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"N".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(result, ReadlineResult::Line(b"alpha two".to_vec()));
    assert!(
        out.contains("\x07"),
        "failed opposite repeat must bell, got {out:?}"
    );
    // No prior search must bell without changing the line.
    let terminal = super::MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"n".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"".to_vec()));
    assert!(
        line.terminal.out.contains("\x07"),
        "repeat without search must bell, got {:?}",
        line.terminal.out
    );
}

#[test]
fn incremental_no_match_bells_per_keystroke_without_extra_at_terminate() {
    // GNU emacs incremental search (patch 0 Bash 5.3 PTY oracle) rings
    // once per failing query extension: `C-r zzz` yields three bells.
    // Terminating that failed query (C-J) and accepting the line add no
    // extra bell, and the line stays the original.
    fn run(events: Vec<TerminalEvent>) -> (ReadlineResult, String) {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut history = History::new();
        history.push("alpha one");
        history.push("alpha two");
        let mut line = Editor::new(Config::default(), terminal, history);
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        (result, line.terminal.out.clone())
    }
    let (result, out) = run(vec![
        TerminalEvent::Bytes(b"draft".to_vec()),
        TerminalEvent::Bytes(vec![0x12]),
        TerminalEvent::Bytes(b"zzz".to_vec()),
        TerminalEvent::Bytes(b"\n".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(result, ReadlineResult::Line(b"draft".to_vec()));
    assert_eq!(
        out.bytes().filter(|byte| *byte == b'\x07').count(),
        3,
        "each failing keystroke must bell once, got {out:?}"
    );
    // A matching query stays silent.
    let (result, out) = run(vec![
        TerminalEvent::Bytes(vec![0x12]),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(result, ReadlineResult::Line(b"alpha two".to_vec()));
    assert!(
        !out.contains("\x07"),
        "matching query must not bell, got {out:?}"
    );
    // A failed repeat toggle keeps the line and bells once.
    let (result, out) = run(vec![
        TerminalEvent::Bytes(vec![0x12]),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(vec![0x12]),
        TerminalEvent::Bytes(vec![0x12]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(result, ReadlineResult::Line(b"alpha one".to_vec()));
    assert!(
        out.contains("\x07"),
        "exhausted repeat toggle must bell, got {out:?}"
    );
}

#[test]
fn vi_no_match_execute_bells_once_and_keeps_original() {
    // GNU vi `/` is non-incremental (patch 0 Bash 5.3 PTY oracle): the
    // query stays in the `/query` prompt until Enter executes, and a
    // failed or empty execute bells once while the line stays original.
    fn run(events: Vec<TerminalEvent>) -> (ReadlineResult, String) {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut history = History::new();
        history.push("alpha one");
        history.push("alpha two");
        let mut line = Editor::new(Config::default(), terminal, history);
        line.load_inputrc_str("set editing-mode vi").unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        (result, line.terminal.out.clone())
    }
    let (result, out) = run(vec![
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"/".to_vec()),
        TerminalEvent::Bytes(b"zzz".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(result, ReadlineResult::Line(b"".to_vec()));
    assert_eq!(
        out.bytes().filter(|byte| *byte == b'\x07').count(),
        1,
        "vi no-match execute must bell exactly once, got {out:?}"
    );
    assert!(
        out.contains("/zzz"),
        "vi prompt must show `/zzz` while querying, got {out:?}"
    );
    // A matching vi query stays silent at execute.
    let (result, out) = run(vec![
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"/".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert_eq!(result, ReadlineResult::Line(b"alpha two".to_vec()));
    assert!(
        !out.contains("\x07"),
        "vi matching execute must not bell, got {out:?}"
    );
    assert!(
        out.contains("/alpha"),
        "vi prompt must show `/alpha` while querying, got {out:?}"
    );
    // Emacs non-incremental search shows `:` while querying.
    let terminal = super::MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1bp".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push("alpha one");
    history.push("alpha two");
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\ep\": non-incremental-reverse-search-history")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"alpha two".to_vec()));
    assert!(
        line.terminal.out.contains(":alpha"),
        "emacs non-incremental prompt must show `:alpha`, got {:?}",
        line.terminal.out
    );
}
