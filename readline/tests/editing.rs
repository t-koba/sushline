mod common;

use common::MemoryTerminal;
use readline::{
    Config, Edit, Editor, History, Hooks, InputrcPath, LineExpansionContext, Prompt,
    ReadlineResult, SpellCorrectionContext, TerminalEvent,
};

struct EditingHook;

impl Hooks for EditingHook {
    fn version(&mut self) -> Option<String> {
        Some("GNU bash, version test".to_string())
    }

    fn edit_and_execute(&mut self, line: &[u8]) -> Option<Vec<u8>> {
        let mut out = line.to_vec();
        out.extend_from_slice(b" edited");
        Some(out)
    }

    fn expand_line(&mut self, context: LineExpansionContext<'_>) -> Option<Edit> {
        let mut out = b"expanded ".to_vec();
        out.extend_from_slice(context.line);
        Some(Edit {
            line: Some(out),
            point: None,
            mark: None,
        })
    }

    fn tty_status(&mut self) -> Option<String> {
        Some("speed 9600 baud".to_string())
    }

    fn spell_correct(&mut self, context: SpellCorrectionContext<'_>) -> Option<Vec<u8>> {
        (context.word == b"teh").then(|| b"the".to_vec())
    }
}

#[test]
fn reads_basic_edited_line() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"b".to_vec()),
        TerminalEvent::Bytes(vec![0x7f]),
        TerminalEvent::Bytes(b"c".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("ac".as_bytes().to_vec()));
}

#[test]
fn supports_inputrc_macro_binding() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": \"hello\"").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("hello".as_bytes().to_vec()));
}

#[test]
fn inputrc_macro_body_replays_key_sequences_with_meta_variables() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": \"\\C-aX\"").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xabc".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0xe1]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set input-meta on\nset convert-meta off\n\"\\M-a\": \"eight\"")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("eight".as_bytes().to_vec()));
}

#[test]
fn negative_arguments_match_readline_line_kill_case_and_transpose_rules() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcde".to_vec()),
        TerminalEvent::Bytes(b"\x02\x02".to_vec()),
        TerminalEvent::Bytes(b"\x1b-\x0b".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("de".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcde".to_vec()),
        TerminalEvent::Bytes(b"\x02\x02".to_vec()),
        TerminalEvent::Bytes(b"\x1b-".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": backward-kill-line")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abc".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"foo bar".to_vec()),
        TerminalEvent::Bytes(b"\x1b-\x1bu".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("foo BAR".as_bytes().to_vec()));

    // A sign-only negative argument at EOL transposes the final pair.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(b"\x1b-\x14".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("ba".as_bytes().to_vec()));
}

#[test]
fn overwrite_mode_argument_and_backspace_replace_with_space() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\x7f".to_vec()),
        TerminalEvent::Bytes(b"\x1b0".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"c".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": overwrite-mode").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("ac ".as_bytes().to_vec()));
}

#[test]
fn numeric_backward_delete_char_kills_for_yank() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcd".to_vec()),
        TerminalEvent::Bytes(b"\x1b2\x7f".to_vec()),
        TerminalEvent::Bytes(b"\x19".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcd".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcd".to_vec()),
        TerminalEvent::Bytes(b"\x02\x02".to_vec()),
        TerminalEvent::Bytes(b"\x1b-\x7f".to_vec()),
        TerminalEvent::Bytes(b"\x19".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcd".as_bytes().to_vec()));
}

#[test]
fn forward_backward_delete_char_honors_numeric_argument() {
    let inputrc = "\"\\C-o\": forward-backward-delete-char";
    for (name, keys, expected) in [
        (
            "interior_positive",
            b"abcdef\x01\x06\x06\x1b3\x0f\r".as_slice(),
            "abf",
        ),
        (
            "interior_zero",
            b"abcdef\x01\x06\x06\x1b0\x0f\r".as_slice(),
            "abcdef",
        ),
        (
            "interior_negative",
            b"abcdef\x01\x06\x06\x1b-1\x0f\r".as_slice(),
            "acdef",
        ),
        (
            "interior_overrun",
            b"abcdef\x01\x06\x06\x1b5\x0f\r".as_slice(),
            "ab",
        ),
        (
            "interior_uncounted",
            b"abcdef\x01\x06\x06\x0f\r".as_slice(),
            "abdef",
        ),
        ("eol_positive", b"abcdef\x1b3\x0f\r".as_slice(), "abc"),
        ("eol_zero", b"abcdef\x1b0\x0f\r".as_slice(), "abcdef"),
        ("eol_negative", b"abcdef\x1b-1\x0f\r".as_slice(), "abcdef"),
        ("eol_uncounted", b"abcdef\x0f\r".as_slice(), "abcde"),
    ] {
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys.to_vec())]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(inputrc).unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.as_bytes().to_vec()),
            "{name}"
        );
    }
}

#[test]
fn forward_backward_delete_char_counted_kills_for_yank() {
    let inputrc = "\"\\C-o\": forward-backward-delete-char";
    for (name, keys, expected) in [
        (
            "interior_positive",
            b"abcdef\x01\x06\x06\x1b3\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "interior_negative",
            b"abcdef\x01\x06\x06\x1b-1\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "eol_positive",
            b"abcdef\x1b3\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "eol_negative",
            b"abcdef\x1b-1\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "zero",
            b"abcdef\x01\x06\x06\x1b0\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "interior_uncounted",
            b"abcdef\x01\x06\x06\x0f\x19\r".as_slice(),
            "abdef",
        ),
        ("eol_uncounted", b"abcdef\x0f\x19\r".as_slice(), "abcde"),
    ] {
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys.to_vec())]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(inputrc).unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.as_bytes().to_vec()),
            "{name}"
        );
    }
}

#[test]
fn delete_char_or_list_honors_numeric_argument() {
    let inputrc = "\"\\C-o\": delete-char-or-list";
    for (name, keys, expected) in [
        (
            "positive",
            b"abcdef\x01\x06\x06\x1b3\x0f\r".as_slice(),
            "abf",
        ),
        (
            "zero",
            b"abcdef\x01\x06\x06\x1b0\x0f\r".as_slice(),
            "abcdef",
        ),
        (
            "negative",
            b"abcdef\x01\x06\x06\x1b-1\x0f\r".as_slice(),
            "acdef",
        ),
        ("uncounted", b"abcdef\x01\x06\x06\x0f\r".as_slice(), "abdef"),
    ] {
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys.to_vec())]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(inputrc).unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.as_bytes().to_vec()),
            "{name}"
        );
    }
}

#[test]
fn delete_char_or_list_counted_deletions_kill_for_yank() {
    let inputrc = "\"\\C-o\": delete-char-or-list";
    for (name, keys, expected) in [
        (
            "positive",
            b"abcdef\x01\x06\x06\x1b3\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "negative",
            b"abcdef\x01\x06\x06\x1b-1\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "zero",
            b"abcdef\x01\x06\x06\x1b0\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "uncounted",
            b"abcdef\x01\x06\x06\x0f\x19\r".as_slice(),
            "abdef",
        ),
    ] {
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys.to_vec())]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(inputrc).unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.as_bytes().to_vec()),
            "{name}"
        );
    }
}

#[test]
fn numeric_delete_char_kills_for_yank() {
    // Explicit positive count deletes forward and saves the text for yank.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcd".to_vec()),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\x1b2\x04".to_vec()),
        TerminalEvent::Bytes(b"\x19".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcd".as_bytes().to_vec()));

    // Explicit negative count deletes backward and saves the text for yank.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcd".to_vec()),
        TerminalEvent::Bytes(b"\x02\x02".to_vec()),
        TerminalEvent::Bytes(b"\x1b-\x04".to_vec()),
        TerminalEvent::Bytes(b"\x19".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcd".as_bytes().to_vec()));

    // Consecutive counted kills concatenate in both directions before yank.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcd".to_vec()),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\x1b1\x04\x1b1\x04".to_vec()),
        TerminalEvent::Bytes(b"\x19".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcd".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcd".to_vec()),
        TerminalEvent::Bytes(b"\x1b-\x04\x1b-\x04".to_vec()),
        TerminalEvent::Bytes(b"\x19".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcd".as_bytes().to_vec()));

    // Uncounted delete-char stays a plain deletion: yank finds no kill.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcd".to_vec()),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(vec![0x04]),
        TerminalEvent::Bytes(b"\x19".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("bcd".as_bytes().to_vec()));
}

#[test]
fn byte_commands_move_over_utf8_codepoints_inside_grapheme_clusters() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes("e\u{301}x".as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x02]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"Y".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": backward-byte").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(vec![101, 204, 89, 129, 120]));
}

#[test]
fn reverse_search_repeats_and_aborts_with_original_line() {
    let mut history = History::new();
    history.push("alpha one");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("alpha one".as_bytes().to_vec())
    );
    assert!(
        line.terminal()
            .out
            .contains("(reverse-i-search)`alpha': alpha one")
    );

    let mut history = History::new();
    history.push("alpha one");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"draft".to_vec()),
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\x07".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("draft".as_bytes().to_vec()));
}

#[test]
fn reverse_search_ctrl_c_byte_interrupts_instead_of_staying_in_search() {
    let mut history = History::new();
    history.push("alpha one");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"draft".to_vec()),
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\x03".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal().out.contains("^C\r\n"));
    assert!(line.terminal().cleared_screen > 0);
}

#[test]
fn quoted_insert_ctrl_c_byte_remains_literal() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x16".to_vec()),
        TerminalEvent::Bytes(b"\x03".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(vec![0x03]));
}

#[test]
fn history_preserve_point_keeps_cursor_column_on_history_navigation() {
    let mut history = History::new();
    history.push("abcdef");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"xx".to_vec()),
        TerminalEvent::Bytes(vec![0x10]),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("set history-preserve-point on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abZcdef".as_bytes().to_vec()));
}

#[test]
fn bind_x_hook_can_update_line_point_and_mark() {
    struct BoundCommandHook;

    impl Hooks for BoundCommandHook {
        fn on_command(&mut self, context: readline::CommandContext<'_>) -> Option<Edit> {
            assert_eq!(context.command, "rewrite");
            assert_eq!(context.line, b"abc");
            assert_eq!(context.point, 3);
            assert_eq!(context.mark, None);
            Some(Edit {
                line: Some(b"aXYZc".to_vec()),
                point: Some(4),
                mark: Some(Some(1)),
            })
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = BoundCommandHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.bind_api()
        .bind_application_command("\"\\C-o\"", "rewrite")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("aXYZc".as_bytes().to_vec()));
}

#[test]
fn shell_word_commands_use_hook_token_spans() {
    struct ColonTokenHook;

    impl Hooks for ColonTokenHook {
        fn shell_word_spans(&mut self, line: &[u8]) -> Option<Vec<(usize, usize)>> {
            let mut spans = Vec::new();
            let mut start = None;
            for (idx, byte) in line.iter().copied().enumerate() {
                if matches!(byte, b':' | b' ') {
                    if let Some(start) = start.take() {
                        spans.push((start, idx));
                    }
                } else {
                    start.get_or_insert(idx);
                }
            }
            if let Some(start) = start {
                spans.push((start, line.len()));
            }
            Some(spans)
        }
    }

    let inputrc = r#"
"\C-o": shell-forward-word
"\C-p": shell-backward-kill-word
"\C-t": shell-transpose-words
"#;

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"aa:bb".to_vec()),
        TerminalEvent::Bytes(vec![0x01, 0x0f]),
        TerminalEvent::Bytes(b"X\r".to_vec()),
    ]);
    let mut hooks = ColonTokenHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(inputrc).unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"aaX:bb".to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"aa:bb".to_vec()),
        TerminalEvent::Bytes(vec![0x10]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ColonTokenHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(inputrc).unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"aa:".to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"aa:bb".to_vec()),
        TerminalEvent::Bytes(vec![0x14]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ColonTokenHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(inputrc).unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"bb:aa".to_vec()));
}

#[test]
fn negative_numeric_argument_reverses_motion_direction() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(vec![0x1b, b'-']),
        TerminalEvent::Bytes(vec![0x06]),
        TerminalEvent::Bytes(b"X\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xab".as_bytes().to_vec()));
}

#[test]
fn editing_word_breaks_hook_controls_word_commands() {
    struct WordBreakHook;

    impl Hooks for WordBreakHook {
        fn editing_word_breaks(&mut self) -> Option<Vec<u8>> {
            Some(b" \t\n".to_vec())
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"one two-three".to_vec()),
        TerminalEvent::Bytes(vec![0x17]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = WordBreakHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("one ".as_bytes().to_vec()));
}

#[test]
fn editing_word_breaks_hook_preserves_non_utf8_break_byte() {
    struct NonUtf8BreakHook;

    impl Hooks for NonUtf8BreakHook {
        fn editing_word_breaks(&mut self) -> Option<Vec<u8>> {
            Some(vec![0xff])
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![b'a', 0xff, b'b']),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = NonUtf8BreakHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": backward-kill-word")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(vec![b'a', 0xff]));
}

#[test]
fn editing_word_breaks_hook_preserves_multibyte_break() {
    struct MultibyteBreakHook;

    impl Hooks for MultibyteBreakHook {
        fn editing_word_breaks(&mut self) -> Option<Vec<u8>> {
            Some("é".as_bytes().to_vec())
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![b'a', 0xc3, 0xa9, b'b']),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = MultibyteBreakHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": backward-kill-word")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(vec![b'a', 0xc3, 0xa9]));
}

#[test]
fn hook_backed_commands_use_application_supplied_behavior() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"echo".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = EditingHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": edit-and-execute-command")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("echo edited".as_bytes().to_vec())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"~/src".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = EditingHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": shell-expand-line")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("expanded ~/src".as_bytes().to_vec())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = EditingHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": display-shell-version")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert!(line.terminal().out.contains("GNU bash, version test"));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = EditingHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": tty-status").unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert!(line.terminal().out.contains("speed 9600 baud"));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"teh".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = EditingHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": spell-correct-word")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("the".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"echo".to_vec()),
        TerminalEvent::Bytes(vec![0x1b, 0x0f]),
    ]);
    let mut hooks = EditingHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(
        "set editing-mode vi\nset keymap vi-command\n\"\\C-o\": vi-edit-and-execute-command",
    )
    .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("echo edited".as_bytes().to_vec())
    );
}

#[test]
fn contextual_shell_expand_hook_can_set_line_and_point() {
    struct ContextShellExpandHook {
        seen: bool,
    }

    impl Hooks for ContextShellExpandHook {
        fn expand_line(&mut self, context: LineExpansionContext<'_>) -> Option<Edit> {
            assert_eq!(context.line, b"abc");
            assert_eq!(context.point, 1);
            self.seen = true;
            Some(Edit {
                line: Some(b"abcd".to_vec()),
                point: Some(2),
                mark: None,
            })
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x02, 0x02, 0x0f]),
        TerminalEvent::Bytes(b"X\r".to_vec()),
    ]);
    let mut hooks = ContextShellExpandHook { seen: false };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": shell-expand-line")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"abXcd".to_vec()));
    assert!(hooks.seen);
}

#[test]
fn contextual_spell_correct_hook_receives_line_and_word_range() {
    struct ContextSpellHook {
        seen: bool,
    }

    impl Hooks for ContextSpellHook {
        fn spell_correct(&mut self, context: SpellCorrectionContext<'_>) -> Option<Vec<u8>> {
            assert_eq!(context.line, b"say teh");
            assert_eq!(context.point, 7);
            assert_eq!(context.word_start, 4);
            assert_eq!(context.word_end, 7);
            assert_eq!(context.word, b"teh");
            self.seen = true;
            Some(b"the".to_vec())
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"say teh".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ContextSpellHook { seen: false };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": spell-correct-word")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"say the".to_vec()));
    assert!(hooks.seen);
}

#[test]
fn edit_and_execute_without_hook_does_not_execute_application_policy() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"original".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": edit-and-execute-command")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("original".as_bytes().to_vec()));
    assert!(line.terminal().out.contains('\x07'));
}

#[test]
fn execute_named_command_reads_command_name_and_dispatches_it() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"beginning-of-line".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": execute-named-command")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xabc".as_bytes().to_vec()));
}

#[test]
fn prefix_meta_metaizes_next_key() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(vec![0x18]),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(
        "set convert-meta on\n\"\\C-x\": prefix-meta\n\"\\M-a\": beginning-of-line",
    )
    .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xab".as_bytes().to_vec()));
}

#[test]
fn numeric_set_mark_uses_absolute_buffer_position() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\x1b3".to_vec()),
        TerminalEvent::Bytes(vec![0x18]),
        TerminalEvent::Bytes(vec![0x17]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-x\": set-mark\n\"\\C-w\": kill-region")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("def".as_bytes().to_vec()));
}

#[test]
fn numeric_insert_comment_toggles_existing_comment() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"#abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b1".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": insert-comment").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abc".as_bytes().to_vec()));
}

#[test]
fn operate_and_get_next_prefills_next_readline() {
    let mut history = History::new();
    history.push("one");
    history.push("two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x10".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": operate-and-get-next")
        .unwrap();
    let first = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(first, ReadlineResult::Line("two".as_bytes().to_vec()));

    *line.terminal_mut() = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"\r".to_vec())]);
    let second = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(second, ReadlineResult::Line(Vec::new()));
}

#[test]
fn last_keyboard_macro_persists_across_reads_for_replay() {
    // GNU retains the last recorded macro across prompts: recording `ab`
    // (which stores `a` due to consecutive self-insert grouping) then
    // replaying with C-x e yields `a`.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x18, b'(']),
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(vec![0x18, b')']),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let first = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(first, ReadlineResult::Line(b"ab".to_vec()));

    *line.terminal_mut() = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x18, b'e']),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let second = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(second, ReadlineResult::Line(b"a".to_vec()));
}

#[test]
fn killed_text_persists_across_reads_for_yank() {
    // GNU keeps killed text on the kill ring across prompts: killing `abc`
    // with C-u, accepting the empty line, then yanking with C-y replays it.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x15]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let first = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(first, ReadlineResult::Line(Vec::new()));

    *line.terminal_mut() = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x19]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let second = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(second, ReadlineResult::Line(b"abc".to_vec()));
}

#[test]
fn re_read_init_file_loads_configured_inputrc() {
    let dir = tempfile::tempdir().unwrap();
    let inputrc = dir.path().join("inputrc");
    std::fs::write(&inputrc, "\"\\C-p\": beginning-of-line\n").unwrap();
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\x10".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let config = Config {
        inputrc_path: InputrcPath::Path(inputrc),
        ..Default::default()
    };
    let mut line = Editor::new(config, terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": re-read-init-file")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xabc".as_bytes().to_vec()));
}

#[test]
fn re_read_init_file_reloads_last_explicit_inputrc_file() {
    let dir = tempfile::tempdir().unwrap();
    let inputrc = dir.path().join("inputrc");
    std::fs::write(&inputrc, "\"\\C-o\": re-read-init-file\n").unwrap();
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\x10".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_file(&inputrc).unwrap();
    std::fs::write(
        &inputrc,
        "\"\\C-o\": re-read-init-file\n\"\\C-p\": beginning-of-line\n",
    )
    .unwrap();

    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xabc".as_bytes().to_vec()));
}

#[test]
fn configured_inputrc_is_loaded_at_construction_and_persists_across_reads() {
    let dir = tempfile::tempdir().unwrap();
    let inputrc = dir.path().join("inputrc");
    std::fs::write(&inputrc, "\"\\C-o\": \"X\"").unwrap();
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let config = Config {
        inputrc_path: InputrcPath::Path(inputrc),
        ..Default::default()
    };
    let mut line = Editor::new(config, terminal, History::new());
    let first = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(first, ReadlineResult::Line(b"X".to_vec()));

    *line.terminal_mut() = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let second = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(second, ReadlineResult::Line(b"X".to_vec()));
}

#[test]
fn disable_completion_self_inserts_tab_key() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set disable-completion on").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("\t".as_bytes().to_vec()));
}

#[test]
fn search_ignore_case_affects_incremental_search() {
    let mut history = History::new();
    history.push("Alpha One");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("set search-ignore-case on").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("Alpha One".as_bytes().to_vec())
    );
}

#[test]
fn yank_pop_without_prior_yank_bells_and_preserves_line() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(vec![0x1d]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-]\": yank-pop").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"abc".to_vec()));
    assert!(
        line.terminal().out.contains('\x07'),
        "stray yank-pop must bell"
    );
}

#[test]
fn yank_pop_after_valid_yank_replaces_text_without_bell() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(
        b"one\x15two\x15X\x19\x1d\r".to_vec(),
    )]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-]\": yank-pop").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"Xtwoone".to_vec()));
    assert!(
        !line.terminal().out.contains('\x07'),
        "valid yank-pop must not bell"
    );
}

#[test]
fn yank_with_empty_ring_bells_and_preserves_line() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x19]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(Vec::new()));
    assert!(line.terminal().out.contains('\x07'), "empty yank must bell");
}

#[test]
fn transpose_chars_uses_counted_target_and_endpoint_rules() {
    // MemoryTerminal cases complement the paired PTY fixtures by exercising
    // the editor's numeric target and cursor movement locally.
    let cases: &[(&str, &[u8], &[u8], &str)] = &[
        (
            "one two three four",
            b"\x01\x06\x06\x06",
            b"\x1b2\x0fX\r",
            "on teXwo three four",
        ),
        (
            "one two three four",
            b"\x01\x06\x06\x06",
            b"\x1b-1\x0fX\r",
            "oneX two three four",
        ),
        (
            "one two three four",
            b"",
            b"\x1b-1\x0fX\r",
            "one two three foruX",
        ),
        (
            "one two three four",
            b"",
            b"\x1b0\x0fX\r",
            "one two three fourX",
        ),
        (
            "abcdefghij",
            b"\x01\x06\x06\x06\x06\x06\x06\x06\x06",
            b"\x1b4\x0fX\r",
            "abcdefgijhX",
        ),
    ];
    for (initial, point_keys, command_keys, expected) in cases {
        let mut keys = initial.as_bytes().to_vec();
        keys.extend_from_slice(point_keys);
        keys.extend_from_slice(command_keys);
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys)]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("\"\\C-o\": transpose-chars").unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(result, ReadlineResult::Line(expected.as_bytes().to_vec()));
    }
}

#[test]
fn delete_horizontal_space_consumes_numeric_argument() {
    // M-2 M-\\ ignores its prefix and must not leak it: the following
    // backward-char moves one step, so Q lands before f.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\x1b2\x1b\\".to_vec()),
        TerminalEvent::Bytes(vec![0x02]),
        TerminalEvent::Bytes(b"Q\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"abcdeQf".to_vec()));
}

#[test]
fn yank_pop_consumes_numeric_argument() {
    // M-2 yank-pop must not leak the prefix into the next command:
    // the following backward-char moves one step, so Q lands before f.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\x1b2\x1d".to_vec()),
        TerminalEvent::Bytes(vec![0x02]),
        TerminalEvent::Bytes(b"Q\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-]\": yank-pop").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"abcdeQf".to_vec()));
}

#[test]
fn yank_with_empty_ring_consumes_numeric_argument() {
    // M-2 C-y on an empty ring bells and must not leak the prefix:
    // the following backward-char moves one step, so Q lands before f.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\x1b2\x19".to_vec()),
        TerminalEvent::Bytes(vec![0x02]),
        TerminalEvent::Bytes(b"Q\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"abcdeQf".to_vec()));
    assert!(line.terminal().out.contains('\x07'), "empty yank must bell");
}

#[cfg(unix)]
fn current_account_name_and_home() -> Option<(String, Vec<u8>)> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let uid = unsafe { libc::geteuid() };
    let passwd = std::fs::read("/etc/passwd").ok()?;
    passwd.split(|byte| *byte == b'\n').find_map(|line| {
        let fields = line.split(|byte| *byte == b':').collect::<Vec<_>>();
        if fields.len() < 6
            || std::str::from_utf8(fields[2])
                .ok()?
                .parse::<libc::uid_t>()
                .ok()?
                != uid
        {
            return None;
        }
        let name = std::str::from_utf8(fields[0]).ok()?.to_string();
        let home = std::path::PathBuf::from(std::ffi::OsString::from_vec(fields[5].to_vec()));
        home.is_dir()
            .then(|| (name, home.as_os_str().as_bytes().to_vec()))
    })
}

#[cfg(unix)]
#[test]
fn tilde_expand_preserves_named_user_trailing_slash() {
    let Some((username, home)) = current_account_name_and_home() else {
        return;
    };
    let mut expected = home;
    expected.extend_from_slice(b"/X");
    let mut keys = format!("~{username}/").into_bytes();
    keys.extend_from_slice(b"\x0fX\r");

    for command in ["tilde-expand", "vi-tilde-expand"] {
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys.clone())]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(&format!("\"\\C-o\": {command}"))
            .unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert!(
            matches!(result, ReadlineResult::Line(actual) if actual == expected),
            "{command} must preserve the explicitly typed slash"
        );
    }
}

#[test]
fn ignored_prefix_commands_consume_numeric_argument() {
    // One probe per ignored-argument class (movement/kill/history/editing):
    // each command is a no-op here and must consume M-2 so the following
    // backward-char moves one step and Q lands before f.
    let cases: &[(&str, Vec<u8>, Option<&str>)] = &[
        // C-x C-x exchange-point-and-mark with no mark (movement).
        ("exchange", b"\x1b2\x18\x18".to_vec(), None),
        // M-& tilde-expand with no tilde word (editing).
        ("tilde", b"\x1b2\x1b&".to_vec(), None),
        // M-< history-beginning with empty history (history nav).
        ("history", b"\x1b2\x1b<".to_vec(), None),
        // M-w copy-region-as-kill with no region (kill).
        (
            "copy-region",
            b"\x1b2\x1bw".to_vec(),
            Some("\"\\ew\": copy-region-as-kill"),
        ),
    ];
    for (name, prefix, inputrc) in cases {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"abcdef".to_vec()),
            TerminalEvent::Bytes(prefix.clone()),
            TerminalEvent::Bytes(vec![0x02]),
            TerminalEvent::Bytes(b"Q\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        if let Some(bindings) = inputrc {
            line.load_inputrc_str(bindings).unwrap();
        }
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"abcdeQf".to_vec()),
            "case {name} leaked its numeric argument"
        );
    }
}

#[test]
fn undo_groups_single_byte_inserts_up_to_twenty_chars() {
    // GNU rl_insert_text concatenates only single-byte inserts up to 20 chars;
    // verified against the patch 0 baseline (Bash 5.3 PTY oracle): 21/25 a's
    // leave 20 after one undo, 20a+é leaves 20 because the complete multibyte
    // char opens its own entry (past the 20-byte cap either way).
    for (typed, after_one_undo) in [
        ("a".repeat(20), "".to_string()),
        ("a".repeat(21), "a".repeat(20)),
        ("a".repeat(25), "a".repeat(20)),
    ] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(typed.as_bytes().to_vec()),
            TerminalEvent::Bytes(vec![0x1f]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(after_one_undo.as_bytes().to_vec()),
            "typed {typed:?}"
        );
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".repeat(25).to_vec()),
        TerminalEvent::Bytes(vec![0x1f]),
        TerminalEvent::Bytes(vec![0x1f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"".to_vec()));

    let typed = format!("{}é", "a".repeat(20));
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(typed.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x1f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("a".repeat(20).as_bytes().to_vec())
    );
}

#[test]
fn undo_short_multibyte_run_splits_like_gnu() {
    // GNU `rl_insert_text` groups only single-byte inserts: `a` stays in one
    // entry while the complete `é` char opens its own, so one undo leaves
    // `a`. Verified against the patch 0 baseline (Bash 5.3 PTY oracle).
    // Incomplete UTF-8 leads buffer across events like a keymap prefix, so
    // torn reads assemble before grouping and agree with batched reads.
    for events in [
        vec![
            TerminalEvent::Bytes("aé".as_bytes().to_vec()),
            TerminalEvent::Bytes(vec![0x1f]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
        vec![
            TerminalEvent::Bytes(b"a".to_vec()),
            TerminalEvent::Bytes("é".as_bytes().to_vec()),
            TerminalEvent::Bytes(vec![0x1f]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
        vec![
            TerminalEvent::Bytes(b"a".to_vec()),
            TerminalEvent::Bytes(vec![0xc3]),
            TerminalEvent::Bytes(vec![0xa9]),
            TerminalEvent::Bytes(vec![0x1f]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
    ] {
        let terminal = MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set input-meta on").unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(result, ReadlineResult::Line(b"a".to_vec()));
    }
}

#[test]
fn undo_numeric_arg_multi_insert_opens_own_entry() {
    // `ab` groups, `M-5 a` (count != 1) commits it first, so one undo leaves
    // `ab` and a second undo leaves the empty line.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(b"\x1b5a".to_vec()),
        TerminalEvent::Bytes(vec![0x1f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"ab".to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(b"\x1b5a".to_vec()),
        TerminalEvent::Bytes(vec![0x1f]),
        TerminalEvent::Bytes(vec![0x1f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"".to_vec()));
}

#[test]
fn undo_multi_byte_literal_opens_own_entry() {
    // `ab` groups, quoted-insert `XY` in one chunk (bytes.len() > 1) commits
    // it first, so one undo leaves `ab`.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(vec![0x16]),
        TerminalEvent::Bytes(b"XY".to_vec()),
        TerminalEvent::Bytes(vec![0x1f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"ab".to_vec()));
}

#[test]
fn tab_insert_honors_numeric_argument() {
    for (keys, expected) in [
        (b"a\x1b3\x0fb\r".as_slice(), "a\t\t\tb"),
        (b"a\x1b0\x0fb\r".as_slice(), "ab"),
        (b"a\x1b-\x0fb\r".as_slice(), "ab"),
        (b"a\x0fb\r".as_slice(), "a\tb"),
    ] {
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys.to_vec())]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("\"\\C-o\": tab-insert").unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(result, ReadlineResult::Line(expected.as_bytes().to_vec()));
    }
}

#[test]
fn undo_tab_insert_extends_single_byte_entry() {
    // `tab-insert` records a single-byte insert, so `ab` + tab stays in one
    // entry and a single undo clears all three columns.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x1f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": tab-insert").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"".to_vec()));
}

#[test]
fn clearing_prefix_commands_consume_numeric_argument() {
    // revert-line and undo empty the line; a leaked M-2 would double the
    // following self-insert (QQ instead of Q).
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\x1b2\x1br".to_vec()),
        TerminalEvent::Bytes(b"Q\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"Q".to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\x1b2\x1f".to_vec()),
        TerminalEvent::Bytes(b"Q\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"Q".to_vec()));
}

#[test]
fn numeric_argument_survives_prefix_commands() {
    // M-2 C-q x repeats the quoted literal.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b2\x11".to_vec()),
        TerminalEvent::Bytes(b"x".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"xx".to_vec()));

    // M-2 C-f still moves two steps.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\x1b2\x06".to_vec()),
        TerminalEvent::Bytes(b"Q\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"abQcdef".to_vec()));
}

#[test]
fn search_exits_consume_numeric_argument() {
    // M-2 before incremental/non-incremental search is consumed on entry,
    // so no exit branch (accept or abort) may leak it: the following
    // backward-char moves one step and Q lands before f.
    // Reverse search: ESC accepts the line, C-g aborts to the original line.
    for exit in [vec![0x1b], vec![0x07]] {
        let name = if exit == vec![0x1b] {
            "reverse-accept"
        } else {
            "reverse-abort"
        };
        let mut history = History::new();
        history.push("alpha one");
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"abcdef".to_vec()),
            TerminalEvent::Bytes(b"\x1b2\x12".to_vec()),
            TerminalEvent::Bytes(exit),
            TerminalEvent::Bytes(vec![0x02]),
            TerminalEvent::Bytes(b"Q\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, history);
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"abcdeQf".to_vec()),
            "case {name} leaked its numeric argument"
        );
    }
    // Non-incremental search bound to C-o: ESC/C-g abort, Enter accepts.
    for exit in [vec![0x1b], vec![0x07], b"\r".to_vec()] {
        let name = match exit.as_slice() {
            [0x1b] => "nonincremental-abort-esc",
            [0x07] => "nonincremental-abort",
            _ => "nonincremental-accept",
        };
        let mut history = History::new();
        history.push("alpha one");
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"abcdef".to_vec()),
            TerminalEvent::Bytes(b"\x1b2".to_vec()),
            TerminalEvent::Bytes(vec![0x0f]),
            TerminalEvent::Bytes(exit),
            TerminalEvent::Bytes(vec![0x02]),
            TerminalEvent::Bytes(b"Q\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, history);
        line.load_inputrc_str("\"\\C-o\": non-incremental-reverse-search-history")
            .unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"abcdeQf".to_vec()),
            "case {name} leaked its numeric argument"
        );
    }
}

#[test]
fn incremental_search_quoted_insert_quotes_control() {
    let mut history = History::new();
    history.push("foo\x01bar");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(vec![0x16]),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"foo\x01bar".to_vec()));
}

#[test]
fn incremental_search_batched_quote_matches_split_reads() {
    for events in [
        vec![
            TerminalEvent::Bytes(b"\x12".to_vec()),
            TerminalEvent::Bytes(b"\x16\x01\r".to_vec()),
        ],
        vec![
            TerminalEvent::Bytes(b"\x12".to_vec()),
            TerminalEvent::Bytes(vec![0x16]),
            TerminalEvent::Bytes(vec![0x01]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
    ] {
        let mut history = History::new();
        history.push("foo\x01bar");
        let terminal = MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, history);
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(result, ReadlineResult::Line(b"foo\x01bar".to_vec()));
    }
}

#[test]
fn incremental_search_remapped_quoted_insert_quotes() {
    let mut history = History::new();
    history.push("foo\x01bar");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": quoted-insert").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"foo\x01bar".to_vec()));
}

#[test]
fn non_incremental_search_ctrl_v_quotes_control() {
    let mut history = History::new();
    history.push("foo\x01bar");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1bp".to_vec()),
        TerminalEvent::Bytes(vec![0x16]),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\ep\": non-incremental-reverse-search-history")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"foo\x01bar".to_vec()));
}

#[test]
fn non_incremental_search_remapped_quote_does_not_quote() {
    let mut history = History::new();
    history.push("foo\x01bar");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1bp".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str(
        "\"\\ep\": non-incremental-reverse-search-history\n\"\\C-o\": quoted-insert",
    )
    .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"".to_vec()));
}

#[test]
fn isearch_terminators_terminate_without_execute_and_cr_accepts() {
    // Default C-J (LF) terminates the search without executing; RET (CR)
    // accepts. Verified against the patch 0 baseline (Bash 5.3 PTY oracle):
    // the match becomes the line, the terminator inserts nothing, and a
    // trailing edit applies before RET accepts.
    for events in [
        vec![
            TerminalEvent::Bytes(b"\x12".to_vec()),
            TerminalEvent::Bytes(b"alpha".to_vec()),
            TerminalEvent::Bytes(b"\n".to_vec()),
            TerminalEvent::Bytes(b"!".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
        vec![
            TerminalEvent::Bytes(b"\x12".to_vec()),
            TerminalEvent::Bytes(b"alpha\n!".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
    ] {
        let mut history = History::new();
        history.push("alpha one");
        history.push("alpha two");
        let terminal = MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, history);
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(result, ReadlineResult::Line(b"!alpha two".to_vec()));
    }

    // Custom isearch-terminators terminate without inserting themselves;
    // batched and split reads agree.
    for events in [
        vec![
            TerminalEvent::Bytes(b"\x12".to_vec()),
            TerminalEvent::Bytes(b"alp".to_vec()),
            TerminalEvent::Bytes(b"z".to_vec()),
            TerminalEvent::Bytes(b"!".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
        vec![
            TerminalEvent::Bytes(b"\x12".to_vec()),
            TerminalEvent::Bytes(b"alpz!".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
    ] {
        let mut history = History::new();
        history.push("alpha one");
        history.push("alpha two");
        let terminal = MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, history);
        line.load_inputrc_str("set isearch-terminators z").unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(result, ReadlineResult::Line(b"!alpha two".to_vec()));
    }

    // A mid-line match leaves point at the match start (`two` at 6).
    let mut history = History::new();
    history.push("alpha one");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"two".to_vec()),
        TerminalEvent::Bytes(b"\n".to_vec()),
        TerminalEvent::Bytes(b"!".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"alpha !two".to_vec()));

    // RET still accepts the match directly.
    let mut history = History::new();
    history.push("alpha one");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"two".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"alpha two".to_vec()));

    // An empty query restores the original line and point (`draft` typed
    // first leaves point at the end, so `!` appends).
    let mut history = History::new();
    history.push("alpha one");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"draft".to_vec()),
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"\n".to_vec()),
        TerminalEvent::Bytes(b"!".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"draft!".to_vec()));

    // A non-empty query with no match leaves point at 0, so `!` prepends.
    let mut history = History::new();
    history.push("alpha one");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"draft".to_vec()),
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"zzz".to_vec()),
        TerminalEvent::Bytes(b"\n".to_vec()),
        TerminalEvent::Bytes(b"!".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"!draft".to_vec()));
}

#[test]
fn isearch_terminator_point_respects_search_ignore_case() {
    // Case-insensitive terminator point (patch 0 Bash 5.3 PTY oracle):
    // uppercase `TWO` matches `two` at 6, so `!` inserts mid-line.
    let mut history = History::new();
    history.push("alpha one");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"TWO".to_vec()),
        TerminalEvent::Bytes(b"\n".to_vec()),
        TerminalEvent::Bytes(b"!".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("set search-ignore-case on").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"alpha !two".to_vec()));
}

#[test]
fn incremental_search_starts_from_history_cursor() {
    // Backward from the middle searches before the cursor (patch 0 Bash 5.3
    // PTY oracle): Up Up lands on `beta`, so `C-r alpha RET` finds
    // `alpha one`, not `alpha two` from the end.
    let mut history = History::new();
    history.push("alpha one");
    history.push("beta");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x10\x10".to_vec()),
        TerminalEvent::Bytes(b"\x12".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"alpha one".to_vec()));

    // Forward from the end has nothing ahead (same oracle): `C-o alpha RET`
    // keeps the empty line.
    let mut history = History::new();
    history.push("alpha one");
    history.push("beta");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x0f".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": forward-search-history")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"".to_vec()));

    // Forward from the middle searches ahead inclusively: Up Up lands on
    // `beta`, so `C-o alpha RET` finds `alpha two`.
    let mut history = History::new();
    history.push("alpha one");
    history.push("beta");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x10\x10".to_vec()),
        TerminalEvent::Bytes(b"\x0f".to_vec()),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": forward-search-history")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"alpha two".to_vec()));
}

#[test]
fn forward_isearch_terminator_point_matches_oracle() {
    // Forward match leaves point at the match start (patch 0 Bash 5.3 PTY
    // oracle via remapped C-o to avoid PTY XOFF): Up Up lands on `beta`,
    // `C-o two C-J !` yields `alpha !two`.
    let mut history = History::new();
    history.push("alpha one");
    history.push("beta");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x10\x10".to_vec()),
        TerminalEvent::Bytes(b"\x0f".to_vec()),
        TerminalEvent::Bytes(b"two".to_vec()),
        TerminalEvent::Bytes(b"\n".to_vec()),
        TerminalEvent::Bytes(b"!".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": forward-search-history")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"alpha !two".to_vec()));

    // Forward with no match from the end keeps the original point
    // (`draft` + C-o `zzz` + C-J + `!` yields `draft!`), while backward
    // yields `!draft`.
    let mut history = History::new();
    history.push("alpha one");
    history.push("beta");
    history.push("alpha two");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"draft".to_vec()),
        TerminalEvent::Bytes(b"\x0f".to_vec()),
        TerminalEvent::Bytes(b"zzz".to_vec()),
        TerminalEvent::Bytes(b"\n".to_vec()),
        TerminalEvent::Bytes(b"!".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": forward-search-history")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"draft!".to_vec()));
}

#[test]
fn vi_search_starts_from_history_cursor_with_literal_query() {
    // GNU vi `/` searches backward and `?` searches forward as
    // non-incremental search (patch 0 Bash 5.3 PTY oracle): the prompt
    // shows `/query`/`?query`, Enter executes, and backward search skips
    // the history cursor. `kk` lands on `beta`, so `/alpha` finds
    // `alpha one` while
    // `?alpha` finds `alpha two`; from the end `/alpha` finds `alpha two`
    // and `?alpha` finds nothing and keeps the empty line.
    for (key, from_mid, expected) in [
        ("/", false, "alpha two"),
        ("?", false, ""),
        ("/", true, "alpha one"),
        ("?", true, "alpha two"),
    ] {
        let mut history = History::new();
        history.push("alpha one");
        history.push("beta");
        history.push("alpha two");
        let mut events = vec![TerminalEvent::Bytes(b"\x1b".to_vec())];
        if from_mid {
            events.push(TerminalEvent::Bytes(b"kk".to_vec()));
        }
        events.push(TerminalEvent::Bytes(key.as_bytes().to_vec()));
        events.push(TerminalEvent::Bytes(b"alpha".to_vec()));
        events.push(TerminalEvent::Bytes(b"\r".to_vec()));
        events.push(TerminalEvent::Bytes(b"\r".to_vec()));
        let terminal = MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, history);
        line.load_inputrc_str("set editing-mode vi").unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.as_bytes().to_vec()),
            "key={key:?} from_mid={from_mid}"
        );
    }
}

#[test]
fn non_incremental_search_again_uses_completed_search_across_reads() {
    // GNU retains the completed search across prompts: searching for `foo`
    // finds `foo new`, and repeating the search on the next prompt finds the
    // older `foo old` instead of dinging on an empty line.
    let mut history = History::new();
    history.push("foo old");
    history.push("foo new");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"foo".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str(
        "\"\\C-o\": non-incremental-reverse-search-history\n\"\\C-]\": non-incremental-reverse-search-history-again",
    )
    .unwrap();
    let first = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(first, ReadlineResult::Line(b"foo new".to_vec()));

    *line.terminal_mut() = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x1d]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let second = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(second, ReadlineResult::Line(b"foo old".to_vec()));
}

#[test]
fn excessive_forward_search_rollback_preserves_point() {
    let mut history = History::new();
    history.push("alpha one");
    history.push("alpha two");
    history.push("alpha three");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x10, 0x10, 0x10]),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(vec![0x0b]),
        TerminalEvent::Bytes(b"alpha".to_vec()),
        TerminalEvent::Bytes(vec![0x02]),
        TerminalEvent::Bytes(vec![0x1b, b'5', 0x1e]),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("set history-preserve-point on\n\"\\C-^\": history-search-forward")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("alphZa three".as_bytes().to_vec())
    );
}
