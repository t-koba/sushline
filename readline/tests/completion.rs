mod common;

use common::MemoryTerminal;
use readline::{
    CompletionAction, CompletionCandidate, CompletionOptions, CompletionRequest,
    CompletionResponse, CompletionType, Config, Editor, History, Hooks, Prompt, QuoteContext,
    ReadlineResult, TerminalEvent,
};

#[test]
fn application_default_filename_fallback_uses_reopened_segment_scope() {
    let dir = tempfile::tempdir().expect("completion fixture directory");
    for name in ["safe-one", "safe-two", "eagle-one", "eagle-two"] {
        std::fs::write(dir.path().join(name), "").expect("write completion fixture");
    }

    struct DefaultFallback {
        contexts: Vec<readline::CompletionContext>,
    }

    impl Hooks for DefaultFallback {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            self.contexts.push(request.context);
            Some(CompletionResponse {
                candidates: Vec::new(),
                options: CompletionOptions {
                    default: true,
                    ..Default::default()
                },
            })
        }

        fn default_complete(&mut self, _: &CompletionRequest) -> Option<CompletionResponse> {
            panic!("default_complete is not requested without bashdefault")
        }
    }

    for command in ["complete", "complete-filename"] {
        let input = format!("echo 'safe'f'{}/e", dir.path().display());
        let expected = format!("echo 'safe'f'{}/eagle-X", dir.path().display());
        let mut events = input.as_bytes().to_vec();
        events.extend_from_slice(b"\x0fX\r");
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(events)]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(&format!("\"\\C-o\": {command}"))
            .unwrap();
        let mut hooks = DefaultFallback {
            contexts: Vec::new(),
        };

        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();

        assert_eq!(
            result,
            ReadlineResult::Line(expected.into_bytes()),
            "{command}"
        );
        assert_eq!(
            line.terminal()
                .out
                .bytes()
                .filter(|byte| *byte == b'\x07')
                .count(),
            1,
            "{command} should report the ambiguous filename fallback"
        );
        assert_eq!(
            hooks.contexts,
            vec![readline::CompletionContext {
                line: input.as_bytes().to_vec(),
                point: input.len(),
                word_start: 5,
                word_end: input.len(),
                word: format!("safef{}/e", dir.path().display()).into_bytes(),
                key: vec![0x0f],
                completion_type: if command == "complete" {
                    CompletionType::Complete
                } else {
                    CompletionType::Filename
                },
            }],
            "{command} must receive the full-word completion context"
        );
    }

    let input = format!("echo '{}/safe", dir.path().display());
    let expected = format!("echo '{}/safe-X", dir.path().display());
    let mut events = input.as_bytes().to_vec();
    events.extend_from_slice(b"\x0fX\r");
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(events)]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let mut hooks = DefaultFallback {
        contexts: Vec::new(),
    };

    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();

    assert_eq!(result, ReadlineResult::Line(expected.into_bytes()));
    assert_eq!(
        line.terminal()
            .out
            .bytes()
            .filter(|byte| *byte == b'\x07')
            .count(),
        1
    );
    assert_eq!(
        hooks.contexts[0].word,
        format!("{}/safe", dir.path().display()).into_bytes()
    );
}

#[test]
fn application_default_complete_receives_full_reopened_word_context() {
    struct DefaultCompleteResponse {
        complete_context: Option<readline::CompletionContext>,
        default_context: Option<readline::CompletionContext>,
    }

    impl Hooks for DefaultCompleteResponse {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            self.complete_context = Some(request.context);
            Some(CompletionResponse {
                candidates: Vec::new(),
                options: CompletionOptions {
                    default: true,
                    bashdefault: true,
                    ..Default::default()
                },
            })
        }

        fn default_complete(&mut self, request: &CompletionRequest) -> Option<CompletionResponse> {
            self.default_context = Some(request.context.clone());
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"application-result".to_vec(),
                    display: None,
                }],
                options: CompletionOptions::default(),
            })
        }
    }

    let input = b"echo 'sa'f'e";
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(
        [input.as_slice(), b"\x0f\r"].concat(),
    )]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let mut hooks = DefaultCompleteResponse {
        complete_context: None,
        default_context: None,
    };

    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();

    assert_eq!(
        result,
        ReadlineResult::Line(b"echo 'application-result' ".to_vec())
    );
    for context in [hooks.complete_context, hooks.default_context] {
        assert_eq!(
            context,
            Some(readline::CompletionContext {
                line: input.to_vec(),
                point: input.len(),
                word_start: 5,
                word_end: input.len(),
                word: b"safe".to_vec(),
                key: vec![0x0f],
                completion_type: CompletionType::Complete,
            })
        );
    }
}

#[derive(Default)]
struct StaticCompletion {
    expected_type: Option<CompletionType>,
    expected_word: Option<&'static str>,
    filenames: bool,
}

impl Hooks for StaticCompletion {
    fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
        if let Some(expected_type) = self.expected_type {
            assert_eq!(request.context.completion_type, expected_type);
        }
        if let Some(expected_word) = self.expected_word {
            assert_eq!(request.context.word.as_slice(), expected_word.as_bytes());
        }
        Some(CompletionResponse {
            candidates: vec![
                CompletionCandidate {
                    replacement: b"alpha".to_vec(),
                    display: None,
                },
                CompletionCandidate {
                    replacement: b"beta".to_vec(),
                    display: None,
                },
            ],
            options: CompletionOptions {
                filenames: self.filenames,
                ..Default::default()
            },
        })
    }
}

struct OptionCompletion {
    options: CompletionOptions,
    candidates: Vec<CompletionCandidate>,
}

impl Hooks for OptionCompletion {
    fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
        Some(CompletionResponse {
            candidates: self.candidates.clone(),
            options: self.options.clone(),
        })
    }
}

struct AnsiCQuoteCompletion {
    candidates: Vec<Vec<u8>>,
    closes_quote: bool,
}

impl Hooks for AnsiCQuoteCompletion {
    fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
        Some(CompletionResponse {
            candidates: self
                .candidates
                .iter()
                .map(|replacement| CompletionCandidate {
                    replacement: replacement.clone(),
                    display: None,
                })
                .collect(),
            options: CompletionOptions {
                filenames: true,
                ..Default::default()
            },
        })
    }

    fn quote_completion(&mut self, context: QuoteContext<'_>) -> Option<Vec<u8>> {
        assert_eq!(context.quote, Some('\''));
        let mut quoted = b"$'".to_vec();
        quoted.extend_from_slice(context.value);
        if self.closes_quote {
            quoted.push(b'\'');
        }
        Some(quoted)
    }
}

struct ThreeCompletion {
    options: CompletionOptions,
}

impl Hooks for ThreeCompletion {
    fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
        Some(CompletionResponse {
            candidates: vec![
                CompletionCandidate {
                    replacement: b"alpha".to_vec(),
                    display: None,
                },
                CompletionCandidate {
                    replacement: b"beta".to_vec(),
                    display: None,
                },
                CompletionCandidate {
                    replacement: b"gamma".to_vec(),
                    display: None,
                },
            ],
            options: self.options.clone(),
        })
    }
}

#[test]
fn menu_complete_uses_completion_hook() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = StaticCompletion {
        expected_type: Some(CompletionType::MenuComplete),
        ..Default::default()
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("alpha ".as_bytes().to_vec()));
}

#[test]
fn completion_query_ctrl_c_interrupts_and_clears_transient_prompt() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\x03".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = StaticCompletion::default();
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set completion-query-items 1")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal().out.contains("> ^C\r\n"));
    assert!(
        !line
            .terminal()
            .out
            .contains("Display all 2 possibilities?^C")
    );
}

#[test]
fn menu_complete_replaces_previous_candidate() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = StaticCompletion {
        expected_type: Some(CompletionType::MenuComplete),
        ..Default::default()
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("beta ".as_bytes().to_vec()));
}

#[test]
fn menu_complete_wraps_to_common_prefix_at_end() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = StaticCompletion {
        expected_type: Some(CompletionType::MenuComplete),
        ..Default::default()
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(Vec::new()));
}

#[test]
fn menu_complete_keeps_state_after_wrapping_to_common_prefix() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = StaticCompletion {
        expected_type: Some(CompletionType::MenuComplete),
        ..Default::default()
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("alpha ".as_bytes().to_vec()));
}

#[test]
fn menu_complete_reuses_initial_completion_response() {
    struct WordDependentCompletion {
        calls: usize,
    }
    impl Hooks for WordDependentCompletion {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            self.calls += 1;
            assert_eq!(request.context.word.as_slice(), b"a");
            Some(CompletionResponse {
                candidates: vec![
                    CompletionCandidate {
                        replacement: b"alpha".to_vec(),
                        display: None,
                    },
                    CompletionCandidate {
                        replacement: b"alpine".to_vec(),
                        display: None,
                    },
                ],
                options: CompletionOptions {
                    nospace: true,
                    ..Default::default()
                },
            })
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = WordDependentCompletion { calls: 0 };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("alpine".as_bytes().to_vec()));
    assert_eq!(hooks.calls, 1);
}

#[test]
fn menu_complete_backward_and_old_menu_complete_share_cycle_behavior() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x10]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ThreeCompletion {
        options: CompletionOptions {
            nospace: true,
            ..Default::default()
        },
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete\n\"\\C-p\": menu-complete-backward")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ThreeCompletion {
        options: CompletionOptions {
            nospace: true,
            ..Default::default()
        },
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": old-menu-complete")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("beta".as_bytes().to_vec()));
}

#[test]
fn shift_tab_sequence_can_bind_to_menu_complete_backward() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ThreeCompletion {
        options: CompletionOptions {
            nospace: true,
            ..Default::default()
        },
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\e[Z\": menu-complete-backward")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("gamma".as_bytes().to_vec()));
}

#[test]
fn menu_complete_numeric_argument_moves_within_saved_candidates() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b2".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ThreeCompletion {
        options: CompletionOptions {
            nospace: true,
            ..Default::default()
        },
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("beta".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\x1b2".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ThreeCompletion {
        options: CompletionOptions {
            nospace: true,
            ..Default::default()
        },
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("gamma".as_bytes().to_vec()));
}

#[test]
fn menu_completion_numeric_exhaustion_rings_and_old_menu_keeps_prefix() {
    type Case = (&'static str, &'static [u8], &'static str);
    let cases: &[Case] = &[
        ("menu-complete", b"\x1b3", "alpX"),
        ("menu-complete", b"\x1b0", "alpX"),
        ("menu-complete-backward", b"\x1b3", "alpX"),
        ("menu-complete-backward", b"\x1b0", "alpX"),
        ("old-menu-complete", b"\x1b3", "alX"),
        ("old-menu-complete", b"\x1b0", "alX"),
    ];
    for (command, argument, expected) in cases {
        let mut keys = b"al".to_vec();
        keys.extend_from_slice(argument);
        keys.extend_from_slice(b"\x0fX\r");
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys)]);
        let mut hooks = OptionCompletion {
            options: CompletionOptions {
                nospace: true,
                ..Default::default()
            },
            candidates: vec![
                CompletionCandidate {
                    replacement: b"alpha".to_vec(),
                    display: None,
                },
                CompletionCandidate {
                    replacement: b"alpine".to_vec(),
                    display: None,
                },
            ],
        };
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(&format!("\"\\C-o\": {command}"))
            .unwrap();
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.as_bytes().to_vec()),
            "{command}"
        );
        assert_eq!(line.terminal().out.matches('\u{7}').count(), 1, "{command}");
    }
}

#[test]
fn repeated_numeric_menu_prefix_exhaustion_rings_once_per_action() {
    let cases: &[(&str, &[u8], &str)] = &[
        ("menu-complete", b"\x1b3", "alpX"),
        ("menu-complete", b"\x1b0", "alpX"),
        ("menu-complete-backward", b"\x1b3", "alpX"),
        ("menu-complete-backward", b"\x1b0", "alpX"),
        ("old-menu-complete", b"\x1b3", "alX"),
        ("old-menu-complete", b"\x1b0", "alX"),
    ];
    for (command, argument, expected) in cases {
        let mut keys = b"al".to_vec();
        keys.extend_from_slice(argument);
        keys.push(0x0f);
        keys.extend_from_slice(argument);
        keys.extend_from_slice(b"\x0fX\r");
        let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(keys)]);
        let mut hooks = OptionCompletion {
            options: CompletionOptions {
                nospace: true,
                ..Default::default()
            },
            candidates: vec![
                CompletionCandidate {
                    replacement: b"alpha".to_vec(),
                    display: None,
                },
                CompletionCandidate {
                    replacement: b"alpine".to_vec(),
                    display: None,
                },
            ],
        };
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(&format!("\"\\C-o\": {command}"))
            .unwrap();
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(expected.as_bytes().to_vec()),
            "{command}"
        );
        assert_eq!(line.terminal().out.matches('\u{7}').count(), 2, "{command}");
    }
}

#[test]
fn menu_complete_display_prefix_is_first_menu_choice() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            nospace: true,
            ..Default::default()
        },
        candidates: vec![
            CompletionCandidate {
                replacement: b"alpha".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"alpine".to_vec(),
                display: None,
            },
        ],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete\nset menu-complete-display-prefix on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("alp".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            nospace: true,
            ..Default::default()
        },
        candidates: vec![
            CompletionCandidate {
                replacement: b"alpha".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"alpine".to_vec(),
                display: None,
            },
        ],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete\nset menu-complete-display-prefix on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("alpha".as_bytes().to_vec()));
}

#[test]
fn menu_complete_show_all_if_ambiguous_displays_matches_on_first_menu() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = StaticCompletion {
        expected_type: Some(CompletionType::MenuComplete),
        ..Default::default()
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete\nset show-all-if-ambiguous on")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert!(line.terminal().out.contains("alpha"));
    assert!(line.terminal().out.contains("beta"));
}

#[test]
fn menu_complete_single_match_completes_without_starting_menu_state() {
    struct SingleCompletion {
        calls: usize,
    }

    impl Hooks for SingleCompletion {
        fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
            self.calls += 1;
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"alpha".to_vec(),
                    display: None,
                }],
                options: CompletionOptions::default(),
            })
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = SingleCompletion { calls: 0 };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("alpha alpha ".as_bytes().to_vec())
    );
    assert_eq!(hooks.calls, 2);
}

#[test]
fn menu_complete_display_only_does_not_start_menu_state() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            action: Some(CompletionAction::DisplayOnly),
            ..Default::default()
        },
        candidates: vec![CompletionCandidate {
            replacement: b"alpha".to_vec(),
            display: None,
        }],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("a".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("alpha"));
}

#[test]
fn menu_complete_uses_programmable_filter_replacement_and_append_options() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            filter_suffix: Some(b".rs".to_vec()),
            replacement_prefix: Some(b"--".to_vec()),
            nosort: true,
            nospace: true,
            ..Default::default()
        },
        candidates: vec![
            CompletionCandidate {
                replacement: b"alpha.rs".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"alpha.txt".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"alpine.rs".to_vec(),
                display: None,
            },
        ],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("--alpine.rs".as_bytes().to_vec())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            append_character: Some(':'),
            ..Default::default()
        },
        candidates: vec![
            CompletionCandidate {
                replacement: b"alpha".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"beta".to_vec(),
                display: None,
            },
        ],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("alpha:".as_bytes().to_vec()));
}

#[test]
fn menu_completion_respects_explicit_append_character_inside_quotes() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"echo $'a'".to_vec()),
        TerminalEvent::Bytes(vec![0x02]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            filenames: true,
            append_character: Some(':'),
            ..Default::default()
        },
        candidates: vec![
            CompletionCandidate {
                replacement: b"alpha".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"alpine".to_vec(),
                display: None,
            },
        ],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"echo $'alpha':".to_vec()));
}

#[test]
fn completion_sort_removes_duplicate_replacements_even_when_display_differs() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions::default(),
        candidates: vec![
            CompletionCandidate {
                replacement: b"same".to_vec(),
                display: Some("zeta".to_string()),
            },
            CompletionCandidate {
                replacement: b"other".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"same".to_vec(),
                display: Some("alpha".to_string()),
            },
        ],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": insert-completions")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"same other ".to_vec()));
}

#[test]
fn completion_nosort_preserves_first_occurrence_but_still_removes_duplicates() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            nosort: true,
            ..Default::default()
        },
        candidates: vec![
            CompletionCandidate {
                replacement: b"beta".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"alpha".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"beta".to_vec(),
                display: None,
            },
        ],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": insert-completions")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"beta alpha ".to_vec()));
}

#[test]
fn menu_complete_marks_and_quotes_filename_candidates() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("alpha dir")).unwrap();
    std::fs::write(dir.path().join("alpha file"), "").unwrap();
    let word = format!("{}/alpha", dir.path().display());
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    let ReadlineResult::Line(line) = result else {
        panic!("expected accepted line");
    };
    assert!(
        line.ends_with(b"alpha\\ dir/") || line.ends_with(b"alpha\\ file "),
        "{line:?}"
    );
}

#[test]
fn menu_completion_preserves_existing_ansi_c_quote_from_quote_hook() {
    for skip_completed_text in ["on", "off"] {
        for closes_quote in [false, true] {
            for cursor_before_quote in [false, true] {
                for actions in [1, 2] {
                    let mut events = vec![TerminalEvent::Bytes(b"echo $'al'".to_vec())];
                    if cursor_before_quote {
                        events.push(TerminalEvent::Bytes(vec![0x02]));
                    }
                    events.extend((0..actions).map(|_| TerminalEvent::Bytes(vec![0x0f])));
                    events.push(TerminalEvent::Bytes(b"\r".to_vec()));
                    let terminal = MemoryTerminal::with_events(events);
                    let mut hooks = AnsiCQuoteCompletion {
                        candidates: vec![b"alpha".to_vec(), b"alpine".to_vec()],
                        closes_quote,
                    };
                    let mut line = Editor::new(Config::default(), terminal, History::new());
                    line.load_inputrc_str(&format!(
                        "\"\\C-o\": menu-complete\nset skip-completed-text {skip_completed_text}"
                    ))
                    .unwrap();
                    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
                    let candidate = if actions == 1 {
                        b"alpha".as_slice()
                    } else {
                        b"alpine".as_slice()
                    };
                    let mut expected = b"echo $'".to_vec();
                    expected.extend_from_slice(candidate);
                    expected.extend_from_slice(b"'");
                    assert_eq!(
                        result,
                        ReadlineResult::Line(expected),
                        "skip_completed_text={skip_completed_text}, closes_quote={closes_quote}, cursor_before_quote={cursor_before_quote}, actions={actions}"
                    );
                }
            }
        }
    }
}

#[test]
fn menu_completion_preserves_ansi_c_tail_for_case_insensitive_prefix_match() {
    struct CaseInsensitiveCompletion;

    impl Hooks for CaseInsensitiveCompletion {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.word.as_slice(), b"safe-al");
            Some(CompletionResponse {
                candidates: [b"safe-Alpha".as_slice(), b"safe-Alpine".as_slice()]
                    .into_iter()
                    .map(|replacement| CompletionCandidate {
                        replacement: replacement.to_vec(),
                        display: None,
                    })
                    .collect(),
                options: CompletionOptions {
                    filenames: true,
                    ..Default::default()
                },
            })
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"echo $'safe-alpha'".to_vec()),
        TerminalEvent::Bytes(vec![0x02, 0x02, 0x02, 0x02]),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = CaseInsensitiveCompletion;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(
        "\"\\C-o\": menu-complete\nset completion-ignore-case on\nset skip-completed-text on",
    )
    .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line(b"echo $'safe-Alphapha'".to_vec())
    );
}

#[test]
fn single_completion_preserves_existing_ansi_c_quote_from_quote_hook() {
    for skip_completed_text in ["on", "off"] {
        for closes_quote in [false, true] {
            for cursor_before_quote in [false, true] {
                let mut events = vec![TerminalEvent::Bytes(b"echo $'al'".to_vec())];
                if cursor_before_quote {
                    events.push(TerminalEvent::Bytes(vec![0x02]));
                }
                events.extend([
                    TerminalEvent::Bytes(vec![0x0f]),
                    TerminalEvent::Bytes(b"\r".to_vec()),
                ]);
                let terminal = MemoryTerminal::with_events(events);
                let mut hooks = AnsiCQuoteCompletion {
                    candidates: vec![b"alpha".to_vec()],
                    closes_quote,
                };
                let mut line = Editor::new(Config::default(), terminal, History::new());
                line.load_inputrc_str(&format!(
                    "\"\\C-o\": complete\nset skip-completed-text {skip_completed_text}"
                ))
                .unwrap();
                let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
                assert_eq!(
                    result,
                    ReadlineResult::Line(b"echo $'alpha' ".to_vec()),
                    "skip_completed_text={skip_completed_text}, closes_quote={closes_quote}, cursor_before_quote={cursor_before_quote}"
                );
            }
        }
    }
}

#[test]
fn insert_completions_replaces_word_and_separates_candidates() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = StaticCompletion {
        expected_type: Some(CompletionType::InsertCompletions),
        ..Default::default()
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": insert-completions")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("alpha beta ".as_bytes().to_vec())
    );
}

#[test]
fn insert_completions_with_no_candidates_preserves_word_and_rings_bell() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"no-match".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions::default(),
        candidates: Vec::new(),
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": insert-completions")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("no-match".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("\x07"));
}

#[test]
fn insert_completions_does_not_mark_directories() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("alpha-dir")).unwrap();
    std::fs::write(dir.path().join("alpha-file"), "").unwrap();
    let word = format!("{}/alp", dir.path().display());
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": insert-completions")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    let ReadlineResult::Line(line) = result else {
        panic!("expected line");
    };
    assert!(
        line.windows(format!("{}/alpha-dir ", dir.path().display()).len())
            .any(|window| window == format!("{}/alpha-dir ", dir.path().display()).as_bytes()),
        "{line:?}"
    );
    assert!(
        !line
            .windows(format!("{}/alpha-dir/ ", dir.path().display()).len())
            .any(|window| window == format!("{}/alpha-dir/ ", dir.path().display()).as_bytes()),
        "{line:?}"
    );
}

#[test]
fn completion_dequotes_and_requotes_current_word() {
    struct OneCompletion;
    impl Hooks for OneCompletion {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert!(matches!(
                request.context.word.as_slice(),
                b"al" | b"foo bar"
            ));
            let replacement = if request.context.word.as_slice() == b"foo bar" {
                "foo bar baz"
            } else {
                "alpha"
            };
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: replacement.as_bytes().to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    ..Default::default()
                },
            })
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"echo \"al".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OneCompletion;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("echo \"alpha\" ".as_bytes().to_vec())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"cat foo\\ bar".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OneCompletion;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("cat foo\\ bar\\ baz ".as_bytes().to_vec())
    );
}

#[test]
fn quote_completion_hook_receives_completion_context_inside_quotes() {
    use std::cell::Cell;

    struct ContextQuoteHook {
        seen_quote_context: Cell<bool>,
    }

    impl Hooks for ContextQuoteHook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.line.as_slice(), b"cat \"alp");
            assert_eq!(request.context.point, b"cat \"alp".len());
            assert_eq!(request.context.word_start, 4);
            assert_eq!(request.context.word_end, b"cat \"alp".len());
            assert_eq!(request.context.word.as_slice(), b"alp");
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"alpha $file".to_vec(),
                    display: None,
                }],
                options: CompletionOptions {
                    filenames: true,
                    ..Default::default()
                },
            })
        }

        fn quote_completion(&mut self, context: QuoteContext<'_>) -> Option<Vec<u8>> {
            assert_eq!(context.value, b"alpha $file");
            assert_eq!(context.line, b"cat \"alp");
            assert_eq!(context.point, b"cat \"alp".len());
            assert_eq!(context.word_start, 4);
            assert_eq!(context.word_end, b"cat \"alp".len());
            assert_eq!(context.word, b"alp");
            assert_eq!(context.quote, Some('"'));
            assert_eq!(context.completion_type, CompletionType::Complete);
            assert!(context.quote_filename);
            self.seen_quote_context.set(true);
            Some(b"\"sush-quoted\"".to_vec())
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"cat \"alp".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ContextQuoteHook {
        seen_quote_context: Cell::new(false),
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("cat \"sush-quoted\" ".as_bytes().to_vec())
    );
    assert!(hooks.seen_quote_context.get());
}

#[test]
fn quote_completion_hook_can_quote_unquoted_filename_completions() {
    struct LegacyQuoteHook;

    impl Hooks for LegacyQuoteHook {
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
            assert_eq!(context.quote, None);
            assert!(context.quote_filename);
            Some(b"legacy-quoted".to_vec())
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"alp".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = LegacyQuoteHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("legacy-quoted ".as_bytes().to_vec())
    );
}

#[test]
fn filename_hook_candidates_honor_directory_marking() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("alpha-dir")).unwrap();
    let replacement = format!("{}/alpha-dir", dir.path().display()).into_bytes();
    let word = format!("{}/alp", dir.path().display());

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            filenames: true,
            ..Default::default()
        },
        candidates: vec![CompletionCandidate {
            replacement: replacement.clone(),
            display: None,
        }],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line(format!("{}/alpha-dir/", dir.path().display()).into_bytes())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            filenames: true,
            ..Default::default()
        },
        candidates: vec![CompletionCandidate {
            replacement,
            display: None,
        }],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete\nset mark-directories off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line(format!("{}/alpha-dir", dir.path().display()).into_bytes())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            filenames: true,
            suppress_append: true,
            ..Default::default()
        },
        candidates: vec![CompletionCandidate {
            replacement: format!("{}/alpha-dir", dir.path().display()).into_bytes(),
            display: None,
        }],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line(format!("{}/alpha-dir/", dir.path().display()).into_bytes())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            filenames: true,
            append_character: Some(':'),
            ..Default::default()
        },
        candidates: vec![CompletionCandidate {
            replacement: format!("{}/alpha-dir", dir.path().display()).into_bytes(),
            display: None,
        }],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete\nset mark-directories off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line(format!("{}/alpha-dir", dir.path().display()).into_bytes())
    );
}

#[test]
fn glob_complete_marks_directories_but_glob_expand_does_not() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("alpha-dir")).unwrap();
    let word = format!("{}/al*", dir.path().display());

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": glob-complete-word")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line(format!("{}/alpha-dir/", dir.path().display()).into_bytes())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": glob-expand-word")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line(format!("{}/alpha-dir", dir.path().display()).into_bytes())
    );
}

#[test]
fn glob_completion_hook_can_expand_non_utf8_patterns() {
    struct ByteGlobHook;

    impl Hooks for ByteGlobHook {
        fn glob_expand(&mut self, pattern: &[u8]) -> Option<Vec<Vec<u8>>> {
            assert_eq!(pattern, b"pre\xff*");
            Some(vec![b"pre\xff-match".to_vec()])
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"pre\xff*".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = ByteGlobHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": glob-complete-word")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"pre\xff-match ".to_vec()));
}

#[test]
fn programmable_completion_options_control_fallbacks_and_spacing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("alpha-dir")).unwrap();
    std::fs::write(dir.path().join("alpha-file"), "").unwrap();
    let word = format!("{}/alp", dir.path().display());
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            dirnames: true,
            nospace: true,
            ..Default::default()
        },
        candidates: Vec::new(),
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert!(
        matches!(&result, ReadlineResult::Line(line) if line.ends_with(b"alpha-dir/")),
        "{result:?}"
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"x".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            nosort: true,
            nospace: true,
            ..Default::default()
        },
        candidates: vec![
            CompletionCandidate {
                replacement: b"zeta".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"alpha".to_vec(),
                display: None,
            },
        ],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": menu-complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("zeta".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"no-match".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions::default(),
        candidates: Vec::new(),
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("no-match".as_bytes().to_vec()));

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha-file"), "").unwrap();
    let word = format!("{}/alp", dir.path().display());
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            bashdefault: true,
            ..Default::default()
        },
        candidates: Vec::new(),
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line(word.into_bytes()));
}

#[test]
fn programmable_completion_options_cover_quote_and_directory_fallbacks() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"cat \"alp".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            filenames: true,
            noquote: true,
            ..Default::default()
        },
        candidates: vec![CompletionCandidate {
            replacement: b"alpha file".to_vec(),
            display: None,
        }],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("cat \"alpha file\" ".as_bytes().to_vec())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"cat \"alp".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            fullquote: true,
            ..Default::default()
        },
        candidates: vec![CompletionCandidate {
            replacement: b"alpha $file".to_vec(),
            display: None,
        }],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("cat \"alpha \\$file\" ".as_bytes().to_vec())
    );

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("alpha-dir")).unwrap();
    let word = format!("{}/alp", dir.path().display());
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(word.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            plusdirs: true,
            nospace: true,
            ..Default::default()
        },
        candidates: Vec::new(),
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete\nset mark-directories off")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert!(
        matches!(&result, ReadlineResult::Line(line) if line.ends_with(b"alpha-dir")),
        "{result:?}"
    );
}

#[test]
fn programmable_completion_options_filter_transform_and_display_only() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            filter_suffix: Some(b".rs".to_vec()),
            replacement_prefix: Some(b"--".to_vec()),
            suppress_append: true,
            ..Default::default()
        },
        candidates: vec![
            CompletionCandidate {
                replacement: b"alpha.rs".to_vec(),
                display: None,
            },
            CompletionCandidate {
                replacement: b"alpha.txt".to_vec(),
                display: None,
            },
        ],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("--alpha.rs".as_bytes().to_vec())
    );

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = OptionCompletion {
        options: CompletionOptions {
            action: Some(CompletionAction::DisplayOnly),
            ..Default::default()
        },
        candidates: vec![CompletionCandidate {
            replacement: b"alpha".to_vec(),
            display: None,
        }],
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("a".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("alpha"));
}

#[test]
fn completion_variables_display_and_skip_completed_text() {
    struct AlpineCompletion;
    impl Hooks for AlpineCompletion {
        fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"alpine".to_vec(),
                    display: None,
                }],
                options: Default::default(),
            })
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"alpine".to_vec()),
        TerminalEvent::Bytes(b"\x02\x02\x02\x02".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = AlpineCompletion;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete\nset skip-completed-text on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Line("alpine ".as_bytes().to_vec()));

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = StaticCompletion {
        expected_type: Some(CompletionType::Complete),
        ..Default::default()
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete\nset show-all-if-ambiguous on")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert!(line.terminal().out.contains("alpha"));
    assert!(line.terminal().out.contains("beta"));
}

#[test]
fn variable_completion_uses_hook_names() {
    struct VariableHook;
    impl Hooks for VariableHook {
        fn variable_names(&mut self) -> Vec<Vec<u8>> {
            vec![b"SUSH_LOCAL".to_vec(), b"SUSH_OTHER".to_vec()]
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"$SUSH_L".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = VariableHook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete-variable")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(
        result,
        ReadlineResult::Line("$SUSH_LOCAL ".as_bytes().to_vec())
    );
}

#[test]
fn repeated_tab_lists_unmodified_directory_completions() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha"), "").unwrap();
    std::fs::write(dir.path().join("beta"), "").unwrap();
    let input = format!("{}/", dir.path().display());
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(input.as_bytes().to_vec()),
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(input.into_bytes()));
    assert!(line.terminal().out.contains("alpha"));
    assert!(line.terminal().out.contains("beta"));
}

#[test]
fn repeated_tab_uses_default_screen_size_when_terminal_reports_zero() {
    struct AlphaCompletion;
    impl Hooks for AlphaCompletion {
        fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
            Some(CompletionResponse {
                candidates: vec![
                    CompletionCandidate {
                        replacement: b"alpha".to_vec(),
                        display: None,
                    },
                    CompletionCandidate {
                        replacement: b"alpine".to_vec(),
                        display: None,
                    },
                ],
                options: CompletionOptions::default(),
            })
        }
    }

    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"a".to_vec()),
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 0;
    terminal.rows = 0;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let mut hooks = AlphaCompletion;

    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();

    assert_eq!(result, ReadlineResult::Line(b"alp".to_vec()));
    assert!(line.terminal().out.contains("alpha   alpine"));
    assert!(!line.terminal().out.contains("--More--"));
}

#[test]
fn complete_into_braces_uses_default_completion_fallbacks() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha"), "").unwrap();
    std::fs::write(dir.path().join("alpine"), "").unwrap();
    let input = format!("{}/al", dir.path().display());
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(input.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete-into-braces")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    let ReadlineResult::Line(line) = result else {
        panic!("expected accepted line");
    };
    assert_eq!(
        line,
        format!("{}/alp{{ha,ine}} ", dir.path().display()).into_bytes()
    );
}

#[test]
fn completion_prefix_display_length_abbreviates_common_prefix() {
    struct LongPrefixCompletion;
    impl Hooks for LongPrefixCompletion {
        fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
            Some(CompletionResponse {
                candidates: vec![
                    CompletionCandidate {
                        replacement: b"longprefix-alpha".to_vec(),
                        display: None,
                    },
                    CompletionCandidate {
                        replacement: b"longprefix-beta".to_vec(),
                        display: None,
                    },
                ],
                options: CompletionOptions {
                    filenames: true,
                    ..Default::default()
                },
            })
        }
    }

    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = LongPrefixCompletion;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(
        "\"\\C-o\": possible-completions\nset completion-prefix-display-length 4",
    )
    .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert!(line.terminal().out.contains("...alpha"));
    assert!(line.terminal().out.contains("...beta"));
}

#[test]
fn filename_listing_renders_control_bytes_as_caret_notation() {
    struct Hook;
    impl Hooks for Hook {
        fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
            Some(CompletionResponse {
                candidates: vec![
                    CompletionCandidate {
                        replacement: b"probe\x1b[2Jend".to_vec(),
                        display: None,
                    },
                    CompletionCandidate {
                        replacement: b"probe-plain".to_vec(),
                        display: None,
                    },
                ],
                options: CompletionOptions {
                    filenames: true,
                    ..Default::default()
                },
            })
        }
    }
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"probe".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = Hook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": possible-completions")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    let out = line.terminal().out.clone();
    assert!(
        out.contains("probe^[[2Jend"),
        "caret rendering missing in {out:?}"
    );
    assert!(
        !out.contains("\x1b[2J"),
        "raw ESC sequence leaked into listing in {out:?}"
    );
}

#[test]
fn possible_completions_after_point_moves_to_previous_visual_row_redraws_from_origin() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\x02".to_vec()),
        TerminalEvent::Bytes(b"\x02".to_vec()),
        TerminalEvent::Bytes(b"\x0f".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 8;
    let mut hooks = StaticCompletion {
        expected_type: Some(CompletionType::PossibleCompletions),
        ..Default::default()
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": possible-completions")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();

    assert_eq!(result, ReadlineResult::Line(b"abcdef".to_vec()));
    assert!(line.terminal().moved_up.contains(&4));
}

#[test]
fn export_completions_writes_machine_readable_records() {
    struct Hook;
    impl Hooks for Hook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.word.as_slice(), b"al");
            Some(CompletionResponse {
                candidates: vec![
                    CompletionCandidate {
                        replacement: b"alpha".to_vec(),
                        display: Some("alpha display".to_string()),
                    },
                    CompletionCandidate {
                        replacement: b"alpine".to_vec(),
                        display: None,
                    },
                ],
                ..Default::default()
            })
        }
    }
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"al".to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = Hook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": export-completions")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert!(line.terminal().out.contains("\r\n3\nal\n0:2\n"));
    assert!(line.terminal().out.contains("alp\n"));
    assert!(line.terminal().out.contains("alpha\n"));
    assert!(line.terminal().out.contains("alpine\n"));
}

#[test]
fn export_completions_reports_byte_offsets() {
    struct Hook;
    impl Hooks for Hook {
        fn complete(&mut self, request: CompletionRequest) -> Option<CompletionResponse> {
            assert_eq!(request.context.point, "é a".len());
            assert_eq!(request.context.word_start, "é ".len());
            assert_eq!(request.context.word_end, "é a".len());
            Some(CompletionResponse {
                candidates: vec![CompletionCandidate {
                    replacement: b"alpha".to_vec(),
                    display: None,
                }],
                ..Default::default()
            })
        }
    }
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes("é a".as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut hooks = Hook;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": export-completions")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert!(line.terminal().out.contains("\r\n1\na\n3:4\nalpha\n"));
}

#[test]
fn bell_style_controls_completion_failure_signal() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x0f]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("\"\\C-o\": complete\nset disable-completion on\nset bell-style visible")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(Vec::new()));
    assert!(line.terminal().out.contains("\x1b[?5h\x1b[?5l"));
}

#[test]
fn completion_query_response_suffix_survives_batching() {
    fn run(events: Vec<TerminalEvent>) -> (ReadlineResult, ReadlineResult, String) {
        let terminal = MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set completion-query-items 1")
            .unwrap();
        let mut hooks = StaticCompletion::default();
        let first = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        let second = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        let out = line.terminal().out.clone();
        (first, second, out)
    }

    for answer in [b"y".as_slice(), b"n".as_slice()] {
        let split = run(vec![
            TerminalEvent::Bytes(b"\t".to_vec()),
            TerminalEvent::Bytes(b"\t".to_vec()),
            TerminalEvent::Bytes(answer.to_vec()),
            TerminalEvent::Bytes(b"Z".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
            TerminalEvent::Bytes(b"W".to_vec()),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]);
        let mut batched_answer = answer.to_vec();
        batched_answer.extend_from_slice(b"Z\rW");
        let batched = run(vec![
            TerminalEvent::Bytes(b"\t".to_vec()),
            TerminalEvent::Bytes(b"\t".to_vec()),
            TerminalEvent::Bytes(batched_answer),
            TerminalEvent::Bytes(b"\r".to_vec()),
        ]);
        assert_eq!(split.0, batched.0, "answer={answer:?}");
        assert_eq!(split.1, batched.1, "answer={answer:?}");
        assert_eq!(
            split.0,
            ReadlineResult::Line(b"Z".to_vec()),
            "answer={answer:?}"
        );
        assert_eq!(
            split.1,
            ReadlineResult::Line(b"W".to_vec()),
            "answer={answer:?}"
        );
        let split_listed = split.2.contains("alpha") && split.2.contains("beta");
        let batched_listed = batched.2.contains("alpha") && batched.2.contains("beta");
        assert_eq!(split_listed, batched_listed, "answer={answer:?}");
        assert_eq!(
            split_listed,
            answer == b"y",
            "answer={answer:?} split={:?} batched={:?}",
            split.2,
            batched.2
        );
    }
}

#[test]
fn completion_query_interrupt_preserves_suffix_for_next_read() {
    for batched in [false, true] {
        let query_response = if batched {
            TerminalEvent::Bytes(vec![0x03, b'Z'])
        } else {
            TerminalEvent::Bytes(vec![0x03])
        };
        let mut events = vec![
            TerminalEvent::Bytes(b"\t".to_vec()),
            TerminalEvent::Bytes(b"\t".to_vec()),
            query_response,
        ];
        if !batched {
            events.push(TerminalEvent::Bytes(b"Z".to_vec()));
        }
        events.push(TerminalEvent::Bytes(b"\r".to_vec()));
        let terminal = MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set completion-query-items 1")
            .unwrap();
        let mut hooks = StaticCompletion::default();
        let first = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(first, ReadlineResult::Interrupted, "batched={batched}");
        let second = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        assert_eq!(
            second,
            ReadlineResult::Line(b"Z".to_vec()),
            "batched={batched}"
        );
    }
}

struct ManyCompletion;

impl Hooks for ManyCompletion {
    fn complete(&mut self, _: CompletionRequest) -> Option<CompletionResponse> {
        Some(CompletionResponse {
            candidates: [
                "alpha-00",
                "bravo-01",
                "charlie-02",
                "delta-03",
                "echo-04",
                "foxtrot-05",
                "golf-06",
                "hotel-07",
                "india-08",
                "juliet-09",
            ]
            .into_iter()
            .map(|name| CompletionCandidate {
                replacement: name.as_bytes().to_vec(),
                display: None,
            })
            .collect(),
            options: CompletionOptions::default(),
        })
    }
}

#[test]
fn completion_pager_quit_preserves_batched_suffix() {
    fn run(events: Vec<TerminalEvent>) -> (ReadlineResult, String) {
        let mut terminal = MemoryTerminal::with_events(events);
        terminal.columns = 20;
        terminal.rows = 5;
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set completion-display-width 20")
            .unwrap();
        let mut hooks = ManyCompletion;
        let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
        let out = line.terminal().out.clone();
        (result, out)
    }

    let split = run(vec![
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"q".to_vec()),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let batched = run(vec![
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"\t".to_vec()),
        TerminalEvent::Bytes(b"qZ".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    for (label, output) in [("split", &split.1), ("batched", &batched.1)] {
        assert!(
            output.contains("--More--"),
            "{label} pager should prompt, got {output:?}"
        );
    }
    assert_eq!(split.0, batched.0);
    assert_eq!(split.0, ReadlineResult::Line(b"Z".to_vec()));
}
