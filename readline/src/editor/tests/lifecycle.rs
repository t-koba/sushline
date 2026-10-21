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
fn inputrc_diagnostics_go_to_the_terminal_diagnostic_sink() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(vec![b'\r'])]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set bell-style no-such-style")
        .unwrap();
    line.read_line(Prompt::new("P>"), &mut ()).unwrap();

    let terminal = &line.terminal;
    assert!(
        terminal
            .diagnostics
            .contains("could not set value to `no-such-style'"),
        "{}",
        terminal.diagnostics
    );
    assert!(
        !terminal.out.contains("could not set value"),
        "{}",
        terminal.out
    );
}

#[test]
fn every_readline_command_is_in_typed_or_named_dispatch_table() {
    assert!(
        NAMED_READLINE_COMMAND_DISPATCH
            .binary_search(&"vi-set-register")
            .is_ok()
    );
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
fn batched_search_query_draws_like_split_reads() {
    // GNU Readline redraws the search display for every query key, so a
    // batch that carries the query with the search key draws the same
    // screen as the split reads. A Bash 5.3 PTY observation: the batched
    // write and the split write produce the same output.
    fn run(events: Vec<TerminalEvent>) -> (String, Vec<u16>) {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.read_line(Prompt::new("> "), &mut ()).unwrap();
        (
            line.terminal.out.clone(),
            line.terminal.moved_columns.clone(),
        )
    }
    let split = run(vec![
        TerminalEvent::Bytes(vec![0x12]),
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"\x12ab".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    assert!(
        split.0.contains("reverse-i-search)`ab"),
        "the split reads must draw the query: {:?}",
        split.0
    );
    assert!(
        batched.0.contains("reverse-i-search)`ab"),
        "the batched reads must draw the query: {:?}",
        batched.0
    );
    assert_eq!(
        batched, split,
        "the batched query must draw like the split reads"
    );
}

#[test]
fn batched_display_changes_draw_like_split_reads() {
    // A key that changes the display without running a command, like the
    // query of a search or the target of the vi character search, redraws
    // at once, so a batch that carries such a key with the command
    // before it draws like the split reads.
    fn run(inputrc: &str, events: Vec<Vec<u8>>) -> (String, Vec<u16>) {
        let terminal = super::MemoryTerminal::with_events(
            events.into_iter().map(TerminalEvent::Bytes).collect(),
        );
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(inputrc).unwrap();
        line.read_line(Prompt::new("> "), &mut ()).unwrap();
        (
            line.terminal.out.clone(),
            line.terminal.moved_columns.clone(),
        )
    }
    type Case = (&'static str, Vec<Vec<u8>>, Vec<Vec<u8>>);
    let cases: [Case; 3] = [
        // The non-incremental search query.
        (
            "",
            vec![vec![0x1b, b'p'], b"ab".to_vec(), vec![b'\r'], vec![b'\r']],
            vec![b"\x1bpab".to_vec(), vec![b'\r'], vec![b'\r']],
        ),
        // The vi character search target.
        (
            "set editing-mode vi",
            vec![b"abc,def\x1b0f".to_vec(), vec![b','], vec![b'\r']],
            vec![b"abc,def\x1b0f,".to_vec(), vec![b'\r']],
        ),
        // The vi overwrite-mode replacement.
        (
            "set editing-mode vi",
            vec![b"abc\x1b0r".to_vec(), vec![b'X'], vec![b'\r']],
            vec![b"abc\x1b0rX".to_vec(), vec![b'\r']],
        ),
    ];
    for (inputrc, split, batched) in cases {
        let split_drawing = run(inputrc, split.clone());
        let batched_drawing = run(inputrc, batched.clone());
        assert_eq!(
            batched_drawing, split_drawing,
            "the batched drawing must match the split drawing for {split:?}"
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
fn vi_register_named_command_has_no_effect_when_extension_is_disabled() {
    let terminal = super::MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0dw".to_vec()),
        TerminalEvent::Bytes(b"iDEF".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0dw".to_vec()),
        TerminalEvent::Bytes(b"Qa\x1bP\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    // Raw keymap insertion bypasses the bind API intentionally: execution
    // still must not activate a disabled implementation-specific command.
    line.keymap.bind(
        crate::keymap::KeyMapName::ViCommand,
        crate::keymap::KeySequence::new(b"Q".to_vec()),
        crate::keymap::KeyBinding::NamedCommand("vi-set-register".to_string()),
    );
    assert_eq!(
        line.read_line(Prompt::new("> "), &mut ()).unwrap(),
        ReadlineResult::Line(b"DEF".to_vec())
    );
}

#[test]
fn vi_register_extension_binds_from_inputrc_and_controls_kill_put() {
    let terminal = super::MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0Qadw".to_vec()),
        TerminalEvent::Bytes(b"iDEF".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0dw".to_vec()),
        TerminalEvent::Bytes(b"QaP\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(
        "set editing-mode vi\nset sushline-vi-registers on\nset keymap vi-command\n\"Q\": vi-set-register",
    )
    .unwrap();
    assert_eq!(
        line.read_line(Prompt::new("> "), &mut ()).unwrap(),
        ReadlineResult::Line(b"abc".to_vec())
    );
}

#[test]
fn vi_register_extension_runs_through_execute_named_command() {
    let terminal = super::MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1b]),
        TerminalEvent::Bytes(b"0Qvi-set-register\ra".to_vec()),
        TerminalEvent::Bytes(b"iZ\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(
        "set editing-mode vi\nset sushline-vi-registers on\nset keymap vi-command\n\"Q\": execute-named-command",
    )
    .unwrap();
    assert_eq!(
        line.read_line(Prompt::new("> "), &mut ()).unwrap(),
        ReadlineResult::Line(b"Zabc".to_vec())
    );
}

#[test]
fn batched_vi_register_remainder_matches_fragmented_reads() {
    // The explicitly enabled Sushline extension consumes one register name;
    // trailing bytes are separate input. Batched [a,i,Z] must select
    // register a then enter insert like split reads.
    fn run(events: Vec<TerminalEvent>) -> ReadlineResult {
        let terminal = super::MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set editing-mode vi\nset sushline-vi-registers on")
            .unwrap();
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
        line.load_inputrc_str("set editing-mode vi\nset sushline-vi-registers on")
            .unwrap();
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

#[test]
fn setup_failure_after_raw_entry_still_restores_terminal_mode() {
    #[derive(Default)]
    struct FailingSetupTerminal {
        fail_meta: bool,
        fail_keypad: bool,
        fail_write: bool,
        fail_flush: bool,
        raw: bool,
        restores: usize,
    }

    impl crate::terminal::TerminalIo for FailingSetupTerminal {
        fn enter_raw_mode(&mut self) -> std::io::Result<()> {
            self.raw = true;
            Ok(())
        }
        fn restore_mode(&mut self) -> std::io::Result<()> {
            self.restores += 1;
            self.raw = false;
            Ok(())
        }
        fn read_event(
            &mut self,
            _: Option<std::time::Duration>,
        ) -> std::io::Result<crate::terminal::TerminalEvent> {
            Ok(crate::terminal::TerminalEvent::Timeout)
        }
        fn write(&mut self, _: &str) -> std::io::Result<()> {
            if self.fail_write {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "injected write failure",
                ));
            }
            Ok(())
        }
        fn write_bytes(&mut self, _: &[u8]) -> std::io::Result<()> {
            if self.fail_write {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "injected write failure",
                ));
            }
            Ok(())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            if self.fail_flush {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "injected flush failure",
                ));
            }
            Ok(())
        }
        fn size(&self) -> std::io::Result<crate::terminal::TerminalSize> {
            Ok(crate::terminal::TerminalSize {
                columns: 80,
                rows: 24,
            })
        }
        fn clear_after_cursor(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn move_to_column(&mut self, _: u16) -> std::io::Result<()> {
            Ok(())
        }
        fn set_meta_key_enabled(&mut self, _: bool) -> std::io::Result<()> {
            if self.fail_meta {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "injected meta failure",
                ));
            }
            Ok(())
        }
        fn set_application_keypad_enabled(&mut self, _: bool) -> std::io::Result<()> {
            if self.fail_keypad {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "injected keypad failure",
                ));
            }
            Ok(())
        }
    }

    // Each post-entry setup operation fails while raw mode was entered;
    // cleanup must still restore the terminal and the setup error survives.
    for (name, make_terminal) in [
        (
            "meta",
            (|t: &mut FailingSetupTerminal| t.fail_meta = true) as fn(&mut FailingSetupTerminal),
        ),
        (
            "keypad",
            (|t: &mut FailingSetupTerminal| t.fail_keypad = true) as fn(&mut FailingSetupTerminal),
        ),
        (
            "write",
            (|t: &mut FailingSetupTerminal| t.fail_write = true) as fn(&mut FailingSetupTerminal),
        ),
        (
            "flush",
            (|t: &mut FailingSetupTerminal| t.fail_flush = true) as fn(&mut FailingSetupTerminal),
        ),
    ] {
        let mut terminal = FailingSetupTerminal::default();
        make_terminal(&mut terminal);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        // Keep this failure-path test independent of TERM-sensitive defaults:
        // keypad failure requires an explicit on override, and bracketed paste
        // exercises setup writes for the write/flush cases.
        let inputrc = if name == "keypad" {
            "set enable-keypad on\nset enable-bracketed-paste on"
        } else {
            "set enable-bracketed-paste on"
        };
        line.load_inputrc_str(inputrc).unwrap();
        assert!(
            line.flag(crate::variables::BoolVariable::EnableBracketedPaste),
            "{name}: bracketed paste must be on for write/flush coverage"
        );
        if name == "keypad" {
            assert!(line.flag(crate::variables::BoolVariable::EnableKeypad));
        }
        let err = match line.read_line(Prompt::new("> "), &mut ()) {
            Ok(_) => panic!("{name}: setup failure must error"),
            Err(err) => err,
        };
        assert_eq!(
            err.to_string(),
            "injected ".to_string() + name + " failure",
            "{name}: original setup error must survive cleanup"
        );
        assert!(
            !line.terminal.raw,
            "{name}: raw mode must be restored after setup failure"
        );
        assert_eq!(
            line.terminal.restores, 1,
            "{name}: restore must run once after setup failure"
        );
    }
}

#[test]
fn positive_digit_overflow_resets_without_panic() {
    // A numeric argument may not exceed 1000000, as measured on GNU
    // Readline 8.2: ESC + 13 nines fills the argument with six nines, the
    // seventh exceeds the limit and resets the argument with a bell so no
    // huge repeat executes, and the six digits that follow the reset are
    // ordinary input, as they are on GNU. The editor dispatches the input
    // one byte at a time, like GNU Readline's per-character reads.
    let terminal = super::MemoryTerminal::with_events(vec![crate::terminal::TerminalEvent::Bytes(
        b"\x1b9999999999999\x01\r".to_vec(),
    )]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ());
    assert_eq!(
        result.unwrap(),
        ReadlineResult::Line(b"999999".to_vec()),
        "overflowing positive arg must not panic and must accept"
    );
    assert!(
        line.terminal.out.contains("\x07"),
        "overflow must bell, got {:?}",
        line.terminal.out
    );
}

#[test]
fn negative_digit_overflow_resets_without_panic() {
    // The negative argument has the same limit: the magnitude may not
    // exceed 1000000, so the same reset policy applies, and the digits
    // that follow the reset are ordinary input, as they are on GNU.
    let terminal = super::MemoryTerminal::with_events(vec![crate::terminal::TerminalEvent::Bytes(
        b"\x1b-9999999999999\x01\r".to_vec(),
    )]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ());
    assert_eq!(
        result.unwrap(),
        ReadlineResult::Line(b"999999".to_vec()),
        "overflowing negative arg must not panic and must accept"
    );
    assert!(
        line.terminal.out.contains("\x07"),
        "negative overflow must bell, got {:?}",
        line.terminal.out
    );
}

#[test]
fn numeric_argument_accepts_the_limit_and_resets_above_it() {
    // The limit is the magnitude 1000000, as measured on GNU Readline 8.2:
    // the argument of 1000000 is accepted without a bell, and the argument
    // of 10000001 exceeds the limit, so the bell rings and the argument is
    // reset. The digits of the argument are consumed either way, so the
    // accepted line is empty.
    let terminal = super::MemoryTerminal::with_events(vec![crate::terminal::TerminalEvent::Bytes(
        b"\x1b1000000\r".to_vec(),
    )]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ());
    assert_eq!(
        result.unwrap(),
        ReadlineResult::Line(Vec::new()),
        "the argument at the limit must be accepted"
    );
    assert!(
        !line.terminal.out.contains("\x07"),
        "the argument at the limit must not bell, got {:?}",
        line.terminal.out
    );

    let terminal = super::MemoryTerminal::with_events(vec![crate::terminal::TerminalEvent::Bytes(
        b"\x1b10000001\r".to_vec(),
    )]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ());
    assert_eq!(
        result.unwrap(),
        ReadlineResult::Line(Vec::new()),
        "the argument above the limit must be consumed with the argument"
    );
    assert!(
        line.terminal.out.contains("\x07"),
        "the argument above the limit must bell, got {:?}",
        line.terminal.out
    );
}

#[test]
fn repeated_universal_argument_overflow_resets_without_panic() {
    // The universal argument obeys the same limit as the digit argument:
    // 4^9 is 262144 and 4^10 is 1048576, which exceeds 1000000, so the
    // tenth universal argument must reset with a bell instead of panicking
    // (debug) or wrapping to 0 (release). With reset the following `a`
    // inserts once; wrap would insert zero.
    let mut events = Vec::new();
    for _ in 0..10 {
        events.push(crate::terminal::TerminalEvent::Bytes(vec![0x15]));
    }
    events.push(crate::terminal::TerminalEvent::Bytes(b"a".to_vec()));
    events.push(crate::terminal::TerminalEvent::Bytes(b"\r".to_vec()));
    let terminal = super::MemoryTerminal::with_events(events);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-u\": universal-argument")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ());
    assert_eq!(
        result.unwrap(),
        ReadlineResult::Line(b"a".to_vec()),
        "universal overflow must reset to no-arg, not wrap to zero"
    );
    assert!(
        line.terminal.out.contains("\x07"),
        "universal overflow must bell, got {:?}",
        line.terminal.out
    );
}

#[test]
fn lone_bracketed_paste_end_dispatches_when_fragmented() {
    let mut line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    line.load_inputrc_str("set enable-bracketed-paste off")
        .unwrap();
    let mut state = EditorState::new(Prompt::new("> "), None);
    let marker = b"\x1b[201~";
    for (index, byte) in marker.iter().enumerate() {
        let outcome = line.handle_bytes(&mut state, &[*byte], &mut ()).unwrap();
        assert!(matches!(outcome, EditorOutcome::Continue));
        if index + 1 < marker.len() {
            assert!(state.buffer.as_bytes().is_empty());
            assert!(!line.terminal().out.contains('\x07'));
        }
    }
    // Do not wait for another byte (or a key-sequence timeout) to complete
    // an otherwise-unbound marker.
    assert!(state.input.pending_key.is_empty());
    assert_eq!(state.buffer.as_bytes(), b"~");
    assert_eq!(line.terminal().out.matches('\x07').count(), 1);
    let outcome = line.handle_bytes(&mut state, b"abc\r", &mut ()).unwrap();
    assert!(matches!(outcome, EditorOutcome::Accepted(bytes) if bytes == b"~abc"));
}

#[test]
fn lone_bracketed_paste_end_respects_keymap_and_chunk_boundaries() {
    let keys = b"\x1b[201~abc\r";
    for (inputrc, expected, bells) in [
        ("", b"~abc".as_slice(), 1),
        ("set enable-bracketed-paste off", b"~abc", 1),
        ("\"\\e[201~\": \"X\"", b"Xabc", 0),
        ("\"\\e[201\": \"X\"", b"X~abc", 0),
        ("\"\\e[201~a\": \"X\"", b"Xbc", 0),
        ("\"\\e[201~z\": \"X\"", b"~abc", 1),
        ("\"~\": \"X\"", b"Xabc", 1),
    ] {
        // Batched, every two-chunk split, and entirely bytewise reads.
        let mut chunks = vec![vec![keys.as_slice()]];
        chunks.extend((1..keys.len()).map(|split| vec![&keys[..split], &keys[split..]]));
        chunks.push(keys.chunks(1).collect());
        for chunks in chunks {
            let terminal = MemoryTerminal::with_events(
                chunks
                    .iter()
                    .map(|chunk| TerminalEvent::Bytes(chunk.to_vec()))
                    .collect(),
            );
            let mut line = Editor::new(Config::default(), terminal, History::new());
            line.load_inputrc_str(inputrc).unwrap();
            let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
            assert_eq!(
                result,
                ReadlineResult::Line(expected.to_vec()),
                "inputrc={inputrc:?}, chunks={chunks:?}"
            );
            assert_eq!(
                line.terminal().out.matches('\x07').count(),
                bells,
                "inputrc={inputrc:?}, chunks={chunks:?}"
            );
        }
    }
}

#[test]
fn empty_bracketed_paste_sets_mark_at_point_for_frame_and_eof() {
    let mut editor = Editor::new(Config::default(), MemoryTerminal::default(), History::new());

    let mut framed = EditorState::new(Prompt::new("> "), Some(b"abc".to_vec()));
    let point = framed.buffer.point();
    framed.mark = Some(1);
    framed.paste.bracketed_paste = true;
    framed.paste.bracketed_paste_start = Some(point);
    editor
        .handle_bytes(&mut framed, b"\x1b[201~", &mut ())
        .unwrap();
    assert_eq!(framed.mark, Some(point));
    assert!(!framed.paste.bracketed_paste);

    let mut eof = EditorState::new(Prompt::new("> "), Some(b"abc".to_vec()));
    let point = eof.buffer.point();
    eof.mark = Some(1);
    eof.paste.bracketed_paste = true;
    eof.paste.bracketed_paste_start = Some(point);
    editor.finish_bracketed_paste(&mut eof);
    assert_eq!(eof.mark, Some(point));
    assert!(!eof.paste.bracketed_paste);
}

#[test]
fn bracketed_paste_normalizes_cr_to_lf_in_complete_and_eof_payloads() {
    let mut editor = Editor::new(Config::default(), MemoryTerminal::default(), History::new());

    let mut complete = EditorState::new(Prompt::new("> "), Some(b"prefix".to_vec()));
    editor
        .handle_bytes(&mut complete, b"\x1b[200~", &mut ())
        .unwrap();
    editor
        .handle_bytes(&mut complete, b"a\rb", &mut ())
        .unwrap();
    assert_eq!(complete.buffer.as_bytes(), b"prefix");
    assert_eq!(complete.paste.bracketed_paste_pending, b"a\nb");
    editor
        .handle_bytes(&mut complete, b"\x1b[201~", &mut ())
        .unwrap();
    assert_eq!(complete.buffer.as_bytes(), b"prefixa\nb");

    let mut eof = EditorState::new(Prompt::new("> "), None);
    editor
        .handle_bytes(&mut eof, b"\x1b[200~", &mut ())
        .unwrap();
    editor.handle_bytes(&mut eof, b"a\rb", &mut ()).unwrap();
    editor.finish_bracketed_paste(&mut eof);
    assert_eq!(eof.buffer.as_bytes(), b"a\nb");
}

#[test]
fn bracketed_paste_leaves_trailing_cr_outside_the_payload() {
    // The custom CR binding must run after the paste marker, not be normalized.
    let mut editor = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    editor
        .load_inputrc_str(
            "set enable-bracketed-paste on\nset bind-tty-special-chars off\n\"\\C-m\": self-insert",
        )
        .unwrap();
    let mut state = EditorState::new(Prompt::new("> "), None);
    editor
        .handle_bytes(&mut state, b"\x1b[200~", &mut ())
        .unwrap();

    assert!(matches!(
        editor
            .handle_bytes(&mut state, b"a\x1b[201~\r", &mut ())
            .unwrap(),
        EditorOutcome::Continue
    ));
    assert_eq!(state.buffer.as_bytes(), b"a\r");
}

#[test]
fn incomplete_bracketed_paste_flushes_before_propagating_read_error() {
    let terminal = MemoryTerminal::with_events_then_error(
        vec![
            TerminalEvent::Bytes(b"\x1b[200~".to_vec()),
            TerminalEvent::Bytes(b"partial\x1b[201".to_vec()),
        ],
        std::io::Error::from_raw_os_error(libc::EIO),
    );
    let mut editor = Editor::new(Config::default(), terminal, History::new());
    editor
        .load_inputrc_str("set enable-bracketed-paste on")
        .unwrap();

    let result = editor.read_line(Prompt::new("> "), &mut ());

    match result {
        Err(ReadlineError::Io(error)) => assert_eq!(error.raw_os_error(), Some(libc::EIO)),
        other => panic!("expected EIO, got {other:?}"),
    }
    assert!(
        editor.terminal().out.contains("partial") && editor.terminal().out.contains("201"),
        "pending paste and its partial end marker were not redisplayed before the I/O error: {:?}",
        editor.terminal().out
    );
}

#[test]
fn bracketed_paste_buffers_partial_data_until_the_split_terminator_completes() {
    let mut editor = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let mut state = EditorState::new(Prompt::new("> "), Some(b"prefix".to_vec()));
    editor
        .handle_bytes(&mut state, b"\x1b[200~", &mut ())
        .unwrap();
    let chunks = [
        b"abcdefgh".as_slice(),
        b"ijklmnop",
        b"\x1b",
        b"[",
        b"2",
        b"0",
        b"1",
    ];
    let mut pending = Vec::new();
    for chunk in chunks {
        pending.extend_from_slice(chunk);
        assert!(matches!(
            editor.handle_bytes(&mut state, chunk, &mut ()).unwrap(),
            EditorOutcome::Continue
        ));
        editor.render(&mut state).unwrap();
        assert!(state.paste.bracketed_paste);
        assert_eq!(state.buffer.as_bytes(), b"prefix");
        assert_eq!(state.paste.bracketed_paste_pending, pending);
        assert!(!editor.terminal().out.contains("abc"));
        assert!(!editor.terminal().out.contains("ijklmnop"));
    }
    editor.handle_bytes(&mut state, b"~", &mut ()).unwrap();
    editor.render(&mut state).unwrap();
    assert!(!state.paste.bracketed_paste);
    assert!(state.paste.bracketed_paste_pending.is_empty());
    assert_eq!(state.buffer.as_bytes(), b"prefixabcdefghijklmnop");
    assert!(editor.terminal().out.contains("abcdefghijklmnop"));
    assert!(matches!(
        editor.handle_bytes(&mut state, b"\r", &mut ()).unwrap(),
        EditorOutcome::Accepted(line) if line == b"prefixabcdefghijklmnop"
    ));
}

#[test]
fn bracketed_paste_normalizes_cr_while_preserving_truncated_end_prefix() {
    let mut editor = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let mut state = EditorState::new(Prompt::new("> "), None);
    for chunk in [b"\x1b[200~abc\x1b[201".as_slice(), b"\r", b"~"] {
        assert!(matches!(
            editor.handle_bytes(&mut state, chunk, &mut ()).unwrap(),
            EditorOutcome::Continue
        ));
        editor.render(&mut state).unwrap();
        assert!(state.paste.bracketed_paste);
        assert!(state.buffer.as_bytes().is_empty());
        assert!(!editor.terminal().out.contains("abc"));
    }
    assert_eq!(state.paste.bracketed_paste_pending, b"abc\x1b[201\n~");
    editor
        .handle_bytes(&mut state, b"\x1b[201~", &mut ())
        .unwrap();
    assert!(!state.paste.bracketed_paste);
    assert_eq!(state.buffer.as_bytes(), b"abc\x1b[201\n~");
}

fn read_line_bytes_with_inputrc(input: &[u8], inputrc: &str) -> Vec<u8> {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(input.to_vec())]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    if !inputrc.is_empty() {
        line.load_inputrc_str(inputrc).unwrap();
    }
    match line.read_line(Prompt::new("> "), &mut ()).unwrap() {
        ReadlineResult::Line(bytes) => bytes,
        other => panic!("expected accepted line, got {other:?}"),
    }
}

#[test]
fn character_motion_with_byte_oriented_on_steps_single_bytes() {
    // Byte-oriented motion steps UTF-8 constituent bytes: 中 is e4 b8 ad,
    // x is 78, Y is 59. Two backward bytes from the end rest at 2, so Y
    // lands inside the sequence; two forward bytes from the start rest at
    // the same offset. Both directions yield e4 b8 59 ad 78.
    let backward = "中x\x02\x02Y\r".as_bytes();
    assert_eq!(
        read_line_bytes_with_inputrc(backward, "set byte-oriented on"),
        vec![0xe4, 0xb8, 0x59, 0xad, 0x78]
    );
    let forward = "中x\x01\x06\x06Y\r".as_bytes();
    assert_eq!(
        read_line_bytes_with_inputrc(forward, "set byte-oriented on"),
        vec![0xe4, 0xb8, 0x59, 0xad, 0x78]
    );
}

#[test]
fn character_motion_in_utf8_without_byte_oriented_preserves_current_motion() {
    // UTF-8 `byte-oriented off` keeps the current character motion for this fixture: two backward steps
    // from the end reach the start (Y中x) and two forward steps from the
    // start reach the end (中xY). This is the retained behavior when the
    // locale is UTF-8; the C-locale bytewise case is covered by the PTY
    // oracle with `LC_ALL=C` in the child process.
    let probe = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    if !probe.variables.utf8_locale() {
        return;
    }
    let backward = "中x\x02\x02Y\r".as_bytes();
    assert_eq!(
        read_line_bytes_with_inputrc(backward, "set byte-oriented off"),
        vec![0x59, 0xe4, 0xb8, 0xad, 0x78]
    );
    let forward = "中x\x01\x06\x06Y\r".as_bytes();
    assert_eq!(
        read_line_bytes_with_inputrc(forward, "set byte-oriented off"),
        vec![0xe4, 0xb8, 0xad, 0x78, 0x59]
    );
}
