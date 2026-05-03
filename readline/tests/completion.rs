mod common;

use common::MemoryTerminal;
use readline::{
    CompletionAction, CompletionCandidate, CompletionOptions, CompletionRequest,
    CompletionResponse, CompletionType, Config, Editor, History, Hooks, Prompt, ReadlineResult,
    TerminalEvent,
};

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
    assert_eq!(result, ReadlineResult::Line("alpha".as_bytes().to_vec()));
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
    assert_eq!(result, ReadlineResult::Line("beta".as_bytes().to_vec()));
}

#[test]
fn menu_complete_stops_at_end_and_restores_original_text() {
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
    assert_eq!(result, ReadlineResult::Line("a".as_bytes().to_vec()));
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
    assert_eq!(result, ReadlineResult::Line("alpine".as_bytes().to_vec()));

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
        fn variable_names(&mut self) -> Vec<String> {
            vec!["SUSH_LOCAL".to_string(), "SUSH_OTHER".to_string()]
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
    assert!(line.contains(&b'{'));
    assert!(line.windows("alpha".len()).any(|window| window == b"alpha"));
    assert!(
        line.windows("alpine".len())
            .any(|window| window == b"alpine")
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
