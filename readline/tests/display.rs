mod common;

use common::MemoryTerminal;
use readline::{Config, Editor, History, Prompt, ReadlineResult, TerminalEvent, TerminalSize};

#[test]
fn mark_modified_lines_adds_prompt_marker_for_changed_history_line() {
    let mut history = History::new();
    history.push("abcdef");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x10]),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("set mark-modified-lines on").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcdefZ".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("*> abcdefZ"));
}

#[test]
fn horizontal_scroll_mode_renders_window_around_point() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdefghijkl".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 8;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set horizontal-scroll-mode on")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert!(line.terminal().out.contains("hijkl"));
    assert!(!line.terminal().out.contains("> abcdefghijkl"));
}

#[test]
fn redisplay_accounts_for_multiline_prompt_and_wrap_column() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 6;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let _ = line
        .read_line(Prompt::new("ignored\nλ> "), &mut ())
        .unwrap();
    assert!(line.terminal().out.contains("ignored\nλ> abcdef"));
    assert_eq!(line.terminal().moved_columns.last(), Some(&3));
    assert!(line.terminal().moved_up.iter().any(|rows| *rows > 0));
    assert!(line.terminal().cleared_screen > 0);
}

#[test]
fn resize_event_recomputes_wrap_using_new_terminal_size() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Resize(TerminalSize {
            columns: 12,
            rows: 24,
        }),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 4;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let _ = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(line.terminal().moved_columns.last(), Some(&8));
    assert!(line.terminal().moved_up.iter().any(|rows| *rows >= 1));
}

#[test]
fn enable_active_region_highlights_marked_region() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x01".to_vec()),
        TerminalEvent::Bytes(b"\x00".to_vec()),
        TerminalEvent::Bytes(b"\x06".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-active-region on")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert!(line.terminal().out.contains("\x1b[7ma\x1b[0m"));
}

#[test]
fn redisplay_renders_control_characters_visibly() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x11]),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("\x01".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("^A"));
}
