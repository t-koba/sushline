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
fn sushline_resolves_relative_inputrc_include_from_inputrc_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("inputrc");
    let included = dir.path().join("included.inputrc");
    fs::write(&included, "\"\\C-x\\C-a\": beginning-of-line\n").expect("write included inputrc");
    fs::write(&main, "$include included.inputrc\n").expect("write main inputrc");
    let keys = b"abc\x18\x01X\r";

    let sushline = run_sushline_harness_with_inputrc_path(keys, &main);

    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
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
    let keys = b"one two\x01\x1bu\x1bc\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("ONE Two".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("ONE Two".to_string()),
        "{sushline}"
    );
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
fn bash_readline_and_sushline_accept_same_simple_undo_edit() {
    let keys = b"abc\x1f\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(accepted_line(&sushline), Some(String::new()), "{sushline}");
}

#[test]
fn sushline_keyboard_macro_records_consecutive_self_insert_keys() {
    let keys = b"\x18(abc\x18)\x18e\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("abca".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abcabc".to_string()),
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
fn bash_readline_and_sushline_accept_same_numeric_kill_word() {
    let keys = b"one two three\x01\x1b2\x1bdX\x19\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(
        accepted_line(&bash),
        Some("Xone two three".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("Xone two three".to_string()),
        "{sushline}"
    );
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
        Some("Xone".to_string()),
        "{sushline}"
    );
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
    let keys = b"echo foo|bar\x01\x0fX\r";
    let inputrc = r#""\C-o": shell-forward-word"#;
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
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
    let typed = format!("{}/alp\t\r", dir.path().display());

    let bash = run_bash_readline(typed.as_bytes());
    let sushline = run_sushline_harness(typed.as_bytes());

    assert_eq!(
        accepted_line(&bash),
        Some(format!("{}/alpha file ", dir.path().display())),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some(format!("{}/alpha\\ file ", dir.path().display())),
        "{sushline}"
    );
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
        Some("xxc".to_string()),
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
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty(command, keys)
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
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; {history_commands}; if [ -n {bind_command} ]; then bind {bind_command}; fi; IFS= read -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
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

fn run_pty_with_size(mut command: CommandBuilder, keys: &[u8], size: PtySize) -> String {
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
                    writer.write_all(keys).expect("write keys");
                    writer.flush().expect("flush keys");
                    sent_keys = true;
                }
                if String::from_utf8_lossy(&out).contains("SUSHLINE_ACCEPTED:") {
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
    let start = output.find(marker)? + marker.len();
    let rest = &output[start..];
    let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
