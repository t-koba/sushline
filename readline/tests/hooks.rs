//! Hooks contract tests.

mod common;

use common::MemoryTerminal;
use readline::{
    CompletionCandidate, CompletionOptions, CompletionRequest, CompletionResponse, CompletionType,
    Config, Edit, Editor, History, HistoryExpansion, HistoryExpansionContext, Hooks,
    LineExpansionContext, Prompt, QuoteContext, ReadlineResult, TerminalEvent,
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

#[test]
fn reopened_filename_segment_preserves_full_callback_context_and_quote_override() {
    struct SegmentHook {
        line: Vec<u8>,
        word: Vec<u8>,
        value: Vec<u8>,
        override_quote: bool,
        saw_complete: bool,
        saw_default: bool,
        saw_quote: bool,
    }

    impl SegmentHook {
        fn check_request(&self, request: &CompletionRequest) {
            assert_eq!(request.context.line, self.line);
            assert_eq!(request.context.point, self.line.len());
            assert_eq!(request.context.word_start, 5);
            assert_eq!(request.context.word_end, self.line.len());
            assert_eq!(request.context.word, self.word);
        }
    }

    impl Hooks for SegmentHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            self.check_request(&request);
            self.saw_complete = true;
            None
        }

        fn default_complete(&mut self, request: &CompletionRequest) -> Option<CompletionResponse> {
            self.check_request(request);
            self.saw_default = true;
            None
        }

        fn quote_completion(&mut self, context: QuoteContext<'_>) -> Option<Vec<u8>> {
            assert_eq!(context.line, self.line);
            assert_eq!(context.point, self.line.len());
            assert_eq!(context.word_start, 5);
            assert_eq!(context.word_end, self.line.len());
            assert_eq!(context.word, self.word);
            assert_eq!(context.quote, Some('\''));
            assert_eq!(context.value, self.value);
            self.saw_quote = true;
            self.override_quote.then(|| b"hook quoted".to_vec())
        }
    }

    let dir = tempfile::tempdir().unwrap();
    for name in ["eagle-one", "eagle-two"] {
        std::fs::write(dir.path().join(name), b"").unwrap();
    }
    let segment = format!("{}/e", dir.path().display());
    let input = format!("echo 'sa'f'{segment}").into_bytes();
    for command in ["complete", "complete-filename"] {
        for override_quote in [false, true] {
            let terminal = MemoryTerminal::with_events(vec![
                TerminalEvent::Bytes(input.clone()),
                TerminalEvent::Bytes(b"\x0fX\r".to_vec()),
            ]);
            let mut line = Editor::new(Config::default(), terminal, History::new());
            line.load_inputrc_str(&format!("\"\\C-o\": {command}"))
                .unwrap();
            let mut hooks = SegmentHook {
                line: input.clone(),
                word: format!("saf{segment}").into_bytes(),
                value: format!("saf{segment}agle-").into_bytes(),
                override_quote,
                saw_complete: false,
                saw_default: false,
                saw_quote: false,
            };
            let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
            let expected = if override_quote {
                b"echo hook quotedX".to_vec()
            } else {
                format!("echo 'sa'f'{segment}agle-X").into_bytes()
            };
            assert_eq!(result, ReadlineResult::Line(expected));
            assert!(hooks.saw_complete && hooks.saw_quote);
            assert_eq!(hooks.saw_default, command == "complete");
        }
    }
}

#[test]
fn reopened_quote_does_not_rescope_application_completion_candidates() {
    struct ApplicationHook {
        use_default: bool,
    }

    impl ApplicationHook {
        fn response(&self, request: &CompletionRequest) -> CompletionResponse {
            assert_eq!(request.context.line, b"echo 'sa'f'e");
            assert_eq!(request.context.point, 12);
            assert_eq!(request.context.word_start, 5);
            assert_eq!(request.context.word_end, 12);
            assert_eq!(request.context.word, b"safe");
            CompletionResponse {
                candidates: vec![
                    CompletionCandidate {
                        replacement: b"safe-one".to_vec(),
                        display: None,
                    },
                    CompletionCandidate {
                        replacement: b"safe-two".to_vec(),
                        display: None,
                    },
                ],
                options: CompletionOptions {
                    filenames: true,
                    ..Default::default()
                },
            }
        }
    }

    impl Hooks for ApplicationHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            (!self.use_default).then(|| self.response(&request))
        }

        fn default_complete(&mut self, request: &CompletionRequest) -> Option<CompletionResponse> {
            assert!(self.use_default);
            Some(self.response(request))
        }

        fn quote_completion(&mut self, context: QuoteContext<'_>) -> Option<Vec<u8>> {
            assert_eq!(context.word, b"safe");
            assert_eq!(context.value, b"safe-");
            None
        }
    }

    for use_default in [false, true] {
        let terminal =
            MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"echo 'sa'f'e\tX\r".to_vec())]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let result = line
            .read_line(Prompt::new("> "), &mut ApplicationHook { use_default })
            .unwrap();
        assert_eq!(result, ReadlineResult::Line(b"echo 'safe-'X".to_vec()));
    }
}

#[test]
fn fullquote_ansi_c_completion_reuses_existing_closer() {
    struct FullquoteHook {
        filenames: bool,
        called: bool,
    }
    impl Hooks for FullquoteHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, b"echo $'saf'");
            assert_eq!(request.context.point, request.context.line.len() - 1);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"saf'file".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: self.filenames,
                    fullquote: true,
                    ..Default::default()
                },
            })
        }
    }

    for filenames in [false, true] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"echo $'saf'\x1b[D".to_vec()),
            TerminalEvent::Bytes(b"\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let mut hooks = FullquoteHook {
            filenames,
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"echo $'saf'\\''file' X".to_vec())
        );
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn fullquote_ansi_c_completion_overrides_noquote() {
    struct FullquoteHook {
        noquote: bool,
        called: bool,
    }
    impl Hooks for FullquoteHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, b"echo $'saf");
            assert_eq!(request.context.point, request.context.line.len());
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"saf'file".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    fullquote: true,
                    noquote: self.noquote,
                    ..Default::default()
                },
            })
        }
    }

    for noquote in [false, true] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"echo $'saf".to_vec()),
            TerminalEvent::Bytes(b"\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let mut hooks = FullquoteHook {
            noquote,
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"echo $'saf'\\''file' X".to_vec())
        );
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn menu_nospace_ansi_c_completion_inserts_without_separator() {
    struct MenuHook {
        noquote: bool,
        called: bool,
    }
    impl Hooks for MenuHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, b"echo $'saf");
            assert_eq!(request.context.point, request.context.line.len());
            assert_eq!(request.context.word, b"saf");
            assert_eq!(
                request.context.completion_type,
                CompletionType::MenuComplete
            );
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![
                    CompletionCandidate {
                        replacement: b"saf file-one".to_vec(),
                        display: None,
                    },
                    CompletionCandidate {
                        replacement: b"saf file-two".to_vec(),
                        display: None,
                    },
                ],
                options: CompletionOptions {
                    filenames: true,
                    noquote: self.noquote,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    for noquote in [false, true] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"echo $'saf".to_vec()),
            TerminalEvent::Bytes(b"\x0fX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
        let mut hooks = MenuHook {
            noquote,
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(b"echo $'saf file-one'X".to_vec())
        );
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn ambiguous_nospace_ansi_c_completion_keeps_common_prefix_open() {
    struct AmbiguousHook {
        noquote: bool,
        called: bool,
    }
    impl Hooks for AmbiguousHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, b"echo $'saf");
            assert_eq!(request.context.point, request.context.line.len());
            assert_eq!(request.context.word, b"saf");
            assert_eq!(request.context.completion_type, CompletionType::Complete);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![
                    CompletionCandidate {
                        replacement: b"saf file-one".to_vec(),
                        display: None,
                    },
                    CompletionCandidate {
                        replacement: b"saf file-two".to_vec(),
                        display: None,
                    },
                ],
                options: CompletionOptions {
                    filenames: true,
                    noquote: self.noquote,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    for noquote in [false, true] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"echo $'saf".to_vec()),
            TerminalEvent::Bytes(b"\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let mut hooks = AmbiguousHook {
            noquote,
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(result, ReadlineResult::Line(b"echo $'saf file-X".to_vec()));
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            1
        );
    }
}

#[test]
fn fullquote_ansi_c_completion_respects_skip_completed_text_suffix() {
    struct FullquoteSuffixHook {
        called: bool,
    }
    impl Hooks for FullquoteSuffixHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, b"echo $'saf file'");
            assert_eq!(request.context.point, request.context.line.len() - 5);
            assert_eq!(request.context.completion_type, CompletionType::Complete);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"saf file".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    fullquote: true,
                    noquote: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    for (skip, expected) in [
        ("on", b"echo $'saf file'X".as_slice()),
        ("off", b"echo $'saf file'Xfile'".as_slice()),
    ] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"echo $'saf file'".to_vec()),
            TerminalEvent::Bytes(b"\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.variables_mut()
            .insert("skip-completed-text".to_string(), skip.to_string());
        let mut hooks = FullquoteSuffixHook { called: false };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.to_vec()),
            "skip={skip}"
        );
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn fullquote_escaped_ansi_c_suffix_preserves_raw_overlap_behavior() {
    struct EscapedSuffixHook {
        called: bool,
    }
    impl Hooks for EscapedSuffixHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            let input = b"echo $'saf\\'file'";
            assert_eq!(request.context.line, input);
            assert_eq!(request.context.point, input.len() - 7);
            assert_eq!(request.context.completion_type, CompletionType::Complete);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"saf'file".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    fullquote: true,
                    noquote: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let input = b"echo $'saf\\'file'".to_vec();
    let expected = b"echo $'saf'\\''file'X\\'file'";
    for skip in ["on", "off"] {
        let mut events = vec![TerminalEvent::Bytes(input.clone())];
        for _ in 0..7 {
            events.push(TerminalEvent::Bytes(b"\x1b[D".to_vec()));
        }
        events.push(TerminalEvent::Bytes(b"\tX\r".to_vec()));
        let terminal = MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.variables_mut()
            .insert("skip-completed-text".to_string(), skip.to_string());
        let mut hooks = EscapedSuffixHook { called: false };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.to_vec()),
            "skip={skip}"
        );
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn fullquote_ansi_c_completion_preserves_radix_escape_suffixes_at_cursor_splits() {
    struct RadixSuffixHook {
        input: Vec<u8>,
        point: usize,
        completion_type: CompletionType,
        called: bool,
    }
    impl Hooks for RadixSuffixHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.input);
            assert_eq!(request.context.point, self.point);
            assert_eq!(request.context.completion_type, self.completion_type);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"safe space-tail".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    fullquote: true,
                    noquote: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let cases = [
        (
            "hex before escape",
            b"\\x20tail".as_slice(),
            8,
            b"\\x20tail".as_slice(),
        ),
        (
            "hex after x",
            b"\\x20tail".as_slice(),
            6,
            b"20tail".as_slice(),
        ),
        (
            "hex after first digit",
            b"\\x20tail".as_slice(),
            5,
            b"0tail".as_slice(),
        ),
        (
            "hex after escape",
            b"\\x20tail".as_slice(),
            4,
            b"tail".as_slice(),
        ),
        (
            "octal before escape",
            b"\\040tail".as_slice(),
            8,
            b"\\040tail".as_slice(),
        ),
        (
            "octal after zero",
            b"\\040tail".as_slice(),
            6,
            b"40tail".as_slice(),
        ),
        (
            "octal after second digit",
            b"\\040tail".as_slice(),
            5,
            b"0tail".as_slice(),
        ),
        (
            "octal after escape",
            b"\\040tail".as_slice(),
            4,
            b"tail".as_slice(),
        ),
    ];
    let modes = [
        (
            "complete",
            CompletionType::Complete,
            b"\tX\r".as_slice(),
            false,
        ),
        (
            "menu-complete",
            CompletionType::MenuComplete,
            b"\x0fX\r".as_slice(),
            true,
        ),
    ];

    for skip in ["on", "off"] {
        for (mode, completion_type, completion_keys, menu_complete) in modes {
            for (case, escape, left_count, suffix) in cases {
                let mut input = b"echo $'safe".to_vec();
                input.extend_from_slice(escape);
                let mut events = vec![TerminalEvent::Bytes(input.clone())];
                for _ in 0..left_count {
                    events.push(TerminalEvent::Bytes(b"\x1b[D".to_vec()));
                }
                events.push(TerminalEvent::Bytes(completion_keys.to_vec()));
                let terminal = MemoryTerminal::with_events(events);
                let mut line = Editor::new(Config::default(), terminal, History::new());
                if menu_complete {
                    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
                }
                line.variables_mut()
                    .insert("skip-completed-text".to_string(), skip.to_string());
                let point = input.len() - left_count;
                let mut hooks = RadixSuffixHook {
                    input,
                    point,
                    completion_type,
                    called: false,
                };
                let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
                let mut expected = b"echo $'safe space-tail'X".to_vec();
                expected.extend_from_slice(suffix);
                assert_eq!(
                    result,
                    ReadlineResult::Line(expected),
                    "mode={mode} skip={skip} case={case}"
                );
                assert!(hooks.called);
                assert_eq!(
                    line.terminal()
                        .out
                        .bytes()
                        .filter(|byte| *byte == b'\x07')
                        .count(),
                    0,
                    "mode={mode} skip={skip} case={case}"
                );
            }
        }
    }
}

#[test]
fn fullquote_ansi_c_completion_preserves_radix_suffix_before_closer() {
    struct RadixCloserHook {
        input: Vec<u8>,
        point: usize,
        completion_type: CompletionType,
        called: bool,
    }
    impl Hooks for RadixCloserHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.input);
            assert_eq!(request.context.point, self.point);
            assert_eq!(request.context.completion_type, self.completion_type);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"safe space-tail".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    fullquote: true,
                    noquote: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let cases = [
        (
            "hex after x",
            b"echo $'safe\\x20tail'".as_slice(),
            7,
            b"20tail'".as_slice(),
        ),
        (
            "hex after escape",
            b"echo $'safe\\x20tail'".as_slice(),
            5,
            b"tail'".as_slice(),
        ),
        (
            "octal after zero",
            b"echo $'safe\\040tail'".as_slice(),
            7,
            b"40tail'".as_slice(),
        ),
        (
            "octal after escape",
            b"echo $'safe\\040tail'".as_slice(),
            5,
            b"tail'".as_slice(),
        ),
    ];
    let modes = [
        (
            "complete",
            CompletionType::Complete,
            b"\tX\r".as_slice(),
            false,
        ),
        (
            "menu-complete",
            CompletionType::MenuComplete,
            b"\x0fX\r".as_slice(),
            true,
        ),
    ];

    for skip in ["on", "off"] {
        for (mode, completion_type, completion_keys, menu_complete) in modes {
            for (case, input, left_count, suffix) in cases {
                let mut events = vec![TerminalEvent::Bytes(input.to_vec())];
                for _ in 0..left_count {
                    events.push(TerminalEvent::Bytes(b"\x1b[D".to_vec()));
                }
                events.push(TerminalEvent::Bytes(completion_keys.to_vec()));
                let terminal = MemoryTerminal::with_events(events);
                let mut line = Editor::new(Config::default(), terminal, History::new());
                if menu_complete {
                    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
                }
                line.variables_mut()
                    .insert("skip-completed-text".to_string(), skip.to_string());
                let mut hooks = RadixCloserHook {
                    input: input.to_vec(),
                    point: input.len() - left_count,
                    completion_type,
                    called: false,
                };
                let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
                let mut expected = b"echo $'safe space-tail'X".to_vec();
                expected.extend_from_slice(suffix);
                assert_eq!(
                    result,
                    ReadlineResult::Line(expected),
                    "mode={mode} skip={skip} case={case}"
                );
                assert!(hooks.called);
                assert_eq!(
                    line.terminal()
                        .out
                        .bytes()
                        .filter(|byte| *byte == b'\x07')
                        .count(),
                    0,
                    "mode={mode} skip={skip} case={case}"
                );
            }
        }
    }
}

#[test]
fn fullquote_ansi_c_completion_replaces_radix_prefix_boundaries() {
    struct RadixPrefixHook {
        input: Vec<u8>,
        called: bool,
    }
    impl Hooks for RadixPrefixHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.input);
            assert_eq!(request.context.point, self.input.len());
            assert_eq!(request.context.completion_type, CompletionType::Complete);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"safe space-tail".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    fullquote: true,
                    noquote: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let cases = [
        ("hex prefix", b"echo $'safe\\x".as_slice()),
        ("hex partial", b"echo $'safe\\x2".as_slice()),
        ("octal prefix", b"echo $'safe\\0".as_slice()),
        ("octal partial", b"echo $'safe\\04".as_slice()),
        ("hex escape", b"echo $'safe\\x20tail".as_slice()),
        ("octal escape", b"echo $'safe\\040tail".as_slice()),
    ];
    for skip in ["on", "off"] {
        for (case, input) in cases {
            let terminal = MemoryTerminal::with_events(vec![
                TerminalEvent::Bytes(input.to_vec()),
                TerminalEvent::Bytes(b"\tX\r".to_vec()),
            ]);
            let mut line = Editor::new(Config::default(), terminal, History::new());
            line.variables_mut()
                .insert("skip-completed-text".to_string(), skip.to_string());
            let mut hooks = RadixPrefixHook {
                input: input.to_vec(),
                called: false,
            };
            let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
            assert_eq!(
                result,
                ReadlineResult::Line(b"echo $'safe space-tail'X".to_vec()),
                "skip={skip} case={case}"
            );
            assert!(hooks.called);
            assert_eq!(
                line.terminal()
                    .out
                    .bytes()
                    .filter(|byte| *byte == b'\x07')
                    .count(),
                0,
                "skip={skip} case={case}"
            );
        }
    }
}

#[test]
fn fullquote_ansi_c_directory_completion_marks_radix_split_suffixes() {
    struct RadixDirectoryHook {
        input: Vec<u8>,
        point: usize,
        completion_type: CompletionType,
        candidate: Vec<u8>,
        called: bool,
    }
    impl Hooks for RadixDirectoryHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.input);
            assert_eq!(request.context.point, self.point);
            assert_eq!(request.context.completion_type, self.completion_type);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: self.candidate.clone(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    fullquote: true,
                    noquote: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("safe space-tail");
    std::fs::create_dir(&directory).unwrap();
    let prefix = root.path().join("safe").to_string_lossy().into_owned();
    let candidate = directory.to_string_lossy().into_owned().into_bytes();
    let cases = [
        (
            "hex after x",
            format!("echo $'{prefix}\\x20tail'").into_bytes(),
            7,
            b"20tail'".as_slice(),
        ),
        (
            "hex after escape",
            format!("echo $'{prefix}\\x20tail'").into_bytes(),
            5,
            b"tail'".as_slice(),
        ),
        (
            "octal after zero",
            format!("echo $'{prefix}\\040tail'").into_bytes(),
            7,
            b"40tail'".as_slice(),
        ),
        (
            "octal after escape",
            format!("echo $'{prefix}\\040tail'").into_bytes(),
            5,
            b"tail'".as_slice(),
        ),
    ];
    let modes = [
        (
            "complete",
            CompletionType::Complete,
            b"\tX\r".as_slice(),
            false,
        ),
        (
            "menu-complete",
            CompletionType::MenuComplete,
            b"\x0fX\r".as_slice(),
            true,
        ),
    ];

    for mark in ["on", "off"] {
        for skip in ["on", "off"] {
            for (mode, completion_type, completion_keys, menu_complete) in modes {
                for (case, input, left_count, suffix) in &cases {
                    let mut events = vec![TerminalEvent::Bytes(input.to_vec())];
                    for _ in 0..*left_count {
                        events.push(TerminalEvent::Bytes(b"\x1b[D".to_vec()));
                    }
                    events.push(TerminalEvent::Bytes(completion_keys.to_vec()));
                    let terminal = MemoryTerminal::with_events(events);
                    let mut line = Editor::new(Config::default(), terminal, History::new());
                    if menu_complete {
                        line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
                    }
                    line.variables_mut()
                        .insert("mark-directories".to_string(), mark.to_string());
                    line.variables_mut()
                        .insert("skip-completed-text".to_string(), skip.to_string());
                    let mut hooks = RadixDirectoryHook {
                        input: input.to_vec(),
                        point: input.len() - *left_count,
                        completion_type,
                        candidate: candidate.clone(),
                        called: false,
                    };
                    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
                    let mut expected = b"echo $'".to_vec();
                    expected.extend_from_slice(&candidate);
                    expected.push(b'\'');
                    if mark == "on" {
                        expected.push(b'/');
                    }
                    expected.push(b'X');
                    expected.extend_from_slice(suffix);
                    assert_eq!(
                        result,
                        ReadlineResult::Line(expected),
                        "mode={mode} mark={mark} skip={skip} case={case}"
                    );
                    assert!(hooks.called);
                    assert_eq!(
                        line.terminal()
                            .out
                            .bytes()
                            .filter(|byte| *byte == b'\x07')
                            .count(),
                        0,
                        "mode={mode} mark={mark} skip={skip} case={case}"
                    );
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn fullquote_ansi_c_symlink_directory_avoids_duplicate_typed_slash_at_radix_splits() {
    use std::os::unix::fs::symlink;

    struct SymlinkDirectoryHook {
        input: Vec<u8>,
        point: usize,
        completion_type: CompletionType,
        candidate: Vec<u8>,
        called: bool,
    }
    impl Hooks for SymlinkDirectoryHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.input);
            assert_eq!(request.context.point, self.point);
            assert_eq!(request.context.completion_type, self.completion_type);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: self.candidate.clone(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    fullquote: true,
                    noquote: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("target-dir")).unwrap();
    let directory = root.path().join("safe space-tail");
    symlink(root.path().join("target-dir"), &directory).unwrap();
    let candidate = directory.to_string_lossy().into_owned().into_bytes();
    let cases = [
        (
            "hex after escape",
            b"echo $'safe\\x20tail/'".to_vec(),
            6,
            b"tail/'".as_slice(),
        ),
        (
            "hex before slash",
            b"echo $'safe\\x20tail/'".to_vec(),
            2,
            b"/'".as_slice(),
        ),
        (
            "octal after escape",
            b"echo $'safe\\040tail/'".to_vec(),
            6,
            b"tail/'".as_slice(),
        ),
        (
            "octal before slash",
            b"echo $'safe\\040tail/'".to_vec(),
            2,
            b"/'".as_slice(),
        ),
    ];
    let modes = [
        (
            "complete",
            CompletionType::Complete,
            b"\tX\r".as_slice(),
            false,
        ),
        (
            "menu-complete",
            CompletionType::MenuComplete,
            b"\x0fX\r".as_slice(),
            true,
        ),
    ];

    for mark in ["on", "off"] {
        for symlinked in ["on", "off"] {
            for skip in ["on", "off"] {
                for (mode, completion_type, completion_keys, menu_complete) in modes {
                    for (case, input, left_count, suffix) in &cases {
                        let mut events = vec![TerminalEvent::Bytes(input.to_vec())];
                        for _ in 0..*left_count {
                            events.push(TerminalEvent::Bytes(b"\x1b[D".to_vec()));
                        }
                        events.push(TerminalEvent::Bytes(completion_keys.to_vec()));
                        let terminal = MemoryTerminal::with_events(events);
                        let mut line = Editor::new(Config::default(), terminal, History::new());
                        if menu_complete {
                            line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
                        }
                        line.variables_mut()
                            .insert("mark-directories".to_string(), mark.to_string());
                        line.variables_mut().insert(
                            "mark-symlinked-directories".to_string(),
                            symlinked.to_string(),
                        );
                        line.variables_mut()
                            .insert("skip-completed-text".to_string(), skip.to_string());
                        let mut hooks = SymlinkDirectoryHook {
                            input: input.to_vec(),
                            point: input.len() - *left_count,
                            completion_type,
                            candidate: candidate.clone(),
                            called: false,
                        };
                        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
                        let mut expected = b"echo $'".to_vec();
                        expected.extend_from_slice(&candidate);
                        expected.push(b'\'');
                        if mark == "on" && symlinked == "on" && suffix.first() != Some(&b'/') {
                            expected.push(b'/');
                        }
                        expected.push(b'X');
                        expected.extend_from_slice(suffix);
                        assert_eq!(
                            result,
                            ReadlineResult::Line(expected),
                            "mode={mode} mark={mark} symlinked={symlinked} skip={skip} case={case}"
                        );
                        assert!(hooks.called);
                        assert_eq!(
                            line.terminal()
                                .out
                                .bytes()
                                .filter(|byte| *byte == b'\x07')
                                .count(),
                            0,
                            "mode={mode} mark={mark} symlinked={symlinked} skip={skip} case={case}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn fullquote_ansi_c_menu_completion_replaces_radix_prefix_boundaries() {
    struct RadixMenuHook {
        input: Vec<u8>,
        called: bool,
    }
    impl Hooks for RadixMenuHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.input);
            assert_eq!(request.context.point, self.input.len());
            assert_eq!(
                request.context.completion_type,
                CompletionType::MenuComplete
            );
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"safe space-tail".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    fullquote: true,
                    noquote: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let cases = [
        b"echo $'safe\\x".as_slice(),
        b"echo $'safe\\x2".as_slice(),
        b"echo $'safe\\0".as_slice(),
        b"echo $'safe\\04".as_slice(),
        b"echo $'safe\\x20tail".as_slice(),
        b"echo $'safe\\040tail".as_slice(),
    ];
    for skip in ["on", "off"] {
        for input in cases {
            let terminal = MemoryTerminal::with_events(vec![
                TerminalEvent::Bytes(input.to_vec()),
                TerminalEvent::Bytes(b"\x0fX\r".to_vec()),
            ]);
            let mut line = Editor::new(Config::default(), terminal, History::new());
            line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
            line.variables_mut()
                .insert("skip-completed-text".to_string(), skip.to_string());
            let mut hooks = RadixMenuHook {
                input: input.to_vec(),
                called: false,
            };
            let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
            assert_eq!(
                result,
                ReadlineResult::Line(b"echo $'safe space-tail'X".to_vec()),
                "skip={skip} input={input:?}"
            );
            assert!(hooks.called);
            assert_eq!(
                line.terminal()
                    .out
                    .bytes()
                    .filter(|byte| *byte == b'\x07')
                    .count(),
                0,
                "skip={skip} input={input:?}"
            );
        }
    }
}

#[test]
fn nospace_ansi_c_completion_respects_skip_completed_text_suffix() {
    struct SkipHook {
        noquote: bool,
        nospace: bool,
        line: Vec<u8>,
        called: bool,
    }
    impl Hooks for SkipHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.line);
            assert_eq!(request.context.point, self.line.len() - 5);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"saf file".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    noquote: self.noquote,
                    nospace: self.nospace,
                    ..Default::default()
                },
            })
        }
    }

    let input = b"echo $'saf file'".to_vec();
    for nospace in [false, true] {
        for (skip, noquote, expected) in [
            ("on", true, b"echo $'saf fileX'".as_slice()),
            ("off", true, b"echo $'saf fileXfile'".as_slice()),
            ("on", false, b"echo $'saf file'X".as_slice()),
            ("off", false, b"echo $'saf file'Xfile'".as_slice()),
        ] {
            let mut events = vec![TerminalEvent::Bytes(input.clone())];
            for _ in 0..5 {
                events.push(TerminalEvent::Bytes(b"\x1b[D".to_vec()));
            }
            events.push(TerminalEvent::Bytes(b"\tX\r".to_vec()));
            let terminal = MemoryTerminal::with_events(events);
            let mut line = Editor::new(Config::default(), terminal, History::new());
            line.variables_mut()
                .insert("skip-completed-text".to_string(), skip.to_string());
            let mut hooks = SkipHook {
                noquote,
                nospace,
                line: input.clone(),
                called: false,
            };
            let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
            let expected = if !nospace && skip == "on" && !noquote {
                b"echo $'saf file' X".as_slice()
            } else {
                expected
            };
            assert_eq!(
                result,
                ReadlineResult::Line(expected.to_vec()),
                "skip={skip} noquote={noquote} nospace={nospace}"
            );
            assert!(hooks.called);
            assert_eq!(
                line.terminal()
                    .out
                    .bytes()
                    .filter(|byte| *byte == b'\x07')
                    .count(),
                0
            );
        }
    }
}

#[test]
fn nospace_ansi_c_filename_completion_suppresses_trailing_space() {
    struct FilenameHook {
        candidate: Vec<u8>,
        line: Vec<u8>,
        called: bool,
    }
    impl Hooks for FilenameHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.line);
            assert_eq!(request.context.point, self.line.len());
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: self.candidate.clone(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let root = tempfile::tempdir().unwrap();
    let file_path = root.path().join("saf file");
    std::fs::write(&file_path, b"").unwrap();
    let directory_path = root.path().join("saf dir");
    std::fs::create_dir(&directory_path).unwrap();
    let prefix = root.path().join("saf");
    let input = format!("echo $'{}", prefix.display()).into_bytes();

    for (candidate_path, mark_directories, suffix) in [
        (&file_path, "on", ""),
        (&directory_path, "on", "/"),
        (&directory_path, "off", ""),
    ] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(input.clone()),
            TerminalEvent::Bytes(b"\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.variables_mut()
            .insert("mark-directories".to_string(), mark_directories.to_string());
        let mut hooks = FilenameHook {
            candidate: candidate_path.to_string_lossy().into_owned().into_bytes(),
            line: input.clone(),
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        let expected = format!("echo $'{}'{suffix}X", candidate_path.display());
        assert_eq!(result, ReadlineResult::Line(expected.into_bytes()));
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn nospace_ansi_c_filename_completion_preserves_existing_closer() {
    struct FilenameHook {
        candidate: Vec<u8>,
        line: Vec<u8>,
        called: bool,
    }
    impl Hooks for FilenameHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.line);
            assert_eq!(request.context.point, self.line.len() - 1);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: self.candidate.clone(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let root = tempfile::tempdir().unwrap();
    let file_path = root.path().join("saf file");
    std::fs::write(&file_path, b"").unwrap();
    let directory_path = root.path().join("saf dir");
    std::fs::create_dir(&directory_path).unwrap();
    let prefix = root.path().join("saf");
    let input = format!("echo $'{}'", prefix.display()).into_bytes();

    for (candidate_path, mark_directories, suffix) in [
        (&file_path, "on", ""),
        (&directory_path, "on", "/"),
        (&directory_path, "off", ""),
    ] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(input.clone()),
            TerminalEvent::Bytes(b"\x1b[D\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.variables_mut()
            .insert("mark-directories".to_string(), mark_directories.to_string());
        let mut hooks = FilenameHook {
            candidate: candidate_path.to_string_lossy().into_owned().into_bytes(),
            line: input.clone(),
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        let expected = format!("echo $'{}'{suffix}X", candidate_path.display());
        assert_eq!(result, ReadlineResult::Line(expected.into_bytes()));
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn noquote_nospace_ansi_c_directory_completion_preserves_closer() {
    struct DirectoryHook {
        candidate: Vec<u8>,
        line: Vec<u8>,
        called: bool,
    }
    impl Hooks for DirectoryHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.line);
            assert_eq!(request.context.point, self.line.len() - 1);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: self.candidate.clone(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    noquote: true,
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let root = tempfile::tempdir().unwrap();
    let directory_path = root.path().join("saf dir");
    std::fs::create_dir(&directory_path).unwrap();
    let prefix = root.path().join("saf");
    let input = format!("echo $'{}'", prefix.display()).into_bytes();
    let candidate = directory_path.to_string_lossy().into_owned().into_bytes();

    for (mark_directories, suffix) in [("on", "/"), ("off", "")] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(input.clone()),
            TerminalEvent::Bytes(b"\x1b[D\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.variables_mut()
            .insert("mark-directories".to_string(), mark_directories.to_string());
        let mut hooks = DirectoryHook {
            candidate: candidate.clone(),
            line: input.clone(),
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        let expected = format!("echo $'{}{suffix}X'", directory_path.display());
        assert_eq!(result, ReadlineResult::Line(expected.into_bytes()));
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn noquote_ansi_c_directory_completion_stays_open() {
    struct DirectoryHook {
        candidate: Vec<u8>,
        line: Vec<u8>,
        called: bool,
    }
    impl Hooks for DirectoryHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.line);
            assert_eq!(request.context.point, self.line.len());
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: self.candidate.clone(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    noquote: true,
                    ..Default::default()
                },
            })
        }
    }

    let root = tempfile::tempdir().unwrap();
    let candidate_path = root.path().join("saf dir");
    std::fs::create_dir(&candidate_path).unwrap();
    let prefix = root.path().join("saf");
    let input = format!("echo $'{}", prefix.display()).into_bytes();
    let candidate = candidate_path.to_string_lossy().into_owned().into_bytes();

    for mark_directories in ["on", "off"] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(input.clone()),
            TerminalEvent::Bytes(b"\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.variables_mut()
            .insert("mark-directories".to_string(), mark_directories.to_string());
        let mut hooks = DirectoryHook {
            candidate: candidate.clone(),
            line: input.clone(),
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        let slash = if mark_directories == "on" { "/" } else { "" };
        let expected = format!("echo $'{}{slash}X", candidate_path.display());
        assert_eq!(result, ReadlineResult::Line(expected.into_bytes()));
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn unquoted_completion_preserves_existing_ansi_c_closer() {
    struct ContextHook {
        options: CompletionOptions,
        called: bool,
    }
    impl Hooks for ContextHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, b"echo $'saf'");
            assert_eq!(request.context.point, request.context.line.len() - 1);
            assert_eq!(request.context.word, b"saf");
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"saf file".to_vec(),
                    display: None,
                }],
                options: self.options.clone(),
            })
        }
    }

    for options in [
        CompletionOptions::default(),
        CompletionOptions {
            filenames: true,
            noquote: true,
            ..Default::default()
        },
    ] {
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(
            b"echo $'saf'\x1b[D\tX\r".to_vec(),
        )]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let mut hooks = ContextHook {
            options,
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(result, ReadlineResult::Line(b"echo $'saf fileX'".to_vec()));
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn plain_application_completion_preserves_ansi_c_quote_context() {
    struct PlainHook {
        called: bool,
    }
    impl Hooks for PlainHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, b"echo $'saf");
            assert_eq!(request.context.point, request.context.line.len());
            assert_eq!(request.context.word, b"saf");
            assert_eq!(request.context.completion_type, CompletionType::Complete);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"saf file".to_vec(),
                    display: None,
                }],
                options: CompletionOptions::default(),
            })
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"echo $'saf".to_vec()),
        TerminalEvent::Bytes(b"\tX\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let mut hooks = PlainHook { called: false };
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"echo $'saf file' X".to_vec()));
    assert!(hooks.called);
    assert_eq!(
        line.terminal()
            .out
            .bytes()
            .filter(|byte| *byte == b'\x07')
            .count(),
        0
    );
}

#[test]
fn noquote_filename_completion_preserves_ansi_c_quote_context() {
    struct NoquoteHook {
        candidate: Vec<u8>,
        called: bool,
    }
    impl Hooks for NoquoteHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, b"echo $'saf");
            assert_eq!(request.context.point, request.context.line.len());
            assert_eq!(request.context.word, b"saf");
            assert_eq!(request.context.completion_type, CompletionType::Complete);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: self.candidate.clone(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    noquote: true,
                    ..Default::default()
                },
            })
        }
    }

    for (candidate, expected) in [
        (b"saf file".as_slice(), b"echo $'saf file' X".as_slice()),
        (b"saf'file".as_slice(), b"echo $'saf'file' X".as_slice()),
    ] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"echo $'saf".to_vec()),
            TerminalEvent::Bytes(b"\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let mut hooks = NoquoteHook {
            candidate: candidate.to_vec(),
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(result, ReadlineResult::Line(expected.to_vec()));
        assert!(hooks.called);
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            0
        );
    }
}

#[test]
fn reopened_plusdirs_supplements_nonempty_full_word_candidates() {
    let root = tempfile::tempdir().unwrap();
    let segment_dir = root.path().join("segment");
    std::fs::create_dir(&segment_dir).unwrap();
    std::fs::create_dir(segment_dir.join("eagle-dir-one")).unwrap();
    std::fs::create_dir(segment_dir.join("eagle-dir-two")).unwrap();
    let segment = format!("{}/e", segment_dir.display());
    let input = format!("echo 'sa'f'{segment}").into_bytes();
    let word = format!("saf{segment}").into_bytes();
    let application_candidate = format!("saf{}/eagle-app", segment_dir.display()).into_bytes();

    struct MixedPlusdirsHook {
        line: Vec<u8>,
        word: Vec<u8>,
        candidate: Vec<u8>,
        completion_type: CompletionType,
        called: bool,
    }

    impl Hooks for MixedPlusdirsHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.line);
            assert_eq!(request.context.point, self.line.len());
            assert_eq!(request.context.word_start, 5);
            assert_eq!(request.context.word_end, self.line.len());
            assert_eq!(request.context.word, self.word);
            assert_eq!(request.context.completion_type, self.completion_type);
            self.called = true;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: self.candidate.clone(),
                    display: None,
                }],
                options: CompletionOptions {
                    plusdirs: true,
                    ..Default::default()
                },
            })
        }
    }

    for (completion_type, keys, expected_line, expected_bells) in [
        (
            CompletionType::Complete,
            b"\tX\r".as_slice(),
            [input.as_slice(), b"X"].concat(),
            1,
        ),
        (
            CompletionType::PossibleCompletions,
            b"\x0f\r".as_slice(),
            input.clone(),
            0,
        ),
    ] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(input.clone()),
            TerminalEvent::Bytes(keys.to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        if completion_type == CompletionType::PossibleCompletions {
            line.load_inputrc_str("\"\\C-o\": possible-completions")
                .unwrap();
        }
        let mut hooks = MixedPlusdirsHook {
            line: input.clone(),
            word: word.clone(),
            candidate: application_candidate.clone(),
            completion_type,
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected_line),
            "{completion_type:?}"
        );
        assert!(hooks.called, "{completion_type:?}");
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            expected_bells,
            "{completion_type:?}"
        );
        if completion_type == CompletionType::PossibleCompletions {
            let output = &line.terminal().out;
            for candidate in ["eagle-app", "eagle-dir-one/", "eagle-dir-two/"] {
                assert!(
                    output.contains(candidate),
                    "missing {candidate:?} in {output:?}"
                );
            }
        }
    }
}

#[test]
fn reopened_segment_scopes_builtin_directory_fallbacks() {
    let root = tempfile::tempdir().unwrap();
    let cases: [(&str, CompletionOptions, &[&str], &str); 3] = [
        (
            "dirnames",
            CompletionOptions {
                dirnames: true,
                ..Default::default()
            },
            &["safe-one", "safe-two", "eagle-dir-one", "eagle-dir-two"],
            "agle-dir-X",
        ),
        (
            "plusdirs",
            CompletionOptions {
                plusdirs: true,
                ..Default::default()
            },
            &["eagle-dir-one", "eagle-dir-two", "safe-dir-one"],
            "agle-dir-X",
        ),
        (
            "default-plusdirs",
            CompletionOptions {
                default: true,
                plusdirs: true,
                ..Default::default()
            },
            &["safe-one", "safe-two", "eagle-one", "eagle-two", "safe-dir"],
            "agle-X",
        ),
    ];

    struct EmptyApplicationHook {
        options: CompletionOptions,
        line: Vec<u8>,
        word: Vec<u8>,
        called: bool,
    }

    impl Hooks for EmptyApplicationHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line, self.line);
            assert_eq!(request.context.point, self.line.len());
            assert_eq!(request.context.word_start, 5);
            assert_eq!(request.context.word_end, self.line.len());
            assert_eq!(request.context.word, self.word);
            self.called = true;
            Some(CompletionResponse {
                candidates: Vec::new(),
                options: self.options.clone(),
            })
        }
    }

    for (kind, options, names, expected_suffix) in cases {
        let cwd = root.path().join(kind);
        std::fs::create_dir(&cwd).unwrap();
        for name in names {
            let path = cwd.join(name);
            if name.contains("-dir-") || name.ends_with("-dir") {
                std::fs::create_dir(path).unwrap();
            } else {
                std::fs::write(path, b"").unwrap();
            }
        }

        let segment = format!("{}/e", cwd.display());
        let input = format!("echo 'sa'f'{segment}").into_bytes();
        let expected = format!("echo 'sa'f'{segment}{expected_suffix}");
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(input.clone()),
            TerminalEvent::Bytes(b"\tX\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        let mut hooks = EmptyApplicationHook {
            options,
            line: input.clone(),
            word: format!("saf{segment}").into_bytes(),
            called: false,
        };
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.into_bytes()),
            "{kind}"
        );
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            1,
            "{kind} should report ambiguous built-in fallback"
        );
        assert!(hooks.called, "{kind}");
    }
}
