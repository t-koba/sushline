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
fn vi_unix_word_rubout_uses_readline_word_boundary() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"foo/bar".to_vec()),
        TerminalEvent::Bytes(vec![0x17]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("foo/".as_bytes().to_vec()));
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

#[test]
fn vi_zero_width_change_does_not_enter_insert() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"c".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("Xabc".as_bytes().to_vec()));
}

#[test]
fn vi_char_search_target_persists_across_reads_for_repeat() {
    // GNU remembers the completed `f` search across prompts: `;` repeats it
    // on the next line instead of dinging, so `x` removes the found `c`.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcde".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"f".to_vec()),
        TerminalEvent::Bytes(b"c".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let first = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(first, ReadlineResult::Line(b"abcde".to_vec()));

    *line.terminal_mut() = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcde".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b";".to_vec()),
        TerminalEvent::Bytes(b"x".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let second = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(second, ReadlineResult::Line(b"abde".to_vec()));
}

#[test]
fn vi_last_change_persists_across_reads_for_dot_repeat() {
    // GNU replays the completed change across prompts: `x` on the first line
    // arms `.` for the next line instead of leaving it unbound.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"x".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let first = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(first, ReadlineResult::Line(b"bc".to_vec()));

    *line.terminal_mut() = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"def".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b".".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let second = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(second, ReadlineResult::Line(b"ef".to_vec()));
}

#[test]
fn vi_mark_name_must_be_a_lowercase_letter() {
    // GNU Readline names a vi mark by a lowercase ASCII letter, and rings
    // for any other name.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"m".to_vec()),
        TerminalEvent::Bytes(b"A".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abc".as_bytes().to_vec()));
    assert!(
        line.terminal().out.contains('\x07'),
        "a mark name that is not a lowercase letter must ring"
    );
}

#[test]
fn vi_multibyte_character_search_takes_the_whole_character() {
    // The search target is one character, so the bytes of a multibyte
    // character wait for the rest of it and the search matches it.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a\xC3\xA9b".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"0".to_vec()),
        TerminalEvent::Bytes(b"f".to_vec()),
        TerminalEvent::Bytes(vec![0xC3]),
        TerminalEvent::Bytes(vec![0xA9]),
        TerminalEvent::Bytes(b"i".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\x1b".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set editing-mode vi").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("aXéb".as_bytes().to_vec()));
}
