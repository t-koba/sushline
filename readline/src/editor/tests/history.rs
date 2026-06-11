use super::*;

#[test]
fn history_expansion_supports_quick_substitution_and_event_search() {
    let mut history = History::new();
    history.push("echo src/lib.rs src/main.rs");
    history.push("git checkout main");

    assert_eq!(
        expand_history_for_test("^main^feature^", &history),
        "git checkout feature"
    );
    assert_eq!(
        expand_history_for_test("^main^feature", &history),
        "git checkout feature"
    );
    assert_eq!(
        expand_history_for_test("!?checkout?:2", &history),
        "main".to_string()
    );
    assert_eq!(
        expand_history_for_test("!?checkout?:%", &history),
        "checkout"
    );
    assert_eq!(
        expand_history_for_test("!-2:$:r", &history),
        "src/main".to_string()
    );
    assert_eq!(
        expand_history_for_test("!-2:1-$", &history),
        "src/lib.rs src/main.rs".to_string()
    );
    assert_eq!(
        expand_history_for_test("!-2:1-2", &history),
        "src/lib.rs src/main.rs".to_string()
    );
}

#[test]
fn history_expansion_honors_histchars_variable() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"%".to_vec()),
        TerminalEvent::Bytes(b"%".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push("echo custom");
    let config = Config {
        auto_add_history: true,
        ..Default::default()
    };
    let mut line = Editor::new(config, terminal, history);
    line.load_inputrc_str("set histchars %~#\n\"\\C-o\": history-expand-line")
        .unwrap();
    let mut hooks = TestHistoryHook;
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("echo custom".as_bytes().to_vec())
    );

    let mut history = History::new();
    history.push("git checkout main");
    assert_eq!(
        expand_history_with_chars_for_test("~main~dev~", &history, HistoryChars::parse("%~#")),
        Ok("git checkout dev".to_string())
    );
}

#[test]
fn history_expansion_reports_event_and_word_errors() {
    let mut history = History::new();
    history.push("echo one");

    assert_eq!(
        expand_history_with_chars_for_test("!missing", &history, HistoryChars::parse("!^#")),
        Err(HistoryExpansionError::EventNotFound("!missing".to_string()))
    );
    assert_eq!(
        expand_history_with_chars_for_test("!!:9", &history, HistoryChars::parse("!^#")),
        Err(HistoryExpansionError::BadWordSpecifier("9".to_string()))
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"!missing".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": history-expand-line")
        .unwrap();
    let mut hooks = TestHistoryHook;
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("!missing".as_bytes().to_vec()));
    assert!(line.terminal.out.contains("!missing: event not found"));
}
