//! Hooks contract tests.

mod common;

use common::MemoryTerminal;
use readline::{
    CompletionCandidate, CompletionOptions, CompletionRequest, CompletionResponse, Config, Edit,
    Editor, History, HistoryExpansion, HistoryExpansionContext, Hooks, LineExpansionContext,
    Prompt, QuoteContext, ReadlineResult, TerminalEvent,
};
use std::cell::Cell;

fn line_with_history(
    events: Vec<TerminalEvent>,
    history_entries: &[&str],
    hooks: &mut impl Hooks,
) -> ReadlineResult {
    let terminal = MemoryTerminal::with_events(events);
    let mut history = History::new();
    for entry in history_entries {
        history.push(*entry);
    }
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str(
        r#"
"\C-o": history-expand-line
"\C-x": shell-expand-line
"\C-y": yank-last-arg
"\C-f": shell-forward-word
        "#,
    )
    .unwrap();
    line.read_line(Prompt::new("> "), hooks).unwrap()
}

#[test]
fn default_history_expansion_is_builtin_and_hooks_can_pass_through() {
    let result = line_with_history(
        vec![
            TerminalEvent::Bytes(b"!!".to_vec()),
            TerminalEvent::Bytes(vec![0x0f]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
        &["echo previous"],
        &mut (),
    );
    assert_eq!(result, ReadlineResult::Line(b"echo previous".to_vec()));

    struct PassThroughHistory;
    impl Hooks for PassThroughHistory {
        fn expand_history(
            &mut self,
            context: HistoryExpansionContext<'_>,
        ) -> Result<HistoryExpansion, String> {
            Ok(HistoryExpansion::unchanged(context.line))
        }
    }

    let result = line_with_history(
        vec![
            TerminalEvent::Bytes(b"!!".to_vec()),
            TerminalEvent::Bytes(vec![0x0f]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
        &["echo previous"],
        &mut PassThroughHistory,
    );
    assert_eq!(result, ReadlineResult::Line(b"!!".to_vec()));
}

#[test]
fn expand_line_receives_context_and_can_set_point() {
    struct ExpandHook {
        seen: bool,
    }

    impl Hooks for ExpandHook {
        fn expand_line(&mut self, context: LineExpansionContext<'_>) -> Option<Edit> {
            assert_eq!(context.line, b"abcd");
            assert_eq!(context.point, 2);
            self.seen = true;
            Some(Edit {
                line: Some(b"wxyz".to_vec()),
                point: Some(1),
                mark: None,
            })
        }
    }

    let mut hooks = ExpandHook { seen: false };
    let result = line_with_history(
        vec![
            TerminalEvent::Bytes(b"abcd".to_vec()),
            TerminalEvent::Bytes(vec![0x02, 0x02, 0x18]),
            TerminalEvent::Bytes(b"!\r".to_vec()),
        ],
        &[],
        &mut hooks,
    );
    assert_eq!(result, ReadlineResult::Line(b"w!xyz".to_vec()));
    assert!(hooks.seen);
}

#[test]
fn quote_completion_controls_hook_quoting_and_builtin_fallback() {
    struct QuoteHook {
        quote: bool,
    }

    impl Hooks for QuoteHook {
        fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"alpha file".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    ..Default::default()
                },
            })
        }

        fn quote_completion(&mut self, context: QuoteContext<'_>) -> Option<Vec<u8>> {
            assert_eq!(context.value, b"alpha file");
            assert!(context.quote_filename);
            self.quote.then(|| b"hook quoted".to_vec())
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"alp".to_vec()),
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line
        .read_line(Prompt::new("> "), &mut QuoteHook { quote: false })
        .unwrap();
    assert_eq!(result, ReadlineResult::Line(b"alpha\\ file ".to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"alp".to_vec()),
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line
        .read_line(Prompt::new("> "), &mut QuoteHook { quote: true })
        .unwrap();
    assert_eq!(result, ReadlineResult::Line(b"hook quoted ".to_vec()));
}

#[test]
fn shell_words_only_affects_history_words_not_shell_word_motion() {
    struct WordsOnlyHook {
        words_called: Cell<bool>,
    }

    impl Hooks for WordsOnlyHook {
        fn shell_words(&mut self, _: &[u8]) -> Option<Vec<Vec<u8>>> {
            self.words_called.set(true);
            Some(vec![b"HOOKWORD".to_vec()])
        }
    }

    let mut hooks = WordsOnlyHook {
        words_called: Cell::new(false),
    };
    let result = line_with_history(
        vec![
            TerminalEvent::Bytes(vec![0x19]),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ],
        &["echo builtin"],
        &mut hooks,
    );
    assert_eq!(result, ReadlineResult::Line(b"HOOKWORD".to_vec()));
    assert!(hooks.words_called.get());

    let mut hooks = WordsOnlyHook {
        words_called: Cell::new(false),
    };
    let result = line_with_history(
        vec![
            TerminalEvent::Bytes(b"aa:bb".to_vec()),
            TerminalEvent::Bytes(vec![0x01, 0x06]),
            TerminalEvent::Bytes(b"X\r".to_vec()),
        ],
        &[],
        &mut hooks,
    );
    assert_eq!(result, ReadlineResult::Line(b"aa:bbX".to_vec()));
    assert!(!hooks.words_called.get());
}

#[test]
fn invalid_shell_word_spans_fall_back_to_builtin_parser() {
    struct InvalidSpanHook;
    impl Hooks for InvalidSpanHook {
        fn shell_word_spans(&mut self, _: &[u8]) -> Option<Vec<(usize, usize)>> {
            Some(vec![(2, 2), (1, 4)])
        }
    }

    let result = line_with_history(
        vec![
            TerminalEvent::Bytes(b"aa bb".to_vec()),
            TerminalEvent::Bytes(vec![0x01, 0x06]),
            TerminalEvent::Bytes(b"X\r".to_vec()),
        ],
        &[],
        &mut InvalidSpanHook,
    );
    assert_eq!(result, ReadlineResult::Line(b"aaX bb".to_vec()));
}

#[test]
fn complete_none_and_bashdefault_use_default_complete() {
    struct DefaultHook {
        complete_empty_with_bashdefault: bool,
        default_called: Cell<bool>,
    }

    impl Hooks for DefaultHook {
        fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
            self.complete_empty_with_bashdefault
                .then(|| CompletionResponse {
                    candidates: Vec::new(),
                    options: CompletionOptions {
                        bashdefault: true,
                        ..Default::default()
                    },
                })
        }

        fn default_complete(&mut self, _: &CompletionRequest) -> Option<CompletionResponse> {
            self.default_called.set(true);
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"defaulted".to_vec(),
                    display: None,
                }],
                options: CompletionOptions::default(),
            })
        }
    }

    for complete_empty_with_bashdefault in [false, true] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"def".to_vec()),
            TerminalEvent::Bytes(b"\t".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]);
        let mut hooks = DefaultHook {
            complete_empty_with_bashdefault,
            default_called: Cell::new(false),
        };
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(result, ReadlineResult::Line(b"defaulted ".to_vec()));
        assert!(hooks.default_called.get());
    }
}

#[test]
fn glob_expand_uses_byte_patterns() {
    struct GlobHook;
    impl Hooks for GlobHook {
        fn glob_expand(&mut self, pattern: &[u8]) -> Option<Vec<Vec<u8>>> {
            assert_eq!(pattern, b"pre\xff*");
            Some(vec![b"pre\xff-match".to_vec()])
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"pre\xff*".to_vec()),
        TerminalEvent::Bytes(vec![0x18, b'*']),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut GlobHook).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"pre\xff-match".to_vec()));
}
