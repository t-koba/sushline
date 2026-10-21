use std::io;
use std::process::Command;
use std::time::Duration;

use readline::{
    BindQuery, Config, Editor, History, KeyMapName, TerminalEvent, TerminalIo, TerminalSize,
};

#[path = "oracle/host.rs"]
mod host;

#[path = "oracle/host_readline.rs"]
mod host_readline;

fn editor() -> Editor<DummyTerminal> {
    Editor::new(Config::default(), DummyTerminal, History::new())
}

fn bash_oracle_command() -> Command {
    host::require_oracle_host();
    let mut command = Command::new("bash");
    command.env("TERM", "xterm-256color");
    command
}

fn bash_oracle_command_with_inherited_term() -> Command {
    // Use the test process's TERM for comparisons against Sushline defaults,
    // while isolating the oracle from developer-specific inputrc settings.
    host::require_oracle_host();
    let mut command = Command::new("bash");
    command.env("INPUTRC", "/dev/null");
    command
}

#[test]
fn bind_reusable_output_is_stable() {
    let mut line = editor();
    let mut bind = line.bind_api();

    bind.apply_line("\"\\C-o\": \"echo hi\"").unwrap();
    bind.apply_line("\"\\C-x\\C-a\": beginning-of-line")
        .unwrap();

    let bindings = bind.print(BindQuery::PrintReusable);
    let macros = bind.print(BindQuery::PrintMacrosReusable);
    assert!(macros.contains("\"\\C-o\": \"echo hi\""));
    assert!(bindings.contains("\"\\C-x\\C-a\": beginning-of-line"));
}

#[test]
fn bind_x_reusable_output_matches_gnu_readline_oracle() {
    let bash = bash_oracle_command()
        .args([
            "--noprofile",
            "--norc",
            "-i",
            "-c",
            r#"bind -x '"\C-x\C-a": echo hi'; bind -X"#,
        ])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        bash.status.success(),
        "{}",
        String::from_utf8_lossy(&bash.stderr)
    );

    let mut line = editor();
    let mut bind = line.bind_api();
    let sushline = bind
        .apply_builtin_args(&["-x", "\"\\C-x\\C-a\": echo hi", "-X"])
        .unwrap();

    assert_eq!(sushline, String::from_utf8_lossy(&bash.stdout));
}

#[test]
fn bind_p_does_not_print_bind_x_application_commands() {
    let bash = bash_oracle_command()
        .args([
            "--noprofile",
            "--norc",
            "-i",
            "-c",
            r#"bind -x '"\C-x\C-a": echo hi'; bind -p"#,
        ])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        bash.status.success(),
        "{}",
        String::from_utf8_lossy(&bash.stderr)
    );
    assert!(!String::from_utf8_lossy(&bash.stdout).contains("\\C-x\\C-a"));

    let mut line = editor();
    let mut bind = line.bind_api();
    bind.apply_builtin_args(&["-x", "\"\\C-x\\C-a\": echo hi"])
        .unwrap();

    assert!(!bind.print(BindQuery::PrintReusable).contains("\\C-x\\C-a"));
    assert!(
        bind.print(BindQuery::PrintApplicationCommandsReusable)
            .contains("\\C-x\\C-a")
    );
}

#[test]
fn bind_reusable_output_includes_default_bindings_and_unbound_comments() {
    let mut line = editor();
    let bind = line.bind_api();

    let output = bind.print(BindQuery::PrintReusable);
    assert!(output.contains("\"\\C-x\\C-?\": backward-kill-line"));
    assert!(output.contains("\"\\e\\\\\": delete-horizontal-space"));
    assert!(output.contains("\"\\ec\": capitalize-word"));
    assert!(output.contains("\"\\el\": downcase-word"));
    assert!(output.contains("\"\\et\": transpose-words"));
    assert!(output.contains("\"\\eu\": upcase-word"));
    assert!(output.contains("\"\\C-@\": set-mark"));
    assert!(output.contains("\"\\e \": set-mark"));
    assert!(output.contains("\"\\C-x\\C-x\": exchange-point-and-mark"));
    assert!(output.contains("\"\\C-_\": undo"));
    assert!(output.contains("\"\\C-x\\C-u\": undo"));
    assert!(output.contains("\"\\C-x(\": start-kbd-macro"));
    assert!(output.contains("\"\\C-x)\": end-kbd-macro"));
    assert!(output.contains("\"\\C-xe\": call-last-kbd-macro"));
    assert!(output.contains("\"a\": self-insert"));
    assert!(output.contains("\"\\e[200~\": bracketed-paste-begin"));
    assert!(output.contains("# kill-region (not bound)"));
    assert!(output.contains("# alias-expand-line (not bound)"));
}

#[test]
fn default_variables_are_visible_to_bind_api() {
    let mut line = editor();
    let bind = line.bind_api();
    let output = bind.print(BindQuery::PrintVariablesReusable);
    assert!(output.contains("set editing-mode emacs"));
    assert!(output.contains("set keyseq-timeout 500"));
    assert!(output.contains("set completion-query-items 100"));
    assert!(output.contains("set show-all-if-ambiguous off"));
}

#[test]
fn bind_variable_output_matches_gnu_readline_oracle() {
    let reusable = bash_oracle_command_with_inherited_term()
        .args(["--noprofile", "--norc", "-i", "-c", "bind -v"])
        .output()
        .expect("bash must be available for bind oracle tests");
    let descriptive = bash_oracle_command_with_inherited_term()
        .args(["--noprofile", "--norc", "-i", "-c", "bind -V"])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        reusable.status.success(),
        "{}",
        String::from_utf8_lossy(&reusable.stderr)
    );
    assert!(
        descriptive.status.success(),
        "{}",
        String::from_utf8_lossy(&descriptive.stderr)
    );

    let mut line = editor();
    let bind = line.bind_api();
    let reusable = String::from_utf8_lossy(&reusable.stdout).into_owned();
    let descriptive = String::from_utf8_lossy(&descriptive.stdout).into_owned();

    assert_eq!(
        comparable_variable_output_without_colors(&bind.print(BindQuery::PrintVariablesReusable)),
        comparable_variable_output_without_colors(&reusable)
    );
    assert_eq!(
        comparable_variable_output_without_colors(&bind.print(BindQuery::PrintVariables)),
        comparable_variable_output_without_colors(&descriptive)
    );
    let color_names = ["active-region-start-color", "active-region-end-color"];
    assert_eq!(
        selected_variable_lines(&bind.print(BindQuery::PrintVariablesReusable), &color_names),
        selected_variable_lines(&reusable, &color_names),
        "compare terminfo-backed colors under the same inherited TERM"
    );
}

#[test]
fn bind_variable_terminal_defaults_match_gnu_readline_oracle() {
    let output = bash_oracle_command_with_inherited_term()
        .args(["--noprofile", "--norc", "-i", "-c", "bind -v"])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bash = String::from_utf8_lossy(&output.stdout).into_owned();

    let mut line = editor();
    let bind = line.bind_api();
    let sushline = bind.print(BindQuery::PrintVariablesReusable);
    let terminal_variables = [
        "enable-active-region",
        "enable-bracketed-paste",
        "enable-keypad",
        "active-region-start-color",
        "active-region-end-color",
    ];
    assert_eq!(
        selected_variable_lines(&sushline, &terminal_variables),
        selected_variable_lines(&bash, &terminal_variables),
        "TERM-dependent defaults should match Bash under the same TERM"
    );
}

#[test]
fn bind_variable_set_normalization_matches_gnu_readline_oracle() {
    let commands = [
        "set completion-ignore-case maybe",
        "set disable-completion",
        "set history-size many",
        "set completion-query-items many",
        "set completion-display-width many",
        "set completion-prefix-display-length many",
        "set keyseq-timeout many",
        "set bell-style none",
        "set comment-begin \"// x\"",
        "set emacs-mode-string \"\\e[1mE\"",
    ];
    let bash_command = commands
        .iter()
        .map(|command| format!("bind {}", shell_single_quote(command)))
        .chain(std::iter::once("bind -v".to_string()))
        .collect::<Vec<_>>()
        .join("; ");
    let bash = bash_oracle_command()
        .args(["--noprofile", "--norc", "-i", "-c", &bash_command])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        bash.status.success(),
        "{}",
        String::from_utf8_lossy(&bash.stderr)
    );

    let mut line = editor();
    let mut bind = line.bind_api();
    for command in commands {
        bind.apply_line(command).unwrap();
    }
    let sushline = bind.print(BindQuery::PrintVariablesReusable);

    let selected = [
        "bell-style",
        "comment-begin",
        "completion-display-width",
        "completion-ignore-case",
        "completion-prefix-display-length",
        "completion-query-items",
        "disable-completion",
        "emacs-mode-string",
        "history-size",
        "keyseq-timeout",
    ];
    assert_eq!(
        selected_variable_lines(&sushline, &selected),
        selected_variable_lines(&String::from_utf8_lossy(&bash.stdout), &selected)
    );
}

#[test]
fn bind_editing_mode_stores_the_keymap_gnu_stores() {
    let commands = ["set editing-mode vi-command"];
    let bash_command = commands
        .iter()
        .map(|command| format!("bind {}", shell_single_quote(command)))
        .chain(std::iter::once("bind -v".to_string()))
        .collect::<Vec<_>>()
        .join("; ");
    let bash = bash_oracle_command()
        .args(["--noprofile", "--norc", "-i", "-c", &bash_command])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        bash.status.success(),
        "{}",
        String::from_utf8_lossy(&bash.stderr)
    );

    let mut line = editor();
    let mut bind = line.bind_api();
    for command in commands {
        bind.apply_line(command).unwrap();
    }
    let sushline = bind.print(BindQuery::PrintVariablesReusable);

    let selected = ["editing-mode", "keymap"];
    assert_eq!(
        selected_variable_lines(&sushline, &selected),
        selected_variable_lines(&String::from_utf8_lossy(&bash.stdout), &selected)
    );
}

#[test]
fn bind_query_reports_function_bindings() {
    let mut line = editor();
    let bind = line.bind_api();

    let yank = bind.print(BindQuery::QueryFunction("yank".to_string()));
    let unbound = bind.print(BindQuery::QueryFunction("vi-append-eol".to_string()));
    let unknown = bind.print(BindQuery::QueryFunction("not-a-command".to_string()));

    assert_eq!(yank, "yank can be invoked via \"\\C-y\".\n");
    assert_eq!(unbound, "vi-append-eol is not bound to any keys\n");
    assert_eq!(unknown, "not-a-command is not a function\n");
}

#[test]
fn bind_function_listing_includes_implemented_commands() {
    let mut line = editor();
    let bind = line.bind_api();

    let output = bind.print(BindQuery::PrintFunctions);
    assert!(output.contains("kill-line can be found on \"\\C-k\"."));
    assert!(output.contains("yank-pop can be found on \"\\ey\"."));
    assert!(output.contains("universal-argument is not bound to any keys"));
}

#[test]
fn bind_lists_gnu_readline_function_names() {
    let mut line = editor();
    let bind = line.bind_api();

    let output = bind.print(BindQuery::ListFunctionNames);
    assert!(output.lines().count() > 160);
    assert!(output.contains("alias-expand-line\n"));
    assert!(output.contains("history-substring-search-backward\n"));
    assert!(output.contains("vi-yank-to\n"));
    assert!(!output.contains("not-a-command\n"));
}

#[test]
fn bind_function_name_list_matches_gnu_readline_oracle() {
    let output = bash_oracle_command()
        .args(["--noprofile", "--norc", "-i", "-c", "bind -l"])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bash = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut line = editor();
    let bind = line.bind_api();
    let sushline = bind
        .print(BindQuery::ListFunctionNames)
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();

    assert_eq!(sushline, bash);
}

#[test]
fn bind_reusable_default_key_lines_match_gnu_readline_oracle() {
    let output = bash_oracle_command()
        .args(["--noprofile", "--norc", "-i", "-c", "bind -p"])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bash = String::from_utf8_lossy(&output.stdout);

    let mut line = editor();
    let bind = line.bind_api();
    let sushline = bind.print(BindQuery::PrintReusable);

    for line in [
        r#""\C-a": beginning-of-line"#,
        r#""\C-e": end-of-line"#,
        r#""\C-k": kill-line"#,
        r#""\C-y": yank"#,
        r#""\e.": yank-last-arg"#,
        r#""\e[200~": bracketed-paste-begin"#,
    ] {
        assert!(bash.contains(line), "GNU Readline bind -p missing {line}");
        assert!(sushline.contains(line), "sushline bind -p missing {line}");
    }
}

#[test]
fn bind_default_keymap_binding_lines_match_gnu_readline_oracle() {
    for (name, bash_command, keymap) in [
        ("emacs", "bind -p", None),
        (
            "vi-command",
            "bind -m vi-command -p",
            Some(KeyMapName::ViCommand),
        ),
        (
            "vi-insert",
            "bind -m vi-insert -p",
            Some(KeyMapName::ViInsert),
        ),
    ] {
        let output = bash_oracle_command()
            .args(["--noprofile", "--norc", "-i", "-c", bash_command])
            .output()
            .expect("bash must be available for bind oracle tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bash = String::from_utf8_lossy(&output.stdout);

        let mut line = editor();
        let mut bind = line.bind_api();
        if let Some(keymap) = keymap {
            bind.apply_builtin_args(&["-m", keymap.as_str()]).unwrap();
        }
        let sushline = bind.print(BindQuery::PrintReusable);

        assert_eq!(
            default_binding_lines(&sushline),
            default_binding_lines(&bash),
            "{name} default binding lines differ"
        );
    }
}

#[test]
fn bind_vi_command_default_key_lines_match_gnu_readline_oracle() {
    let output = bash_oracle_command()
        .args(["--noprofile", "--norc", "-i", "-c", "bind -m vi-command -p"])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bash = String::from_utf8_lossy(&output.stdout);

    let mut line = editor();
    let mut bind = line.bind_api();
    bind.apply_builtin_args(&["-m", KeyMapName::ViCommand.as_str()])
        .unwrap();
    let sushline = bind.print(BindQuery::PrintReusable);

    for line in [
        r#""\C-d": vi-eof-maybe"#,
        r#""\C-g": abort"#,
        r#""\C-h": backward-char"#,
        r#""\C-n": next-history"#,
        r#""\C-p": previous-history"#,
        "\"#\": insert-comment",
        r#""*": bash-vi-complete"#,
        r#""+": next-history"#,
        r#""-": previous-history"#,
        r#""B": vi-prev-word"#,
        r#""E": vi-end-word"#,
        r#""R": vi-replace"#,
        r#""W": vi-next-word"#,
        r#""_": vi-yank-arg"#,
        r#""b": vi-prev-word"#,
        r#""e": vi-end-word"#,
        r#""r": vi-change-char"#,
        r#""v": vi-edit-and-execute-command"#,
        r#""w": vi-next-word"#,
        r#""x": vi-delete"#,
        r#""X": vi-rubout"#,
        r#""d": vi-delete-to"#,
        r#""p": vi-put"#,
        r#""&": vi-tilde-expand"#,
    ] {
        assert!(
            bash.contains(line),
            "GNU Readline bind -m vi-command -p missing {line}"
        );
        assert!(
            sushline.contains(line),
            "sushline bind -m vi-command -p missing {line}"
        );
    }
    assert!(!bash.contains(r#""\"": vi-set-register"#));
    assert!(!sushline.contains(r#""\"": vi-set-register"#));
}

#[test]
fn bind_vi_insert_default_key_lines_match_gnu_readline_oracle() {
    let output = bash_oracle_command()
        .args(["--noprofile", "--norc", "-i", "-c", "bind -m vi-insert -p"])
        .output()
        .expect("bash must be available for bind oracle tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bash = String::from_utf8_lossy(&output.stdout);

    let mut line = editor();
    let mut bind = line.bind_api();
    bind.apply_builtin_args(&["-m", KeyMapName::ViInsert.as_str()])
        .unwrap();
    let sushline = bind.print(BindQuery::PrintReusable);

    for line in [
        r#""\C-d": vi-eof-maybe"#,
        r#""\C-h": backward-delete-char"#,
        r#""\C-i": complete"#,
        r#""\C-j": accept-line"#,
        r#""\C-m": accept-line"#,
        r#""\C-n": menu-complete"#,
        r#""\C-p": menu-complete-backward"#,
        r#""\C-r": reverse-search-history"#,
        r#""\C-s": forward-search-history"#,
        r#""\C-t": transpose-chars"#,
        r#""\C-u": unix-line-discard"#,
        r#""\C-v": quoted-insert"#,
        r#""\C-w": vi-unix-word-rubout"#,
        r#""\C-y": yank"#,
        r#""\C-_": vi-undo"#,
        r#""\C-?": backward-delete-char"#,
        r#""\e": vi-movement-mode"#,
        r#""\e[200~": bracketed-paste-begin"#,
    ] {
        assert!(
            bash.contains(line),
            "GNU Readline bind -m vi-insert -p missing {line}"
        );
        assert!(
            sushline.contains(line),
            "sushline bind -m vi-insert -p missing {line}"
        );
    }
}

#[test]
fn bind_accepts_dispatched_readline_command_names() {
    let mut line = editor();
    let mut bind = line.bind_api();

    bind.apply_line("\"\\C-x\\C-e\": edit-and-execute-command")
        .unwrap();

    assert_eq!(
        bind.print(BindQuery::QueryFunction(
            "edit-and-execute-command".to_string()
        )),
        "edit-and-execute-command can be invoked via \"\\C-x\\C-e\".\n"
    );
    assert!(
        bind.print(BindQuery::PrintReusable)
            .contains("\"\\C-x\\C-e\": edit-and-execute-command")
    );
}

#[test]
fn every_readline_command_name_can_be_bound() {
    let mut list_line = editor();
    let commands = list_line
        .bind_api()
        .print(BindQuery::ListFunctionNames)
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    for command in commands {
        let mut line = editor();
        let mut bind = line.bind_api();
        bind.apply_line(&format!("\"\\C-o\": {command}"))
            .unwrap_or_else(|err| panic!("{command} should bind: {}", err.message));
        assert!(
            bind.print(BindQuery::QueryFunction(command))
                .contains("can be invoked via")
        );
    }
}

#[test]
fn compatibility_document_covers_bind_commands_and_variables() {
    let compatibility =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../COMPATIBILITY.md"))
            .expect("read compatibility document");

    let mut list_line = editor();
    for command in list_line
        .bind_api()
        .print(BindQuery::ListFunctionNames)
        .lines()
    {
        assert!(
            compatibility.contains(&format!("`{command}`")),
            "COMPATIBILITY.md missing bind command {command}"
        );
    }

    let mut variable_line = editor();
    for line in variable_line
        .bind_api()
        .print(BindQuery::PrintVariablesReusable)
        .lines()
    {
        let Some(name) = line
            .strip_prefix("set ")
            .and_then(|rest| rest.split(' ').next())
        else {
            continue;
        };
        assert!(
            compatibility.contains(&format!("`{name}`")),
            "COMPATIBILITY.md missing bind variable {name}"
        );
    }
}

// COMPATIBILITY.md uses single-backtick spans. Pipes inside them or escaped
// by an odd backslash run are not table separators.
fn markdown_table_cell_count(line: &str) -> Option<usize> {
    let line = line.trim();
    if !line.starts_with('|') {
        return None;
    }

    let (mut cells, mut in_code, mut slashes, mut offset) = (1, false, 0, 0);
    for ch in line.chars() {
        if ch == '\\' {
            slashes += 1;
            offset += ch.len_utf8();
            continue;
        }
        let escaped = slashes % 2 == 1;
        slashes = 0;
        if ch == '`' && !escaped {
            in_code = !in_code;
        } else if ch == '|'
            && !in_code
            && !escaped
            && offset != 0
            && offset + ch.len_utf8() != line.len()
        {
            cells += 1;
        }
        offset += ch.len_utf8();
    }
    assert!(!in_code, "unclosed inline-code span in table row: {line}");
    Some(cells)
}

fn is_markdown_table_separator(line: &str) -> bool {
    line.trim().trim_matches('|').split('|').all(|cell| {
        let dashes = cell.trim().trim_start_matches(':').trim_end_matches(':');
        dashes.len() >= 3 && dashes.bytes().all(|byte| byte == b'-')
    })
}

fn assert_markdown_table_rows_match_headers(markdown: &str) {
    let lines = markdown.lines().collect::<Vec<_>>();
    let mut index = 0;
    while index + 1 < lines.len() {
        let Some(expected) = markdown_table_cell_count(lines[index]) else {
            index += 1;
            continue;
        };
        if !is_markdown_table_separator(lines[index + 1]) {
            index += 1;
            continue;
        }
        assert_eq!(markdown_table_cell_count(lines[index + 1]), Some(expected));
        index += 2;
        while index < lines.len() {
            let Some(actual) = markdown_table_cell_count(lines[index]) else {
                break;
            };
            assert_eq!(
                actual,
                expected,
                "wrong table cell count on line {}",
                index + 1
            );
            index += 1;
        }
    }
}

#[test]
fn markdown_table_scanner_ignores_code_and_escaped_pipes() {
    let table = r#"| Feature | Notes |
| --- | --- |
| extglob | `@(a|b)` |
| escaped | raw\|pipe |"#;
    assert_markdown_table_rows_match_headers(table);
}

#[test]
fn compatibility_document_tables_have_consistent_cell_counts() {
    let compatibility =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../COMPATIBILITY.md"))
            .expect("read compatibility document");
    assert_markdown_table_rows_match_headers(&compatibility);
}

#[test]
fn compatibility_document_classifies_editor_bind_and_history_coverage() {
    let compatibility =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../COMPATIBILITY.md"))
            .expect("read compatibility document");

    let documented_rows = compatibility
        .split("## Compatibility Boundaries")
        .nth(1)
        .expect("compatibility boundary section");
    let known_deviation_rows = documented_rows
        .lines()
        .filter(|line| line.starts_with('|') && line.contains("| Known deviation |"))
        .collect::<Vec<_>>();
    assert!(
        known_deviation_rows
            .iter()
            .any(|row| row.starts_with("| Signal install/catch/reraise |")),
        "COMPATIBILITY.md must retain the | Signal install/catch/reraise | deviation"
    );
    assert!(documented_rows.lines().any(|line| {
        line.starts_with("| Cell width and locale |")
            && line.contains("| Compatible |")
            && line.contains("Bash 5.3")
            && line.contains("PTY runtime")
    }));
    assert!(documented_rows.lines().any(|line| {
        line.starts_with("| Unwrapped ANSI in prompts |")
            && line.contains("| Implemented |")
            && line.contains("Bash 5.3")
            && line.contains("PTY runtime")
    }));
    assert!(documented_rows.lines().any(|line| {
        line.starts_with("| Completion redraw of invalid filename bytes |")
            && line.contains("| Compatible |")
    }));
    assert!(documented_rows.lines().any(|line| line.starts_with(
        "| `menu-complete`, `menu-complete-backward`, `old-menu-complete` | Compatible |"
    )));
    for forbidden in ["| Not implemented |", "| Untested |"] {
        let rows = documented_rows
            .lines()
            .filter(|line| line.starts_with('|') && line.contains(forbidden))
            .collect::<Vec<_>>();
        assert!(rows.is_empty(), "{forbidden} rows remain: {rows:?}");
    }

    // Rows whose first column names a long list of methods are locked by
    // their status alone, so the lock does not depend on the rest of the row.
    for (row_start, status) in [
        ("| Files: `read_history`", "Mixed"),
        ("| File range: `read_history_range`", "Mixed"),
    ] {
        let row = compatibility
            .lines()
            .find(|line| line.starts_with(row_start))
            .unwrap_or_else(|| panic!("COMPATIBILITY.md must keep the {row_start} row"));
        assert!(
            row.contains(&format!("| {status} |")),
            "the {row_start} row must be {status}: {row}"
        );
    }

    for area in [
        "| Basic line editing | Mixed |",
        "| Emacs keymap, bindable names, and `bind` | Mixed |",
        "| vi mode | Mixed |",
        "| Init file/inputrc | Mixed |",
        "| History navigation/search | Mixed |",
        "| History expansion | Mixed |",
        "| History file storage | Mixed |",
        "| `character-search`, `character-search-backward` | Mixed |",
        "| `vi-arg-digit`, `vi-search`, `vi-search-again`, `vi-char-search` | Mixed |",
    ] {
        assert!(
            compatibility.contains(area),
            "COMPATIBILITY.md must classify {area}"
        );
    }
}

#[test]
fn bind_can_unbind_keys_and_commands() {
    let mut line = editor();
    let mut bind = line.bind_api();

    assert!(bind.unbind_key("\"\\C-y\"").unwrap());
    bind.apply_builtin_args(&["-m", "vi-insert"]).unwrap();
    assert!(bind.unbind_key("\"\\C-y\"").unwrap());
    bind.apply_builtin_args(&["-m", "vi-command"]).unwrap();
    assert!(bind.unbind_key("\"\\C-y\"").unwrap());
    assert_eq!(
        bind.print(BindQuery::QueryFunction("yank".to_string())),
        "yank is not bound to any keys\n"
    );

    bind.apply_builtin_args(&["-m", "emacs"]).unwrap();
    assert!(bind.unbind_command("yank-pop").unwrap() > 0);
    assert_eq!(
        bind.print(BindQuery::QueryFunction("yank-pop".to_string())),
        "yank-pop is not bound to any keys\n"
    );
    assert!(bind.unbind_command("not-a-command").is_err());
}

#[test]
fn bind_abnormal_diagnostics_match_gnu_readline_representative_cases() {
    fn bash_bind_stderr(args: &[&str]) -> String {
        let script = format!("bind {}", args.join(" "));
        let output = bash_oracle_command()
            .args(["--noprofile", "--norc", "-c", &script])
            .output()
            .expect("bash must be available for bind diagnostic oracle tests");
        assert!(!output.status.success(), "{script} unexpectedly succeeded");
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    let mut line = editor();
    let mut bind = line.bind_api();

    for (args, expected) in [
        (vec!["-z"], "-z: invalid option"),
        (vec!["-q"], "-q: option requires an argument"),
        (vec!["-m"], "-m: option requires an argument"),
        (vec!["-r"], "-r: option requires an argument"),
        (vec!["-u"], "-u: option requires an argument"),
        (vec!["-x"], "-x: option requires an argument"),
        (vec!["-Z"], "-Z: invalid option"),
        (vec!["-lq"], "-q: option requires an argument"),
        (vec!["-lpq"], "-q: option requires an argument"),
        (
            vec!["-f", "/no/such/file"],
            "/no/such/file: cannot read: No such file or directory",
        ),
        (vec!["-m", "nope"], "`nope': invalid keymap name"),
        (vec!["-u", "nope"], "`nope': unknown function name"),
        (
            vec!["-x", "nope"],
            "nope: first non-whitespace character is not `\"'",
        ),
    ] {
        let bash = bash_bind_stderr(&args);
        let sushline = bind.apply_builtin_args(&args.to_vec()).unwrap_err().message;
        assert!(bash.contains(expected), "{args:?}: {bash}");
        assert_eq!(sushline, expected, "{args:?}");
    }
}

struct DummyTerminal;

impl TerminalIo for DummyTerminal {
    fn enter_raw_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn restore_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn read_event(&mut self, _timeout: Option<Duration>) -> io::Result<TerminalEvent> {
        Ok(TerminalEvent::Timeout)
    }

    fn write(&mut self, _text: &str) -> io::Result<()> {
        Ok(())
    }

    fn write_bytes(&mut self, _bytes: &[u8]) -> io::Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn size(&self) -> io::Result<TerminalSize> {
        Ok(TerminalSize {
            columns: 80,
            rows: 24,
        })
    }

    fn clear_after_cursor(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn move_to_column(&mut self, _column: u16) -> io::Result<()> {
        Ok(())
    }
}

fn comparable_variable_output_without_colors(output: &str) -> String {
    output
        .lines()
        .filter(|line| {
            !line.starts_with("set active-region-start-color ")
                && !line.starts_with("set active-region-end-color ")
                && !line.starts_with("active-region-start-color is set to ")
                && !line.starts_with("active-region-end-color is set to ")
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

fn selected_variable_lines(output: &str, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .filter_map(|name| {
            output
                .lines()
                .find(|line| line.starts_with(&format!("set {name} ")))
                .map(ToString::to_string)
        })
        .collect()
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn default_binding_lines(output: &str) -> Vec<String> {
    let mut lines = output
        .lines()
        .filter(|line| line.starts_with('"'))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    lines.sort();
    lines
}
