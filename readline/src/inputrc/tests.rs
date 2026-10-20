use super::*;
use crate::config::Config;

#[test]
fn reports_invalid_set_directives_like_gnu_readline() {
    let config = Config::default();
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    let diagnostics = InputrcParser::new()
        .parse_str(
            "set bell-style no-such-style\nset no-such-variable on\nset editing-mode vi\n",
            &config,
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].line, 1);
    assert_eq!(diagnostics[0].variable, "bell-style");
    assert_eq!(
        diagnostics[0].message,
        "could not set value to `no-such-style'"
    );
    assert!(diagnostics[0].path.is_none());
    assert_eq!(diagnostics[1].line, 2);
    assert_eq!(diagnostics[1].variable, "no-such-variable");
    assert_eq!(diagnostics[1].message, "unknown variable name");
    // The valid directive after the invalid ones still applies.
    assert_eq!(keymap.current(), KeyMapName::ViInsert);
}

#[test]
fn accepts_the_set_values_gnu_readline_accepts() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    let diagnostics = InputrcParser::new()
        .parse_str(
            "set bell-style on\nset bell-style visible\nset bell-style NONE\nset editing-mode vi-command\nset editing-mode emacs-standard\nset comment-begin ##\n",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    // GNU stores the canonical value of the variables it normalizes.
    assert_eq!(variables["bell-style"], "none");
    assert_eq!(variables["editing-mode"], "emacs");
    assert_eq!(keymap.current(), KeyMapName::EmacsStandard);
}

#[test]
fn unquotes_set_values_like_gnu_readline() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    let diagnostics = InputrcParser::new()
        .parse_str(
            "set comment-begin \"\"\nset bell-style \"none\"\nset editing-mode \"vi-command\"\nset bell-style \"bogus\"\n",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    // GNU reads the value without the surrounding quotes, so the empty
    // value is empty and the rejected value is reported unquoted.
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].variable, "comment-begin");
    assert_eq!(diagnostics[0].message, "could not set value to `'");
    assert_eq!(diagnostics[1].variable, "bell-style");
    assert_eq!(diagnostics[1].message, "could not set value to `bogus'");
    assert_eq!(variables["bell-style"], "none");
    assert_eq!(variables["editing-mode"], "vi");
    // GNU derives the keymap variable from the editing mode.
    assert_eq!(variables["keymap"], "vi-insert");
    assert_eq!(keymap.current(), KeyMapName::ViInsert);
}

#[test]
fn keeps_the_current_bell_style_for_an_empty_quoted_value() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    let diagnostics = InputrcParser::new()
        .parse_str(
            "set bell-style none\nset bell-style \"\"\n",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(variables["bell-style"], "none");
}

#[test]
fn keeps_the_current_bell_style_for_an_empty_bell_style_value() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    let diagnostics = InputrcParser::new()
        .parse_str(
            "set bell-style none\nset bell-style\n",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(variables["bell-style"], "none");
}

#[test]
fn rejects_the_set_values_gnu_readline_rejects() {
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    let diagnostics = InputrcParser::new()
        .parse_str(
            "set comment-begin\nset editing-mode em\nset keymap not-a-keymap\n",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();

    assert_eq!(diagnostics.len(), 3);
    assert_eq!(diagnostics[0].variable, "comment-begin");
    assert_eq!(diagnostics[0].message, "could not set value to `'");
    assert_eq!(diagnostics[1].variable, "editing-mode");
    assert_eq!(diagnostics[1].message, "could not set value to `em'");
    assert_eq!(diagnostics[2].variable, "keymap");
    assert_eq!(
        diagnostics[2].message,
        "could not set value to `not-a-keymap'"
    );
}

#[test]
fn sushline_vi_register_command_is_opt_in_for_inputrc_bindings() {
    let config = Config::default();
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    assert!(!variables.is_on("sushline-vi-registers"));

    InputrcParser::new()
        .parse_str(
            "set keymap vi-command\n\"Q\": vi-set-register",
            &config,
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    assert!(keymap.lookup(KeyMapName::ViCommand, b"Q").is_none());

    InputrcParser::new()
        .parse_str(
            "set sushline-vi-registers on\nset keymap vi-command\n\"Q\": vi-set-register",
            &config,
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    assert!(variables.is_on("sushline-vi-registers"));
    assert_eq!(
        keymap.lookup(KeyMapName::ViCommand, b"Q"),
        Some(&KeyBinding::NamedCommand("vi-set-register".to_string()))
    );
}

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
fn history_search_delimiter_chars_is_not_a_readline_variable() {
    // GNU Readline 8.3 has no `history-search-delimiter-chars` variable
    // (`bind` rejects it as unknown), so setting it stores nothing and the
    // omitted-`?` needle scans to the end of the line regardless.
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "set history-search-delimiter-chars X",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    assert!(!variables.contains_key("history-search-delimiter-chars"));
}

#[test]
fn histchars_family_variables_are_not_readline_variables() {
    // GNU rejects `histchars`, `history-word-delimiters`,
    // `history-no-expand-chars`, and `history-quotes-inhibit-expansion` as
    // unknown, so none of them may be stored.
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "set histchars %~#\nset history-word-delimiters XYZ\nset history-no-expand-chars XYZ\nset history-quotes-inhibit-expansion on",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    for name in [
        "histchars",
        "history-word-delimiters",
        "history-no-expand-chars",
        "history-quotes-inhibit-expansion",
    ] {
        assert!(!variables.contains_key(name), "{name}");
    }
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
fn default_matches_new_for_includes() {
    let dir = tempfile::tempdir().unwrap();
    let include = dir.path().join("included.inputrc");
    fs::write(&include, "set completion-ignore-case on").unwrap();
    let source = format!("$include \"{}\"", include.display());

    for parser in [InputrcParser::new(), InputrcParser::default()] {
        let mut keymap = KeyMap::emacs_default();
        let mut variables = Variables::new();
        parser
            .parse_str(&source, &Config::default(), &mut keymap, &mut variables)
            .unwrap();
        assert!(variables.is_on("completion-ignore-case"));
    }
}

#[test]
fn default_matches_new_for_version_predicates() {
    let source = r#"
        $if version >= 8.0
        z: beginning-of-line
        $else
        z: end-of-line
        $endif
    "#;

    for parser in [InputrcParser::new(), InputrcParser::default()] {
        let mut keymap = KeyMap::emacs_default();
        let mut variables = Variables::new();
        parser
            .parse_str(source, &Config::default(), &mut keymap, &mut variables)
            .unwrap();
        assert_eq!(
            keymap.lookup(KeyMapName::EmacsStandard, b"z"),
            Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
        );
    }
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

#[test]
fn parses_valid_binding_before_non_utf8_trailing_comment() {
    let mut bytes = br##""\C-o": beginning-of-line # comment "##.to_vec();
    bytes.push(0xff);
    bytes.push(b'\n');
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    InputrcParser::new()
        .parse_bytes(&bytes, &Config::default(), &mut keymap, &mut variables)
        .unwrap();

    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
    );
}

#[test]
fn parses_valid_binding_before_bare_non_utf8_trailing_bytes() {
    let mut bytes = br##""\C-o": beginning-of-line trailing "##.to_vec();
    bytes.push(0xff);
    bytes.push(b'\n');
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    InputrcParser::new()
        .parse_bytes(&bytes, &Config::default(), &mut keymap, &mut variables)
        .unwrap();

    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
    );
}

#[test]
fn does_not_strip_non_utf8_comments_from_set_directives() {
    let mut bytes = b"set bell-style none # comment ".to_vec();
    bytes.push(0xff);
    bytes.push(b'\n');
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::default_for_config(&Config::default());

    InputrcParser::new()
        .parse_bytes(&bytes, &Config::default(), &mut keymap, &mut variables)
        .unwrap();

    assert_eq!(variables["bell-style"], "audible");
}

#[test]
fn parses_quoted_macros_before_valid_trailing_text() {
    let inputrc = r#"
"\C-o": "hello" trailing
"\C-p": 'world' # comment
"#;
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    InputrcParser::new()
        .parse_str(inputrc, &Config::default(), &mut keymap, &mut variables)
        .unwrap();

    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Macro(b"hello".to_vec()))
    );
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x10]),
        Some(&KeyBinding::Macro(b"world".to_vec()))
    );
}

#[test]
fn parses_quoted_macros_before_non_utf8_trailing_bytes() {
    for (line, key, expected) in [
        (
            br##""\C-o": "hello" trailing "##.as_slice(),
            &[0x0f][..],
            b"hello".as_slice(),
        ),
        (
            br##""\C-p": 'world' # comment "##.as_slice(),
            &[0x10][..],
            b"world".as_slice(),
        ),
    ] {
        let mut bytes = line.to_vec();
        bytes.push(0xff);
        bytes.push(b'\n');
        let mut keymap = KeyMap::emacs_default();
        let mut variables = Variables::new();

        InputrcParser::new()
            .parse_bytes(&bytes, &Config::default(), &mut keymap, &mut variables)
            .unwrap();

        assert_eq!(
            keymap.lookup(KeyMapName::EmacsStandard, key),
            Some(&KeyBinding::Macro(expected.to_vec()))
        );
    }
}

#[test]
fn binds_raw_non_utf8_byte_inside_double_quoted_key() {
    // Measured Bash 5.3 PTY case: a single raw `0xFF` inside a
    // double-quoted key binds as that key byte; the ASCII `\377` form
    // remains supported by the regular UTF-8 parser.
    let mut bytes = b"\"".to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(b"\": beginning-of-line\n");
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_bytes(&bytes, &Config::default(), &mut keymap, &mut variables)
        .unwrap();
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0xff]),
        Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
    );
    // Escaped `\377` still binds the same byte through the `str` path.
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "\"\\377\": beginning-of-line\n",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0xff]),
        Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
    );
}

#[test]
fn raw_non_utf8_key_respects_inactive_nested_conditionals() {
    let mut bytes = b"$if version < 0.0\n".to_vec();
    bytes.push(b'"');
    bytes.push(0xff);
    bytes.extend_from_slice(b"\": beginning-of-line\n\"\\377\": beginning-of-line\n");
    bytes.extend_from_slice(b"$if version >= 0.0\n\"");
    bytes.push(0xfe);
    bytes.extend_from_slice(b"\": beginning-of-line\n$else\n\"");
    bytes.push(0xfc);
    bytes.extend_from_slice(b"\": beginning-of-line\n$endif\n$else\n\"");
    bytes.push(0xfd);
    bytes.extend_from_slice(b"\": end-of-line\n$endif\n");

    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_bytes(&bytes, &Config::default(), &mut keymap, &mut variables)
        .unwrap();

    for key in [0xff, 0xfe, 0xfc] {
        assert_eq!(
            keymap.lookup(KeyMapName::EmacsStandard, &[key]),
            Some(&KeyBinding::Command(EditCommand::SelfInsert))
        );
    }
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0xfd]),
        Some(&KeyBinding::Command(EditCommand::EndOfLine))
    );
}

#[test]
fn raw_non_utf8_key_does_not_salvage_invalid_binding_values() {
    let mut invalid_command = b"\"".to_vec();
    invalid_command.push(0xff);
    invalid_command.extend_from_slice(b"\": beginning-of-line");
    invalid_command.push(0xff);
    invalid_command.push(b'\n');

    let mut invalid_macro = b"\"".to_vec();
    invalid_macro.push(0xff);
    invalid_macro.extend_from_slice(b"\": \"before ");
    invalid_macro.push(0xff);
    invalid_macro.extend_from_slice(b" after\"\n");

    for bytes in [invalid_command, invalid_macro] {
        let mut keymap = KeyMap::emacs_default();
        let mut variables = Variables::new();
        InputrcParser::new()
            .parse_bytes(&bytes, &Config::default(), &mut keymap, &mut variables)
            .unwrap();
        assert_eq!(
            keymap.lookup(KeyMapName::EmacsStandard, &[0xff]),
            Some(&KeyBinding::Command(EditCommand::SelfInsert))
        );
    }
}

#[test]
fn does_not_salvage_invalid_bytes_inside_a_quoted_macro() {
    let mut bytes = br##""\C-q": "before "##.to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(b" after\"\n");
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();

    InputrcParser::new()
        .parse_bytes(&bytes, &Config::default(), &mut keymap, &mut variables)
        .unwrap();

    assert!(
        keymap
            .lookup(KeyMapName::EmacsStandard, &[0x11])
            .is_some_and(|binding| !matches!(binding, KeyBinding::Macro(_)))
    );
}

#[test]
fn non_utf8_comment_bytes_do_not_fail_surrounding_lines() {
    // Valid lines survive a Latin-1 0xFF comment byte; eight-bit escapes
    // still decode to bytes.
    let bytes = b"set editing-mode emacs\n# \xff\n\"\\C-o\": \"\\377\"\n".to_vec();
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_bytes(&bytes, &Config::default(), &mut keymap, &mut variables)
        .unwrap();
    assert_eq!(variables["editing-mode"], "emacs");
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Macro(vec![0xff]))
    );
    // Same bytes via a file read take the byte path, not read_to_string.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inputrc");
    std::fs::write(&path, &bytes).unwrap();
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_file(&path, &Config::default(), &mut keymap, &mut variables)
        .unwrap();
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x0f]),
        Some(&KeyBinding::Macro(vec![0xff]))
    );
}

#[test]
fn eight_bit_escapes_do_not_abort_later_lines() {
    // `"\\400"` wraps to NUL and `"\\x"` stays literal `x` (Bash 5.3
    // oracle); neither fails the file, so the later `set` still applies.
    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "\"\\400\": beginning-of-line\nset bell-style none",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0x00]),
        Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
    );
    assert_eq!(variables["bell-style"], "none");

    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "\"\\x\": beginning-of-line\nset bell-style none",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, b"x"),
        Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
    );
    assert_eq!(variables["bell-style"], "none");

    let mut keymap = KeyMap::emacs_default();
    let mut variables = Variables::new();
    InputrcParser::new()
        .parse_str(
            "\"\\777\": beginning-of-line\nset bell-style none",
            &Config::default(),
            &mut keymap,
            &mut variables,
        )
        .unwrap();
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsStandard, &[0xff]),
        Some(&KeyBinding::Command(EditCommand::BeginningOfLine))
    );
    assert_eq!(variables["bell-style"], "none");
}

#[test]
fn set_keyword_accepts_case_insensitive_whitespace_runs() {
    for line in [
        "set\tbell-style none\n",
        "set  bell-style none\n",
        "SET bell-style none\n",
        "Set\t bell-style none\n",
    ] {
        let mut keymap = KeyMap::emacs_default();
        let mut variables = Variables::new();
        InputrcParser::new()
            .parse_str(line, &Config::default(), &mut keymap, &mut variables)
            .unwrap_or_else(|err| panic!("line {line:?} must parse: {err:?}"));
        assert_eq!(variables["bell-style"], "none", "line {line:?}");
    }
}
