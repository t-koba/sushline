use super::*;
use crate::keymap::KeyMap;

#[test]
fn bind_api_gates_vi_register_command_and_tables_on_inputrc_variable() {
    let mut keymap = KeyMap::emacs_default();
    let config = Config::default();
    let mut vars = Variables::default_for_config(&config);
    assert!(!vars.is_on("sushline-vi-registers"));
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &config);

    assert!(
        !api.print(BindQuery::ListFunctionNames)
            .lines()
            .any(|name| name == "vi-set-register")
    );
    assert_eq!(
        api.print(BindQuery::QueryFunction("vi-set-register".to_string())),
        "vi-set-register is not a function\n"
    );
    assert!(api.apply_line("\"Q\": vi-set-register").is_err());
    assert!(api.unbind_command("vi-set-register").is_err());

    api.apply_line("set sushline-vi-registers on").unwrap();
    assert!(
        api.print(BindQuery::PrintVariablesReusable)
            .lines()
            .any(|line| line == "set sushline-vi-registers on")
    );
    api.apply_line("set keymap vi-command").unwrap();
    api.apply_line("\"Q\": vi-set-register").unwrap();
    assert!(
        api.print(BindQuery::ListFunctionNames)
            .lines()
            .any(|name| name == "vi-set-register")
    );
    assert!(
        api.print(BindQuery::PrintFunctions)
            .contains("vi-set-register can be found on \"Q\".")
    );
    assert_eq!(
        api.print(BindQuery::QueryFunction("vi-set-register".to_string())),
        "vi-set-register can be invoked via \"Q\".\n"
    );
    assert!(
        api.print(BindQuery::PrintReusable)
            .contains("\"Q\": vi-set-register")
    );

    api.apply_line("set sushline-vi-registers off").unwrap();
    assert!(
        !api.print(BindQuery::ListFunctionNames)
            .lines()
            .any(|name| name == "vi-set-register")
    );
    assert!(
        !api.print(BindQuery::PrintReusable)
            .contains("vi-set-register")
    );
}

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
    assert_eq!(output, "\"\\C-o\" \"echo $READLINE_LINE\"\n");
    assert_eq!(
        api.print(BindQuery::PrintApplicationCommands),
        "\"\\C-o\" executes `echo $READLINE_LINE`\n"
    );
    assert!(api.unbind_application_command("\"\\C-o\"").unwrap());
    assert_eq!(api.print(BindQuery::PrintApplicationCommandsReusable), "");
}

#[test]
fn bind_x_reusable_output_can_be_read_back() {
    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());
    let output = api
        .apply_builtin_args(&["-x", "\"\\C-x\\C-a\": echo hi", "-X"])
        .unwrap();

    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());
    api.apply_builtin_args(&["-x", output.trim()]).unwrap();

    assert_eq!(
        api.print(BindQuery::PrintApplicationCommandsReusable),
        output
    );
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
        "yank is not bound to any keys.\n"
    );
    api.apply_builtin_args(&["-m", "vi-insert", "-r", "\"\\C-y\""])
        .unwrap();
    assert_eq!(
        api.apply_builtin_args(&["-q", "yank"]).unwrap(),
        "yank is not bound to any keys.\n"
    );
    api.apply_builtin_args(&["-m", "vi-command", "-r", "\"\\C-y\""])
        .unwrap();
    assert_eq!(
        api.apply_builtin_args(&["-q", "yank"]).unwrap(),
        "yank is not bound to any keys.\n"
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
fn bind_f_resolves_absolute_includes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inputrc");
    let included = dir.path().join("included");
    std::fs::write(&included, "\"\\C-p\": end-of-line\n").unwrap();
    std::fs::write(&path, format!("$include {}\n", included.display())).unwrap();

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

#[test]
fn bind_api_emacs_submap_queries_and_mutations_use_relative_sequences() {
    let mut keymap = KeyMap::emacs_default();
    let mut vars = Variables::new();
    let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());
    api.apply_builtin_args(&["-m", "emacs-meta", r#""a": "META""#])
        .unwrap();
    assert_eq!(
        api.print(BindQuery::PrintMacrosReusable),
        "\"a\": \"META\"\n"
    );
    api.apply_builtin_args(&["-m", "emacs", r#""\ea": "ROOT""#])
        .unwrap();
    assert_eq!(
        api.print(BindQuery::PrintMacrosReusable),
        "\"\\ea\": \"ROOT\"\n"
    );
    api.apply_builtin_args(&["-m", "emacs-meta"]).unwrap();
    assert_eq!(
        api.print(BindQuery::PrintMacrosReusable),
        "\"a\": \"ROOT\"\n"
    );
    assert!(api.unbind_key("a").unwrap());
    assert_eq!(api.print(BindQuery::PrintMacrosReusable), "");
    api.apply_line(r#""\C-xa": beginning-of-line"#).unwrap();
    assert!(
        api.print(BindQuery::QueryFunction("beginning-of-line".into()))
            .contains(r#""\C-xa""#)
    );
    assert!(
        api.print(BindQuery::PrintReusable)
            .contains(r#""\C-xa": beginning-of-line"#)
    );
    api.unbind_command("beginning-of-line").unwrap();
    assert_eq!(
        api.print(BindQuery::QueryFunction("beginning-of-line".into())),
        "beginning-of-line is not bound to any keys.\n"
    );
    // Application-command print/remove paths use the same view as macros/functions.
    api.bind_application_command("z", "app").unwrap();
    assert_eq!(
        api.print(BindQuery::PrintApplicationCommandsReusable),
        "\"z\" \"app\"\n"
    );
    api.apply_builtin_args(&["-m", "emacs"]).unwrap();
    assert!(api.unbind_application_command(r#""\ez""#).unwrap());
    api.apply_builtin_args(&["-m", "emacs-ctlx", r#""a": "CTLX""#])
        .unwrap();
    assert_eq!(
        api.print(BindQuery::PrintMacrosReusable),
        "\"a\": \"CTLX\"\n"
    );
    api.apply_builtin_args(&["-m", "emacs", "-r", r#""\C-xa""#])
        .unwrap();
    assert_eq!(api.print(BindQuery::PrintMacrosReusable), "");
}

#[test]
fn bind_unbound_query_period_does_not_change_function_listing() {
    for map in [
        "emacs-standard",
        "emacs-meta",
        "emacs-ctlx",
        "vi-insert",
        "vi-command",
    ] {
        let mut keymap = KeyMap::emacs_default();
        let mut vars = Variables::new();
        let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());
        api.apply_builtin_args(&["-m", map, "-u", "abort"]).unwrap();
        assert_eq!(
            api.print(BindQuery::QueryFunction("abort".into())),
            "abort is not bound to any keys.\n",
            "{map}"
        );
        assert_eq!(
            api.print(BindQuery::PrintFunctions)
                .lines()
                .find(|line| line.starts_with("abort ")),
            Some("abort is not bound to any keys"),
            "{map}"
        );
    }
}

#[test]
fn bind_submap_macro_formats_keep_ordinary_order_and_put_fallback_last() {
    for (map, prefix) in [("emacs-meta", r"\e"), ("emacs-ctlx", r"\C-x")] {
        let mut keymap = KeyMap::emacs_default();
        let mut vars = Variables::new();
        let mut api = BindApi::with_config(&mut keymap, &mut vars, &Config::default());
        let root = format!(r#""{prefix}": "ROOT""#);
        api.apply_builtin_args(&[
            "-m",
            "emacs-standard",
            &root,
            "-m",
            map,
            r#""z": "LAST""#,
            r#""\C-xa": "CHILD""#,
            r#""a": "FIRST""#,
        ])
        .unwrap();
        assert_eq!(
            api.print(BindQuery::PrintMacrosReusable),
            "\"\\C-xa\": \"CHILD\"\n\"a\": \"FIRST\"\n\"z\": \"LAST\"\n\"\": \"ROOT\"\n",
            "{map}"
        );
        assert_eq!(
            api.print(BindQuery::PrintMacros),
            "\\C-xa outputs CHILD\na outputs FIRST\nz outputs LAST\n outputs ROOT\n",
            "{map}"
        );
    }
}
