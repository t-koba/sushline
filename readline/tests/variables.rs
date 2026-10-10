mod common;

use common::MemoryTerminal;
use readline::{Config, Editor, History, Prompt, ReadlineResult, TerminalEvent};

#[test]
fn variables_api_exposes_variables_without_map_leakage() {
    let terminal = MemoryTerminal::with_events(Vec::new());
    let mut line = Editor::new(Config::default(), terminal, History::new());
    assert_eq!(
        line.variables().get("editing-mode").map(String::as_str),
        Some("emacs")
    );

    line.variables_mut()
        .insert("bell-style".to_string(), "none".to_string());
    assert!(line.variables().contains_key("bell-style"));
    assert_eq!(
        line.variables().get("bell-style").map(String::as_str),
        Some("none")
    );
}

#[test]
fn meta_variables_translate_eight_bit_input() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0xe1]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set convert-meta on\n\"\\ea\": \"META\"")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("META".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0xe1]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set input-meta off\nset meta-flag off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("a".as_bytes().to_vec()));
}

#[test]
fn output_meta_and_enable_meta_key_have_terminal_side_effects() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes("é".as_bytes().to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set output-meta off\nset enable-meta-key off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("é".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("\\303\\251"));
    assert_eq!(line.terminal().meta_enabled, vec![false]);
}

#[test]
fn enable_keypad_has_terminal_side_effects() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"\r".to_vec())]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-keypad on").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(Vec::new()));
    assert_eq!(line.terminal().keypad_enabled, vec![true, false]);
}

#[test]
fn csi_skip_commands_consume_terminal_escape_sequence() {
    for command in ["skip-csi-sequence", "arrow-key-prefix"] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(vec![0x0f]),
            TerminalEvent::Bytes(b"\x1b[1;5C".to_vec()),
            TerminalEvent::Bytes(b"X".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(&format!("\"\\C-o\": {command}"))
            .unwrap();
        let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line("X".as_bytes().to_vec()),
            "{command}"
        );
    }
}

#[test]
fn less_common_variables_have_observable_side_effects() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(vec![0x08]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.tty_special = vec![(0x08, "backward-delete-char")];
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set bind-tty-special-chars on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("a".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"(a)".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set blink-matching-paren on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("(a)".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("\x1b[s"));
    assert!(line.terminal().out.contains("\x1b[u"));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes("é".as_bytes().to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set output-meta on\nset byte-oriented on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("é".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("\\303\\251"));
}

#[test]
fn tty_special_bindings_win_over_prior_user_bindings_while_on() {
    // Decided policy: with bind-tty-special-chars on, the tty byte rebinds
    // each read and wins over an inputrc user binding for the same byte.
    // The user binding is a macro so the control case proves it parsed:
    // without tty metadata the same byte inserts `Q`.
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(vec![0x08]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.tty_special = vec![(0x08, "backward-delete-char")];
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set bind-tty-special-chars on\n\"\\C-h\": \"Q\"")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("a".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(vec![0x08]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set bind-tty-special-chars on\n\"\\C-h\": \"Q\"")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abQ".as_bytes().to_vec()));
}

#[test]
fn blink_matching_paren_uses_rendered_control_char_width() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x16]),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"(".to_vec()),
        TerminalEvent::Bytes(b")".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set blink-matching-paren on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(vec![0x01, b'(', b')']));
    // Prompt `> ` is 2 cells and `\x01` renders as `^A` (2 cells), so `(` sits at column 4.
    // Blink moves there between the final render's reset to column 0 and its move
    // to the end of `^A()` (column 6).
    let moved = &line.terminal().moved_columns;
    assert!(
        moved.len() >= 3 && moved[moved.len() - 3..] == [4, 0, 6],
        "blink then final render should be [4, 0, 6], got {moved:?}"
    );
}

#[test]
fn blink_matching_paren_moves_up_on_wrapped_lines() {
    let mut events = vec![TerminalEvent::Bytes(b"(".to_vec())];
    for _ in 0..8 {
        events.push(TerminalEvent::Bytes(b"a".to_vec()));
    }
    events.push(TerminalEvent::Bytes(b")".to_vec()));
    events.push(TerminalEvent::Bytes(b"\r".to_vec()));
    let mut terminal = MemoryTerminal::with_events(events);
    terminal.columns = 10;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set blink-matching-paren on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"(aaaaaaaa)".to_vec()));
    // Prompt `> ` is 2 cells; `(` sits at (row 0, col 2) while point after `)`
    // is on row 1, so blink must move up one row before moving to column 2.
    assert_eq!(line.terminal().moved_up.last(), Some(&1));
    let moved = &line.terminal().moved_columns;
    assert!(
        moved.len() >= 4 && moved[moved.len() - 4..] == [2, 0, 0, 2],
        "blink then final render should be [2, 0, 0, 2], got {moved:?}"
    );
}

#[test]
fn show_mode_in_prompt_adds_mode_string() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"\r".to_vec())]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set show-mode-in-prompt on\nset emacs-mode-string EMACS:")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert!(line.terminal().out.contains("EMACS:"));
    assert!(line.terminal().out.contains("> "));
}

#[test]
fn bracketed_paste_off_still_pastes_without_terminal_mode() {
    // Oracle (patch 0 baseline, Bash 5.3 PTY): the variable gates terminal
    // DEC mode only; injected `ESC[200~...ESC[201~` still pastes literally
    // when off, including framed controls, and sets the mark.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[200~".to_vec()),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"\x1b[201~".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"a".to_vec()));
    assert!(!line.terminal().out.contains("\x1b[?2004h"));
    // Framed control byte stays literal inside the paste (outside it would
    // dispatch as a command); the paste sets the mark like the on case.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[200~".to_vec()),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\x1b[201~".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(vec![0x01]));
    assert!(!line.terminal().out.contains("\x1b[?2004h"));
}

#[test]
fn bracketed_paste_off_guards_mark_hold_and_lone_end() {
    // Pasted text sets the mark: C-x C-x exchanges point with the paste
    // start, so `X` lands before the pasted `a` (without a mark the
    // exchange is a no-op and the line would be `aX`).
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[200~".to_vec()),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"\x1b[201~".to_vec()),
        TerminalEvent::Bytes(b"\x18\x18".to_vec()),
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"Xa".to_vec()));
    assert!(!line.terminal().out.contains("\x1b[?2004h"));
    // An unterminated begin holds the line: the intermediate CR is paste
    // payload, so only the CR after the end marker accepts the line.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[200~".to_vec()),
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
        TerminalEvent::Bytes(b"b".to_vec()),
        TerminalEvent::Bytes(b"\x1b[201~".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(vec![b'a', b'\r', b'b']));
    assert!(!line.terminal().out.contains("\x1b[?2004h"));
    // A lone unframed end marker takes the normal unbound path: ESC is
    // filtered as a control and the remaining bytes insert literally.
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[201~".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"[201~".to_vec()));
    assert!(!line.terminal().out.contains("\x1b[?2004h"));
}

#[test]
fn bracketed_paste_preserves_trailing_bytes_in_same_chunk() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[200~".to_vec()),
        TerminalEvent::Bytes(b"hi\x1b[201~X".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"hiX".to_vec()));
}

#[test]
fn bracketed_paste_variable_enables_terminal_mode_and_pastes_literal_text() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[200~".to_vec()),
        TerminalEvent::Bytes(b"literal".to_vec()),
        TerminalEvent::Bytes(b"\x03".to_vec()),
        TerminalEvent::Bytes(b"\ntext\x1b[201~".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"literal\x03\ntext".to_vec()));
    assert!(line.terminal().out.contains("\x1b[?2004h"));
    assert!(line.terminal().out.contains("\x1b[?2004l"));
}
