mod common;

use common::MemoryTerminal;
use readline::{
    Config, Editor, History, HistoryExpansionContext, HistoryExpansionPolicy, Hooks, Prompt,
    ReadlineResult, TerminalEvent, expand_history,
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
                &HistoryExpansionPolicy::default(),
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
fn history_expansion_command_without_hook_leaves_line_unchanged() {
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
    assert_eq!(result, ReadlineResult::Line(b"!!".to_vec()));
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
