use super::*;

#[test]
fn interrupted_readline_cleans_up_terminal_modes() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Signal(libc::SIGINT)]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal.out.contains("> ^C\r\n"));
    assert!(line.terminal.out.contains("\x1b[?2004h"));
    assert!(line.terminal.out.contains("\x1b[?2004l"));
}

#[test]
fn interrupted_readline_marks_current_input_line() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Signal(libc::SIGINT),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());

    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();

    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal.out.contains("> ^C\r\n"));
    assert!(line.terminal.cleared_screen > 0);
}
