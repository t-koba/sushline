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
fn bracketed_paste_variable_enables_terminal_mode_and_pastes_literal_text() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[200~".to_vec()),
        TerminalEvent::Bytes(b"literal\ntext\x1b[201~".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("literal\ntext".as_bytes().to_vec())
    );
    assert!(line.terminal().out.contains("\x1b[?2004h"));
    assert!(line.terminal().out.contains("\x1b[?2004l"));
}
