//! The bind checks that need no oracle host.
//!
//! They compare Sushline's own bind output, its default key lines and its
//! default variable values against expectations written in the test, so the
//! CI job that does not build the pinned Bash and Readline host builds and
//! runs them.

use readline::{BindQuery, Config, Editor, History, Terminal};

fn editor() -> Editor<Terminal> {
    Editor::new(Config::default(), Terminal::default(), History::new())
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
fn bind_query_reports_function_bindings() {
    let mut line = editor();
    let bind = line.bind_api();

    let yank = bind.print(BindQuery::QueryFunction("yank".to_string()));
    let unbound = bind.print(BindQuery::QueryFunction("vi-append-eol".to_string()));
    let unknown = bind.print(BindQuery::QueryFunction("not-a-command".to_string()));

    assert_eq!(yank, "yank can be invoked via \"\\C-y\".\n");
    assert_eq!(unbound, "vi-append-eol is not bound to any keys.\n");
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
        "yank is not bound to any keys.\n"
    );

    bind.apply_builtin_args(&["-m", "emacs"]).unwrap();
    assert!(bind.unbind_command("yank-pop").unwrap() > 0);
    assert_eq!(
        bind.print(BindQuery::QueryFunction("yank-pop".to_string())),
        "yank-pop is not bound to any keys.\n"
    );
    assert!(bind.unbind_command("not-a-command").is_err());
}
