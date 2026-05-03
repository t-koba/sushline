mod common;

use common::MemoryTerminal;
use readline::{Config, Editor, History, Prompt, ReadlineResult, TerminalEvent};

#[test]
fn named_vi_operator_commands_delete_motion_range() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"one two".to_vec()),
        TerminalEvent::Bytes(b"\x1b0dw".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("two".as_bytes().to_vec()));
}

#[test]
fn vi_command_unbound_keys_do_not_self_insert() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"q".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abc".as_bytes().to_vec()));
}

#[test]
fn vi_forward_char_operator_deletes_one_character() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"l".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("bc".as_bytes().to_vec()));
}

#[test]
fn vi_char_search_consumes_pending_operator() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc,def".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"f".to_vec()),
        TerminalEvent::Bytes(b",".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("def".as_bytes().to_vec()));
}

#[test]
fn vi_invalid_motion_cancels_pending_operator() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"q".to_vec()),
        TerminalEvent::Bytes(b"l".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abc".as_bytes().to_vec()));
}

#[test]
fn vi_overstrike_enters_replace_insert_mode() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"R".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xbc".as_bytes().to_vec()));
}

#[test]
fn vi_change_to_end_redo_replays_insert_text() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc def".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"C".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b".".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("X".as_bytes().to_vec()));
}

#[test]
fn vi_substitute_line_updates_kill_register() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"S".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"p".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xabc".as_bytes().to_vec()));
}

#[test]
fn vi_unix_word_rubout_uses_whitespace_boundary() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"foo/bar".to_vec()),
        TerminalEvent::Bytes(vec![0x17]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(Vec::new()));
}

#[test]
fn named_vi_operator_commands_handle_double_operator() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"one two".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"y".to_vec()),
        TerminalEvent::Bytes(b"y".to_vec()),
        TerminalEvent::Bytes(b"$".to_vec()),
        TerminalEvent::Bytes(b"p".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("one twoone two".as_bytes().to_vec())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"one two".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("X".as_bytes().to_vec()));
}

#[test]
fn vi_character_marks_round_trip() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"m".to_vec()),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"$".to_vec()),
        TerminalEvent::Bytes(b"`".to_vec()),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xabc".as_bytes().to_vec()));
}

#[test]
fn vi_character_search_uses_next_key_and_repeats() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcabc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"f".to_vec()),
        TerminalEvent::Bytes(b"c".to_vec()),
        TerminalEvent::Bytes(b";".to_vec()),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcabXc".as_bytes().to_vec()));
}

#[test]
fn vi_till_search_repeats_in_reverse_with_comma() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcabc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"t".to_vec()),
        TerminalEvent::Bytes(b"c".to_vec()),
        TerminalEvent::Bytes(b";".to_vec()),
        TerminalEvent::Bytes(b",".to_vec()),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcXabc".as_bytes().to_vec()));
}

#[test]
fn vi_named_registers_store_and_put_text() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc def".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"\"".to_vec()),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"y".to_vec()),
        TerminalEvent::Bytes(b"w".to_vec()),
        TerminalEvent::Bytes(b"$".to_vec()),
        TerminalEvent::Bytes(b"\"".to_vec()),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"p".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("abc defabc".as_bytes().to_vec())
    );
}

#[test]
fn vi_redo_replays_insert_change_group() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"$".to_vec()),
        TerminalEvent::Bytes(b".".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("XabcX".as_bytes().to_vec()));
}

#[test]
fn vi_redo_replays_operator_motion_change() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"one two three".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"w".to_vec()),
        TerminalEvent::Bytes(b".".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("three".as_bytes().to_vec()));
}

#[test]
fn vi_operator_pending_state_is_visible_in_prompt() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"one two".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"d".to_vec()),
        TerminalEvent::Bytes(b"w".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(
        "set editing-mode vi\nset show-mode-in-prompt on\nset vi-cmd-mode-string CMD:",
    )
    .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("two".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("CMD:d> "));
}
