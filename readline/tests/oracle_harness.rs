use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use readline::History;
use std::fs;
use std::io::{Read, Write};
use std::process::Command;
use std::time::{Duration, Instant};

const READY_PROMPT: &str = "SUSHLINE_READY>";

#[test]
#[ignore = "requires a local GNU bash/readline oracle and PTY driver"]
fn oracle_bash_version_is_available() {
    let output = Command::new("bash")
        .arg("--version")
        .output()
        .expect("bash must be available for oracle tests");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("GNU bash"));
}

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
fn bash_readline_and_sushline_resolve_relative_inputrc_include_from_inputrc_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("inputrc");
    let included = dir.path().join("included.inputrc");
    fs::write(&included, "\"\\C-x\\C-a\": beginning-of-line\n").expect("write included inputrc");
    fs::write(&main, "$include included.inputrc\n").expect("write main inputrc");
    let keys = b"abc\x18\x01X\r";

    let bash = run_bash_readline_with_inputrc_path(keys, &main);
    let sushline = run_sushline_harness_with_inputrc_path(keys, &main);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
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
fn bash_readline_and_sushline_accept_same_prefix_meta_command() {
    let inputrc = "\"\\C-o\": prefix-meta\n\"\\M-a\": beginning-of-line";
    let keys = b"abc\x0faX\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
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
fn bash_readline_and_sushline_history_search_backward_by_prefix() {
    let keys = b"alp\x1d\r";
    let inputrc = r#""\C-]": history-search-backward"#;
    let history = ["alpha one", "beta", "alpha two"];
    let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(
        accepted_line(&bash),
        Some("alpha two".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("alpha two".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_history_search_forward_restores_edit() {
    let keys = b"alp\x1d\x1e\r";
    let backward = r#""\C-]": history-search-backward"#;
    let forward = r#""\C-^": history-search-forward"#;
    let inputrc = format!("{backward}\n{forward}");
    let history = ["alpha one", "beta", "alpha two"];
    let bash = run_bash_readline_with_bindings_and_history(keys, &inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, &inputrc, &history);

    assert_eq!(accepted_line(&bash), Some("alp".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("alp".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_reverse_search_accepts_match() {
    let keys = b"\x12two\r";
    let history = ["alpha one", "beta", "alpha two"];
    let bash = run_bash_readline_with_bindings_and_history(keys, "", &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);

    assert_eq!(
        accepted_line(&bash),
        Some("alpha two".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("alpha two".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_reverse_search_repeats_to_older_match() {
    let keys = b"\x12alpha\x12\r";
    let history = ["alpha one", "beta", "alpha two"];
    let bash = run_bash_readline_with_bindings_and_history(keys, "", &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);

    assert_eq!(
        accepted_line(&bash),
        Some("alpha one".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("alpha one".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_fetch_same_numbered_history_entry() {
    let inputrc = r#""\C-o": fetch-history"#;
    let history = ["one", "two", "three"];
    let keys = b"\x1b2\x0f\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_operate_and_get_next_prefill_same_next_read() {
    let inputrc = r#""\C-o": operate-and-get-next"#;
    let history = ["one", "two", "three"];
    for keys in [b"\x10\x10\x0f\r".as_slice(), b"\x10\x1b1\x0f\r".as_slice()] {
        let bash =
            run_bash_readline_two_reads_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline =
            run_sushline_harness_two_reads_with_inputrc_and_history(keys, inputrc, &history);

        assert_eq!(
            accepted_numbered_line(&sushline, 1),
            accepted_numbered_line(&bash, 1),
            "keys={keys:?}\nfirst read\nbash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            accepted_numbered_line(&sushline, 2),
            accepted_numbered_line(&bash, 2),
            "keys={keys:?}\nsecond read\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_expand_same_history_words_and_modifiers() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["echo /tmp/foo.txt alpha beta"];

    for (typed, expected) in [
        ("!!:0", "echo"),
        ("!!:^", "/tmp/foo.txt"),
        ("!!:$", "beta"),
        ("!!:*", "/tmp/foo.txt alpha beta"),
        ("!!:1-2", "/tmp/foo.txt alpha"),
        ("!!:1-", "/tmp/foo.txt alpha"),
        ("!!:1:h", "/tmp"),
        ("!!:1:t", "foo.txt"),
        ("!!:1:r", "/tmp/foo"),
        ("!!:1:e", ".txt"),
        ("!!:q", "'echo /tmp/foo.txt alpha beta'"),
        ("!!:x", "'echo' '/tmp/foo.txt' 'alpha' 'beta'"),
        ("!!:1:q", "'/tmp/foo.txt'"),
        ("!!:1:x", "'/tmp/foo.txt'"),
        ("!?foo?:%", "/tmp/foo.txt"),
        ("!?foo?:%:r", "/tmp/foo"),
        ("!!:s/alpha/ALPHA/", "echo /tmp/foo.txt ALPHA beta"),
        ("!!:gs/o/O/", "echO /tmp/fOO.txt alpha beta"),
        ("!!:s/o/O/:&", "echO /tmp/fOo.txt alpha beta"),
        ("!!:s/o/O/:g&", "echO /tmp/fOO.txt alpha beta"),
        ("!!:Gs/o/O/", "echO /tmp/fOo.txt alpha beta"),
        ("!!:s/o/O/:G&", "echO /tmp/fOo.txt alpha beta"),
        ("!!:s/\\//_/", "echo _tmp/foo.txt alpha beta"),
        ("!!:gs#/#_#", "echo _tmp_foo.txt alpha beta"),
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_history_expand(typed, &history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);

        assert_eq!(bash, expected, "{typed}");
        assert_eq!(
            accepted_line(&sushline),
            Some(expected.to_string()),
            "{typed}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_expand_same_history_edge_cases() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = [
        "echo zero one two three",
        "printf /a/b/c.txt alpha alpha beta",
        "grep needle middle needle-last tail",
    ];

    for typed in [
        "!!",
        "!!$",
        "!!^",
        "!!*",
        "!!-2",
        "!!2",
        "!!0",
        r"\!!",
        "foo=!!",
        "\"!!\"",
        "!-1",
        "!1",
        "!echo",
        "!!:2*",
        "!!:2-$",
        "!!:-2",
        "!!:2-",
        "!!:0-2",
        "!!:1:q",
        "!!:2*:q",
        "!!:2*:x",
        "!?needle?:%",
        "!?needle?:%:q",
        "!?needle middle?",
        "!?needle? !??:%",
        "!printf:s/alpha/ALPHA",
        "!printf:gs/alpha/ALPHA",
        "!printf:s/alpha/&+&/",
        "!printf:s/alpha",
        "!printf:as/alpha/ALPHA/",
        "!printf:s/alpha/ALPHA/:a&",
        "!printf:s/alpha/ALPHA/:as/beta/BETA/",
        "!printf:s#/#_#",
        "!printf:s/\\//_/",
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_history_expand(typed, &history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);

        assert_eq!(
            accepted_line(&sushline),
            Some(bash.clone()),
            "{typed}: bash={bash:?}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_reject_same_history_substitution_failures() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["printf /a/b/c.txt alpha alpha beta"];

    for typed in [
        "!printf:s/missing/MISSING/",
        "!printf:gs/missing/MISSING/",
        "!printf:Gs/missing/MISSING/",
        "!printf:&",
        "^missing^MISSING^",
        "^^MISSING^",
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash =
            run_bash_readline_with_inputrc_file_and_history(keys.as_bytes(), inputrc, &history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{typed}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_do_not_quick_substitute_from_history_expand_line() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["echo alpha alpha"];
    let keys = b"^alpha^ALPHA^\x18\x01\r^^BETA^\x18\x01\r";
    let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_two_reads_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(
        accepted_numbered_line(&sushline, 1),
        accepted_numbered_line(&bash, 1),
        "first read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&sushline, 2),
        accepted_numbered_line(&bash, 2),
        "second read\nbash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_apply_same_history_quote_inhibit_policy() {
    let inputrc = "set history-quotes-inhibit-expansion on\n\"\\C-x\\C-a\": history-expand-line";
    let history = ["echo alpha"];
    let keys = b"'!!\x18\x01\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_preserve_same_quoted_history_words() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let quoted_history = [r#"printf "two words" $'ansi word' tail"#];

    for typed in ["!!:1", "!!:2", "!!:1:q"] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_history_expand(typed, &quoted_history);
        let sushline = run_sushline_harness_with_inputrc_and_history(
            keys.as_bytes(),
            inputrc,
            &quoted_history,
        );

        assert_eq!(
            accepted_line(&sushline),
            Some(bash.clone()),
            "{typed}: bash={bash:?}\nsushline={sushline}"
        );
    }

    let shell_word_history =
        [r#"echo $foo foo=bar a@b a\ b {a,b} $(printf hi) `printf hi` <(echo hi) >(cat) tail"#];
    for typed in [
        "!!:1", "!!:2", "!!:3", "!!:4", "!!:5", "!!:6", "!!:7", "!!:8", "!!:9",
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_history_expand(typed, &shell_word_history);
        let sushline = run_sushline_harness_with_inputrc_and_history(
            keys.as_bytes(),
            inputrc,
            &shell_word_history,
        );

        assert_eq!(
            accepted_line(&sushline),
            Some(bash.clone()),
            "{typed}: bash={bash:?}\nsushline={sushline}"
        );
    }

    let shell_group_history = ["echo arr=(one two) $((1 + 2)) ((a + b)) tail"];
    for typed in [
        "!!:1", "!!:2", "!!:3", "!!:4", "!!:5", "!!:6", "!!:7", "!!:8", "!!:9", "!!:10", "!!:*",
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_history_expand(typed, &shell_group_history);
        let sushline = run_sushline_harness_with_inputrc_and_history(
            keys.as_bytes(),
            inputrc,
            &shell_group_history,
        );

        assert_eq!(
            accepted_line(&sushline),
            Some(bash.clone()),
            "{typed}: bash={bash:?}\nsushline={sushline}"
        );
    }

    let redirection_history = ["cat <in >out 2>&1 |& sed s/a/b/ && echo done"];
    for typed in [
        "!!:1", "!!:2", "!!:3", "!!:4", "!!:5", "!!:6", "!!:7", "!!:8", "!!:9", "!!:10", "!!:11",
        "!!:*",
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_history_expand(typed, &redirection_history);
        let sushline = run_sushline_harness_with_inputrc_and_history(
            keys.as_bytes(),
            inputrc,
            &redirection_history,
        );

        assert_eq!(
            accepted_line(&sushline),
            Some(bash.clone()),
            "{typed}: bash={bash:?}\nsushline={sushline}"
        );
    }

    let compact_redirection_history = ["cmd 2>file 12>>file 3<in 4<&0 >&2 <&0 &>>file ;& ;;& <>rw"];
    for typed in [
        "!!:1", "!!:2", "!!:3", "!!:4", "!!:5", "!!:6", "!!:7", "!!:8", "!!:9", "!!:10", "!!:11",
        "!!:12", "!!:13", "!!:14", "!!:15", "!!:16", "!!:17", "!!:18", "!!:*",
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_history_expand(typed, &compact_redirection_history);
        let sushline = run_sushline_harness_with_inputrc_and_history(
            keys.as_bytes(),
            inputrc,
            &compact_redirection_history,
        );

        assert_eq!(
            accepted_line(&sushline),
            Some(bash.clone()),
            "{typed}: bash={bash:?}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_yank_same_quoted_history_argument() {
    let inputrc = r#""\C-o": yank-nth-arg"#;
    let history = [r#"printf "two words" $'ansi word' tail"#];
    let keys = b"\x0f\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_yank_same_numbered_and_repeated_history_argument() {
    let inputrc = r#""\C-o": yank-nth-arg"#;
    let history = ["cmd one two", "cmd alpha beta"];
    for keys in [
        b"\x1b1\x0f\r".as_slice(),
        b"\x1b1\x0f\x0f\r".as_slice(),
        b"\x0f\x0f\r".as_slice(),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_yank_last_arg_repeats_same_history_cycle() {
    let history = ["cmd one two", "cmd alpha beta"];
    for keys in [b"\x1b.\r".as_slice(), b"\x1b.\x1b.\r".as_slice()] {
        let bash = run_bash_readline_with_bindings_and_history(keys, "", &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "keys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_dynamic_history_completion_uses_common_prefix() {
    let inputrc = r#""\C-o": dynamic-complete-history"#;
    let history = ["echo alpha", "echo alpine"];
    let keys = b"al\x0f\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

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
fn bash_readline_and_sushline_accept_same_named_command_word_motion() {
    let keys = b"one two/three\x18\x01X\r";
    let inputrc = r#""\C-x\C-a": shell-backward-word"#;
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&bash),
        Some("one Xtwo/three".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("one Xtwo/three".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_command_word_motion_over_metacharacters() {
    let inputrc = r#""\C-o": shell-forward-word"#;
    for keys in [
        b"echo foo|bar\x01\x0fX\r".as_slice(),
        b"cat <(echo hi)|wc\x01\x0f\x0fX\r".as_slice(),
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
fn bash_readline_and_sushline_accept_same_shell_kill_word() {
    for (inputrc, keys) in [
        (
            r#""\C-o": shell-kill-word"#,
            b"foo|bar\x01\x0fX\r".as_slice(),
        ),
        (
            "\"\\C-p\": shell-forward-word\n\"\\C-o\": shell-kill-word",
            b"cat <(echo hi)|wc\x01\x10\x0fX\r".as_slice(),
        ),
        (
            "\"\\C-p\": shell-forward-word\n\"\\C-o\": shell-backward-kill-word",
            b"cat <(echo hi)|wc\x01\x10\x10\x0fX\r".as_slice(),
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "inputrc={inputrc:?}\nkeys={keys:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_accept_same_shell_transpose_words() {
    let inputrc = r#""\C-o": shell-transpose-words"#;
    for keys in [
        b"one \"two words\" three\x0f\r".as_slice(),
        b"cat <(echo hi) tail\x0f\r".as_slice(),
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
fn bash_readline_and_sushline_complete_filename_ignoring_case() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("AlphaFile");
    fs::write(&file, "").expect("write fixture");
    let typed = format!("{}/alp\t\r", dir.path().display());
    let expected = format!("{}/AlphaFile ", dir.path().display());
    let inputrc = "set completion-ignore-case on";

    let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
    let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

    assert_eq!(accepted_line(&bash), Some(expected.clone()), "{bash}");
    assert_eq!(accepted_line(&sushline), Some(expected), "{sushline}");
}

#[test]
fn bash_readline_and_sushline_complete_same_mapped_case_directory_without_marker() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("alpha-dir")).expect("mkdir fixture");
    let typed = format!("{}/alpha_dir\t\r", dir.path().display());
    let inputrc =
        "set completion-ignore-case on\nset completion-map-case on\nset mark-directories off";

    let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
    let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_complete_same_symlinked_directory_markers() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("target-dir")).expect("mkdir fixture");
    symlink(dir.path().join("target-dir"), dir.path().join("alpha-link")).expect("symlink fixture");
    let typed = format!("{}/alp\t\r", dir.path().display());

    for inputrc in [
        "",
        "set mark-symlinked-directories on",
        "set mark-directories off\nset mark-symlinked-directories on",
    ] {
        let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "inputrc={inputrc:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_complete_same_hidden_filename_cases() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join(".alpha"), "").expect("write hidden fixture");

    for (inputrc, typed) in [
        ("", format!("{}/.a\t\r", dir.path().display())),
        (
            "set match-hidden-files on",
            format!("{}/a\t\r", dir.path().display()),
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "inputrc={inputrc:?}\ntyped={typed:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_mark_directories_for_glob_completion_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("alpha-dir")).expect("mkdir fixture");
    fs::write(dir.path().join("alpha-file"), "").expect("write fixture");

    for (binding, typed) in [
        (
            "\"\\C-o\": glob-complete-word",
            format!("{}/al*\x0f\r", dir.path().display()),
        ),
        (
            "\"\\C-o\": glob-expand-word",
            format!("{}/al*\x0f\r", dir.path().display()),
        ),
        (
            "\"\\C-o\": glob-list-expansions\nset completion-query-items 999",
            format!("{}/al*\x0f\r", dir.path().display()),
        ),
        (
            "\"\\C-o\": insert-completions",
            format!("{}/al\x0f\r", dir.path().display()),
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), binding);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), binding);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "binding={binding:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_quote_same_completed_filename_with_space() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("alpha file");
    fs::write(&file, "").expect("write fixture");
    for typed in [
        format!("{}/alp\t\r", dir.path().display()),
        format!("{}/alpha\\ f\t\r", dir.path().display()),
    ] {
        let bash = run_bash_readline(typed.as_bytes());
        let sushline = run_sushline_harness(typed.as_bytes());

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "typed={typed:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_quote_same_completed_filename_with_shell_metacharacters() {
    for name in [
        "alpha'quote",
        "alpha$dollar",
        "alpha`tick`",
        "alpha[bracket]",
        "alpha#hash",
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join(name), "").expect("write fixture");
        let typed = format!("{}/alp\t\r", dir.path().display());

        let bash = run_bash_readline(typed.as_bytes());
        let sushline = run_sushline_harness(typed.as_bytes());

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "name={name:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_complete_same_quoted_filename_with_space() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("alpha file");
    fs::write(&file, "").expect("write fixture");
    let typed = format!("cat \"{}/alp\t\r", dir.path().display());

    let bash = run_bash_readline(typed.as_bytes());
    let sushline = run_sushline_harness(typed.as_bytes());

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_ring_same_bell_for_unmodified_ambiguous_completion() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("alpha"), "").expect("write alpha fixture");
    fs::write(dir.path().join("beta"), "").expect("write beta fixture");
    let typed = format!("{}/\t\r", dir.path().display());

    let bash = run_bash_readline(typed.as_bytes());
    let sushline = run_sushline_harness(typed.as_bytes());

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
    assert!(bell_count(&bash) > 0, "bash={bash}");
}

#[test]
fn bash_readline_and_sushline_ring_same_bell_when_ambiguous_completion_extends_prefix() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("alpha"), "").expect("write alpha fixture");
    fs::write(dir.path().join("alpine"), "").expect("write alpine fixture");
    let typed = format!("{}/a\t\r", dir.path().display());

    let bash = run_bash_readline(typed.as_bytes());
    let sushline = run_sushline_harness(typed.as_bytes());

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
fn bash_readline_and_sushline_ring_same_bell_for_ambiguous_complete_filename() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("alpha"), "").expect("write alpha fixture");
    fs::write(dir.path().join("alpine"), "").expect("write alpine fixture");
    let inputrc = r#""\C-o": complete-filename"#;
    let typed = format!("{}/a\x0f\r", dir.path().display());

    let bash = run_bash_readline_with_bindings(typed.as_bytes(), inputrc);
    let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

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
    assert!(bell_count(&bash) > 0, "bash={bash}");
}

#[test]
fn bash_readline_and_sushline_complete_same_filenames_into_braces() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("alpha"), "").expect("write alpha fixture");
    fs::write(dir.path().join("alpine"), "").expect("write alpine fixture");
    let inputrc = r#""\C-o": complete-into-braces"#;

    for typed in [
        format!("{}/al\x0f\r", dir.path().display()),
        format!("cat \"{}/al\x0f\r", dir.path().display()),
        format!("cat '{}/al\x0f\r", dir.path().display()),
    ] {
        let bash = run_bash_readline_with_bindings(typed.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "typed={typed:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_cycle_same_menu_completion_items() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("alpha"), "").expect("write alpha fixture");
    fs::write(dir.path().join("alpine"), "").expect("write alpine fixture");
    for (inputrc, typed) in [
        (
            r#""\C-o": menu-complete"#,
            format!("{}/al\x0f\x0f\x0f\r", dir.path().display()),
        ),
        (
            r#""\C-o": menu-complete-backward"#,
            format!("{}/al\x0f\r", dir.path().display()),
        ),
        (
            r#""\C-o": old-menu-complete"#,
            format!("{}/al\x0f\r", dir.path().display()),
        ),
    ] {
        let bash = run_bash_readline_with_bindings(typed.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "inputrc={inputrc:?}\ntyped={typed:?}\nbash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "inputrc={inputrc:?}\ntyped={typed:?}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_delete_char_or_list_matches_edit_and_list_cases() {
    let inputrc = r#""\C-o": delete-char-or-list"#;
    let bash = run_bash_readline_with_bindings(b"abc\x02\x0f\r", inputrc);
    let sushline = run_sushline_harness_with_inputrc(b"abc\x02\x0f\r", inputrc);
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "delete case\nbash={bash}\nsushline={sushline}"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("alpha"), "").expect("write alpha fixture");
    fs::write(dir.path().join("alpine"), "").expect("write alpine fixture");
    let inputrc = "\"\\C-o\": delete-char-or-list\nset completion-query-items 999";
    let typed = format!("{}/al\x0f\r", dir.path().display());
    let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
    let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "list case\nbash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_tilde_expand_preserves_spacing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().to_string_lossy().into_owned();
    let inputrc = r#""\C-o": tilde-expand"#;
    let env = [("HOME", home.as_str())];

    for keys in [
        b"echo   ~/sub\x0f\r".as_slice(),
        b"PATH=~/bin:~/lib\x0f\r".as_slice(),
        b"echo ~+ ~-\x0f\r".as_slice(),
    ] {
        let bash = run_bash_readline_with_bindings_and_env(keys, inputrc, &env);
        let sushline = run_sushline_harness_with_inputrc_and_env(keys, inputrc, &env);

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
fn bash_readline_and_sushline_complete_same_single_quoted_filename_with_space() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("alpha file");
    fs::write(&file, "").expect("write fixture");
    let typed = format!("cat '{}/alp\t\r", dir.path().display());

    let bash = run_bash_readline(typed.as_bytes());
    let sushline = run_sushline_harness(typed.as_bytes());

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_display_same_colored_filename_stats() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("alpha-dir")).expect("mkdir fixture");
    let typed = format!("{}/alp\x0f\r", dir.path().display());
    let inputrc = "\"\\C-o\": possible-filename-completions\nset colored-stats on\nset completion-query-items 999";
    let env = [("LS_COLORS", "di=35:fi=0")];

    let bash = run_bash_readline_with_inputrc_file_and_env(typed.as_bytes(), inputrc, &env);
    let sushline = run_sushline_harness_with_inputrc_and_env(typed.as_bytes(), inputrc, &env);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert!(bash.contains("\x1b[35m"), "{bash}");
    assert!(sushline.contains("\x1b[35m"), "{sushline}");
}

#[test]
fn bash_readline_and_sushline_complete_same_command_and_variable_fallbacks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let command_path = dir.path().join("sushlinecmd");
    fs::write(&command_path, "#!/bin/sh\n").expect("write command fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&command_path, fs::Permissions::from_mode(0o755))
            .expect("chmod command fixture");
    }

    let path = format!(
        "{}:{}",
        dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let bash = run_bash_readline_with_bindings_and_env(
        b"sushlinecm\x0f\r",
        "\"\\C-o\": complete-command",
        &[("PATH", &path)],
    );
    let sushline = run_sushline_harness_with_inputrc_and_env(
        b"sushlinecm\x0f\r",
        "\"\\C-o\": complete-command",
        &[("PATH", &path)],
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    let bash = run_bash_readline_with_bindings(b"ech\x0f\r", "\"\\C-o\": complete-command");
    let sushline = run_sushline_harness_with_inputrc(b"ech\x0f\r", "\"\\C-o\": complete-command");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    let env = [("SUSHLINE_COMPLETION_ORACLE", "1")];
    let bash = run_bash_readline_with_bindings_and_env(
        b"echo $SUSHLINE_COMPLETION_ORA\x0f\r",
        "\"\\C-o\": complete-variable",
        &env,
    );
    let sushline = run_sushline_harness_with_inputrc_and_env(
        b"echo $SUSHLINE_COMPLETION_ORA\x0f\r",
        "\"\\C-o\": complete-variable",
        &env,
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_complete_same_user_and_host_fallbacks() {
    let bash = run_bash_readline_with_bindings(b"~roo\x0f\r", "\"\\C-o\": complete-username");
    let sushline = run_sushline_harness_with_inputrc(b"~roo\x0f\r", "\"\\C-o\": complete-username");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    let bash = run_bash_readline_with_bindings(b"local\x0f\r", "\"\\C-o\": complete-hostname");
    let sushline =
        run_sushline_harness_with_inputrc(b"local\x0f\r", "\"\\C-o\": complete-hostname");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_vi_completion_bindings() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("alpha"), "").expect("write alpha fixture");
    fs::write(dir.path().join("alpine"), "").expect("write alpine fixture");
    let inputrc = "set editing-mode vi\nset completion-query-items 999";

    for key in [b'*', b'=', b'\\'] {
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
fn bash_readline_and_sushline_accept_same_edit_under_narrow_terminal() {
    let keys = b"abcdef ghijkl\x01X\r";
    let bash = run_bash_readline_with_size(
        keys,
        "",
        &[],
        PtySize {
            rows: 4,
            cols: 12,
            pixel_width: 0,
            pixel_height: 0,
        },
    );
    let sushline = run_sushline_harness_with_size(
        keys,
        "",
        &[],
        PtySize {
            rows: 4,
            cols: 12,
            pixel_width: 0,
            pixel_height: 0,
        },
    );

    assert_eq!(
        accepted_line(&bash),
        Some("Xabcdef ghijkl".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabcdef ghijkl".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_accept_same_screen_line_motion() {
    let size = PtySize {
        rows: 4,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    for (command, keys) in [
        (
            "previous-screen-line",
            b"abcdefghij klmnopqrst uvwxyz\x0fX\r".as_slice(),
        ),
        (
            "next-screen-line",
            b"abcdefghij klmnopqrst uvwxyz\x01\x0fX\r".as_slice(),
        ),
    ] {
        let inputrc = format!(r#""\C-o": {command}"#);
        let bash = run_bash_readline_with_size(keys, &inputrc, &[], size);
        let sushline = run_sushline_harness_with_size(keys, &inputrc, &[], size);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "command={command}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_history_timestamp_file_records_load_as_sushline_timestamps() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("history");
    let command = format!(
        "HISTFILE={}; HISTTIMEFORMAT='%s '; history -s 'echo one'; history -s 'printf two'; history -w",
        shell_single_quote(&path.to_string_lossy())
    );
    let output = Command::new("bash")
        .args(["--noprofile", "--norc", "-c", &command])
        .output()
        .expect("bash must be available for history timestamp oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let raw = fs::read_to_string(&path).expect("history file");
    assert!(raw.lines().step_by(2).all(|line| {
        line.strip_prefix('#')
            .is_some_and(|timestamp| timestamp.bytes().all(|byte| byte.is_ascii_digit()))
    }));
    let history = History::read_file(&path).expect("read timestamped history");
    assert_eq!(
        history
            .entries()
            .iter()
            .map(|entry| entry.line())
            .collect::<Vec<_>>(),
        vec!["echo one", "printf two"]
    );
    assert!(history.entries().iter().all(|entry| {
        entry
            .timestamp
            .as_deref()
            .is_some_and(|timestamp| timestamp.starts_with('#'))
    }));
}

fn run_bash_readline(keys: &[u8]) -> String {
    run_bash_readline_with_bindings(keys, "")
}

fn run_bash_readline_with_bindings(keys: &[u8], bindings: &str) -> String {
    run_bash_readline_with_bindings_and_history(keys, bindings, &[])
}

fn run_bash_readline_eof_with_inputrc(keys: &[u8], inputrc: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; if IFS= read -r -e -p "{READY_PROMPT}" line; then printf 'SUSHLINE_ACCEPTED:%s\n' "$line"; else printf 'SUSHLINE_EOF\n'; fi"#
        ),
    ]);
    run_pty_until(command, keys, "SUSHLINE_EOF")
}

fn run_bash_readline_with_bindings_and_env(
    keys: &[u8],
    bindings: &str,
    env: &[(&str, &str)],
) -> String {
    run_bash_readline_with_size_and_env(
        keys,
        bindings,
        &[],
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        env,
    )
}

fn run_bash_history_expand(expansion: &str, history: &[&str]) -> String {
    let history_commands = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    let output = Command::new("bash")
        .args([
            "--noprofile",
            "--norc",
            "-i",
            "-c",
            &format!(
                "set +H; {history_commands}; history -p {}",
                shell_single_quote(expansion)
            ),
        ])
        .output()
        .expect("bash must be available for history oracle tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .unwrap_or_default()
        .to_string()
}

fn run_bash_readline_with_inputrc_file(keys: &[u8], inputrc: &str) -> String {
    run_bash_readline_with_inputrc_file_and_env(keys, inputrc, &[])
}

fn run_bash_readline_with_inputrc_file_and_env(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    run_bash_readline_with_inputrc_path_and_env(keys, &path, env)
}

fn run_bash_readline_with_inputrc_file_and_history(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let mut history_commands = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    if history_commands.is_empty() {
        history_commands = ":".to_string();
    }
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; {history_commands}; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty(command, keys)
}

fn run_bash_readline_with_reloaded_inputrc(
    keys: &[u8],
    initial_inputrc: &str,
    reloaded_inputrc: &str,
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, initial_inputrc).expect("write initial inputrc");
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty_after_prompt(command, keys, || {
        fs::write(&path, reloaded_inputrc).expect("write reloaded inputrc");
    })
}

fn run_bash_readline_with_inputrc_path(keys: &[u8], path: &std::path::Path) -> String {
    run_bash_readline_with_inputrc_path_and_env(keys, path, &[])
}

fn run_bash_readline_with_inputrc_path_and_env(
    keys: &[u8],
    path: &std::path::Path,
    env: &[(&str, &str)],
) -> String {
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    for (name, value) in env {
        command.env(name, value);
    }
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty(command, keys)
}

fn run_bash_readline_two_reads_with_inputrc_file_and_history(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let mut history_commands = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    if history_commands.is_empty() {
        history_commands = ":".to_string();
    }
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; {history_commands}; IFS= read -r -e -p "{READY_PROMPT}" line1; printf 'SUSHLINE_ACCEPTED_1:%s\n' "$line1"; IFS= read -r -e -p "{READY_PROMPT}" line2; printf 'SUSHLINE_ACCEPTED_2:%s\n' "$line2""#
        ),
    ]);
    run_pty_until(command, keys, "SUSHLINE_ACCEPTED_2:")
}

fn run_bash_readline_with_bindings_and_history(
    keys: &[u8],
    bindings: &str,
    history: &[&str],
) -> String {
    run_bash_readline_with_size(
        keys,
        bindings,
        history,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
}

fn run_bash_readline_with_size(
    keys: &[u8],
    bindings: &str,
    history: &[&str],
    size: PtySize,
) -> String {
    run_bash_readline_with_size_and_env(keys, bindings, history, size, &[])
}

fn run_bash_readline_with_size_and_env(
    keys: &[u8],
    bindings: &str,
    history: &[&str],
    size: PtySize,
    env: &[(&str, &str)],
) -> String {
    let bind_command = shell_single_quote(bindings);
    let mut history_commands = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    if history_commands.is_empty() {
        history_commands = ":".to_string();
    }
    let mut command = CommandBuilder::new("bash");
    for (name, value) in env {
        command.env(name, value);
    }
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; {history_commands}; if [ -n {bind_command} ]; then bind {bind_command}; fi; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty_with_size(command, keys, size)
}

fn run_sushline_harness(keys: &[u8]) -> String {
    run_sushline_harness_with_inputrc(keys, "")
}

fn run_sushline_harness_with_inputrc(keys: &[u8], inputrc: &str) -> String {
    run_sushline_harness_with_inputrc_and_history(keys, inputrc, &[])
}

fn run_sushline_harness_until(keys: &[u8], inputrc: &str, stop_marker: &str) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    run_pty_until(command, keys, stop_marker)
}

fn run_sushline_harness_with_inputrc_and_env(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    for (name, value) in env {
        command.env(name, value);
    }
    run_pty(command, keys)
}

fn run_sushline_harness_with_inputrc_path(keys: &[u8], path: &std::path::Path) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC_FILE", path.to_string_lossy().as_ref());
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    run_pty(command, keys)
}

fn run_sushline_harness_with_reloaded_inputrc(
    keys: &[u8],
    initial_inputrc: &str,
    reloaded_inputrc: &str,
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, initial_inputrc).expect("write initial inputrc");
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC_FILE", path.to_string_lossy().as_ref());
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    run_pty_after_prompt(command, keys, || {
        fs::write(&path, reloaded_inputrc).expect("write reloaded inputrc");
    })
}

fn run_sushline_harness_with_inputrc_and_history(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
) -> String {
    run_sushline_harness_with_size(
        keys,
        inputrc,
        history,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
}

fn run_sushline_harness_with_size(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
    size: PtySize,
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", history.join("\n"));
    run_pty_with_size(command, keys, size)
}

fn run_sushline_harness_two_reads_with_inputrc_and_history(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", history.join("\n"));
    command.env("SUSHLINE_READS", "2");
    run_pty_until(command, keys, "SUSHLINE_ACCEPTED_2:")
}

fn run_pty(command: CommandBuilder, keys: &[u8]) -> String {
    run_pty_with_size(
        command,
        keys,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
}

fn run_pty_with_size(command: CommandBuilder, keys: &[u8], size: PtySize) -> String {
    run_pty_with_size_until(command, keys, size, "SUSHLINE_ACCEPTED:")
}

fn run_pty_until(command: CommandBuilder, keys: &[u8], stop_marker: &str) -> String {
    run_pty_with_size_until(
        command,
        keys,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        stop_marker,
    )
}

fn run_pty_after_prompt(
    command: CommandBuilder,
    keys: &[u8],
    after_prompt: impl FnOnce(),
) -> String {
    run_pty_with_size_until_after_prompt(
        command,
        keys,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        "SUSHLINE_ACCEPTED:",
        Some(after_prompt),
    )
}

fn run_pty_with_size_until(
    command: CommandBuilder,
    keys: &[u8],
    size: PtySize,
    stop_marker: &str,
) -> String {
    run_pty_with_size_until_after_prompt(command, keys, size, stop_marker, None::<fn()>)
}

fn run_pty_with_size_until_after_prompt(
    mut command: CommandBuilder,
    keys: &[u8],
    size: PtySize,
    stop_marker: &str,
    mut after_prompt: Option<impl FnOnce()>,
) -> String {
    command.env("TERM", "xterm-256color");
    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(size).expect("open pty");

    let mut child = pair.slave.spawn_command(command).expect("spawn command");
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().expect("pty reader");
    let mut writer = pair.master.take_writer().expect("pty writer");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut out = Vec::new();
    let mut buf = [0_u8; 1024];
    let mut sent_keys = false;

    while Instant::now() < deadline {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if !sent_keys && String::from_utf8_lossy(&out).contains(READY_PROMPT) {
                    if let Some(after_prompt) = after_prompt.take() {
                        after_prompt();
                    }
                    writer.write_all(keys).expect("write keys");
                    writer.flush().expect("flush keys");
                    sent_keys = true;
                }
                if String::from_utf8_lossy(&out).contains(stop_marker) {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    String::from_utf8_lossy(&out).into_owned()
}

fn accepted_line(output: &str) -> Option<String> {
    let marker = "SUSHLINE_ACCEPTED:";
    accepted_line_after_marker(output, marker)
}

fn accepted_numbered_line(output: &str, number: usize) -> Option<String> {
    accepted_line_after_marker(output, &format!("SUSHLINE_ACCEPTED_{number}:"))
}

fn accepted_line_after_marker(output: &str, marker: &str) -> Option<String> {
    let start = output.find(marker)? + marker.len();
    let rest = &output[start..];
    let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn bell_count(output: &str) -> usize {
    output.bytes().filter(|byte| *byte == b'\x07').count()
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
