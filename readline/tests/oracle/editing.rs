//! Editing compatibility cases for key dispatch, movement/insertion, signals,
//! paste, numeric arguments, and keyboard macros. This broad group spans several
//! fixture-scoped claims in `COMPATIBILITY.md` and may need finer grouping if a
//! later risk audit demonstrates useful separation. The named shell-word
//! movement/kill/transpose fixtures cited by the `COMPATIBILITY.md`
//! shell-forward-word, shell-backward-word, shell-transpose-words,
//! shell-kill-word, and shell-backward-kill-word rows live in `completion.rs`,
//! not in this group.

use super::host::require_oracle_host;
use super::pty::*;
use super::screen::Screen;
use std::fs;
use std::time::Duration;

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
fn bash_drops_the_input_that_arrived_with_the_interrupt_while_sushline_keeps_it() {
    // This harness explicitly selects SigintPolicy::Interrupted. It keeps
    // suffix bytes for the next read; the Bash host drops them. This is not
    // a comparison of the built-in default with the direct GNU library.
    let bash = run_bash_interactive_after_ctrl_c_with_keys(b"abc\x03X\r");
    let sushline = run_sushline_harness_after_ctrl_c_with_keys(b"abc\x03X\r");

    // The Bash host drops the trailing input and accepts the line that
    // follows it.
    assert!(bash.contains("SUSHLINE_ACCEPTED:ok"), "{bash}");
    assert!(!bash.contains("X: command not found"), "{bash}");
    // Sushline's explicit Interrupted policy keeps the trailing input.
    assert!(sushline.contains("SUSHLINE_ACCEPTED_2:X"), "{sushline}");
}

#[test]
fn sushline_ctrl_c_signal_interrupts_readline_without_io_error() {
    require_oracle_host();
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
    assert!(bash.contains("SUSHLINE_READY1>^C"), "{bash}");
    assert!(bash.contains("SUSHLINE_READY2>"), "{bash}");
    assert!(sushline.contains("SUSHLINE_READY1:^C"), "{sushline}");
    assert!(sushline.contains("SUSHLINE_READY2:"), "{sushline}");
    assert!(bash.contains("SUSHLINE_ACCEPTED:ok"), "{bash}");
    assert_eq!(accepted_numbered_line(&sushline, 2), Some("ok".to_string()));
    assert!(!sushline.contains("Interrupted system call"), "{sushline}");
    assert!(!sushline.contains("SUSHLINE_ERROR"), "{sushline}");
}

#[test]
fn bash_and_sushline_del_inputrc_macro_precedence_matches_with_tty_binding_on_and_off() {
    let keys = b"ab\x7f\r";
    for enabled in ["on", "off"] {
        let inputrc = format!("set bind-tty-special-chars {enabled}\n\"\\x7f\": \"Q\"\n");
        let bash = run_bash_readline_with_inputrc_file(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        assert_eq!(accepted_line(&bash).as_deref(), Some("abQ"));
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("abQ"),
            "bind-tty-special-chars {enabled}"
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
#[test]
fn bash_and_sushline_tty_special_binding_precedence_covers_tty_specials() {
    let cases = [
        ("kill", b"ab\x15X\r".as_slice(), "\"\\C-u\": \"Q\"", "abQX"),
        (
            "word erase",
            b"alpha beta\x17X\r".as_slice(),
            "\"\\C-w\": \"Q\"",
            "alpha betaQX",
        ),
        ("eof", b"ab\x04X\r".as_slice(), "\"\\C-d\": \"Q\"", "abQX"),
    ];
    for (label, keys, binding, expected) in cases {
        let inputrc = format!("set bind-tty-special-chars on\n{binding}\n");
        let bash = run_bash_readline_with_inputrc_file(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        assert_eq!(accepted_line(&bash).as_deref(), Some(expected), "{label}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
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
fn bash_readline_and_sushline_word_motion_honors_numeric_argument() {
    let interior = b"one two three\x01\x06\x06\x06\x06\x06";
    // Expected lines for Meta-f from line start and inside "two", then
    // for Meta-b from line end and inside "two". Labels describe the keys:
    // `negative_one` sends M-- M-1, which composes an argument of -11.
    for (desc, prefix, forward, backward) in [
        (
            "uncounted",
            b"".as_slice(),
            ["oneX two three", "one twoX three"],
            ["one two Xthree", "one Xtwo three"],
        ),
        (
            "zero",
            b"\x1b0".as_slice(),
            ["Xone two three", "one tXwo three"],
            ["one two threeX", "one tXwo three"],
        ),
        (
            "one",
            b"\x1b1".as_slice(),
            ["oneX two three", "one twoX three"],
            ["one two Xthree", "one Xtwo three"],
        ),
        (
            "two",
            b"\x1b2".as_slice(),
            ["one twoX three", "one two threeX"],
            ["one Xtwo three", "Xone two three"],
        ),
        (
            "three",
            b"\x1b3".as_slice(),
            ["one two threeX", "one two threeX"],
            ["Xone two three", "Xone two three"],
        ),
        (
            "negative_one",
            b"\x1b-\x1b1".as_slice(),
            ["Xone two three", "Xone two three"],
            ["one two threeX", "one two threeX"],
        ),
        (
            "nine",
            b"\x1b9".as_slice(),
            ["one two threeX", "one two threeX"],
            ["Xone two three", "Xone two three"],
        ),
    ] {
        for (position, start, motion, expected) in [
            (
                "start",
                b"one two three\x01".as_slice(),
                b"\x1bfX\r",
                forward[0],
            ),
            ("end", b"one two three".as_slice(), b"\x1bbX\r", backward[0]),
            (
                "interior forward",
                interior.as_slice(),
                b"\x1bfX\r",
                forward[1],
            ),
            (
                "interior backward",
                interior.as_slice(),
                b"\x1bbX\r",
                backward[1],
            ),
        ] {
            let mut keys = start.to_vec();
            keys.extend_from_slice(prefix);
            keys.extend_from_slice(motion);
            let bash = run_bash_readline(&keys);
            let sushline = run_sushline_harness(&keys);

            for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
                assert_eq!(
                    accepted_line(output).as_deref(),
                    Some(expected),
                    "{editor} {position}, {desc}: {output:?}"
                );
                assert_eq!(
                    bell_count(output),
                    0,
                    "{editor} {position}, {desc}: {output:?}"
                );
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_compose_meta_minus_and_meta_digits() {
    if !host_carries_measured_release() {
        eprintln!(
            "Meta-minus composition was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    // Point 13 distinguishes -1, -11, -12 and -10 without clamping. A
    // Meta-digit establishes an explicit magnitude, so a later bare digit
    // appends instead of replacing; -101 reaches the start of the line and
    // rings once when the following C-f cannot move.
    for (prefix, expected, bells) in [
        (b"\x1b-".as_slice(), "abcdefghijklXmnopqrstuvwxyz", 0),
        (b"\x1b-\x1b1".as_slice(), "abXcdefghijklmnopqrstuvwxyz", 0),
        (b"\x1b-\x1b2".as_slice(), "aXbcdefghijklmnopqrstuvwxyz", 0),
        (b"\x1b-\x1b0".as_slice(), "abcXdefghijklmnopqrstuvwxyz", 0),
        (
            b"\x1b-\x1b-\x1b1".as_slice(),
            "abcdefghijklmnXopqrstuvwxyz",
            0,
        ),
        (b"\x1b-\x1b01".as_slice(), "Xabcdefghijklmnopqrstuvwxyz", 1),
        (
            b"\x1b-\x1b0\x1b1".as_slice(),
            "Xabcdefghijklmnopqrstuvwxyz",
            1,
        ),
        (b"\x1b-01".as_slice(), "abcdefghijklXmnopqrstuvwxyz", 0),
        (b"\x1b-0\x1b1".as_slice(), "abcdefghijklXmnopqrstuvwxyz", 0),
    ] {
        let mut keys = b"abcdefghijklmnopqrstuvwxyz\x01".to_vec();
        keys.extend_from_slice(&[0x06; 13]);
        keys.extend_from_slice(prefix);
        keys.extend_from_slice(b"\x06X\r");
        let bash = run_bash_readline(&keys);
        let sushline = run_sushline_harness(&keys);

        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{editor} {prefix:?}: {output:?}"
            );
            assert_eq!(bell_count(output), bells, "{editor} {prefix:?}: {output:?}");
        }
    }
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
fn bash_and_sushline_c_locale_backward_char_is_bytewise() {
    assert_character_motion_bytes("中x\x02\x02Y\r".as_bytes(), "59e4b8ad78");
}

#[test]
fn bash_and_sushline_c_locale_forward_char_is_bytewise() {
    assert_character_motion_bytes("中x\x01\x06\x06Y\r".as_bytes(), "e4b8ad7859");
}

fn assert_character_motion_bytes(keys: &[u8], scalar_hex: &str) {
    // Bash 5.3 PTY runtime evidence only, not GNU source/API evidence or
    // physical glyph width: two steps and Y insert e4 b8 59 ad 78 in byte
    // mode, versus Y中x (backward) / 中xY (forward) in UTF-8 scalar mode.
    // Hex markers avoid losing the invalid UTF-8 created by bytewise insertion.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let bash_script = format!(
        r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'BASH_ACCEPTED_HEX:%s\n' "$(printf '%s' "$line" | od -An -v -tx1 | tr -d ' \n')""#
    );
    for locale in ["C", "C.UTF-8"] {
        if locale == "C.UTF-8" && !c_utf8_locale_available() {
            eprintln!("skipping C.UTF-8 character-motion cases: locale unavailable");
            continue;
        }
        for byte_oriented in ["off", "on"] {
            let inputrc = format!(
                "set input-meta on\nset output-meta on\nset convert-meta off\nset meta-flag off\nset byte-oriented {byte_oriented}\n"
            );
            fs::write(&path, &inputrc).expect("write inputrc");
            let mut bash_command = portable_pty::CommandBuilder::new("bash");
            bash_command.env("INPUTRC", path.to_string_lossy().as_ref());
            bash_command.env("LC_ALL", locale);
            bash_command.args(["--noprofile", "--norc", "-i", "-c", &bash_script]);
            let bash = run_pty_until(bash_command, keys, "BASH_ACCEPTED_HEX:");

            let mut sushline_command = sushline_harness_command();
            sushline_command.env("SUSHLINE_INPUTRC", &inputrc);
            sushline_command.env("SUSHLINE_PROMPT", READY_PROMPT);
            sushline_command.env("SUSHLINE_HISTORY", "");
            sushline_command.env("SUSHLINE_ACCEPTED_HEX", "1");
            sushline_command.env("LC_ALL", locale);
            let sushline = run_pty_until(sushline_command, keys, "SUSHLINE_ACCEPTED_HEX:");

            let expected = if locale == "C.UTF-8" && byte_oriented == "off" {
                scalar_hex
            } else {
                "e4b859ad78"
            };
            // Both markers must exist and independently match the expected
            // ASCII hex; equality of two absent or lossy lines cannot pass.
            for (output, marker) in [
                (&bash, "BASH_ACCEPTED_HEX:"),
                (&sushline, "SUSHLINE_ACCEPTED_HEX:"),
            ] {
                assert_eq!(
                    accepted_line_after_marker(output, marker).as_deref(),
                    Some(expected),
                    "LC_ALL={locale}, byte-oriented={byte_oriented}, keys={keys:?}; {output}"
                );
            }
        }
    }
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
fn bash_readline_and_sushline_keep_keyboard_macro_recording_across_prompts() {
    // GNU Readline keeps an unfinished keyboard-macro recording across
    // prompts: the definition started in the first read continues in the
    // next one, and the accept-line that ended the first read is part of
    // the recorded macro.
    let keys = b"\x18(ABC\rDEF\x18)\x18e\r";
    let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(keys, "", &[]);
    let sushline = run_sushline_harness_two_reads_with_inputrc_and_history(keys, "", &[]);

    for number in 1..=2 {
        assert_eq!(
            accepted_numbered_line(&sushline, number),
            accepted_numbered_line(&bash, number),
            "read {number}\nbash={bash}\nsushline={sushline}"
        );
    }
    assert_eq!(
        accepted_numbered_line(&bash, 1),
        Some("ABC".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 2),
        Some("DEFA".to_string()),
        "{bash}"
    );
}

#[test]
fn bash_readline_and_sushline_continue_keyboard_macro_replay_across_prompts() {
    // GNU Readline keeps macro bytes in the input stream, so a replay that
    // accepts a line continues in the next prompt.
    let keys = b"\x18(ABC\rDEF\x18)\x18e\r\r";
    let bash = run_bash_readline_three_reads_with_inputrc_file(keys, "");
    let sushline = run_sushline_harness_three_reads_with_inputrc(keys, "");

    for number in 1..=3 {
        assert_eq!(
            accepted_numbered_line(&sushline, number),
            accepted_numbered_line(&bash, number),
            "read {number}\nbash={bash}\nsushline={sushline}"
        );
    }
    assert_eq!(
        accepted_numbered_line(&bash, 1),
        Some("ABC".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 2),
        Some("DEFA".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 3),
        Some("D".to_string()),
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
fn bash_readline_and_sushline_kill_line_resets_mark() {
    let mut observed = Vec::new();
    let mut expected = Vec::new();
    for (keys, line) in [
        (
            b"abcdef\x01\x06\x06\x00\x06\x06\x0b\x18\x18X\r".as_slice(),
            "abcdX",
        ),
        (b"abc\x01\x00\x05\x0b\x18\x18X\r".as_slice(), "abcX"),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            observed.push((host, accepted_line(output), bell_count(output)));
            expected.push((host, Some(line.to_string()), 0));
        }
    }
    assert_eq!(observed, expected);
}

#[test]
fn bash_readline_and_sushline_backward_and_unix_line_kill_mark_parity() {
    // Pinned Bash 5.3 / Readline 8.3 observations: the non-negative no-ops
    // retain the mark; the negative backward-kill-line no-op and these real
    // kills reset it. No other kills are covered.
    for (keys, line, bells) in [
        (b"abc\x05\x00\x01\x18\x7f\x18\x18X\r".as_slice(), "abcX", 1),
        (
            b"abcdef\x01\x06\x06\x00\x05\x1b-\x18\x7f\x18\x18X\r".as_slice(),
            "abcdefX",
            0,
        ),
        (
            b"abcdef\x05\x00\x01\x06\x06\x06\x15\x18\x18X\r".as_slice(),
            "Xdef",
            0,
        ),
        (b"abc\x05\x00\x01\x15\x18\x18X\r".as_slice(), "abcX", 1),
        (b"abcdef\x01\x00\x05\x18\x7f\x18\x18X\r".as_slice(), "X", 0),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(line),
                "keys={keys:?} {host}: {output}"
            );
            assert_eq!(bell_count(output), bells, "keys={keys:?} {host}: {output}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_word_kill_mark_parity() {
    // Pin only these word kills followed by exchange-point-and-mark.
    for keys in [
        b"abcdef\x05\x00\x01\x1b\x64\x18\x18X\r".as_slice(),
        b"abcdef\x00\x05\x1b\x7f\x18\x18X\r".as_slice(),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some("X"),
                "keys={keys:?} {host}: {output}"
            );
            assert_eq!(bell_count(output), 0, "keys={keys:?} {host}: {output}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_negative_kill_line_bell_parity() {
    for (keys, expected, bells) in [
        (b"\x1b-\x0b\r".as_slice(), "", 1),
        (b"abc\x01\x1b-\x0bX\r".as_slice(), "Xabc", 1),
        (b"abc\x1b-\x0bX\r".as_slice(), "X", 0),
        (b"abc\x01\x0bX\r".as_slice(), "X", 0),
        (b"abc\x01\x18\x7fX\r".as_slice(), "Xabc", 1),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);

        for (name, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "keys={keys:?} {name}: {output}"
            );
            assert_eq!(bell_count(output), bells, "keys={keys:?} {name}: {output}");
        }
    }
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
fn bash_readline_and_sushline_delete_horizontal_space_ignores_numeric_argument() {
    for (desc, prefix) in [
        ("uncounted", b"".as_slice()),
        ("zero", b"\x1b0".as_slice()),
        ("one", b"\x1b1".as_slice()),
        ("two", b"\x1b2".as_slice()),
        ("negative_one", b"\x1b-\x1b1".as_slice()),
    ] {
        let mut keys = b"one   two\x1bb".to_vec();
        keys.extend_from_slice(prefix);
        keys.extend_from_slice(b"\x1b\\X\r");
        let bash = run_bash_readline(&keys);
        let sushline = run_sushline_harness(&keys);

        assert_eq!(
            accepted_line(&bash),
            Some("oneXtwo".to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        println!(
            "{desc}: bash bells={} sush bells={}",
            bell_count(&bash),
            bell_count(&sushline)
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "{desc}: {sushline}"
        );
    }
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
fn bash_readline_and_sushline_case_word_counts_from_inside_word() {
    for (command, [negative_one, negative_multiple, positive_two]) in [
        (
            b'u',
            [
                "one TXwo three four",
                "ONE TXwo three four",
                "one tWO THREEX four",
            ],
        ),
        (
            b'c',
            [
                "one TXwo three four",
                "One TXwo three four",
                "one tWo ThreeX four",
            ],
        ),
        (
            b'l',
            [
                "one tXwo three four",
                "one tXwo three four",
                "one two threeX four",
            ],
        ),
    ] {
        for (prefix, expected) in [
            (b"\x1b-1".as_slice(), negative_one),
            (b"\x1b-2".as_slice(), negative_multiple),
            (b"\x1b-3".as_slice(), negative_multiple),
            (b"\x1b2".as_slice(), positive_two),
        ] {
            let keys = [
                b"one two three four\x01\x06\x06\x06\x06\x06".as_slice(),
                prefix,
                &[0x1b, command],
                b"X\r",
            ]
            .concat();
            let bash = run_bash_readline(&keys);
            let sushline = run_sushline_harness(&keys);
            for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
                assert_eq!(
                    accepted_line(output).as_deref(),
                    Some(expected),
                    "keys={keys:?} {host}: {output}"
                );
                assert_eq!(bell_count(output), 0, "keys={keys:?} {host}: {output}");
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_negative_case_word_counts_reach_earlier_uppercase_word() {
    // A lowercase earlier word hides whether a negative count reached it;
    // with `ONE TWO` GNU downcases the earlier word at counts -2 and -3.
    for (command, expected) in [(b'l', "one tXWO three four"), (b'u', "ONE TXWO three four")] {
        for prefix in [b"\x1b-2".as_slice(), b"\x1b-3".as_slice()] {
            let keys = [
                b"ONE TWO three four\x01\x06\x06\x06\x06\x06".as_slice(),
                prefix,
                &[0x1b, command],
                b"X\r",
            ]
            .concat();
            let bash = run_bash_readline(&keys);
            let sushline = run_sushline_harness(&keys);
            for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
                assert_eq!(
                    accepted_line(output).as_deref(),
                    Some(expected),
                    "keys={keys:?} {host}: {output}"
                );
                assert_eq!(bell_count(output), 0, "keys={keys:?} {host}: {output}");
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_transpose_chars_numeric_arguments() {
    // Paired Bash 5.3 numeric PTY cases assert accepted lines only; they do
    // not claim redisplay or bell parity. Selected bell cases are covered
    // by bash_readline_and_sushline_noop_deletions_ring_like_bash.
    let inputrc = r#""\C-o": transpose-chars"#;
    type Case = (
        &'static str,
        &'static str,
        &'static [u8],
        &'static [u8],
        &'static str,
    );
    let cases: &[Case] = &[
        (
            "interior_plain",
            "one two three four",
            b"\x01\x06\x06\x06",
            b"",
            "on eXtwo three four",
        ),
        (
            "interior_positive_2",
            "one two three four",
            b"\x01\x06\x06\x06",
            b"\x1b2",
            "on teXwo three four",
        ),
        (
            "interior_positive_3",
            "one two three four",
            b"\x01\x06\x06\x06",
            b"\x1b3",
            "on tweXo three four",
        ),
        (
            "interior_negative_1",
            "one two three four",
            b"\x01\x06\x06\x06",
            b"\x1b-1",
            "oneX two three four",
        ),
        (
            "interior_zero",
            "one two three four",
            b"\x01\x06\x06\x06",
            b"\x1b0",
            "oneX two three four",
        ),
        (
            "eol_plain",
            "one two three four",
            b"",
            b"",
            "one two three foruX",
        ),
        (
            "eol_positive_2",
            "one two three four",
            b"",
            b"\x1b2",
            "one two three foruX",
        ),
        (
            "eol_sign_only_negative",
            "one two three four",
            b"",
            b"\x1b-",
            "one two three foruX",
        ),
        (
            "eol_negative_1",
            "one two three four",
            b"",
            b"\x1b-1",
            "one two three foruX",
        ),
        (
            "eol_negative_2",
            "one two three four",
            b"",
            b"\x1b-2",
            "one two three foruX",
        ),
        (
            "eol_zero",
            "one two three four",
            b"",
            b"\x1b0",
            "one two three fourX",
        ),
        (
            "positive_count_clamped_at_end",
            "abcdefghij",
            b"\x01\x06\x06\x06\x06\x06\x06\x06\x06",
            b"\x1b4",
            "abcdefgijhX",
        ),
    ];
    for (name, line, point_keys, argument, expected) in cases {
        let mut keys = line.as_bytes().to_vec();
        keys.extend_from_slice(point_keys);
        keys.extend_from_slice(argument);
        keys.extend_from_slice(b"\x0fX\r");
        let bash = run_bash_readline_with_bindings(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{name}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected.to_string()),
            "{name}: {sushline}"
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
fn bash_readline_and_sushline_transpose_words_numeric_arguments() {
    // The inserted X exposes accepted-line cursor placement; display and bell
    // behavior are not asserted by these numeric cases.
    let inputrc = r#""\C-o": transpose-words"#;
    type Case = (&'static str, Option<usize>, &'static [u8], &'static str);
    let cases: &[Case] = &[
        ("interior_plain", Some(3), b"", "two oneX three four"),
        (
            "interior_positive_2",
            Some(3),
            b"\x1b2",
            "three two oneX four",
        ),
        (
            "interior_positive_3",
            Some(3),
            b"\x1b3",
            "four two three oneX",
        ),
        (
            "interior_negative_1",
            Some(3),
            b"\x1b-1",
            "oneX two three four",
        ),
        (
            "interior_negative_2",
            Some(3),
            b"\x1b-2",
            "oneX two three four",
        ),
        ("interior_zero", Some(3), b"\x1b0", "oneX two three four"),
        (
            "after_two_positive_2",
            Some(7),
            b"\x1b2",
            "one four three twoX",
        ),
        (
            "after_two_negative_1",
            Some(7),
            b"\x1b-1",
            "one twoX three four",
        ),
        (
            "after_three_negative_1",
            Some(13),
            b"\x1b-1",
            "one two threeX four",
        ),
        ("eol_plain", None, b"", "one two four threeX"),
        ("eol_positive_2", None, b"\x1b2", "one four three twoX"),
        ("eol_positive_3", None, b"\x1b3", "four two three oneX"),
        ("eol_negative_1", None, b"\x1b-1", "one two three fourX"),
        ("eol_negative_2", None, b"\x1b-2", "one two three fourX"),
        ("eol_zero", None, b"\x1b0", "one two three fourX"),
    ];
    for (name, position, argument, expected) in cases {
        let mut keys = b"one two three four".to_vec();
        if let Some(forward_chars) = position {
            keys.push(0x01);
            keys.extend(std::iter::repeat_n(0x06, *forward_chars));
        }
        keys.extend_from_slice(argument);
        keys.extend_from_slice(b"\x0fX\r");
        let bash = run_bash_readline_with_bindings(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{name}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected.to_string()),
            "{name}: {sushline}"
        );
    }
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
fn bash_readline_and_sushline_kill_whole_line_ignores_numeric_prefix() {
    let inputrc = r#""\C-o": kill-whole-line"#;
    for keys in [
        b"abc def\x1b2\x0fX\r".as_slice(),
        b"abc def\x1b0\x0fX\r".as_slice(),
        b"abc def\x1b-1\x0fX\r".as_slice(),
        b"abc def\x1b2\x0f\x0bX\r".as_slice(),
        b"abc def\x1b2\x0f\x01\x0bX\r".as_slice(),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        for (name, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some("X"),
                "{name} keys={keys:?}: {output:?}"
            );
            assert_eq!(bell_count(output), 0, "{name} keys={keys:?}: {output:?}");
        }
    }
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
fn bash_and_sushline_numeric_argument_insert_opens_own_undo_entry() {
    // A numeric-argument insert opens its own entry: one undo leaves `ab`,
    // and a second undo removes the earlier grouped insert.
    for (keys, expected) in [
        (b"ab\x1b5a\x1f\r".as_slice(), "ab"),
        (b"ab\x1b5a\x1f\x1f\r".as_slice(), ""),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);

        assert_eq!(accepted_line(&bash).as_deref(), Some(expected), "{bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_match_single_byte_undo_group_boundaries() {
    // GNU groups at most 20 single-byte self-inserts: one undo removes all
    // 20, but after 21 it removes only the final insert. Multibyte inserts
    // split that group, so undoing `aé` removes only `é`.
    for (count, expected) in [(20, ""), (21, "aaaaaaaaaaaaaaaaaaaa")] {
        let mut keys = vec![b'a'; count];
        keys.extend_from_slice(b"\x1f\r");
        let bash = run_bash_readline(&keys);
        let sushline = run_sushline_harness(&keys);

        assert_eq!(accepted_line(&bash).as_deref(), Some(expected), "{bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{sushline}"
        );
    }

    let mut keys = Vec::from("aé".as_bytes());
    keys.extend_from_slice(b"\x1f\r");
    let bash = run_bash_readline(&keys);
    let sushline = run_sushline_harness(&keys);

    assert_eq!(accepted_line(&bash).as_deref(), Some("a"), "{bash}");
    assert_eq!(accepted_line(&sushline).as_deref(), Some("a"), "{sushline}");
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
        assert_eq!(
            bell_count(&sushline),
            bash_bells,
            "Sushline {name}: {sushline:?}"
        );
        let expected_diagnostics = usize::from(name == "negative repetitions");
        let diagnostic = "readline: maximum macro execution nesting level exceeded";
        assert_eq!(
            bash.matches(diagnostic).count(),
            expected_diagnostics,
            "Bash {name}: {bash:?}"
        );
        assert_eq!(
            sushline.matches(diagnostic).count(),
            expected_diagnostics,
            "Sushline {name}: {sushline:?}"
        );
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
    require_oracle_host();
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
        accepted_line(&bash),
        Some("Xabc".to_string()),
        "Bash: {bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "Sushline: {sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_complete_named_command_names() {
    if !host_carries_measured_release() {
        eprintln!(
            "Named command completion was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for keys in [
        b"abc\x1bxbegin\tline\rX\r".as_slice(),
        b"abc\x1bxbegin line\rX\r".as_slice(),
        b"abc\x1bxbeginning-of-\tline\rX\r".as_slice(),
    ] {
        for (name, output) in [
            ("Bash", run_bash_readline_with_inputrc_file(keys, "")),
            ("Sushline", run_sushline_harness_with_inputrc(keys, "")),
        ] {
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some("Xabc"),
                "{name} keys={keys:?}: {output:?}"
            );
            assert_eq!(bell_count(&output), 0, "{name} keys={keys:?}: {output:?}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_list_ambiguous_named_command_candidates() {
    if !host_carries_measured_release() {
        eprintln!("Ambiguous named-command listing was measured on Readline 8.3");
        return;
    }
    let keys = b"abc\x1bxbeginning-of-\tline\rX\r";
    for (name, output) in [
        ("Bash", run_bash_readline_with_inputrc_file(keys, "")),
        ("Sushline", run_sushline_harness_with_inputrc(keys, "")),
    ] {
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("Xabc"),
            "{name} keys={keys:?}: {output:?}"
        );
        assert_eq!(bell_count(&output), 0, "{name} keys={keys:?}: {output:?}");
        for candidate in ["beginning-of-history", "beginning-of-line"] {
            assert!(
                output.contains(candidate),
                "{name} did not list {candidate}: {output:?}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_list_empty_named_command_candidates_without_confirmation() {
    if !host_carries_measured_release() {
        eprintln!("Empty named-command listing was measured on Readline 8.3");
        return;
    }
    let keys = b"abc\x1bx\t\rX\r";
    let inputrc = "set page-completions off\n";
    for (name, output) in [
        ("Bash", run_bash_readline_with_inputrc_file(keys, inputrc)),
        ("Sushline", run_sushline_harness_with_inputrc(keys, inputrc)),
    ] {
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("abcX"),
            "{name} keys={keys:?}: {output:?}"
        );
        assert_eq!(bell_count(&output), 0, "{name} keys={keys:?}: {output:?}");
        assert!(
            !output.contains("Display all"),
            "{name} asked to confirm the candidate list: {output:?}"
        );
        for candidate in ["beginning-of-history", "beginning-of-line"] {
            assert!(
                output.contains(candidate),
                "{name} did not list {candidate}: {output:?}"
            );
        }
    }
}

/// GNU's completion pager treats `n` and `q` alike: both stop listing and
/// leave the caller ready for the next input. Sushline used to advance pages
/// on `n`, which kept the pager alive until the PTY deadline.
#[test]
fn bash_readline_and_sushline_stop_completion_pager_on_n_or_q() {
    if !host_carries_measured_release() {
        eprintln!("Completion pager quit keys were measured on Readline 8.3");
        return;
    }
    for answer in [b"n".as_slice(), b"q".as_slice()] {
        let mut after_answer = answer.to_vec();
        after_answer.extend_from_slice(b"\rX\r");
        let steps = [
            ("SUSHLINE_READY>", b"\x1bx\t".as_slice()),
            ("--More--", after_answer.as_slice()),
        ];
        for (name, output) in [
            ("Bash", run_bash_readline_steps_with_inputrc(&steps, "")),
            (
                "Sushline",
                run_sushline_harness_steps_with_inputrc(&steps, ""),
            ),
        ] {
            let context = format!("{name} answer={answer:?}: {output:?}");
            assert_eq!(accepted_line(&output).as_deref(), Some("X"), "{context}");
            assert_eq!(output.matches("--More--").count(), 1, "{context}");
            assert_eq!(bell_count(&output), 0, "{context}");
        }
    }
}

/// A command-name completion with no match rings once per TAB/SPACE on both
/// editors; the later unknown-name submission rings once more. Accepted lines
/// are identical; only the paired bell counts distinguish the behavior.
#[test]
fn bash_readline_and_sushline_ring_on_failed_named_command_completion() {
    if !host_carries_measured_release() {
        eprintln!("Failed named-completion bells were measured on Readline 8.3");
        return;
    }
    for (keys, bells) in [
        (b"abc\x1bxzzz\t\rX\r".as_slice(), 2),
        (b"abc\x1bxzzz\t\t\t\rX\r".as_slice(), 4),
        (b"abc\x1bxzzz \t\rX\r".as_slice(), 3),
    ] {
        for (name, output) in [
            ("Bash", run_bash_readline_with_inputrc_file(keys, "")),
            ("Sushline", run_sushline_harness_with_inputrc(keys, "")),
        ] {
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some("abcX"),
                "{name} keys={keys:?}: {output:?}"
            );
            assert_eq!(
                bell_count(&output),
                bells,
                "{name} keys={keys:?}: {output:?}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_redispatch_named_digit_argument_key() {
    assert_named_invoking_key_cases(
        "",
        &[
            (b"\x1bxdigit-argument\rX\r", "xX"),
            (b"\x1bxdigit-argument\r\r", "x"),
            (b"\x1bxdigit-argument\r3X\r", "x3X"),
            (b"\x1bxdigit-argument\r-X\r", "x-X"),
            (b"\x1b0\x1bxdigit-argument\rX\r", "X"),
            (b"\x1b2\x1bxdigit-argument\rX\r", "xxX"),
            (b"\x1b3\x1bxdigit-argument\rY\r", "xxxY"),
        ],
    );
}

#[test]
fn bash_readline_and_sushline_use_named_digit_argument_invoking_digit_as_count() {
    let inputrc = r#""5": execute-named-command
"0": execute-named-command"#;
    assert_named_invoking_key_cases(
        inputrc,
        &[
            (b"5digit-argument\rX\r", "XXXXX"),
            (b"0digit-argument\rX\r", ""),
            (b"5digit-argument\r3X\r", &"X".repeat(53)),
        ],
    );
}

#[test]
fn bash_readline_and_sushline_redispatch_named_self_insert_key() {
    assert_named_invoking_key_cases(
        "",
        &[
            (b"\x1bxself-insert\rY\r", "xY"),
            (b"\x1bxself-insert\r\r", "x"),
            (b"\x1bxself-insert\r3Y\r", "x3Y"),
            (b"\x1bxself-insert\r-Y\r", "x-Y"),
            (b"\x1b2\x1bxself-insert\rY\r", "xxY"),
            (b"\x1b-\x1bxself-insert\rY\r", "Y"),
            (b"\x1b0\x1bxself-insert\rY\r", "Y"),
            (b"\x1b3\x1bxself-insert\rYZ\r", "xxxYZ"),
        ],
    );
}

/// An empty execute-named-command name is a silent no-op on Readline 8.3:
/// the original line is accepted with no bell, while an unknown non-empty name
/// rings exactly once. Both editors are compared on accepted line and bells.
#[test]
fn bash_readline_and_sushline_empty_named_command_name_is_silent() {
    if !host_carries_measured_release() {
        eprintln!(
            "Named command empty name was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for &(keys, expected, bells) in &[
        (b"abc\x1bx\r\r".as_slice(), "abc", 0),
        (b"abc\x1bxno-such-cmd-xyz\r\r".as_slice(), "abc", 1),
        (b"\x1bx\rX\r".as_slice(), "X", 0),
    ] {
        for (name, output) in [
            ("Bash", run_bash_readline_with_inputrc_file(keys, "")),
            ("Sushline", run_sushline_harness_with_inputrc(keys, "")),
        ] {
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(expected),
                "{name} keys={keys:?}: {output:?}"
            );
            assert_eq!(
                bell_count(&output),
                bells,
                "{name} keys={keys:?}: {output:?}"
            );
        }
    }
}

/// A named self-insert inserts the byte that opened the prompt literally,
/// even when that byte is bound to another command. Pinned on Readline 8.3
/// with Ctrl-O bound to execute-named-command: 0x0f is the invoking byte.
#[test]
fn bash_readline_and_sushline_insert_bound_named_self_insert_key_literally() {
    if !host_carries_measured_release() {
        eprintln!(
            "Bound named self-insert was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    let inputrc = r#""\C-o": execute-named-command"#;
    for (keys, expected) in [
        (b"\x0fself-insert\rY\r".as_slice(), "\u{f}Y"),
        (b"\x0fself-insert\r\r".as_slice(), "\u{f}"),
    ] {
        for (name, output) in [
            ("Bash", run_bash_readline_with_bindings(keys, inputrc)),
            ("Sushline", run_sushline_harness_with_inputrc(keys, inputrc)),
        ] {
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(expected),
                "{name} keys={keys:?}: {output:?}"
            );
            assert_eq!(bell_count(&output), 0, "{name} keys={keys:?}: {output:?}");
        }
    }
}

/// Dismissing the execute-named-command prompt without submitting a command
/// drops a pending count. Pinned on Readline 8.3 with Ctrl-G as the cancel key:
/// Escape is not used here because GNU reads it as a meta prefix inside the
/// prompt, a separate gap that predates count preservation. Only the accepted
/// line is compared: GNU rings on Ctrl-G abort while Sushline stays silent.
#[test]
fn bash_readline_and_sushline_drop_count_when_named_command_is_cancelled() {
    if !host_carries_measured_release() {
        eprintln!(
            "Named command cancel was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for &(keys, expected) in &[
        (b"\x1b3\x1bx\x07X\r".as_slice(), "X"),
        (b"\x1b2\x1bx\x07Y\r".as_slice(), "Y"),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, "");
        let sushline = run_sushline_harness_with_inputrc(keys, "");
        for (name, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{name} keys={keys:?}: {output:?}"
            );
        }
    }
}

fn assert_named_invoking_key_cases(inputrc: &str, cases: &[(&[u8], &str)]) {
    if !host_carries_measured_release() {
        eprintln!(
            "Named invoking-key forwarding was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for &(keys, expected) in cases {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        for (name, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{name} keys={keys:?}: {output:?}"
            );
            assert_eq!(bell_count(output), 0, "{name} keys={keys:?}: {output:?}");
        }
    }
}

#[test]
fn sushline_prefix_meta_inputrc_extension_preserves_literal_fallback() {
    require_oracle_host();
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
fn bash_readline_and_sushline_match_bell_preference_directives() {
    for (case, directives, expected_visible, expected_bells) in [
        ("defaults", "", false, 1),
        ("style_none", "set bell-style none\n", false, 0),
        ("style_audible", "set bell-style audible\n", false, 1),
        ("style_visible", "set bell-style visible\n", true, 0),
        ("prefer_on", "set prefer-visible-bell on\n", true, 0),
        ("prefer_off", "set prefer-visible-bell off\n", false, 1),
        (
            "none_then_on",
            "set bell-style none\nset prefer-visible-bell on\n",
            true,
            0,
        ),
        (
            "on_then_none",
            "set prefer-visible-bell on\nset bell-style none\n",
            false,
            0,
        ),
        (
            "none_then_off",
            "set bell-style none\nset prefer-visible-bell off\n",
            false,
            1,
        ),
        (
            "off_then_none",
            "set prefer-visible-bell off\nset bell-style none\n",
            false,
            0,
        ),
        (
            "visible_then_off",
            "set bell-style visible\nset prefer-visible-bell off\n",
            false,
            1,
        ),
        (
            "off_then_visible",
            "set prefer-visible-bell off\nset bell-style visible\n",
            true,
            0,
        ),
    ] {
        let inputrc = [directives, "\"1\": do-lowercase-version\n"].concat();
        let keys = b"1\r";
        let bash = run_bash_readline_with_inputrc_file(keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, &inputrc);

        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some(String::new()),
                "{editor} {case}: {output}"
            );
            assert_eq!(
                output.contains("\x1b[?5h"),
                expected_visible,
                "{editor} {case}: {output}"
            );
            assert_eq!(
                bell_count(output),
                expected_bells,
                "{editor} {case}: {output}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_match_lowercase_version_cases() {
    for (case, inputrc, keys, expected, expected_bells) in [
        (
            "case_variant",
            r#""A": do-lowercase-version"#,
            b"A\r".as_slice(),
            "a",
            0,
        ),
        (
            "no_case_variant",
            r#""1": do-lowercase-version"#,
            b"1\r".as_slice(),
            "",
            1,
        ),
    ] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {case}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {case}: {sushline}"
        );
        assert_eq!(bell_count(&bash), expected_bells, "Bash {case}: {bash}");
        assert_eq!(
            bell_count(&sushline),
            expected_bells,
            "Sushline {case}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_match_lowercase_version_meta_dispatch() {
    let inputrc = "set bell-style audible\nset prefer-visible-bell off\n";
    for (case, keys, expected, expected_bells) in [
        (
            "meta_capitalize",
            b"hello\x01\x1bC\r".as_slice(),
            "Hello",
            0,
        ),
        ("meta_unbound", b"\x1bA\r".as_slice(), "", 1),
        ("ctrl_x_unbound", b"abc\x18UX\r".as_slice(), "abcX", 1),
        (
            "ctrl_x_direct_unbound",
            b"abc\x18qX\r".as_slice(),
            "abcX",
            1,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{editor} {case}: {output}"
            );
            assert_eq!(
                bell_count(output),
                expected_bells,
                "{editor} {case}: {output}"
            );
        }
    }
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
        accepted_line(&bash),
        Some("Xabc".to_string()),
        "Bash vi switch: {bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "Sushline vi switch: {sushline}"
    );

    let emacs_inputrc = r#"set editing-mode vi
"\C-o": emacs-editing-mode"#;
    let emacs_keys = b"abc\x0f\x01X\r";
    let bash = run_bash_readline_with_inputrc_file(emacs_keys, emacs_inputrc);
    let sushline = run_sushline_harness_with_inputrc(emacs_keys, emacs_inputrc);
    assert_eq!(
        accepted_line(&bash),
        Some("Xabc".to_string()),
        "Bash emacs switch: {bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "Sushline emacs switch: {sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_csi_skip_commands_consume_the_sequence() {
    // GNU binds `\e[` to skip-csi-sequence and `\eO` to arrow-key-prefix by
    // default. Feed each editor a full escape sequence under those defaults:
    // both must discard it and accept the following text.
    for (name, keys) in [
        ("skip-csi-sequence", &b"\x1b[1;5CX\r"[..]),
        ("arrow-key-prefix", &b"\x1bOAX\r"[..]),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);

        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some("X"),
            "Bash {name}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("X"),
            "Sushline {name}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_unbound_csi_sequences_consume_the_keymap_miss() {
    // A CSI prefix that is live in the keymap but has no matching leaf is a
    // keymap miss: GNU rings once, discards the live prefix and the first
    // byte that failed lookup, then processes the remaining input normally.
    for (name, seq, expected) in [
        ("two_digits", &b"\x1b[99~"[..], "abc9~X"),
        ("double_z", &b"\x1b[ZZ"[..], "abcZX"),
        ("one_one_tilde", &b"\x1b[11~"[..], "abc~X"),
        ("semicolon", &b"\x1b[;"[..], "abcX"),
    ] {
        let mut keys = b"abc".to_vec();
        keys.extend_from_slice(seq);
        keys.extend_from_slice(b"X\r");
        let bash = run_bash_readline(&keys);
        let sushline = run_sushline_harness(&keys);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 1, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 1, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_forward_backward_delete_char_numeric_arguments_match_bash_5_3() {
    // Bash 5.3 PTY accepted-line observations at an interior point and EOL.
    let inputrc = r#""\C-o": forward-backward-delete-char"#;
    for (name, keys, expected) in [
        (
            "interior_positive",
            b"abcdef\x01\x06\x06\x1b3\x0f\r".as_slice(),
            "abf",
        ),
        (
            "interior_zero",
            b"abcdef\x01\x06\x06\x1b0\x0f\r".as_slice(),
            "abcdef",
        ),
        (
            "interior_negative",
            b"abcdef\x01\x06\x06\x1b-1\x0f\r".as_slice(),
            "acdef",
        ),
        (
            "interior_overrun",
            b"abcdef\x01\x06\x06\x1b5\x0f\r".as_slice(),
            "ab",
        ),
        (
            "interior_uncounted",
            b"abcdef\x01\x06\x06\x0f\r".as_slice(),
            "abdef",
        ),
        ("eol_positive", b"abcdef\x1b3\x0f\r".as_slice(), "abc"),
        ("eol_zero", b"abcdef\x1b0\x0f\r".as_slice(), "abcdef"),
        ("eol_negative", b"abcdef\x1b-1\x0f\r".as_slice(), "abcdef"),
        ("eol_uncounted", b"abcdef\x0f\r".as_slice(), "abcde"),
    ] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_forward_backward_delete_char_counted_kills_for_yank() {
    // Bash 5.3 PTY observations measure counted kill-ring restoration separately
    // from accepted-line deletion, with uncounted interior and EOL controls.
    let inputrc = r#""\C-o": forward-backward-delete-char"#;
    for (name, keys, expected) in [
        (
            "interior_positive",
            b"abcdef\x01\x06\x06\x1b3\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "interior_negative",
            b"abcdef\x01\x06\x06\x1b-1\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "eol_positive",
            b"abcdef\x1b3\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "eol_negative",
            b"abcdef\x1b-1\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "zero",
            b"abcdef\x01\x06\x06\x1b0\x0f\x19\r".as_slice(),
            "abcdef",
        ),
        (
            "interior_uncounted",
            b"abcdef\x01\x06\x06\x0f\x19\r".as_slice(),
            "abdef",
        ),
        ("eol_uncounted", b"abcdef\x0f\x19\r".as_slice(), "abcde"),
    ] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_forward_backward_delete_char_deletes_backward_in_overwrite_mode() {
    let inputrc = r#""\C-o": forward-backward-delete-char
"\C-p": overwrite-mode"#;
    for (name, keys, expected) in [
        (
            "overwrite_mode_sanity",
            b"abcd\x01\x10X\r".as_slice(),
            "Xbcd",
        ),
        ("uncounted_eol", b"abcd\x10\x0f\r".as_slice(), "abc"),
        ("counted_eol", b"abcd\x10\x1b2\x0f\r".as_slice(), "ab"),
        (
            "negative_interior",
            b"abcd\x01\x06\x06\x10\x1b-1\x0f\r".as_slice(),
            "acd",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_tab_insert_numeric_arguments_match_bash_5_3() {
    // Bash 5.3 PTY comparison checks accepted bytes, not TAB display expansion.
    let inputrc = r#""\C-o": tab-insert"#;
    for (name, keys, expected) in [
        ("positive", b"a\x1b3\x0fb\r".as_slice(), "a\t\t\tb"),
        ("zero", b"a\x1b0\x0fb\r".as_slice(), "ab"),
        ("negative", b"a\x1b-\x0fb\r".as_slice(), "ab"),
        ("uncounted", b"a\x0fb\r".as_slice(), "a\tb"),
    ] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline}"
        );
    }
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

/// The row the terminal shows with the pending numeric argument drawn.
///
/// The host draws the indicator over the prompt and redraws the prompt once
/// the argument is consumed, so the screen is looked at while the indicator
/// is there. The indicator takes the place of the prompt, so it starts the
/// row, and the row that starts with it is the one a case compares, because
/// a program may print its own output after the editor finished.
fn pending_argument_row(editor: &str, output: &str, indicator: &str) -> String {
    let screen = Screen::new(24, 80);
    let rows = screen
        .last_rows_starting_with(output.as_bytes(), indicator)
        .unwrap_or_else(|| {
            panic!("{editor} must draw {indicator} in place of the prompt: {output:?}")
        });
    rows.iter()
        .find(|row| row.contains(indicator))
        .cloned()
        .unwrap_or_else(|| {
            panic!("{editor} must draw {indicator} in place of the prompt: {rows:?}")
        })
}

/// Whether the host carries the measured Readline release.
///
/// GNU Readline 8.3 draws `(arg: N) ` where the prompt would be while a
/// numeric argument is pending, and the prompt is not on the screen while
/// the indicator is (Bash 5.3 PTY observation on the host's Readline 8.3).
/// The 8.2 release leaves the prompt on the screen, so the comparison holds
/// on the release the evidence was measured on and reports nothing on a host
/// that carries another one.
fn host_carries_measured_release() -> bool {
    super::host_readline::host_carries(super::host_readline::MEASURED_RELEASE)
}
#[test]
fn bash_and_sushline_draw_the_pending_numeric_argument_in_place_of_the_prompt() {
    if !host_carries_measured_release() {
        eprintln!(
            "the pending argument rendering was measured on Readline 8.3, and the host carries another release, so this case reports nothing"
        );
        return;
    }
    let keys = b"\x1b3\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    let bash_row = pending_argument_row("Bash", &bash, "(arg: 3)");
    let sushline_row = pending_argument_row("Sushline", &sushline, "(arg: 3)");
    assert_eq!(
        bash_row, sushline_row,
        "the drawn indicator must match the host"
    );
    assert_eq!(
        bash_row, "(arg: 3)",
        "the indicator must take the place of the prompt"
    );
}

/// A leading zero is an argument of its own.
///
/// The digit zero starts an argument of zero, and the indicator carries it
/// like any other value.
#[test]
fn bash_and_sushline_draw_the_zero_pending_numeric_argument() {
    if !host_carries_measured_release() {
        eprintln!(
            "the pending argument rendering was measured on Readline 8.3, and the host carries another release, so this case reports nothing"
        );
        return;
    }
    let keys = b"\x1b0\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    let bash_row = pending_argument_row("Bash", &bash, "(arg: 0)");
    let sushline_row = pending_argument_row("Sushline", &sushline, "(arg: 0)");
    assert_eq!(
        bash_row, sushline_row,
        "the drawn indicator must match the host"
    );
    assert_eq!(
        bash_row, "(arg: 0)",
        "the indicator must carry the pending argument"
    );
}

/// The indicator grows with the pending argument.
#[test]
fn bash_and_sushline_draw_the_growing_pending_numeric_argument() {
    if !host_carries_measured_release() {
        eprintln!(
            "the pending argument rendering was measured on Readline 8.3, and the host carries another release, so this case reports nothing"
        );
        return;
    }
    let keys = b"\x1b3\x1b4\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    let bash_row = pending_argument_row("Bash", &bash, "(arg: 34)");
    let sushline_row = pending_argument_row("Sushline", &sushline, "(arg: 34)");
    assert_eq!(
        bash_row, sushline_row,
        "the drawn indicator must match the host"
    );
    assert_eq!(
        bash_row, "(arg: 34)",
        "the indicator must carry the pending argument"
    );
}

/// The universal argument is drawn in place of the prompt.
#[test]
fn bash_and_sushline_draw_the_pending_universal_argument_in_place_of_the_prompt() {
    if !host_carries_measured_release() {
        eprintln!(
            "the pending argument rendering was measured on Readline 8.3, and the host carries another release, so this case reports nothing"
        );
        return;
    }
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let keys = b"\x0f\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    let bash_row = pending_argument_row("Bash", &bash, "(arg: 4)");
    let sushline_row = pending_argument_row("Sushline", &sushline, "(arg: 4)");
    assert_eq!(
        bash_row, sushline_row,
        "the drawn indicator must match the host"
    );
    assert_eq!(
        bash_row, "(arg: 4)",
        "the indicator must carry the universal argument"
    );
}

/// The mode indicator is replaced with the prompt.
///
/// The mode indicator belongs to the prompt, so a pending argument replaces
/// it as well (Bash 5.3 PTY observation).
#[test]
fn bash_and_sushline_replace_the_mode_indicator_with_the_pending_argument() {
    if !host_carries_measured_release() {
        eprintln!(
            "the pending argument rendering was measured on Readline 8.3, and the host carries another release, so this case reports nothing"
        );
        return;
    }
    let inputrc =
        "set editing-mode vi\nset show-mode-in-prompt on\n\"\\C-o\": universal-argument\n";
    let keys = b"\x0f";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    let bash_row = pending_argument_row("Bash", &bash, "(arg: 4)");
    let sushline_row = pending_argument_row("Sushline", &sushline, "(arg: 4)");
    assert_eq!(
        bash_row, sushline_row,
        "the drawn indicator must match the host"
    );
    assert_eq!(
        bash_row, "(arg: 4)",
        "the indicator must take the place of the prompt"
    );
}

/// The line's text follows the indicator.
#[test]
fn bash_and_sushline_draw_the_pending_numeric_argument_before_the_line_text() {
    if !host_carries_measured_release() {
        eprintln!(
            "the pending argument rendering was measured on Readline 8.3, and the host carries another release, so this case reports nothing"
        );
        return;
    }
    let keys = b"abc\x1b3\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    let bash_row = pending_argument_row("Bash", &bash, "(arg: 3)");
    let sushline_row = pending_argument_row("Sushline", &sushline, "(arg: 3)");
    assert_eq!(
        bash_row, sushline_row,
        "the drawn indicator must match the host"
    );
    assert_eq!(
        bash_row, "(arg: 3) abc",
        "the line's text must follow the indicator"
    );
}

/// The sign-only prefix draws the sign of the pending argument.
#[test]
fn bash_and_sushline_draw_the_negative_pending_numeric_argument() {
    if !host_carries_measured_release() {
        eprintln!(
            "the pending argument rendering was measured on Readline 8.3, and the host carries another release, so this case reports nothing"
        );
        return;
    }
    let keys = b"\x1b-\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    let bash_row = pending_argument_row("Bash", &bash, "(arg: -1)");
    let sushline_row = pending_argument_row("Sushline", &sushline, "(arg: -1)");
    assert_eq!(
        bash_row, sushline_row,
        "the drawn indicator must match the host"
    );
    assert_eq!(
        bash_row, "(arg: -1)",
        "the indicator must carry the sign of the pending argument"
    );
}

#[test]
fn bash_readline_and_sushline_reset_a_numeric_argument_above_the_limit() {
    // A numeric argument may not exceed 1000000: the argument fills with six
    // nines, the seventh exceeds the limit and rings the bell, and the
    // argument is reset, so the six digits that follow the reset reach the
    // shell as ordinary text. Runtime observation against the tested Bash
    // 5.3 host, not GNU source proof.
    let keys = b"\x1b9999999999999\r";
    let bash = run_bash_readline(keys);
    let sushline = run_sushline_harness(keys);

    assert_eq!(accepted_line(&bash), Some("999999".to_string()), "{bash}");
    assert_eq!(bell_count(&bash), 1, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&sushline), bell_count(&bash), "{sushline}");
}

#[test]
fn bash_and_sushline_reset_a_universal_argument_above_the_limit() {
    // The universal argument obeys the same limit as the digit argument:
    // 4^10 exceeds 1000000, so the tenth universal argument rings the bell
    // and resets the argument, and the `a` that follows is inserted once.
    // Runtime observation against the tested Bash 5.3 host, not GNU source
    // proof.
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let mut keys = vec![0x0f; 10];
    keys.extend_from_slice(b"a\r");
    let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("a".to_string()), "{bash}");
    assert_eq!(bell_count(&bash), 1, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&sushline), bell_count(&bash), "{sushline}");
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
fn bash_and_sushline_keep_explicit_meta_zero_count_through_universal_argument() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    // Point starts at byte 60; the count selects where C-f leaves the sentinel.
    for (desc, prefix, position) in [
        ("explicit meta zero", b"\x1b-\x1b0".as_slice(), 50),
        ("sign only", b"\x1b-".as_slice(), 56),
        ("explicit meta digit", b"\x1b-\x1b1".as_slice(), 49),
        ("bare leading zero", b"\x1b-0".as_slice(), 60),
    ] {
        let mut keys = vec![b'a'; 60];
        keys.extend_from_slice(prefix);
        keys.extend_from_slice(b"\x0f\x06X\r");
        let expected = format!("{}X{}", "a".repeat(position), "a".repeat(60 - position));
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);

        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), 0, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_reset_universal_count_on_following_minus() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    // Point starts at byte 60; C-f applies the resulting numeric argument.
    for (desc, suffix, position, bells) in [
        ("minus only", b"\x1b-".as_slice(), 59, 0),
        ("minus then meta zero", b"\x1b-\x1b0".as_slice(), 50, 0),
        (
            "minus meta zero then digit",
            b"\x1b-\x1b03".as_slice(),
            0,
            1,
        ),
        ("minus then bare digit", b"\x1b-3".as_slice(), 57, 0),
    ] {
        let mut keys = vec![b'a'; 60];
        keys.push(b'\x0f');
        keys.extend_from_slice(suffix);
        keys.extend_from_slice(b"\x06X\r");
        let expected = format!("{}X{}", "a".repeat(position), "a".repeat(60 - position));
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);

        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), bells, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_swallow_argument_keys_inside_the_literal_tail() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let a = "a".repeat(60);
    let expected = format!("{a}555X");
    // After a no-op universal tail, GNU consumes a Meta-prefixed argument
    // key or a universal-argument key and every later argument key, without
    // inserting them.
    for (desc, keys) in [
        (
            "meta six then bare seven",
            b"\x1b3\x0f5\x1b67\x06X\r".as_slice(),
        ),
        (
            "meta zero then bare six",
            b"\x1b3\x0f5\x1b06\x06X\r".as_slice(),
        ),
        ("one inner universal", b"\x1b3\x0f5\x0f6\x06X\r".as_slice()),
        (
            "two inner universals",
            b"\x1b3\x0f5\x0f\x0f6\x06X\r".as_slice(),
        ),
        ("plain tail control", b"\x1b3\x0f5\x06X\r".as_slice()),
        (
            "meta six and two bare digits",
            b"\x1b3\x0f5\x1b678\x06X\r".as_slice(),
        ),
        (
            "inner universal and two bare digits",
            b"\x1b3\x0f5\x0f67\x06X\r".as_slice(),
        ),
        (
            "two meta prefixes and a bare digit",
            b"\x1b3\x0f5\x1b6\x1b89\x06X\r".as_slice(),
        ),
        (
            "inner universal then meta and a bare digit",
            b"\x1b3\x0f5\x0f6\x1b89\x06X\r".as_slice(),
        ),
    ] {
        let mut input = a.as_bytes().to_vec();
        input.extend_from_slice(keys);
        let bash = run_bash_readline_with_inputrc_file(&input, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&input, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), 1, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_insert_the_noop_universal_literal_tail() {
    // The first digit or minus after a no-op universal is literal text; GNU
    // keeps following argument keys literal too, with no extra bell until a
    // blocked movement consumes the argument.
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let a = "a".repeat(60);
    for (desc, keys, text, bells) in [
        (
            "two bare digits",
            b"\x1b3\x0f56\x06X\r".as_slice(),
            "5556X",
            1,
        ),
        (
            "bare then meta",
            b"\x1b3\x0f5\x1b6\x06X\r".as_slice(),
            "555X",
            1,
        ),
        (
            "bare then minus",
            b"\x1b3\x0f5-\x06X\r".as_slice(),
            "555-X",
            1,
        ),
        (
            "zero then digit",
            b"\x1b3\x0f05\x06X\r".as_slice(),
            "0005X",
            1,
        ),
        (
            "negative then two",
            b"\x1b-\x1b1\x0f56\x06X\r".as_slice(),
            "6X",
            1,
        ),
        (
            "meta then two",
            b"\x1b3\x0f\x1b056\x06X\r".as_slice(),
            "056X",
            2,
        ),
        (
            "two minuses",
            b"\x1b3\x0f5--\x06X\r".as_slice(),
            "555--X",
            1,
        ),
        ("direct minus", b"\x1b3\x0f-\x06X\r".as_slice(), "---X", 1),
        (
            "direct meta minus",
            b"\x1b3\x0f\x1b-\x06X\r".as_slice(),
            "-X",
            2,
        ),
        (
            "minus then digit",
            b"\x1b3\x0f-5\x06X\r".as_slice(),
            "---5X",
            1,
        ),
        (
            "self insert then tail",
            b"\x1b3\x0f5a6\x06X\r".as_slice(),
            "555a6X",
            1,
        ),
        (
            "self insert only",
            b"\x1b3\x0f5a\x06X\r".as_slice(),
            "555aX",
            1,
        ),
        (
            "no blocked movement",
            b"\x1b3\x0f5a6X\r".as_slice(),
            "555a6X",
            0,
        ),
    ] {
        let mut input = vec![b'a'; 60];
        input.extend_from_slice(keys);
        let expected = format!("{a}{text}");
        let bash = run_bash_readline_with_inputrc_file(&input, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&input, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), bells, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_compose_counts_after_repeated_universal_noop() {
    // `M-- M-0` leaves a negative Meta-digit count. A second universal after
    // its no-op multiplies that count while retaining composition flags, so
    // a following digit appends and a following minus toggles the sign.
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let a = "a".repeat(60);
    for (desc, suffix, expected) in [
        (
            "bare digit",
            b"\x1b-\x1b0\x0f\x0f3\x06X\r".as_slice(),
            format!("X{a}"),
        ),
        (
            "meta digit",
            b"\x1b-\x1b0\x0f\x0f\x1b3\x06X\r".as_slice(),
            format!("X{a}"),
        ),
        (
            "minus",
            b"\x1b-\x1b0\x0f\x0f-\x06X\r".as_slice(),
            format!("{a}X"),
        ),
        (
            "meta minus",
            b"\x1b-\x1b0\x0f\x0f\x1b-\x06X\r".as_slice(),
            format!("{a}X"),
        ),
    ] {
        let mut keys = vec![b'a'; 60];
        keys.extend_from_slice(suffix);
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), 1, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_match_digits_after_noop_universal_argument() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    // Point starts at byte 60; C-f applies the resulting numeric argument.
    for (desc, suffix, position, inserted, bells) in [
        ("meta zero", b"\x1b-\x1b0\x0f\x1b0".as_slice(), 61, "0", 2),
        ("meta one", b"\x1b-\x1b0\x0f\x1b1".as_slice(), 61, "1", 2),
        (
            "second universal",
            b"\x1b-\x1b0\x0f\x0f".as_slice(),
            20,
            "",
            0,
        ),
        ("bare zero", b"\x1b-\x1b0\x0f0".as_slice(), 60, "", 1),
        ("bare three", b"\x1b-\x1b0\x0f3".as_slice(), 60, "", 1),
        ("no-op control", b"\x1b-\x1b0\x0f".as_slice(), 50, "", 0),
    ] {
        let mut keys = vec![b'a'; 60];
        keys.extend_from_slice(suffix);
        keys.extend_from_slice(b"\x06X\r");
        let expected = format!(
            "{}{}X{}",
            "a".repeat(position - inserted.len()),
            inserted,
            "a".repeat(60 + inserted.len() - position)
        );
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);

        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), bells, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_ring_for_blocked_character_motion_by_direction() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let a = "a".repeat(60);
    for (desc, keys, expected, bells) in [
        (
            "forward overshoot from start",
            b"\x01\x0f\x0f\x0f\x06X\r".as_slice(),
            format!("{a}X"),
            0,
        ),
        (
            "forward overshoot partway",
            b"\x02\x02\x0f\x06X\r".as_slice(),
            format!("{a}X"),
            0,
        ),
        (
            "forward fully blocked",
            b"\x0f\x06X\r".as_slice(),
            format!("{a}X"),
            1,
        ),
        (
            "backward overshoot",
            b"\x1b-\x0f\x0f\x0f\x06X\r".as_slice(),
            format!("X{a}"),
            1,
        ),
    ] {
        let mut input = a.as_bytes().to_vec();
        input.extend_from_slice(keys);
        let bash = run_bash_readline_with_inputrc_file(&input, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&input, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), bells, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_start_fresh_counts_after_a_chain_minus_tail() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let base = format!("{}{}", "a".repeat(60), "-".repeat(48));
    // Bare argument keys remain literal in this tail, but a Meta-prefixed
    // argument key starts a new signed count and ends the tail.
    for (desc, suffix, expected) in [
        ("bare digit", b"5X\r".as_slice(), format!("{base}5X")),
        (
            "meta digit",
            b"\x1b5X\r".as_slice(),
            format!("{base}{}", "X".repeat(5)),
        ),
        ("second minus", b"-X\r".as_slice(), format!("{base}-X")),
        ("meta minus", b"\x1b-X\r".as_slice(), base.clone()),
        ("letter", b"YX\r".as_slice(), format!("{base}YX")),
        (
            "meta five then bare six",
            b"\x1b56X\r".as_slice(),
            format!("{base}{}", "X".repeat(56)),
        ),
        (
            "bare five then meta six",
            b"5\x1b6X\r".as_slice(),
            format!("{base}5{}", "X".repeat(6)),
        ),
        (
            "meta five then meta six",
            b"\x1b5\x1b6X\r".as_slice(),
            format!("{base}{}", "X".repeat(56)),
        ),
        (
            "meta minus then digit",
            b"\x1b-5X\r".as_slice(),
            base.clone(),
        ),
    ] {
        let mut input = vec![b'a'; 60];
        input.extend_from_slice(b"\x1b3\x0f\x0f\x0f-");
        input.extend_from_slice(suffix);
        let bash = run_bash_readline_with_inputrc_file(&input, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&input, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), 0, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_insert_minus_after_a_multiplied_universal_chain() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let a = "a".repeat(60);
    // The minus is literal text repeated by the multiplied positive count;
    // C-f then moves one byte because the argument was consumed.
    for (desc, prefix, hyphens) in [
        (
            "M-2 with three universals",
            b"\x1b2\x0f\x0f\x0f-".as_slice(),
            32,
        ),
        (
            "M-3 with three universals",
            b"\x1b3\x0f\x0f\x0f-".as_slice(),
            48,
        ),
        (
            "M-4 with three universals",
            b"\x1b4\x0f\x0f\x0f-".as_slice(),
            64,
        ),
        (
            "M-3 with Meta-minus",
            b"\x1b3\x0f\x0f\x0f\x1b-".as_slice(),
            48,
        ),
        ("M-3 with two universals", b"\x1b3\x0f\x0f-".as_slice(), 12),
    ] {
        let expected = format!("{}{}X{}", "-".repeat(hyphens), &a[..1], &a[..59]);
        let mut input = a.as_bytes().to_vec();
        input.push(b'\x01');
        input.extend_from_slice(prefix);
        input.extend_from_slice(b"\x06X\r");
        let bash = run_bash_readline_with_inputrc_file(&input, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&input, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), 0, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_multiply_repeated_universal_chains() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let a = "a".repeat(60);
    // Third and later universal arguments must keep multiplying; a separate
    // state flag preserves digit/minus composition after the no-op chain.
    for (desc, keys, expected, bells) in [
        (
            "three universals",
            b"\x1b3\x0f\x0f\x0f\x06X\r".as_slice(),
            format!("{}X{}", &a[..48], &a[..12]),
            Some(0),
        ),
        (
            "four universals",
            b"\x1b3\x0f\x0f\x0f\x0f\x06X\r".as_slice(),
            format!("{a}X"),
            Some(0),
        ),
        (
            "negative three universals",
            b"\x1b-\x1b0\x0f\x0f\x0f\x06X\r".as_slice(),
            format!("X{a}"),
            Some(1),
        ),
        (
            "two universals control",
            b"\x1b3\x0f\x0f\x06X\r".as_slice(),
            format!("{}X{}", &a[..12], &a[..48]),
            Some(0),
        ),
    ] {
        let mut input = a.as_bytes().to_vec();
        if desc != "negative three universals" {
            input.push(b'\x01');
        }
        input.extend_from_slice(keys);
        let bash = run_bash_readline_with_inputrc_file(&input, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&input, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            if let Some(bells) = bells {
                assert_eq!(bell_count(output), bells, "{editor} {desc}: {output}");
            }
        }
    }
}

#[test]
fn bash_and_sushline_repeat_noop_universal_digits_by_explicit_count() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    // Point starts at byte 60; every accepted line ends at the line end.
    for (desc, prefix, inserted, bells) in [
        ("count three", b"\x1b3\x0f5".as_slice(), "555", 1),
        ("count two", b"\x1b2\x0f5".as_slice(), "55", 1),
        ("zero digit", b"\x1b3\x0f0".as_slice(), "000", 1),
        ("meta digit", b"\x1b3\x0f\x1b0".as_slice(), "0", 2),
        ("negative count", b"\x1b-\x1b1\x0f5".as_slice(), "", 1),
        (
            "universal then explicit",
            b"\x0f3\x0f4".as_slice(),
            "444",
            1,
        ),
    ] {
        let mut keys = vec![b'a'; 60];
        keys.extend_from_slice(prefix);
        keys.extend_from_slice(b"\x06X\r");
        let expected = format!("{}{}X", "a".repeat(60), inserted);
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);

        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), bells, "{editor} {desc}: {output}");
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
fn bash_readline_and_sushline_yank_ignores_numeric_argument() {
    // Consecutive C-u kills coalesce into one kill-ring entry here; this
    // fixture pins that coalesced-text yank, not multi-entry ring selection.
    for (desc, prefix) in [
        ("uncounted", b"".as_slice()),
        ("zero", b"\x1b0".as_slice()),
        ("one", b"\x1b1".as_slice()),
        ("two", b"\x1b2".as_slice()),
        ("negative_one", b"\x1b-\x1b1".as_slice()),
    ] {
        let mut keys = b"one\x15two\x15".to_vec();
        keys.extend_from_slice(prefix);
        keys.push(0x19);
        keys.push(b'\r');
        let bash = run_bash_readline(&keys);
        let sushline = run_sushline_harness(&keys);

        assert_eq!(
            accepted_line(&bash),
            Some("twoone".to_string()),
            "{desc}: {bash}"
        );
        println!(
            "{desc}: bash bells={} sush bells={}",
            bell_count(&bash),
            bell_count(&sushline)
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_yank_ignores_numeric_argument_with_distinct_kill_entries() {
    // C-f at end of `two` separates the two C-u kills, making their ring
    // entries distinct; Bash rings once for that failed forward motion.
    // Every tested C-y prefix yanks only the newest entry, `two`.
    for (desc, prefix) in [
        ("uncounted", b"".as_slice()),
        ("zero", b"\x1b0".as_slice()),
        ("one", b"\x1b1".as_slice()),
        ("two", b"\x1b2".as_slice()),
        ("negative_one", b"\x1b-\x1b1".as_slice()),
    ] {
        let mut keys = b"one\x15two\x06\x15".to_vec();
        keys.extend_from_slice(prefix);
        keys.extend_from_slice(b"\x19X\r");
        let bash = run_bash_readline(&keys);
        let sushline = run_sushline_harness(&keys);

        assert_eq!(
            accepted_line(&bash),
            Some("twoX".to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), 1, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "{desc}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_yank_pop_ignores_numeric_argument() {
    // The two failed C-f motions separate the C-u kills and ring twice.
    // C-y yanks `three`; Meta-y must rotate to `two`, not leave `Xthree`.
    for (desc, prefix) in [
        ("uncounted", b"".as_slice()),
        ("zero", b"\x1b0".as_slice()),
        ("two", b"\x1b2".as_slice()),
        ("nine", b"\x1b9".as_slice()),
        ("negative_one", b"\x1b-\x1b1".as_slice()),
    ] {
        let mut keys = b"one\x15two\x06\x15three\x06\x15X\x19".to_vec();
        keys.extend_from_slice(prefix);
        keys.extend_from_slice(b"\x1by\r");
        let bash = run_bash_readline(&keys);
        let sushline = run_sushline_harness(&keys);

        assert_eq!(
            accepted_line(&bash),
            Some("Xtwo".to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), 2, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 2, "{desc}: {sushline}");
    }
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
fn bash_readline_and_sushline_copy_word_commands_honor_signed_numeric_arguments() {
    let cases: &[(&str, &[u8], &str, &str, usize)] = &[
        (
            "copy-backward-word plain",
            b"",
            "copy-backward-word",
            "one/two three fourtwoX",
            0,
        ),
        (
            "copy-backward-word M-2",
            b"\x1b2",
            "copy-backward-word",
            "one/two three fourone/twoX",
            0,
        ),
        (
            "copy-backward-word M--1",
            b"\x1b-1",
            "copy-backward-word",
            "one/two three fourthreeX",
            0,
        ),
        (
            "copy-backward-word M-0",
            b"\x1b0",
            "copy-backward-word",
            "one/two three fourX",
            1,
        ),
        (
            "copy-forward-word M-2",
            b"\x1b2",
            "copy-forward-word",
            "one/two three fourthree fourX",
            0,
        ),
        (
            "copy-forward-word M--1",
            b"\x1b-1",
            "copy-forward-word",
            "one/two three fourtwoX",
            0,
        ),
        (
            "copy-forward-word M-0",
            b"\x1b0",
            "copy-forward-word",
            "one/two three fourX",
            1,
        ),
    ];
    for (name, argument, command, expected, expected_bells) in cases {
        let mut keys = b"one/two three four\x01".to_vec();
        keys.extend_from_slice(&[0x06; 7]);
        keys.extend_from_slice(argument);
        keys.extend_from_slice(b"\x0f\x05\x19X\r");
        let inputrc = format!(r#""\C-o": {command}"#);
        let bash = run_bash_readline_with_bindings(&keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, &inputrc);
        let expected = Some((*expected).to_string());

        assert_eq!(accepted_line(&bash), expected, "{name}: Bash");
        assert_eq!(accepted_line(&sushline), expected, "{name}: Sushline");
        assert_eq!(bell_count(&bash), *expected_bells, "{name}: Bash");
        assert_eq!(bell_count(&sushline), *expected_bells, "{name}: Sushline");
    }
}

#[test]
fn bash_readline_and_sushline_copy_forward_word_skips_leading_whitespace() {
    let inputrc = r#""\C-o": copy-forward-word"#;
    let mut keys = b"one/two three four\x01".to_vec();
    keys.extend_from_slice(&[0x06; 7]);
    keys.extend_from_slice(b"\x0f\x05\x19X\r");
    let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);
    let expected = Some("one/two three fourthreeX".to_string());

    assert_eq!(accepted_line(&bash), expected, "Bash");
    assert_eq!(accepted_line(&sushline), expected, "Sushline");
    assert_eq!(bell_count(&bash), 0, "Bash");
    assert_eq!(bell_count(&sushline), 0, "Sushline");
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
fn bash_readline_and_sushline_accept_same_direct_vi_complete_binding() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("alpha"), "").expect("write alpha fixture");
    fs::write(dir.path().join("alpine"), "").expect("write alpine fixture");
    let inputrc = "set editing-mode vi\nset completion-query-items 999\nset keymap vi-command\n\"*\": vi-complete";
    let typed = format!("{}/al\x1b*\r", dir.path().display());
    let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
    let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);
    let path = dir.path().display();
    let expected = format!("{path}/alpha {path}/alpine ");

    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some(expected.as_str()),
        "Bash: {bash}"
    );
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some(expected.as_str()),
        "Sushline: {sushline}"
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
fn bash_readline_and_sushline_unix_word_rubout_numeric_arguments_match_bash_5_3() {
    let inputrc = r#""\C-o": unix-word-rubout"#;
    let prefix = b"one two three four\x01\x06\x06\x06\x06\x06\x06\x06";
    let cases: &[(&str, &[u8], &[u8], &str)] = &[
        ("plain", b"", b"\x0fX\r", "one X three four"),
        ("positive_two", b"\x1b2", b"\x0fX\r", "X three four"),
        ("explicit_zero", b"\x1b0", b"\x0fX\r", "one X three four"),
        ("negative_one", b"\x1b-1", b"\x0fX\r", "one X three four"),
        (
            "positive_two_yank_at_eol",
            b"\x1b2",
            b"\x0f\x05\x19X\r",
            " three fourone twoX",
        ),
    ];
    for (name, argument, suffix, expected) in cases {
        let mut keys = prefix.to_vec();
        keys.extend_from_slice(argument);
        keys.extend_from_slice(suffix);
        let bash = run_bash_readline_with_bindings(keys.as_slice(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_slice(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{name}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected.to_string()),
            "{name}: {sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "{name}: {bash}\n{sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_unix_filename_rubout_numeric_arguments_match_bash_5_3() {
    let inputrc = r#""\C-o": unix-filename-rubout"#;
    let prefix = b"one/two three four\x01\x06\x06\x06\x06\x06\x06\x06";
    let cases: &[(&str, &[u8], &str)] = &[
        ("plain", b"", "one/ three fourtwoX"),
        ("positive_two", b"\x1b2", " three fourone/twoX"),
        ("explicit_zero", b"\x1b0", "one/ three fourtwoX"),
        ("negative_one", b"\x1b-1", "one/ three fourtwoX"),
    ];
    for (name, argument, expected) in cases {
        let mut keys = prefix.to_vec();
        keys.extend_from_slice(argument);
        keys.extend_from_slice(b"\x0f\x05\x19X\r");
        let bash = run_bash_readline_with_bindings(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);
        let expected = Some((*expected).to_string());
        assert_eq!(accepted_line(&bash), expected, "{name}: Bash");
        assert_eq!(accepted_line(&sushline), expected, "{name}: Sushline");
        assert_eq!(bell_count(&bash), 0, "{name}: Bash");
        assert_eq!(bell_count(&sushline), 0, "{name}: Sushline");
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

/// Paired Bash 5.3 PTY cases for GNU numeric counts on vi `r`.
///
/// These pin the counted replacements, end-of-line clamping, the point left on
/// the last replaced character, the empty-line bell, and one undo entry.
#[test]
fn bash_readline_and_sushline_accept_same_vi_counted_replace() {
    let inputrc = "set editing-mode vi";
    let cases: &[(&str, &[u8], &str, usize)] = &[
        ("count_three", b"abcdef\x1b03rX\r", "XXXdef", 0),
        ("count_past_end", b"abcdef\x1b010rX\r", "XXXXXX", 0),
        (
            "count_past_end_after_motion",
            b"abcde\x1b0l10rX\r",
            "aXXXX",
            0,
        ),
        ("clamped_at_eol", b"abcde\x1b$3rX\r", "abcdX", 0),
        ("eol_uncounted", b"abcde\x1b$rX\r", "abcdX", 0),
        ("point_on_last_replaced", b"abcdef\x1b03rXx\r", "XXdef", 0),
        (
            "point_after_last_replaced",
            b"abcdef\x1b03rXlx\r",
            "XXXef",
            0,
        ),
        ("empty_line", b"\x1b3rX\r", "", 1),
        ("failed_then_undo", b"\x1b3rXu\r", "", 1),
        ("failed_then_two_undos", b"\x1b3rXuu\r", "", 2),
        ("failed_then_undo_all", b"\x1b3rXU\r", "", 1),
        ("failed_uncounted_then_undo", b"\x1brXu\r", "", 1),
        ("undo", b"abcdef\x1b03rXu\r", "abcdef", 0),
    ];
    for (name, keys, expected_line, expected_bells) in cases {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        let expected_line = Some((*expected_line).to_string());

        assert_eq!(accepted_line(&bash), expected_line, "{name}: Bash: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            expected_line,
            "{name}: Sushline: {sushline}"
        );
        assert_eq!(bell_count(&bash), *expected_bells, "{name}: Bash: {bash}");
        assert_eq!(
            bell_count(&sushline),
            *expected_bells,
            "{name}: Sushline: {sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_replace_whole_multibyte_character() {
    // GNU vi `r` consumes one whole character as the replacement key and
    // replaces whole characters in the line, even when the PTY delivers the
    // replacement bytes one at a time. These pin only the measured forms.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("single", "abc\x1b0rあ\r", "あbc", 0),
        ("counted_two", "abc\x1b02rあ\r", "ああc", 0),
        ("at_eol", "abc\x1b$rあ\r", "abあ", 0),
        ("redo", "abc\x1b0rあ.\r", "あbc", 0),
        ("undo", "abc\x1b02rあu\r", "abc", 0),
        ("four_byte", "abc\x1b0r𝄞\r", "𝄞bc", 0),
        ("mixed_width", "aéllo\x1b0r日\r", "日éllo", 0),
        ("ascii_replacement", "aéllo\x1b0rX\r", "Xéllo", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        let expected = Some(expected.to_string());
        assert_eq!(accepted_line(&bash), expected, "{desc}: {bash}");
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(accepted_line(&sushline), expected, "{desc}: {sushline}");
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

/// Paired Bash 5.3 PTY cases for GNU numeric counts on vi `s`.
///
/// These pin the counted deletions, end-of-line clamping, the empty-line bell,
/// and one undo entry for the deletion and its following insert.
#[test]
fn bash_readline_and_sushline_accept_same_vi_counted_subst() {
    let inputrc = "set editing-mode vi";
    let cases: &[(&str, &[u8], &str, usize)] = &[
        ("count_three", b"abcdef\x1b03sXY\x1b\r", "XYdef", 0),
        (
            "count_past_end_after_motion",
            b"abcde\x1b0l10sXY\x1b\r",
            "aXY",
            0,
        ),
        ("clamped_at_eol", b"abcde\x1b$3sXY\x1b\r", "abcdXY", 0),
        ("eol_uncounted", b"abcde\x1b$sXY\x1b\r", "abcdXY", 0),
        ("empty_line", b"\x1b3sXY\x1b\r", "XY", 1),
        ("undo", b"abcdef\x1b03sXY\x1bu\r", "abcdef", 0),
    ];
    for (name, keys, expected_line, expected_bells) in cases {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        let expected_line = Some((*expected_line).to_string());

        assert_eq!(accepted_line(&bash), expected_line, "{name}: Bash: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            expected_line,
            "{name}: Sushline: {sushline}"
        );
        assert_eq!(bell_count(&bash), *expected_bells, "{name}: Bash: {bash}");
        assert_eq!(
            bell_count(&sushline),
            *expected_bells,
            "{name}: Sushline: {sushline}"
        );
    }
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
fn bash_readline_and_sushline_vi_put_honors_numeric_argument() {
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected) in [
        ("put_two", b"abc\x1b0yy2p\r".as_slice(), "aabcabcbc"),
        ("put_three", b"abc\x1b0yy3p\r".as_slice(), "aabcabcabcbc"),
        (
            "put_word_two",
            b"abc def\x1b0dw2p\r".as_slice(),
            "dabc abc ef",
        ),
    ] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
    let keys = b"abc\x1b0yy2P\r";
    let bash = run_bash_readline_with_bindings(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "put_before_two: bash={bash}\nsushline={sushline}"
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
fn bash_readline_and_sushline_quoted_insert_repeats_by_numeric_argument() {
    for (desc, prefix, expected) in [
        ("uncounted", b"".as_slice(), "a"),
        ("zero", b"\x1b0".as_slice(), ""),
        ("one", b"\x1b1".as_slice(), "a"),
        ("two", b"\x1b2".as_slice(), "aa"),
        ("three", b"\x1b3".as_slice(), "aaa"),
    ] {
        let mut keys = prefix.to_vec();
        keys.extend_from_slice(b"\x16a\r");
        let bash = run_bash_readline(&keys);
        let sushline = run_sushline_harness(&keys);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        println!(
            "{desc}: bash bells={} sush bells={}",
            bell_count(&bash),
            bell_count(&sushline)
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "{desc}: {sushline}"
        );
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
fn bash_readline_and_sushline_numeric_delete_char_kills_for_yank() {
    // Bash 5.3 PTY comparisons for counted forward/backward deletion and plain deletion.
    for (desc, keys, expected) in [
        (
            "positive_count",
            b"abcd\x01\x1b2\x04\x19\r".as_slice(),
            "abcd",
        ),
        ("negative_count", b"abcd\x1b-\x04\x19\r".as_slice(), "abcd"),
        (
            "adjacent_forward_counts",
            b"abcd\x01\x1b1\x04\x1b1\x04\x19\r".as_slice(),
            "abcd",
        ),
        (
            "adjacent_backward_counts",
            b"abcd\x1b-\x04\x1b-\x04\x19\r".as_slice(),
            "abcd",
        ),
        ("uncounted", b"abcd\x01\x04\x19\r".as_slice(), "bcd"),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        let expected = Some(expected.to_string());
        assert_eq!(accepted_line(&bash), expected.clone(), "{desc}: {bash}");
        assert_eq!(accepted_line(&sushline), expected, "{desc}: {sushline}");
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
fn bash_readline_and_sushline_set_mark_region_matches() {
    let inputrc = "\"\\C-o\": set-mark\n\"\\C-w\": kill-region";
    let keys = b"abc\x01\x0f\x06\x17\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    assert_eq!(accepted_line(&bash), Some("bc".to_string()), "{bash}");
    assert_eq!(bell_count(&bash), 0, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&sushline), 0, "{sushline}");
}

#[test]
fn bash_readline_and_sushline_numeric_mark_prefix_parity() {
    // Pin only these numeric prefixes before exchange-point-and-mark and
    // set-mark. Runtime observation, not a general numeric rule.
    for (desc, keys, expected) in [
        (
            "exchange_uncounted",
            b"abcdef\x01\x06\x06\x00\x05\x18\x18X\r".as_slice(),
            "abXcdef",
        ),
        (
            "exchange_m1",
            b"abcdef\x01\x06\x06\x00\x05\x1b1\x18\x18X\r".as_slice(),
            "abXcdef",
        ),
        (
            "exchange_m2",
            b"abcdef\x01\x06\x06\x00\x05\x1b2\x18\x18X\r".as_slice(),
            "abXcdef",
        ),
        (
            "exchange_m0",
            b"abcdef\x01\x06\x06\x00\x05\x1b0\x18\x18X\r".as_slice(),
            "abXcdef",
        ),
        (
            "exchange_negative_m1",
            b"abcdef\x01\x06\x06\x00\x05\x1b-\x31\x18\x18X\r".as_slice(),
            "abXcdef",
        ),
        (
            "set_mark_uncounted",
            b"abcdef\x01\x06\x06\x00\x18\x18X\r".as_slice(),
            "abXcdef",
        ),
        (
            "set_mark_m2",
            b"abcdef\x01\x06\x06\x1b2\x00\x18\x18X\r".as_slice(),
            "abXcdef",
        ),
        (
            "set_mark_m0",
            b"abcdef\x01\x06\x06\x1b0\x00\x18\x18X\r".as_slice(),
            "Xabcdef",
        ),
        (
            "set_mark_negative_m1",
            b"abcdef\x01\x06\x06\x1b-\x31\x00\x18\x18X\r".as_slice(),
            "Xabcdef",
        ),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        println!(
            "{desc}: bash bells={} sush bells={}",
            bell_count(&bash),
            bell_count(&sushline)
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected.to_string()),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
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
fn bash_readline_and_sushline_numeric_backward_kill_line_parity() {
    for (desc, keys, expected) in [
        (
            "negative_one_eol",
            b"abc def\x1b-1\x18\x7fX\r".as_slice(),
            "abc defX",
        ),
        (
            "negative_one_midline",
            b"abc def\x1bb\x1b-1\x18\x7fX\r".as_slice(),
            "abc X",
        ),
        (
            "negative_one_bol",
            b"abc def\x01\x1b-1\x18\x7fX\r".as_slice(),
            "X",
        ),
        (
            "negative_two_eol",
            b"abc def\x1b-2\x18\x7fX\r".as_slice(),
            "abc defX",
        ),
        (
            "negative_two_midline",
            b"abc def\x1bb\x1b-2\x18\x7fX\r".as_slice(),
            "abc X",
        ),
        (
            "negative_two_bol",
            b"abc def\x01\x1b-2\x18\x7fX\r".as_slice(),
            "X",
        ),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some(expected.to_string()),
                "{desc}: {host}={output}"
            );
            assert_eq!(bell_count(output), 0, "{desc}: {host}={output}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_numeric_unix_line_discard_parity() {
    for (desc, keys) in [
        ("zero", b"abc def\x1b0\x15X\r".as_slice()),
        ("negative_one", b"abc def\x1b-\x31\x15X\r".as_slice()),
        ("negative_two", b"abc def\x1b-\x32\x15X\r".as_slice()),
        ("two", b"abc def\x1b2\x15X\r".as_slice()),
        ("uncounted", b"abc def\x15X\r".as_slice()),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some("X".to_string()),
                "{desc}: {host}={output}"
            );
            assert_eq!(bell_count(output), 0, "{desc}: {host}={output}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_zero_prefix_delete_bell_parity() {
    for (desc, keys, expected, bells) in [
        (
            "delete_two",
            b"abcdef\x01\x1b2\x04X\r".as_slice(),
            "Xcdef",
            0,
        ),
        (
            "delete_three",
            b"abcdef\x01\x1b3\x04X\r".as_slice(),
            "Xdef",
            0,
        ),
        (
            "delete_zero",
            b"abcdef\x01\x1b0\x04X\r".as_slice(),
            "Xabcdef",
            0,
        ),
        ("delete_zero_empty", b"\x1b0\x04X\r".as_slice(), "X", 1),
        (
            "delete_negative_one",
            b"abcdef\x01\x1b-\x31\x04X\r".as_slice(),
            "Xabcdef",
            1,
        ),
        (
            "delete_uncounted",
            b"abcdef\x01\x04X\r".as_slice(),
            "Xbcdef",
            0,
        ),
        (
            "backward_delete_two",
            b"abcdef\x1b2\x7fX\r".as_slice(),
            "abcdX",
            0,
        ),
        (
            "backward_delete_zero",
            b"abcdef\x1b0\x7fX\r".as_slice(),
            "abcdefX",
            0,
        ),
        (
            "backward_delete_zero_empty",
            b"\x1b0\x7fX\r".as_slice(),
            "X",
            1,
        ),
        (
            "backward_delete_zero_at_beginning",
            b"abc\x01\x1b0\x7fX\r".as_slice(),
            "Xabc",
            1,
        ),
        (
            "backward_delete_negative_one",
            b"abcdef\x1b-\x31\x7fX\r".as_slice(),
            "abcdefX",
            1,
        ),
        (
            "backward_delete_uncounted",
            b"abcdef\x7fX\r".as_slice(),
            "abcdeX",
            0,
        ),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some(expected.to_string()),
                "{desc}: {host}={output}"
            );
            assert_eq!(bell_count(output), bells, "{desc}: {host}={output}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_numeric_word_kill_prefix_parity() {
    for (desc, keys, expected) in [
        (
            "kill_word_two",
            b"abc def\x01\x1b2\x1b\x64X\r".as_slice(),
            "X",
        ),
        (
            "kill_word_zero",
            b"abc def\x01\x1b0\x1b\x64X\r".as_slice(),
            "Xabc def",
        ),
        (
            "kill_word_negative_one",
            b"abc def\x01\x1b-\x31\x1b\x64X\r".as_slice(),
            "Xabc def",
        ),
        (
            "backward_kill_word_two",
            b"abc def\x1b2\x1b\x7fX\r".as_slice(),
            "X",
        ),
        (
            "backward_kill_word_zero",
            b"abc def\x1b0\x1b\x7fX\r".as_slice(),
            "abc defX",
        ),
        (
            "backward_kill_word_negative_one",
            b"abc def\x1b-\x31\x1b\x7fX\r".as_slice(),
            "abc defX",
        ),
        (
            "kill_word_uncounted",
            b"abc def\x01\x1b\x64X\r".as_slice(),
            "X def",
        ),
        (
            "backward_kill_word_uncounted",
            b"abc def\x1b\x7fX\r".as_slice(),
            "abc X",
        ),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some(expected.to_string()),
                "{desc}: {host}={output}"
            );
            assert_eq!(bell_count(output), 0, "{desc}: {host}={output}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_numeric_transpose_prefix_parity() {
    for (desc, keys, expected, bells) in [
        (
            "transpose_chars_two",
            b"abcdef\x01\x06\x06\x1b2\x14X\r".as_slice(),
            "acdbXef",
            0,
        ),
        (
            "transpose_chars_three",
            b"abcdef\x01\x06\x06\x1b3\x14X\r".as_slice(),
            "acdebXf",
            0,
        ),
        (
            "transpose_chars_zero",
            b"abcdef\x01\x06\x06\x1b0\x14X\r".as_slice(),
            "abXcdef",
            0,
        ),
        (
            "transpose_chars_negative_one",
            b"abcdef\x01\x06\x06\x1b-\x31\x14X\r".as_slice(),
            "abXcdef",
            0,
        ),
        (
            "transpose_chars_uncounted",
            b"abcdef\x01\x06\x06\x14X\r".as_slice(),
            "acbXdef",
            0,
        ),
        (
            "transpose_words_two",
            b"abc def ghi\x1b2\x1b\x74X\r".as_slice(),
            "ghi def abcX",
            0,
        ),
        (
            "transpose_words_zero",
            b"abc def ghi\x1b0\x1b\x74X\r".as_slice(),
            "abc def ghiX",
            0,
        ),
        (
            "transpose_words_negative_one",
            b"abc def ghi\x1b-\x31\x1b\x74X\r".as_slice(),
            "abc def ghiX",
            1,
        ),
        (
            "transpose_words_uncounted",
            b"abc def ghi\x1b\x74X\r".as_slice(),
            "abc ghi defX",
            0,
        ),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some(expected.to_string()),
                "{desc}: {host}={output}"
            );
            assert_eq!(bell_count(output), bells, "{desc}: {host}={output}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_numeric_word_case_prefix_parity() {
    for (desc, keys, expected) in [
        (
            "upcase_word_two",
            b"abc def ghi\x01\x1b2\x1b\x75X\r".as_slice(),
            "ABC DEFX ghi",
        ),
        (
            "upcase_word_zero",
            b"abc def ghi\x01\x1b0\x1b\x75X\r".as_slice(),
            "Xabc def ghi",
        ),
        (
            "upcase_word_negative_one",
            b"abc def ghi\x01\x1b-\x31\x1b\x75X\r".as_slice(),
            "Xabc def ghi",
        ),
        (
            "upcase_word_uncounted",
            b"abc def ghi\x01\x1b\x75X\r".as_slice(),
            "ABCX def ghi",
        ),
        (
            "downcase_word_two",
            b"ABC DEF GHI\x01\x1b2\x1b\x6cX\r".as_slice(),
            "abc defX GHI",
        ),
        (
            "downcase_word_zero",
            b"ABC DEF GHI\x01\x1b0\x1b\x6cX\r".as_slice(),
            "XABC DEF GHI",
        ),
        (
            "downcase_word_negative_one",
            b"ABC DEF GHI\x01\x1b-\x31\x1b\x6cX\r".as_slice(),
            "XABC DEF GHI",
        ),
        (
            "downcase_word_uncounted",
            b"ABC DEF GHI\x01\x1b\x6cX\r".as_slice(),
            "abcX DEF GHI",
        ),
        (
            "capitalize_word_two",
            b"abc def ghi\x01\x1b2\x1b\x63X\r".as_slice(),
            "Abc DefX ghi",
        ),
        (
            "capitalize_word_zero",
            b"abc def ghi\x01\x1b0\x1b\x63X\r".as_slice(),
            "Xabc def ghi",
        ),
        (
            "capitalize_word_negative_one",
            b"abc def ghi\x01\x1b-\x31\x1b\x63X\r".as_slice(),
            "Xabc def ghi",
        ),
        (
            "capitalize_word_uncounted",
            b"abc def ghi\x01\x1b\x63X\r".as_slice(),
            "AbcX def ghi",
        ),
    ] {
        let bash = run_bash_readline(keys);
        let sushline = run_sushline_harness(keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some(expected.to_string()),
                "{desc}: {host}={output}"
            );
            assert_eq!(bell_count(output), 0, "{desc}: {host}={output}");
        }
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

fn run_bracketed_paste_checkpoints(paste: &[u8], include_hex: bool) -> (Vec<String>, Vec<String>) {
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
    let accepted_output = if include_hex {
        r#"printf 'SUSHLINE_ACCEPTED:%s\nSUSHLINE_ACCEPTED_HEX:' "$line"; printf '%s' "$line" | od -An -tx1 | tr -d ' \n'; printf '\nSUSHLINE_DONE\n'"#
    } else {
        r#"printf 'SUSHLINE_ACCEPTED:%s\nSUSHLINE_DONE\n' "$line""#
    };
    let script = format!(
        r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; {accepted_output}"#
    );
    bash.args(["--noprofile", "--norc", "-i", "-c", &script]);
    let mut sushline = CommandBuilder::new("sh");
    isolate_startup_inputrc(&mut sushline);
    sushline.env("LC_ALL", "C");
    sushline.env("TERM", "xterm-256color");
    sushline.env("SUSHLINE_INPUTRC", inputrc);
    sushline.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline.env("SUSHLINE_HISTORY", "");
    if include_hex {
        sushline.env("SUSHLINE_ACCEPTED_HEX", "");
    }
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
        run_bracketed_paste_checkpoints(b"\x1b[200~abcdefghijklmnop\x1b[201~", false);

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
fn bash_read_e_and_sushline_expose_bracketed_paste_cr_boundary() {
    // In this fixture, Readline maps pasted CR to LF; Bash `read -e` reports
    // only the first line while Sushline exposes the full editor byte buffer.
    let (bash_frames, sushline_frames) =
        run_bracketed_paste_checkpoints(b"\x1b[200~a\rb\x1b[201~", true);

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
    assert!(sushline_line.starts_with("a\r\nb\r\n"), "{sushline_line:?}");
    let accepted_hex = |frames: &[String]| {
        frames[1]
            .split_once("SUSHLINE_ACCEPTED_HEX:")
            .expect("accepted line hex")
            .1
            .split(['\r', '\n'])
            .next()
            .unwrap()
            .to_owned()
    };
    assert_eq!(accepted_hex(&bash_frames), "61");
    assert_eq!(accepted_hex(&sushline_frames), "610a62");
}

#[test]
fn bash_and_sushline_insert_control_bytes_literally_in_bracketed_paste() {
    let inputrc = "set enable-bracketed-paste on\nset bell-style audible\n";
    for term in [Some("xterm-256color"), Some("dumb")] {
        for (byte, expected) in [
            (0x01, "a\u{1}b"),
            (0x09, "a\u{9}b"),
            (0x0b, "a\u{b}b"),
            (0x12, "a\u{12}b"),
            (0x1b, "a\u{1b}b"),
            (0x7f, "a\u{7f}b"),
        ] {
            let mut keys = b"\x1b[200~a".to_vec();
            keys.push(byte);
            keys.extend_from_slice(b"b\x1b[201~\r");
            let bash = run_bash_readline_with_term(&keys, inputrc, term);
            let sushline = run_sushline_harness_with_term(&keys, inputrc, term);
            for (name, output) in [("Bash", &bash), ("Sushline", &sushline)] {
                assert_eq!(
                    accepted_line(output).as_deref(),
                    Some(expected),
                    "{name} TERM={term:?} byte={byte:#04x}: {output:?}"
                );
                assert_eq!(
                    bell_count(output),
                    0,
                    "{name} TERM={term:?} byte={byte:#04x}: {output:?}"
                );
            }
        }
    }
}

#[test]
fn bash_and_sushline_insert_multibyte_payload_literally_in_bracketed_paste() {
    if !c_utf8_locale_available() {
        return;
    }

    let inputrc = "set enable-bracketed-paste on\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for payload in ["é", "中", "🦀"] {
        let keys = format!("\x1b[200~a{payload}b\x1b[201~\r").into_bytes();
        let bash = run_bash_readline_with_inputrc_file_and_env(&keys, inputrc, &env);
        let sushline = run_sushline_harness_with_inputrc_and_env(&keys, inputrc, &env);
        let expected = format!("a{payload}b");
        for (name, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert!(
                output.contains("\x1b[?2004h"),
                "{name} payload={payload:?} did not enable bracketed paste: {output:?}"
            );
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{name} payload={payload:?}: {output:?}"
            );
            assert_eq!(
                bell_count(output),
                0,
                "{name} payload={payload:?}: {output:?}"
            );
        }
    }
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

#[test]
fn bash_readline_and_sushline_multibyte_character_search_matches_like_bash() {
    // A character search takes a character, so a multibyte target is one
    // character: the search matches it and consumes the whole character,
    // like GNU. The vi mark commands take one byte, so a multibyte mark
    // name is not a name and the rest of the character arrives as ordinary
    // input. Measured against Bash 5.3 in a PTY. Runtime observation, not
    // GNU source proof.
    let vi = "set editing-mode vi";
    for (desc, keys, inputrc, expected, bells) in [
        (
            "emacs_no_match",
            b"abc\x01\x1d\xC3\xA9\r".as_slice(),
            "",
            "abc",
            1,
        ),
        (
            "emacs_match",
            b"a\xC3\xA9bc\x01\x1d\xC3\xA9\x04\r".as_slice(),
            "",
            "abc",
            0,
        ),
        (
            "emacs_backward_match",
            b"a\xC3\xA9bc\x1b\x1d\xC3\xA9\x04\r".as_slice(),
            "",
            "abc",
            0,
        ),
        (
            "emacs_no_match_wide",
            b"abc\x01\x1d\xE2\x82\xAC\r".as_slice(),
            "",
            "abc",
            1,
        ),
        ("vi_no_match", b"ab\x1b0f\xC3\xA9\r".as_slice(), vi, "ab", 1),
        (
            "vi_match",
            b"a\xC3\xA9b\x1b0f\xC3\xA9\x1biX\x1b\r".as_slice(),
            vi,
            "aXéb",
            0,
        ),
        (
            "vi_backward_match",
            b"a\xC3\xA9b\x1b$F\xC3\xA9\x1biX\x1b\r".as_slice(),
            vi,
            "aXéb",
            0,
        ),
        (
            "vi_no_match_wide",
            b"abc\x1b0f\xE2\x82\xAC\r".as_slice(),
            vi,
            "abc",
            1,
        ),
        (
            "vi_mark_name_is_one_byte",
            b"abc\x1b0m\xE2\x82\xAC\r".as_slice(),
            vi,
            "abc",
            3,
        ),
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
fn bash_readline_and_sushline_vi_mark_names_are_lowercase_letters_like_bash() {
    // GNU Readline names a vi mark by a lowercase ASCII letter and rings
    // for any other name. Measured against Bash 5.3 in a PTY. Runtime
    // observation, not GNU source proof.
    let vi = "set editing-mode vi";
    for (desc, keys, bells) in [
        ("lower", b"abc\x1b0ma\r".as_slice(), 0),
        ("upper", b"abc\x1b0mA\r".as_slice(), 1),
        ("digit", b"abc\x1b0m1\r".as_slice(), 1),
        ("goto_unset_lower", b"abc\x1b0`a\r".as_slice(), 1),
        ("goto_unset_upper", b"abc\x1b0`A\r".as_slice(), 1),
        ("set_then_goto", b"abc\x1b0ma\x1b0`a\r".as_slice(), 0),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, vi);
        let sushline = run_sushline_harness_with_inputrc(keys, vi);
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
fn bash_readline_and_sushline_byte_motions_land_on_utf8_boundaries() {
    if !c_utf8_locale_available() {
        return;
    }

    let locale_env = [("LC_ALL", "C.UTF-8")];
    let forward = r#""\C-o": forward-byte"#;
    let backward = r#""\C-o": backward-byte"#;
    for (inputrc, keys, expected) in [
        (forward, "αβγ\u{1}\u{f}\u{f}X\r".as_bytes(), "αXβγ"),
        (backward, "αβγ\u{2}\u{f}\u{f}Y\r".as_bytes(), "αYβγ"),
    ] {
        let bash = run_bash_readline_with_bindings_and_env(keys, inputrc, &locale_env);
        let sushline = run_sushline_harness_with_inputrc_and_env(keys, inputrc, &locale_env);
        let context = format!("keys={keys:?}\nbash={bash}\nsushline={sushline}");
        assert_eq!(accepted_line(&bash).as_deref(), Some(expected), "{context}");
        assert_eq!(accepted_line(&sushline), accepted_line(&bash), "{context}");
    }
}

#[test]
fn bash_and_sushline_restart_counts_after_universal_in_multiplied_minus_tail() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let base = format!("{}{}", "a".repeat(60), "-".repeat(48));
    // A universal argument ends a multiplied-minus tail. GNU then treats the
    // following argument keys as a fresh count instead of swallowing them.
    for (desc, suffix, repeat) in [
        ("bare universal", b"\x0fX\r".as_slice(), 4),
        ("bare digit", b"\x0f5X\r".as_slice(), 5),
        ("two bare digits", b"\x0f56X\r".as_slice(), 56),
        ("minus", b"\x0f-X\r".as_slice(), 0),
        ("meta digit", b"\x0f\x1b5X\r".as_slice(), 5),
        ("two universals then digit", b"\x0f\x0f5X\r".as_slice(), 5),
    ] {
        let mut input = vec![b'a'; 60];
        input.extend_from_slice(b"\x1b3\x0f\x0f\x0f-");
        input.extend_from_slice(suffix);
        let bash = run_bash_readline_with_inputrc_file(&input, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&input, inputrc);
        let expected = format!("{base}{}", "X".repeat(repeat));
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), 0, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_keep_fresh_counts_after_a_literal_tail_prefix() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    let base = format!("{}555", "a".repeat(60));
    // An argument prefix inside a no-op literal tail starts a fresh count.
    // Without a blocking command, the following self-insert repeats by it;
    // bare digits extend it and a minus makes it negative.
    for (desc, suffix, repeat) in [
        ("universal", b"5\x0fX\r".as_slice(), 4),
        ("universal then digit", b"5\x0f6X\r".as_slice(), 6),
        ("universal then two digits", b"5\x0f67X\r".as_slice(), 67),
        ("universal then minus", b"5\x0f-X\r".as_slice(), 0),
        ("meta digit", b"5\x1b6X\r".as_slice(), 6),
        ("meta two digits", b"5\x1b67X\r".as_slice(), 67),
        (
            "universal digit then meta",
            b"5\x0f6\x1b7X\r".as_slice(),
            67,
        ),
    ] {
        let mut input = vec![b'a'; 60];
        input.extend_from_slice(b"\x1b3\x0f");
        input.extend_from_slice(suffix);
        let bash = run_bash_readline_with_inputrc_file(&input, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&input, inputrc);
        let expected = format!("{base}{}", "X".repeat(repeat));
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected.as_str()),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), 0, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_keep_negative_sign_after_bare_minus() {
    let inputrc = "set bind-tty-special-chars off\n\"\\C-o\": universal-argument\n";
    // A bare minus after a sign-only argument is not a second sign on GNU:
    // the argument stays negative and the following self-insert is dropped.
    // Meta-prefixed minuses still toggle the sign.
    for (desc, keys, expected) in [
        ("meta minus then bare minus", b"\x1b--X\r".as_slice(), ""),
        (
            "meta minus then two bare minuses",
            b"\x1b---X\r".as_slice(),
            "",
        ),
        (
            "universal then two bare minuses",
            b"\x0f--X\r".as_slice(),
            "",
        ),
        ("bare minus then digit", b"\x1b--1X\r".as_slice(), ""),
        ("two meta minuses", b"\x1b-\x1b-X\r".as_slice(), "X"),
        (
            "universal minus then meta minus",
            b"\x0f-\x1b-X\r".as_slice(),
            "X",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{editor} {desc}: {output}"
            );
            assert_eq!(bell_count(output), 0, "{editor} {desc}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_insert_literal_argument_keys_after_positive_counts() {
    let inputrc = "";
    // GNU turns an argument key into literal text once a positive count is
    // established: a minus after any positive count, or a digit after a
    // positive sign-only count. Counts themselves still compose as before.
    for (desc, keys, expected) in [
        ("meta two literal minus", b"\x1b2-X\r".as_slice(), "--X"),
        ("meta two two minuses", b"\x1b2--X\r".as_slice(), "---X"),
        ("meta two minus digit", b"\x1b2-1X\r".as_slice(), "--1X"),
        ("two meta minuses minus", b"\x1b-\x1b--X\r".as_slice(), "-X"),
        ("two meta minuses digit", b"\x1b-\x1b-1X\r".as_slice(), "1X"),
        (
            "two meta minuses minus digit",
            b"\x1b-\x1b--1X\r".as_slice(),
            "-1X",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{editor} {desc}: {output}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_start_a_literal_tail_at_zero_count() {
    let inputrc = "";
    let mut base = b"abcdefghijklmnopqrstuvwxyz\x01".to_vec();
    base.extend_from_slice(&[b'\x06'; 5]);
    // A minus at a zero count inserts no copies but starts a literal tail, so
    // later bare argument keys insert once. A positive sign-only zero also
    // inserts one bare digit, while negative zeros still compose digits.
    for (desc, suffix, expected) in [
        (
            "meta zero minus",
            b"\x1b0-X\r".as_slice(),
            "abcdeXfghijklmnopqrstuvwxyz",
        ),
        (
            "minus zero minus",
            b"\x1b-0-X\r".as_slice(),
            "abcdeXfghijklmnopqrstuvwxyz",
        ),
        (
            "meta zero minus digit",
            b"\x1b0-1X\r".as_slice(),
            "abcde1Xfghijklmnopqrstuvwxyz",
        ),
        (
            "meta zero minus minus",
            b"\x1b0--X\r".as_slice(),
            "abcde-Xfghijklmnopqrstuvwxyz",
        ),
        (
            "minus zero meta minus digit",
            b"\x1b-0\x1b-1X\r".as_slice(),
            "abcde1Xfghijklmnopqrstuvwxyz",
        ),
        (
            "meta zero minus meta digit",
            b"\x1b0-\x1b1X\r".as_slice(),
            "abcdeXfghijklmnopqrstuvwxyz",
        ),
    ] {
        let mut keys = base.clone();
        keys.extend_from_slice(suffix);
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{editor} {desc}: {output}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_read_consecutive_bytes_for_negative_quoted_insert() {
    for (desc, keys, expected) in [
        (
            "minus meta one",
            b"\x1b-\x1b1\x16abcdefghijk\r".as_slice(),
            Some("abcdefghijk"),
        ),
        (
            "minus meta two",
            b"\x1b-\x1b2\x16abcdefghijkl\r".as_slice(),
            Some("abcdefghijkl"),
        ),
        (
            "enter as the last quoted byte",
            b"\x1b-\x1b1\x16abcdefghij\r\x7fX\r".as_slice(),
            Some("abcdefghijX"),
        ),
        (
            "positive count repeats one byte",
            b"\x1b2\x16abc\r".as_slice(),
            Some("aabc"),
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, "");
        let sushline = run_sushline_harness_with_inputrc(keys, "");
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                expected,
                "{editor} {desc}: {output}"
            );
        }
        println!(
            "{desc}: bash bells={} sush bells={}",
            bell_count(&bash),
            bell_count(&sushline)
        );
    }
}

#[cfg(unix)]
#[test]
fn bash_and_sushline_pin_multiline_bracketed_paste_across_resize() {
    use portable_pty::{CommandBuilder, PtySize};

    let dir = tempfile::tempdir().expect("paste resize dir");
    let inputrc = "set enable-bracketed-paste on\nset bell-style audible\n";
    let inputrc_path = dir.path().join("inputrc");
    fs::write(&inputrc_path, inputrc).expect("write inputrc");
    let mut bash_command = CommandBuilder::new("bash");
    bash_command.env("INPUTRC", inputrc_path.as_os_str());
    bash_command.env("HISTFILE", dir.path().join("history").as_os_str());
    bash_command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    let mut sushline_command = sushline_harness_command();
    sushline_command.env("SUSHLINE_INPUTRC", inputrc);
    sushline_command.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline_command.env("SUSHLINE_HISTORY", "");

    for (editor, command) in [("Bash", bash_command), ("Sushline", sushline_command)] {
        let (output, post_resize) = run_pty_steps_then_resize_until_with_post_resize(
            command,
            &[(READY_PROMPT, &b"\x1b[200~first\nsecond\x1b[201~"[..])],
            dir.path(),
            PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            },
            PtyResizeAction {
                marker: "second",
                size: PtySize {
                    rows: 12,
                    cols: 20,
                    pixel_width: 0,
                    pixel_height: 0,
                },
                post_resize_marker: Some(READY_PROMPT),
                after_keys: b"\r",
            },
            "SUSHLINE_ACCEPTED:",
        );
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("first"),
            "{editor}: resize changed multiline paste acceptance\n{output:?}"
        );
        let redrawn = post_resize.split("SUSHLINE_ACCEPTED:").next().unwrap_or("");
        assert!(
            redrawn.contains("second"),
            "{editor}: second pasted line was not repainted after resize\n{post_resize:?}"
        );
        assert!(
            !output.contains("--More--"),
            "{editor}: unexpected pager prompt\n{output:?}"
        );
        assert_eq!(
            bell_count(&output),
            0,
            "{editor}: unexpected bell\n{output:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn bash_and_sushline_pin_bracketed_paste_across_resize_before_end_marker() {
    use portable_pty::{CommandBuilder, PtySize};

    let dir = tempfile::tempdir().expect("paste resize dir");
    let inputrc = "set enable-bracketed-paste on\nset bell-style audible\n";
    let inputrc_path = dir.path().join("inputrc");
    fs::write(&inputrc_path, inputrc).expect("write inputrc");
    // Readline does not echo a paste payload before its end marker, so the
    // two pre-resize gates are short shell writes to the same terminal.
    let markers = r#"( sleep 1; printf '\rPASTE_NOW\n'; sleep 1; printf '\rRESIZE_NOW\n' ) & "#;
    let mut bash_command = CommandBuilder::new("bash");
    bash_command.env("INPUTRC", inputrc_path.as_os_str());
    bash_command.env("HISTFILE", dir.path().join("history").as_os_str());
    bash_command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"{markers}stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    let mut sushline_command = CommandBuilder::new("bash");
    isolate_startup_inputrc(&mut sushline_command);
    sushline_command.env("SUSHLINE_HARNESS", env!("CARGO_BIN_EXE_sushline-harness"));
    sushline_command.env("SUSHLINE_INPUTRC", inputrc);
    sushline_command.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline_command.env("SUSHLINE_HISTORY", "");
    sushline_command.args(["-c", &format!(r#"{markers}exec "$SUSHLINE_HARNESS""#)]);

    for (desc, second_step, after_keys) in [
        ("open frame", &b"def"[..], &b"\x1b[201~\r"[..]),
        ("partial end marker", &b"def\x1b[201"[..], &b"~\r"[..]),
    ] {
        for (editor, command) in [
            ("Bash", bash_command.clone()),
            ("Sushline", sushline_command.clone()),
        ] {
            let (output, post_resize) = run_pty_steps_then_resize_until_with_post_resize(
                command,
                &[
                    (READY_PROMPT, &b"\x1b[200~abc"[..]),
                    ("PASTE_NOW", second_step),
                ],
                dir.path(),
                PtySize {
                    rows: 30,
                    cols: 100,
                    pixel_width: 0,
                    pixel_height: 0,
                },
                PtyResizeAction {
                    marker: "RESIZE_NOW",
                    size: PtySize {
                        rows: 12,
                        cols: 20,
                        pixel_width: 0,
                        pixel_height: 0,
                    },
                    post_resize_marker: None,
                    after_keys,
                },
                "SUSHLINE_ACCEPTED:",
            );
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some("abcdef"),
                "{editor} {desc}: {output:?}"
            );
            assert_eq!(bell_count(&output), 0, "{editor} {desc}: {output:?}");
            assert!(
                post_resize.contains("SUSHLINE_ACCEPTED:abcdef"),
                "{editor} {desc}: acceptance did not follow the resize\n{post_resize:?}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_pin_insert_comment_default_and_numeric_toggle() {
    for (label, inputrc, keys, expected) in [
        ("default", "", b"alpha\x1b#".as_slice(), "#alpha"),
        (
            "explicit",
            "set comment-begin \"## \"",
            b"alpha\x1b#".as_slice(),
            "## alpha",
        ),
        (
            "numeric_insert",
            "",
            b"alpha\x1b3\x1b#".as_slice(),
            "#alpha",
        ),
        (
            "numeric_remove",
            "",
            b"#alpha\x1b3\x1b#".as_slice(),
            "alpha",
        ),
        (
            "explicit_remove",
            "set comment-begin \"## \"",
            b"## alpha\x1b3\x1b#".as_slice(),
            "alpha",
        ),
        (
            "partial_prefix",
            "set comment-begin \"## \"",
            b"#alpha\x1b3\x1b#".as_slice(),
            "## #alpha",
        ),
        ("empty_line", "", b"\x1b#".as_slice(), "#"),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        let context = format!("{label}: bash={bash} sushline={sushline}");
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash accepted line differs\n{context}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline accepted line differs\n{context}"
        );
        assert_eq!(bell_count(&bash), 0, "Bash rang a bell\n{context}");
        assert_eq!(bell_count(&sushline), 0, "Sushline rang a bell\n{context}");
    }
}

#[test]
fn bash_readline_and_sushline_explicit_skip_csi_sequence_discards_the_tail() {
    // With `\e[` explicitly bound to skip-csi-sequence, the introducer is
    // already consumed when the command runs; both editors must discard the
    // rest of the CSI sequence through its final byte, then accept the
    // following text.
    let inputrc = "\"\\e[\": skip-csi-sequence\n";
    for (name, seq, expected) in [
        ("parameters", &b"\x1b[99~"[..], "abcX"),
        ("final_byte_only", &b"\x1b[ZZ"[..], "abcZX"),
    ] {
        let mut keys = b"abc".to_vec();
        keys.extend_from_slice(seq);
        keys.extend_from_slice(b"X\r");
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_arrow_key_prefix_rings_on_unbound_tail() {
    // GNU's arrow-key-prefix accepts a bound arrow code after the consumed
    // introducer. Any other tail rings once and consumes exactly one byte;
    // it is not a full CSI sequence, so a following final byte stays input.
    for (name, inputrc, seq, expected) in [
        (
            "ss3_digit",
            "\"\\eO\": arrow-key-prefix\n",
            &b"\x1bO99~"[..],
            "abc9~X",
        ),
        (
            "ss3_unbound_code",
            "\"\\eO\": arrow-key-prefix\n",
            &b"\x1bOP"[..],
            "abcX",
        ),
        (
            "ss3_second_introducer",
            "\"\\eO\": arrow-key-prefix\n",
            &b"\x1bO[A"[..],
            "abcAX",
        ),
        (
            "csi_digit",
            "\"\\e[\": arrow-key-prefix\n",
            &b"\x1b[1"[..],
            "abcX",
        ),
    ] {
        let mut keys = b"abc".to_vec();
        keys.extend_from_slice(seq);
        keys.extend_from_slice(b"X\r");
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 1, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 1, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_explicit_ss3_skip_discards_the_tail() {
    let inputrc = "\"\\eO\": skip-csi-sequence\n";
    for (name, seq, expected) in [
        ("parameters", &b"\x1bO99~"[..], "abcX"),
        ("final_byte", &b"\x1bO[A "[..], "abcA X"),
    ] {
        let mut keys = b"abc".to_vec();
        keys.extend_from_slice(seq);
        keys.extend_from_slice(b"X\r");
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_full_key_skip_starts_in_the_tail() {
    for (name, inputrc, seq) in [
        (
            "csi_key",
            "\"\\e[1~\": skip-csi-sequence\n",
            &b"\x1b[1~99~"[..],
        ),
        (
            "ss3_key",
            "\"\\eOA\": skip-csi-sequence\n",
            &b"\x1bOA99~"[..],
        ),
    ] {
        let mut keys = b"abc".to_vec();
        keys.extend_from_slice(seq);
        keys.extend_from_slice(b"X\r");
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some("abcX"),
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("abcX"),
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_arrow_key_prefix_consumes_numeric_argument() {
    let inputrc = "\"\\eO\": arrow-key-prefix\n";
    for (name, count) in [("two", b'2'), ("three", b'3')] {
        let keys = [b"abc\x1b".as_slice(), &[count], b"\x1bO9X\r"].concat();
        let bash = run_bash_readline_with_inputrc_file(&keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some("abcX"),
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 1, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("abcX"),
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 1, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_custom_skip_starts_at_the_next_byte() {
    // GNU starts skipping at the next byte after any skip-csi-sequence
    // binding, not only after an ESC [ or ESC O introducer. It consumes
    // parameter/intermediate bytes through the first final byte; a nested
    // ESC ends the skip after being consumed.
    for (name, inputrc, keys, expected) in [
        (
            "single_byte_csi_final",
            "\"y\": skip-csi-sequence\n",
            &b"abcy[D X\r"[..],
            "abcD X",
        ),
        (
            "single_byte_parameters",
            "\"y\": skip-csi-sequence\n",
            &b"abcy[99~X\r"[..],
            "abc99~X",
        ),
        (
            "single_byte_without_introducer",
            "\"y\": skip-csi-sequence\n",
            &b"abcy9Z\r"[..],
            "abc",
        ),
        (
            "single_byte_space_parameter",
            "\"y\": skip-csi-sequence\n",
            &b"abcy Z\r"[..],
            "abc",
        ),
        (
            "single_byte_final_byte",
            "\"y\": skip-csi-sequence\n",
            &b"abcyZZ\r"[..],
            "abcZ",
        ),
        (
            "single_byte_ss3_final",
            "\"y\": skip-csi-sequence\n",
            &b"abcyOA X\r"[..],
            "abcA X",
        ),
        (
            "single_byte_nested_escape",
            "\"y\": skip-csi-sequence\n",
            &b"abcy\x1b[D X\r"[..],
            "abc[D X",
        ),
        (
            "escape_binding",
            "\"\\eY\": skip-csi-sequence\n",
            &b"abc\x1bY[D X\r"[..],
            "abcD X",
        ),
        (
            "multi_byte_binding",
            "\"y1\": skip-csi-sequence\n",
            &b"abcy1[D X\r"[..],
            "abcD X",
        ),
        (
            "control_byte_tabs",
            "\"y\": skip-csi-sequence\n",
            &b"abcy\x0931~X\r"[..],
            "abc31~X",
        ),
        (
            "control_byte_nul",
            "\"y\": skip-csi-sequence\n",
            &b"abcy\x0031~X\r"[..],
            "abc31~X",
        ),
        (
            "control_byte_del",
            "\"y\": skip-csi-sequence\n",
            &b"abcy\x7f31~X\r"[..],
            "abc31~X",
        ),
        (
            "control_byte_ctrl_a",
            "\"y\": skip-csi-sequence\n",
            &b"abcy\x01X\r"[..],
            "abcX",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_bare_introducer_skip_discards_the_tail() {
    for (name, intro, seq) in [
        ("bracket", "[", &b"[99~"[..]),
        ("letter_o", "O", &b"O99~"[..]),
    ] {
        let inputrc = format!("\"{intro}\": skip-csi-sequence\n");
        let mut keys = b"abc".to_vec();
        keys.extend_from_slice(seq);
        keys.extend_from_slice(b"X\r");
        let bash = run_bash_readline_with_inputrc_file(&keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, &inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some("abcX"),
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("abcX"),
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_bare_arrow_prefix_uses_arrow_keymap() {
    let history = ["one", "two"];
    for (name, inputrc, keys, expected, bells) in [
        (
            "bracket_history",
            "\"[\": arrow-key-prefix\n",
            &b"abc[A\r"[..],
            Some("two"),
            0,
        ),
        (
            "bracket_history_twice",
            "\"[\": arrow-key-prefix\n",
            &b"abc[A[A\r"[..],
            Some("one"),
            0,
        ),
        (
            "bracket_history_return",
            "\"[\": arrow-key-prefix\n",
            &b"abc[A[B\r"[..],
            Some("abc"),
            0,
        ),
        (
            "bracket_backward",
            "\"[\": arrow-key-prefix\n",
            &b"abc[D X\r"[..],
            Some("ab Xc"),
            0,
        ),
        (
            "bracket_forward",
            "\"[\": arrow-key-prefix\n",
            &b"abc[D[C X\r"[..],
            Some("abc X"),
            0,
        ),
        (
            "bracket_unbound",
            "\"[\": arrow-key-prefix\n",
            &b"abc[D[H X\r"[..],
            Some("ab Xc"),
            1,
        ),
        (
            "ss3_history",
            "\"O\": arrow-key-prefix\n",
            &b"abcOA\r"[..],
            Some("two"),
            0,
        ),
        (
            "ss3_backward",
            "\"O\": arrow-key-prefix\n",
            &b"abcOD X\r"[..],
            Some("ab Xc"),
            0,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            expected,
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            expected,
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_counted_bare_arrow_prefix() {
    let inputrc = "\"[\": arrow-key-prefix\n";
    let history = ["one", "two", "three"];
    for (name, keys, expected, bells) in [
        ("history", &b"abc\x1b2[A\r"[..], Some("two"), 0),
        ("movement", &b"abc\x1b2[D X\r"[..], Some("a Xbc"), 0),
        ("unbound", &b"abc\x1b2[H X\r"[..], Some("abc X"), 1),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            expected,
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            expected,
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_bare_skip_restarts_matching_introducer() {
    for (name, intro, tail, expected) in [
        ("bracket_csi", b'[', &b"\x1b[A"[..], Some("abcX")),
        ("bracket_ss3", b'[', &b"\x1bOA"[..], Some("abcOAX")),
        ("bracket_other", b'[', &b"\x1bX Z"[..], Some("abcX ZX")),
        ("letter_o_ss3", b'O', &b"\x1bOA"[..], Some("abcX")),
        ("letter_o_csi", b'O', &b"\x1b[A"[..], Some("abc[AX")),
        ("letter_o_other", b'O', &b"\x1bX Z"[..], Some("abcX ZX")),
    ] {
        let inputrc = format!("\"{}\": skip-csi-sequence\n", intro as char);
        let mut keys = b"abc".to_vec();
        keys.push(intro);
        keys.extend_from_slice(tail);
        keys.extend_from_slice(b"X\r");
        let bash = run_bash_readline_with_inputrc_file(&keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, &inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            expected,
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            expected,
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_explicit_skip_ends_at_nested_escape() {
    for (name, intro, tail, expected) in [
        ("csi_then_csi", b"\x1b[", &b"\x1b[A"[..], Some("abc[AX")),
        ("csi_then_ss3", b"\x1b[", &b"\x1bOA"[..], Some("abcOAX")),
        ("csi_then_other", b"\x1b[", &b"\x1bX Z"[..], Some("abcX ZX")),
        ("ss3_then_csi", b"\x1bO", &b"\x1b[A"[..], Some("abc[AX")),
        ("ss3_then_ss3", b"\x1bO", &b"\x1bOA"[..], Some("abcOAX")),
    ] {
        let inputrc = format!(
            "\"{}\": skip-csi-sequence\n",
            if intro[1] == b'[' { "\\e[" } else { "\\eO" }
        );
        let mut keys = b"abc".to_vec();
        keys.extend_from_slice(intro);
        keys.extend_from_slice(tail);
        keys.extend_from_slice(b"X\r");
        let bash = run_bash_readline_with_inputrc_file(&keys, &inputrc);
        let sushline = run_sushline_harness_with_inputrc(&keys, &inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            expected,
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            expected,
            "Sushline {name}: {sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "Sushline {name}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_arrow_prefix_lowercase_tails() {
    let cases: [(&str, &[u8], &[u8], &str); 8] = [
        ("[a at end", b"[", b"a!", "xyz!"),
        ("[a in middle", b"[", b"a!", "xy!z"),
        ("[d at end", b"[", b"d!", "xy!z"),
        ("Oa at end", b"O", b"a!", "xyz!"),
        ("Od at end", b"O", b"d!", "xy!z"),
        ("[d counted", b"[", b"d!", "ab!cd"),
        ("[a counted", b"[", b"a!", "abcd!"),
        ("[A counted", b"[", b"A!", "abcd!"),
    ];
    for (name, intro, tail, expected) in cases {
        let inputrc = format!(
            "\"{}\": arrow-key-prefix\n",
            if intro[0] == b'[' { "[" } else { "O" }
        );
        let mut keys = if name.contains("counted") {
            b"abcd".to_vec()
        } else {
            b"xyz".to_vec()
        };
        if name.contains("counted") {
            keys.extend_from_slice(b"\x1b2");
        }
        if name.contains("in middle") {
            keys.push(2);
        }
        keys.extend_from_slice(intro);
        keys.extend_from_slice(tail);
        keys.push(b'\r');
        let bash = run_bash_readline_with_inputrc_file(&keys, &inputrc);
        let sush = run_sushline_harness_with_inputrc(&keys, &inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "Bash {name}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "Bash {name}: {bash}");
        assert_eq!(
            accepted_line(&sush).as_deref(),
            Some(expected),
            "Sushline {name}: {sush}"
        );
        assert_eq!(bell_count(&sush), 0, "Sushline {name}: {sush}");
    }
}

#[test]
fn bash_readline_and_sushline_lowercase_arrow_prefix_with_history() {
    let history: &[&str] = &["one", "two"];
    for (intro, inputrc) in [
        ("[", "\"[\": arrow-key-prefix\n"),
        ("O", "\"O\": arrow-key-prefix\n"),
    ] {
        let intro = intro.as_bytes()[0];
        for (tail, expected) in [(b'a', "two!"), (b'b', "xyz!"), (b'c', "xyz!")] {
            let mut keys = b"xyz".to_vec();
            if tail == b'b' {
                keys.extend_from_slice(&[intro, b'A']);
            } else {
                keys.push(2);
            }
            keys.extend_from_slice(&[intro, tail]);
            keys.extend_from_slice(b"!\r");
            let bash = run_bash_readline_with_inputrc_file_and_history(&keys, inputrc, history);
            let sush = run_sushline_harness_with_inputrc_and_history(&keys, inputrc, history);
            assert_eq!(
                accepted_line(&bash).as_deref(),
                Some(expected),
                "Bash {intro:?} tail {tail:?}: {bash}"
            );
            assert_eq!(bell_count(&bash), 0, "Bash {intro:?} tail {tail:?}: {bash}");
            assert_eq!(
                accepted_line(&sush).as_deref(),
                Some(expected),
                "Sushline {intro:?} tail {tail:?}: {sush}"
            );
            assert_eq!(
                bell_count(&sush),
                0,
                "Sushline {intro:?} tail {tail:?}: {sush}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_delete_on_empty_line_does_not_signal_eof() {
    // GNU Readline ties end-of-file to the terminal EOF character (Ctrl-D by
    // default), not to the `delete-char` command. A Delete key or any other
    // key bound to `delete-char` rings once on an empty line and keeps
    // reading; only the EOF character signals EOF. Bash 5.3 PTY observation,
    // not GNU source proof, and independent of `bind-tty-special-chars`.
    for (desc, inputrc, keys, accepted, bells, eof) in [
        (
            "delete_key",
            "\"\\e[3~\": delete-char\n",
            b"\x1b[3~X\r".as_slice(),
            Some("X"),
            1,
            false,
        ),
        (
            "delete_key_tty_off",
            "set bind-tty-special-chars off\n\"\\e[3~\": delete-char\n",
            b"\x1b[3~X\r".as_slice(),
            Some("X"),
            1,
            false,
        ),
        (
            "custom_delete_char",
            "\"X\": delete-char\n",
            b"XY\r".as_slice(),
            Some("Y"),
            1,
            false,
        ),
        ("ctrl_d", "", b"\x04\r".as_slice(), None, 0, true),
        (
            "ctrl_d_tty_off",
            "set bind-tty-special-chars off\n",
            b"\x04\r".as_slice(),
            None,
            0,
            true,
        ),
    ] {
        let bash = run_bash_readline_eof_with_inputrc(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        for (side, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                output.contains("SUSHLINE_EOF"),
                eof,
                "{desc} {side}: {output}"
            );
            assert_eq!(
                accepted_line(output),
                accepted.map(str::to_string),
                "{desc} {side}: {output}"
            );
            assert_eq!(bell_count(output), bells, "{desc} {side}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_end_of_file_command_signals_eof_only_for_eof_key() {
    // GNU Readline's `end-of-file` command signals EOF only when the terminal
    // EOF byte is invoked on an empty line; otherwise it rings once and does
    // not edit the line. Bash 5.3 PTY observation, not GNU source proof, and
    // limited to the bindings below (no numeric-argument cases).
    for (desc, inputrc, keys, accepted, bells, eof) in [
        (
            "eof_key_empty",
            "\"X\": end-of-file\n",
            b"XY\r".as_slice(),
            Some("Y"),
            1,
            false,
        ),
        (
            "eof_key_eol",
            "\"X\": end-of-file\n",
            b"abcX\r".as_slice(),
            Some("abc"),
            1,
            false,
        ),
        (
            "eof_key_interior",
            "\"X\": end-of-file\n",
            b"abc\x1b[DX\r".as_slice(),
            Some("abc"),
            1,
            false,
        ),
        (
            "ctrl_d_empty",
            "\"\\C-d\": end-of-file\n",
            b"\x04\r".as_slice(),
            None,
            0,
            true,
        ),
        (
            "ctrl_d_eol",
            "\"\\C-d\": end-of-file\n",
            b"abc\x04\r".as_slice(),
            Some("abc"),
            1,
            false,
        ),
        (
            "ctrl_d_interior",
            "\"\\C-d\": end-of-file\n",
            b"abc\x1b[D\x04\r".as_slice(),
            Some("abc"),
            1,
            false,
        ),
    ] {
        let bash = run_bash_readline_eof_with_inputrc(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        for (side, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                output.contains("SUSHLINE_EOF"),
                eof,
                "{desc} {side}: {output}"
            );
            assert_eq!(
                accepted_line(output),
                accepted.map(str::to_string),
                "{desc} {side}: {output}"
            );
            assert_eq!(bell_count(output), bells, "{desc} {side}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_vi_backward_word_stops_at_punctuation_run() {
    // GNU Readline's vi `b` treats a contiguous punctuation run as its own
    // word class: from inside or just after the run it lands at the run's
    // start, and only the next `b` reaches the preceding word. Bash 5.3 PTY
    // observation, not GNU source proof, and limited to these fixtures.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected) in [
        (
            "after_run",
            b"one--two three\x1b0lllllbiX\r".as_slice(),
            "oneX--two three",
        ),
        (
            "run_end",
            b"one--two three\x1b0llllbiX\r".as_slice(),
            "oneX--two three",
        ),
        (
            "run_start",
            b"one--two three\x1b0lllbiX\r".as_slice(),
            "Xone--two three",
        ),
        (
            "at_space",
            b"one--two three\x1b0llllllllbiX\r".as_slice(),
            "one--Xtwo three",
        ),
        (
            "next_word",
            b"one--two three\x1b0lllllllllbiX\r".as_slice(),
            "one--Xtwo three",
        ),
    ] {
        let bash = run_bash_readline_with_bindings(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_end_word_stops_at_punctuation_run() {
    // GNU Readline's vi `e` keeps a contiguous punctuation run as its own
    // word class: from inside a run it stops at the run's end, and from the
    // end of a run it advances to the next run's end across any whitespace.
    // This is Bash 5.3 PTY evidence for these fixtures, not GNU source proof.
    let inputrc = "set editing-mode vi";
    for (desc, left, motion, expected) in [
        ("word_interior", 0, "eiX", "onXe--two three"),
        ("word_end", 2, "eiX", "one-X-two three"),
        ("run_start", 3, "eiX", "one-X-two three"),
        ("run_end", 4, "eiX", "one--twXo three"),
        ("across_space", 7, "eiX", "one--two threXe"),
        ("change_run", 2, "ceX", "onXtwo three"),
        ("change_after_run", 4, "ceX", "one-X three"),
        ("big_end_unchanged", 2, "EiX", "one--twXo three"),
    ] {
        let keys = format!("one--two three\x1b0{}{}\r", "l".repeat(left), motion);
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_delete_to_end_keeps_point_on_last_character() {
    // GNU backs the cursor up after a delete reaches the end of the line.
    // The inserted X shows the point: these counts start inside a word or
    // punctuation run and delete across the rest of the line.
    let inputrc = "set editing-mode vi";
    for (desc, line, left, motion, expected) in [
        ("dollar", "one two three", 3, "d$iX", "onXe"),
        ("D", "one two three", 3, "DiX", "onXe"),
        ("last_word", "one two three", 8, "dwiX", "one twoX "),
        ("two_words", "one two three", 5, "d2wiX", "one Xt"),
        ("punctuation", "one--two three", 5, "d2wiX", "one-X-"),
        ("punctuation_late", "one--two three", 6, "d2wiX", "one--Xt"),
        ("three_words", "one--two three", 3, "d3wiX", "onXe"),
        ("x", "abc", 2, "xiX", "aXb"),
        ("two_x", "abc", 2, "2xiX", "aXb"),
        ("three_x", "abcd", 3, "3xiX", "abXc"),
        ("x_multibyte", "aéb", 2, "xiX", "aXé"),
    ] {
        let keys = format!("{line}\x1b0{}{motion}\r", "l".repeat(left));
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_delete_stores_text_for_put() {
    // GNU puts text removed by `x` or a counted `x`; without a stored
    // register a following `p`/`P` leaves the line shortened. Bash 5.3 PTY
    // observations for these counted and placement cases.
    let inputrc = "set editing-mode vi";
    for (desc, line, left, keys, expected) in [
        ("put_after", "abc", 2, "xp", "abc"),
        ("put_before", "abc", 2, "xP", "acb"),
        ("counted_from_start", "abc", 0, "2xp", "cab"),
        ("count_overrun", "abc", 2, "3xp", "abc"),
    ] {
        let keys = format!("{line}\x1b0{}{keys}\r", "l".repeat(left));
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_change_word_matches_punctuation_runs() {
    // GNU Readline's vi `cw` changes one word-class run, treating a
    // punctuation run as its own class and preserving the whitespace after a
    // word; `dw` crosses that whitespace. These are Bash 5.3 PTY observations
    // for this line and these counts, not a general vi word-class rule.
    let inputrc = "set editing-mode vi";
    for (desc, left, motion, expected) in [
        ("change_run_start", 3, "cwX", "oneXtwo three"),
        ("change_run_end", 4, "cwX", "one-Xtwo three"),
        ("change_word_space", 5, "cwX", "one--X three"),
        ("change_word_end", 7, "cwX", "one--twX three"),
        ("change_whitespace", 8, "cwX", "one--twoXthree"),
        ("delete_run_start", 3, "dwiX", "oneXtwo three"),
        ("delete_run_end", 4, "dwiX", "one-Xtwo three"),
        ("delete_word_space", 5, "dwiX", "one--Xthree"),
        ("change_twice_word", 0, "c2wX", "Xtwo three"),
        ("change_twice_run", 2, "c2wX", "onXtwo three"),
        ("change_twice_space", 5, "c2wX", "one--X"),
        ("delete_twice", 3, "d2wiX", "oneXthree"),
        ("change_bigword", 0, "cWX", "X three"),
        ("delete_bigword", 0, "dWiX", "Xthree"),
    ] {
        let keys = format!("one--two three\x1b0{}{}\r", "l".repeat(left), motion);
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_backward_word_follows_gnu_boundaries() {
    // GNU Readline's `rl_vi_bword` algorithm stops at the boundary
    // between an identifier run and a punctuation run, but backtracks over a
    // short run when the next class boundary belongs to a one-character
    // word. Bash 5.3 PTY observations for these line shapes and counts.
    let inputrc = "set editing-mode vi";
    for (desc, text, left, motion, expected) in [
        ("one_char_word", "a-b", 2, "b", "@a-b"),
        ("one_char_word_long_run", "a--b", 3, "b", "@a--b"),
        ("long_word", "aa-b", 3, "b", "aa@-b"),
        ("long_word_long_run", "aa--b", 4, "b", "aa@--b"),
        ("one_char_two_letter_target", "a-bb", 2, "b", "@a-bb"),
        ("inside_target", "a-bb", 3, "b", "a-@bb"),
        ("long_words", "one--two", 5, "b", "one@--two"),
        ("across_space", "a b", 2, "b", "@a b"),
        ("previous_one_char_word", "a b c", 4, "b", "a @b c"),
        ("count_two", "one--two three", 5, "2b", "@one--two three"),
        ("one_char_eol", "one--b", 6, "b", "one@--b"),
        ("one_char_after_space_eol", "abc d", 5, "b", "@abc d"),
        ("two_char_eol", "one-bb", 6, "b", "one-@bb"),
        ("one_char_only", "b", 1, "b", "@b"),
    ] {
        let keys = format!("{text}\x1b0{}{motion}i@\r", "l".repeat(left));
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_end_word_from_whitespace_follows_gnu() {
    // GNU's `rl_vi_eword` skips a whitespace gap and then advances to the
    // end of the following class run; Bash 5.3 PTY observations for these
    // gaps, counts and operators.
    let inputrc = "set editing-mode vi";
    for (desc, text, left, motion, expected) in [
        ("space", "ab cd", 2, "eiX", "ab cXd"),
        ("space_run", "ab   cd", 2, "eiX", "ab   cXd"),
        ("counted", "ab cd ef", 2, "2eiX", "ab cd eXf"),
        ("delete", "ab cd", 2, "de", "ab"),
        ("change", "one !!!two", 4, "ceX", "one Xtwo"),
        ("punctuation", "one--two three", 8, "eiX", "one--two threXe"),
    ] {
        let keys = format!("{text}\x1b0{}{motion}\r", "l".repeat(left));
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_change_word_follows_gnu_whitespace_boundaries() {
    // GNU `cw` runs `rl_vi_fword` and then backs the end point up to the end
    // of the current run, so whitespace under the cursor changes one
    // character. Bash 5.3 PTY observations for these gaps and counts,
    // including trailing whitespace where GNU temporarily appends a blank.
    let inputrc = "set editing-mode vi";
    for (desc, text, left, motion, expected) in [
        ("interior_run", "one  two", 3, "cwX", "oneX two"),
        ("interior_run_middle", "one  two", 4, "cwX", "one Xtwo"),
        ("leading_run", "  leading", 0, "cwX", "X leading"),
        ("longer_run", "one   two", 3, "cwX", "oneX  two"),
        ("longer_run_middle", "one   two", 4, "cwX", "one X two"),
        ("last_trailing_space", "one ", 2, "cwX", "onX"),
        ("at_end_after_space", "one ", 3, "cwX", "oneX"),
        ("last_of_trailing_run", "one  ", 4, "cwX", "one X"),
        ("first_of_trailing_run", "one  ", 3, "cwX", "oneX"),
        (
            "counted_from_space",
            "one  two three",
            3,
            "c2wX",
            "oneX three",
        ),
        ("counted_from_leading", "  leading two", 0, "c2wX", "X two"),
        ("word_at_end", "one", 2, "cwX", "onX"),
    ] {
        let keys = format!("{text}\x1b0{}{motion}\r", "l".repeat(left));
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_backward_word_from_end_of_line() {
    // GNU `rl_vi_bword` reads the NUL at end of line as non-whitespace, so
    // `b` from the end crosses trailing whitespace to the previous word.
    // Bash 5.3 PTY observations for these trailing gaps.
    let inputrc = "set editing-mode vi";
    for (desc, text, left, motion, expected) in [
        ("one_space", "ab ", 3, "biX", "Xab "),
        ("two_spaces", "ab  ", 4, "biX", "Xab  "),
        (
            "long_line",
            "one--two three ",
            15,
            "biX",
            "one--two Xthree ",
        ),
        (
            "long_line_two_spaces",
            "one--two three  ",
            16,
            "biX",
            "one--two Xthree  ",
        ),
        ("no_trailing_space", "ab", 2, "biX", "Xab"),
        (
            "punctuation_line",
            "one--two three",
            14,
            "biX",
            "one--two Xthree",
        ),
        ("delete_from_end", "ab ", 3, "bdwiX", "X"),
    ] {
        let keys = format!("{text}\x1b0{}{motion}\r", "l".repeat(left));
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_custom_arrow_key_prefix_bindings() {
    // GNU selects the arrow keymap from the command, not from whether the
    // binding ends in `[` or `O`. Bash 5.3 PTY observations for a single-byte
    // custom prefix and for a bare ESC binding, with bound and unbound tails.
    let custom = "\"x\": arrow-key-prefix\n";
    let escape = "\"\\e\": arrow-key-prefix\n";
    let ctrl_o = "\"\\C-o\": arrow-key-prefix\n";
    for (desc, inputrc, keys, expected, bells) in [
        ("backward", custom, &b"abcxDX\r"[..], Some("abXc"), 0),
        ("lowercase_tail", custom, &b"abcxDx\r"[..], None, 1),
        ("forward", custom, &b"abcxCX\r"[..], Some("abcX"), 1),
        ("unbound_tail", custom, &b"abcxHX\r"[..], Some("abcX"), 1),
        (
            "escape_csi",
            escape,
            &b"abc\x1b[D X\r"[..],
            Some("ab Xc"),
            0,
        ),
        (
            "escape_ss3",
            escape,
            &b"abc\x1bOD X\r"[..],
            Some("ab Xc"),
            0,
        ),
        (
            "ctrl_o_csi",
            ctrl_o,
            &b"abc\x0f\x1b[1;5CX\r"[..],
            Some("abc[1;5CX"),
            1,
        ),
        (
            "ctrl_o_backward",
            ctrl_o,
            &b"abc\x0fDX\r"[..],
            Some("abXc"),
            0,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            expected,
            "Bash {desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "Bash {desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "Sushline {desc}: {sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "Sushline {desc}: {sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_undo_ignores_text_before_first_command_mode() {
    // GNU frees the undo list on the first ESC into vi command mode, so `u`
    // leaves text typed before that ESC alone. Later edits remain undoable.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("typed", "abc\x1b0llu\r", "abc", 1),
        ("typed_then_put", "abc\x1b0llu\x1bp\r", "abc", 2),
        ("repeated", "abc\x1b0llu\x1bu\r", "abc", 2),
        ("later_insert", "abc\x1b0iXYZ\x1bu\r", "abc", 0),
        ("after_delete", "abc\x1b0llx\x1bu\r", "abc", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_and_sushline_vi_change_undo_restores_original_line() {
    // GNU keeps a change operator's deletion and the inserted replacement in
    // one undo entry, so one `u` restores the original line. `r` is a control:
    // it replaces without deleting a range and already had one undo entry.
    let inputrc = "set editing-mode vi";
    for (desc, motion) in [
        ("change_word", "cwX"),
        ("change_two_chars", "cwXY"),
        ("counted_change_word", "2cwX"),
        ("change_line", "ccX"),
        ("change_to_end", "Cx"),
        ("substitute", "sX"),
        ("replace_char", "rx"),
    ] {
        let keys = format!("abc\x1b0ll{motion}\x1bu\r");
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some("abc".to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "{desc}: {sushline}");
    }
}

#[test]
fn bash_and_sushline_vi_uppercase_u_undoes_the_whole_undo_list() {
    // GNU binds U to revert-line, but in vi mode that command undoes every
    // undo entry, rings on an empty list and leaves point at byte zero. The
    // emacs revert-line behavior stays unchanged.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("after_insert", "abc\x1biX\x1bU\r", "abc", 0),
        ("two_inserts", "abc\x1biX\x1biY\x1bU\r", "abc", 0),
        ("after_delete", "abc\x1b0llx\x1bU\r", "abc", 0),
        ("after_change", "abc\x1b0llcwX\x1bU\r", "abc", 0),
        ("empty_after_undo", "abc\x1b0llcwX\x1bu\x1bU\r", "abc", 1),
        ("empty_on_fresh_line", "abc\x1bU\r", "abc", 1),
        ("point_at_start", "abc\x1b0llx\x1bUiZ\x1b\r", "Zabc", 0),
        ("empty_keeps_point_mid_line", "abc\x1bUiZ\r", "abZc", 1),
        (
            "empty_keeps_point_after_motion",
            "abcdef\x1b0lllUiZ\r",
            "abcZdef",
            1,
        ),
        ("two_empty_keep_point", "abc\x1bUUiZ\r", "abZc", 2),
        ("two_empty_commands", "abc\x1bU\x1bu\r", "abc", 2),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
fn bash_and_sushline_vi_redo_initial_insert() {
    // GNU saves the text typed before the first ESC as the vi redo change.
    // It is replayed at the current point even when a failed `p`/`P` in
    // between did not establish an explicit change.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("at_end_once", "abc\x1b.\r", "ababcc", 0),
        ("at_end_twice", "abc\x1b..\r", "abababccc", 0),
        ("at_start", "abc\x1b0.\r", "abcabc", 0),
        ("after_motion", "abc\x1b0ll..\r", "abababccc", 0),
        ("failed_put", "abc\x1b0llp.\r", "abcabc", 1),
        ("failed_put_before", "abc\x1b0llP.\r", "ababcc", 1),
        (
            "explicit_insert_overrides",
            "abc\x1biZ\x1b..\r",
            "abZZZc",
            0,
        ),
        ("last_multibyte", "a\u{e9}\x1b.\r", "a\u{e9}\u{e9}", 0),
        (
            "two_multibyte",
            "\u{65e5}\u{672c}\x1b.\r",
            "\u{65e5}\u{672c}\u{672c}",
            0,
        ),
        ("mixed_width", "h\u{e9}llo\x1b.\r", "h\u{e9}ll\u{e9}lloo", 0),
        (
            "mixed_width_tail",
            "ab\u{e9}cd\x1b.\r",
            "ab\u{e9}c\u{e9}cdd",
            0,
        ),
        ("backspace_breaks_initial", "abcd\x08\x1b.\r", "abc", 0),
        ("word_kill_breaks_initial", "abc def\x17\x1b.\r", "abc ", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
fn bash_and_sushline_vi_undo_honors_count() {
    // GNU vi-undo repeats up to the typed count and rings once when it runs
    // out of undo entries. A zero or absent count undoes one entry.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("uncounted", "abc\x1biX\x1biY\x1b0u\r", "abXc", 0),
        ("count_two", "abc\x1biX\x1biY\x1b02u\r", "abc", 0),
        ("count_overrun", "abc\x1biX\x1biY\x1b03u\r", "abc", 1),
        ("then_count_two", "abc\x1biX\x1biY\x1b0u2u\r", "abc", 1),
        ("zero_count", "abc\x1biX\x1biY\x1b00u\r", "abXc", 0),
        ("single_overrun", "abc\x1biX\x1b02u\r", "abc", 1),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
fn bash_and_sushline_vi_redo_initial_insert_per_read() {
    // GNU refreshes the initial-insert redo change on each read when the
    // persisted redo command was an insertion. A persisted operator such as
    // `x` stays the redo command instead.
    let inputrc = "set editing-mode vi";
    for (desc, keys, first, second, bells) in [
        ("no_prior_change", "abc\x1b\rxyz\x1b.\r", "abc", "xyxyzz", 0),
        (
            "prior_insert",
            "abc\x1biX\x1b\rdef\x1b.\r",
            "abXc",
            "dedeff",
            0,
        ),
        ("prior_operator", "abc\x1b0x\rdef\x1b.\r", "bc", "de", 0),
        (
            "prior_insert_empty_read",
            "abc\x1biX\x1b\r\x1b.\r",
            "abXc",
            "",
            0,
        ),
        (
            "prior_insert_backspace",
            "abc\x1biX\x1b\rdef\x08\x1b.\r",
            "abXc",
            "de",
            0,
        ),
        (
            "prior_insert_word_kill",
            "abc\x1biX\x1b\rdef \x17\x1b.\r",
            "abXc",
            "",
            0,
        ),
        (
            "synthesized_insert_empty_read",
            "abc\x1b\r\x1b.\r",
            "abc",
            "abc",
            0,
        ),
        (
            "synthesized_insert_backspace",
            "abc\x1b\rdef\x08\x1b.\r",
            "abc",
            "de",
            0,
        ),
        (
            "prior_append_empty_read",
            "abc\x1bA X \x1b\r\x1b.\r",
            "abc X ",
            "",
            0,
        ),
    ] {
        let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(
            keys.as_bytes(),
            inputrc,
            &[],
        );
        let sushline =
            run_sushline_harness_two_reads_with_inputrc_and_history(keys.as_bytes(), inputrc, &[]);
        assert_eq!(
            accepted_numbered_line(&bash, 1),
            Some(first.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_numbered_line(&bash, 2),
            Some(second.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_numbered_line(&sushline, 1),
            accepted_numbered_line(&bash, 1),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            accepted_numbered_line(&sushline, 2),
            accepted_numbered_line(&bash, 2),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_and_sushline_vi_delete_rings_on_empty_line() {
    // A `vi-delete` with no character to remove rings once, both when typed
    // directly and when replayed as a persisted `vi-redo` change. Other
    // operators that do nothing on an empty line stay silent.
    let inputrc = "set editing-mode vi";
    for (desc, keys, first, second, bells) in [
        ("empty_line", "\x1bx\r", "", None, 1),
        ("zero_count", "\x1b00x\r", "", None, 1),
        ("large_count", "\x1b10x\r", "", None, 1),
        ("after_emptying_line", "abc\x1b0xxx.\r", "", None, 1),
        (
            "redo_after_empty_read",
            "abc\x1b0x\r\x1b.\r",
            "bc",
            Some(""),
            1,
        ),
        (
            "counted_redo_after_empty_read",
            "abc\x1b0x\r\x1b2.\r",
            "bc",
            Some(""),
            1,
        ),
        (
            "two_redos_after_empty_read",
            "abc\x1b0x\r\x1b..\r",
            "bc",
            Some(""),
            2,
        ),
        (
            "redo_on_nonempty_read",
            "abc\x1b0x\rdef\x1b.\r",
            "bc",
            Some("de"),
            0,
        ),
        ("redo_of_delete_line", "abc\x1bdd\r\x1b.\r", "", Some(""), 0),
        ("redo_of_delete_to", "abc\x1bD\r\x1b.\r", "ab", Some(""), 0),
        (
            "redo_of_insert",
            "abc\x1biX\x1b\r\x1b.\r",
            "abXc",
            Some(""),
            0,
        ),
        ("no_redo", "\x1b.\r", "", None, 0),
    ] {
        let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(
            keys.as_bytes(),
            inputrc,
            &[],
        );
        let sushline =
            run_sushline_harness_two_reads_with_inputrc_and_history(keys.as_bytes(), inputrc, &[]);
        let expected_second = second.map(str::to_string);
        assert_eq!(
            accepted_numbered_line(&bash, 1),
            Some(first.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_numbered_line(&bash, 2),
            expected_second,
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_numbered_line(&sushline, 1),
            accepted_numbered_line(&bash, 1),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            accepted_numbered_line(&sushline, 2),
            accepted_numbered_line(&bash, 2),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_change_case_counts_keep_point_on_final_character() {
    // GNU repeats a counted `~` on the final character when point cannot
    // advance, so even counts restore it and odd counts change it. Point must
    // also stay on that character for a following `x`.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("once_at_start", "abc\x1b0~\r", "Abc", 0),
        ("counted_from_start", "abc\x1b02~\r", "ABc", 0),
        ("counted_mid_line", "abc\x1b0l2~\r", "aBC", 0),
        ("final_twice", "abc\x1b0ll2~\r", "abc", 0),
        ("final_thrice", "abc\x1b0ll3~\r", "abC", 0),
        ("final_four_times", "abc\x1b0ll4~\r", "abc", 0),
        ("final_five_times", "abc\x1b0ll5~\r", "abC", 0),
        ("initial_point_count_two", "abc\x1b2~\r", "abc", 0),
        ("initial_point_count_three", "abc\x1b3~\r", "abC", 0),
        ("final_then_delete", "abc\x1b0ll~x\r", "ab", 0),
        ("multibyte_counted", "a\u{e9}llo\x1b0~~~\r", "A\u{c9}Llo", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_forward_char_stays_on_last_character() {
    // In vi command mode, forward-char cannot leave point past the final
    // character. Starting there rings once; a counted motion that only runs
    // out partway is silent. Space and right-arrow share the binding.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("one_short", "abc\x1b0ll\r", "abc", 0),
        ("past_end", "abc\x1b0lll\r", "abc", 1),
        ("twice_past_end", "abc\x1b0llll\r", "abc", 2),
        ("count_reaches_end", "abc\x1b03l\r", "abc", 0),
        ("count_exceeds_end", "abc\x1b05l\r", "abc", 0),
        ("count_then_single", "abc\x1b02ll\r", "abc", 1),
        ("count_from_end", "abc\x1b2l\r", "abc", 1),
        ("multibyte_past_end", "a\u{e9}\x1b0lll\r", "a\u{e9}", 2),
        ("multibyte_then_delete", "a\u{e9}\x1b0llx\r", "a", 1),
        ("space_past_end", "abc\x1b0   \r", "abc", 1),
        ("arrow_past_end", "abc\x1b0\x1b[C\x1b[C\x1b[C\r", "abc", 1),
        ("backward_at_start", "abc\x1b0h\r", "abc", 1),
        ("empty_line", "\x1bl\r", "", 1),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_dollar_keeps_point_on_last_character() {
    // GNU leaves vi command-mode point on the final character after `$`, so
    // the next character operator acts on it. End-of-line operators keep their
    // inclusive behavior; insert-mode append after `$` still starts at end.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("change_case", "abc\x1b$~\r", "abC", 0),
        ("delete", "abc\x1b$x\r", "ab", 0),
        ("change_case_then_delete", "abc\x1b$~x\r", "ab", 0),
        ("counted_change_case", "abc\x1b$2~\r", "abc", 0),
        ("replace", "abc\x1b$rX\r", "abX", 0),
        ("forward_at_end", "abc\x1b$l\r", "abc", 1),
        ("forward_then_delete", "abc\x1b$lx\r", "ab", 1),
        ("multibyte_delete", "a\u{e9}\x1b$x\r", "a", 0),
        ("empty_line_delete", "\x1b$x\r", "", 1),
        ("backward", "abc\x1b$h\r", "abc", 0),
        ("delete_to_end", "abc def\x1b0llD\r", "ab", 0),
        ("change_to_end", "abc def\x1b0llCXY\x1b\r", "abXY", 0),
        ("yank_to_end", "abc def\x1b0llY$p\r", "abc defc def", 0),
        (
            "dollar_motion_delete",
            "abc def\x1b0ll$\x1bx\r",
            "abc de",
            0,
        ),
        (
            "dollar_motion_change",
            "abc def\x1b0llc$XY\x1b\r",
            "abXY",
            0,
        ),
        (
            "dollar_motion_yank",
            "abc def\x1b0lly$p\r",
            "abcc def def",
            0,
        ),
        ("append_after_dollar", "abc\x1b$A X\r", "abc X", 0),
        ("counted_dollar", "abc def\x1b2$\r", "abc def", 0),
        ("single_character", "a\x1b$x\r", "", 0),
        ("empty_line", "\x1b$\r", "", 0),
        ("redo_after_insert", "abc\x1b0iX\x1b$.\r", "XabXc", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_forward_char_operator_reaches_line_end() {
    // A pending operator needs `l` to reach the end-of-line position, while
    // plain `l` alone stays on the final character.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("delete_last", "abc\x1b0lldl\r", "ab", 0),
        ("change_last", "abc\x1b0llclZ\r", "abZ", 0),
        ("yank_last", "abc\x1b0llylp\r", "abcc", 0),
        ("delete_only_character", "a\x1b0dl\r", "", 0),
        ("delete_count_to_end", "abc\x1b0d3l\r", "", 0),
        ("delete_count_past_end", "abc\x1b0d5l\r", "", 0),
        ("counted_from_middle", "abc\x1b0ld2l\r", "a", 0),
        ("multibyte_delete_last", "a\u{e9}\x1b0ldl\r", "a", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_backward_delete_honors_count_and_rings() {
    // GNU X deletes up to count characters before point and rings when the
    // count cannot be satisfied, including at column zero and on empty lines.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("at_column_zero", "abc\x1b0X\r", "abc", 1),
        ("twice_at_column_zero", "abc\x1b0XX\r", "abc", 2),
        ("zero_count", "abc\x1b00X\r", "abc", 1),
        ("count_exceeds_available", "abc\x1b0l3X\r", "bc", 1),
        ("exact_count", "abc\x1b0ll2X\r", "c", 0),
        ("count_one_past_available", "abc\x1b0ll3X\r", "c", 1),
        ("at_end", "abc\x1bX\r", "ac", 0),
        ("empty_line", "\x1bX\r", "", 1),
        ("counted_from_end", "abc def\x1b$5X\r", "af", 0),
        ("multibyte_exact", "a\u{e9}\x1b$2X\r", "\u{e9}", 1),
        ("multibyte_overshoot", "a\u{e9}\u{e9}\x1b$3X\r", "\u{e9}", 1),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_backward_delete_redo_replays_count() {
    // GNU records X (and its count) as the redo command even when it rings.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("no_op_then_redo", "abc\x1b0X.\r", "abc", 2),
        ("after_delete_then_redo", "abc\x1b0xX.\r", "bc", 2),
        ("after_replace_then_redo", "abc\x1b0rZX.\r", "Zbc", 2),
        ("no_op_then_replace_redo", "abc\x1b0XrZ.\r", "Zbc", 1),
        ("counted_redo", "abc def\x1b$2X.\r", "abf", 0),
        ("counted_redo_twice", "abc def\x1b$2X..\r", "f", 0),
        ("undo_then_redo", "abc def\x1b$2Xu.\r", "abc f", 0),
        ("redo_after_motion", "abc def\x1b$2X0.\r", "abc f", 1),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[cfg(unix)]
#[test]
fn bash_and_sushline_c_locale_vi_replace_pins_raw_bytes_and_bells() {
    // C/POSIX bytewise mode consumes one replacement byte; UTF-8 mode consumes
    // the whole character. Assert raw accepted bytes, not lossy strings.
    use portable_pty::{CommandBuilder, PtySize};
    use std::fs;

    require_oracle_host();
    let root = tempfile::tempdir().expect("tempdir");
    let home = tempfile::tempdir().expect("home");
    let home_path = home.path().to_string_lossy().into_owned();
    let inputrc_path = root.path().join("inputrc");
    let inputrc =
        "set editing-mode vi\nset input-meta on\nset output-meta on\nset convert-meta off\n";
    let script = r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "SUSHLINE_READY>" line; printf 'SUSHLINE_ACCEPTED_HEX:'; printf '%s' "$line" | od -An -tx1 | tr -d ' \n'; printf '\nSUSHLINE_DONE\n'"#;
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    let run_bash = |keys: &[u8], locale: &str| {
        fs::write(&inputrc_path, inputrc).expect("write inputrc");
        let mut command = CommandBuilder::new("bash");
        command.env("INPUTRC", inputrc_path.to_string_lossy().as_ref());
        command.env("LC_ALL", locale);
        command.env("HOME", &home_path);
        command.args(["--noprofile", "--norc", "-i", "-c", script]);
        run_pty_in_dir_with_size_raw_until(
            command,
            keys,
            root.path(),
            size,
            "SUSHLINE_ACCEPTED_HEX:",
        )
    };
    let run_sushline = |keys: &[u8], locale: &str| {
        let mut command = sushline_harness_command();
        command.env("SUSHLINE_INPUTRC", inputrc);
        command.env("SUSHLINE_PROMPT", "SUSHLINE_READY>");
        command.env("SUSHLINE_HISTORY", "");
        command.env("SUSHLINE_ACCEPTED_HEX", "1");
        command.env("LC_ALL", locale);
        command.env("HOME", &home_path);
        run_pty_in_dir_with_size_raw_until(
            command,
            keys,
            root.path(),
            size,
            "SUSHLINE_ACCEPTED_HEX:",
        )
    };
    fn accepted_bytes(output: &[u8]) -> Option<Vec<u8>> {
        let marker = b"SUSHLINE_ACCEPTED_HEX:";
        let start = output
            .windows(marker.len())
            .position(|window| window == marker)?
            + marker.len();
        let hex = output[start..]
            .iter()
            .skip_while(|byte| byte.is_ascii_whitespace())
            .take_while(|byte| byte.is_ascii_hexdigit())
            .copied()
            .collect::<Vec<_>>();
        let hex = std::str::from_utf8(&hex).unwrap();
        if !hex.len().is_multiple_of(2) {
            return None;
        }
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
            .collect()
    }
    fn bells(output: &[u8]) -> usize {
        output.iter().filter(|byte| **byte == 7).count()
    }

    for (desc, keys, locale, expected, expected_bells) in [
        (
            "c_replace_one_byte",
            b"abc\x1b0r\xe3\x81\x82\r".as_slice(),
            "C",
            vec![0xe3, b'b', b'c'],
            2,
        ),
        (
            "c_redo_one_byte",
            b"abc\x1b0r\xe3\x81\x82l.\r".as_slice(),
            "C",
            vec![0xe3, 0xe3, b'c'],
            2,
        ),
        (
            "c_ascii_redo",
            b"abc\x1b0rZl.\r".as_slice(),
            "C",
            b"ZZc".to_vec(),
            0,
        ),
        (
            "utf8_replace_whole_char",
            b"abc\x1b0r\xe3\x81\x82\r".as_slice(),
            "C.UTF-8",
            vec![0xe3, 0x81, 0x82, b'b', b'c'],
            0,
        ),
        (
            "utf8_redo_whole_char",
            b"abc\x1b0r\xe3\x81\x82l.\r".as_slice(),
            "C.UTF-8",
            vec![0xe3, 0x81, 0x82, 0xe3, 0x81, 0x82, b'c'],
            0,
        ),
        (
            "utf8_ascii_redo",
            b"abc\x1b0rZl.\r".as_slice(),
            "C.UTF-8",
            b"ZZc".to_vec(),
            0,
        ),
    ] {
        let bash = run_bash(keys, locale);
        let sushline = run_sushline(keys, locale);
        assert_eq!(
            accepted_bytes(&bash),
            Some(expected.clone()),
            "{desc}: bash"
        );
        assert_eq!(bells(&bash), expected_bells, "{desc}: bash");
        assert_eq!(
            accepted_bytes(&sushline),
            accepted_bytes(&bash),
            "{desc}: sushline={sushline:?}"
        );
        assert_eq!(
            bells(&sushline),
            expected_bells,
            "{desc}: sushline={sushline:?}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_operator_motions_distinguish_arrow_from_l() {
    // GNU cancels a pending operator when a right arrow reaches the final
    // character, while `l` and space extend the operator range to it.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("delete_arrow", "abc\x1b0lld\x1b[C\r", "abc", 1),
        ("change_arrow", "abc\x1b0llc\x1b[CZ\r", "abc", 2),
        ("yank_arrow", "abc\x1b0lly\x1b[Cp\r", "abc", 2),
        ("delete_l", "abc\x1b0lldl\r", "ab", 0),
        ("delete_space", "abc\x1b0lld \r", "ab", 0),
        ("plain_arrow", "abc\x1b0ll\x1b[Cx\r", "ab", 1),
        ("delete_ss3_arrow", "abc\x1b0lld\x1bOC\r", "abc", 1),
        ("delete_arrow_from_start", "abc\x1b0d\x1b[C\r", "abc", 0),
        (
            "delete_arrow_long_line",
            "abcdefghi\x1b0ld\x1b[C\r",
            "abcdefghi",
            0,
        ),
        (
            "delete_arrow_after_blocked_l",
            "abc\x1b0hld\x1b[C\r",
            "abc",
            1,
        ),
        (
            "delete_arrow_count_3",
            "abc def\x1b0d3\x1b[C\r",
            "abc def",
            0,
        ),
        (
            "delete_arrow_count_9",
            "abc def\x1b0d9\x1b[C\r",
            "abc def",
            0,
        ),
        ("change_arrow_mid_line", "abc\x1b0hlc\x1b[CZ\r", "abc", 2),
        ("yank_arrow_mid_line", "abc\x1b0hly\x1b[Cp\r", "abc", 2),
        ("delete_ss3_arrow_from_start", "abc\x1b0d\x1bOC\r", "abc", 0),
        (
            "arrow_moves_point_after_cancel",
            "abc\x1b0ld\x1b[CX\r",
            "ac",
            0,
        ),
        ("operator_space_still_applies", "abc\x1b0hld \r", "ac", 1),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_operators_cancel_on_non_motion_keys() {
    // GNU applies a pending vi operator only to the vi motion bound to the
    // movement command. Arrow/Home/End keys and unbound keys cancel it
    // silently, while h/l/space/0/$ and word motions still apply it. The
    // Ctrl-H case measures cancellation without moving point, unlike h.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("left_from_zero", "abc\x1b0d\x1b[D\r", "abc", 1),
        ("left_from_one", "abc\x1b0ld\x1b[D\r", "abc", 0),
        ("left_ss3", "abc\x1b0ld\x1bOD\r", "abc", 0),
        ("left_count", "abc\x1b0l2d\x1b[D\r", "abc", 0),
        ("left_multibyte", "a\u{e9}\x1bld\x1b[D\r", "a\u{e9}", 1),
        ("change_left", "abc\x1b0lc\x1b[DZ\r", "abc", 1),
        ("yank_left", "abc\x1b0ly\x1b[Dp\r", "abc", 1),
        ("ctrl_b", "abc\x1b0ld\x02\r", "abc", 0),
        ("ctrl_f", "abc\x1b0ld\x06\r", "abc", 0),
        ("ctrl_a", "abc\x1b0ld\x01\r", "abc", 0),
        ("ctrl_e", "abc\x1b0ld\x05\r", "abc", 0),
        ("unbound_printable", "abc\x1b0ldQ\r", "abc", 0),
        ("unbound_csi", "abc\x1b0ld\x1b[Z\r", "abc", 2),
        ("end_key", "abc def\x1b0ld\x1b[F\r", "abc def", 0),
        ("home_key", "abc def\x1bld\x1b[H\r", "abc def", 1),
        ("h", "abc\x1b0ldh\r", "bc", 0),
        ("backspace", "abc\x1b0ld\x08\r", "abc", 0),
        ("l", "abc\x1b0lldl\r", "ab", 0),
        ("space", "abc\x1b0lld \r", "ab", 0),
        ("backward_word", "abc def\x1b$db\r", "abc f", 0),
        ("forward_word", "abc def\x1b$dw\r", "abc de", 0),
        ("end_word", "abc def\x1b0de\r", " def", 0),
        ("first_non_blank", "abc\x1bd^\r", "c", 0),
        ("zero", "abc\x1bd0\r", "c", 0),
        ("dollar", "abc\x1b0d$\r", "", 0),
        ("find", "abc def\x1b0dfd\r", "ef", 0),
        ("find_backward", "abc def\x1b$dFd\r", "abc f", 0),
        ("till", "abc def\x1b0dte\r", "ef", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_operators_skip_non_motion_commands() {
    // GNU drops a pending operator and ignores a bound command that is not a
    // vi motion. Movement keys still move point after cancellation.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("delete_char", "abc\x1b0dx\r", "abc", 0),
        ("backward_delete_char", "abc\x1b0dX\r", "abc", 0),
        ("put", "abc\x1b0dp\r", "abc", 0),
        ("undo", "abc\x1b0du\r", "abc", 0),
        ("change_case", "abc\x1b0ld~\r", "abc", 0),
        ("redo", "abc def\x1b0ld.\r", "abc def", 0),
        ("delete_to_end", "abc def\x1b0ldD\r", "abc def", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_char_search_keeps_esc_prefix() {
    // When ESC is the failed search target, GNU still uses it to start a key
    // sequence, so a following CSI/SS3 arrow moves the cursor.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("forward_csi", "abc def\x1b0f\x1b[Cx\r", "ac def", 1),
        ("forward_ss3", "abc def\x1b0f\x1bOCx\r", "ac def", 1),
        ("forward_csi_only", "abc def\x1b0f\x1b[C\r", "abc def", 1),
        ("backward_csi", "abc def\x1b$f\x1b[Dx\r", "abc df", 1),
        ("till_csi", "abc def\x1b0t\x1b[Cx\r", "ac def", 1),
        ("backward_till_csi", "abc def\x1b$T\x1b[Dx\r", "abc df", 1),
        ("esc_x", "abc def\x1b0f\x1bx\r", "bc def", 1),
        ("esc_bracket", "abc def\x1b0f\x1b[x\r", "bc def", 2),
        ("multibyte_target", "a\u{e9}b\x1b0f\u{e9}x\r", "ab", 0),
        ("failed_search_operator", "abc def\x1b0fqdx\r", "abc def", 1),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_replace_esc_keeps_key_prefix() {
    // An ESC used as the replacement character is ignored, but GNU still
    // treats it as the start of the next key sequence.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("csi_arrow", "abc\x1b0r\x1b[Cx\r", "ac", 0),
        ("ss3_arrow", "abc\x1b0r\x1bOCx\r", "ac", 0),
        ("meta_x", "abc\x1b0r\x1bxx\r", "c", 0),
        ("bare_esc", "abc\x1b0r\x1bx\r", "bc", 0),
        ("ordinary_target", "abc\x1b0rZx\r", "bc", 0),
        ("multibyte_target", "a\u{e9}b\x1b0r\u{e9}x\r", "\u{e9}b", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_csi_word_motions_cancel_operator() {
    // A CSI word-motion key cancels a pending operator at the motion itself;
    // the next command is executed normally instead of being dropped.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        (
            "alt_left_insert",
            "abc def\x1b0ld\x1b[1;3DiZ\x1b\r",
            "Zabc def",
            0,
        ),
        (
            "ctrl_right_insert",
            "abc def\x1b0ld\x1b[1;5CiZ\x1b\r",
            "abcZ def",
            0,
        ),
        ("vi_backward_word", "abc def\x1b0ldbZ\r", "bc def", 1),
        ("vi_forward_word", "abc def\x1b0ldwZ\r", "adef", 1),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_redo_after_cancelled_operator() {
    // GNU replaces the redo change when a pending operator is cancelled: `.`
    // then appends one blank at the end of the line and rings.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("unbound_key", "abc\x1b0xdq.\r", "bc ", 1),
        ("cancelled_by_l", "abc\x1b0xdx.\r", "bc ", 1),
        ("right_arrow", "abc\x1b0xd\x1b[C.\r", "bc ", 1),
        ("csi_word_motion", "abc\x1b0xd\x1b[1;5C.\r", "bc ", 1),
        ("mid_line", "abc def\x1b0llldq.\r", "abc def ", 1),
        ("without_prior_edit", "abc\x1b0dq.\r", "abc ", 1),
        ("after_insert", "abc\x1b0iZ\x1bdq.\r", "Zabc ", 1),
        ("after_undo", "abc\x1b0xdqu.\r", "abc ", 1),
        ("undo_after_redo", "abc\x1b0xdq.u\r", "abc ", 1),
        ("undo_after_repeated_redo", "abc\x1b0xdq.u.\r", "abc  ", 2),
        ("repeated", "abc\x1b0xd\x1b[C..\r", "bc  ", 2),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
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
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_redo_after_failed_search() {
    let inputrc = "set editing-mode vi";
    for (desc, keys) in [
        ("undo", "abc\x1b0xfqu\r"),
        ("redo", "abc\x1b0xfq.\r"),
        ("undo_whole_line", "abc\x1b0xfqU\r"),
        ("after_cw", "abc\x1b0cwZ\x1bfqu\r"),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_replace_ignored_esc_runs() {
    // After an ignored replacement ESC, GNU consumes any redundant bare ESC
    // run and starts a fresh sequence for the following CSI/SS3 key. Retaining
    // the replacement ESC in that run resolves to a destructive vi command.
    let inputrc = "set editing-mode vi";
    for escapes in 1..=5 {
        for (desc, tail, expected, bells) in [
            ("csi_right", "[C", "ac", 0),
            ("csi_left", "[D", "bc", 1),
            ("ss3_right", "OC", "ac", 0),
            ("plain_key", "x", "c", 0),
        ] {
            let keys = format!("abc\x1b0r{}{}x\r", "\x1b".repeat(escapes), tail);
            let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
            let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
            let case = format!("{desc} after {escapes} ESCs");
            assert_eq!(
                accepted_line(&bash),
                Some(expected.to_string()),
                "{case}: {bash}"
            );
            assert_eq!(bell_count(&bash), bells, "{case}: {bash}");
            assert_eq!(
                accepted_line(&sushline),
                accepted_line(&bash),
                "{case}: bash={bash}\nsushline={sushline}"
            );
            assert_eq!(
                bell_count(&sushline),
                bells,
                "{case}: bash={bash}\nsushline={sushline}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_vi_dot_replay_count() {
    // An explicit count before `.` replaces the saved count and becomes the
    // new saved count for later replays.
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected) in [
        ("replace_then_dot", "abcdefghij\x1b03ry0fe.\r", "yyydyyyhij"),
        (
            "replace_then_counted_dot",
            "abcdefghij\x1b03ry02.\r",
            "yyydefghij",
        ),
        (
            "counted_dot_then_dot",
            "abcdefghij\x1b03ry02.0fe.\r",
            "yyydyyghij",
        ),
        (
            "replace_char_then_counted_dot",
            "abcdefghij\x1b03rx2.\r",
            "xxxxefghij",
        ),
        (
            "multi_digit_change",
            "abcdefghijklmnop\x1b010ry2.\r",
            "yyyyyyyyyyylmnop",
        ),
        ("delete_then_counted_dot", "abcdefghij\x1b02x3.\r", "fghij"),
        ("delete_then_dot", "abcdefghij\x1b02x.\r", "efghij"),
        ("delete_line_then_counted_dot", "abcdefghij\x1b02dd3.\r", ""),
        ("zero_before_dot", "abcdefghij\x1b03ry0.\r", "yyydefghij"),
        ("no_saved_count", "abcdefghij\x1b0x3.\r", "efghij"),
        ("insert_count2", "abc\x1biZ\x1b2.\r", "abZZZc"),
        ("insert_count3", "abc\x1biZ\x1b3.\r", "abZZZZc"),
        ("insert_multi_count2", "abc\x1biYZ\x1b2.\r", "abYYZYZZc"),
        ("insert_saved_count", "abc\x1b2iZ\x1b.\r", "abZZc"),
        ("insert_count_then_plain", "abc\x1biZ\x1b2..\r", "abZZZZc"),
        ("put_plain", "abc\x1b0xp2.\r", "baaac"),
        ("put_counted", "abc\x1b0x3p2.\r", "baaaaac"),
        ("put_before", "abc\x1b0xP2.\r", "aaabc"),
        ("put_count_then_plain_dot", "abc\x1b0xp2..\r", "baaaaac"),
        (
            "put_counted_then_count_then_plain",
            "abc\x1b0x3p2..\r",
            "baaaaaaac",
        ),
        (
            "put_before_count_then_plain_dot",
            "abc\x1b0xP2..\r",
            "aaaaabc",
        ),
        (
            "two_digit_leading_zero",
            "abcdefghijklmnopqrstuvwxyz\x1b023rx.\r",
            "xxxxxxxxxxxxxxxxxxxxxxxxxx",
        ),
        (
            "two_digit_saved",
            "abcdefghijklmnopqrstuvwxyz\x1b012rx.\r",
            "xxxxxxxxxxxxxxxxxxxxxxxxyz",
        ),
        (
            "two_digit_plain",
            "abcdefghijklmnopqrstuvwxyz\x1b010rx.\r",
            "xxxxxxxxxxxxxxxxxxxtuvwxyz",
        ),
        (
            "negative_count",
            "abcdefghij\x1b03ry\x1b-2.\r",
            "yyyyefghij",
        ),
        (
            "counted_dot_then_plain_dot",
            "abcdefghij\x1b03ry02..2.\r",
            "yyyyefghij",
        ),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_cancelled_redo_survives_reads() {
    // A cancelled operator remains the redo change into the next read, where
    // GNU's initial-insert refresh does not replace it.
    let inputrc = "set editing-mode vi";
    for (desc, keys, first, second, bells) in [
        (
            "with_initial_text",
            "abc\x1b0dq\rdef\x1b.\r",
            "abc",
            "def ",
            1,
        ),
        ("empty_read", "abc\x1b0dq\r\x1b.\r", "abc", " ", 1),
        ("after_deletion", "abc\x1b0xdq\rdef\x1b.\r", "bc", "def ", 1),
        (
            "motion_before_redo",
            "abc\x1b0dq\rdef\x1b0.\r",
            "abc",
            "def ",
            1,
        ),
    ] {
        let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(
            keys.as_bytes(),
            inputrc,
            &[],
        );
        let sushline =
            run_sushline_harness_two_reads_with_inputrc_and_history(keys.as_bytes(), inputrc, &[]);
        assert_eq!(
            accepted_numbered_line(&bash, 1),
            Some(first.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_numbered_line(&bash, 2),
            Some(second.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_numbered_line(&sushline, 1),
            accepted_numbered_line(&bash, 1),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            accepted_numbered_line(&sushline, 2),
            accepted_numbered_line(&bash, 2),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_replacement_esc_expires_on_keyseq_timeout() {
    // The ignored replacement ESC still introduces the next sequence when it
    // arrives promptly. After keyseq-timeout, GNU has expired that prefix and
    // the following CSI sequence is an unbound command.
    let before = b"abc\x1b0r\x1b";
    let after = b"[Cx\r";
    for (desc, pause, line, bells) in [
        ("prompt", Duration::from_millis(20), "ac", 0),
        ("expired", Duration::from_millis(900), "x", 1),
    ] {
        let bash =
            run_bash_readline_paused_with_bindings("set editing-mode vi", before, pause, after);
        let sushline = run_sushline_harness_paused_with_inputrc(
            "set editing-mode vi\nset keyseq-timeout 100",
            before,
            pause,
            after,
        );
        assert_eq!(
            accepted_line(&bash),
            Some(line.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_undo_after_cancelled_redo_survives_reads() {
    // A cancelled redo persists, but GNU still frees the undo list on the
    // next read's first entry to command mode.
    let inputrc = "set editing-mode vi";
    for (desc, keys, second, bells) in [
        ("undo", "abc\x1b0dq\rdef\x1bu\r", "def", 1),
        ("redo_then_undo", "abc\x1b0dq\rdef\x1b.u\r", "def ", 2),
        ("undo_all", "abc\x1b0dq\rdef\x1bU\r", "def", 1),
        ("redo_then_undo_all", "abc\x1b0dq\rdef\x1b.U\r", "def ", 2),
    ] {
        let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(
            keys.as_bytes(),
            inputrc,
            &[],
        );
        let sushline =
            run_sushline_harness_two_reads_with_inputrc_and_history(keys.as_bytes(), inputrc, &[]);
        assert_eq!(
            accepted_numbered_line(&bash, 1),
            Some("abc".to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(
            accepted_numbered_line(&bash, 2),
            Some(second.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_numbered_line(&sushline, 1),
            accepted_numbered_line(&bash, 1),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            accepted_numbered_line(&sushline, 2),
            accepted_numbered_line(&bash, 2),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bells,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_undo_leaves_point_on_last_character() {
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected) in [
        ("delete_at_end_then_insert", "abcdef\x1bxuiX\r", "abcdeXf"),
        ("delete_at_end_then_delete", "abcdef\x1bxux\r", "abcde"),
        (
            "change_then_undo_then_dot",
            "abcdef\x1bcwZ\x1bu.\r",
            "abcdeZ",
        ),
        (
            "change_then_undo_then_insert",
            "abcdef\x1bcwZ\x1buiX\r",
            "abcdeXf",
        ),
        ("insert_at_end_then_insert", "abc\x1bAX\x1buiY\r", "abYc"),
        ("insert_at_end_then_delete", "abc\x1bAX\x1bux\r", "ab"),
        (
            "insert_in_middle_then_insert",
            "abc\x1b0lxX\x1buiY\r",
            "aYc",
        ),
        ("last_character_then_delete", "a\x1bxux\r", ""),
        ("two_undos", "abcdef\x1bxxuux\r", "abcde"),
        ("counted_undo", "abcdef\x1bxx2ux\r", "abcde"),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            0,
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}
