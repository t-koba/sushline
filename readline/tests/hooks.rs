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
