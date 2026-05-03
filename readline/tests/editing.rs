mod common;

use common::MemoryTerminal;
use readline::{
    Config, Edit, Editor, History, Hooks, InputrcPath, Prompt, ReadlineResult, TerminalEvent,
};

struct EditingHook;

impl Hooks for EditingHook {
    fn version(&self) -> Option<String> {
        Some("GNU bash, version test".to_string())
    }

    fn edit_and_execute(&mut self, line: &[u8]) -> Option<Vec<u8>> {
        let mut out = line.to_vec();
        out.extend_from_slice(b" edited");
        Some(out)
    }

    fn expand_application_line(&mut self, line: &[u8]) -> Option<Vec<u8>> {
        let mut out = b"expanded ".to_vec();
        out.extend_from_slice(line);
        Some(out)
    }

    fn tty_status(&self) -> Option<String> {
        Some("speed 9600 baud".to_string())
    }

    fn spell_correct(&mut self, word: &[u8]) -> Option<Vec<u8>> {
        (word == b"teh").then(|| b"the".to_vec())
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

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(b"\x1b-\x14".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("ab".as_bytes().to_vec()));
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
        fn editing_word_breaks(&self) -> Option<Vec<u8>> {
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
