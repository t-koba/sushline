#![allow(unused_imports)]
use super::pty::*;
use readline::History;
use std::fs;
use std::process::Command;

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
