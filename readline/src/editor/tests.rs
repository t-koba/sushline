use super::*;
use crate::completion::builtin::{complete_commands_with_hooks_bytes, glob_complete};
use crate::completion::display::{
    color_completion_prefix, common_prefix_bytes, format_completion_items_with_trailing,
};
use crate::completion::filename::{
    FilenameOptions, complete_filenames_bytes, glob_match, ls_color_code_from_spec,
};
use crate::completion::{CompletionContext, CompletionRequest, CompletionResponse, CompletionType};
use crate::terminal::{TerminalEvent, TerminalSize};
use history::expansion::{
    HistoryChars, HistoryExpansionError, HistoryExpansionPolicy, expand_history,
};
use std::collections::VecDeque;

const NAMED_READLINE_COMMAND_DISPATCH: &[&str] = &[
    "alias-expand-line",
    "arrow-key-prefix",
    "backward-byte",
    "bash-vi-complete",
    "bracketed-paste-begin",
    "character-search",
    "character-search-backward",
    "clear-display",
    "complete-command",
    "complete-filename",
    "complete-hostname",
    "complete-into-braces",
    "complete-username",
    "complete-variable",
    "copy-backward-word",
    "copy-forward-word",
    "dabbrev-expand",
    "delete-char-or-list",
    "display-shell-version",
    "do-lowercase-version",
    "dump-functions",
    "dump-macros",
    "dump-variables",
    "dynamic-complete-history",
    "edit-and-execute-command",
    "emacs-editing-mode",
    "execute-named-command",
    "export-completions",
    "fetch-history",
    "forward-backward-delete-char",
    "forward-byte",
    "forward-search-history",
    "glob-complete-word",
    "glob-expand-word",
    "glob-list-expansions",
    "history-and-alias-expand-line",
    "history-expand-line",
    "history-substring-search-backward",
    "history-substring-search-forward",
    "insert-comment",
    "insert-completions",
    "insert-last-argument",
    "magic-space",
    "menu-complete",
    "menu-complete-backward",
    "next-screen-line",
    "non-incremental-forward-search-history",
    "non-incremental-forward-search-history-again",
    "non-incremental-reverse-search-history",
    "non-incremental-reverse-search-history-again",
    "old-menu-complete",
    "operate-and-get-next",
    "overwrite-mode",
    "possible-command-completions",
    "possible-completions",
    "possible-filename-completions",
    "possible-hostname-completions",
    "possible-username-completions",
    "possible-variable-completions",
    "previous-screen-line",
    "re-read-init-file",
    "redraw-current-line",
    "shell-backward-kill-word",
    "shell-backward-word",
    "shell-expand-line",
    "shell-forward-word",
    "shell-kill-word",
    "shell-transpose-words",
    "skip-csi-sequence",
    "spell-correct-word",
    "tab-insert",
    "tilde-expand",
    "tty-status",
    "unix-filename-rubout",
    "vi-arg-digit",
    "vi-bWord",
    "vi-back-to-indent",
    "vi-backward-bigword",
    "vi-backward-word",
    "vi-bword",
    "vi-change-case",
    "vi-change-char",
    "vi-change-to",
    "vi-char-search",
    "vi-column",
    "vi-complete",
    "vi-delete",
    "vi-delete-to",
    "vi-eWord",
    "vi-edit-and-execute-command",
    "vi-editing-mode",
    "vi-end-bigword",
    "vi-end-word",
    "vi-eof-maybe",
    "vi-eword",
    "vi-fWord",
    "vi-fetch-history",
    "vi-first-print",
    "vi-forward-bigword",
    "vi-forward-word",
    "vi-fword",
    "vi-goto-mark",
    "vi-match",
    "vi-next-word",
    "vi-overstrike",
    "vi-overstrike-delete",
    "vi-prev-word",
    "vi-put",
    "vi-redo",
    "vi-replace",
    "vi-rubout",
    "vi-search",
    "vi-search-again",
    "vi-set-mark",
    "vi-subst",
    "vi-tilde-expand",
    "vi-undo",
    "vi-unix-word-rubout",
    "vi-yank-arg",
    "vi-yank-pop",
    "vi-yank-to",
    "yank-last-arg",
    "yank-nth-arg",
];

fn expand_history_for_test(line: &str, history: &History) -> String {
    String::from_utf8(
        expand_history(
            line.as_bytes(),
            history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
    )
    .unwrap()
}

fn expand_history_with_chars_for_test(
    line: &str,
    history: &History,
    histchars: HistoryChars,
) -> Result<String, HistoryExpansionError> {
    expand_history(
        line.as_bytes(),
        history,
        histchars,
        &HistoryExpansionPolicy::default(),
        |_| false,
    )
    .map(|bytes| String::from_utf8(bytes).unwrap())
}

struct TestHistoryHook;

impl Hooks for TestHistoryHook {
    fn expand_history(
        &mut self,
        context: crate::hooks::HistoryExpansionContext<'_>,
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

#[derive(Default)]
struct MemoryTerminal {
    events: VecDeque<TerminalEvent>,
    out: String,
    columns: u16,
    tty_special: Vec<(u8, &'static str)>,
    meta_enabled: Vec<bool>,
    keypad_enabled: Vec<bool>,
    moved_columns: Vec<u16>,
    moved_up: Vec<u16>,
    cleared_screen: usize,
}

impl MemoryTerminal {
    fn with_events(events: Vec<TerminalEvent>) -> Self {
        Self {
            events: events.into(),
            out: String::new(),
            columns: 80,
            tty_special: Vec::new(),
            meta_enabled: Vec::new(),
            keypad_enabled: Vec::new(),
            moved_columns: Vec::new(),
            moved_up: Vec::new(),
            cleared_screen: 0,
        }
    }
}

impl TerminalIo for MemoryTerminal {
    fn enter_raw_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn restore_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn read_event(&mut self, _: Option<Duration>) -> io::Result<TerminalEvent> {
        Ok(self.events.pop_front().unwrap_or(TerminalEvent::Timeout))
    }

    fn write(&mut self, text: &str) -> io::Result<()> {
        self.out.push_str(text);
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.push_str(&String::from_utf8_lossy(bytes));
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn size(&self) -> io::Result<TerminalSize> {
        Ok(TerminalSize {
            columns: self.columns,
            rows: 24,
        })
    }

    fn clear_after_cursor(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn clear_to_screen_end(&mut self) -> io::Result<()> {
        self.cleared_screen += 1;
        Ok(())
    }

    fn clear_display(&mut self) -> io::Result<()> {
        self.cleared_screen += 1;
        self.out.push_str("\r\x1b[J");
        Ok(())
    }

    fn move_to_column(&mut self, column: u16) -> io::Result<()> {
        self.moved_columns.push(column);
        Ok(())
    }

    fn set_meta_key_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.meta_enabled.push(enabled);
        Ok(())
    }

    fn set_application_keypad_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.keypad_enabled.push(enabled);
        Ok(())
    }

    fn move_up(&mut self, rows: u16) -> io::Result<()> {
        self.moved_up.push(rows);
        Ok(())
    }

    fn tty_special_bindings(&self) -> Vec<(u8, &'static str)> {
        self.tty_special.clone()
    }
}

#[test]
fn every_readline_command_is_in_typed_or_named_dispatch_table() {
    for command in crate::keymap::BIND_FUNCTION_NAMES {
        assert!(
            EditCommand::parse(command).is_some()
                || NAMED_READLINE_COMMAND_DISPATCH
                    .binary_search(command)
                    .is_ok(),
            "{command} must have an explicit dispatch classification"
        );
    }
}

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
        fn command_names(&self) -> Vec<String> {
            vec!["while".to_string(), "echo".to_string()]
        }
    }
    let hooks = CommandHook;
    let response = complete_commands_with_hooks_bytes(b"wh", &hooks);
    assert!(
        response
            .candidates
            .iter()
            .any(|candidate| candidate.replacement.as_slice() == b"while")
    );
    let response = complete_commands_with_hooks_bytes(b"ech", &hooks);
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
    let response = glob_complete(
        &format!("{}/*", dir.path().display()),
        &(),
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
        &(),
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

#[test]
fn interrupted_readline_cleans_up_terminal_modes() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Signal(libc::SIGINT)]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal.out.contains("^C\r\n"));
    assert!(line.terminal.out.contains("\x1b[?2004h"));
    assert!(line.terminal.out.contains("\x1b[?2004l"));
}
