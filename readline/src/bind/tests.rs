use super::*;
use crate::keymap::KeyMap;

#[test]
fn bind_api_prints_stable_reusable_bindings() {
    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());
    api.apply_line("\"\\C-o\": \"echo hi\"").unwrap();
    let printed = api.print(BindQuery::PrintMacrosReusable);
    assert_eq!(printed, "\"\\C-o\": \"echo hi\"\n");
}

#[test]
fn bind_x_builtin_args_query_and_remove_application_commands() {
    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());

    let output = api
        .apply_builtin_args(&["-x", "\"\\C-o\": \"echo $READLINE_LINE\"", "-X"])
        .unwrap();
    assert_eq!(output, "\"\\C-o\": \"echo $READLINE_LINE\"\n");
    assert_eq!(
        api.print(BindQuery::PrintApplicationCommands),
        "\"\\C-o\" executes `echo $READLINE_LINE`\n"
    );
    assert!(api.unbind_application_command("\"\\C-o\"").unwrap());
    assert_eq!(api.print(BindQuery::PrintApplicationCommandsReusable), "");
}

#[test]
fn bind_builtin_args_cover_common_query_options() {
    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());

    let output = api.apply_builtin_args(&["-q", "yank"]).unwrap();
    assert_eq!(output, "yank can be invoked via \"\\C-y\".\n");
    api.apply_builtin_args(&["-r", "\"\\C-y\""]).unwrap();
    assert_eq!(
        api.apply_builtin_args(&["-q", "yank"]).unwrap(),
        "yank can be invoked via \"\\C-y\".\n"
    );
    api.apply_builtin_args(&["-m", "vi-insert", "-r", "\"\\C-y\""])
        .unwrap();
    assert_eq!(
        api.apply_builtin_args(&["-q", "yank"]).unwrap(),
        "yank is not bound to any keys\n"
    );
}

#[test]
fn bind_f_builtin_arg_reads_inputrc_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inputrc");
    std::fs::write(
        &path,
        "\"\\C-o\": beginning-of-line\nset completion-ignore-case on\n",
    )
    .unwrap();

    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());
    api.apply_builtin_args(&["-f", path.to_str().unwrap()])
        .unwrap();

    let query = api.print(BindQuery::QueryFunction("beginning-of-line".to_string()));
    assert!(query.contains("\"\\C-a\""));
    assert!(query.contains("\"\\C-o\""));
    assert_eq!(vars["completion-ignore-case"], "on");
}

#[test]
fn bind_f_uses_runtime_config_for_conditionals() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("inputrc");
    std::fs::write(&file, "$if custom-app\n\"\\C-a\": end-of-line\n$endif\n").unwrap();

    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let config = Config {
        application_name: "custom-app".to_string(),
        ..Default::default()
    };
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &config);
    api.apply_builtin_args(&["-f", file.to_str().unwrap()])
        .unwrap();

    assert!(
        api.print(BindQuery::QueryFunction("end-of-line".to_string()))
            .contains("\"\\C-a\"")
    );
}

#[test]
fn bind_f_resolves_relative_includes_from_inputrc_directory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inputrc");
    let included = dir.path().join("included");
    std::fs::write(&included, "\"\\C-p\": end-of-line\n").unwrap();
    std::fs::write(&path, "$include included\n").unwrap();

    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());
    api.apply_builtin_args(&["-f", path.to_str().unwrap()])
        .unwrap();

    let query = api.print(BindQuery::QueryFunction("end-of-line".to_string()));
    assert!(query.contains("\"\\C-p\""));
}

#[test]
fn bind_x_accepts_named_key_specs() {
    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());
    api.apply_builtin_args(&["-x", "Control-x: echo bound"])
        .unwrap();
    assert!(
        api.print(BindQuery::PrintApplicationCommands)
            .contains("echo bound")
    );
}

#[test]
fn bind_builtin_args_support_compound_print_options() {
    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());

    let output = api.apply_builtin_args(&["-lp"]).unwrap();
    assert!(output.starts_with("abort\naccept-line\n"));
    assert!(output.contains("\"\\C-a\": beginning-of-line"));
}

#[test]
fn bind_builtin_args_reject_compound_options_that_need_arguments() {
    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());

    assert_eq!(
        api.apply_builtin_args(&["-lq"]).unwrap_err().message,
        "-q: option requires an argument"
    );
}
