use super::pty::*;
use std::fs;

#[test]
fn bash_readline_and_sushline_accept_same_basic_emacs_edit() {
    let keys = b"abc\x01X\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_and_sushline_ctrl_c_signal_path_wins_over_inputrc_macro() {
    let inputrc = "set bind-tty-special-chars on\n\"\\C-c\": \"Q\"\n";
    let keys = b"abc\x03X\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_until(keys, inputrc, "SUSHLINE_INTERRUPTED");

    assert!(bash.contains("^C"), "{bash}");
    assert_eq!(accepted_line(&bash), None);
    assert!(sushline.contains("^C"), "{sushline}");
    assert!(sushline.contains("SUSHLINE_INTERRUPTED"), "{sushline}");
    assert_eq!(accepted_line(&sushline), None);
}

#[test]
fn sushline_ctrl_c_signal_interrupts_readline_without_io_error() {
    let sushline = run_sushline_harness_until(b"abc\x03", "", "SUSHLINE_INTERRUPTED");

    assert!(sushline.contains("^C"), "{sushline}");
    assert!(sushline.contains("SUSHLINE_INTERRUPTED"), "{sushline}");
    assert!(!sushline.contains("Interrupted system call"), "{sushline}");
    assert!(!sushline.contains("SUSHLINE_ERROR"), "{sushline}");
}

#[test]
fn bash_and_sushline_ctrl_c_return_to_next_prompt_without_partial_line() {
    let bash = run_bash_interactive_after_ctrl_c();
    let sushline = run_sushline_harness_after_ctrl_c();

    assert!(bash.contains("^C"), "{bash}");
    assert!(sushline.contains("^C"), "{sushline}");
    assert!(bash.contains(&format!("{READY_PROMPT}^C")), "{bash}");
    assert!(
        sushline.contains(&format!("{READY_PROMPT}^C")),
        "{sushline}"
    );
    assert!(bash.contains("SUSHLINE_ACCEPTED:ok"), "{bash}");
    assert_eq!(accepted_numbered_line(&sushline, 2), Some("ok".to_string()));
    assert!(!sushline.contains("Interrupted system call"), "{sushline}");
    assert!(!sushline.contains("SUSHLINE_ERROR"), "{sushline}");
}

#[test]
fn bash_and_sushline_tty_special_binding_precedence_is_a_scoped_deviation() {
    let keys = b"ab\x7f\r";
    for (enabled, sushline_expected) in [("on", "a"), ("off", "abQ")] {
        let inputrc = format!("set bind-tty-special-chars {enabled}\n\"\\x7f\": \"Q\"\n");
        let bash = run_bash_readline_with_inputrc_file(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        assert_eq!(accepted_line(&bash).as_deref(), Some("abQ"));
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(sushline_expected),
            "bind-tty-special-chars {enabled}"
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
#[test]
fn bash_and_sushline_tty_special_binding_precedence_covers_tty_specials() {
    let cases = [
        (
            "kill",
            b"ab\x15X\r".as_slice(),
            "\"\\C-u\": \"Q\"",
            "abQX",
            "X",
        ),
        (
            "word erase",
            b"alpha beta\x17X\r".as_slice(),
            "\"\\C-w\": \"Q\"",
            "alpha betaQX",
            "alpha X",
        ),
        (
            "eof",
            b"ab\x04X\r".as_slice(),
            "\"\\C-d\": \"Q\"",
            "abQX",
            "abX",
        ),
    ];
    for (label, keys, binding, bash_expected, sushline_expected) in cases {
        let inputrc = format!("set bind-tty-special-chars on\n{binding}\n");
        let bash = run_bash_readline_with_inputrc_file(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(bash_expected),
            "{label}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(sushline_expected),
            "{label}"
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
#[test]
fn bash_and_sushline_ctrl_r_enters_reverse_search_with_bind_tty_special_chars_on_and_off() {
    let keys = b"ab\x12X\r";
    for enabled in ["on", "off"] {
        let inputrc = format!("set bind-tty-special-chars {enabled}\n");
        let bash = run_bash_readline_with_inputrc_file(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        for (implementation, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert!(
                output.contains("reverse-i-search"),
                "{implementation}, bind-tty-special-chars {enabled}: {output}"
            );
            assert_eq!(
                accepted_line(output).as_deref(),
                Some("ab"),
                "{implementation}, bind-tty-special-chars {enabled}"
            );
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn bash_and_sushline_keep_ixon_flow_control_for_ctrl_s_ctrl_q() {
    let keys = b"ab\x13X\x11\r";
    let inputrc = "set bind-tty-special-chars on\n";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash).as_deref(), Some("abX"), "{bash}");
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("abX"),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_backspace_edit() {
    let keys = b"abc\x7fd\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("abd".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abd".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_word_motion_edit() {
    let keys = b"one two three\x1bbX\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(
        accepted_line(&bash),
        Some("one two Xthree".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("one two Xthree".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_wide_character_edit() {
    let keys = "あb\u{2}X\r".as_bytes();
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("あXb".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("あXb".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_and_sushline_c_locale_multibyte_motion_is_scoped_difference() {
    // This is PTY runtime evidence for the tested Bash settings, not GNU source proof.
    let keys = "中x\x02\x02\r".as_bytes();
    let inputrc = "set input-meta on\nset output-meta on\nset convert-meta off\nset meta-flag off\nset byte-oriented off\n";
    let env = [("LC_ALL", "C")];
    let bash = run_bash_readline_with_inputrc_file_and_env(keys, inputrc, &env);
    let sushline = run_sushline_harness_with_inputrc_and_env(keys, inputrc, &env);

    assert_eq!(accepted_line(&bash), Some("中x".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("中x".to_string()),
        "{sushline}"
    );
    assert!(
        bash.contains("中x\x08\x08"),
        "expected two single backspaces immediately after input; {bash}"
    );

    // C-locale rendering emits 中's three UTF-8 bytes, while editing moves
    // over it as one scalar. These output positions do not assert a physical
    // terminal width for the glyph.
    let after_first_motion = format!("{READY_PROMPT}中x\u{1b}[K\u{1b}[19G");
    let after_second_motion = format!("{READY_PROMPT}中x\u{1b}[K\u{1b}[16G");
    assert!(
        sushline.contains(&after_first_motion),
        "expected redraw cursor after 中 following first Ctrl-B; {sushline}"
    );
    assert!(
        sushline.contains(&after_second_motion),
        "expected redraw cursor at prompt end following second Ctrl-B; {sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_combining_character_edit() {
    let keys = "e\u{301}b\u{2}X\r".as_bytes();
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_kill_and_yank_edit() {
    let keys = b"abc def\x15X\x18\x01\r";
    let inputrc = r#""\C-x\C-a": yank"#;
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("Xabc def".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc def".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_keep_last_macro_across_reads() {
    let keys = b"\x18(ab\x18)\r\x18e\r";
    let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(keys, "", &[]);
    let sushline = run_sushline_harness_two_reads_with_inputrc_and_history(keys, "", &[]);

    assert_eq!(
        accepted_numbered_line(&sushline, 1),
        accepted_numbered_line(&bash, 1),
        "first read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 1),
        Some("ab".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&sushline, 2),
        accepted_numbered_line(&bash, 2),
        "second read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 2),
        Some("a".to_string()),
        "{bash}"
    );
}

#[test]
fn bash_readline_and_sushline_keep_killed_text_across_reads() {
    let keys = b"abc\x15\r\x19\r";
    let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(keys, "", &[]);
    let sushline = run_sushline_harness_two_reads_with_inputrc_and_history(keys, "", &[]);

    assert_eq!(
        accepted_numbered_line(&bash, 1),
        Some(String::new()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&sushline, 1),
        accepted_numbered_line(&bash, 1),
        "first read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 2),
        Some("abc".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&sushline, 2),
        accepted_numbered_line(&bash, 2),
        "second read\nbash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_hex_inputrc_key_binding() {
    let keys = b"abc\x18\x01X\r";
    let inputrc = r#""\x18\x01": beginning-of-line"#;
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_octal_inputrc_key_binding() {
    let keys = b"abc\x18\x01X\r";
    let inputrc = r#""\030\001": beginning-of-line"#;
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_kill_line_edit() {
    let keys = b"abc def\x01\x06\x06\x0bX\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("abX".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abX".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_transpose_edit() {
    let keys = b"ab\x18\x01\r";
    let inputrc = r#""\C-x\C-a": transpose-chars"#;
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("ba".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("ba".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_delete_horizontal_space_edit() {
    let keys = b"one   two\x1bb\x1b\\\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("onetwo".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("onetwo".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_case_word_edits() {
    for keys in [
        b"one two\x01\x1bu\x1bc\r".as_slice(),
        b"foo-bar baz\x01\x1b2\x1bu\r".as_slice(),
        b"FOO BAR\x1b-1\x1bl\r".as_slice(),
        b"foo BAR\x1b-1\x1bc\r".as_slice(),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_transpose_words_edit() {
    let keys = b"one two\x1bt\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("two one".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("two one".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_backward_kill_line_edit() {
    let keys = b"abc def\x1bb\x18\x7fX\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("Xdef".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xdef".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_kill_whole_line_edit() {
    let keys = b"abc\x18\x01X\r";
    let inputrc = r#""\C-x\C-a": kill-whole-line"#;
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("X".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("X".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_revert_line_edit() {
    let keys = b"abc\x1brX\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("X".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("X".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_exchange_point_and_mark_edit() {
    let keys = b"abc\x01\x00\x05\x18\x18X\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_kill_region_edit() {
    let keys = b"abc\x01\x00\x06\x06\x18\x02X\r";
    let inputrc = "\"\\C-x\\C-b\": kill-region";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("Xc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_copy_region_as_kill_edit() {
    let keys = b"abc\x01\x00\x06\x06\x18\x02\x05\x19\r";
    let inputrc = "\"\\C-x\\C-b\": copy-region-as-kill";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("abcab".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abcab".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_copy_word_commands() {
    for (command, keys) in [
        ("copy-backward-word", b"one two\x0f\x19\r".as_slice()),
        ("copy-forward-word", b"one two\x01\x0f\x05\x19\r".as_slice()),
    ] {
        let inputrc = format!(r#""\C-o": {command}"#);
        let bash = run_bash_readline_with_bindings(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "command={command}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_simple_undo_edit() {
    let keys = b"abc\x1f\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(accepted_line(&sushline), Some(String::new()), "{sushline}");
}

#[test]
fn bash_readline_and_sushline_accept_same_short_multibyte_undo_split() {
    // GNU groups only single-byte inserts: `aé` plus undo leaves `a`.
    let mut keys = Vec::from("aé".as_bytes());
    keys.extend_from_slice(b"\x1f\r");
    let bash = run_bash_readline(&keys);
    let sushline = run_sushline_harness(&keys);

    assert_eq!(accepted_line(&bash), Some("a".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("a".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_keyboard_macro_self_insert_replay() {
    let keys = b"\x18(abc\x18)\x18e\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("abca".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abca".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_keyboard_macro_with_command() {
    let keys = b"ab\x18(\x01X\x18)\x18e\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("XXab".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("XXab".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_and_sushline_append_the_previous_macro_for_explicit_start_arguments() {
    let universal_inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    for (name, keys, inputrc, expected) in [
        (
            "no argument starts a fresh definition",
            b"\x18(a\x18)\x18(b\x18)\x18e\r".as_slice(),
            "",
            "abb",
        ),
        (
            "explicit one",
            b"\x18(a\x18)\x1b1\x18(b\x18)\x18e\r".as_slice(),
            "",
            "aabaab",
        ),
        (
            "explicit two",
            b"\x18(a\x18)\x1b2\x18(b\x18)\x18e\r".as_slice(),
            "",
            "aabaab",
        ),
        (
            "explicit zero",
            b"\x18(a\x18)\x1b0\x18(b\x18)\x18e\r".as_slice(),
            "",
            "aabaab",
        ),
        (
            "sign-only negative argument",
            b"\x18(a\x18)\x1b-\x18(b\x18)\x18e\r".as_slice(),
            "",
            "aabaab",
        ),
        (
            "universal argument only",
            b"\x18(a\x18)\x0f\x18(b\x18)\x18e\r".as_slice(),
            universal_inputrc,
            "abb",
        ),
        (
            "explicit count followed by universal argument",
            b"\x18(a\x18)\x1b1\x0f\x18(b\x18)\x18e\r".as_slice(),
            universal_inputrc,
            "aabaab",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline:?}"
        );
    }
}

#[test]
fn bash_and_sushline_apply_numeric_arguments_to_whole_keyboard_macro() {
    for (name, keys, expected, bash_bells) in [
        (
            "repeat whole body twice",
            b"\x18(a\x01b\x18)\x1b2\x18e\r".as_slice(),
            "bbabaa",
            0,
        ),
        (
            "zero repetitions",
            b"\x18(a\x01b\x18)\x1b0\x18eZ\r".as_slice(),
            "bZa",
            0,
        ),
        (
            "negative repetitions",
            b"\x18(a\x01b\x18)\x1b-\x18eZ\r".as_slice(),
            "bZa",
            1,
        ),
        (
            "repeat backward-edit body three times",
            b"\x18(a\x02b\x18)\x1b3\x18e\r".as_slice(),
            "bbbbaaaa",
            0,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, "");
        let sushline = run_sushline_harness_with_inputrc(keys, "");

        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline:?}"
        );
        assert_eq!(bell_count(&bash), bash_bells, "Bash {name}: {bash:?}");
        assert_eq!(bell_count(&sushline), 0, "Sushline {name}: {sushline:?}");
        if name == "negative repetitions" {
            assert!(
                bash.contains("readline: maximum macro execution nesting level exceeded"),
                "Bash {name}: {bash:?}"
            );
            assert!(
                !sushline.contains("readline: maximum macro execution nesting level exceeded"),
                "Sushline {name}: {sushline:?}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_apply_numeric_arguments_to_end_keyboard_macro() {
    let universal_inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let diagnostic = "readline: maximum macro execution nesting level exceeded";
    for (name, keys, inputrc, expected, bells, diagnostics) in [
        (
            "default count",
            b"\x18(a\x18)Z\r".as_slice(),
            "",
            "aZ",
            0,
            0,
        ),
        (
            "explicit one",
            b"\x18(a\x1b1\x18)Z\r".as_slice(),
            "",
            "aZ",
            0,
            0,
        ),
        (
            "explicit two",
            b"\x18(a\x1b2\x18)Z\r".as_slice(),
            "",
            "aaZ",
            0,
            0,
        ),
        (
            "explicit three",
            b"\x18(a\x1b3\x18)Z\r".as_slice(),
            "",
            "aaaZ",
            0,
            0,
        ),
        (
            "zero count",
            b"\x18(a\x1b0\x18)Z\r".as_slice(),
            "",
            "aZ",
            1,
            1,
        ),
        (
            "sign-only negative count",
            b"\x18(a\x1b-\x18)Z\r".as_slice(),
            "",
            "aZ",
            1,
            1,
        ),
        (
            "whole-body replay",
            b"\x18(a\x01b\x1b2\x18)Z\r".as_slice(),
            "",
            "bZbaa",
            0,
            0,
        ),
        (
            "whole-body replay twice",
            b"\x18(a\x01b\x1b3\x18)Z\r".as_slice(),
            "",
            "bZbabaa",
            0,
            0,
        ),
        (
            "universal argument",
            b"\x18(a\x0f\x18)Z\r".as_slice(),
            universal_inputrc,
            "aaaaZ",
            0,
            0,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline:?}"
        );
        for output in [&bash, &sushline] {
            assert_eq!(bell_count(output), bells, "{name}: {output}");
            assert_eq!(
                output.matches(diagnostic).count(),
                diagnostics,
                "{name}: {output}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_discard_macro_after_nonpositive_end_count() {
    for (name, argument) in [
        ("M-0", b"\x1b0".as_slice()),
        ("sign-only M-minus", b"\x1b-".as_slice()),
    ] {
        let mut keys = b"\x18(a".to_vec();
        keys.extend_from_slice(argument);
        keys.extend_from_slice(b"\x18)\x18eZ\r");
        let bash = run_bash_readline_with_inputrc_file(&keys, "");
        let sushline = run_sushline_harness_with_inputrc(&keys, "");

        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some("aZ"),
                "{editor} {name}: {output:?}"
            );
            assert_eq!(bell_count(output), 2, "{editor} {name}: {output}");
            assert_eq!(
                output
                    .matches("readline: maximum macro execution nesting level exceeded")
                    .count(),
                1,
                "{editor} {name}: {output}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_invalidate_macro_after_stray_end_kbd_macro() {
    let keys = b"\x18(a\x18)b\x18)\x18eZ\r";
    let bash = run_bash_readline_with_inputrc_file(keys, "");
    let sushline = run_sushline_harness_with_inputrc(keys, "");

    for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
        assert_eq!(
            accepted_line(output).as_deref(),
            Some("abZ"),
            "{editor}: {output:?}"
        );
        assert_eq!(bell_count(output), 2, "{editor}: {output}");
        assert!(!output.contains("maximum macro execution nesting level exceeded"));
    }
}

#[test]
fn bash_and_sushline_abort_keyboard_macro_recording_on_abort() {
    for (name, keys, expected, expected_bells) in [
        (
            "abort without a previous definition",
            b"\x18(a\x07b\x18)\x18eZ\r".as_slice(),
            "abZ",
            3,
        ),
        (
            "abort after a previous definition",
            b"\x18(a\x18)\x18(b\x07c\x18)\x18eZ\r".as_slice(),
            "abcZ",
            3,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, "");
        let sushline = run_sushline_harness_with_inputrc(keys, "");
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline:?}"
        );
        assert_eq!(bell_count(&bash), expected_bells, "Bash {name}: {bash}");
        assert_eq!(
            bell_count(&sushline),
            expected_bells,
            "Sushline {name}: {sushline}"
        );
        assert!(!bash.contains("maximum macro execution nesting level exceeded"));
        assert!(!sushline.contains("maximum macro execution nesting level exceeded"));
    }
}

#[test]
fn bash_and_sushline_abort_invalidates_previous_keyboard_macro() {
    // Measured Bash 5.3 PTY fixtures: Abort clears the completed definition
    // whether it interrupts an active recording or runs with no recording, so
    // the following call has no definition to replay.
    for (name, keys, expected) in [
        (
            "abort active recording after a previous definition",
            b"\x18(a\x18)\x18(b\x07\x18eZ\r".as_slice(),
            "abZ",
        ),
        (
            "abort without an active recording after a previous definition",
            b"\x18(a\x18)\x07\x18eZ\r".as_slice(),
            "aZ",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, "");
        let sushline = run_sushline_harness_with_inputrc(keys, "");
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{editor} {name}: {output:?}"
            );
            assert_eq!(bell_count(output), 2, "{editor} {name}: {output}");
            assert!(!output.contains("maximum macro execution nesting level exceeded"));
        }
    }
}

#[test]
fn bash_and_sushline_abort_active_recording_on_nested_start_kbd_macro() {
    // GNU `rl_start_kbd_macro` aborts when already defining
    // (`RL_STATE_MACRODEF`): the nested no-argument start rings once and
    // drops the partial `a` definition, so the following end is stray
    // (rings once and invalidates) and the call has no definition (rings
    // once). The `b` between the nested start and end stays outside any
    // definition.
    let keys = b"\x18(a\x18(b\x18)\x18eZ\r";
    let bash = run_bash_readline_with_inputrc_file(keys, "");
    let sushline = run_sushline_harness_with_inputrc(keys, "");

    for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
        assert_eq!(
            accepted_line(output).as_deref(),
            Some("abZ"),
            "{editor}: {output:?}"
        );
        assert_eq!(bell_count(output), 3, "{editor}: {output}");
        assert!(!output.contains("maximum macro execution nesting level exceeded"));
    }
}

#[test]
fn bash_and_sushline_ring_once_when_calling_last_macro_without_a_definition() {
    for keys in [b"\x18eZ\r".as_slice(), b"\x1b3\x18eZ\r".as_slice()] {
        let bash = run_bash_readline_with_inputrc_file(keys, "");
        let sushline = run_sushline_harness_with_inputrc(keys, "");

        assert_eq!(accepted_line(&bash).as_deref(), Some("Z"), "{bash}");
        assert_eq!(accepted_line(&sushline).as_deref(), Some("Z"), "{sushline}");
        assert_eq!(bell_count(&bash), 1, "{bash}");
        assert_eq!(bell_count(&sushline), 1, "{sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_replay_same_keyboard_macro_edge_cases() {
    for keys in [
        b"\x18(a\x18)\x1b3\x18e\r".as_slice(),
        b"\x18(a\x18)\x18ee\r".as_slice(),
        b"\x18(a\x01b\x18)\x18e\r".as_slice(),
        b"\x18(a\x18(\x18)b\x18)\x18e\r".as_slice(),
        b"\x18(\x1b3x\x18)\x18e\r".as_slice(),
        b"\x18(\x18)\x18eZ\r".as_slice(),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_record_inputrc_macro_binding_same_way() {
    let inputrc = r#""\C-p": "xy""#;
    let keys = b"\x18(\x10\x18)\x18e\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_and_sushline_record_nested_inputrc_macro_in_execution_order() {
    // Nested outer-over-inner inputrc macros record the outer trigger, the
    // nested trigger, and every insert byte in execution order, so replaying
    // inserts xyzxyxyz after the live xyz.
    let inputrc = "\"\\C-o\": \"\\C-pz\"\n\"\\C-p\": \"xy\"\n";
    let keys = b"\x18(\x0f\x18)\x18e\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
        assert_eq!(
            accepted_line(output).as_deref(),
            Some("xyzxyzxyxyz"),
            "{editor}: {output:?}"
        );
        assert_eq!(bell_count(output), 0, "{editor}: {output:?}");
    }
}

#[test]
fn bash_and_sushline_record_nested_inputrc_macro_around_movement() {
    // Beginning-of-line plus Q after the nested expansion stays in the
    // definition in execution order, so replay inserts at the recorded point
    // before returning to the line start for the second Q.
    let inputrc = "\"\\C-o\": \"\\C-pz\"\n\"\\C-p\": \"xy\"\n";
    let keys = b"\x18(\x0f\x01Q\x18)\x18e\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
        assert_eq!(
            accepted_line(output).as_deref(),
            Some("QQxyzxyxyzxyz"),
            "{editor}: {output:?}"
        );
        assert_eq!(bell_count(output), 0, "{editor}: {output:?}");
    }
}

#[test]
fn bash_and_sushline_record_vi_change_from_inputrc_macro() {
    // Measured Bash 5.3 PTY behavior: C-o's `iX ESC` expansion becomes the
    // latest vi change, so dot repeats X rather than the earlier physical A.
    let inputrc = "set editing-mode vi\nset keymap vi-command\n\"\\C-o\": \"iX\\C-[\"\n";
    let keys = b"A\x1b\x0f.\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
        assert_eq!(
            accepted_line(output).as_deref(),
            Some("XXA"),
            "{editor}: {output:?}"
        );
        assert_eq!(bell_count(output), 0, "{editor}: {output:?}");
    }
}

#[test]
fn bash_and_sushline_ignore_numeric_arguments_before_inputrc_macro_expansion() {
    let macros = [
        ("simple", r#""\C-xm": "ab""#, "abZ"),
        ("compound", r#""\C-xm": "a\C-bX""#, "XZa"),
    ];
    let arguments = [
        ("M-2", b"\x1b2".as_slice()),
        ("M-0", b"\x1b0".as_slice()),
        ("sign-only M-minus", b"\x1b-".as_slice()),
    ];

    for (macro_name, inputrc, expected) in macros {
        for (argument_name, argument) in arguments {
            let mut keys = argument.to_vec();
            keys.extend_from_slice(b"\x18mZ\r");
            let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
            let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);

            assert_eq!(
                accepted_line(&bash).as_deref(),
                Some(expected),
                "Bash {macro_name} {argument_name}: {bash:?}"
            );
            assert_eq!(
                accepted_line(&sushline).as_deref(),
                Some(expected),
                "Sushline {macro_name} {argument_name}: {sushline:?}"
            );
            assert_eq!(
                bell_count(&bash),
                0,
                "Bash {macro_name} {argument_name}: {bash}"
            );
            assert_eq!(
                bell_count(&sushline),
                0,
                "Sushline {macro_name} {argument_name}: {sushline}"
            );
        }
    }
}

#[test]
fn sushline_keyboard_macro_call_while_recording_does_not_recurse() {
    for (keys, expected) in [
        (b"\x18(a\x18e\x18)\x18e\r".as_slice(), "a"),
        (b"\x18(a\x18)\x18(\x18eB\x18)\x18e\r".as_slice(), "aB"),
    ] {
        let sushline = run_sushline_harness(keys);

        assert_eq!(
            accepted_line(&sushline),
            Some(expected.to_string()),
            "keys={keys:?}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_execute_named_command() {
    let inputrc = r#""\C-o": execute-named-command"#;
    let keys = b"abc\x0fbeginning-of-line\rX\r";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn sushline_prefix_meta_inputrc_extension_preserves_literal_fallback() {
    let inputrc = "set convert-meta off\nset force-meta-prefix off\n\"\\C-o\": prefix-meta\n\"\\M-a\": beginning-of-line";
    let keys = b"abc\x0faX\r";
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        Some("abcaX".to_string()),
        "{sushline}"
    );

    let overlapping_inputrc = "set convert-meta off\nset force-meta-prefix off\n\"\\C-o\": prefix-meta\n\"\\ea\\C-b\": beginning-of-line";
    let overlapping_keys = b"abc\x0facX\r";
    let sushline = run_sushline_harness_with_inputrc(overlapping_keys, overlapping_inputrc);
    assert_eq!(
        accepted_line(&sushline),
        Some("abcacX".to_string()),
        "synthetic prefix fallback should preserve its literal bytes: {sushline}"
    );

    let physical_meta_keys = b"abc\x0fa\x1bbX\r";
    let sushline = run_sushline_harness_with_inputrc(physical_meta_keys, overlapping_inputrc);
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabca".to_string()),
        "physical Meta input after the synthetic fallback should dispatch normally: {sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_lowercase_version_command() {
    let inputrc = r#""A": do-lowercase-version"#;
    let keys = b"A\r";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_abort_command() {
    let inputrc = r#""\C-o": abort"#;
    let keys = b"abc\x0fX\r";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        bell_count(&sushline),
        bell_count(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_preserve_line_through_clear_and_redraw_commands() {
    for command in ["clear-screen", "clear-display", "redraw-current-line"] {
        let inputrc = format!(r#""\C-o": {command}"#);
        let keys = b"abc\x0fX\r";
        let bash = run_bash_readline_with_bindings(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "command={command}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_editing_mode_switch_commands() {
    let vi_inputrc = r#""\C-o": vi-editing-mode"#;
    let vi_keys = b"abc\x0f\x1b0iX\r";
    let bash = run_bash_readline_with_bindings(vi_keys, vi_inputrc);
    let sushline = run_sushline_harness_with_inputrc(vi_keys, vi_inputrc);
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "vi switch\nbash={bash}\nsushline={sushline}"
    );

    let emacs_inputrc = r#"set editing-mode vi
"\C-o": emacs-editing-mode"#;
    let emacs_keys = b"abc\x0f\x01X\r";
    let bash = run_bash_readline_with_inputrc_file(emacs_keys, emacs_inputrc);
    let sushline = run_sushline_harness_with_inputrc(emacs_keys, emacs_inputrc);
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "emacs switch\nbash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_numeric_self_insert() {
    let keys = b"\x1b3x\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("xxx".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("xxx".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_multidigit_numeric_self_insert() {
    // A multi-digit argument (M-1 M-0) repeats like the single-digit form in
    // the tested Bash 5.3 PTY fixtures: the second meta-digit chains onto
    // the pending argument instead of breaking dispatch. Runtime observation,
    // not GNU source proof.
    let keys = b"\x1b1\x1b0x\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(
        accepted_line(&bash),
        Some("xxxxxxxxxx".to_string()),
        "{bash}"
    );
    assert_eq!(bell_count(&bash), 0, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&sushline), 0, "{sushline}");
}

#[test]
fn bash_readline_and_sushline_ignore_negative_numeric_self_insert() {
    // A negative repeat count inserts nothing in the tested Bash 5.3 PTY
    // fixtures. Runtime observation, not GNU source proof.
    let keys = b"\x1b-\x1b1\x1b0x\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(bell_count(&bash), 0, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&sushline), 0, "{sushline}");
}

#[test]
fn bash_and_sushline_preserve_negative_numeric_argument_after_leading_zero_digits() {
    for (name, keys, expected) in [
        ("bare digits", b"abc\x01\x1b-01\x06X\r".as_slice(), "Xabc"),
        (
            "separate meta-digit events",
            b"abc\x05\x1b-\x1b0\x1b1\x02X\r".as_slice(),
            "abcX",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, "");
        let sushline = run_sushline_harness_with_inputrc(keys, "");

        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline:?}"
        );
    }
}

#[test]
fn bash_and_sushline_match_universal_argument_digit_composition() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    for (name, prefix, expected) in [
        (
            "universal then digit",
            b"\x0f3".as_slice(),
            "abcXdefghijklmnopqrstuvwxyz",
        ),
        (
            "repeated universal then digit",
            b"\x0f\x0f3".as_slice(),
            "abcXdefghijklmnopqrstuvwxyz",
        ),
        (
            "explicit digit then universal",
            b"\x1b3\x0f".as_slice(),
            "abcXdefghijklmnopqrstuvwxyz",
        ),
        (
            "digit after universal then universal",
            b"\x0f3\x0f".as_slice(),
            "abcXdefghijklmnopqrstuvwxyz",
        ),
        (
            "negative universal count control",
            b"\x0f-2".as_slice(),
            "Xabcdefghijklmnopqrstuvwxyz",
        ),
    ] {
        let mut keys = b"abcdefghijklmnopqrstuvwxyz\x01".to_vec();
        keys.extend_from_slice(prefix);
        keys.extend_from_slice(b"\x06X\r");
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);

        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline:?}"
        );
        if name == "explicit digit then universal" {
            assert_eq!(bell_count(&bash), 0, "Bash {name}: {bash:?}");
            assert_eq!(bell_count(&sushline), 0, "Sushline {name}: {sushline:?}");
        }
    }
}

#[test]
fn bash_and_sushline_keep_negative_sign_across_universal_argument() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let keys = b"abcdefghijklmnopqrstuvwxyz\x01\x1b-\x0f\x1b-\x06X\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some("Xabcdefghijklmnopqrstuvwxyz"),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&bash), 1, "{bash}");
    assert_eq!(bell_count(&sushline), bell_count(&bash), "{sushline}");
}

#[test]
fn bash_and_sushline_match_post_digit_meta_minus_after_universal_noop() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let keys = b"abcdefghijklmnopqrstuvwxyz\x01\x1b3\x0f\x1b-\x06X\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some("-aXbcdefghijklmnopqrstuvwxyz"),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&bash), 1, "{bash}");
    assert_eq!(bell_count(&sushline), bell_count(&bash), "{sushline}");
}

#[test]
fn bash_and_sushline_keep_negative_universal_count_when_followed_by_digit() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let keys = b"abcdefghijklmnopqrstuvwxyz\x01\x1b-\x0f\x1b-\x1b1\x06X\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some("Xabcdefghijklmnopqrstuvwxyz"),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&bash), 1, "{bash}");
    assert_eq!(bell_count(&sushline), bell_count(&bash), "{sushline}");
}

#[test]
fn bash_and_sushline_match_repeated_and_post_digit_meta_minus() {
    for (name, keys, expected) in [
        (
            "repeated sign-only minus",
            b"abcdefghijklmnopqrstuvwxyz\x01\x1b-\x1b-\x06X\r".as_slice(),
            "aXbcdefghijklmnopqrstuvwxyz",
        ),
        (
            "post-digit minus",
            b"abcdefghijklmnopqrstuvwxyz\x01\x1b3\x1b-\x06X\r".as_slice(),
            "---aXbcdefghijklmnopqrstuvwxyz",
        ),
        (
            "minus toggles sign after leading zero",
            b"abcdefghijklmnopqrstuvwxyz\x01\x1b-\x1b0\x1b-\x1b1\x06X\r".as_slice(),
            "aXbcdefghijklmnopqrstuvwxyz",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, "");
        let sushline = run_sushline_harness_with_inputrc(keys, "");

        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline:?}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_zero_numeric_self_insert() {
    let keys = b"\x1b0x\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(accepted_line(&sushline), Some(String::new()), "{sushline}");
}

#[test]
fn bash_readline_and_sushline_accept_same_zero_numeric_delete() {
    let keys = b"abc\x01\x1b0\x04\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("abc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_numeric_motion() {
    let keys = b"abcdef\x1b3\x02X\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("abcXdef".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abcXdef".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_zero_numeric_motion() {
    let keys = b"abc\x1b0\x02X\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("abcX".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abcX".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_forward_byte_inside_utf8() {
    let inputrc = r#""\C-o": forward-byte"#;
    let keys = "éZ\u{1}\u{f}X\r".as_bytes();
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_backward_byte_inside_utf8() {
    let inputrc = r#""\C-o": backward-byte"#;
    let keys = "e\u{301}x\u{2}\u{f}Y\r".as_bytes();
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_numeric_kill_word() {
    for keys in [
        b"one two three\x01\x1b2\x1bdX\x19\r".as_slice(),
        b"one two three\x1b-1\x1bdX\x19\r".as_slice(),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_undo_after_midline_insert() {
    let keys = b"abc\x02X\x1f\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("abc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_yank_pop_edit() {
    let keys = b"one\x15two\x15X\x19\x1d\r";
    let inputrc = r#""\C-]": yank-pop"#;
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("Xtwoone".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xtwoone".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_yank_pop_cycle_and_invalidation_match() {
    let inputrc = r#""\C-]": yank-pop"#;

    for keys in [
        b"one\x15two\x15three\x15X\x19\x1d\x1d\r".as_slice(),
        b"one\x15two\x15X\x19\x02\x1d\r".as_slice(),
        b"one\x15two\x15X\x19Y\x1d\r".as_slice(),
        b"one\x15two\x15X\x19\x1b2\x1d\r".as_slice(),
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
fn bash_readline_and_sushline_coalesce_forward_kills() {
    let keys = b"abc def\x01\x1bd\x1bdX\x19\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("Xabc def".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc def".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_coalesce_mixed_direction_kills() {
    for keys in [
        b"one two three\x1bb\x1b\x7f\x1bdX\x19\r".as_slice(),
        b"one two three\x01\x1bd\x1b\x7fX\x19\r".as_slice(),
        b"abc def\x01\x06\x06\x0b\x18\x7fX\x19\r".as_slice(),
        b"abc def\x1bb\x18\x7f\x0bX\x19\r".as_slice(),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_empty_kill_ring_boundaries_match() {
    for keys in [
        b"abc\x15\x0bX\x19\r".as_slice(),
        b"abc\x15X\x0bY\x19\r".as_slice(),
        b"abc\x15X\x19\x0b\x1d\r".as_slice(),
    ] {
        let inputrc = r#""\C-]": yank-pop"#;
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
fn bash_readline_and_sushline_copy_commands_update_kill_ring_same_way() {
    for (inputrc, keys) in [
        (
            r#""\C-o": copy-forward-word"#,
            b"one two three\x01\x0f\x0fX\x19\r".as_slice(),
        ),
        (
            r#""\C-o": copy-backward-word"#,
            b"one two three\x0f\x0fX\x19\r".as_slice(),
        ),
        (
            r#""\C-o": copy-backward-word"#,
            b"one two three\x02\x0fX\x19\r".as_slice(),
        ),
        (
            r#""\C-o": copy-forward-word"#,
            b"one two three\x01\x0f\x06\x0fX\x19\r".as_slice(),
        ),
        (
            r#""\C-o": copy-forward-word"#,
            b"one two three\x01\x0fY\x0fX\x19\r".as_slice(),
        ),
        (
            "\"\\C-o\": copy-forward-word\n\"\\C-]\": yank-pop",
            b"one two three\x01\x0f\x15X\x19\x1d\r".as_slice(),
        ),
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
fn bash_readline_and_sushline_accept_same_character_search_commands() {
    for (command, keys) in [
        ("character-search", b"abcabc\x01\x0fcX\r".as_slice()),
        ("character-search-backward", b"abcabc\x0fbX\r".as_slice()),
    ] {
        let inputrc = format!(r#""\C-o": {command}"#);
        let bash = run_bash_readline_with_bindings(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "command={command}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_completion_bindings() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("alpha"), "").expect("write alpha fixture");
    fs::write(dir.path().join("alpine"), "").expect("write alpine fixture");
    let inputrc = "set editing-mode vi\nset completion-query-items 999";

    for key in *b"*=\\" {
        let typed = format!("{}/al\x1b{}\r", dir.path().display(), key as char);
        let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "key={key:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_character_search() {
    let keys = b"abcabc\x1b0fcix\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("abxcabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abxcabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_repeat_vi_char_search_across_reads() {
    // GNU remembers the completed `f` search across reads: `;` repeats it on
    // the next line instead of dinging, so `x` removes the found `c`.
    let keys = b"abcde\x1b0fc\rabcde\x1b0;x\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(keys, inputrc, &[]);
    let sushline = run_sushline_harness_two_reads_with_inputrc_and_history(keys, inputrc, &[]);

    assert_eq!(
        accepted_numbered_line(&sushline, 1),
        accepted_numbered_line(&bash, 1),
        "first read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 1),
        Some("abcde".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&sushline, 2),
        accepted_numbered_line(&bash, 2),
        "second read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 2),
        Some("abde".to_string()),
        "{bash}"
    );
}

#[test]
fn bash_readline_and_sushline_repeat_vi_change_across_reads() {
    // GNU replays the completed change across reads: `x` on the first line
    // arms `.` for the next line instead of leaving it unbound.
    let keys = b"abc\x1b0x\rdef\x1b0.\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(keys, inputrc, &[]);
    let sushline = run_sushline_harness_two_reads_with_inputrc_and_history(keys, inputrc, &[]);

    assert_eq!(
        accepted_numbered_line(&sushline, 1),
        accepted_numbered_line(&bash, 1),
        "first read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 1),
        Some("bc".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&sushline, 2),
        accepted_numbered_line(&bash, 2),
        "second read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 2),
        Some("ef".to_string()),
        "{bash}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_delete_motion() {
    let keys = b"abc def\x1b0dw\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("def".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("def".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_named_word_rubout_commands() {
    for (command, keys) in [
        ("backward-kill-word", b"one/two three\x0f\r".as_slice()),
        ("unix-word-rubout", b"one/two three\x0f\r".as_slice()),
        ("unix-filename-rubout", b"one/two three\x0f\r".as_slice()),
    ] {
        let inputrc = format!(r#""\C-o": {command}"#);
        let bash = run_bash_readline_with_bindings(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "command={command}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_counted_motion() {
    let keys = b"one two three\x1b03wX\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_word_and_match_motions() {
    let inputrc = "set editing-mode vi";
    for keys in [
        b"one two\x1b0wiX\r".as_slice(),
        b"one-two three\x1b0wiX\r".as_slice(),
        b"one-two three\x1b0eiX\r".as_slice(),
        b"one-two three\x1b$b iX\r".as_slice(),
        b"one-two three\x1b0WiX\r".as_slice(),
        b"one-two three\x1b0EiX\r".as_slice(),
        b"one-two three\x1b$BiX\r".as_slice(),
        b"  one two\x1b0^iX\r".as_slice(),
        b"abcdef\x1b03|iX\r".as_slice(),
        b"a(b[c]d)e\x1b0f(%iX\r".as_slice(),
        b"a(b[c]d)e\x1b0f[%iX\r".as_slice(),
    ] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_counted_delete_motion() {
    let keys = b"one two three\x1b0d2w\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_composed_counts_and_cursor_after_operator() {
    let inputrc = "set editing-mode vi";
    for keys in [
        &b"one two three four\x1b02d2w\r"[..],
        &b"one two three\x1b0dwiX\r"[..],
        &b"one two\x1b0cWXY\x1b\r"[..],
        &b"one two\x1b0ywp\r"[..],
        &b"one two\x1b0yWp\r"[..],
    ] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_replace_redo() {
    let keys = b"abc\x1b0rx.\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("xbc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("xbc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_compound_redo_and_mark_failure() {
    let inputrc = "set editing-mode vi";
    for keys in [&b"abc def\x1b0cwXY\x1b.\r"[..], &b"abc\x1b`zx\r"[..]] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_put_after_yank() {
    let keys = b"abc\x1b0yyp\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_command_defaults() {
    let inputrc = "set editing-mode vi";
    for keys in [
        b"abc\x1b0rX\r".as_slice(),
        b"abc\x1b0RXY\x1b\r".as_slice(),
        b"abc def\x1b0D\r".as_slice(),
        b"abc def\x1b0CXY\x1b\r".as_slice(),
        b"abc def\x1b0SXY\x1b\r".as_slice(),
        b"abc def\x1b0Y$p\r".as_slice(),
        b"abc\x1b0~\r".as_slice(),
        b"abc\x1b0rXu\r".as_slice(),
        b"abc\x1b0#\r".as_slice(),
    ] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_insert_self_insert_controls() {
    let inputrc = "set editing-mode vi";
    for keys in [b"\x01\r".as_slice(), b"A\x02B\r".as_slice()] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_reject_hidden_vi_register_named_command() {
    let inputrc = "set editing-mode vi\nset keymap vi-command\n\"Q\": execute-named-command";
    let keys = b"abc\x1b0Qvi-set-register\rax\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("axbc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("axbc".to_string()),
        "{sushline}"
    );
    assert_eq!(bell_count(&bash), 1, "{bash}");
    assert_eq!(bell_count(&sushline), 1, "{sushline}");
}

#[test]
fn bash_readline_and_sushline_leave_vi_register_key_unbound_by_default() {
    let keys = b"abc def\x1b0\"ayw\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_mark_round_trip() {
    let keys = b"abc def\x1b0mlw`liX\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_search_from_cursor() {
    // Vi `/` (backward) and `?` (forward) skip the history cursor entry with
    // a literal query (patch 0 Bash 5.3 PTY oracle): `ESC k k` lands on
    // `beta`, so `/alpha` finds `alpha one` while `?alpha` finds `alpha
    // two`; from the end `/alpha` finds `alpha two` and `?alpha` finds
    // nothing and keeps the empty line. Emacs incremental search stays
    // inclusive; vi skips the cursor so a matching cursor finds its
    // neighbor (`ESC k k` on `alpha two` skips it: `/alpha` finds `alpha
    // one`, `?alpha` finds `alpha three`).
    let history = ["alpha one", "beta", "alpha two"];
    let inputrc = "set editing-mode vi\n";
    for (keys, expected) in [
        (b"\x1b/alpha\r\r".as_slice(), Some("alpha two".to_string())),
        (b"\x1b?alpha\r\r".as_slice(), Some("".to_string())),
        (
            b"\x1bkk/alpha\r\r".as_slice(),
            Some("alpha one".to_string()),
        ),
        (
            b"\x1bkk?alpha\r\r".as_slice(),
            Some("alpha two".to_string()),
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        assert_eq!(accepted_line(&bash), expected, "{bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "bash={bash}\nsushline={sushline}"
        );
    }
    let history = ["alpha one", "alpha two", "alpha three"];
    for (keys, expected) in [
        (
            b"\x1bkk/alpha\r\r".as_slice(),
            Some("alpha one".to_string()),
        ),
        (
            b"\x1bkk?alpha\r\r".as_slice(),
            Some("alpha three".to_string()),
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        assert_eq!(accepted_line(&bash), expected, "keys={keys:?} {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_history_search() {
    let keys = b"\x0falpha\r\r";
    let inputrc = "\"\\C-o\": vi-search";
    let history = ["alpha one", "beta two"];
    let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn vi_search_ignore_case_is_ignored_like_bash_in_utf8_locale() {
    if !c_utf8_locale_available() {
        return;
    }

    let inputrc = "set editing-mode vi\nset search-ignore-case on";
    let locale_env = [("LC_ALL", "C.UTF-8")];
    type ViSearchCase<'a> = (&'a [&'a str], &'a [u8], &'a str, bool, &'a str);
    let cases: &[ViSearchCase<'_>] = &[
        (&["ALPHA old"], b"\x1b/", "alpha", false, ""),
        (&["ÄLPHA old"], b"\x1b/", "Älpha", false, ""),
        (
            &["ALPHA old", "alpha new"],
            b"\x1b/",
            "alpha",
            true,
            "alpha new",
        ),
        (
            &["alpha old", "ALPHA new"],
            b"\x1bkk?",
            "alpha",
            false,
            "alpha old",
        ),
    ];

    for (history, prefix, query, repeat, expected) in cases {
        let mut keys = prefix.to_vec();
        keys.extend_from_slice(query.as_bytes());
        keys.push(b'\r');
        if *repeat {
            keys.push(b'n');
        }
        keys.push(b'\r');

        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            inputrc,
            history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            inputrc,
            history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some((*expected).to_string()),
            "query={query:?} repeat={repeat}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some((*expected).to_string()),
            "query={query:?} repeat={repeat}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_vi_insert_at_beginning() {
    let keys = b"abc\x1b0iX\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_vi_delete_under_cursor() {
    let keys = b"abc\x1bhx\r";
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("ac".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("ac".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_named_history_and_expansion_helpers() {
    let history = ["cmd one two", "cmd alpha beta"];
    for (inputrc, keys) in [
        (r#""\C-o": vi-fetch-history"#, b"\x1b2\x0f\r".as_slice()),
        (r#""\C-o": vi-yank-arg"#, b"\x0f\r".as_slice()),
        (r#""\C-o": vi-yank-arg"#, b"\x1b1\x0f\r".as_slice()),
        (r#""\C-o": vi-yank-arg"#, b"\x1b2\x0f\r".as_slice()),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "inputrc={inputrc:?}\nkeys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().to_string_lossy().into_owned();
    let env = [("HOME", home.as_str())];
    let inputrc = r#""\C-o": vi-tilde-expand"#;
    let keys = b"~/sub\x0f\r";
    let bash = run_bash_readline_with_bindings_and_env(keys, inputrc, &env);
    let sushline = run_sushline_harness_with_inputrc_and_env(keys, inputrc, &env);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_unix_word_rubout() {
    let inputrc = "set editing-mode vi\n\"\\C-o\": vi-unix-word-rubout";
    let keys = b"foo/bar\x0f\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_handle_same_vi_eof_maybe() {
    let inputrc = "set editing-mode vi";
    let bash = run_bash_readline_eof_with_inputrc(b"\x04", inputrc);
    let sushline = run_sushline_harness_until(b"\x04", inputrc, "SUSHLINE_EOF");
    assert!(bash.contains("SUSHLINE_EOF"), "{bash}");
    assert!(sushline.contains("SUSHLINE_EOF"), "{sushline}");

    let keys = b"abc\x1b\x04\r";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_yank_pop_bell_matches() {
    let inputrc = r#""\C-]": yank-pop"#;
    for keys in [
        b"abc\x1d\r".as_slice(),
        b"one\x15two\x15X\x19\x02\x1d\r".as_slice(),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
        assert!(bell_count(&bash) > 0, "bash={bash}");
    }
    let keys = b"one\x15two\x15X\x19\x1d\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        bell_count(&sushline),
        bell_count(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_render_output_meta_per_locale() {
    if !c_utf8_locale_available() {
        return;
    }

    let valid_utf8 = b"\xc3\xa9\r";
    let invalid_byte = b"\xff\r";
    for (locale, setting, expected_valid) in [
        ("C", "off", "\\303\\251"),
        ("C", "on", "é"),
        ("C.UTF-8", "off", "é"),
        ("C.UTF-8", "on", "é"),
    ] {
        let inputrc = format!(
            "set convert-meta off\nset input-meta off\nset meta-flag off\nset byte-oriented off\nset output-meta {setting}"
        );
        let env = [("LC_ALL", locale)];
        let valid_bash = run_bash_readline_with_inputrc_file_and_env(valid_utf8, &inputrc, &env);
        let valid_sushline = run_sushline_harness_with_inputrc_and_env(valid_utf8, &inputrc, &env);
        let expected_valid_render = format!("{READY_PROMPT}{expected_valid}");
        assert!(
            valid_bash.contains(&expected_valid_render),
            "Bash 5.3 {locale} output-meta {setting}: {valid_bash:?}"
        );
        assert!(
            valid_sushline.contains(&expected_valid_render),
            "Sushline {locale} output-meta {setting}: {valid_sushline:?}"
        );

        let expected_invalid = if setting == "off" { "\\377" } else { "�" };
        let invalid_bash =
            run_bash_readline_with_inputrc_file_and_env(invalid_byte, &inputrc, &env);
        let invalid_sushline =
            run_sushline_harness_with_inputrc_and_env(invalid_byte, &inputrc, &env);
        let expected_invalid_render = format!("{READY_PROMPT}{expected_invalid}");
        assert!(
            invalid_bash.contains(&expected_invalid_render),
            "Bash 5.3 {locale} output-meta {setting}, invalid byte: {invalid_bash:?}"
        );
        assert!(
            invalid_sushline.contains(&expected_invalid_render),
            "Sushline {locale} output-meta {setting}, invalid byte: {invalid_sushline:?}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_render_byte_oriented_with_output_meta() {
    if !c_utf8_locale_available() {
        return;
    }

    let valid_utf8 = b"\xc3\xa9\r";
    let invalid_byte = b"\xff\r";
    for locale in ["C", "C.UTF-8"] {
        for byte_oriented in ["off", "on"] {
            for output_meta in ["off", "on"] {
                let inputrc = format!(
                    "set convert-meta off\nset input-meta off\nset meta-flag off\nset byte-oriented {byte_oriented}\nset output-meta {output_meta}"
                );
                let env = [("LC_ALL", locale)];
                let valid_bash =
                    run_bash_readline_with_inputrc_file_and_env(valid_utf8, &inputrc, &env);
                let valid_sushline =
                    run_sushline_harness_with_inputrc_and_env(valid_utf8, &inputrc, &env);
                let escaped_valid =
                    output_meta == "off" && (locale == "C" || byte_oriented == "on");
                let expected_valid = if escaped_valid { "\\303\\251" } else { "é" };
                let expected_valid_render = format!("{READY_PROMPT}{expected_valid}");
                assert!(
                    valid_bash.contains(&expected_valid_render),
                    "Bash 5.3 {locale}, byte-oriented {byte_oriented}, output-meta {output_meta}: {valid_bash:?}"
                );
                assert!(
                    valid_sushline.contains(&expected_valid_render),
                    "Sushline {locale}, byte-oriented {byte_oriented}, output-meta {output_meta}: {valid_sushline:?}"
                );

                let expected_invalid = if output_meta == "off" { "\\377" } else { "�" };
                let invalid_bash =
                    run_bash_readline_with_inputrc_file_and_env(invalid_byte, &inputrc, &env);
                let invalid_sushline =
                    run_sushline_harness_with_inputrc_and_env(invalid_byte, &inputrc, &env);
                let expected_invalid_render = format!("{READY_PROMPT}{expected_invalid}");
                assert!(
                    invalid_bash.contains(&expected_invalid_render),
                    "Bash 5.3 {locale}, byte-oriented {byte_oriented}, output-meta {output_meta}, invalid byte: {invalid_bash:?}"
                );
                assert!(
                    invalid_sushline.contains(&expected_invalid_render),
                    "Sushline {locale}, byte-oriented {byte_oriented}, output-meta {output_meta}, invalid byte: {invalid_sushline:?}"
                );
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_render_quoted_controls_independent_of_echo_control_setting() {
    if !c_utf8_locale_available() {
        return;
    }

    // Quoted-insert places SOH, DEL, and TAB in the line buffer without
    // dispatching them as editing commands. Bash keeps controls visible even
    // when echo-control-characters is off; TAB remains expanded at its stop.
    let keys = b"\x16\x01\x16\x7f\x16\t\r";
    for locale in ["C", "C.UTF-8"] {
        for echo_control in ["off", "on"] {
            let inputrc =
                format!("set byte-oriented off\nset echo-control-characters {echo_control}");
            let env = [("LC_ALL", locale)];
            let bash = run_bash_readline_with_inputrc_file_and_env(keys, &inputrc, &env);
            let sushline = run_sushline_harness_with_inputrc_and_env(keys, &inputrc, &env);
            let expected = format!("{READY_PROMPT}^A^?     ");
            assert!(
                bash.contains(&expected),
                "Bash 5.3 {locale}, echo-control-characters {echo_control}: {bash:?}"
            );
            assert!(
                sushline.contains(&expected),
                "Sushline {locale}, echo-control-characters {echo_control}: {sushline:?}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_blink_matching_paren_on_self_insert() {
    let env = [("LC_ALL", "C"), ("TERM", "xterm")];
    let keys = b"()\r";
    let on = "set blink-matching-paren on";
    let off = "set blink-matching-paren off";
    let bash_on = run_bash_readline_with_inputrc_file_and_env(keys, on, &env);
    let bash_off = run_bash_readline_with_inputrc_file_and_env(keys, off, &env);
    let sushline_on = run_sushline_harness_with_inputrc_and_env(keys, on, &env);
    let sushline_off = run_sushline_harness_with_inputrc_and_env(keys, off, &env);
    let bash_motion = ")\x08\x08";
    let sushline_motion = "\x1b[s\x1b[16G\x1b[u";

    assert_eq!(accepted_line(&bash_on), Some("()".to_string()), "{bash_on}");
    assert_eq!(
        accepted_line(&bash_off),
        Some("()".to_string()),
        "{bash_off}"
    );
    assert_eq!(
        accepted_line(&sushline_on),
        Some("()".to_string()),
        "{sushline_on}"
    );
    assert_eq!(
        accepted_line(&sushline_off),
        Some("()".to_string()),
        "{sushline_off}"
    );
    assert!(bash_on.contains(bash_motion), "{bash_on:?}");
    assert!(!bash_off.contains(bash_motion), "{bash_off:?}");
    assert!(sushline_on.contains(sushline_motion), "{sushline_on:?}");
    assert!(!sushline_off.contains(sushline_motion), "{sushline_off:?}");

    let unmatched = b")\r";
    let bash_unmatched = run_bash_readline_with_inputrc_file_and_env(unmatched, on, &env);
    let sushline_unmatched = run_sushline_harness_with_inputrc_and_env(unmatched, on, &env);
    assert_eq!(accepted_line(&bash_unmatched), Some(")".to_string()));
    assert_eq!(accepted_line(&sushline_unmatched), Some(")".to_string()));
    assert!(!bash_unmatched.contains("\x08\x08"), "{bash_unmatched:?}");
    assert!(
        !sushline_unmatched.contains("\x1b[s"),
        "{sushline_unmatched:?}"
    );
}

#[test]
fn bash_readline_and_sushline_unix_discard_empty_line_rings() {
    // C-u with nothing to kill rings once in the tested Bash 5.3 PTY
    // fixtures; killing a line stays silent. Runtime observation, not GNU
    // source proof.
    for (desc, keys, expected, bells) in [
        ("empty", b"\x15\r".as_slice(), "", 1),
        ("kills_line", b"abc\x15\r".as_slice(), "", 0),
        ("point_at_start", b"abc\x01\x15\r".as_slice(), "abc", 1),
        ("twice_empty", b"\x15\x15\r".as_slice(), "", 2),
        ("neg_point0", b"abc\x01\x1b-\x15\r".as_slice(), "abc", 1),
        ("count_ignored", b"abc\x1b2\x15\r".as_slice(), "", 0),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_noop_deletions_ring_like_bash() {
    // Backward delete, transpose, and word rubout with nothing to act on
    // ring once in the tested Bash 5.3 PTY fixtures; successful edits stay
    // silent. Runtime observation, not GNU source proof.
    for (desc, keys, expected, bells) in [
        ("bdel_empty", b"\x7f\r".as_slice(), "", 1),
        ("bdel_point0", b"ab\x01\x7f\r".as_slice(), "ab", 1),
        ("bdel_deletes", b"ab\x7f\r".as_slice(), "a", 0),
        ("transpose_empty", b"\x14\r".as_slice(), "", 1),
        ("transpose_point0", b"ab\x01\x14\r".as_slice(), "ab", 1),
        ("transpose_swaps", b"ab\x14\r".as_slice(), "ba", 0),
        ("transpose_zero_empty", b"\x1b0\x14\r".as_slice(), "", 0),
        ("transpose_zero_ab", b"ab\x1b0\x14\r".as_slice(), "ab", 0),
        ("wrubout_empty", b"\x17\r".as_slice(), "", 1),
        ("wrubout_point0", b"ab\x01\x17\r".as_slice(), "ab", 1),
        ("wrubout_rubs", b"ab\x17\r".as_slice(), "", 0),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_forward_delete_noop_rings_like_bash() {
    // Forward delete with nothing under the cursor rings once in the tested
    // Bash 5.3 PTY fixtures, including an explicit zero count; deleting a
    // character stays silent. Runtime observation, not GNU source proof.
    for (desc, keys, expected, bells) in [
        ("eol", b"abc\x04\r".as_slice(), "abc", 1),
        ("zero_empty", b"\x1b0\x04\r".as_slice(), "", 1),
        ("deletes", b"abc\x01\x04\r".as_slice(), "bc", 0),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_noop_transpose_words_rings_like_bash() {
    // transpose-words with nothing to swap rings once in the tested Bash 5.3
    // PTY fixtures; an explicit zero count and a successful swap stay
    // silent. Runtime observation, not GNU source proof.
    for (desc, keys, expected, bells) in [
        ("empty", b"\x1bt\r".as_slice(), "", 1),
        ("single_word", b"ab\x1bt\r".as_slice(), "ab", 1),
        ("point_at_start", b"ab cd\x01\x1bt\r".as_slice(), "ab cd", 1),
        ("swaps", b"ab cd\x1bt\r".as_slice(), "cd ab", 0),
        ("zero_empty", b"\x1b0\x1bt\r".as_slice(), "", 0),
        (
            "zero_words",
            b"ab cd\x01\x1b0\x1bt\r".as_slice(),
            "ab cd",
            0,
        ),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_empty_undo_rings_like_bash() {
    // Undo with nothing to undo rings once in the tested Bash 5.3 PTY
    // fixtures, in emacs and vi command mode; a successful undo stays
    // silent. Runtime observation, not GNU source proof.
    for (desc, inputrc, keys, expected, bells) in [
        (
            "emacs_empty",
            "".to_string(),
            b"\x1f\r".as_slice(),
            "".to_string(),
            1,
        ),
        (
            "emacs_undoes",
            "".to_string(),
            b"abc\x1f\r".as_slice(),
            "".to_string(),
            0,
        ),
        (
            "emacs_exhausted",
            "".to_string(),
            b"abc\x1f\x1f\r".as_slice(),
            "".to_string(),
            1,
        ),
        (
            // Leading ESC forces vi command mode deterministically: a bare
            // `u` first key is subject to initial-mode flake in this harness.
            "vi_empty",
            "set editing-mode vi".to_string(),
            b"\x1bu\r".as_slice(),
            "".to_string(),
            1,
        ),
    ] {
        let inputrc: &str = &inputrc;
        let history: &[&str] = &[];
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.clone()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_noop_kill_revert_ring_like_bash() {
    // backward-kill-line and revert-line with nothing to act on ring once
    // in the tested Bash 5.3 PTY fixtures; successful reverts stay silent.
    // Runtime observation, not GNU source proof.
    for (desc, inputrc, keys, expected, bells) in [
        (
            "bkill_empty",
            "".to_string(),
            b"\x18\x7f\r".as_slice(),
            "".to_string(),
            1,
        ),
        (
            "revert_empty",
            "".to_string(),
            b"\x1br\r".as_slice(),
            "".to_string(),
            1,
        ),
        (
            "revert_restores",
            "".to_string(),
            b"abc\x1br\r".as_slice(),
            "".to_string(),
            0,
        ),
    ] {
        let inputrc: &str = &inputrc;
        let history: &[&str] = &[];
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.clone()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_empty_yank_rings_like_bash() {
    // yank and yank-pop with nothing to yank ring once in the tested Bash
    // 5.3 PTY fixtures. Runtime observation, not GNU source proof.
    for (desc, keys) in [
        ("yank_empty", b"\x19\r".as_slice()),
        ("yankpop_empty", b"\x1by\r".as_slice()),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(accepted_line(&bash), Some(String::new()), "{desc}: {bash}");
        assert_eq!(bell_count(&bash), 1, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 1, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_character_search_bells_like_bash() {
    // character-search with no match rings once in the tested Bash 5.3 PTY
    // fixtures; a successful search stays silent. Runtime observation, not
    // GNU source proof.
    for (desc, keys, bells) in [
        ("fwd_no_match", b"abc\x1dx\r".as_slice(), 1),
        ("fwd_match", b"abc\x01\x1dc\r".as_slice(), 0),
        ("bwd_no_match", b"abc\x01\x1b\x1dz\r".as_slice(), 1),
        ("bwd_match", b"abc\x1b\x1da\r".as_slice(), 0),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(
            accepted_line(&bash),
            Some("abc".to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_abort_rings_like_bash() {
    // abort rings and preserves the line in the tested Bash 5.3 PTY
    // fixtures. Runtime observation, not GNU source proof.
    for (desc, keys, expected) in [
        ("empty", b"\x07\r".as_slice(), ""),
        ("with_text", b"abc\x07\r".as_slice(), "abc"),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), 1, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 1, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_kill_line_variants_ring_like_bash() {
    // kill-line and kill-whole-line stay silent with nothing to kill, while
    // backward-kill-line with nothing behind point rings once, in the tested
    // Bash 5.3 PTY fixtures. Runtime observation, not GNU source proof.
    let kwh = "\"\\C-o\": kill-whole-line";
    for (desc, inputrc, keys, expected, bells) in [
        ("kill_line_empty", "", b"\x0b\r".as_slice(), "", 0),
        ("kwhole_empty", kwh, b"\x0f\r".as_slice(), "", 0),
        ("kwhole_nonempty", kwh, b"abc\x0f\r".as_slice(), "", 0),
        ("bkill_empty", "", b"\x18\x7f\r".as_slice(), "", 1),
        ("bkill_bol", "", b"abc\x01\x18\x7f\r".as_slice(), "abc", 1),
        ("kill_line_bol", "", b"abc\x01\x0b\r".as_slice(), "", 0),
    ] {
        let inputrc: &str = inputrc;
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_region_without_mark_uses_line_start_like_bash() {
    // With no mark set, region commands use line start as the mark in the
    // tested Bash 5.3 PTY fixtures; a degenerate empty region stays a silent
    // no-op. Runtime observation, not GNU source proof.
    let kill = "\"\\C-o\": kill-region";
    let copy = "\"\\C-o\": copy-region-as-kill";
    for (desc, inputrc, keys, expected, bells) in [
        ("kill_end", kill, b"abc\x0f\r".as_slice(), "", 0),
        ("kill_mid", kill, b"abc\x01\x06\x0f\r".as_slice(), "bc", 0),
        ("kill_point0", kill, b"abc\x01\x0f\r".as_slice(), "abc", 0),
        (
            "copy_then_yank",
            copy,
            b"abc\x0f\x01\x19\r".as_slice(),
            "abcabc",
            0,
        ),
    ] {
        let inputrc: &str = inputrc;
        let history: &[&str] = &[];
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_mark_bells_match_like_bash() {
    // exchange-point-and-mark with no mark set stays silent while
    // vi-goto-mark with no mark rings once in the tested Bash 5.3 PTY
    // fixtures. Runtime observation, not GNU source proof.
    let keys = b"\x18\x18\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);
    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(bell_count(&bash), 0, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&sushline), 0, "{sushline}");

    let inputrc = "set editing-mode vi";
    let keys = b"\x1b`a\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(bell_count(&bash), 1, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&sushline), 1, "{sushline}");
}

#[test]
fn bash_readline_and_sushline_backward_kill_word_matches_like_bash() {
    // backward-kill-word stays silent with nothing behind point and kills
    // the previous word otherwise in the tested Bash 5.3 PTY fixtures.
    // Runtime observation, not GNU source proof.
    for (desc, keys, expected, bells) in [
        ("empty", b"\x1b\x7f\r".as_slice(), "", 0),
        ("point0", b"ab cd\x01\x1b\x7f\r".as_slice(), "ab cd", 0),
        (
            "kills_word",
            b"ab cd\x01\x06\x06\x06\x1b\x7f\r".as_slice(),
            "cd",
            0,
        ),
        ("kill_word_eol", b"abc\x1bd\r".as_slice(), "abc", 0),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_vi_put_empty_rings_like_bash() {
    // vi put with an empty kill ring rings once in the tested Bash 5.3 PTY
    // fixtures; command mode is entered explicitly for determinism.
    // Runtime observation, not GNU source proof.
    let inputrc = "set editing-mode vi";
    for (desc, keys) in [
        ("put", b"\x1bp\r".as_slice()),
        ("put_before", b"\x1bP\r".as_slice()),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(accepted_line(&bash), Some(String::new()), "{desc}: {bash}");
        assert_eq!(bell_count(&bash), 1, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 1, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_vi_redo_empty_stays_silent_like_bash() {
    // vi redo with nothing to redo stays silent in the tested Bash 5.3 PTY
    // fixtures. Runtime observation, not GNU source proof.
    let inputrc = "set editing-mode vi";
    let keys = b"\x1b.\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(bell_count(&bash), 0, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&sushline), 0, "{sushline}");
}

#[test]
fn bash_readline_and_sushline_vi_char_search_bells_like_bash() {
    // vi character search with no match rings once in the tested Bash 5.3
    // PTY fixtures; a successful search stays silent. Runtime observation,
    // not GNU source proof.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("fwd_no_match", b"ab\x1bfx\r".as_slice(), "ab", 1),
        ("bwd_no_match", b"ab\x1bFx\r".as_slice(), "ab", 1),
        ("bwd_match", b"abx\x1bFb\r".as_slice(), "abx", 0),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_numeric_kill_line_matches_like_bash() {
    // A numeric argument on kill-line, backward-kill-line and revert-line
    // behaves the same in the tested Bash 5.3 PTY fixtures: kills happen
    // silently, and revert-line ignores the count. Runtime observation, not
    // GNU source proof.
    for (desc, keys, expected, bells) in [
        ("kill_line_m2", b"abcdef\x1b2\x0b\r".as_slice(), "abcdef", 0),
        ("bkill_line_m2", b"abcdef\x1b2\x18\x7f\r".as_slice(), "", 0),
        ("revert_m2", b"abc\x1b2\x1br\r".as_slice(), "", 0),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_unbound_esc_matches_like_bash() {
    let vi = "set editing-mode vi";
    let em_pair: &[u8] = b"\x1b\x1babc\r";
    let em_meta: &[u8] = b"\x1bqabc\r";
    let vi_double: &[u8] = b"\x1b\x1b\r";
    let vi_triple: &[u8] = b"\x1b\x1b\x1b\r";
    let vi_meta: &[u8] = b"\x1b\x1bqabc\r";
    for (desc, keys, inputrc, expected, bells) in [
        ("vi_double", vi_double, vi, Some(""), 0),
        ("vi_triple", vi_triple, vi, Some(""), 0),
        ("vi_meta", vi_meta, vi, Some("bc"), 1),
        ("em_pair", em_pair, "", Some("abc"), 1),
        ("em_meta", em_meta, "", Some("abc"), 1),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sush = run_sushline_harness_with_inputrc(keys, inputrc);
        let want = expected.map(str::to_string);
        assert_eq!(accepted_line(&bash), want, "{desc}");
        assert_eq!(accepted_line(&sush), want, "{desc}");
        assert_eq!(bell_count(&bash), bells, "{desc}");
        assert_eq!(bell_count(&sush), bells, "{desc}");
    }
}

#[test]
fn bash_readline_and_sushline_char_move_edges_ring_like_bash() {
    for (desc, keys, expected, bells) in [
        ("back_at_start", b"abc\x01\x02\r".as_slice(), "abc", 1),
        ("fwd_at_end", b"abc\x06\r".as_slice(), "abc", 1),
        ("back_over_count", b"abc\x1b5\x02\r".as_slice(), "abc", 1),
        ("fwd_over_count", b"abc\x1b5\x06\r".as_slice(), "abc", 1),
        ("back_exact_count", b"abc\x1b2\x02\r".as_slice(), "abc", 0),
        ("back_zero_count", b"abc\x1b0\x02\r".as_slice(), "abc", 0),
        ("back_negative", b"abc\x1b-\x02\r".as_slice(), "abc", 1),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(accepted_line(&bash), Some(expected.to_string()), "{desc}");
        assert_eq!(
            accepted_line(&sushline),
            Some(expected.to_string()),
            "{desc}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}");
        assert_eq!(bell_count(&sushline), bells, "{desc}");
    }
}

#[test]
fn bash_readline_and_sushline_match_lone_bracketed_paste_end() {
    let keys = b"\x1b[201~abc\r";
    for inputrc in ["", "set enable-bracketed-paste off\n"] {
        let bash = run_bash_readline_with_term(keys, inputrc, Some("xterm-256color"));
        let sushline = run_sushline_harness_with_term(keys, inputrc, Some("xterm-256color"));
        for (name, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some("~abc"),
                "{name}, inputrc={inputrc:?}: {output:?}"
            );
            assert_eq!(
                bell_count(output),
                1,
                "{name}, inputrc={inputrc:?}: {output:?}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_match_bracketed_paste_terminal_mode_matrix() {
    let cases = [
        (Some("xterm-256color"), "", true),
        (Some("vt100"), "", true),
        (Some("vt52"), "", false),
        (Some("dumb"), "", false),
        (None, "", false),
        (Some("dumb"), "set enable-bracketed-paste on\n", true),
        (Some("vt52"), "set enable-bracketed-paste on\n", true),
        (
            Some("xterm-256color"),
            "set enable-bracketed-paste off\n",
            false,
        ),
    ];

    for (term, inputrc, expected) in cases {
        let bash = run_bash_readline_with_term(b"\r", inputrc, term);
        let sushline = run_sushline_harness_with_term(b"\r", inputrc, term);
        for (name, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(""),
                "{name}, TERM={term:?}, inputrc={inputrc:?}: {output:?}"
            );
            assert_eq!(
                output.contains("\x1b[?2004h"),
                expected,
                "{name}, TERM={term:?}, inputrc={inputrc:?}: {output:?}"
            );
            assert_eq!(
                output.contains("\x1b[?2004l"),
                expected,
                "{name}, TERM={term:?}, inputrc={inputrc:?}: {output:?}"
            );
        }
    }
}

fn run_bracketed_paste_checkpoints(paste: &[u8]) -> (Vec<String>, Vec<String>) {
    use portable_pty::{CommandBuilder, PtySize};
    use std::fs;

    let dir = tempfile::tempdir().expect("tempdir");
    let inputrc = "set enable-bracketed-paste on\nset bell-style audible\n";
    let inputrc_path = dir.path().join("inputrc");
    fs::write(&inputrc_path, inputrc).expect("write inputrc");
    let mut bash = CommandBuilder::new("bash");
    bash.env("LC_ALL", "C");
    bash.env("TERM", "xterm-256color");
    bash.env("INPUTRC", inputrc_path.to_string_lossy().as_ref());
    let script = format!(
        r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\nSUSHLINE_DONE\n' "$line""#
    );
    bash.args(["--noprofile", "--norc", "-i", "-c", &script]);
    let mut sushline = CommandBuilder::new("sh");
    sushline.env("LC_ALL", "C");
    sushline.env("TERM", "xterm-256color");
    sushline.env("SUSHLINE_INPUTRC", inputrc);
    sushline.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline.env("SUSHLINE_HISTORY", "");
    sushline.env("SUSHLINE_BIN", env!("CARGO_BIN_EXE_sushline-harness"));
    sushline.args(["-c", "\"$SUSHLINE_BIN\"; printf 'SUSHLINE_DONE\\n'"]);
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };
    // An unfinished paste consumes Ctrl-G literally, so it cannot be probed
    // with an abort barrier. Only checkpoint after the complete terminator.
    // Partial-buffer and split-marker invariants are checked synchronously in
    // editor/tests/lifecycle.rs, not inferred from PTY silence or write chunks.
    let mut keys = paste.to_vec();
    keys.push(7);
    let frames = |command| {
        let out = super::pty::run_pty_steps_with_size_until(
            command,
            &[(READY_PROMPT, &keys), ("\x07", b"\r")],
            size,
            "SUSHLINE_DONE",
        );
        let (before, _) = out.split_once('\x07').expect("paste abort barrier");
        assert!(
            out.contains("SUSHLINE_DONE"),
            "incomplete acceptance: {out:?}"
        );
        vec![before.to_owned(), out]
    };
    (frames(bash), frames(sushline))
}

#[test]
fn bash_and_sushline_insert_complete_bracketed_paste_before_enter() {
    let (bash_frames, sushline_frames) =
        run_bracketed_paste_checkpoints(b"\x1b[200~abcdefghijklmnop\x1b[201~");

    for (name, frames) in [("Bash", &bash_frames), ("Sushline", &sushline_frames)] {
        assert!(
            !frames[0].contains("SUSHLINE_ACCEPTED:"),
            "{name} accepted before Enter: {frames:?}"
        );
        assert!(
            frames[0].contains("abcdefghijklmnop"),
            "{name} did not insert the full paste after its terminator: {frames:?}"
        );
        assert_eq!(
            accepted_line(&frames[1]).as_deref(),
            Some("abcdefghijklmnop"),
            "{name}: {frames:?}"
        );
    }
}

#[test]
fn bash_and_sushline_complete_paste_without_accepting_but_handle_cr_differently() {
    let (bash_frames, sushline_frames) = run_bracketed_paste_checkpoints(b"\x1b[200~a\rb\x1b[201~");

    for (name, frames) in [("bash", &bash_frames), ("sushline", &sushline_frames)] {
        assert!(
            !frames[0].contains("SUSHLINE_ACCEPTED:"),
            "{name} accepted before the later CR: {frames:?}"
        );
    }
    let bash_line = bash_frames[1]
        .split_once("SUSHLINE_ACCEPTED:")
        .expect("Bash accepted marker")
        .1;
    let sushline_line = sushline_frames[1]
        .split_once("SUSHLINE_ACCEPTED:")
        .expect("Sushline accepted marker")
        .1;
    assert!(bash_line.starts_with("a\r\n"), "{bash_line:?}");
    assert!(sushline_line.starts_with("a\rb\r\n"), "{sushline_line:?}");
}

#[test]
fn bash_and_sushline_match_cr_splitting_partial_paste_end_marker() {
    let keys = b"\x1b[200~abc\x1b[201\r~\x1b[201~\r";
    let inputrc = "set enable-bracketed-paste on\nset bell-style audible\n";
    let bash = run_bash_readline_with_term(keys, inputrc, Some("xterm-256color"));
    let sushline = run_sushline_harness_with_term(keys, inputrc, Some("xterm-256color"));

    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some("abc\x1b[201"),
        "{bash:?}"
    );
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("abc\x1b[201"),
        "{sushline:?}"
    );
}

#[test]
fn bash_and_sushline_match_complete_bracketed_paste_fixtures() {
    let framed_paste = b"\x1b[200~abc\x1b[201~\r";
    for (term, inputrc) in [
        (Some("xterm-256color"), ""),
        (Some("xterm-256color"), "set enable-bracketed-paste off\n"),
        (Some("dumb"), "set enable-bracketed-paste on\n"),
    ] {
        let bash = run_bash_readline_with_term(framed_paste, inputrc, term);
        let sushline = run_sushline_harness_with_term(framed_paste, inputrc, term);
        assert_eq!(accepted_line(&bash).as_deref(), Some("abc"), "{bash:?}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("abc"),
            "{sushline:?}"
        );
        assert_eq!(bell_count(&bash), 0, "{bash:?}");
        assert_eq!(bell_count(&sushline), 0, "{sushline:?}");
    }

    // Pasted text sets the mark at its starting point in both editors.
    let mark_keys = b"\x1b[200~abc\x1b[201~\x18\x18X\r";
    let bash = run_bash_readline_with_term(mark_keys, "", Some("xterm-256color"));
    let sushline = run_sushline_harness_with_term(mark_keys, "", Some("xterm-256color"));
    assert_eq!(accepted_line(&bash).as_deref(), Some("Xabc"), "{bash:?}");
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("Xabc"),
        "{sushline:?}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn bash_and_sushline_insert_partial_bracketed_paste_at_true_pty_eof() {
    // The PTY master stays open. Canonical VEOF releases the partial paste and
    // produces a zero-byte read; the child must remain alive until the later LF.
    let bash = run_bash_readline_with_canonical_paste_eof(b"");
    let sushline = run_sushline_harness_with_canonical_paste_eof(b"");

    assert_eq!(accepted_line(&bash).as_deref(), Some("partial"), "{bash}");
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("partial"),
        "{sushline}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn bash_and_sushline_insert_partial_end_marker_at_true_pty_eof() {
    let suffix = b"\x1b[201";
    let bash = run_bash_readline_with_canonical_paste_eof(suffix);
    let sushline = run_sushline_harness_with_canonical_paste_eof(suffix);

    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some("partial\x1b[201"),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("partial\x1b[201"),
        "{sushline}"
    );
}
