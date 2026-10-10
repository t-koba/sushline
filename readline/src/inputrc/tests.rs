use super::*;
use crate::config::Config;

#[test]
fn parses_variables_bindings_and_conditionals() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    let config = Config {
        application_name: "Bash".to_string(),
        ..Default::default()
    };
    InputrcParser::new()
        .parse_str(
            r#"
                set editing-mode emacs
                "\C-x\C-a": beginning-of-line
                $if Bash
                "\C-o": "echo hi"
                $endif
                "#,
            &config,
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert_eq!(variables["editing-mode"], "emacs");
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x18, 0x01]),
        Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
    );
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Macro(b"echo hi".to_vec()))
    );
}

#[test]
fn ignores_unknown_variables_and_treats_unknown_bool_values_as_off() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
            .parse_str(
                "set completion-query-items many\nset not-a-readline-variable on\nset completion-ignore-case maybe\nset disable-completion",
                &Config::default(),
                &mut keymap,
                &mut variables,
            )
            .unwrap();
    assert_eq!(variables["completion-query-items"], "0");
    assert!(!variables.contains_key("not-a-readline-variable"));
    assert_eq!(variables["completion-ignore-case"], "off");
    assert_eq!(variables["disable-completion"], "on");
}

#[test]
fn ignores_invalid_editing_mode_and_keymap_values() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
            .parse_str(
                "set editing-mode vi\nset editing-mode readline-but-not-real\nset keymap vi-command\nset keymap not-a-keymap",
                &Config::default(),
                &mut keymap,
                &mut variables,
            )
            .unwrap();
    assert_eq!(variables["editing-mode"], "vi");
    assert_eq!(variables["keymap"], "vi-command");
    assert_eq!(keymap.current(), KeyMapName::ViInsert);
}

#[test]
fn ignores_unknown_directives_and_function_bindings() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "$unknown directive\n\"\\C-x\\C-a\": not-a-real-function\n\"\\C-o\": end-of-line",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert!(
        keymap
            .lookup(KeyMapName::EmacsStandard, &[0x18, 0x01])
            .is_none()
    );
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Command(EditCommand::EndOfLine))
    );
}

#[test]
fn keymap_variable_selects_binding_target_without_changing_runtime_map() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "set editing-mode vi\nset keymap vi-command\nq: accept-line",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert_eq!(keymap.current(), KeyMapName::ViInsert);
    assert!(matches!(
        keymap.lookup(KeyMapName::ViCommand, b"q"),
        Some(KeyBinding::Command(EditCommand::AcceptLine))
    ));
    assert!(matches!(
        keymap.lookup(KeyMapName::ViInsert, b"q"),
        Some(KeyBinding::Command(EditCommand::SelfInsert))
    ));
}

#[test]
fn parses_gnu_style_conditions_and_trailing_function_text() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            r#"
                $if version >= 8.0
                "\C-a": beginning-of-line trailing documentation is ignored
                $endif
                $if mode > vi
                "\C-]": end-of-line
                $endif
                $if mode=emacs
                "\C-o": "mode"
                $endif
                $if completion-ignore-case=on
                "\C-x\C-b": end-of-line
                $endif
                "#,
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x01]),
        Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
    );
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Macro(b"mode".to_vec()))
    );
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x18, 0x02]),
        None
    );
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x1d]),
        Some(&KeyBinding::NamedCommand("character-search".to_string()))
    );
}

#[test]
fn ignores_active_include_read_errors() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "$include definitely-not-present.inputrc\n\"\\C-o\": end-of-line",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Command(EditCommand::EndOfLine))
    );
}

#[test]
fn include_paths_accept_quotes_without_environment_expansion() {
    let dir = tempfile::tempdir().unwrap();
    let include = dir.path().join("included file.inputrc");
    fs::write(&include, "set completion-ignore-case on").unwrap();
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    // Quoted paths with spaces resolve literally.
    InputrcParser::new()
        .parse_str(
            &format!("$include \"{}\"", include.display()),
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    assert_eq!(variables["completion-ignore-case"], "on");
}

#[test]
fn include_paths_leave_dollar_vars_unexpanded() {
    // GNU does not expand $VAR in $include: even when the var points at a
    // valid dir, the literal `$...` path misses and is skipped.
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("included.inputrc"),
        "set completion-ignore-case on",
    )
    .unwrap();
    unsafe {
        std::env::set_var("SUSHLINE_INPUTRC_INCLUDE_DIR", dir.path());
    }
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "$include \"$SUSHLINE_INPUTRC_INCLUDE_DIR/included.inputrc\"\n\"\\C-o\": end-of-line",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    unsafe {
        std::env::remove_var("SUSHLINE_INPUTRC_INCLUDE_DIR");
    }
    // Would be `on` under env expansion; stays absent when left literal.
    assert!(!variables.contains_key("completion-ignore-case"));
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Command(EditCommand::EndOfLine))
    );
}

#[test]
fn unknown_tilde_user_include_is_silently_skipped() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "$include ~sushline-nonexistent-user-xyz/definitely-not-present.inputrc\n\"\\C-o\": end-of-line",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Command(EditCommand::EndOfLine))
    );
}

#[test]
fn cyclic_include_hits_depth_protection() {
    let dir = tempfile::tempdir().unwrap();
    let loop_file = dir.path().join("loop.inputrc");
    fs::write(&loop_file, format!("$include {}", loop_file.display())).unwrap();
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    let err = InputrcParser::new()
        .parse_file(&loop_file, &Config::default(), &mut keymap, &mut variables)
        .expect_err("cyclic $include must hit depth protection");
    assert!(
        err.message.contains("include depth exceeded"),
        "unexpected error: {err:?}"
    );
}
