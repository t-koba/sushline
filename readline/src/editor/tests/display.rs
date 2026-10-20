use super::*;

/// The display model follows the diagnostic sink.
///
/// A diagnostic that reaches the terminal's own screen moves the terminal
/// cursor, so the tracked cursor row moves with it. A diagnostic that goes
/// to a separate sink leaves the terminal display untouched, so the next
/// redisplay starts at the rendered line again instead of erasing what the
/// terminal already showed above it.
#[test]
fn diagnostic_row_tracking_follows_the_sink() {
    let message = "readline: maximum macro execution nesting level exceeded";

    // A sink on the terminal's own screen: the message is written below
    // the rendered line and occupies the rows it wraps to.
    let mut line = Editor::new(
        Config::default(),
        MemoryTerminal::with_events(Vec::new()),
        History::new(),
    );
    line.terminal.columns = 20;
    line.terminal.diagnostics_on_screen = true;
    let mut state = EditorState::new(Prompt::new("> "), None);
    // The rendered line occupies two terminal rows and the cursor is on
    // the second one.
    state.display.rendered_rows = 1;
    state.display.rendered_cursor_row = 1;

    line.write_diagnostic_below_rendered_line(&mut state, message)
        .unwrap();

    // The message wraps to two rows at twenty columns, and its terminator
    // takes the row below them.
    assert_eq!(state.display.rendered_cursor_row, 5);
    assert!(line.terminal.out.contains("\r\n"));

    // A sink on another stream: the message never reaches the screen, so
    // the tracked cursor row stays where the last redisplay left it.
    let mut line = Editor::new(
        Config::default(),
        MemoryTerminal::with_events(Vec::new()),
        History::new(),
    );
    line.terminal.columns = 20;
    let mut state = EditorState::new(Prompt::new("> "), None);
    state.display.rendered_rows = 1;
    state.display.rendered_cursor_row = 1;

    line.write_diagnostic_below_rendered_line(&mut state, message)
        .unwrap();

    assert_eq!(state.display.rendered_cursor_row, 1);
    assert_eq!(line.terminal.out, "");
    assert_eq!(line.terminal.diagnostics, format!("{message}\n"));
}

/// The redisplay after a diagnostic must not start above the rendered line.
///
/// The rendered line fills two terminal rows at twenty columns, so a
/// redisplay that starts above its first row erases output the terminal
/// already showed.
#[test]
fn diagnostic_to_a_separate_sink_keeps_the_redisplay_on_the_rendered_line() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"0123456789ABCDEFGHIJ".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.terminal.columns = 20;
    line.load_inputrc_str("\"\\C-o\": \"x\\C-oS\"").unwrap();

    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();

    assert!(
        line.terminal
            .diagnostics
            .contains("readline: maximum macro execution nesting level exceeded")
    );
    // The rendered line fills two rows, so the redisplay never moves the
    // cursor above its first row.
    assert!(
        line.terminal.moved_up.iter().all(|&rows| rows <= 1),
        "{:?}",
        line.terminal.moved_up
    );
    assert!(matches!(result, ReadlineResult::Line(_)));
}
