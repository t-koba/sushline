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

/// The pending numeric argument is drawn in place of the prompt.
///
/// GNU Readline 8.3 draws `(arg: N) ` where the prompt would be while a
/// numeric argument is pending, and the indicator grows with the argument
/// (Bash 5.3 PTY observation on the host's Readline 8.3).
#[test]
fn a_pending_numeric_argument_draws_the_gnu_indicator_in_place_of_the_prompt() {
    let line = Editor::new(
        Config::default(),
        MemoryTerminal::with_events(Vec::new()),
        History::new(),
    );
    let mut state = EditorState::new(Prompt::new("P> "), None);

    assert_eq!(line.effective_prompt(&state).0, "P> ");

    state.numeric_arg = Some(3);
    let (prompt, width) = line.effective_prompt(&state);
    assert_eq!(prompt, "(arg: 3) ");
    assert_eq!(width, 9);

    // The indicator grows with the argument, and a sign-only prefix shows
    // its sign.
    state.numeric_arg = Some(34);
    assert_eq!(line.effective_prompt(&state).0, "(arg: 34) ");
    state.numeric_arg = Some(-1);
    assert_eq!(line.effective_prompt(&state).0, "(arg: -1) ");

    // A leading zero is an argument of its own.
    state.numeric_arg = Some(0);
    assert_eq!(line.effective_prompt(&state).0, "(arg: 0) ");

    state.numeric_arg = None;
    assert_eq!(line.effective_prompt(&state).0, "P> ");
}

/// The indicator is drawn while the argument is pending, and the prompt
/// returns once the argument is consumed.
#[test]
fn the_numeric_argument_indicator_is_drawn_until_the_argument_is_consumed() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"\x1b3x\r".to_vec())]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("P> "), &mut ());

    assert_eq!(result.unwrap(), ReadlineResult::Line(b"xxx".to_vec()));
    let out = line.terminal.out.clone();
    assert!(
        out.contains("(arg: 3) "),
        "the pending argument must be drawn: {out:?}"
    );
    assert!(
        out.contains("P> xxx"),
        "the prompt must return with the consumed argument: {out:?}"
    );
}
