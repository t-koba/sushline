mod common;

use common::MemoryTerminal;
use readline::{
    Config, Editor, History, HistoryChars, HistoryExpansion, HistoryExpansionContext,
    HistoryExpansionPolicy, Hooks, Prompt, ReadlineResult, TerminalEvent, expand_history,
    get_history_event, history_arg_extract, history_tokenize,
};

struct AliasHook;

impl Hooks for AliasHook {
    fn expand_aliases(&mut self, line: &[u8]) -> Option<Vec<u8>> {
        line.strip_prefix(b"ll").map(|rest| {
            let mut out = b"ls -l".to_vec();
            out.extend_from_slice(rest);
            out
        })
    }
}

struct HistoryHook;

impl Hooks for HistoryHook {
    fn expand_history(
        &mut self,
        context: HistoryExpansionContext<'_>,
    ) -> Option<Result<Vec<u8>, String>> {
        Some(
            expand_history(
                context.line,
                context.history,
                context.histchars,
                context.policy,
                |_| false,
            )
            .map_err(|err| err.message()),
        )
    }
}

struct HistoryAndAliasHook;

impl Hooks for HistoryAndAliasHook {
    fn expand_aliases(&mut self, line: &[u8]) -> Option<Vec<u8>> {
        line.strip_prefix(b"ll").map(|rest| {
            let mut out = b"ls -l".to_vec();
            out.extend_from_slice(rest);
            out
        })
    }

    fn expand_history(
        &mut self,
        context: HistoryExpansionContext<'_>,
    ) -> Option<Result<Vec<u8>, String>> {
        HistoryHook.expand_history(context)
    }
}

#[test]
fn readline_reexports_history_helper_apis() {
    let policy = HistoryExpansionPolicy::default();
    let line = b"echo one | sed 's/o/O/'";
    assert_eq!(
        history_tokenize(line, &policy),
        vec![
            b"echo".to_vec(),
            b"one".to_vec(),
            b"|".to_vec(),
            b"sed".to_vec(),
            b"'s/o/O/'".to_vec(),
        ]
    );
    assert_eq!(
        history_arg_extract(1, 3, line, &policy),
        Some(b"one | sed".to_vec())
    );

    let mut history = History::new();
    history.push("echo previous");
    let event = get_history_event(b"!!", &history, HistoryChars::parse("!^#"), &policy)
        .unwrap()
        .expect("event");
    assert_eq!(event.line, b"echo previous");
    assert_eq!(event.next_index, 2);
}

#[test]
fn named_history_expansion_commands_expand_events() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"!!:1-$:gs/e/E/".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push("echo previous value");
    let config = Config {
        auto_add_history: true,
        ..Default::default()
    };
    let mut line = Editor::new(config, terminal, history);
    line.load_inputrc_str("\"\\C-o\": history-expand-line")
        .unwrap();
    let mut hooks = HistoryHook;
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("prEvious valuE".as_bytes().to_vec())
    );
}

#[test]
fn history_expansion_command_without_hook_uses_core_expander() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"!!".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push("echo previous");
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": history-expand-line")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"echo previous".to_vec()));
}

#[test]
fn history_expansion_command_uses_inputrc_policy_variables() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"'!!'".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push("echo previous");
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str(
        "set history-quotes-inhibit-expansion on\n\"\\C-o\": history-expand-line",
    )
    .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"'!!'".to_vec()));
}

#[test]
fn history_expansion_print_only_status_is_observable() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"!!:p".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push("echo previous");
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": history-expand-line")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"!!:p".to_vec()));
    assert!(line.terminal().out.contains("echo previous"));
}

#[test]
fn history_expansion_hook_can_return_print_only_status() {
    struct PrintOnlyHook;

    impl Hooks for PrintOnlyHook {
        fn expand_history_with_status(
            &mut self,
            context: HistoryExpansionContext<'_>,
        ) -> Option<Result<HistoryExpansion, String>> {
            assert_eq!(context.line, b"!!");
            Some(Ok(HistoryExpansion {
                line: b"hook-expanded".to_vec(),
                print_only: true,
            }))
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"!!".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = PrintOnlyHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": history-expand-line")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"!!".to_vec()));
    assert!(line.terminal().out.contains("hook-expanded"));
}

#[test]
fn alias_expansion_uses_shell_hook_boundary() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ll /tmp".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = AliasHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": alias-expand-line")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("ls -l /tmp".as_bytes().to_vec())
    );
}

#[test]
fn history_and_alias_expansion_combines_history_then_shell_alias() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"!!".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push("ll /var");
    let mut hooks = HistoryAndAliasHook;
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("\"\\C-o\": history-and-alias-expand-line")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("ls -l /var".as_bytes().to_vec())
    );
}

#[test]
fn history_expansion_hook_can_replace_core_expander() {
    struct HistoryHook;

    impl Hooks for HistoryHook {
        fn expand_history(
            &mut self,
            context: HistoryExpansionContext<'_>,
        ) -> Option<Result<Vec<u8>, String>> {
            assert_eq!(context.line, b"!!");
            Some(Ok(b"hook-expanded".to_vec()))
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"!!".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = HistoryHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": history-expand-line")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("hook-expanded".as_bytes().to_vec())
    );
}

#[test]
fn history_size_variable_limits_accepted_history() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"new".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push("old1");
    history.push("old2");
    let config = Config {
        auto_add_history: true,
        ..Default::default()
    };
    let mut line = Editor::new(config, terminal, history);
    line.load_inputrc_str("set history-size 2").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("new".as_bytes().to_vec()));
    assert_eq!(
        line.history()
            .entries()
            .iter()
            .map(|entry| entry.line())
            .collect::<Vec<_>>(),
        vec!["old2", "new"]
    );
}
