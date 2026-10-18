use super::pty::*;
use std::fs;

#[test]
fn bash_readline_and_sushline_ignore_same_invalid_inputrc_variables() {
    let keys = b"abc\x18\x01X\r";
    let inputrc = r#"
set not-a-readline-variable on
set completion-query-items many
set completion-ignore-case maybe
"\C-x\C-a": beginning-of-line
"#;
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_load_same_absolute_inputrc_include() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("inputrc");
    let included = dir.path().join("included.inputrc");
    fs::write(&included, "\"\\C-o\": beginning-of-line\n").expect("write included inputrc");
    fs::write(&main, format!("$include {}\n", included.display())).expect("write main inputrc");
    let keys = b"abc\x0fX\r";

    let bash = run_bash_readline_with_inputrc_path(keys, &main);
    let sushline = run_sushline_harness_with_inputrc_path(keys, &main);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_match_inputrc_include_path_rules() {
    let cwd = tempfile::tempdir().expect("cwd");
    let home = tempfile::tempdir().expect("home");
    let included = "\"\\C-o\": beginning-of-line\n";
    fs::write(cwd.path().join("relative.inputrc"), included).expect("write CWD include");
    fs::write(home.path().join("home.inputrc"), included).expect("write HOME include");
    let home_path = home.path().to_str().expect("HOME path is UTF-8");
    let keys = b"abc\x0fX\r";

    let check = |label: &str, directive: &str, env: &[(&str, &str)], expected: &str| {
        // A sentinel binding proves whether the include replaced the binding.
        let inputrc = format!("\"\\C-o\": \"Q\"\n{directive}\n");
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, env, cwd.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, env, cwd.path());
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{label}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{label}: {sushline}"
        );
    };

    check(
        "CWD-relative include",
        "$include relative.inputrc",
        &[],
        "Xabc",
    );
    check(
        "HOME tilde include",
        "$include ~/home.inputrc",
        &[("HOME", home_path)],
        "Xabc",
    );
    check(
        "literal environment-variable include",
        "$include $HOME/home.inputrc",
        &[("HOME", home_path)],
        "abcQX",
    );
    check("missing include", "$include missing.inputrc", &[], "abcQX");
}

#[test]
fn bash_readline_and_sushline_apply_same_term_condition_inputrc_branch() {
    let keys = b"abc\x0fX\r";
    let inputrc = r#"
$if term=xterm-256color
"\C-o": beginning-of-line
$else
"\C-o": end-of-line
$endif
"#;
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_apply_same_version_mode_and_variable_conditions() {
    let inputrc = r#"
$if version >= 8.0
"\C-o": beginning-of-line
$else
"\C-o": end-of-line
$endif
$if mode=emacs
"\C-p": end-of-line
$endif
set completion-ignore-case on
$if completion-ignore-case=on
"\C-n": beginning-of-line
$endif
"#;
    for keys in [
        b"abc\x0fX\r".as_slice(),
        b"abc\x01\x10X\r".as_slice(),
        b"abc\x0eX\r".as_slice(),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_apply_same_nested_inputrc_conditions() {
    let inputrc = r#"
$if term=not-xterm
$include definitely-not-present.inputrc
"\C-o": end-of-line
$else
  $if version >= 8.0
  "\C-o": beginning-of-line
  $else
  "\C-o": end-of-line
  $endif
$endif
"#;
    let keys = b"abc\x0fX\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_bind_same_named_inputrc_key_sequences() {
    let inputrc = r#"
set force-meta-prefix on
Control-o: beginning-of-line
Meta-p: end-of-line
"#;

    for keys in [b"abc\x0fX\r".as_slice(), b"abc\x01\x1bpX\r".as_slice()] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_match_meta_prefix_input_across_locales() {
    if !c_utf8_locale_available() {
        return;
    }

    let high_bit = b"\xe1\r";
    let escape_prefix = b"\x1ba\r";
    let cases = [
        (
            "C",
            "",
            high_bit.as_slice(),
            "X",
            false,
            "C converts high-bit input",
        ),
        (
            "C.UTF-8",
            "",
            high_bit.as_slice(),
            "X",
            false,
            "UTF-8 keeps 8-bit Meta input",
        ),
        (
            "C.UTF-8",
            "",
            escape_prefix.as_slice(),
            "",
            true,
            "unforced ESC-prefixed Meta is unbound",
        ),
        (
            "C.UTF-8",
            "set force-meta-prefix on\n",
            escape_prefix.as_slice(),
            "X",
            false,
            "forced Meta prefix uses ESC",
        ),
        (
            "C.UTF-8",
            "set force-meta-prefix on\n",
            high_bit.as_slice(),
            "\u{fffd}",
            false,
            "forced Meta prefix leaves a raw high bit literal",
        ),
        (
            "C.UTF-8",
            "set convert-meta off\nset input-meta off\nset meta-flag off\n",
            high_bit.as_slice(),
            "X",
            false,
            "input flags do not strip the received high bit",
        ),
    ];

    for (locale, settings, keys, expected, expect_bell, case) in cases {
        let inputrc = format!("{settings}\"\\M-a\": \"X\"\n");
        let env = [("LC_ALL", locale)];
        let bash = run_bash_readline_with_inputrc_file_and_env(keys, &inputrc, &env);
        let sushline = run_sushline_harness_with_inputrc_and_env(keys, &inputrc, &env);

        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{case}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{case}: bash={bash:?} sushline={sushline:?}"
        );
        assert_eq!(bell_count(&bash) > 0, expect_bell, "{case}: {bash:?}");
        assert_eq!(
            bell_count(&sushline) > 0,
            expect_bell,
            "{case}: {sushline:?}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_apply_same_inputrc_keymap_targets() {
    let inputrc = r#"
set editing-mode vi
set keymap vi-insert
"\C-o": beginning-of-line
set keymap vi-command
q: accept-line
"#;

    for keys in [b"abc\x0fX\r".as_slice(), b"abc\x1bq".as_slice()] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]

fn bash_readline_and_sushline_re_read_init_file_reload_same_bindings() {
    let initial = r#""\C-o": re-read-init-file"#;
    let reloaded = r#""\C-o": re-read-init-file
"\C-p": beginning-of-line"#;
    let keys = b"abc\x0f\x10X\r";

    let bash = run_bash_readline_with_reloaded_inputrc(keys, initial, reloaded);
    let sushline = run_sushline_harness_with_reloaded_inputrc(keys, initial, reloaded);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_dump_same_functions_variables_and_macros() {
    for (command, inputrc, expected) in [
        (
            "dump-functions",
            r#""\C-o": dump-functions"#,
            "beginning-of-line can be found",
        ),
        (
            "dump-variables",
            r#""\C-o": dump-variables"#,
            "editing-mode is set to",
        ),
        (
            "dump-macros",
            "\"\\C-o\": dump-macros\n\"\\C-p\": \"macro\"",
            "outputs macro",
        ),
    ] {
        let keys = b"abc\x0f\r";
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "command={command}\nbash={bash}\nsushline={sushline}"
        );
        assert!(bash.contains(expected), "command={command}\nbash={bash}");
        assert!(
            sushline.contains(expected),
            "command={command}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_print_same_last_keyboard_macro() {
    let inputrc = r#""\C-o": print-last-kbd-macro"#;
    let keys = b"\x18(\x01X\x18)\x0f\r";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert!(bash.contains("\\C-aX"), "{bash}");
    assert!(sushline.contains("\\C-aX"), "{sushline}");
    assert!(!sushline.contains("\"\\C-aX\""), "{sushline}");
}

#[test]
fn bash_readline_and_sushline_terminal_mode_sequences_follow_runtime_boundary() {
    let keys = b"\r";
    let inputrc = "set enable-keypad on\n";
    let terms = [
        Some("xterm-256color"),
        Some("vt100"),
        Some("vt52"),
        Some("dumb"),
        Some("unknown"),
        Some(""),
        None,
    ];

    for term in terms {
        let bash = run_bash_readline_with_term(keys, inputrc, term);
        let sushline = run_sushline_harness_with_term(keys, inputrc, term);
        assert_eq!(
            accepted_line(&bash),
            Some(String::new()),
            "TERM={term:?}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(String::new()),
            "TERM={term:?}: bash={bash:?} sushline={sushline:?}"
        );

        let sequences: &[&str] = match term {
            Some("xterm-256color") | Some("vt100") => &["\x1b[?1h\x1b=", "\x1b[?1l\x1b>"],
            Some("vt52") => &["\x1b=", "\x1b>"],
            _ => &[],
        };
        for sequence in sequences {
            assert!(
                bash.contains(sequence),
                "Bash 5.3 should emit terminfo keypad sequence {sequence:?} for TERM={term:?}: {bash:?}"
            );
            assert!(
                sushline.contains(sequence),
                "Sushline should emit terminfo keypad sequence {sequence:?} for TERM={term:?}: {sushline:?}"
            );
        }
        if let [entry, exit] = sequences {
            for (name, output) in [("Bash 5.3", &bash), ("Sushline", &sushline)] {
                let entry_pos = output.find(entry).unwrap();
                let prompt_pos = output.find(READY_PROMPT).expect("ready prompt");
                let exit_pos = output.find(exit).unwrap();
                let accepted_pos = output.find("SUSHLINE_ACCEPTED:").expect("accepted marker");
                assert!(
                    entry_pos < prompt_pos && prompt_pos < exit_pos && exit_pos < accepted_pos,
                    "{name} should enter keypad mode before the prompt and leave before acceptance, TERM={term:?}: {output:?}"
                );
            }
        }
        if sequences.is_empty() {
            for forbidden in ["\x1b=", "\x1b>"] {
                assert!(
                    !bash.contains(forbidden),
                    "Bash 5.3 unexpectedly emitted keypad sequence {forbidden:?} for TERM={term:?}: {bash:?}"
                );
                assert!(
                    !sushline.contains(forbidden),
                    "Sushline emitted keypad sequence {forbidden:?} without terminfo capability for TERM={term:?}: {sushline:?}"
                );
            }
        }
        for (disabled_inputrc, behavior) in [
            ("set enable-keypad off\n", "explicit off"),
            ("", "default off"),
        ] {
            let bash = run_bash_readline_with_term(keys, disabled_inputrc, term);
            let sushline = run_sushline_harness_with_term(keys, disabled_inputrc, term);
            assert_eq!(
                accepted_line(&bash),
                Some(String::new()),
                "{behavior}, TERM={term:?}: {bash:?}"
            );
            assert_eq!(
                accepted_line(&sushline),
                Some(String::new()),
                "{behavior}, TERM={term:?}: bash={bash:?} sushline={sushline:?}"
            );
            for forbidden in ["\x1b=", "\x1b>"] {
                assert!(
                    !bash.contains(forbidden),
                    "Bash 5.3 emitted keypad sequence {forbidden:?} with {behavior}, TERM={term:?}: {bash:?}"
                );
                assert!(
                    !sushline.contains(forbidden),
                    "Sushline emitted keypad sequence {forbidden:?} with {behavior}, TERM={term:?}: {sushline:?}"
                );
            }
        }

        for (meta_inputrc, setting) in [
            ("", "default"),
            ("set enable-meta-key on\n", "explicit on"),
            ("set enable-meta-key off\n", "explicit off"),
        ] {
            let bash = run_bash_readline_with_term(keys, meta_inputrc, term);
            let sushline = run_sushline_harness_with_term(keys, meta_inputrc, term);
            assert_eq!(
                accepted_line(&bash),
                Some(String::new()),
                "{setting} enable-meta-key, TERM={term:?}: {bash:?}"
            );
            assert_eq!(
                accepted_line(&sushline),
                Some(String::new()),
                "{setting} enable-meta-key, TERM={term:?}: bash={bash:?} sushline={sushline:?}"
            );
            assert_no_meta_key_sequences(&bash, "Bash 5.3", term, setting);
            assert_no_meta_key_sequences(&sushline, "Sushline", term, setting);
        }
    }
}

fn assert_no_meta_key_sequences(
    output: &str,
    implementation: &str,
    term: Option<&str>,
    setting: &str,
) {
    for forbidden in ["\x1b[?1034h", "\x1b[?1034l"] {
        assert!(
            !output.contains(forbidden),
            "{implementation} emitted meta-key mode sequence {forbidden:?} with {setting}, TERM={term:?}: {output:?}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_apply_binding_before_non_utf8_inputrc_comment() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let mut inputrc = br##""\C-o": beginning-of-line # comment "##.to_vec();
    inputrc.push(0xff);
    inputrc.push(b'\n');
    fs::write(&path, inputrc).expect("write inputrc");
    let keys = [b"abc".as_slice(), &[0x0f, b'X', b'\r']].concat();

    let bash = run_bash_readline_with_inputrc_path(&keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&keys, &path);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(bell_count(&bash), 0, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
    assert_eq!(bell_count(&sushline), 0, "{sushline}");
}

#[test]
fn raw_non_utf8_inputrc_key_is_a_documented_deviation_but_escape_matches() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let prefix = b"set convert-meta off\nset input-meta on\nset meta-flag off\n";
    let mut raw_inputrc = prefix.to_vec();
    raw_inputrc.push(b'"');
    raw_inputrc.push(0xff);
    raw_inputrc.extend_from_slice(b"\": beginning-of-line\n");
    fs::write(&path, raw_inputrc).expect("write raw inputrc");
    let keys = [b"abc".as_slice(), &[0xff, b'X', b'\r']].concat();

    let bash = run_bash_readline_with_inputrc_path(&keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&keys, &path);
    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abc\u{fffd}X".to_string()),
        "{sushline}"
    );

    let mut escaped_inputrc = prefix.to_vec();
    escaped_inputrc.extend_from_slice(b"\"\\377\": beginning-of-line\n");
    fs::write(&path, escaped_inputrc).expect("write escaped inputrc");
    let bash = run_bash_readline_with_inputrc_path(&keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&keys, &path);
    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_do_not_strip_invalid_comments_from_set_directives() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let mut inputrc = b"set bell-style none # comment ".to_vec();
    inputrc.push(0xff);
    inputrc.push(b'\n');
    fs::write(&path, inputrc).expect("write inputrc");
    let keys = [b"abc".as_slice(), &[0x07, b'\r']].concat();
    let bash = run_bash_readline_with_inputrc_path(&keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&keys, &path);

    assert_eq!(accepted_line(&bash), Some("abc".to_string()), "{bash}");
    assert_eq!(bell_count(&bash), 1, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abc".to_string()),
        "{sushline}"
    );
    assert_eq!(bell_count(&sushline), 1, "{sushline}");
}

#[test]
fn bash_readline_and_sushline_apply_binding_before_bare_invalid_trailing_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let mut inputrc = br##""\C-o": beginning-of-line trailing "##.to_vec();
    inputrc.push(0xff);
    inputrc.push(b'\n');
    fs::write(&path, inputrc).expect("write inputrc");
    let keys = b"abc\x0fX\r";
    let bash = run_bash_readline_with_inputrc_path(keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(keys, &path);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_apply_quoted_macros_before_non_utf8_trailing_bytes() {
    for line in [
        br##""\C-o": "hello" trailing "##.as_slice(),
        br##""\C-o": 'hello' # comment "##.as_slice(),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("inputrc");
        let mut inputrc = line.to_vec();
        inputrc.push(0xff);
        inputrc.push(b'\n');
        fs::write(&path, inputrc).expect("write inputrc");
        let keys = b"\x0f\r";
        let bash = run_bash_readline_with_inputrc_path(keys, &path);
        let sushline = run_sushline_harness_with_inputrc_path(keys, &path);

        assert_eq!(accepted_line(&bash).as_deref(), Some("hello"), "{bash:?}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("hello"),
            "{sushline:?}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_decode_single_quoted_macro_escapes() {
    let inputrc = br##""\C-o": '\C-aX'"##;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let keys = b"abc\x0f\r";
    let bash = run_bash_readline_with_inputrc_path(keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(keys, &path);

    assert_eq!(accepted_line(&bash).as_deref(), Some("Xabc"), "{bash:?}");
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("Xabc"),
        "{sushline:?}"
    );
}

#[test]
fn bash_readline_and_sushline_apply_quoted_macros_before_valid_trailing_text() {
    for inputrc in [
        "\"\\C-o\": \"hello\" trailing\n",
        "\"\\C-o\": \"hello\" # comment\n",
        "\"\\C-o\": 'hello' # comment\n",
    ] {
        let keys = b"\x0f\r";
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(accepted_line(&bash).as_deref(), Some("hello"), "{bash:?}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("hello"),
            "{sushline:?}"
        );
    }
}
