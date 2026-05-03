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
    assert!(!variables.contains_key("completion-query-items"));
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
    assert!(keymap.lookup(KeyMapName::ViInsert, b"q").is_none());
}

#[test]
fn parses_gnu_style_conditions_and_trailing_function_text() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    variables.insert("completion-ignore-case".to_string(), "on".to_string());
    InputrcParser::new()
        .parse_str(
            r#"
                $if version >= 8.0
                "\C-a": beginning-of-line trailing documentation is ignored
                $endif
                $if mode > vi
                "\C-]": end-of-line
                $endif
                $if completion-ignore-case=on
                "\C-o": "case"
                $endif
                $if editing-mode != vi
                "\C-e": end-of-line
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
        Some(&KeyBinding::Macro(b"case".to_vec()))
    );
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x05]),
        Some(&KeyBinding::Command(EditCommand::EndOfLine))
    );
    assert_eq!(keymap.lookup(KeyMapName::EmacsStandard, &[0x1d]), None);
}

#[test]
fn reports_active_include_read_errors() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    let err = InputrcParser::new()
        .parse_str(
            "$include definitely-not-present.inputrc",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap_err();
    assert_eq!(err.line, 1);
    assert!(err.message.contains("cannot include"));
}

#[test]
fn include_paths_accept_quotes_and_environment_expansion() {
    let dir = tempfile::tempdir().unwrap();
    let include = dir.path().join("included file.inputrc");
    fs::write(&include, "set completion-ignore-case on").unwrap();
    unsafe {
        std::env::set_var("SUSHLINE_INPUTRC_INCLUDE_DIR", dir.path());
    }
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "$include \"$SUSHLINE_INPUTRC_INCLUDE_DIR/included file.inputrc\"",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    unsafe {
        std::env::remove_var("SUSHLINE_INPUTRC_INCLUDE_DIR");
    }
    assert_eq!(variables["completion-ignore-case"], "on");
}
