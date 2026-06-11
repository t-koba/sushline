use super::*;

#[test]
fn revert_line_restores_initial_prefill() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"X".to_vec()),
        TerminalEvent::Bytes(vec![0x1b, b'r']),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.pending_initial_line = Some(b"seed".to_vec());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("seed".as_bytes().to_vec()));
}

#[test]
fn default_filename_completion_generates_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("alpha-file");
    std::fs::write(&file, "").unwrap();
    let word = format!("{}/alp", dir.path().display());
    let line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let response = complete_filenames_bytes(
        word.as_bytes(),
        &FilenameOptions::from_variables(&line.variables),
    );
    assert_eq!(response.candidates.len(), 1);
    assert!(response.candidates[0].replacement.ends_with(b"alpha-file"));
}

#[cfg(target_os = "linux")]
#[test]
fn filename_completion_preserves_non_utf8_names_as_ansi_c_quote() {
    use std::os::unix::ffi::OsStringExt;
    let dir = tempfile::tempdir().unwrap();
    let name = std::ffi::OsString::from_vec(vec![b'a', 0xff, b'b']);
    std::fs::write(dir.path().join(&name), "").unwrap();
    let word = format!("{}/a", dir.path().display());
    let line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let response = complete_filenames_bytes(
        word.as_bytes(),
        &FilenameOptions::from_variables(&line.variables),
    );
    assert_eq!(response.candidates.len(), 1);
    assert!(
        response.candidates[0]
            .replacement_string()
            .contains("$'a\\xffb'")
    );
}

#[test]
fn filename_completion_honors_mark_directories_and_map_case() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("alpha-dir")).unwrap();
    let word = format!("{}/alpha_dir", dir.path().display());
    let mut line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    line.load_inputrc_str(
        "set mark-directories off\nset completion-ignore-case on\nset completion-map-case on",
    )
    .unwrap();
    let response = complete_filenames_bytes(
        word.as_bytes(),
        &FilenameOptions::from_variables(&line.variables),
    );
    assert_eq!(response.candidates.len(), 1);
    assert!(response.candidates[0].replacement.ends_with(b"alpha-dir"));
    assert!(!response.candidates[0].replacement.ends_with(b"/"));
    assert_eq!(response.candidates[0].display.as_deref(), Some("alpha-dir"));
}

#[cfg(unix)]
#[test]
fn filename_completion_marks_symlinked_directories_like_readline() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("target-dir")).unwrap();
    symlink(dir.path().join("target-dir"), dir.path().join("alpha-link")).unwrap();
    let word = format!("{}/alp", dir.path().display());

    let mut line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let response = complete_filenames_bytes(
        word.as_bytes(),
        &FilenameOptions::from_variables(&line.variables),
    );
    assert_eq!(response.candidates.len(), 1);
    assert!(response.options.nospace);
    assert!(response.candidates[0].replacement.ends_with(b"alpha-link"));
    assert!(!response.candidates[0].replacement.ends_with(b"/"));
    assert_eq!(
        response.candidates[0].display.as_deref(),
        Some("alpha-link/")
    );

    line.load_inputrc_str("set mark-symlinked-directories on")
        .unwrap();
    let response = complete_filenames_bytes(
        word.as_bytes(),
        &FilenameOptions::from_variables(&line.variables),
    );
    assert_eq!(response.candidates.len(), 1);
    assert!(response.options.nospace);
    assert!(response.candidates[0].replacement.ends_with(b"alpha-link"));
    assert!(!response.candidates[0].replacement.ends_with(b"/"));

    line.load_inputrc_str("set mark-directories off").unwrap();
    let response = complete_filenames_bytes(
        word.as_bytes(),
        &FilenameOptions::from_variables(&line.variables),
    );
    assert_eq!(response.candidates.len(), 1);
    assert!(response.options.nospace);
    assert!(response.candidates[0].replacement.ends_with(b"alpha-link"));
    assert!(!response.candidates[0].replacement.ends_with(b"/"));
    assert_eq!(
        response.candidates[0].display.as_deref(),
        Some("alpha-link")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn raw_filename_completion_does_not_append_space_after_marked_directory() {
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let dir = tempfile::tempdir().unwrap();
    let name = OsString::from_vec(vec![b'a', 0xff]);
    std::fs::create_dir(dir.path().join(&name)).unwrap();
    let mut word = dir.path().as_os_str().as_bytes().to_vec();
    word.push(b'/');
    word.push(b'a');

    let line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let response =
        complete_filenames_bytes(&word, &FilenameOptions::from_variables(&line.variables));
    assert_eq!(response.candidates.len(), 1);
    assert!(response.options.nospace);
    assert!(response.candidates[0].replacement.ends_with(&[b'a', 0xff]));
    assert!(!response.candidates[0].replacement.ends_with(b"/"));

    let response =
        complete_directories_bytes(&word, &FilenameOptions::from_variables(&line.variables));
    assert_eq!(response.candidates.len(), 1);
    assert!(response.options.nospace);
    assert!(response.candidates[0].replacement.ends_with(&[b'a', 0xff]));
    assert!(!response.candidates[0].replacement.ends_with(b"/"));
}

#[test]
fn directory_completion_recomputes_nospace_after_filtering() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("alpha-dir")).unwrap();
    std::fs::write(dir.path().join("alpha-file"), "").unwrap();
    let word = format!("{}/alp", dir.path().display());

    let line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let response = complete_directories_bytes(
        word.as_bytes(),
        &FilenameOptions::from_variables(&line.variables),
    );
    assert_eq!(response.candidates.len(), 1);
    assert!(response.options.nospace);
    assert!(response.candidates[0].replacement.ends_with(b"alpha-dir"));
}

#[test]
fn ls_colors_extension_rules_override_file_kind_colors() {
    let path = Path::new("alpha.rs");
    assert_eq!(
        ls_color_code_from_spec("alpha.rs", path, "fi", "fi=0:*.rs=38;5;214:*.txt=32"),
        Some("38;5;214".to_string())
    );
    assert_eq!(
        ls_color_code_from_spec("alpha.bin", path, "fi", "fi=0:*.rs=38;5;214"),
        Some("0".to_string())
    );
}

#[test]
fn default_completion_leaves_application_context_to_hook() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha-file"), "").unwrap();

    struct CompletionApplicationHook;
    impl Hooks for CompletionApplicationHook {
        fn default_complete(&mut self, request: &CompletionRequest) -> Option<CompletionResponse> {
            Some(CompletionResponse {
                candidates: vec![crate::completion::CompletionCandidate {
                    replacement: format!("app:{}", String::from_utf8_lossy(&request.context.word))
                        .into_bytes(),
                    display: None,
                }],
                options: Default::default(),
            })
        }
    }
    let mut hooks = CompletionApplicationHook;
    let mut line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let command_response = line.default_completion(
        &CompletionRequest {
            context: CompletionContext {
                line: b"sushline-c".to_vec(),
                point: 10,
                word_start: 0,
                word_end: 10,
                word: b"sushline-c".to_vec(),
                key: b"\t".to_vec(),
                completion_type: CompletionType::Complete,
            },
        },
        &mut hooks,
    );
    assert_eq!(
        command_response.candidates[0].replacement.as_slice(),
        b"app:sushline-c"
    );

    let mut line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let filename_word = format!("{}/alp", dir.path().display());
    let filename_response = line.default_completion(
        &CompletionRequest {
            context: CompletionContext {
                line: filename_word.as_bytes().to_vec(),
                point: filename_word.len(),
                word_start: 0,
                word_end: filename_word.len(),
                word: filename_word.as_bytes().to_vec(),
                key: b"\t".to_vec(),
                completion_type: CompletionType::Complete,
            },
        },
        &mut (),
    );
    assert!(
        filename_response
            .candidates
            .iter()
            .any(|candidate| candidate.replacement.ends_with(b"alpha-file"))
    );
    let bare_response = line.default_completion(
        &CompletionRequest {
            context: CompletionContext {
                line: b"sudo sushline-c".to_vec(),
                point: 15,
                word_start: 5,
                word_end: 15,
                word: b"sushline-c".to_vec(),
                key: b"\t".to_vec(),
                completion_type: CompletionType::Complete,
            },
        },
        &mut (),
    );
    assert!(
        !bare_response
            .candidates
            .iter()
            .any(|candidate| candidate.replacement.as_slice() == b"app:sushline-c")
    );
}

#[test]
fn command_completion_gets_application_language_words_from_hooks() {
    struct CommandHook;
    impl Hooks for CommandHook {
        fn command_names(&mut self) -> Vec<Vec<u8>> {
            vec![b"while".to_vec(), b"echo".to_vec()]
        }
    }
    let mut hooks = CommandHook;
    let response = complete_commands_with_hooks_bytes(b"wh", &mut hooks);
    assert!(
        response
            .candidates
            .iter()
            .any(|candidate| candidate.replacement.as_slice() == b"while")
    );
    let response = complete_commands_with_hooks_bytes(b"ech", &mut hooks);
    assert!(
        response
            .candidates
            .iter()
            .any(|candidate| candidate.replacement.as_slice() == b"echo")
    );
}

#[test]
fn application_command_binding_passes_readline_context_and_applies_edit() {
    #[derive(Debug, PartialEq, Eq)]
    struct SeenCommand {
        command: String,
        line: Vec<u8>,
        point: usize,
        mark: Option<usize>,
        argument: Option<i32>,
        key: Vec<u8>,
        keymap: crate::keymap::KeyMapName,
    }

    struct ApplicationCommandHook {
        seen: Vec<SeenCommand>,
    }

    impl Hooks for ApplicationCommandHook {
        fn on_command(
            &mut self,
            context: crate::hooks::CommandContext<'_>,
        ) -> Option<crate::hooks::Edit> {
            self.seen.push(SeenCommand {
                command: context.command.to_string(),
                line: context.line.to_vec(),
                point: context.point,
                mark: context.mark,
                argument: context.argument,
                key: context.key.to_vec(),
                keymap: context.keymap,
            });
            Some(crate::hooks::Edit {
                line: Some(b"rewritten".to_vec()),
                point: Some(3),
                mark: Some(Some(1)),
            })
        }
    }

    let terminal =
        MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"abc\x1b2\x0f\r".to_vec())]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.bind_api()
        .apply_builtin_args(&["-x", "\"\\C-o\": __widget"])
        .unwrap();
    let mut hooks = ApplicationCommandHook { seen: Vec::new() };

    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();

    assert_eq!(result, ReadlineResult::Line(b"rewritten".to_vec()));
    assert_eq!(
        hooks.seen,
        vec![SeenCommand {
            command: "__widget".to_string(),
            line: b"abc".to_vec(),
            point: 3,
            mark: None,
            argument: Some(2),
            key: vec![0x0f],
            keymap: crate::keymap::KeyMapName::EmacsStandard,
        }]
    );
}

#[test]
fn glob_completion_matches_bracket_expressions() {
    assert!(glob_match("file[0-9].rs", "file7.rs"));
    assert!(glob_match("file[!0-9].rs", "filex.rs"));
    assert!(!glob_match("file[!0-9].rs", "file7.rs"));
    assert!(glob_match("[[:alpha:]][[:digit:]]", "a7"));
    assert!(!glob_match("[![:digit:]]", "7"));
    assert!(glob_match(r"file\*.rs", "file*.rs"));
    assert!(!glob_match(r"file\*.rs", "file1.rs"));
}

#[test]
fn glob_completion_hides_dotfiles_unless_pattern_starts_with_dot() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".secret"), "").unwrap();
    std::fs::write(dir.path().join("visible"), "").unwrap();
    let line = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    let mut hooks = ();
    let response = glob_complete(
        &format!("{}/*", dir.path().display()),
        &mut hooks,
        line.variables(),
    );
    assert!(
        response
            .candidates
            .iter()
            .all(|candidate| !candidate.replacement.ends_with(b".secret"))
    );
    let response = glob_complete(
        &format!("{}/.*", dir.path().display()),
        &mut hooks,
        line.variables(),
    );
    assert!(
        response
            .candidates
            .iter()
            .any(|candidate| candidate.replacement.ends_with(b".secret"))
    );
}

#[test]
fn completion_display_honors_layout_variables() {
    let items = vec![
        "alpha".to_string(),
        "beta".to_string(),
        "gamma".to_string(),
        "delta".to_string(),
    ];
    assert_eq!(
        format_completion_items_with_trailing(&items, 16, false, false),
        vec!["alpha  gamma", "beta   delta"]
    );
    assert_eq!(
        format_completion_items_with_trailing(&items, 16, true, false),
        vec!["alpha  beta", "gamma  delta"]
    );
    assert_eq!(
        format_completion_items_with_trailing(&items, 16, false, true),
        vec!["alpha  gamma  ", "beta   delta  "]
    );
}

#[test]
fn colored_completion_prefix_marks_common_prefix() {
    let items = ["alpha".to_string(), "alpine".to_string()];
    let candidates = items
        .iter()
        .map(|item| crate::completion::CompletionCandidate {
            replacement: item.clone().into_bytes(),
            display: None,
        })
        .collect::<Vec<_>>();
    let prefix = String::from_utf8_lossy(&common_prefix_bytes(&candidates).unwrap()).into_owned();
    assert_eq!(
        color_completion_prefix(&items[0], &candidates[0].replacement_string(), &prefix),
        "\x1b[1malp\x1b[0mha"
    );
}

#[test]
fn possible_completions_uses_query_and_visible_stats() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"y".to_vec())]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set completion-query-items 1\nset visible-stats on")
        .unwrap();
    let response = CompletionResponse {
        candidates: vec![
            crate::completion::CompletionCandidate {
                replacement: b"alpha".to_vec(),
                display: None,
            },
            crate::completion::CompletionCandidate {
                replacement: b"beta".to_vec(),
                display: None,
            },
        ],
        options: Default::default(),
    };
    line.display_completions(&response).unwrap();
    assert!(line.terminal.out.contains("Display all 2 possibilities?"));
    assert!(line.terminal.out.contains("alpha "));
}

#[test]
fn visible_stats_marks_file_types_for_filename_completions() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dir")).unwrap();
    let file = dir.path().join("run");
    std::fs::write(&file, "").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let terminal = MemoryTerminal::default();
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set visible-stats on").unwrap();
    let response = CompletionResponse {
        candidates: vec![
            crate::completion::CompletionCandidate {
                replacement: dir.path().join("dir").display().to_string().into_bytes(),
                display: Some("dir".to_string()),
            },
            crate::completion::CompletionCandidate {
                replacement: file.display().to_string().into_bytes(),
                display: Some("run".to_string()),
            },
        ],
        options: crate::completion::CompletionOptions {
            filenames: true,
            ..Default::default()
        },
    };
    line.display_completions(&response).unwrap();
    assert!(line.terminal.out.contains("dir/"));
    #[cfg(unix)]
    assert!(line.terminal.out.contains("run*"));
}

#[test]
fn page_completions_negative_answer_suppresses_display() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"n".to_vec())]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set completion-query-items 1\nset visible-stats on")
        .unwrap();
    let response = CompletionResponse {
        candidates: vec![
            crate::completion::CompletionCandidate {
                replacement: b"alpha".to_vec(),
                display: None,
            },
            crate::completion::CompletionCandidate {
                replacement: b"beta".to_vec(),
                display: None,
            },
        ],
        options: Default::default(),
    };
    line.display_completions(&response).unwrap();
    assert!(line.terminal.out.contains("Display all 2 possibilities?"));
    assert!(!line.terminal.out.contains("alpha "));
}
