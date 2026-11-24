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

fn assert_screen_row(editor: &str, output: &str, expected: &str) {
    let mut screen = Screen::new(24, 80);
    screen.feed(output.as_bytes());
    assert!(
        screen.rows_text().iter().any(|row| row == expected),
        "{editor} must render {expected:?}: {output:?}"
    );
}

/// Count the raw U+200B bytes in the final incremental-search query: the last
/// search prompt prefix and the editor's query terminator bound the slice.
fn final_search_query_markers<'a>(output: &'a [u8], terminator: &[u8]) -> &'a [u8] {
    let prefix = b"reverse-i-search)`";
    let start = output
        .windows(prefix.len())
        .rposition(|window| window == prefix)
        .expect("search prompt prefix")
        + prefix.len();
    let end = output[start..]
        .windows(terminator.len())
        .position(|window| window == terminator)
        .expect("search query terminator")
        + start;
    &output[start..end]
}

/// Prompt-only named-command fixtures stop when the expected row is rendered,
/// instead of waiting for an accept line that these cases never send.
fn named_query_outputs(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
    row: &str,
) -> [(&'static str, String); 2] {
    let size = portable_pty::PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };
    [
        (
            "Bash",
            run_bash_readline_inputrc_until_screen_row(keys, inputrc, history, size, row),
        ),
        (
            "Sushline",
            run_sushline_harness_until_screen_row(keys, inputrc, history, size, row),
        ),
    ]
}

/// Run one byte-oriented editing case on both editors and require each ASCII-hex
/// acceptance marker to equal the exact hex of `expected`, plus the pinned
/// bell count. Comparing lossy strings would hide invalid raw bytes behind
/// U+FFFD; comparing only the editors with each other would accept two absent
/// markers.
fn assert_bytewise_editing_bytes(
    desc: &str,
    keys: &[u8],
    expected: &[u8],
    bells: usize,
    inputrc: &str,
    env: &[(&str, &str)],
) -> (Vec<u8>, Vec<u8>) {
    use portable_pty::PtySize;

    require_oracle_host();
    let dir = tempfile::tempdir().expect("tempdir");
    let inputrc_path = dir.path().join("inputrc");
    fs::write(&inputrc_path, inputrc).expect("write inputrc");
    let script = r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "SUSHLINE_READY>" line; printf 'BASH_ACCEPTED_HEX:'; printf '%s' "$line" | od -An -v -tx1 | tr -d ' \n'; printf '\n'"#;
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    let mut bash_command = portable_pty::CommandBuilder::new("bash");
    bash_command.env("INPUTRC", inputrc_path.to_string_lossy().as_ref());
    bash_command.env("HOME", dir.path());
    for (name, value) in env {
        bash_command.env(name, value);
    }
    bash_command.args(["--noprofile", "--norc", "-i", "-c", script]);
    let bash = run_pty_in_dir_with_size_raw_until(
        bash_command,
        keys,
        dir.path(),
        size,
        "BASH_ACCEPTED_HEX:",
    );

    let mut sush_command = sushline_harness_command();
    sush_command.env("SUSHLINE_INPUTRC", inputrc);
    sush_command.env("SUSHLINE_PROMPT", READY_PROMPT);
    sush_command.env("SUSHLINE_HISTORY", "");
    sush_command.env("SUSHLINE_ACCEPTED_HEX", "1");
    sush_command.env("HOME", dir.path());
    for (name, value) in env {
        sush_command.env(name, value);
    }
    let sush = run_pty_in_dir_with_size_raw_until(
        sush_command,
        keys,
        dir.path(),
        size,
        "SUSHLINE_ACCEPTED_HEX:",
    );

    let expected_hex = expected
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    for (output, marker, editor) in [
        (&bash, "BASH_ACCEPTED_HEX:", "bash"),
        (&sush, "SUSHLINE_ACCEPTED_HEX:", "sushline"),
    ] {
        let output = String::from_utf8_lossy(output);
        assert_eq!(
            accepted_line_after_marker(&output, marker).as_deref(),
            Some(expected_hex.as_str()),
            "{desc}: {editor}, keys={keys:?}"
        );
        assert_eq!(
            bell_count(&output),
            bells,
            "{desc}: {editor}, keys={keys:?}"
        );
    }
    (bash, sush)
}

/// Raw output before an acceptance marker, without lossy UTF-8 substitution.
fn pre_acceptance_bytes<'a>(output: &'a [u8], marker: &str) -> &'a [u8] {
    let marker = marker.as_bytes();
    let end = output
        .windows(marker.len())
        .position(|window| window == marker)
        .expect("acceptance marker");
    &output[..end]
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
fn bash_readline_and_sushline_transpose_words_from_inside_word() {
    // GNU 8.3-p0 paired PTY on `alpha three`: offset 1 keeps the line and
    // rings, offset 7 swaps whole words without a bell.
    for (name, forward, expected, bells) in [
        ("inside_first", 1, "alpha three", 1),
        ("inside_second", 7, "three alpha", 0),
    ] {
        let mut keys = b"alpha three\x01".to_vec();
        keys.extend(std::iter::repeat_n(0x06, forward));
        keys.extend_from_slice(b"\x1bt\r");
        let bash = run_bash_readline(&keys);
        let sushline = run_sushline_harness(&keys);
        for (host, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some(expected.to_string()),
                "{name}: {host}={output}"
            );
            assert_eq!(bell_count(output), bells, "{name}: {host}={output}");
        }
    }
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
/// Ctrl-G is used as the abort key here; Escape stays in the query as raw
/// text. Both editors ring once and drop the pending count; the accepted line
/// and bell count are compared.
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
            assert_eq!(bell_count(output), 1, "{name} keys={keys:?}: {output:?}");
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
        b"abcX\x19\x1d\r".as_slice(),
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
        (
            "c_destination_byte",
            b"\xe3\x81\x82bc\x1b0rX\r".as_slice(),
            "C",
            vec![88, 129, 130, 98, 99],
            0,
        ),
        (
            "c_destination_counted",
            b"\xe3\x81\x82bc\x1b02rXY\r".as_slice(),
            "C",
            vec![88, 88, 130, 98, 99],
            0,
        ),
        (
            "c_destination_dot",
            b"\xe3\x81\x82bc\x1b0rXl.\r".as_slice(),
            "C",
            vec![88, 88, 130, 98, 99],
            0,
        ),
        (
            "utf8_destination_whole_char",
            b"\xe3\x81\x82bc\x1b0rX\r".as_slice(),
            "C.UTF-8",
            vec![88, 98, 99],
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
        ("backspace", "abc\x1b0ld\x08x\r", "ac", 0),
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

#[test]
fn bash_and_sushline_vi_put_dot_undo_group() {
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected) in [
        ("put_dot_undo", "abc\x1b0xp.u\r", "bc"),
        ("put_two_dots_undo", "abc\x1b0xp..u\r", "bc"),
        ("put_dot_two_undos", "abc\x1b0xp.uu\r", "abc"),
        ("put_counted_dot_undo", "abc\x1b0xp2.u\r", "bc"),
        ("two_puts_undo", "abc\x1b0xppu\r", "bc"),
        ("put_dot_put_undo", "abc\x1b0xp.pu\r", "bc"),
        ("put_dot_upper_put_undo", "abc\x1b0xp.Pu\r", "baac"),
        ("upper_put_dot_undo", "abc\x1b0xP.u\r", "abc"),
        ("put_dot_delete_undo", "abc\x1b0xp.xu\r", "baac"),
        ("put_then_insert_undo", "abc\x1b0xpiX\x1bu\r", "bac"),
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

#[test]
fn bash_and_sushline_emacs_undo_keeps_end_point() {
    for (desc, keys, expected) in [
        ("undo_rubout_then_insert", "abc\x7f\x1fX\r", "abcX"),
        ("undo_rubout_then_rubout", "abc\x7f\x1f\x7f\r", "ab"),
        ("undo_rubout_left_insert", "abc\x7f\x1f\x1b[DX\r", "abXc"),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), "");
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), "");
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

#[test]
fn bash_and_sushline_vi_put_leaves_point_on_last_character() {
    for (desc, keys, expected) in [
        ("lower_insert", "abc\x1b0xpiX\x1b\r", "bXac"),
        ("lower_delete", "abc\x1b0xpx\r", "bc"),
        ("lower_forward", "abc\x1b0xpl\r", "bac"),
        ("upper_insert", "abc\x1b0xPiX\x1b\r", "Xabc"),
        ("upper_delete", "abc\x1b0xPx\r", "bc"),
        ("counted_insert", "abc\x1b0x2piX\x1b\r", "baXac"),
        ("yanked_insert", "abc\x1byl0p iX\x1b\r", "acXbc"),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), "set editing-mode vi");
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), "set editing-mode vi");
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

#[test]
fn bash_and_sushline_vi_put_dot_replay_moves_forward() {
    for (desc, keys, expected) in [
        ("dot_insert", "abc\x1b0xp.iX\x1b\r", "baXac"),
        ("double_dot_insert", "abc\x1b0xp..iX\x1b\r", "baaXac"),
        ("counted_dot_insert", "abc\x1b0x2p.iX\x1b\r", "baaaXac"),
        ("upper_dot_insert", "abc\x1b0xP.iX\x1b\r", "Xaabc"),
        ("put_undo_dot", "abc\x1b0xpu.\r", "bca"),
        ("put_dot_undo_dot", "abc\x1b0xp.u.\r", "bca"),
        ("counted_put_undo_dot", "abc\x1b0x2pu.\r", "bcaa"),
        ("put_undo_double_dot", "abc\x1b0xpu..\r", "bcaa"),
        ("lower_put_after_l", "abc\x1b0xplp\r", "baca"),
        ("put_after_l_undo", "abc\x1b0xplpu\r", "bac"),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), "set editing-mode vi");
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), "set editing-mode vi");
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

#[test]
fn bash_and_sushline_vi_put_undo_group_uses_one_byte_yanks() {
    for (desc, keys, expected) in [
        ("yank_two", "abcd\x1b02xp.u\r", "cabd"),
        ("yank_three", "abcdef\x1b03xp.u\r", "dabcef"),
        ("yank_two_two_undos", "abcd\x1b02xp.uu\r", "cd"),
        ("single_byte_control", "abc\x1b0xp.u\r", "bc"),
        ("single_byte_count_control", "abc\x1b0x2pu\r", "bc"),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), "set editing-mode vi");
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), "set editing-mode vi");
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

#[test]
fn bash_and_sushline_vi_put_undo_group_splits_on_motion() {
    for (desc, keys, expected) in [
        ("backward", "abc\x1b0xph.u\r", "bac"),
        ("line_start", "abc\x1b0xp0.u\r", "bac"),
        ("line_end", "abc\x1b0xp$.u\r", "bac"),
        ("after_dot", "abc\x1b0xp.0.u\r", "baac"),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), "set editing-mode vi");
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), "set editing-mode vi");
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

#[test]
fn bash_and_sushline_vi_put_undo_respects_byte_cap() {
    for (desc, ops, expected) in [
        ("twenty_puts", "pppppppppppppppppppp", "bc"),
        (
            "twenty_one_puts",
            "ppppppppppppppppppppp",
            "baaaaaaaaaaaaaaaaaaaac",
        ),
        (
            "twenty_two_puts",
            "pppppppppppppppppppppp",
            "baaaaaaaaaaaaaaaaaaaac",
        ),
        ("put_and_nineteen_dots", "p...................", "bc"),
        (
            "put_and_twenty_dots",
            "p....................",
            "baaaaaaaaaaaaaaaaaaaac",
        ),
        (
            "put_and_twenty_one_dots",
            "p.....................",
            "baaaaaaaaaaaaaaaaaaaac",
        ),
        (
            "counted_crosses_cap",
            "ppppppppppppppppppp2p",
            "baaaaaaaaaaaaaaaaaaaac",
        ),
        (
            "counted_then_dot_crosses",
            "pppppppppppppppppp2p.",
            "baaaaaaaaaaaaaaaaaaaac",
        ),
        ("counted_early", "2ppppppppppppppppppp", "bc"),
    ] {
        let keys = format!("abc\x1b0x{ops}u\r");
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), "set editing-mode vi");
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), "set editing-mode vi");
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

#[test]
fn bash_and_sushline_vi_insert_undo_keeps_long_run() {
    for (desc, keys, expected, mode) in [
        (
            "insert_twenty",
            "abc\x1b0iaaaaaaaaaaaaaaaaaaaa\x1bu\r",
            "abc",
            "vi",
        ),
        (
            "insert_twenty_one",
            "abc\x1b0iaaaaaaaaaaaaaaaaaaaaa\x1bu\r",
            "abc",
            "vi",
        ),
        (
            "insert_forty",
            "abc\x1b0iaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\x1bu\r",
            "abc",
            "vi",
        ),
        (
            "insert_multibyte_twenty_two",
            "abc\x1b0iééééééééééé\x1bu\r",
            "abc",
            "vi",
        ),
        (
            "replace_twenty_one",
            "abc\x1b0RXXXXXXXXXXXXXXXXXXXXX\x1bu\r",
            "abc",
            "vi",
        ),
        (
            "insert_dot_twenty_one",
            "abc\x1biX\x1b.....................u\r",
            "abXXXXXXXXXXXXXXXXXXXXXc",
            "vi",
        ),
        (
            "emacs_twenty_one",
            "aaaaaaaaaaaaaaaaaaaaa\x1fX\r",
            "aaaaaaaaaaaaaaaaaaaaX",
            "emacs",
        ),
    ] {
        let inputrc = if mode == "vi" {
            "set editing-mode vi"
        } else {
            ""
        };
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

#[test]
fn bash_and_sushline_first_counted_vi_put_respects_byte_cap() {
    for (desc, keys, expected) in [
        ("p_twenty", "abc\x1b0x20pu\r", "bc"),
        ("p_twenty_one", "abc\x1b0x21pu\r", "baaaaaaaaaaaaaaaaaaaac"),
        ("p_twenty_five", "abc\x1b0x25pu\r", "baaaaaaaaaaaaaaaaaaaac"),
        ("p_twenty_five_two_undos", "abc\x1b0x25puu\r", "bc"),
        ("P_twenty_five", "abc\x1b0x25Pu\r", "aaaaaaaaaaaaaaaaaaaabc"),
        ("P_twenty_five_two_undos", "abc\x1b0x25Puu\r", "bc"),
        (
            "insert_dot_twenty_five_control",
            "abc\x1b0xiX\x1b25.uu\r",
            "bc",
        ),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), "set editing-mode vi");
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), "set editing-mode vi");
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

#[test]
fn bash_and_sushline_counted_vi_put_undo_chunks() {
    for (desc, keys, expected) in [
        ("count_forty", "xab\x1b0lx40pu\r", "xbaaaaaaaaaaaaaaaaaaaa"),
        (
            "count_forty_five",
            "xab\x1b0lx45pu\r",
            "xbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        (
            "count_forty_five_two_undos",
            "xab\x1b0lx45puu\r",
            "xbaaaaaaaaaaaaaaaaaaaa",
        ),
        ("count_forty_five_three_undos", "xab\x1b0lx45puuu\r", "xb"),
        ("count_sixty_three_undos", "xab\x1b0lx60puuu\r", "xb"),
        (
            "dot_after_count_forty_five",
            "xab\x1b0lx45p.u\r",
            "xbaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        ("multibyte_two_yank", "xéb\x1b0lx10pu\r", "xbééééééééé"),
        (
            "multibyte_two_yank_two_undos",
            "xéb\x1b0lx10puu\r",
            "xbéééééééé",
        ),
        (
            "multibyte_three_yank",
            "x漢b\x1b0lx10pu\r",
            "xb漢漢漢漢漢漢漢漢漢",
        ),
        ("multibyte_uppercase_put", "xéb\x1b0lx10Pu\r", "xéééééééééb"),
        (
            "upper_dot_one_undo",
            "xab\x1b0lx45P.u\r",
            "xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab",
        ),
        (
            "upper_dot_two_undos",
            "xab\x1b0lx45P.uu\r",
            "xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab",
        ),
        (
            "upper_dot_three_undos",
            "xab\x1b0lx45P.uuu\r",
            "xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab",
        ),
        (
            "multibyte_dot_one_undo",
            "xéb\x1b0lx10p.u\r",
            "xbééééééééééééééééééé",
        ),
        (
            "multibyte_dot_two_undos",
            "xéb\x1b0lx10p.uu\r",
            "xbéééééééééééééééééé",
        ),
        (
            "multibyte_dot_three_undos",
            "xéb\x1b0lx10p.uuu\r",
            "xbééééééééééééééééé",
        ),
        (
            "three_byte_dot_one_undo",
            "x漢b\x1b0lx10p.u\r",
            "xb漢漢漢漢漢漢漢漢漢漢漢漢漢漢漢漢漢漢漢",
        ),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), "set editing-mode vi");
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), "set editing-mode vi");
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

#[test]
fn bash_and_sushline_emacs_undo_cap_after_vi_mode_switch() {
    let inputrc = concat!(
        "set editing-mode vi\n",
        "set keymap vi-insert\n",
        "\"\\C-t\": emacs-editing-mode"
    );
    for (desc, inserted) in [("twenty_one", 21), ("twenty_five", 25)] {
        let keys = "abc\x1bA\x14".to_owned() + &"a".repeat(inserted) + "\x1f\r";
        let expected = "abc".to_owned() + &"a".repeat(20);
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.clone()),
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

#[test]
fn bash_and_sushline_long_vi_edit_undo_entries() {
    let inputrc = "set editing-mode vi";
    let thirty_a = "a".repeat(30);
    let thirty_six = "abcdefghijklmnopqrstuvwxyz0123456789".to_string();
    for (desc, keys, expected, bells) in [
        (
            "delete_line",
            format!("{thirty_a}\x1bddu\r"),
            thirty_a.clone(),
            0,
        ),
        (
            "change_line",
            format!("{thirty_a}\x1bccbbbbb\x1bu\r"),
            thirty_a.clone(),
            0,
        ),
        (
            "counted_x",
            format!("{thirty_six}\x1b030xu\r"),
            thirty_six.clone(),
            0,
        ),
        (
            "counted_r",
            format!("{thirty_six}\x1b030rxu\r"),
            thirty_six.clone(),
            0,
        ),
        (
            "counted_r_dot_one_undo",
            "abcdefghij\x1b03rx.u\r".to_string(),
            "xxxdefghij".to_string(),
            0,
        ),
        (
            "counted_r_dot_two_undos",
            "abcdefghij\x1b03rx.uu\r".to_string(),
            "abcdefghij".to_string(),
            0,
        ),
        (
            "two_dot_replays_one_undo",
            "abcdefghij\x1b03rx..u\r".to_string(),
            "xxxxxfghij".to_string(),
            0,
        ),
        (
            "multibyte_counted_r",
            "xéééé\x1b03rxu\r".to_string(),
            "xéééé".to_string(),
            0,
        ),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(accepted_line(&bash), Some(expected), "{desc}: {bash}");
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
fn bash_and_sushline_vi_insert_redo_chains_and_counts() {
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("append_then_append_dot", "a1\x1ba2\x1b.\r", "a122", 0),
        ("insert_then_append_dot", "i1\x1ba2\x1b.\r", "i122", 0),
        (
            "counted_insert_dot",
            "abc\x1b05ihello\x1b.\r",
            "hellhellooabc",
            0,
        ),
        (
            "counted_insert_explicit_dot",
            "abc\x1b05ihello\x1b3..\r",
            "hellhellohellohellhelloooabc",
            0,
        ),
        (
            "counted_append_dot",
            "abc\x1b05ahello\x1b.\r",
            "ahellohellobc",
            0,
        ),
        ("append_then_append_dot_undo", "a1\x1ba2\x1b.u\r", "a12", 0),
        ("append_then_two_dots", "a1\x1ba2\x1b..\r", "a1222", 0),
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
fn bash_and_sushline_vi_explicit_dot_counts() {
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        (
            "replace_explicit",
            "abcdefghij\x1b03rx3.\r",
            "xxxxxfghij",
            0,
        ),
        (
            "replace_explicit_then_plain",
            "abcdefghij\x1b03rx2..\r",
            "xxxxxfghij",
            0,
        ),
        (
            "replace_plain_then_explicit",
            "abcdefghij\x1b03rx.3.\r",
            "xxxxxxxhij",
            0,
        ),
        ("delete_explicit", "abcdefghij\x1b03x3.\r", "ghij", 0),
        (
            "delete_plain_then_explicit",
            "abcdefghij\x1b03x.2.\r",
            "ij",
            0,
        ),
        ("change_explicit", "abc def ghi\x1b0cwZ\x1b3.\r", "Z", 0),
        (
            "replace_explicit_then_undo",
            "abcdefghij\x1b03rx3.u\r",
            "xxxdefghij",
            0,
        ),
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
fn bash_and_sushline_vi_insert_yank_pop_matches_gnu() {
    let inputrc =
        "set editing-mode vi\nset keymap vi-insert\n\"Q\": vi-yank-pop\n\"W\": yank-pop\n";
    let mut cases = vec![
        ("vi_no_yank", "", "Q", 2, 2),
        ("plain_no_yank", "", "W", 2, 2),
    ];
    for (name, cmd) in [("vi", "Q"), ("plain", "W")] {
        for count in 1..=4 {
            cases.push((
                name,
                "one\x17two\x17three\x17X\x19",
                cmd,
                count,
                if name == "vi" { count } else { 0 },
            ));
        }
    }
    for (name, prefix, cmd, count, bells) in cases {
        let keys = format!("{prefix}{}\r", cmd.repeat(count));
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), inputrc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            accepted_line(&sush),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
        assert_eq!(
            bell_count(&bash),
            bells,
            "{name}: bash={bash:?} sushline={sush:?}"
        );
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
    }

    // A failed vi-yank-pop invalidates the yank state in GNU, so the next
    // plain yank-pop rings instead of staying silent.
    for (suffix, bells) in [
        ("QW", 2),
        ("QWWW", 4),
        ("WQW", 2),
        ("WWQWW", 3),
        ("QWQW", 4),
    ] {
        let keys = format!("one\x17two\x17three\x17X\x19{suffix}\r");
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), inputrc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some("Xthree"),
            "{suffix}: bash={bash:?}"
        );
        assert_eq!(
            accepted_line(&sush),
            accepted_line(&bash),
            "{suffix}: bash={bash:?} sushline={sush:?}"
        );
        assert_eq!(bell_count(&bash), bells, "{suffix}: bash={bash:?}");
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{suffix}: bash={bash:?} sushline={sush:?}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_command_yank_pop_matches_gnu() {
    let inputrc =
        "set editing-mode vi\nset keymap vi-command\n\"Q\": vi-yank-pop\n\"W\": yank-pop\n";
    let cases = [
        ("no_kill", "Q", "Q", 0),
        ("kills_without_put", "one\x17two\x17three\x17\x1bQ", "", 1),
        (
            "insert_yank_then_command",
            "one\x17two\x17three\x17X\x19\x1bQ",
            "Xthree",
            1,
        ),
        (
            "command_yank",
            "one\x17two\x17three\x17\x1b\x19Q",
            "three",
            1,
        ),
        ("put", "one\x17two\x17three\x17\x1bpQ", "three", 0),
        ("put_before", "one\x17two\x17three\x17\x1bPQ", "three", 0),
        (
            "put_repeated",
            "one\x17two\x17three\x17\x1bpQQQ",
            "three",
            0,
        ),
        (
            "put_then_motion_zero",
            "one\x17two\x17three\x17\x1bp0Q",
            "three",
            1,
        ),
        (
            "put_then_motion_back",
            "one\x17two\x17three\x17\x1bphQ",
            "three",
            1,
        ),
        (
            "put_then_failed_motion",
            "one\x17two\x17three\x17\x1bplQ",
            "three",
            2,
        ),
        ("single_byte_put", "a\x17b\x17c\x17\x1bpQ", "c", 0),
        (
            "single_byte_put_repeated",
            "a\x17b\x17c\x17\x1bpQQQ",
            "c",
            0,
        ),
        (
            "plain_pop_after_put",
            "one\x17two\x17three\x17\x1bpWWW",
            "three",
            3,
        ),
        (
            "plain_pop_then_vi_pop",
            "one\x17two\x17three\x17\x1bpWQ",
            "three",
            2,
        ),
        (
            "numeric_plain_pop",
            "one\x17two\x17three\x17\x1bp2W",
            "three",
            1,
        ),
        (
            "numeric_plain_pop_repeated",
            "one\x17two\x17three\x17\x1bp2WW",
            "three",
            2,
        ),
        (
            "numeric_vi_pop_after_put",
            "one\x17two\x17three\x17\x1bp2Q",
            "three",
            0,
        ),
        (
            "numeric_plain_pop_after_vi_pop",
            "one\x17two\x17three\x17\x1bp2QW",
            "three",
            1,
        ),
    ];
    for (name, keys, line, bells) in cases {
        let keys = format!("{keys}\r");
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), inputrc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(line),
            "{name}: bash={bash:?}"
        );
        assert_eq!(
            accepted_line(&sush),
            accepted_line(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
        assert_eq!(bell_count(&bash), bells, "{name}: bash={bash:?}");
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_failed_word_motion_rings() {
    let inputrc = "set editing-mode vi\n";
    let cases = [
        ("eol_w", "abcdefghij\u{1b}w\r", "abcdefghij", 1),
        ("eol_W", "abcdefghij\u{1b}W\r", "abcdefghij", 1),
        ("empty_w", "\u{1b}w\r", "", 1),
        ("one_word_w", "abc def\u{1b}w\r", "abc def", 1),
        ("one_word_ww", "abc def\u{1b}ww\r", "abc def", 2),
        ("one_word_counted_w", "abc def\u{1b}2w\r", "abc def", 1),
        ("one_word_large_w", "abc def\u{1b}10w\r", "abc def", 1),
        ("one_word_W", "abc def\u{1b}W\r", "abc def", 1),
        ("one_word_WW", "abc def\u{1b}WW\r", "abc def", 2),
        ("one_word_counted_W", "abc def\u{1b}2W\r", "abc def", 1),
        (
            "three_words_w_success",
            "abc def ghi\u{1b}0www\r",
            "abc def ghi",
            0,
        ),
        (
            "three_words_w_overshoot",
            "abc def ghi\u{1b}0wwww\r",
            "abc def ghi",
            1,
        ),
        (
            "three_words_W_success",
            "abc def ghi\u{1b}0WWW\r",
            "abc def ghi",
            0,
        ),
        (
            "three_words_W_overshoot",
            "abc def ghi\u{1b}0WWWW\r",
            "abc def ghi",
            1,
        ),
        ("delete_word", "abc def\u{1b}dw\r", "abc de", 0),
        ("delete_bigword", "abc def\u{1b}dW\r", "abc de", 0),
        ("delete_counted_word", "abc def\u{1b}d2w\r", "abc de", 0),
        ("change_word", "abc def\u{1b}cwXYZ\u{1b}\r", "abc deXYZ", 0),
        ("failed_motion_keeps_point", "abc\u{1b}wiX\r", "abXc", 1),
        ("failed_motion_append_point", "abc\u{1b}WaX\r", "abcX", 1),
        (
            "overshoot_keeps_point",
            "abc def\u{1b}0wwwiX\r",
            "abc deXf",
            1,
        ),
        (
            "counted_overshoot_silent",
            "abc def ghi\u{1b}04w\r",
            "abc def ghi",
            0,
        ),
        (
            "counted_large_overshoot_silent",
            "abc def ghi\u{1b}010W\r",
            "abc def ghi",
            0,
        ),
        ("single_word_count_silent", "abc\u{1b}02w\r", "abc", 0),
        ("multibyte_w_point", "αβγ\u{1b}0wiX\r", "αβXγ", 0),
        ("multibyte_counted_w_point", "αβγδ\u{1b}02wiX\r", "αβγXδ", 0),
        ("multibyte_ww_point", "αβ γδ\u{1b}0wwiX\r", "αβ γXδ", 0),
    ];
    for (name, keys, expected, bells) in cases {
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), inputrc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{name}: bash={bash:?}"
        );
        assert_eq!(
            accepted_line(&sush),
            accepted_line(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
        assert_eq!(bell_count(&bash), bells, "{name}: bash={bash:?}");
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_failed_backward_word_motion_rings() {
    let inputrc = "set editing-mode vi\n";
    let cases = [
        ("empty_b", "\u{1b}b\r", "", 1),
        ("empty_B", "\u{1b}B\r", "", 1),
        ("column_zero_b", "abc\u{1b}0b\r", "abc", 1),
        ("column_zero_B", "abc\u{1b}0B\r", "abc", 1),
        ("column_zero_empty", "\u{1b}0b\r", "", 1),
        (
            "column_zero_repeated_b",
            "abc def ghi\u{1b}0bbb\r",
            "abc def ghi",
            3,
        ),
        (
            "column_zero_repeated_B",
            "abc def ghi\u{1b}0BBBB\r",
            "abc def ghi",
            4,
        ),
        (
            "column_zero_counted_b",
            "abc def ghi\u{1b}02b\r",
            "abc def ghi",
            1,
        ),
        (
            "column_zero_large_B",
            "abc def ghi\u{1b}010B\r",
            "abc def ghi",
            1,
        ),
        (
            "column_zero_trailing_count",
            "abc def ghi\u{1b}0b2\r",
            "abc def ghi",
            1,
        ),
        ("from_middle_b", "abc def ghi\u{1b}0lb\r", "abc def ghi", 0),
        (
            "from_middle_repeated_b",
            "abc def ghi\u{1b}0lbb\r",
            "abc def ghi",
            1,
        ),
        (
            "from_end_count_overshoots",
            "abc def ghi\u{1b}10b\r",
            "abc def ghi",
            0,
        ),
        (
            "from_end_large_B_overshoots",
            "abc def ghi\u{1b}10B\r",
            "abc def ghi",
            0,
        ),
        (
            "delete_at_column_zero",
            "abc def ghi\u{1b}0db\r",
            "abc def ghi",
            1,
        ),
        (
            "delete_bigword_at_column_zero",
            "abc def ghi\u{1b}0dB\r",
            "abc def ghi",
            1,
        ),
        (
            "delete_counted_at_column_zero",
            "abc def ghi\u{1b}0d2b\r",
            "abc def ghi",
            1,
        ),
        (
            "yank_at_column_zero",
            "abc def ghi\u{1b}0yb\r",
            "abc def ghi",
            1,
        ),
        ("delete_success", "abc def ghi\u{1b}db\r", "abc def i", 0),
        (
            "delete_bigword_success",
            "abc def ghi\u{1b}dB\r",
            "abc def i",
            0,
        ),
        (
            "failed_motion_keeps_point",
            "abc def ghi\u{1b}0biX\r",
            "Xabc def ghi",
            1,
        ),
        (
            "failed_bigword_keeps_point",
            "abc def ghi\u{1b}0BaX\r",
            "aXbc def ghi",
            1,
        ),
        (
            "back_off_then_failed_motion",
            "abc def ghi\u{1b}0hdb\r",
            "abc def ghi",
            2,
        ),
    ];
    for (name, keys, expected, bells) in cases {
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), inputrc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{name}: bash={bash:?}"
        );
        assert_eq!(
            accepted_line(&sush),
            accepted_line(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
        assert_eq!(bell_count(&bash), bells, "{name}: bash={bash:?}");
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_zero_width_change_enters_insert() {
    let inputrc = "set editing-mode vi\n";
    let cases = [
        ("c0_then_i", "abc\u{1b}0c0iX\u{1b}\r", "iXabc", 0),
        ("c0", "abc\u{1b}0c0XYZ\u{1b}\r", "XYZabc", 0),
        ("ch", "abc\u{1b}0chXYZ\u{1b}\r", "XYZabc", 1),
        ("cb", "abc\u{1b}0cbXYZ\u{1b}\r", "XYZabc", 1),
        ("cB", "abc\u{1b}0cBXYZ\u{1b}\r", "XYZabc", 1),
        ("c_caret", "abc\u{1b}0c^XYZ\u{1b}\r", "XYZabc", 0),
        ("c0_dot", "abc\u{1b}0c0XYZ\u{1b}.\r", "XYZZabc", 0),
        ("c0_undo", "abc\u{1b}0c0XYZ\u{1b}u\r", "abc", 0),
        ("c0_empty_insert", "abc\u{1b}0c0\u{1b}\r", "abc", 0),
        ("c0_multibyte_insert", "abc\u{1b}0c0αβ\u{1b}\r", "αβabc", 0),
        ("cw_control", "abc def\u{1b}0cwXYZ\u{1b}\r", "XYZ def", 0),
        (
            "c0_longer_line",
            "abc def\u{1b}0c0XYZ\u{1b}\r",
            "XYZabc def",
            0,
        ),
        (
            "c_caret_nonzero_width",
            "  abc\u{1b}0c^XYZ\u{1b}\r",
            "XYZabc",
            0,
        ),
    ];
    for (name, keys, expected, bells) in cases {
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), inputrc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{name}: bash={bash:?}"
        );
        assert_eq!(
            accepted_line(&sush),
            accepted_line(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
        assert_eq!(bell_count(&bash), bells, "{name}: bash={bash:?}");
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
    }
}

#[test]
fn bash_and_sushline_vi_non_ascii_word_motion_bells() {
    let inputrc = "set editing-mode vi\n";
    let cases = [
        ("non_ascii_w", "αβγ\u{1b}w\r", "αβγ", 0),
        ("non_ascii_W", "αβγ\u{1b}W\r", "αβγ", 0),
        ("non_ascii_ww", "αβγ\u{1b}ww\r", "αβγ", 0),
        ("non_ascii_0ww", "αβγ\u{1b}0ww\r", "αβγ", 0),
        ("longer_non_ascii_w", "αβγδ\u{1b}w\r", "αβγδ", 0),
        ("mixed_non_ascii_w", "αβ γδ\u{1b}w\r", "αβ γδ", 0),
        ("ascii_then_non_ascii_w", "abc αβ\u{1b}w\r", "abc αβ", 0),
        ("non_ascii_then_ascii_w", "αβ abc\u{1b}w\r", "αβ abc", 1),
        ("non_ascii_b", "αβγ\u{1b}b\r", "αβγ", 0),
        ("non_ascii_column_zero_b", "αβγ\u{1b}0b\r", "αβγ", 1),
        ("non_ascii_column_zero_B", "αβγ\u{1b}0B\r", "αβγ", 1),
        ("non_ascii_column_zero_h", "αβγ\u{1b}0h\r", "αβγ", 1),
    ];
    for (name, keys, expected, bells) in cases {
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), inputrc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{name}: bash={bash:?}"
        );
        assert_eq!(
            accepted_line(&sush),
            accepted_line(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
        assert_eq!(bell_count(&bash), bells, "{name}: bash={bash:?}");
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{name}: bash={bash:?} sushline={sush:?}"
        );
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-wise insertion inside UTF-8.
///
/// `forward-byte` can leave point between the bytes of a multibyte character.
/// Both editors accept the same raw bytes; a lossy string comparison would
/// hide the difference between those bytes and U+FFFD. This fixture also pins
/// the distinguishing pre-acceptance redisplay fragments: GNU patches the
/// partial byte without painting the inserted `Z`, while Sushline repaints
/// the whole invalid line. No full redisplay-byte parity is claimed.
#[test]
fn bash_and_sushline_bytewise_insertion_inside_utf8_keeps_raw_bytes() {
    let inputrc = "\"\\C-xf\": forward-byte\n\"\\C-xb\": backward-byte\n";
    let (bash, sush) = assert_bytewise_editing_bytes(
        "insert_ascii_inside_utf8",
        b"\xc3\xa4b\x01\x18f\x18b\x18fZ\r",
        &[0xc3, 0x5a, 0xa4, 0x62],
        0,
        inputrc,
        &[("LC_ALL", "C.UTF-8")],
    );
    let bash_pre = pre_acceptance_bytes(&bash, "BASH_ACCEPTED_HEX:");
    let sush_pre = pre_acceptance_bytes(&sush, "SUSHLINE_ACCEPTED_HEX:");
    // GNU patches the invalid byte in place and never paints the typed Z;
    // Sushline repaints the whole line, including the inserted Z. These are
    // the distinguishing fragments, not a claim of full redisplay-byte parity.
    assert!(
        bash_pre.windows(6).any(|w| w == b"\x08\x08\xc3\x1b[C"),
        "bash must include the partial-byte patch: {bash_pre:?}"
    );
    assert!(
        !bash_pre.contains(&b'Z'),
        "bash must not paint the inserted Z: {bash_pre:?}"
    );
    assert!(
        sush_pre
            .windows(READY_PROMPT.len() + 4)
            .any(|w| w == b"SUSHLINE_READY>\xc3\x5a\xa4\x62"),
        "sushline must repaint the invalid line with Z: {sush_pre:?}"
    );
    assert!(
        !sush_pre.windows(6).any(|w| w == b"\x08\x08\xc3\x1b[C"),
        "sushline must not use GNU's partial-byte patch: {sush_pre:?}"
    );
}

/// Paired pre-Enter checkpoint for the same insertion. Ctrl-G after each
/// complete key step makes the captured frame causal: the `Z` paint is present
/// or absent before `accept-line` can affect redisplay.
#[test]
fn bash_and_sushline_bytewise_insertion_paint_differs_before_enter() {
    use portable_pty::PtySize;

    require_oracle_host();
    let dir = tempfile::tempdir().expect("tempdir");
    let inputrc = "\"\\C-xf\": forward-byte\n\"\\C-xb\": backward-byte\n";
    let inputrc_path = dir.path().join("inputrc");
    fs::write(&inputrc_path, inputrc).expect("write inputrc");
    let script = r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "SUSHLINE_READY>" line; printf 'BASH_ACCEPTED_HEX:'; printf '%s' "$line" | od -An -v -tx1 | tr -d ' 
'; printf '
'"#;
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };
    let steps: Vec<Vec<u8>> = vec![
        b"\xc3\xa4b".to_vec(),
        vec![0x01],
        b"\x18f".to_vec(),
        b"\x18b".to_vec(),
        b"\x18fZ".to_vec(),
    ];

    let mut bash_command = portable_pty::CommandBuilder::new("bash");
    bash_command.env("INPUTRC", inputrc_path.to_string_lossy().as_ref());
    bash_command.env("HOME", dir.path());
    bash_command.env("LC_ALL", "C.UTF-8");
    bash_command.args(["--noprofile", "--norc", "-i", "-c", script]);
    let bash_frames = run_pty_checkpoints_raw(bash_command, READY_PROMPT, &steps, size);

    let mut sush_command = sushline_harness_command();
    sush_command.env("SUSHLINE_INPUTRC", inputrc);
    sush_command.env("SUSHLINE_PROMPT", READY_PROMPT);
    sush_command.env("SUSHLINE_HISTORY", "");
    sush_command.env("LC_ALL", "C.UTF-8");
    let sush_frames = run_pty_checkpoints_raw(sush_command, READY_PROMPT, &steps, size);

    assert_eq!(bash_frames.len(), steps.len());
    assert_eq!(sush_frames.len(), steps.len());
    let bash = bash_frames.last().expect("bash final frame");
    let sush = sush_frames.last().expect("sushline final frame");
    assert!(
        bash.ends_with(b"\x08\x08\xc3\x1b[C"),
        "bash must stop at the partial-byte patch before Enter: {bash:?}"
    );
    assert!(
        !bash.contains(&b'Z'),
        "bash must not paint the inserted Z before Enter: {bash:?}"
    );
    assert!(
        sush.windows(READY_PROMPT.len() + 4)
            .any(|window| window == b"SUSHLINE_READY>\xc3\x5a\xa4\x62"),
        "sushline must repaint the inserted Z before Enter: {sush:?}"
    );
    assert_eq!(
        sush.iter().filter(|byte| **byte == b'Z').count(),
        1,
        "sushline must paint the inserted Z once before Enter: {sush:?}"
    );
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented vi word-motion point.
///
/// In byte-oriented mode GNU leaves a stalled `w`/`W` on the final byte of a
/// multibyte character, but a motion from before that character on its first
/// byte. The UTF-8 controls keep the grapheme back-off.
#[test]
fn bash_and_sushline_byte_oriented_vi_word_point_matches_gnu() {
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let cases: [(&str, &[u8], usize); 4] = [
        ("aé\u{1b}wiX\r", b"a\xc3X\xa9", 1),
        ("aé\u{1b}WiX\r", b"a\xc3X\xa9", 1),
        ("aé\u{1b}0wiX\r", b"aX\xc3\xa9", 0),
        ("aé b\u{1b}wiX\r", b"a\xc3\xa9 Xb", 1),
    ];
    for (keys, expected, bells) in cases {
        assert_bytewise_editing_bytes("{keys:?}", keys.as_bytes(), expected, bells, byte_rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for undo point after a mode switch.
///
/// Undo must use the active editing mode: a prior visit to vi command mode
/// must not keep the vi end-of-line back-off after `emacs-editing-mode`.
#[test]
fn bash_and_sushline_undo_point_follows_active_mode() {
    let rc = "set editing-mode vi\n\"\\C-o\": emacs-editing-mode\n";
    let cases = [
        ("abc\u{1b}A\u{f}\u{7f}\u{1f}X\r", "abcX", 0),
        ("abc\u{1b}A\u{f}\u{7f}\u{1f}\u{7f}\r", "ab", 0),
        ("abc\u{f}\u{7f}\u{1f}X\r", "abcX", 0),
        ("abc\u{1b}A\u{1f}X\r", "abcX", 1),
    ];
    for (keys, expected, bells) in cases {
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), rc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
        assert_eq!(accepted_line(&bash), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(accepted_line(&sush), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(bell_count(&bash), bells, "{keys:?}");
        assert_eq!(bell_count(&sush), bells, "{keys:?}");
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for a raw trailing byte at vi word motion.
///
/// A continuation byte alone is invalid UTF-8, so it must not be mistaken for
/// the tail of a multibyte character and silence the failed-motion bell.
#[test]
fn bash_and_sushline_vi_invalid_tail_word_motion_bells() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n"
    );
    let env = [("LC_ALL", "C.UTF-8")];
    for (tail, motion, expected) in [
        (0x80u8, b'w', b"aX\x80"),
        (0x80u8, b'W', b"aX\x80"),
        (0xbfu8, b'w', b"aX\xbf"),
        (0xbfu8, b'W', b"aX\xbf"),
        (0xc3u8, b'w', b"aX\xc3"),
        (0xc3u8, b'W', b"aX\xc3"),
    ] {
        let mut keys = b"a".to_vec();
        keys.push(tail);
        keys.extend_from_slice(&[0x1b, motion, b'i', b'X', b'\r']);
        assert_bytewise_editing_bytes(
            &format!("tail={tail:02x}, motion={}", motion as char),
            &keys,
            expected,
            1,
            rc,
            &env,
        );
    }
    let cases: [(&str, &[u8], usize); 3] = [
        ("aé\u{1b}wiX\r", b"aX\xc3\xa9", 0),
        ("abc\u{1b}wiX\r", b"abXc", 1),
        ("aé\u{1b}0wiX\r", b"aX\xc3\xa9", 0),
    ];
    for (keys, expected, bells) in cases {
        assert_bytewise_editing_bytes("{keys:?}", keys.as_bytes(), expected, bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for repeated zero-width change dot replay.
///
/// GNU leaves redo point after the inserted text, so a second dot edits the
/// next span; the trailing ESC of a recorded zero-width redo must not back it
/// up. The `cw` control keeps the nonzero-width replay point unchanged.
#[test]
fn bash_and_sushline_zero_width_change_repeated_dot_point() {
    let rc = "set editing-mode vi\n";
    let cases = [
        ("abc\u{1b}0c0X\u{1b}..\r", "XXabc", 0),
        ("abc\u{1b}0c0X\u{1b}...\r", "XXabc", 0),
        ("abc\u{1b}0c0X\u{1b}.iY\u{1b}\r", "XYXabc", 0),
        ("abc\u{1b}0c0X\u{1b}.aY\u{1b}\r", "XXYabc", 0),
        ("abc\u{1b}0cbX\u{1b}..\r", "XXabc", 2),
        ("abc\u{1b}0cbX\u{1b}.iY\u{1b}\r", "XYXabc", 2),
        ("abc\u{1b}0cbX\u{1b}.aY\u{1b}\r", "XXYabc", 2),
        ("abc\u{1b}0chX\u{1b}..\r", "XXabc", 2),
        ("abc\u{1b}0chX\u{1b}.iY\u{1b}\r", "XYXabc", 2),
        ("abc\u{1b}0chX\u{1b}.aY\u{1b}\r", "XXYabc", 2),
        ("abc\u{1b}0cBX\u{1b}..\r", "XXabc", 2),
        ("abc\u{1b}0cBX\u{1b}.iY\u{1b}\r", "XYXabc", 2),
        ("abc\u{1b}0cBX\u{1b}.aY\u{1b}\r", "XXYabc", 2),
        ("abc\u{1b}0c^X\u{1b}..\r", "XXabc", 0),
        ("abc\u{1b}0c^X\u{1b}.iY\u{1b}\r", "XYXabc", 0),
        ("abc\u{1b}0c^X\u{1b}.aY\u{1b}\r", "XXYabc", 0),
        ("αβγ\u{1b}0c0X\u{1b}..iY\u{1b}\r", "XYXαβγ", 0),
        ("abc\u{1b}0cwX\u{1b}..iY\u{1b}\r", "YX", 0),
    ];
    for (keys, expected, bells) in cases {
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), rc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
        assert_eq!(accepted_line(&bash), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(accepted_line(&sush), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(bell_count(&bash), bells, "{keys:?}");
        assert_eq!(bell_count(&sush), bells, "{keys:?}");
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for counted zero-width vi changes.
///
/// A count before `c0` must not absorb the zero motion as another count
/// digit; the change and its dot replay keep GNU's accepted line and point.
#[test]
fn bash_and_sushline_counted_zero_width_change_matches_gnu() {
    let rc = "set editing-mode vi\n";
    let cases = [
        ("abc\u{1b}02c0X\u{1b}\r", "Xabc", 0),
        ("abc\u{1b}02chX\u{1b}\r", "Xabc", 1),
        ("abc\u{1b}02cbX\u{1b}\r", "Xabc", 1),
        ("abc\u{1b}03c0XY\u{1b}\r", "XYabc", 0),
        ("abc\u{1b}02c0XYZ\u{1b}\r", "XYZabc", 0),
        ("abc\u{1b}02c0iY\u{1b}\r", "iYabc", 0),
        ("abc\u{1b}02c0X\u{1b}.\r", "XXabc", 0),
        ("abc\u{1b}02c0X\u{1b}..\r", "XXabc", 0),
        ("abc\u{1b}02c0X\u{1b}.iY\u{1b}\r", "XYXabc", 0),
        ("abc\u{1b}2c0X\u{1b}\r", "Xc", 0),
        ("abc\u{1b}0c0X\u{1b}\r", "Xabc", 0),
    ];
    for (keys, expected, bells) in cases {
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), rc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
        assert_eq!(accepted_line(&bash), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(accepted_line(&sush), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(bell_count(&bash), bells, "{keys:?}");
        assert_eq!(bell_count(&sush), bells, "{keys:?}");
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for undo after repeated zero-width dots.
///
/// From the second dot onward GNU consumes one no-op undo entry per replay
/// before undoing the recorded edits; point still stays after the inserted
/// text. The `cw` nonzero-width path is deliberately left out of this claim.
#[test]
fn bash_and_sushline_zero_width_change_undo_steps() {
    let rc = "set editing-mode vi\n";
    let lines = [
        ("abc", "0c0", 0),
        ("abc", "0cb", 2),
        ("abc", "0ch", 2),
        ("abc", "0cB", 2),
        ("abc", "0c^", 0),
        ("αβγ", "0c0", 0),
    ];
    let suffixes = [
        ("..u", 2),
        ("..uu", 1),
        ("..uuu", 0),
        ("...uu", 2),
        ("...uuu", 1),
        (".u", 1),
    ];
    for (line, motion, bells) in lines {
        for (suffix, remaining_insertions) in suffixes {
            let keys = format!("{line}\x1b{motion}X\x1b{suffix}\r");
            let expected = format!("{}{line}", "X".repeat(remaining_insertions));
            let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), rc);
            let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
            assert_eq!(accepted_line(&bash), Some(expected.clone()), "{keys:?}");
            assert_eq!(accepted_line(&sush), Some(expected), "{keys:?}");
            assert_eq!(bell_count(&bash), bells, "{keys:?}");
            assert_eq!(bell_count(&sush), bells, "{keys:?}");
        }
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for repeated byte-oriented word points.
///
/// In `LC_ALL=C` with `byte-oriented on`, raw non-ASCII bytes are punctuation
/// for `w`; `W` scans them as non-whitespace bytes and leaves point on the
/// final byte of a trailing multibyte character. The bell difference for
/// `aé ESC 0ww` is deliberately not part of this fixture.
#[test]
fn bash_and_sushline_byte_oriented_repeated_word_points() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let cases: [(&str, &str, &[u8], usize); 14] = [
        ("aé", "0W", b"a\xc3X\xa9", 0),
        ("aé", "w", b"a\xc3X\xa9", 1),
        ("aé", "0w", b"aX\xc3\xa9", 0),
        ("aé", "02w", b"a\xc3X\xa9", 0),
        ("aé", "0WW", b"a\xc3X\xa9", 1),
        ("aé b", "0w", b"aX\xc3\xa9 b", 0),
        ("aé b", "0ww", b"a\xc3\xa9 Xb", 0),
        ("aé b", "02w", b"a\xc3\xa9 Xb", 0),
        ("aé b", "0W", b"a\xc3\xa9 Xb", 0),
        ("éa", "0w", b"\xc3\xa9Xa", 0),
        ("éa b", "0w", b"\xc3\xa9Xa b", 0),
        ("éa b", "0ww", b"\xc3\xa9a Xb", 0),
        ("abc def", "0ww", b"abc deXf", 0),
        ("a! b", "0w", b"aX! b", 0),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}iX\r");
        assert_bytewise_editing_bytes("{keys:?}", keys.as_bytes(), expected, bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for the Emacs word class.
///
/// GNU treats letters, decimal digits and letter-numbers as word characters,
/// but leaves underscores and other numeric symbols outside a word. The
/// accented-letter row is the control showing the Unicode word class still
/// applies.
#[test]
fn bash_and_sushline_emacs_word_class_excludes_underscores_and_number_symbols() {
    let rc = concat!(
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n"
    );
    let utf8 = [("LC_ALL", "C.UTF-8")];
    let utf8_cases: [(&str, &[u8]); 6] = [
        ("a\u{e9}b", b"a\xc3\xa9bX"),
        ("a\u{e9}b_cd", b"a\xc3\xa9bX_cd"),
        ("a_b", b"aX_b"),
        ("a\u{b2}b", b"aX\xc2\xb2b"),
        ("a\u{bc}b", b"aX\xc2\xbcb"),
        ("a\u{660}b", b"a\xd9\xa0bX"),
    ];
    for (line, expected) in utf8_cases {
        let keys = format!("{line}\x01\x1bfX\r");
        assert_bytewise_editing_bytes(line, keys.as_bytes(), expected, 0, rc, &utf8);
    }
    // The same word class must govern edits, not just motion: from line
    // start M-d deletes `aé b` and stops at the underscore.
    let keys = "a\u{e9}b_cd\x01\x1bdX\r";
    assert_bytewise_editing_bytes(
        "M-d at the accented word boundary",
        keys.as_bytes(),
        b"X_cd",
        0,
        rc,
        &utf8,
    );

    let c_locale = [("LC_ALL", "C")];
    for (line, expected) in [("a_b", b"aX_b".as_slice()), ("a1b", b"a1bX")] {
        let keys = format!("{line}\x01\x1bfX\r");
        assert_bytewise_editing_bytes(line, keys.as_bytes(), expected, 0, rc, &c_locale);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented uppercase `E`.
///
/// With `byte-oriented on`, `E` consumes raw non-whitespace bytes, so a final
/// multibyte scalar can be removed or traversed bytewise, unlike the UTF-8
/// grapheme path.
#[test]
fn bash_and_sushline_byte_oriented_end_bigword_is_bytewise() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C.UTF-8")];
    let cases: [(&str, &[u8], usize); 5] = [
        ("0dE", b"", 0),
        ("0cEZ", b"Z", 0),
        ("0yEP", b"a\xc3\xa9a\xc3\xa9", 0),
        ("0EiX", b"a\xc3X\xa9", 0),
        ("02EiX", b"a\xc3X\xa9", 0),
    ];
    for (motion, expected, bells) in cases {
        let keys = format!("a\u{e9}\x1b{motion}\r");
        assert_bytewise_editing_bytes(motion, keys.as_bytes(), expected, bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented uppercase `B`.
///
/// In byte mode `B` treats raw non-whitespace bytes as one bigword, including
/// continuation bytes, and lands at the start of the line. The prefix cases
/// set the point on the last byte, a continuation byte and a lead byte; the
/// `0` prefix starts at the beginning so `B` fails with a bell.
#[test]
fn bash_and_sushline_byte_oriented_backward_bigword_is_bytewise() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C.UTF-8")];
    let expected = b"Xa\xc3\xa9a\xc3\xa9";
    for (prefix, motion, bells) in [
        ("", "BiX", 0),
        ("h", "BiX", 0),
        ("hh", "BiX", 0),
        ("", "2BiX", 0),
        ("0", "BiX", 1),
    ] {
        let keys = format!("a\u{e9}a\u{e9}\x1b{prefix}{motion}\r");
        assert_bytewise_editing_bytes(
            &format!("{prefix}+{motion}"),
            keys.as_bytes(),
            expected,
            bells,
            rc,
            &env,
        );
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented vi ESC backoff.
///
/// Leaving insert mode steps one raw byte, so a typed multibyte character
/// leaves point on its last byte. The fixture covers the resulting insert
/// points, failed-word bells and repeated/counted word forms.
#[test]
fn bash_and_sushline_byte_oriented_escape_backoff_matches_gnu() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let cases: [(&str, &str, &[u8], usize); 15] = [
        ("aé", "i", b"a\xc3X\xa9", 0),
        ("aé", "wi", b"a\xc3X\xa9", 1),
        ("aé", "Wi", b"a\xc3X\xa9", 1),
        ("aé", "0wi", b"aX\xc3\xa9", 0),
        ("aé", "0Wi", b"a\xc3X\xa9", 0),
        ("aé", "0wwi", b"a\xc3X\xa9", 0),
        ("aé", "02wi", b"a\xc3X\xa9", 0),
        ("aé", "0WWi", b"a\xc3X\xa9", 1),
        ("aé b", "0wi", b"aX\xc3\xa9 b", 0),
        ("aé b", "0wwi", b"a\xc3\xa9 Xb", 0),
        ("éa", "wi", b"\xc3\xa9Xa", 1),
        ("éa b", "0wi", b"\xc3\xa9Xa b", 0),
        ("abc", "i", b"abXc", 0),
        ("abc", "wi", b"abXc", 1),
        ("abc", "0wwi", b"abXc", 1),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}X\r");
        assert_bytewise_editing_bytes("{keys:?}", keys.as_bytes(), expected, bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented backward word points.
///
/// In `LC_ALL=C` with `byte-oriented on`, `b`/`B` step one raw byte before
/// scanning and snap to a byte-sequence lead. High bytes are punctuation for
/// `b` and non-whitespace for `B`. Pending delete ranges and pure-ASCII
/// counted bells are measured elsewhere; this fixture pins insert points and
/// bells for the backward motions themselves.
#[test]
fn bash_and_sushline_byte_oriented_backward_word_points() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let cases: [(&str, &str, &[u8], usize); 31] = [
        ("aé b", "b", b"Xa\xc3\xa9 b", 0),
        ("aé b", "B", b"Xa\xc3\xa9 b", 0),
        ("aé b", "bb", b"Xa\xc3\xa9 b", 1),
        ("aé b", "2b", b"Xa\xc3\xa9 b", 0),
        ("aé b", "$b", b"Xa\xc3\xa9 b", 0),
        ("aé b", "0b", b"Xa\xc3\xa9 b", 1),
        ("aé b", "3b", b"Xa\xc3\xa9 b", 0),
        ("aé b", "02b", b"Xa\xc3\xa9 b", 1),
        ("aé b", "0B", b"Xa\xc3\xa9 b", 1),
        ("éa b", "b", b"\xc3\xa9Xa b", 0),
        ("éa b", "B", b"X\xc3\xa9a b", 0),
        ("éa b", "bb", b"X\xc3\xa9a b", 0),
        ("éa b", "2b", b"X\xc3\xa9a b", 0),
        ("éa b", "$b", b"\xc3\xa9Xa b", 0),
        ("éa b", "0b", b"X\xc3\xa9a b", 1),
        ("éa b", "3b", b"X\xc3\xa9a b", 0),
        ("éa b", "02b", b"X\xc3\xa9a b", 1),
        ("ab é", "b", b"ab X\xc3\xa9", 0),
        ("ab é", "B", b"ab X\xc3\xa9", 0),
        ("ab é", "bb", b"Xab \xc3\xa9", 0),
        ("ab é", "$b", b"ab X\xc3\xa9", 0),
        ("ab é", "0b", b"Xab \xc3\xa9", 1),
        ("ab é", "3b", b"Xab \xc3\xa9", 0),
        ("ab é", "0B", b"Xab \xc3\xa9", 1),
        ("é", "b", b"X\xc3\xa9", 0),
        ("é", "B", b"X\xc3\xa9", 0),
        ("é a", "$b", b"X\xc3\xa9 a", 0),
        ("é a", "$B", b"X\xc3\xa9 a", 0),
        ("abc def", "$b", b"abc Xdef", 0),
        ("abc def", "$bb", b"Xabc def", 0),
        ("a! b", "$b", b"Xa! b", 0),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}iX\r");
        assert_bytewise_editing_bytes("{keys:?}", keys.as_bytes(), expected, bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented backward operator ranges.
///
/// The backward motion point and the delete/yank range agree for these forms;
/// a following insert or put exposes the point left by the operator. Yanking
/// with `y` and putting with `p` has a separate point difference on both
/// byte-oriented and UTF-8 lines, so it is not part of this fixture.
#[test]
fn bash_and_sushline_byte_oriented_backward_operator_ranges() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let cases: [(&str, &str, &[u8], usize); 16] = [
        ("abc def", "$db", b"abc f", 0),
        ("abc def", "$dBiX", b"abc Xf", 0),
        ("abc def", "$dbiX", b"abc Xf", 0),
        ("abc def", "$dbp", b"abc fde", 0),
        ("aé b", "db", b"b", 0),
        ("aé b", "dBiX", b"Xb", 0),
        ("aé b", "dbiX", b"Xb", 0),
        ("aé b", "dbp", b"ba\xc3\xa9 ", 0),
        ("éa b", "db", b"\xc3\xa9b", 0),
        ("éa b", "dBiX", b"Xb", 0),
        ("éa b", "dbiX", b"\xc3\xa9Xb", 0),
        ("éa b", "dbp", b"\xc3\xa9ba ", 0),
        ("ab é", "db", b"ab \xa9", 0),
        ("ab é", "dBiX", b"ab X\xa9", 0),
        ("ab é", "dbiX", b"ab X\xa9", 0),
        ("ab é", "dbp", b"ab \xa9\xc3", 0),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}\r");
        assert_bytewise_editing_bytes("{keys:?}", keys.as_bytes(), expected, bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented vi `x` and `X`.
///
/// In `LC_ALL=C` with `byte-oriented on`, each delete removes one raw byte
/// and leaves point on that byte. A delete at end of line backs off one raw
/// byte; `X` rings only when it removed nothing, while charwise mode rings
/// when a count cannot be satisfied. Following `p` has a separate yank-point
/// difference and is not part of this fixture.
#[test]
fn bash_and_sushline_byte_oriented_vi_delete_char() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let cases: [(&str, &str, &[u8], usize); 36] = [
        ("éa", "X", b"\xc3Za", 0),
        ("éa", "x", b"\xc3Z\xa9", 0),
        ("éa", "0X", b"Z\xc3\xa9a", 1),
        ("éa", "0x", b"Z\xa9a", 0),
        ("aé", "X", b"aZ\xa9", 0),
        ("aé", "x", b"aZ\xc3", 0),
        ("aé", "0X", b"Za\xc3\xa9", 1),
        ("aé", "0x", b"Z\xc3\xa9", 0),
        ("é", "X", b"Z\xa9", 0),
        ("é", "x", b"Z\xc3", 0),
        ("é", "0X", b"Z\xc3\xa9", 1),
        ("é", "0x", b"Z\xa9", 0),
        ("aé b", "0X", b"Za\xc3\xa9 b", 1),
        ("aé b", "0x", b"Z\xc3\xa9 b", 0),
        ("aé b", "0XX", b"Za\xc3\xa9 b", 2),
        ("aé b", "0xx", b"Z\xa9 b", 0),
        ("abc", "X", b"aZc", 0),
        ("abc", "x", b"aZb", 0),
        ("abc", "0X", b"Zabc", 1),
        ("abc", "0x", b"Zbc", 0),
        ("日本", "X", b"\xe6\x97\xa5\xe6Z\xac", 0),
        ("日本", "x", b"\xe6\x97\xa5\xe6Z\x9c", 0),
        ("日本", "0X", b"Z\xe6\x97\xa5\xe6\x9c\xac", 1),
        ("日本", "0x", b"Z\x97\xa5\xe6\x9c\xac", 0),
        ("abc", "3X", b"Zc", 0),
        ("abc", "4X", b"Zc", 0),
        ("a", "2X", b"Za", 1),
        ("", "X", b"Z", 1),
        ("", "2X", b"Z", 1),
        ("abc", "3x", b"aZb", 0),
        ("abc", "4x", b"aZb", 0),
        ("", "2x", b"Z", 1),
        ("éa b", "4X", b"Zb", 0),
        ("éa b", "5X", b"Zb", 0),
        ("éa b", "4x", b"\xc3\xa9aZ ", 0),
        ("éa b", "5x", b"\xc3\xa9aZ ", 0),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}iZ\r");
        assert_bytewise_editing_bytes("{keys:?}", keys.as_bytes(), expected, bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for repeated byte-oriented `b` bells.
///
/// In `LC_ALL=C` with `byte-oriented on`, a failed first step rings, but later
/// counted or repeated steps that run out of line stay silent. The fixture
/// pins those bells with the insert point left by the successful steps, and
/// includes punctuation-run and single-character-word controls.
#[test]
fn bash_and_sushline_byte_oriented_repeated_backward_bells() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let cases: [(&str, &str, &[u8], usize); 22] = [
        ("abc def ghi", "$b", b"abc def Xghi", 0),
        ("abc def ghi", "$bb", b"abc Xdef ghi", 0),
        ("abc def ghi", "$bbb", b"Xabc def ghi", 0),
        ("abc def ghi", "4b", b"Xabc def ghi", 0),
        ("a! b c", "$b", b"a! Xb c", 0),
        ("a! b c", "$bb", b"Xa! b c", 0),
        ("a! b c", "$bbb", b"Xa! b c", 1),
        ("a! b c", "4b", b"Xa! b c", 0),
        ("abc def", "$b", b"abc Xdef", 0),
        ("abc def", "$bb", b"Xabc def", 0),
        ("abc def", "$bbb", b"Xabc def", 1),
        ("abc def", "3b", b"Xabc def", 0),
        ("a b", "$b", b"Xa b", 0),
        ("a b", "$bb", b"Xa b", 1),
        ("a b", "$bbb", b"Xa b", 2),
        ("one", "$b", b"Xone", 0),
        ("one", "$bb", b"Xone", 1),
        ("one", "0b", b"Xone", 1),
        ("one", "2b", b"Xone", 0),
        ("a! b", "$b", b"Xa! b", 0),
        ("a! b", "$bb", b"Xa! b", 1),
        ("a! b", "$bbb", b"Xa! b", 2),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}iX\r");
        assert_bytewise_editing_bytes(&keys, keys.as_bytes(), expected, bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for the point left by vi yank.
///
/// GNU keeps point where the operator was armed: a forward yank leaves it
/// at the yank start, while a backward yank leaves it at the original
/// cursor. A following insert or put exposes that point. Delete and change
/// ranges are not affected by this point rule.
#[test]
fn bash_and_sushline_vi_backward_yank_point() {
    let rc = "set editing-mode vi\n";
    let cases = [
        ("abc def", "$ybiX", "abc deXf", 0),
        ("abc def", "$yBiX", "abc deXf", 0),
        ("abc def", "$ywiX", "abc deXf", 0),
        ("abc def", "$ybP", "abc dedef", 0),
        ("abc def", "$ybp", "abc defde", 0),
        ("abc def ghi", "$ybiX", "abc def ghXi", 0),
        ("abc def ghi", "$ybbiX", "abc def Xghi", 0),
        ("abc def ghi", "$2ybiX", "abc def ghXi", 0),
        ("abc def ghi", "$yBwiX", "abc def ghXi", 1),
        ("abc def ghi", "0ywiX", "Xabc def ghi", 0),
        ("abc def ghi", "0ywbp", "aabc bc def ghi", 1),
        ("abc def ghi", "$ybp", "abc def ghigh", 0),
        ("abc def ghi", "$ybP", "abc def ghghi", 0),
        ("a! b c", "$ybiX", "a! b Xc", 0),
        ("a! b c", "$ybbiX", "a! Xb c", 0),
        ("a! b c", "$ybp", "a! b cb ", 0),
        ("one", "$ybiX", "onXe", 0),
        ("one", "0ybiX", "Xone", 1),
        ("aé b", "ybiX", "aé Xb", 0),
        ("aé b", "ywiX", "aé Xb", 0),
        ("aé b", "ybp", "aé baé ", 0),
        ("aé b", "ybP", "aé aé b", 0),
        ("aé b c", "$ybiX", "aé b Xc", 0),
        ("aé b c", "$ybp", "aé b cb ", 0),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}\r");
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), rc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
        assert_eq!(accepted_line(&bash), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(accepted_line(&sush), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(bell_count(&bash), bells, "{keys:?}");
        assert_eq!(bell_count(&sush), bells, "{keys:?}");
    }

    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let byte_cases: [(&str, &str, &[u8], usize); 7] = [
        ("aé b", "ybiX", b"a\xc3\xa9 Xb", 0),
        ("aé b", "ybp", b"a\xc3\xa9 ba\xc3\xa9 ", 0),
        ("éa b", "ybiX", b"\xc3\xa9a Xb", 0),
        ("éa b", "ybp", b"\xc3\xa9a ba ", 0),
        ("ab é", "ybiX", b"ab \xc3X\xa9", 0),
        ("ab é", "ybp", b"ab \xc3\xa9\xc3", 0),
        ("abc def", "$ybp", b"abc defde", 0),
    ];
    for (line, motion, expected, bells) in byte_cases {
        let keys = format!("{line}\x1b{motion}\r");
        assert_bytewise_editing_bytes(&keys, keys.as_bytes(), expected, bells, byte_rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for vi put after `x` and `X`.
///
/// Both deletes fill the unnamed register: `X` kills the character before
/// point and `x` the character at point, so a following `p` restores it
/// before the inserted marker. In byte-oriented mode the put leaves point
/// on the last raw byte, even when that byte is a UTF-8 continuation byte.
#[test]
fn bash_and_sushline_vi_put_after_delete() {
    let rc = "set editing-mode vi\n";
    let cases = [
        ("abc", "xp", "abZc", 0),
        ("abc", "Xp", "acZb", 0),
        ("abc", "xP", "aZcb", 0),
        ("abc", "XP", "aZbc", 0),
        ("abc", "xxp", "aZb", 0),
        ("abc", "XXp", "cZa", 0),
        ("abc", "0xp", "bZac", 0),
        ("abc", "0Xp", "aZbc", 2),
        ("abc", "2xp", "abZc", 0),
        ("abc", "2Xp", "caZb", 0),
        ("abc def", "xp", "abc deZf", 0),
        ("abc def", "Xp", "abc dfZe", 0),
        ("éa", "xp", "éZa", 0),
        ("éa", "Xp", "aZé", 0),
        ("aé", "xp", "aZé", 0),
        ("aé", "Xp", "éZa", 0),
        ("é", "xp", "Zé", 0),
        ("é", "Xp", "éZ", 2),
        ("日本", "xp", "日Z本", 0),
        ("日本", "Xp", "本Z日", 0),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}iZ\r");
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), rc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
        assert_eq!(accepted_line(&bash), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(accepted_line(&sush), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(bell_count(&bash), bells, "{keys:?}");
        assert_eq!(bell_count(&sush), bells, "{keys:?}");
    }

    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let byte_cases: [(&str, &str, &[u8], usize); 13] = [
        ("abc", "xp", b"abZc", 0),
        ("abc", "Xp", b"acZb", 0),
        ("abc", "xP", b"aZcb", 0),
        ("abc", "XP", b"aZbc", 0),
        ("abc", "2Xp", b"caZb", 0),
        ("éa", "xp", b"\xc3\xa9Za", 0),
        ("éa", "Xp", b"\xc3aZ\xa9", 0),
        ("aé", "xp", b"a\xc3Z\xa9", 0),
        ("aé", "Xp", b"a\xa9Z\xc3", 0),
        ("é", "xp", b"\xc3Z\xa9", 0),
        ("é", "Xp", b"\xa9Z\xc3", 0),
        ("日本", "xp", b"\xe6\x97\xa5\xe6\x9cZ\xac", 0),
        ("日本", "Xp", b"\xe6\x97\xa5\xe6\xacZ\x9c", 0),
    ];
    for (line, motion, expected, bells) in byte_cases {
        let keys = format!("{line}\x1b{motion}iZ\r");
        assert_bytewise_editing_bytes(&keys, keys.as_bytes(), expected, bells, byte_rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for vi change case.
///
/// GNU rings once per application on an uncased character and leaves it
/// unchanged. A cased character with only a multi-character mapping, such as
/// `ß`, is also left alone but stays silent. Byte-oriented mode sees raw
/// bytes: only ASCII letters toggle, a high byte is never replaced, and a
/// successful toggle moves point one byte when the line continues.
#[test]
fn bash_and_sushline_vi_change_case() {
    let rc = "set editing-mode vi\n";
    let cases = [
        ("", "~", "Z", 0),
        ("a", "~", "ZA", 0),
        ("1", "~", "Z1", 1),
        ("!", "~", "Z!", 1),
        (" ", "~", "Z ", 1),
        ("abc", "~", "abZC", 0),
        ("abc", "2~", "abZc", 0),
        ("abc", "3~", "abZC", 0),
        ("abc", "4~", "abZc", 0),
        ("abc", "0~", "AZbc", 0),
        ("abc", "0~.", "ABZc", 0),
        ("abc", "1~", "abZC", 0),
        ("abc", "01~", "AZbc", 0),
        ("a1b", "0~", "AZ1b", 0),
        ("a1b", "2~", "a1Zb", 0),
        ("é", "~", "ZÉ", 0),
        ("é", "2~", "Zé", 0),
        ("é", "~~", "Zé", 0),
        ("é", "~.", "Zé", 0),
        ("é", "u~", "ZÉ", 1),
        ("aé", "~", "aZÉ", 0),
        ("aé", "0~", "AZé", 0),
        ("aé", "2~", "aZé", 0),
        ("ß", "~", "Zß", 0),
        ("ﬁ", "~", "Zﬁ", 0),
        ("Ä", "~", "Zä", 0),
        ("日本", "~", "日Z本", 1),
        ("日本", "2~", "日Z本", 2),
        ("日本", "3~", "日Z本", 3),
        ("ǅ", "~", "Zǆ", 0),
        ("ῼ", "~", "Zῳ", 0),
        ("α", "~", "ZΑ", 0),
        ("я", "~", "ZЯ", 0),
        ("א", "~", "Zא", 1),
        ("😀", "~", "Z😀", 1),
        ("ñ", "~", "ZÑ", 0),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}iZ\r");
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), rc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
        assert_eq!(accepted_line(&bash), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(accepted_line(&sush), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(bell_count(&bash), bells, "{keys:?}");
        assert_eq!(bell_count(&sush), bells, "{keys:?}");
    }

    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let byte_cases: [(&str, &str, &[u8], usize); 36] = [
        ("", "~", b"Z", 0),
        ("a", "~", b"ZA", 0),
        ("1", "~", b"Z1", 1),
        ("!", "~", b"Z!", 1),
        (" ", "~", b"Z ", 1),
        ("abc", "~", b"abZC", 0),
        ("abc", "2~", b"abZc", 0),
        ("abc", "3~", b"abZC", 0),
        ("abc", "4~", b"abZc", 0),
        ("abc", "0~", b"AZbc", 0),
        ("abc", "0~.", b"ABZc", 0),
        ("abc", "1~", b"abZC", 0),
        ("abc", "01~", b"AZbc", 0),
        ("a1b", "0~", b"AZ1b", 0),
        ("a1b", "2~", b"a1Zb", 0),
        ("é", "~", b"\xc3Z\xa9", 1),
        ("é", "2~", b"\xc3Z\xa9", 2),
        ("é", "~~", b"\xc3Z\xa9", 2),
        ("é", "~.", b"\xc3Z\xa9", 2),
        ("é", "u~", b"\xc3Z\xa9", 2),
        ("aé", "~", b"a\xc3Z\xa9", 1),
        ("aé", "0~", b"AZ\xc3\xa9", 0),
        ("aé", "2~", b"a\xc3Z\xa9", 2),
        ("ß", "~", b"\xc3Z\x9f", 1),
        ("ﬁ", "~", b"\xef\xacZ\x81", 1),
        ("Ä", "~", b"\xc3Z\x84", 1),
        ("日本", "~", b"\xe6\x97\xa5\xe6\x9cZ\xac", 1),
        ("日本", "2~", b"\xe6\x97\xa5\xe6\x9cZ\xac", 2),
        ("日本", "3~", b"\xe6\x97\xa5\xe6\x9cZ\xac", 3),
        ("ǅ", "~", b"\xc7Z\x85", 1),
        ("ῼ", "~", b"\xe1\xbfZ\xbc", 1),
        ("α", "~", b"\xceZ\xb1", 1),
        ("я", "~", b"\xd1Z\x8f", 1),
        ("א", "~", b"\xd7Z\x90", 1),
        ("😀", "~", b"\xf0\x9f\x98Z\x80", 1),
        ("ñ", "~", b"\xc3Z\xb1", 1),
    ];
    for (line, motion, expected, bells) in byte_cases {
        let keys = format!("{line}\x1b{motion}iZ\r");
        assert_bytewise_editing_bytes(&keys, keys.as_bytes(), expected, bells, byte_rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for counted vi substitute bells.
///
/// In byte-oriented mode GNU rings once when a count exceeds the characters
/// that can be deleted, even though some were removed; UTF-8 mode stays
/// silent for the same forms. An empty line rings once in both modes.
#[test]
fn bash_and_sushline_byte_oriented_vi_subst_count_bell() {
    let rc = "set editing-mode vi\n";
    let cases = [
        ("abc", "sZZ\x1b", "abZZZ", 0),
        ("abc", "2sZZ\x1b", "abZZZ", 0),
        ("abc", "3sZZ\x1b", "abZZZ", 0),
        ("abc", "4sZZ\x1b", "abZZZ", 0),
        ("abc", "0sZZ\x1b", "ZZZbc", 0),
        ("abc def", "0sZZ\x1b", "ZZZbc def", 0),
        ("abc def", "sZZ\x1b", "abc deZZZ", 0),
        ("abc def", "2sZZ\x1b", "abc deZZZ", 0),
        ("abc def", "3sZZ\x1b", "abc deZZZ", 0),
        ("abc def", "7sZZ\x1b", "abc deZZZ", 0),
        ("abc def", "0lsZZ\x1b", "aZZZc def", 0),
        ("abc def", "0l2sZZ\x1b", "aZZZ def", 0),
        ("", "sZZ\x1b", "ZZZ", 1),
        ("", "2sZZ\x1b", "ZZZ", 1),
        ("", "0sZZ\x1b", "ZZZ", 1),
        ("é", "sZZ\x1b", "ZZZ", 0),
        ("é", "2sZZ\x1b", "ZZZ", 0),
        ("é", "3sZZ\x1b", "ZZZ", 0),
        ("aé", "sZZ\x1b", "aZZZ", 0),
        ("aé", "2sZZ\x1b", "aZZZ", 0),
        ("aé", "3sZZ\x1b", "aZZZ", 0),
        ("日本", "sZZ\x1b", "日ZZZ", 0),
        ("日本", "2sZZ\x1b", "日ZZZ", 0),
        ("日本", "3sZZ\x1b", "日ZZZ", 0),
    ];
    for (line, motion, expected, bells) in cases {
        let keys = format!("{line}\x1b{motion}iZ\r");
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), rc);
        let sush = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
        assert_eq!(accepted_line(&bash), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(accepted_line(&sush), Some(expected.to_owned()), "{keys:?}");
        assert_eq!(bell_count(&bash), bells, "{keys:?}");
        assert_eq!(bell_count(&sush), bells, "{keys:?}");
    }

    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let byte_cases: [(&str, &str, &[u8], usize); 24] = [
        ("abc", "sZZ\x1b", b"abZZZ", 0),
        ("abc", "2sZZ\x1b", b"abZZZ", 1),
        ("abc", "3sZZ\x1b", b"abZZZ", 1),
        ("abc", "4sZZ\x1b", b"abZZZ", 1),
        ("abc", "0sZZ\x1b", b"ZZZbc", 0),
        ("abc def", "0sZZ\x1b", b"ZZZbc def", 0),
        ("abc def", "sZZ\x1b", b"abc deZZZ", 0),
        ("abc def", "2sZZ\x1b", b"abc deZZZ", 1),
        ("abc def", "3sZZ\x1b", b"abc deZZZ", 1),
        ("abc def", "7sZZ\x1b", b"abc deZZZ", 1),
        ("abc def", "0lsZZ\x1b", b"aZZZc def", 0),
        ("abc def", "0l2sZZ\x1b", b"aZZZ def", 0),
        ("", "sZZ\x1b", b"ZZZ", 1),
        ("", "2sZZ\x1b", b"ZZZ", 1),
        ("", "0sZZ\x1b", b"ZZZ", 1),
        ("é", "sZZ\x1b", b"\xc3ZZZ", 0),
        ("é", "2sZZ\x1b", b"\xc3ZZZ", 1),
        ("é", "3sZZ\x1b", b"\xc3ZZZ", 1),
        ("aé", "sZZ\x1b", b"a\xc3ZZZ", 0),
        ("aé", "2sZZ\x1b", b"a\xc3ZZZ", 1),
        ("aé", "3sZZ\x1b", b"a\xc3ZZZ", 1),
        ("日本", "sZZ\x1b", b"\xe6\x97\xa5\xe6\x9cZZZ", 0),
        ("日本", "2sZZ\x1b", b"\xe6\x97\xa5\xe6\x9cZZZ", 1),
        ("日本", "3sZZ\x1b", b"\xe6\x97\xa5\xe6\x9cZZZ", 1),
    ];
    for (line, motion, expected, bells) in byte_cases {
        let keys = format!("{line}\x1b{motion}iZ\r");
        assert_bytewise_editing_bytes(&keys, keys.as_bytes(), expected, bells, byte_rc, &env);
    }

    // An unmet bytewise count rings, but the deleted bytes must still be
    // available to `p`; an empty ring would ring on the paste as well.
    for (desc, keys, expected, bells) in [
        ("unmet_paste", "abcde\x1b010sX\x1bp\r", "Xabcde", 1),
        (
            "unmet_paste_twice",
            "abcde\x1b010sX\x1bpp\r",
            "Xabcdeabcde",
            1,
        ),
        ("met_paste_control", "abcde\x1b05sX\x1bp\r", "Xabcde", 0),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_env(keys.as_bytes(), byte_rc, &env);
        let sush = run_sushline_harness_with_inputrc_and_env(keys.as_bytes(), byte_rc, &env);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sush),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sush}"
        );
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sush}"
        );
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for the default vi `v` boundary.
///
/// GNU invokes the external editor on a temporary file holding the current
/// line; hookless Sushline only rings. The deterministic editor logs the
/// bytes it was handed and replaces them with `:`, so the fixture checks the
/// application boundary without adding a launcher to the library.
#[cfg(unix)]
#[test]
fn bash_and_sushline_vi_default_v_application_boundary() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("editor.sh");
    let log = dir.path().join("seen");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\ncat \"$1\" > \"{}\"\nprintf ':' > \"$1\"\n",
            log.display()
        ),
    )
    .expect("write editor");
    let mut perms = fs::metadata(&script)
        .expect("editor metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).expect("editor chmod");
    let editor = script.to_string_lossy().to_string();
    let env = [("VISUAL", editor.as_str()), ("EDITOR", editor.as_str())];
    let keys = b"abc\x1b0v\r";
    let inputrc = "set editing-mode vi\n";

    let bash = run_bash_readline_with_inputrc_file_and_env(keys, inputrc, &env);
    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(bell_count(&bash), 0, "{bash}");
    assert_eq!(
        fs::read_to_string(&log).expect("editor log"),
        "abc\n",
        "GNU should hand the current line to the editor"
    );

    let sushline = run_sushline_harness_with_inputrc_and_env(keys, inputrc, &env);
    assert_eq!(
        accepted_line(&sushline),
        Some("abc".to_owned()),
        "{sushline}"
    );
    assert_eq!(bell_count(&sushline), 1, "{sushline}");
}

/// Paired GNU 5.3 / Readline 8.3 pin for vi `f`/`F`/`t`/`T` counts, repeated
/// searches, and the direction alternation of `;` and `,`.
#[test]
fn bash_and_sushline_vi_char_search_counts_and_repeats() {
    let inputrc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("f", "abcabcabc\x1b0fcx\r", "ababcabc", 0),
        ("f_two", "abcabcabc\x1b02fcx\r", "abcababc", 0),
        ("f_three", "abcabcabc\x1b03fcx\r", "abcabcab", 0),
        ("f_short_count", "abcabcabc\x1b04fax\r", "abcabcbc", 1),
        ("F", "abcabcabc\x1b$Fcx\r", "abcababc", 0),
        ("F_two", "abcabcabc\x1b$2Fcx\r", "ababcabc", 0),
        ("F_short_count", "abcabcabc\x1b$3Fcx\r", "ababcabc", 1),
        ("t", "abcabcabc\x1b0tcx\r", "acabcabc", 0),
        ("t_two", "abcabcabc\x1b02tcx\r", "abcacabc", 0),
        ("t_three", "abcabcabc\x1b03tcx\r", "abcabcac", 0),
        ("t_short_count", "abcabcabc\x1b04tcx\r", "abcabcac", 1),
        ("T", "abcabcabc\x1b$Tcx\r", "abcabcbc", 0),
        ("T_two", "abcabcabc\x1b$2Tcx\r", "abcbcabc", 0),
        ("T_short_count", "abcabcabc\x1b$3Tcx\r", "abcbcabc", 1),
        ("f_semi", "abcabc\x1b0fa;x\r", "abcbc", 1),
        ("f_comma", "abcabc\x1b0fc,x\r", "ababc", 1),
        ("f_two_semis", "abcabcabc\x1b0fa2;x\r", "abcabcbc", 1),
        ("f_two_commas", "abcabcabc\x1b0fc2,x\r", "ababcabc", 1),
        ("t_semi", "abcabc\x1b0tax;x\r", "abbc", 1),
        ("t_semi_again", "abcabcabc\x1b0ta;x\r", "ababcabc", 0),
        ("t_semi_comma", "abcabc\x1b0tc;,iX\r", "aXbcabc", 1),
        ("t_two_semis", "abcabcabcabc\x1b0ta2;x\r", "abcababcabc", 0),
        ("t_two_semis_short", "abcabcabc\x1b0ta2;x\r", "abcababc", 0),
        (
            "t_three_semis",
            "abcabcabcabc\x1b0ta3;x\r",
            "abcabcababc",
            0,
        ),
        (
            "t_four_semis",
            "abcabcabcabcabc\x1b0ta4;x\r",
            "abcabcabcababc",
            0,
        ),
        (
            "t_counted_search_semi",
            "abcabcabc\x1b02ta;x\r",
            "abcababc",
            0,
        ),
        ("t_two_commas", "abcabcabcabc\x1b$ta2,x\r", "abcabcacabc", 1),
        ("T_two_semis", "abcabcabcabc\x1b$Ta2;x\r", "abcabcacabc", 0),
        (
            "f_two_semis_long",
            "abcabcabcabc\x1b0fa2;x\r",
            "abcabcabcbc",
            0,
        ),
        ("t_comma", "abcabc\x1b0tc,x\r", "acabc", 1),
        ("T_semi", "abcabc\x1b$Tax;x\r", "abca", 0),
        ("T_comma", "abcabc\x1b$Tcx,x\r", "abcc", 0),
        ("semi_no_search", "abc\x1b0;x\r", "bc", 0),
        ("comma_no_search", "abc\x1b0,x\r", "bc", 0),
        ("failed_f_then_semi", "abcabc\x1b0fq;x\r", "bcabc", 2),
        ("failed_f_then_comma", "abcqab\x1b$fq,x\r", "abcab", 1),
        ("failed_f_then_two_commas", "abcqab\x1b$fq,,x\r", "abcab", 2),
        ("failed_t_then_comma", "abcqab\x1b$tq,x\r", "abcqb", 1),
        ("counted_f_dot_replay", "abcabc\x1b02fax\x1b.\r", "abcc", 1),
        ("plain_f_dot_replay", "abcabc\x1b0fax\x1b.\r", "abcc", 0),
        ("delete_with_f", "abcabc\x1b0dfax\r", "c", 0),
        ("change_with_t", "abcabc\x1b0dtax\r", "bc", 0),
        ("yank_with_f_then_put", "abcabc\x1b0yfap\r", "aabcabcabc", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), inputrc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for character search over multibyte
/// characters and over byte-oriented lines with one-byte targets.
#[test]
fn bash_and_sushline_vi_char_search_multibyte_and_byte_modes() {
    let utf8_rc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        (
            "f_multibyte",
            "a\u{e9}b\u{e9}c\x1b0f\u{e9}x\r",
            "ab\u{e9}c",
            0,
        ),
        (
            "t_multibyte",
            "a\u{e9}b\u{e9}c\x1b0t\u{e9}x\r",
            "\u{e9}b\u{e9}c",
            0,
        ),
        (
            "f_count_multibyte",
            "a\u{e9}b\u{e9}c\u{e9}d\x1b02f\u{e9}x\r",
            "a\u{e9}bc\u{e9}d",
            0,
        ),
        (
            "t_first_multibyte",
            "\u{e9}a\u{e9}a\x1b0taiZ\r",
            "Z\u{e9}a\u{e9}a",
            0,
        ),
        (
            "t_first_x_multibyte",
            "\u{e9}a\u{e9}a\x1b0tax\r",
            "a\u{e9}a",
            0,
        ),
        (
            "t_semi_multibyte",
            "\u{e9}a\u{e9}a\x1b0ta;iZ\r",
            "Z\u{e9}a\u{e9}a",
            0,
        ),
        (
            "t_two_multibyte",
            "\u{e9}a\u{e9}a\x1b02ta iZ\r",
            "\u{e9}a\u{e9}Za",
            0,
        ),
        (
            "t_two_semis_multibyte",
            "\u{e9}a\u{e9}a\u{e9}a\x1b0ta2;iZ\r",
            "\u{e9}aZ\u{e9}a\u{e9}a",
            0,
        ),
        (
            "T_target_multibyte",
            "a\u{e9}a\u{e9}\x1b$T\u{e9} iZ\r",
            "a\u{e9}aZ\u{e9}",
            0,
        ),
        (
            "T_target_insert_multibyte",
            "a\u{e9}a\u{e9}\x1b$T\u{e9}iZ\r",
            "a\u{e9}Za\u{e9}",
            0,
        ),
        (
            "T_target_delete_multibyte",
            "a\u{e9}a\u{e9}\x1b$T\u{e9}x\r",
            "a\u{e9}\u{e9}",
            0,
        ),
        (
            "change_with_t_multibyte",
            "\u{e9}a\u{e9}a\x1b0dt\u{e9}\r",
            "\u{e9}a",
            0,
        ),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), utf8_rc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), utf8_rc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }

    // In byte-oriented mode a multibyte search target contributes only its
    // first raw byte; the remaining bytes are redispatched as input.
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    for (desc, keys, expected, bells) in [
        (
            "byte_f_ascii",
            "a\u{e9}bc\u{e9}d\x1b0fcx\r",
            &b"a\xc3\xa9b\xc3\xa9d"[..],
            0,
        ),
        (
            "byte_F_ascii",
            "a\u{e9}bc\u{e9}d\x1b$Fcx\r",
            &b"a\xc3\xa9b\xc3\xa9d"[..],
            0,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected, bells, byte_rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented multibyte search keys.
///
/// Readline consumes only the first raw byte of a multibyte search target;
/// the remaining bytes reach vi command mode as unbound keys and ring.
#[test]
fn bash_and_sushline_vi_char_search_byte_oriented_multibyte_keys() {
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    let kanji = "日".as_bytes();
    for (desc, keys, expected, bells) in [
        (
            "f_e_accent",
            [b"abc\x1b0f".as_slice(), e, b"X\r"].concat(),
            &b"abc"[..],
            3,
        ),
        (
            "f_kanji",
            [b"abc\x1b0f".as_slice(), kanji, b"X\r"].concat(),
            &b"abc"[..],
            4,
        ),
        (
            "F_e_accent",
            [b"abc\x1b$".as_slice(), e, b"\x1b0F", e, b"x\r"].concat(),
            &b"bc"[..],
            4,
        ),
        (
            "t_e_accent",
            [b"abc\x1b0t".as_slice(), e, b"x\r"].concat(),
            &b"bc"[..],
            2,
        ),
        (
            "T_e_accent",
            [b"abc\x1b$T".as_slice(), e, b"x\r"].concat(),
            &b"ab"[..],
            2,
        ),
        // A raw byte target finds that byte inside an otherwise valid scalar.
        (
            "f_first_raw_byte",
            [b"a".as_slice(), e, b"b\x1b0f", &[0xc3], b"x\r"].concat(),
            &b"a\xa9b"[..],
            0,
        ),
        (
            "f_second_raw_byte",
            [b"a".as_slice(), e, b"b\x1b0f", &[0xa9], b"x\r"].concat(),
            &b"a\xc3b"[..],
            0,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, &keys, expected, bells, byte_rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented multibyte input in
/// command-key paths other than character search. Numeric arguments, an
/// unbound quote prefix, quoted insert, incremental search, named commands
/// and macro prefixes already consume multibyte input byte by byte.
#[test]
fn bash_and_sushline_byte_oriented_command_key_boundary() {
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    let kanji = "日".as_bytes();
    let cases: Vec<(&str, Vec<u8>, &[u8], usize)> = vec![
        (
            "count_then_e",
            [b"abc\x1b03".as_slice(), e, b"x\r"].concat(),
            &b"bc"[..],
            2,
        ),
        (
            "count_then_kanji",
            [b"abc\x1b03".as_slice(), kanji, b"x\r"].concat(),
            &b"bc"[..],
            3,
        ),
        (
            "plain_e_then_x",
            [b"abc\x1b0".as_slice(), e, b"x\r"].concat(),
            &b"bc"[..],
            2,
        ),
        (
            "unbound_quote_then_e",
            [b"abc\x1b\"".as_slice(), e, b"x\r"].concat(),
            &b"ab"[..],
            3,
        ),
        (
            "quoted_insert_e",
            [b"ab\x16".as_slice(), e, b"cd\r"].concat(),
            &b"ab\xc3\xa9cd"[..],
            0,
        ),
        (
            "quoted_insert_kanji",
            [b"ab\x16".as_slice(), kanji, b"cd\r"].concat(),
            &b"ab\xe6\x97\xa5cd"[..],
            0,
        ),
        (
            "isearch_e",
            [b"abc\x12".as_slice(), e, b"\r"].concat(),
            &b"abc\xc3\xa9"[..],
            0,
        ),
        (
            "named_command_e",
            [b"abc\x1bX".as_slice(), e, b"\r"].concat(),
            &b"ac"[..],
            2,
        ),
        (
            "minus_e",
            [b"abc\x1b0-".as_slice(), e, b"x\r"].concat(),
            &b"bc"[..],
            2,
        ),
    ];
    for (desc, keys, expected, bells) in cases {
        assert_bytewise_editing_bytes(desc, &keys, expected, bells, byte_rc, &env);
    }

    let macro_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n",
        "\"\\C-x\\303\": \"ZZ\"\n",
        "\"\\303\\251\": \"YY\"\n",
        "\"\\C-xe\": \"QQ\"\n"
    );
    for (desc, keys, expected, bells) in [
        (
            "macro_prefix_first_byte",
            [b"ab\x1b0\x18".as_slice(), e, b"x\r"].concat(),
            &b"b"[..],
            3,
        ),
        (
            "macro_full_e",
            [b"ab\x1b0".as_slice(), e, b"x\r"].concat(),
            &b"b"[..],
            2,
        ),
        ("macro_ctrl_x_e", b"ab\x1b0\x18ex\r".to_vec(), &b"a"[..], 1),
    ] {
        assert_bytewise_editing_bytes(desc, &keys, expected, bells, macro_rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for byte-oriented edits after a search
/// lands on an interior byte of a multibyte scalar.
///
/// `x`, `r`, dot replay and undo already matched; `s` had to delete one raw
/// byte per count rather than the whole scalar at that point.
#[test]
fn bash_and_sushline_byte_oriented_search_edit_undo() {
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    let c3 = [0xc3u8];
    let a9 = [0xa9u8];
    for (desc, keys, expected, bells) in [
        (
            "x_dot",
            [b"a".as_slice(), e, b"b\x1b0f", &c3, b"x.\r"].concat(),
            &b"ab"[..],
            0,
        ),
        (
            "x_undo",
            [b"a".as_slice(), e, b"b\x1b0f", &c3, b"xu\r"].concat(),
            &b"a\xc3\xa9b"[..],
            0,
        ),
        (
            "x_dot_undo",
            [b"a".as_slice(), e, b"b\x1b0f", &c3, b"x.u\r"].concat(),
            &b"a\xa9b"[..],
            0,
        ),
        (
            "s_change",
            [b"a".as_slice(), e, b"b\x1b0f", &c3, b"sZ\x1b\r"].concat(),
            &b"aZ\xa9b"[..],
            0,
        ),
        (
            "s_dot",
            [b"a".as_slice(), e, b"b\x1b0f", &c3, b"sZ\x1b.\r"].concat(),
            &b"aZ\xa9b"[..],
            0,
        ),
        (
            "r_replace",
            [b"a".as_slice(), e, b"b\x1b0f", &a9, b"rA\r"].concat(),
            &b"a\xc3Ab"[..],
            0,
        ),
        (
            "s_lead_register_put",
            [b"a".as_slice(), e, b"b\x1b0f\xc3sZ\x1bp\r"].concat(),
            &b"a\x5a\xc3\xa9b"[..],
            0,
        ),
        (
            "s_lead_register_put_before",
            [b"a".as_slice(), e, b"b\x1b0f\xc3sZ\x1bP\r"].concat(),
            &b"a\xc3\x5a\xa9b"[..],
            0,
        ),
        (
            "s_continuation_register_put",
            [b"a".as_slice(), e, b"b\x1b0f\xa9sZ\x1bp\r"].concat(),
            &b"a\xc3\x5a\xa9b"[..],
            0,
        ),
        (
            "s_ascii_register_put",
            b"abc\x1b0fbsZ\x1bp\r".to_vec(),
            &b"a\x5abc"[..],
            0,
        ),
        (
            "t_x",
            [b"a".as_slice(), e, b"b\x1b0t", &c3, b"x\r"].concat(),
            &b"\xc3\xa9b"[..],
            0,
        ),
        (
            "count2_x",
            [b"ab".as_slice(), e, b"b\x1b02f", &c3, b"x\r"].concat(),
            &b"ab\xa9b"[..],
            1,
        ),
        (
            "x_undo_redo",
            [b"a".as_slice(), e, b"b\x1b0f", &c3, b"xu\x12\r"].concat(),
            &b"a\xc3\xa9b"[..],
            0,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, &keys, expected, bells, byte_rc, &env);
    }
}

/// Bytewise operators after a raw-byte search agree for character, word and
/// to-end deletions, yanks and puts. The change-word and change-case paths
/// are measured separately because their endpoints are not the same here.
#[test]
fn bash_and_sushline_byte_oriented_operators_after_raw_byte_search() {
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    let c3 = [0xc3u8];
    let prefix = || [b"a".as_slice(), e, b"b \x1b0f", &c3].concat();
    let continuation_prefix = || [b"a".as_slice(), e, b"b \x1b0f\xa9"].concat();
    for (desc, keys, expected) in [
        (
            "count_delete_to_end_at_lead",
            [prefix().as_slice(), b"2D\r"].concat(),
            &b"a"[..],
        ),
        (
            "count_change_to_end_at_lead",
            [prefix().as_slice(), b"2CZ\x1b\r"].concat(),
            &b"aZ"[..],
        ),
        (
            "count_yank_put_at_lead",
            [prefix().as_slice(), b"2Y$p\r"].concat(),
            &b"a\xc3\xa9b \xc3\xa9b "[..],
        ),
        (
            "count_substitute_to_end_at_lead",
            [prefix().as_slice(), b"2SZ\x1b\r"].concat(),
            &b"Z"[..],
        ),
        (
            "count_delete_to_end_at_continuation",
            [continuation_prefix().as_slice(), b"2D\r"].concat(),
            &b"a\xc3"[..],
        ),
        (
            "count_change_to_end_at_continuation",
            [continuation_prefix().as_slice(), b"2CZ\x1b\r"].concat(),
            &b"a\xc3Z"[..],
        ),
        (
            "count_yank_put_at_continuation",
            [continuation_prefix().as_slice(), b"2Y$p\r"].concat(),
            &b"a\xc3\xa9b \xa9b "[..],
        ),
        (
            "count_substitute_to_end_at_continuation",
            [continuation_prefix().as_slice(), b"2SZ\x1b\r"].concat(),
            &b"Z"[..],
        ),
        (
            "change",
            [prefix().as_slice(), b"cZ\x1b\r"].concat(),
            &b"a\xc3\xa9b "[..],
        ),
        (
            "change_count",
            [prefix().as_slice(), b"2cZ\x1b\r"].concat(),
            &b"a\xc3\xa9b "[..],
        ),
        (
            "change_word_count",
            [prefix().as_slice(), b"c2wZ\x1b\r"].concat(),
            &b"aZ"[..],
        ),
        (
            "delete",
            [prefix().as_slice(), b"dl\r"].concat(),
            &b"a\xa9b "[..],
        ),
        (
            "delete_count",
            [prefix().as_slice(), b"2dl\r"].concat(),
            &b"ab "[..],
        ),
        (
            "delete_word",
            [prefix().as_slice(), b"dw\r"].concat(),
            &b"ab "[..],
        ),
        (
            "delete_end",
            [prefix().as_slice(), b"d$\r"].concat(),
            &b"a"[..],
        ),
        (
            "yank_put",
            [prefix().as_slice(), b"ylp\r"].concat(),
            &b"a\xc3\xc3\xa9b "[..],
        ),
        (
            "yank_word_put",
            [prefix().as_slice(), b"ywp\r"].concat(),
            &b"a\xc3\xc3\xa9\xa9b "[..],
        ),
        (
            "change_to_end",
            [prefix().as_slice(), b"CZ\x1b\r"].concat(),
            &b"aZ"[..],
        ),
        (
            "delete_to_end",
            [prefix().as_slice(), b"D\r"].concat(),
            &b"a"[..],
        ),
    ] {
        assert_bytewise_editing_bytes(desc, &keys, expected, 0, byte_rc, &env);
    }
}

/// Byte-oriented `~` skips an uncased byte when another byte follows, even
/// inside a multibyte scalar; only an uncased final byte rings. The marker
/// distinguishes a silent skip from a bell with point left in place.
#[test]
fn bash_and_sushline_byte_oriented_change_case_skips_uncased_bytes() {
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    for (desc, keys, expected, bells) in [
        ("colon", b"a:b\x1b0f:~iZ\r".to_vec(), &b"a:Zb"[..], 0),
        ("colon_count", b"a:b\x1b0f:2~iZ\r".to_vec(), &b"a:ZB"[..], 0),
        (
            "colon_count_from_end",
            b"a:b\x1b2~iZ\r".to_vec(),
            &b"a:Zb"[..],
            0,
        ),
        ("cased", b"aZb\x1b0fZ~iQ\r".to_vec(), &b"azQb"[..], 0),
        (
            "lead",
            [b"a".as_slice(), e, b":b\x1b0f", &[0xc3u8], b"~iZ\r"].concat(),
            &b"a\xc3Z\xa9:b"[..],
            0,
        ),
        (
            "continuation",
            [b"a".as_slice(), e, b":b\x1b0f", &[0xa9u8], b"~iZ\r"].concat(),
            &b"a\xc3\xa9Z:b"[..],
            0,
        ),
        (
            "final_byte",
            [e, b"\x1b~iZ\r"].concat(),
            &b"\xc3Z\xa9"[..],
            1,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, &keys, expected, bells, byte_rc, &env);
    }
}

/// Byte-oriented `cw` and `ce` classify raw bytes: high bytes are
/// punctuation, so a scalar next to an ASCII word is changed separately and
/// a point inside it changes only the rest of that byte run. Counted `c2w`
/// keeps the same bytewise class boundaries.
#[test]
fn bash_and_sushline_byte_oriented_change_word_endpoints() {
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    for (desc, keys, expected) in [
        (
            "scalar_next_to_word",
            [b"a".as_slice(), e, b"b \x1b0f", &[0xc3u8], b"cwZ\x1b\r"].concat(),
            &b"aZb "[..],
        ),
        (
            "one_space",
            [b"a".as_slice(), e, b" b \x1b0f", &[0xc3u8], b"cwZ\x1b\r"].concat(),
            &b"aZ b "[..],
        ),
        (
            "two_spaces",
            [b"a".as_slice(), e, b"  b \x1b0f", &[0xc3u8], b"cwZ\x1b\r"].concat(),
            &b"aZ  b "[..],
        ),
        (
            "scalar_next_to_punctuation",
            [b"a".as_slice(), e, b":b\x1b0f", &[0xc3u8], b"cwZ\x1b\r"].concat(),
            &b"aZb"[..],
        ),
        (
            "continuation",
            [b"a".as_slice(), e, b":b\x1b0f", &[0xa9u8], b"cwZ\x1b\r"].concat(),
            &b"a\xc3Zb"[..],
        ),
        (
            "before_scalar",
            [b"a".as_slice(), e, b"b \x1b0cwZ\x1b\r"].concat(),
            &b"Z\xc3\xa9b "[..],
        ),
        (
            "count_next_to_word",
            [b"a".as_slice(), e, b"b \x1b0f", &[0xc3u8], b"c2wZ\x1b\r"].concat(),
            &b"aZ"[..],
        ),
        (
            "count_next_to_punctuation",
            [b"a".as_slice(), e, b":b\x1b0f", &[0xc3u8], b"c2wZ\x1b\r"].concat(),
            &b"aZ"[..],
        ),
        (
            "count_next_to_longer_word",
            [b"a".as_slice(), e, b"bc d\x1b0f", &[0xc3u8], b"c2wZ\x1b\r"].concat(),
            &b"aZ d"[..],
        ),
        (
            "count_from_continuation",
            [b"a".as_slice(), e, b":b\x1b0f", &[0xa9u8], b"c2wZ\x1b\r"].concat(),
            &b"a\xc3Z"[..],
        ),
        (
            "ascii_punctuation",
            b"a:b c\x1b0f:cwZ\x1b\r".to_vec(),
            &b"aZb c"[..],
        ),
        (
            "ascii_word",
            b"abc def\x1b0cwZ\x1b\r".to_vec(),
            &b"Z def"[..],
        ),
        (
            "change_end_next_to_word",
            [b"a".as_slice(), e, b"b \x1b0f", &[0xc3u8], b"ceZ\x1b\r"].concat(),
            &b"aZb "[..],
        ),
        (
            "change_end_next_to_punctuation",
            [b"a".as_slice(), e, b":b\x1b0f", &[0xc3u8], b"ceZ\x1b\r"].concat(),
            &b"aZb"[..],
        ),
        (
            "change_end_from_continuation",
            [b"a".as_slice(), e, b":b\x1b0f", &[0xa9u8], b"ceZ\x1b\r"].concat(),
            &b"a\xc3Zb"[..],
        ),
    ] {
        assert_bytewise_editing_bytes(desc, &keys, expected, 0, byte_rc, &env);
    }
}

/// Undo, redo and dot replay keep the raw-byte ranges changed by bytewise
/// change-word commands, including a point on an interior byte of a scalar.
#[test]
fn bash_and_sushline_byte_oriented_change_word_undo_redo() {
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    let c3 = [0xc3u8];
    let a9 = [0xa9u8];
    let search = |tail: &[u8], key: &[u8]| [b"a".as_slice(), e, tail, b"\x1b0f", key].concat();
    for (desc, keys, expected) in [
        (
            "cw_undo",
            [search(b"b ", &c3).as_slice(), b"cwZ\x1bu\r"].concat(),
            &b"a\xc3\xa9b "[..],
        ),
        (
            "cw_dot",
            [search(b"b ", &c3).as_slice(), b"cwZ\x1b.\r"].concat(),
            &b"aZ"[..],
        ),
        (
            "cw_redo",
            [search(b"b ", &c3).as_slice(), b"cwZ\x1bu\x12\r"].concat(),
            &b"a\xc3\xa9b "[..],
        ),
        (
            "c2w_undo",
            [search(b"b ", &c3).as_slice(), b"c2wZ\x1bu\r"].concat(),
            &b"a\xc3\xa9b "[..],
        ),
        (
            "c2w_dot",
            [search(b"b ", &c3).as_slice(), b"c2wZ\x1b.\r"].concat(),
            &b"aZ"[..],
        ),
        (
            "ce_undo",
            [search(b"b ", &c3).as_slice(), b"ceZ\x1bu\r"].concat(),
            &b"a\xc3\xa9b "[..],
        ),
        (
            "ce_dot",
            [search(b"b ", &c3).as_slice(), b"ceZ\x1b.\r"].concat(),
            &b"aZ "[..],
        ),
        (
            "continuation_undo",
            [search(b":b", &a9).as_slice(), b"cwZ\x1bu\r"].concat(),
            &b"a\xc3\xa9:b"[..],
        ),
        (
            "continuation_dot",
            [search(b":b", &a9).as_slice(), b"cwZ\x1b.\r"].concat(),
            &b"a\xc3Z"[..],
        ),
        (
            "before_scalar_undo",
            [b"a".as_slice(), e, b"b \x1b0cwZ\x1bu\r"].concat(),
            &b"a\xc3\xa9b "[..],
        ),
    ] {
        assert_bytewise_editing_bytes(desc, &keys, expected, 0, byte_rc, &env);
    }
}

/// Vi character-search motions used with operators keep raw-byte ranges in
/// byte-oriented mode. A count that cannot be met reaches the last match,
/// rings once and still lets the pending operator act on that point.
#[test]
fn bash_and_sushline_byte_oriented_operator_char_search() {
    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    let c3 = [0xc3u8];
    let a9 = [0xa9u8];
    let one = || [b"a".as_slice(), e, b"bc:d"].concat();
    let two = || [b"a".as_slice(), e, b"bc:d:e"].concat();
    for (desc, keys, expected, bells) in [
        (
            "cf_lead",
            [one().as_slice(), b"\x1b0f", &c3, b"cf:Z\x1b\r"].concat(),
            &b"aZd"[..],
            0,
        ),
        (
            "df_lead",
            [one().as_slice(), b"\x1b0f", &c3, b"df:\r"].concat(),
            &b"ad"[..],
            0,
        ),
        (
            "yf_lead_put",
            [one().as_slice(), b"\x1b0f", &c3, b"yf:p\r"].concat(),
            &b"a\xc3\xc3\xa9bc:\xa9bc:d"[..],
            0,
        ),
        (
            "ct_lead",
            [one().as_slice(), b"\x1b0f", &c3, b"ct:Z\x1b\r"].concat(),
            &b"aZ:d"[..],
            0,
        ),
        (
            "dt_lead",
            [one().as_slice(), b"\x1b0f", &c3, b"dt:\r"].concat(),
            &b"a:d"[..],
            0,
        ),
        (
            "yt_lead_put",
            [one().as_slice(), b"\x1b0f", &c3, b"yt:p\r"].concat(),
            &b"a\xc3\xc3\xa9bc\xa9bc:d"[..],
            0,
        ),
        (
            "cf_cont",
            [one().as_slice(), b"\x1b0f", &a9, b"cf:Z\x1b\r"].concat(),
            &b"a\xc3Zd"[..],
            0,
        ),
        (
            "df_cont",
            [one().as_slice(), b"\x1b0f", &a9, b"df:\r"].concat(),
            &b"a\xc3d"[..],
            0,
        ),
        (
            "ct_cont",
            [one().as_slice(), b"\x1b0f", &a9, b"ct:Z\x1b\r"].concat(),
            &b"a\xc3Z:d"[..],
            0,
        ),
        (
            "cf_ascii",
            [one().as_slice(), b"\x1b0cf:Z\x1b\r"].concat(),
            &b"Zd"[..],
            0,
        ),
        (
            "df_ascii",
            [one().as_slice(), b"\x1b0df:\r"].concat(),
            &b"d"[..],
            0,
        ),
        (
            "df_raw_byte",
            [one().as_slice(), b"\x1b0df", &a9, b"\r"].concat(),
            &b"bc:d"[..],
            0,
        ),
        (
            "backward_search",
            [one().as_slice(), b"\x1b$f", &c3, b"dFa\r"].concat(),
            &b"d"[..],
            1,
        ),
        (
            "two_c2f",
            [two().as_slice(), b"\x1b0f", &c3, b"c2f:Z\x1b\r"].concat(),
            &b"aZe"[..],
            0,
        ),
        (
            "two_d2f",
            [two().as_slice(), b"\x1b0f", &c3, b"d2f:\r"].concat(),
            &b"ae"[..],
            0,
        ),
        (
            "one_c2f",
            [one().as_slice(), b"\x1b0f", &c3, b"c2f:Z\x1b\r"].concat(),
            &b"aZd"[..],
            1,
        ),
        (
            "one_d2f",
            [one().as_slice(), b"\x1b0f", &c3, b"d2f:\r"].concat(),
            &b"ad"[..],
            1,
        ),
        (
            "one_y2f_put",
            [one().as_slice(), b"\x1b0f", &c3, b"y2f:p\r"].concat(),
            &b"a\xc3\xc3\xa9bc:\xa9bc:d"[..],
            1,
        ),
        (
            "one_d2t",
            [one().as_slice(), b"\x1b0f", &c3, b"d2t:\r"].concat(),
            &b"a:d"[..],
            1,
        ),
        (
            "one_f2_plain",
            [one().as_slice(), b"\x1b0f", &c3, b"2f:x\r"].concat(),
            &b"a\xc3\xa9bcd"[..],
            1,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, &keys, expected, bells, byte_rc, &env);
    }
}

/// GNU rings once for a failed vi character search and again when a pending
/// operator loses its motion. A short count that reaches the last match only
/// rings once and still applies the operator. These cases hold in both
/// character and byte-oriented modes.
#[test]
fn bash_and_sushline_vi_failed_operator_search_rings_twice() {
    let plain = "set editing-mode vi\n";
    let byte = concat!("set editing-mode vi\n", "set byte-oriented on\n");
    let env = [("LC_ALL", "C")];
    for (mode, rc) in [("plain", plain), ("byte", byte)] {
        for (desc, keys, expected, bells) in [
            ("plain_search", b"abc\x1b0f:\r".to_vec(), Some("abc"), 1),
            ("delete", b"abc\x1b0df:\r".to_vec(), Some("abc"), 2),
            ("change", b"abc\x1b0cf:Z\r".to_vec(), Some("abc"), 3),
            ("till_delete", b"abc\x1b0dt:\r".to_vec(), Some("abc"), 2),
            ("till_yank", b"abc\x1b0yt:p\r".to_vec(), Some("abc"), 3),
            ("short_count", b"abc\x1b0d2fb\r".to_vec(), Some("c"), 1),
            ("repeat_delete", b"abc\x1b0fbd;\r".to_vec(), Some("abc"), 2),
            ("repeat_comma", b"abc\x1b0fbd,\r".to_vec(), Some("abc"), 2),
            (
                "delete_esc_target",
                b"abc def\x1b0df\x1b\r".to_vec(),
                Some("abc def"),
                2,
            ),
            (
                "delete_control_target",
                b"abc def\x1b0df\x01\r".to_vec(),
                Some("abc def"),
                2,
            ),
            (
                "change_esc_target",
                b"abc def\x1b0cf\x1b\r".to_vec(),
                Some("abc def"),
                2,
            ),
            (
                "yank_control_target",
                b"abc def\x1b0yf\x01\r".to_vec(),
                Some("abc def"),
                2,
            ),
            (
                "no_operator_esc_target",
                b"abc def\x1b0f\x1b\r".to_vec(),
                Some("abc def"),
                1,
            ),
            (
                "no_operator_control_target",
                b"abc def\x1b0f\x01\r".to_vec(),
                Some("abc def"),
                1,
            ),
        ] {
            let bash = run_bash_readline_with_inputrc_file_and_env(&keys, rc, &env);
            let sush = run_sushline_harness_with_inputrc_and_env(&keys, rc, &env);
            assert_eq!(
                accepted_line(&bash),
                expected.map(str::to_owned),
                "{mode} {desc}: {bash}"
            );
            assert_eq!(bell_count(&bash), bells, "{mode} {desc}: {bash}");
            assert_eq!(
                accepted_line(&sush),
                accepted_line(&bash),
                "{mode} {desc}: bash={bash}\nsushline={sush}"
            );
            assert_eq!(
                bell_count(&sush),
                bell_count(&bash),
                "{mode} {desc}: bash={bash}\nsushline={sush}"
            );
        }
    }

    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let mut keys = b"\xc3\xa9a\x1b0f".to_vec();
    keys.push(0xc3);
    keys.extend(b"df:p\r");
    let bash = run_bash_readline_with_inputrc_file_and_env(&keys, rc, &env);
    let sush = run_sushline_harness_with_inputrc_and_env(&keys, rc, &env);
    assert_eq!(accepted_line(&bash), Some("\u{e9}a".to_owned()), "{bash}");
    assert_eq!(bell_count(&bash), 4, "{bash}");
    assert_eq!(
        accepted_line(&sush),
        accepted_line(&bash),
        "bash={bash}\nsushline={sush}"
    );
    assert_eq!(
        bell_count(&sush),
        bell_count(&bash),
        "bash={bash}\nsushline={sush}"
    );
}

/// Byte-oriented deletes leave point on the deleted raw byte, so a following
/// `p` inserts there and `i` inserts before it. These cases match GNU.
#[test]
fn bash_and_sushline_byte_oriented_put_after_delete_points() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    let a9 = [0xa9u8];
    let cases: [(&str, Vec<u8>, &[u8], usize); 6] = [
        ("x_lead_put", [e, b"a\x1b0xp\r"].concat(), b"\xa9\xc3a", 0),
        (
            "x_lead_put_insert",
            [e, b"a\x1b0xpiZ\r"].concat(),
            b"\xa9Z\xc3a",
            0,
        ),
        (
            "x_cont_put",
            [e, b"a\x1b0f", &a9, b"xp\r"].concat(),
            b"\xc3a\xa9",
            0,
        ),
        ("X_lead_put", [e, b"a\x1b0Xp\r"].concat(), b"\xc3\xa9a", 2),
        ("s_lead_put", [e, b"a\x1b0spZ\r"].concat(), b"pZ\xa9a", 0),
        ("x_lead_dot", [e, b"a\x1b0x.\r"].concat(), b"a", 0),
    ];
    for (desc, keys, expected, bells) in cases {
        assert_bytewise_editing_bytes(desc, &keys, expected, bells, rc, &env);
    }
}

/// A `b`/`h`/arrow motion that cannot move rings once and leaves the line
/// unchanged; empty-line `$` and `x` are silent. A motion that does reach a
/// byte applies the operator normally, even at the last character.
#[test]
fn bash_and_sushline_vi_blocked_operator_motions() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C")];
    for (desc, keys, expected, bells) in [
        (
            "backward_word_at_start",
            b"abc\x1b0db\r".to_vec(),
            Some("abc"),
            1,
        ),
        (
            "backward_char_at_start",
            b"abc\x1b0dh\r".to_vec(),
            Some("abc"),
            1,
        ),
        (
            "forward_char_at_end",
            b"abc\x1bdl\r".to_vec(),
            Some("ab"),
            0,
        ),
        ("end_at_end", b"abc\x1bd$\r".to_vec(), Some("ab"), 0),
        (
            "beginning_at_start",
            b"abc\x1b0d0\r".to_vec(),
            Some("abc"),
            0,
        ),
        (
            "backward_word_after_move",
            b"abc\x1b0hdb\r".to_vec(),
            Some("abc"),
            2,
        ),
        (
            "left_arrow_at_start",
            b"abc\x1b0d\x1b[D\r".to_vec(),
            Some("abc"),
            1,
        ),
        (
            "right_arrow_at_end",
            b"abc\x1bd\x1b[C\r".to_vec(),
            Some("abc"),
            1,
        ),
        ("empty_backward_word", b"\x1bdb\r".to_vec(), Some(""), 1),
        ("empty_backward_char", b"\x1bdh\r".to_vec(), Some(""), 1),
        ("empty_end", b"\x1bd$\r".to_vec(), Some(""), 0),
        ("empty_delete_char", b"\x1bdx\r".to_vec(), Some(""), 0),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_env(&keys, rc, &env);
        let sush = run_sushline_harness_with_inputrc_and_env(&keys, rc, &env);
        assert_eq!(
            accepted_line(&bash),
            expected.map(str::to_owned),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sush),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sush}"
        );
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sush}"
        );
    }
}

/// Vi replace mode (`R`) overwrites one raw byte per printable key when byte
/// motion is in effect, so replacing the lead or continuation byte of a
/// multibyte scalar leaves the other byte intact. Control and single-byte
/// quoted keys are inserted, while a quoted multibyte key inserts its lead
/// byte and overwrites raw bytes with its continuation bytes, as in GNU.
#[test]
fn bash_and_sushline_byte_oriented_vi_overwrite_mode() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    let a = "\u{3042}".as_bytes();
    let c3 = [0xc3u8];
    let a9 = [0xa9u8];
    for (label, start, cases) in [
        (
            "lead",
            &c3[..],
            vec![
                ("printable", b"RZ\x1b\r".to_vec(), &b"Z\xa9bc:d"[..], 1),
                ("two_printable", b"RZY\x1b\r".to_vec(), &b"ZYbc:d"[..], 1),
                ("counted", b"3RZ\x1b\r".to_vec(), &b"Z\xa9bc:d"[..], 1),
                (
                    "control",
                    b"R\x01\x1b\r".to_vec(),
                    &b"\x01\xc3\xa9bc:d"[..],
                    1,
                ),
                (
                    "multibyte_replacement",
                    [b"R".as_slice(), e, b"\x1b\r"].concat(),
                    &b"\xc3\xa9bc:d"[..],
                    1,
                ),
                (
                    "quoted_control",
                    b"R\x16\x01\x1b\r".to_vec(),
                    &b"\x01\xc3\xa9bc:d"[..],
                    1,
                ),
                (
                    "quoted_printable",
                    b"R\x16Z\x1b\r".to_vec(),
                    &b"Z\xc3\xa9bc:d"[..],
                    1,
                ),
                (
                    "quoted_multibyte",
                    [b"R\x16".as_slice(), e, b"\x1b\r"].concat(),
                    &b"\xc3\xa9\xa9bc:d"[..],
                    1,
                ),
                (
                    "quoted_three_byte",
                    [b"R\x16".as_slice(), a, b"\x1b\r"].concat(),
                    &b"\xe3\x81\x82bc:d"[..],
                    1,
                ),
            ],
        ),
        (
            "cont",
            &a9[..],
            vec![
                ("printable", b"RZ\x1b\r".to_vec(), &b"\xc3Zbc:d"[..], 0),
                ("two_printable", b"RZY\x1b\r".to_vec(), &b"\xc3ZYc:d"[..], 0),
                ("counted", b"3RZ\x1b\r".to_vec(), &b"\xc3Zbc:d"[..], 0),
                (
                    "control",
                    b"R\x01\x1b\r".to_vec(),
                    &b"\xc3\x01\xa9bc:d"[..],
                    0,
                ),
                (
                    "multibyte_replacement",
                    [b"R".as_slice(), e, b"\x1b\r"].concat(),
                    &b"\xc3\xc3\xa9c:d"[..],
                    0,
                ),
                (
                    "quoted_control",
                    b"R\x16\x01\x1b\r".to_vec(),
                    &b"\xc3\x01\xa9bc:d"[..],
                    0,
                ),
                (
                    "quoted_printable",
                    b"R\x16Z\x1b\r".to_vec(),
                    &b"\xc3Z\xa9bc:d"[..],
                    0,
                ),
                (
                    "quoted_multibyte",
                    [b"R\x16".as_slice(), e, b"\x1b\r"].concat(),
                    &b"\xc3\xc3\xa9bc:d"[..],
                    0,
                ),
                (
                    "quoted_three_byte",
                    [b"R\x16".as_slice(), a, b"\x1b\r"].concat(),
                    &b"\xc3\xe3\x81\x82c:d"[..],
                    0,
                ),
            ],
        ),
    ] {
        for (desc, tail, expected, bells) in cases {
            let mut keys = [e, b"bc:d\x1b0f".as_slice()].concat();
            keys.extend_from_slice(start);
            keys.extend(tail);
            assert_bytewise_editing_bytes(
                &format!("{label} {desc}"),
                &keys,
                expected,
                bells,
                rc,
                &env,
            );
        }
    }
}

/// Dot after an operator whose motion spanned no text repeats that motion,
/// not the change from before it. A ringing motion (`b`, `h`) rings again and
/// a silent one (`0`) stays silent; the line is unchanged either way. An
/// operator cancelled by an unbound key keeps GNU's blank-appending redo.
#[test]
fn bash_and_sushline_vi_redo_after_blocked_motion() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C")];
    for (desc, keys, expected, bells) in [
        ("backward_word", b"abc\x1b0db.\r".to_vec(), "abc", 2),
        ("backward_char", b"abc\x1b0dh.\r".to_vec(), "abc", 2),
        ("beginning_of_line", b"abc\x1b0d0.\r".to_vec(), "abc", 0),
        (
            "repeated_backward_word",
            b"abc\x1b0db..\r".to_vec(),
            "abc",
            3,
        ),
        (
            "repeated_backward_char",
            b"abc\x1b0dh..\r".to_vec(),
            "abc",
            3,
        ),
        ("empty_backward_word", b"\x1bdb.\r".to_vec(), "", 2),
        ("empty_backward_char", b"\x1bdh.\r".to_vec(), "", 2),
        ("cancelled_operator", b"abc\x1b0dq.\r".to_vec(), "abc ", 1),
        ("following_motion", b"abc\x1b0dbl\r".to_vec(), "abc", 1),
        ("following_delete", b"abc\x1b0dbx\r".to_vec(), "bc", 1),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_env(&keys, rc, &env);
        let sush = run_sushline_harness_with_inputrc_and_env(&keys, rc, &env);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sush),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sush}"
        );
        assert_eq!(
            bell_count(&sush),
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sush}"
        );
    }
}

/// In bytewise vi replace mode a raw high replacement key overwrites the
/// single raw byte at point, just like an ASCII key. The accepted bytes keep
/// the byte that was not overwritten, whether that is the continuation byte
/// after replacing the lead or the lead byte after replacing the continuation.
#[test]
fn bash_and_sushline_byte_oriented_vi_raw_replacement_keys() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    for (label, start, cases) in [
        (
            "lead",
            0xc3u8,
            vec![
                ("lead_byte", 0xc3u8, b"\xc3\xa9bc:d" as &[u8], 1),
                ("continuation_byte", 0xa9u8, b"\xa9\xa9bc:d" as &[u8], 1),
                ("other_lead", 0xe3u8, b"\xe3\xa9bc:d", 1),
                ("invalid_byte", 0xffu8, b"\xff\xa9bc:d", 1),
                ("ascii", b'Z', b"Z\xa9bc:d", 1),
            ],
        ),
        (
            "continuation",
            0xa9u8,
            vec![
                ("lead_byte", 0xc3u8, b"\xc3\xc3bc:d", 0),
                ("continuation_byte", 0xa9u8, b"\xc3\xa9bc:d", 0),
                ("other_lead", 0xe3u8, b"\xc3\xe3bc:d", 0),
                ("invalid_byte", 0xffu8, b"\xc3\xffbc:d", 0),
                ("ascii", b'Z', b"\xc3Zbc:d", 0),
            ],
        ),
    ] {
        for (desc, key, expected, bells) in cases {
            let mut keys = [e, b"bc:d\x1b0f".as_slice()].concat();
            keys.extend([start, b'R', key, 0x1b, b'\r']);
            assert_bytewise_editing_bytes(
                &format!("{label} {desc}"),
                &keys,
                expected,
                bells,
                rc,
                &env,
            );
        }
    }
}

/// Backspace inside vi replace mode restores the byte that was overwritten
/// and steps back, instead of inserting a space. At the start of the replace
/// region it rings and leaves the line alone. The same rule holds for raw
/// bytes of a multibyte character in bytewise mode and for whole characters
/// in UTF-8 mode.
#[test]
fn bash_and_sushline_vi_overstrike_backspace_restores_overwritten_bytes() {
    let bytewise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let charwise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n"
    );
    let c = [("LC_ALL", "C")];
    let utf8 = [("LC_ALL", "C.UTF-8")];
    let e = "\u{e9}".as_bytes();
    for (desc, keys, expected, bells, rc, env) in [
        (
            "ascii_start",
            b"abcdef\x1b0fZRXY\x7f\x1b\r".to_vec(),
            "Xbcdef",
            1,
            bytewise_rc,
            &c[..],
        ),
        (
            "ascii_middle",
            b"abcdef\x1b0fcRXY\x7f\x1b\r".to_vec(),
            "abXdef",
            0,
            bytewise_rc,
            &c[..],
        ),
        (
            "ascii_twice",
            b"abcdef\x1b0fcRXY\x7f\x7f\x1b\r".to_vec(),
            "abcdef",
            0,
            bytewise_rc,
            &c[..],
        ),
        (
            "ascii_at_start_of_replace",
            b"abcdef\x1b0fcR\x7f\x1b\r".to_vec(),
            "abcdef",
            1,
            bytewise_rc,
            &c[..],
        ),
        (
            "ascii_ctrl_h",
            b"abcdef\x1b0fcRXY\x08Z\x1b\r".to_vec(),
            "abXZef",
            0,
            bytewise_rc,
            &c[..],
        ),
        (
            "multibyte_lead",
            [e, b"bc:d\x1b0f".as_slice(), &[0xc3], b"RXY\x7f\x1b\r"].concat(),
            "X\u{fffd}bc:d",
            1,
            bytewise_rc,
            &c[..],
        ),
        (
            "multibyte_lead_twice",
            [e, b"bc:d\x1b0f".as_slice(), &[0xc3], b"RXY\x7f\x7f\x1b\r"].concat(),
            "\u{e9}bc:d",
            1,
            bytewise_rc,
            &c[..],
        ),
        (
            "multibyte_continuation",
            [e, b"bc:d\x1b0f".as_slice(), &[0xa9], b"RXY\x7fZ\x1b\r"].concat(),
            "\u{fffd}XZc:d",
            0,
            bytewise_rc,
            &c[..],
        ),
        (
            "charwise_middle",
            b"abcdef\x1b0fcRXY\x7f\x1b\r".to_vec(),
            "abXdef",
            0,
            charwise_rc,
            &utf8[..],
        ),
        (
            "charwise_at_start_of_replace",
            b"abcdef\x1b0fcR\x7f\x1b\r".to_vec(),
            "abcdef",
            1,
            charwise_rc,
            &utf8[..],
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_env(&keys, rc, env);
        let sushline = run_sushline_harness_with_inputrc_and_env(&keys, rc, env);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

/// An overwrite that writes the same raw byte leaves the bytes unchanged, but
/// GNU still records an undo entry: the following `u` is silent. Marking the
/// pending insert run as a no-op preserves one entry without splitting a later
/// real change into two.
#[test]
fn bash_and_sushline_byte_overwrite_identical_byte_undo() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e = "\u{e9}".as_bytes();
    for (desc, target, tail, bells) in [
        ("lead", 0xc3u8, b"R\xc3\x1bu".as_slice(), 1),
        ("continuation", 0xa9u8, b"R\xa9\x1bu".as_slice(), 0),
        (
            "lead_paste",
            0xc3u8,
            b"R\x1b[200~\xc3\x1b[201~\x1bu".as_slice(),
            1,
        ),
        (
            "continuation_paste",
            0xa9u8,
            b"R\x1b[200~\xa9\x1b[201~\x1bu".as_slice(),
            0,
        ),
        ("different_byte", 0xc3u8, b"R\xe3\x1bu".as_slice(), 1),
        (
            "two_identical_bytes",
            0xc3u8,
            b"R\xc3\xc3\x1bu".as_slice(),
            1,
        ),
    ] {
        let mut keys = [e, b"bc:d\x1b0f".as_slice()].concat();
        keys.extend_from_slice(&[target]);
        keys.extend_from_slice(tail);
        keys.extend_from_slice(b"\r");
        let bash = run_bash_readline_with_inputrc_file_and_env(&keys, rc, &env);
        let sushline = run_sushline_harness_with_inputrc_and_env(&keys, rc, &env);
        assert_eq!(
            accepted_line(&bash),
            Some("\u{e9}bc:d".to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }

    let utf8 = [("LC_ALL", "C.UTF-8")];
    for (desc, tail, bells) in [
        ("identical_multibyte", b"R\xc3\xa9\x1bu".as_slice(), 0),
        ("different_multibyte", b"R\xc3\xa8\x1bu".as_slice(), 0),
        ("no_replacement", b"\x1bu".as_slice(), 1),
    ] {
        let mut keys = [e, b"bc:d\x1b0".as_slice()].concat();
        keys.extend_from_slice(tail);
        keys.extend_from_slice(b"\r");
        let bash = run_bash_readline_with_inputrc_file_and_env(&keys, rc, &utf8);
        let sushline = run_sushline_harness_with_inputrc_and_env(&keys, rc, &utf8);
        assert_eq!(
            accepted_line(&bash),
            Some("\u{e9}bc:d".to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

/// Backspace in vi replace mode undoes the last typed unit, not an offset into
/// the line as it stood when `R` began. Appended bytes are deleted silently,
/// and a replacement that changes byte length is removed in full before the
/// original bytes are restored; another backspace then rings. In bytewise mode
/// each raw byte of a multibyte key is its own replace step.
#[test]
fn bash_and_sushline_vi_overstrike_backspace_tracks_typed_units() {
    let bytewise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let charwise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n"
    );
    let c = [("LC_ALL", "C")];
    let utf8 = [("LC_ALL", "C.UTF-8")];
    let euro = "\u{20ac}".as_bytes();
    for (desc, keys, expected, bells, rc, env) in [
        (
            "appended",
            b"abc\x1bRX Y\x7f\x1b\r".to_vec(),
            "abX ",
            0,
            bytewise_rc,
            &c[..],
        ),
        (
            "appended_twice",
            b"abc\x1bRXY\x7f\x7f\x1b\r".to_vec(),
            "abc",
            0,
            bytewise_rc,
            &c[..],
        ),
        (
            "ctrl_h",
            b"abc\x1bRXY\x08\x1b\r".to_vec(),
            "abX",
            0,
            bytewise_rc,
            &c[..],
        ),
        (
            "wide_char",
            [b"abc\x1b0fbR".as_slice(), euro, b"\x7f\x1b\r"].concat(),
            "abc",
            0,
            charwise_rc,
            &utf8[..],
        ),
        (
            "wide_char_twice",
            [b"abc\x1b0fbR".as_slice(), euro, b"\x7f\x7f\x1b\r"].concat(),
            "abc",
            1,
            charwise_rc,
            &utf8[..],
        ),
        (
            "wide_char_then_insert",
            [b"abc\x1b0fbR".as_slice(), euro, b"\x7fX\x1b\r"].concat(),
            "aXc",
            0,
            charwise_rc,
            &utf8[..],
        ),
        (
            "bytewise_wide_char",
            [b"abc\x1b0fbR".as_slice(), euro, b"\x7f\x1b\r"].concat(),
            "a\u{fffd}",
            0,
            bytewise_rc,
            &c[..],
        ),
        (
            "bytewise_wide_char_twice",
            [b"abc\x1b0fbR".as_slice(), euro, b"\x7f\x7f\x1b\r"].concat(),
            "a\u{fffd}c",
            0,
            bytewise_rc,
            &c[..],
        ),
        (
            "combining_mark",
            "e\u{301}x\x1b0RX\x7f\x1b\r".as_bytes().to_vec(),
            "e\u{301}x",
            0,
            charwise_rc,
            &utf8[..],
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_env(&keys, rc, env);
        let sushline = run_sushline_harness_with_inputrc_and_env(&keys, rc, env);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

/// CSI and SS3 arrows do not move the point inside vi replace mode. GNU
/// dispatches the ESC immediately as vi-movement-mode, so the following `[`
/// or `O` is an unbound command-mode key and the final letter runs as a vi
/// command (`D`, `C`, `A` or `B`). The replace-mode ESC also steps back onto
/// the last replaced character before command mode begins.
#[test]
fn bash_and_sushline_vi_replace_mode_arrow_sequences_end_replace_mode() {
    let charwise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n"
    );
    let bytewise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells, rc) in [
        (
            "csi_left",
            b"abc\x1bRXY\x1b[DZ\x1b\r".as_slice(),
            "abX",
            2,
            charwise_rc,
        ),
        (
            "csi_right",
            b"abc\x1bRXY\x1b[CZ\x1b\r".as_slice(),
            "abXZ",
            1,
            charwise_rc,
        ),
        (
            "csi_up",
            b"abc\x1bRXY\x1b[AZ\x1b\r".as_slice(),
            "abXYZ",
            1,
            charwise_rc,
        ),
        (
            "csi_down",
            b"abc\x1bRXY\x1b[BZ\x1b\r".as_slice(),
            "abXY",
            2,
            charwise_rc,
        ),
        (
            "ss3_left",
            b"abc\x1bRXY\x1bODZ\x1b\r".as_slice(),
            "abX",
            2,
            charwise_rc,
        ),
        (
            "after_one_replacement",
            b"abc\x1bRX\x1b[DZ\x1b\r".as_slice(),
            "ab",
            2,
            charwise_rc,
        ),
        (
            "immediately_after_r",
            b"abc\x1bR\x1b[DZ\x1b\r".as_slice(),
            "a",
            2,
            charwise_rc,
        ),
        (
            "two_csi_lefts",
            b"abc\x1bRXY\x1b[D\x1b[DZ\x1b\r".as_slice(),
            "abX",
            2,
            charwise_rc,
        ),
        (
            "bytewise_csi_left",
            b"abc\x1bRXY\x1b[DZ\x1b\r".as_slice(),
            "abX",
            2,
            bytewise_rc,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_env(keys, rc, &env);
        let sushline = run_sushline_harness_with_inputrc_and_env(keys, rc, &env);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

/// Control bytes and quoted single bytes typed in vi replace mode insert
/// without displacing the text at point. Backspace rings at the replace origin
/// while no printable step exists; after printable steps GNU removes the
/// trailing inserted bytes and restores overwritten bytes with its undo
/// grouping.
#[test]
fn bash_and_sushline_vi_replace_inserts_control_and_quoted_bytes() {
    let charwise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n"
    );
    let bytewise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C.UTF-8")];
    let euro = "\u{20ac}".as_bytes();
    for (desc, keys, expected, bells, rc) in [
        (
            "control",
            b"abc\x1b0R\x01\x1b\r".to_vec(),
            "\u{1}abc",
            0,
            charwise_rc,
        ),
        (
            "control_backspace",
            b"abc\x1b0R\x01\x7f\x1b\r".to_vec(),
            "\u{1}abc",
            1,
            charwise_rc,
        ),
        (
            "two_controls_backspace",
            b"abc\x1b0R\x01\x01\x7f\x1b\r".to_vec(),
            "\u{1}\u{1}abc",
            1,
            charwise_rc,
        ),
        (
            "quoted",
            b"abc\x1b0R\x16X\x1b\r".to_vec(),
            "Xabc",
            0,
            charwise_rc,
        ),
        (
            "quoted_backspace",
            b"abc\x1b0R\x16X\x7f\x1b\r".to_vec(),
            "Xabc",
            1,
            charwise_rc,
        ),
        (
            "quoted_then_printable_backspace",
            b"abc\x1b0R\x16XX\x7f\x1b\r".to_vec(),
            "Xabc",
            0,
            charwise_rc,
        ),
        (
            "control_then_printable_backspace",
            b"abc\x1b0R\x01X\x7f\x1b\r".to_vec(),
            "\u{1}abc",
            0,
            charwise_rc,
        ),
        (
            "combining_quoted_backspace",
            "e\u{301}x\x1b0R\x16X\x7f\x1b\r".as_bytes().to_vec(),
            "Xe\u{301}x",
            1,
            charwise_rc,
        ),
        (
            "bytewise_control_backspace",
            b"abc\x1b0R\x01\x7f\x1b\r".to_vec(),
            "\u{1}abc",
            1,
            bytewise_rc,
        ),
        (
            "bytewise_quoted_backspace",
            b"abc\x1b0R\x16X\x7f\x1b\r".to_vec(),
            "Xabc",
            1,
            bytewise_rc,
        ),
        (
            "bytewise_quoted_high",
            [b"abc\x1b0R\x16\xc3\x7f\x1b\r".as_slice()].concat(),
            "\u{fffd}abc",
            1,
            bytewise_rc,
        ),
        (
            "bytewise_quoted_multibyte",
            [b"abc\x1b0R\x16".as_slice(), euro, b"\x7f\x1b\r"].concat(),
            "\u{fffd}bc",
            0,
            bytewise_rc,
        ),
        (
            "one_printable_control_backspace",
            b"abc\x1b0RX\x01\x7f\x1b\r".to_vec(),
            "abc",
            0,
            charwise_rc,
        ),
        (
            "two_printable_control_backspace",
            b"abc\x1b0RXY\x01\x7f\x1b\r".to_vec(),
            "XYc",
            0,
            charwise_rc,
        ),
        (
            "two_printable_control_backspace_twice",
            b"abc\x1b0RXY\x01\x7f\x7f\x1b\r".to_vec(),
            "abc",
            0,
            charwise_rc,
        ),
        (
            "three_printable_control_backspace",
            b"abc\x1b0RXYZ\x01\x7f\x7f\x7f\x1b\r".to_vec(),
            "abc",
            0,
            charwise_rc,
        ),
        (
            "four_printable_control_backspace_once",
            b"abcdef\x1b0RWXYZ\x01\x7f\x1b\r".to_vec(),
            "WXYZef",
            0,
            charwise_rc,
        ),
        (
            "four_printable_control_backspace_twice",
            b"abcdef\x1b0RWXYZ\x01\x7f\x7f\x1b\r".to_vec(),
            "WXYdef",
            0,
            charwise_rc,
        ),
        (
            "four_printable_control_backspace_thrice",
            b"abcdef\x1b0RWXYZ\x01\x7f\x7f\x7f\x1b\r".to_vec(),
            "WXcdef",
            0,
            charwise_rc,
        ),
        (
            "four_printable_control_backspace_four",
            b"abcdef\x1b0RWXYZ\x01\x7f\x7f\x7f\x7f\x1b\r".to_vec(),
            "abcdef",
            0,
            charwise_rc,
        ),
        (
            "one_printable_quoted_backspace",
            b"abc\x1b0RX\x16Z\x7f\x1b\r".to_vec(),
            "abc",
            0,
            charwise_rc,
        ),
        (
            "two_printable_quoted_backspace",
            b"abc\x1b0RXY\x16Z\x7f\x1b\r".to_vec(),
            "XYc",
            0,
            charwise_rc,
        ),
        (
            "bytewise_two_printable_control_backspace",
            b"abc\x1b0RXY\x01\x7f\x1b\r".to_vec(),
            "XYc",
            0,
            bytewise_rc,
        ),
        (
            "bytewise_two_printable_quoted_backspace",
            b"abc\x1b0RXY\x16Z\x7f\x1b\r".to_vec(),
            "XYc",
            0,
            bytewise_rc,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_env(&keys, rc, &env);
        let sushline = run_sushline_harness_with_inputrc_and_env(&keys, rc, &env);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

/// At line end, a UTF-8 vi word motion must not stop between a base
/// character and its combining mark: the edit point stays before the whole
/// grapheme. Byte-oriented motion keeps its raw-byte endpoint.
#[test]
fn bash_and_sushline_vi_word_motion_final_combining_grapheme() {
    let charwise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n"
    );
    let bytewise_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells, rc) in [
        (
            "w_insert",
            "abe\u{301}\x1b0wiX\x1b\r".as_bytes(),
            "abXe\u{301}",
            0,
            charwise_rc,
        ),
        (
            "bigword_insert",
            "abe\u{301}\x1b0WiX\x1b\r".as_bytes(),
            "abXe\u{301}",
            0,
            charwise_rc,
        ),
        (
            "w_delete_final_char",
            "abe\u{301}\x1b0wx\x1b\r".as_bytes(),
            "ab",
            0,
            charwise_rc,
        ),
        (
            "bytewise_w_insert",
            "abe\u{301}\x1b0wiX\x1b\r".as_bytes(),
            "abeX\u{301}",
            0,
            bytewise_rc,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_env(keys, rc, &env);
        let sushline = run_sushline_harness_with_inputrc_and_env(keys, rc, &env);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for the vi replace register: `r` makes
/// the displaced unit pasteable, a counted replace keeps only the last
/// displaced unit, and a register already filled by `x` is replaced before
/// a following `p` puts its new content.
#[test]
fn bash_and_sushline_vi_replace_updates_paste_register() {
    let charwise_rc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("put_before", "abc\x1b0raP\r", "aabc", 0),
        ("put_after", "abc\x1b0rap\r", "aabc", 0),
        ("counted_put_before", "abcd\x1b02raP\r", "abacd", 0),
        ("counted_put_after", "abcd\x1b02rap\r", "aabcd", 0),
        ("multibyte_put_before", "a\u{e9}b\x1b0rXP\r", "aX\u{e9}b", 0),
        (
            "multibyte_counted_put_before",
            "a\u{e9}b\u{e9}\x1b02rXP\r",
            "X\u{e9}Xb\u{e9}",
            0,
        ),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), charwise_rc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), charwise_rc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }

    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    for (desc, keys, expected, bells) in [
        ("ascii_put_before", &b"abc\x1b0raP\r"[..], &b"aabc"[..], 0),
        (
            "counted_put_before",
            &b"abcd\x1b02raP\r"[..],
            &b"abacd"[..],
            0,
        ),
        (
            "counted_put_after",
            &b"abcd\x1b02rap\r"[..],
            &b"aabcd"[..],
            0,
        ),
        (
            "raw_continuation_put_before",
            b"a\xc3\xa9b\x1b0f\xa9rAp\r",
            b"\x61\xc3\x41\xa9\x62",
            0,
        ),
        (
            "x_seeds_register_then_replace_put",
            b"abcde\x1b0x02rQp\r",
            b"QQcde",
            0,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, keys, expected, bells, byte_rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for vi word motions and operators over
/// combining-mark and joined graphemes away from the line-end snap case.
#[test]
fn bash_and_sushline_vi_word_motion_combining_mark_graphemes() {
    let rc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("w_after_accent", "e\u{301} f\x1b0wiX\r", "e\u{301} Xf", 0),
        (
            "w_count_after_accent",
            "abe\u{301} z\x1b0w2iX\r",
            "abe\u{301} Xz",
            0,
        ),
        ("change_accent", "abe\u{301}\x1b0cwZ\r", "Z", 0),
        (
            "yank_accent",
            "abe\u{301} z\x1b0ywp\r",
            "aabe\u{301} be\u{301} z",
            0,
        ),
        ("delete_final_accent", "e\u{301}\x1b0dw\r", "", 0),
        (
            "w_after_hangul_jamo",
            "e\u{1160} f\x1b0wiX\r",
            "e\u{1160} Xf",
            0,
        ),
        ("w_after_zwj", "a\u{200d}b f\x1b0wiX\r", "a\u{200d}b Xf", 0),
        ("w_end_hangul_jamo", "e\u{1160}\x1b0wiX\r", "Xe\u{1160}", 0),
        ("x_end_hangul_jamo", "e\u{1160}\x1b0wx\r", "", 0),
        ("w_end_zwj", "a\u{200d}b\x1b0wiX\r", "a\u{200d}Xb", 0),
        ("x_end_zwj", "a\u{200d}b\x1b0wx\r", "a\u{200d}", 0),
        ("w_leading_zwj", "\u{200d}ab\x1b0wiX\r", "\u{200d}aXb", 0),
        (
            "w_end_double_zwj",
            "a\u{200d}\u{200d}b\x1b0wiX\r",
            "a\u{200d}\u{200d}Xb",
            0,
        ),
        ("w_end_trailing_zwj", "a\u{200d}\x1b0wiX\r", "Xa\u{200d}", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), rc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for vi words that start with a grapheme
/// mark or joiner. The leading cluster is one vi word; `w` moves past it and
/// `cw` changes it, without stalling or panicking at byte 0.
#[test]
fn bash_and_sushline_vi_words_starting_with_grapheme_marks() {
    let rc = "set editing-mode vi";
    for (desc, keys, expected, bells) in [
        ("mark_w", "\u{301}abc\x1b0wiX\r", "\u{301}aXbc", 0),
        ("mark_cw", "\u{301}abc\x1b0cwZ\r", "Zbc", 0),
        ("zwj_w", "\u{200d}abc\x1b0wiX\r", "\u{200d}aXbc", 0),
        ("zwj_cw", "\u{200d}abc\x1b0cwZ\r", "Zbc", 0),
        ("jamo_w", "\u{1160}abc\x1b0wiX\r", "\u{1160}aXbc", 0),
        ("jamo_cw", "\u{1160}abc\x1b0cwZ\r", "Zbc", 0),
        ("mark_counted_w", "\u{301}abc\x1b02wiX\r", "\u{301}abXc", 0),
        ("mark_2cw_a_b_c", "\u{301}a b c\x1b02cwZ\r", "Z c", 0),
        ("mark_3cw_a_b_c_d", "\u{301}a b c d\x1b03cwZ\r", "Z d", 0),
        ("mark_4cw", "\u{301}a b c d e\x1b04cwZ\r", "Z e", 0),
        ("mark_2cw_a_bc", "\u{301}a bc\x1b02cwZ\r", "Z", 0),
        ("zwj_2cw", "\u{200d}a b c\x1b02cwZ\r", "Z c", 0),
        ("jamo_2cw", "\u{1160}a b c\x1b02cwZ\r", "Z c", 0),
        ("mark_2cw_abc_def", "\u{301}abc def\x1b02cwZ\r", "Z def", 0),
        ("mark_only_cw", "\u{301}\x1b0cwZ\r", "Z", 0),
        ("zwj_only_cw", "\u{200d}\x1b0cwZ\r", "Z", 0),
    ] {
        let bash = run_bash_readline_with_bindings(keys.as_bytes(), rc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), rc);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_owned()),
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
            bell_count(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
    }
}

/// Paired Bash 5.3 / Readline 8.3 pin for UTF-8 vi replace over a base with a
/// combining mark. The write, cursor advance and undo record must cover the
/// whole grapheme; hex acceptance markers reject invalid or lossy bytes.
#[test]
fn bash_and_sushline_vi_replace_combining_grapheme_exact_bytes() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("printable", "e\u{301}xy\x1b0RXY\r", "XYy", 0),
        (
            "grouped_backspace",
            "e\u{301}xy\x1b0RXY\x01\x7f\x7f\r",
            "e\u{301}xy",
            0,
        ),
        ("single", "e\u{301}xy\x1b0RX\r", "Xxy", 0),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for `~` over grapheme clusters.
///
/// A cased scalar maps its whole GNU character, dropping trailing marks or a
/// joiner. An uncased leading mark/Jamo/joiner run skips through its base
/// when the cluster ends before the line end, or through the whole run when
/// the base is final; a run with no base rings, as does a final uncased GNU
/// character.
#[test]
fn bash_and_sushline_vi_change_case_over_graphemes() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("accent", "e\u{301}xy\x1b0~\r", "Exy", 0),
        ("accent_at_end", "e\u{301}\x1b0~\r", "E", 0),
        ("accent_twice", "e\u{301}\x1b02~\r", "e", 0),
        ("zwj", "a\u{200d}by\x1b0~\r", "Aby", 0),
        ("two_marks", "a\u{301}\u{301}b\x1b0~\r", "Ab", 0),
        ("accent_then_insert", "e\u{301}xy\x1b0~iX\r", "EXxy", 0),
        ("jamo_leading", "\u{1160}axy\x1b0~\r", "\u{1160}axy", 0),
        ("mark_leading", "\u{301}a\x1b0~\r", "\u{301}a", 0),
        ("jamo_final", "\u{1160}\x1b0~\r", "\u{1160}", 1),
        ("multichar_mapping", "ßx\x1b0~iX\r", "ßXx", 0),
        ("esc_leading_mark", "\u{301}a\x1biX\r", "\u{301}Xa", 0),
        ("esc_leading_jamo", "\u{1160}a\x1biX\r", "\u{1160}Xa", 0),
        ("change_leading_mark", "\u{301}a\x1b~iX\r", "\u{301}XA", 0),
        ("change_leading_jamo", "\u{1160}a\x1b~iX\r", "\u{1160}XA", 0),
        (
            "skip_long_leading_mark",
            "\u{301}ab\x1b0~iX\r",
            "\u{301}aXb",
            0,
        ),
        (
            "skip_long_leading_mark_delete",
            "\u{301}ab\x1b0~x\r",
            "\u{301}a",
            0,
        ),
        (
            "skip_long_leading_mark_count",
            "\u{301}ab\x1b0~2~\r",
            "\u{301}ab",
            0,
        ),
        (
            "skip_two_leading_marks",
            "\u{301}\u{301}ab\x1b0~iX\r",
            "\u{301}\u{301}aXb",
            0,
        ),
        (
            "two_marks_final_insert",
            "\u{301}\u{301}a\x1b0~iZ\r",
            "\u{301}\u{301}Za",
            0,
        ),
        (
            "two_marks_only_insert",
            "\u{301}\u{301}\x1b0~iZ\r",
            "Z\u{301}\u{301}",
            1,
        ),
        (
            "two_marks_only_repeat",
            "\u{301}\u{301}\x1b0~0~\r",
            "\u{301}\u{301}",
            2,
        ),
        (
            "two_marks_final_delete",
            "\u{301}\u{301}a\x1b0~x\r",
            "\u{301}\u{301}",
            0,
        ),
        (
            "two_zwj_final_insert",
            "\u{200d}\u{200d}a\x1b0~iZ\r",
            "\u{200d}\u{200d}Za",
            0,
        ),
        (
            "skip_short_leading_jamo",
            "\u{1160}a\x1b0~iX\r",
            "\u{1160}Xa",
            0,
        ),
        (
            "skip_long_leading_jamo",
            "\u{1160}ab\x1b0~iX\r",
            "\u{1160}aXb",
            0,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// Paired GNU 5.3 / Readline 8.3 pin for command-mode `r` and `x` over
/// grapheme clusters. GNU ends a cluster at a joining character after a base,
/// and groups a leading mark/Jamo/joiner run with the character it precedes.
#[test]
fn bash_and_sushline_vi_character_operators_over_graphemes() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("r_accent", "e\u{301}xy\x1b0rZ\r", "Zxy", 0),
        ("r_zwj", "a\u{200d}by\x1b0rZ\r", "Zby", 0),
        ("r_jamo", "\u{1160}axy\x1b0rZ\r", "Zxy", 0),
        ("r_multi_mark", "a\u{301}\u{301}b\x1b0rZ\r", "Zb", 0),
        ("r_counted", "e\u{301}xy\x1b02rZ\r", "ZZy", 0),
        ("r_at_end", "e\u{301}\x1b0rZ\r", "Z", 0),
        ("r_zwj_at_end", "a\u{200d}\x1b0rZ\r", "Z", 0),
        ("r_jamo_join", "\u{1160}a\x1b0rZ\r", "Z", 0),
        ("x_accent", "e\u{301}xy\x1b0x\r", "xy", 0),
        ("x_zwj", "a\u{200d}by\x1b0x\r", "by", 0),
        ("x_jamo", "\u{1160}axy\x1b0x\r", "xy", 0),
        ("x_multi_mark", "a\u{301}\u{301}b\x1b0x\r", "b", 0),
        ("x_at_end", "e\u{301}\x1b0x\r", "", 0),
        ("x_zwj_at_end", "a\u{200d}\x1b0x\r", "", 0),
        ("x_jamo_join", "\u{1160}a\x1b0x\r", "", 0),
        ("x_leading_mark_zwj", "\u{301}\u{200d}ab\x1b0x\r", "b", 0),
        (
            "x_leading_mark_zwj_single",
            "\u{301}\u{200d}a\x1b0x\r",
            "",
            0,
        ),
        ("x_leading_two_marks", "\u{301}\u{301}ab\x1b0x\r", "b", 0),
        ("x_leading_jamo_zwj", "\u{1160}\u{200d}ab\x1b0x\r", "b", 0),
        ("r_leading_mark_zwj", "\u{301}\u{200d}ab\x1b0rZ\r", "Zb", 0),
        ("r_leading_two_marks", "\u{301}\u{301}ab\x1b0rZ\r", "Zb", 0),
        ("x_bare_leading_zwj", "\u{200d}ab\x1b0x\r", "b", 0),
        ("x_two_leading_zwj", "\u{200d}\u{200d}ab\x1b0x\r", "b", 0),
        ("x_bare_leading_zwj_single", "\u{200d}a\x1b0x\r", "", 0),
        ("r_bare_leading_zwj", "\u{200d}ab\x1b0rZ\r", "Zb", 0),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// GNU includes a whitespace grapheme with trailing marks or a joiner in the
/// changed range of `cw`, while a plain space stays in place. The exact-byte
/// rows cover both the word-start and final-cluster forms.
#[test]
fn bash_and_sushline_vi_cw_whitespace_graphemes() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("whitespace_mark", "e \u{301}abc\x1b0cwZ\r", "Zabc", 0),
        ("whitespace_mark_single", "e \u{301}a\x1b0cwZ\r", "Za", 0),
        (
            "whitespace_two_spaces_mark",
            "e  \u{301}abc\x1b0cwZ\r",
            "Zabc",
            0,
        ),
        (
            "whitespace_mark_longer",
            "abc \u{301}def\x1b0cwZ\r",
            "Zdef",
            0,
        ),
        ("whitespace_joiner", "e \u{200d}abc\x1b0cwZ\r", "Zabc", 0),
        ("whitespace_mark_final", "e \u{301}\x1b0cwZ\r", "Z", 0),
        ("plain_space_control", "e abc\x1b0cwZ\r", "Z abc", 0),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// GNU groups trailing marks and a base-following joiner with the word when
/// vi `e` scans, and rings when a following space motion is blocked at the
/// final character. Exact bytes pin both the point and the bell.
#[test]
fn bash_and_sushline_vi_end_word_grapheme_boundaries() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("mark_insert", "e\u{301}xy\x1b0eiX\r", "e\u{301}xXy", 0),
        (
            "mark_space_rings",
            "e\u{301}xy\x1b0e iX\r",
            "e\u{301}xXy",
            1,
        ),
        (
            "joiner_insert",
            "a\u{200d}bcd\x1b0eiX\r",
            "a\u{200d}bcXd",
            0,
        ),
        (
            "joiner_space_rings",
            "a\u{200d}bcd\x1b0e iX\r",
            "a\u{200d}bcXd",
            1,
        ),
        (
            "mark_before_space",
            "e\u{301} x y\x1b0eiX\r",
            "e\u{301} Xx y",
            0,
        ),
        (
            "two_marks_insert",
            "e\u{301}\u{301}xy\x1b0eiX\r",
            "e\u{301}\u{301}xXy",
            0,
        ),
        ("jamo_control", "\u{1160}ab\x1b0eiX\r", "\u{1160}aXb", 0),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// Vi end motions and backward word must treat trailing marks and joiners as
/// part of their word. A second `e`/`E` at the final character stays there,
/// and `b` from `$` returns to the first grapheme of the word.
#[test]
fn bash_and_sushline_vi_grapheme_word_motion_repeats() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("counted_e_mark", "e\u{301}xy\x1b2EiX\r", "e\u{301}xXy", 0),
        ("counted_E_mark", "e\u{301}xy\x1b0WEiX\r", "e\u{301}xXy", 0),
        (
            "counted_E_joiner",
            "a\u{200d}bcd\x1b2EiX\r",
            "a\u{200d}bcXd",
            0,
        ),
        ("counted_E_jamo", "\u{1160}ab\x1b2EiX\r", "\u{1160}aXb", 0),
        (
            "counted_E_before_space",
            "e\u{301} x y\x1b2EiX\r",
            "e\u{301} x Xy",
            0,
        ),
        (
            "counted_E_punctuation",
            "e\u{301}--x\x1b2EiX\r",
            "e\u{301}--Xx",
            0,
        ),
        ("backward_mark", "e\u{301}xy\x1b$biX\r", "Xe\u{301}xy", 0),
        (
            "backward_joiner",
            "a\u{200d}bcd\x1b$biX\r",
            "Xa\u{200d}bcd",
            0,
        ),
        (
            "backward_two_marks",
            "e\u{301}x\u{301}y\x1b$biX\r",
            "Xe\u{301}x\u{301}y",
            0,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// An inclusive vi operator following a multibyte character search covers the
/// whole target character, including a combining mark that shares a grapheme
/// with its base. Exact bytes prevent accepting a truncated UTF-8 tail.
#[test]
fn bash_and_sushline_vi_char_search_operator_matches_whole_multibyte() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("delete_precomposed", "a\u{e9}b\x1b0df\u{e9}\r", "b", 0),
        ("change_precomposed", "a\u{e9}b\x1b0cf\u{e9}Z\r", "Zb", 0),
        (
            "yank_precomposed",
            "a\u{e9}b\x1b0yf\u{e9}p\r",
            "aa\u{e9}\u{e9}b",
            0,
        ),
        ("delete_combining", "e\u{301}x-y\x1b0df\u{301}\r", "x-y", 0),
        (
            "change_combining",
            "e\u{301}x-y\x1b0cf\u{301}Z\r",
            "Zx-y",
            0,
        ),
        (
            "delete_mid_combining",
            "e\u{301}x e\u{301}y\x1b0df\u{301}\r",
            "x e\u{301}y",
            0,
        ),
        (
            "counted_delete_precomposed",
            "x\u{e9}a\u{e9}b\x1b02df\u{e9}\r",
            "b",
            0,
        ),
        (
            "delete_second_combining",
            "a\u{301}b\u{301}c\x1b0df\u{301}\r",
            "b\u{301}c",
            0,
        ),
        (
            "backward_delete_precomposed",
            "a\u{e9}b\x1b$F\u{e9}x\r",
            "ab",
            0,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// Vi `e` invoked on the final character must leave point on that character,
/// not one position past it, including when the final grapheme carries marks
/// or a joiner. A following space motion still rings at the line edge.
#[test]
fn bash_and_sushline_vi_end_word_final_character_point() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("ascii", "abc\x1b0$eiX\r", "abXc", 0),
        ("final_mark", "ab\u{301}\x1b0$eiX\r", "aXb\u{301}", 0),
        ("final_joiner", "ab\u{200d}\x1b0$eiX\r", "aXb\u{200d}", 0),
        ("final_jamo", "ab\u{1160}\x1b0$eiX\r", "aXb\u{1160}", 0),
        ("final_punctuation", "one--\x1b0$eiX\r", "one-X-", 0),
        ("ascii_space_rings", "abc\x1b0$e iX\r", "abXc", 1),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// GNU's vi word commands classify only ASCII letters, digits and underscore
/// as word characters in these UTF-8 lines; precomposed non-ASCII letters
/// start a punctuation run. Exact bytes pin motions, operators and a yank/put.
#[test]
fn bash_and_sushline_vi_words_use_ascii_word_char_class() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("accented_motion", "a\u{e9}bc\x1b0wiX\r", "aX\u{e9}bc", 0),
        ("accented_delete", "a\u{e9}bc\x1b0dw\r", "\u{e9}bc", 0),
        ("accented_change", "a\u{e9}bc\x1b0cwZ\r", "Z\u{e9}bc", 0),
        ("accented_yank_put", "a\u{e9}bc\x1b0ywp\r", "aa\u{e9}bc", 0),
        ("leading_accented", "\u{e9}abc\x1b0wiX\r", "\u{e9}Xabc", 0),
        ("latin_capital", "a\u{c4}bc\x1b0wiX\r", "aX\u{c4}bc", 0),
        ("greek", "a\u{3b1}bc\x1b0wiX\r", "aX\u{3b1}bc", 0),
        ("cjk", "a\u{4e16}bc\x1b0wiX\r", "aX\u{4e16}bc", 0),
        ("underscore_control", "a_bc\x1b0wiX\r", "a_bXc", 0),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// GNU vi end-word stops on the base that follows a leading mark, Jamo or
/// joiner run, including when that cluster ends the line. Exact bytes pin
/// both the point and the pending operator range.
#[test]
fn bash_and_sushline_vi_end_word_leading_grapheme_clusters() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("jamo", "\u{1160}a\x1b0EiX\r", "\u{1160}Xa", 0),
        ("mark", "\u{301}a\x1b0EiX\r", "\u{301}Xa", 0),
        ("joiner", "\u{200d}a\x1b0EiX\r", "\u{200d}Xa", 0),
        ("jamo_count", "\u{1160}a\x1b02EiX\r", "\u{1160}Xa", 0),
        ("mark_count", "\u{301}a\x1b02EiX\r", "\u{301}Xa", 0),
        ("joiner_count", "\u{200d}a\x1b02EiX\r", "\u{200d}Xa", 0),
        (
            "double_mark",
            "\u{301}\u{301}a\x1b0EiX\r",
            "\u{301}\u{301}Xa",
            0,
        ),
        ("mark_only", "\u{301}\x1b0EiX\r", "X\u{301}", 0),
        ("change", "\u{301}a\x1b0cEZ\r", "Z", 0),
        ("yank_put", "\u{200d}a\x1b0yEp\r", "\u{200d}a\u{200d}a", 0),
        ("mark_space_e", "\u{301} \x1b0eiX\r", "\u{301}X ", 0),
        ("mark_space_E", "\u{301} \x1b0EiX\r", "\u{301}X ", 0),
        ("jamo_space_e", "\u{1160} \x1b0eiX\r", "\u{1160}X ", 0),
        ("joiner_space_E", "\u{200d} \x1b0EiX\r", "\u{200d}X ", 0),
        (
            "trailing_mark_e",
            "\u{301}a\u{301}\x1b0eiX\r",
            "\u{301}Xa\u{301}",
            0,
        ),
        (
            "trailing_mark_E",
            "\u{301}a\u{301}\x1b0EiX\r",
            "\u{301}Xa\u{301}",
            0,
        ),
        (
            "trailing_joiner_E",
            "\u{301}a\u{200d}\x1b0EiX\r",
            "\u{301}Xa\u{200d}",
            0,
        ),
        (
            "jamo_trailing_mark_E",
            "\u{1160}a\u{301}\x1b0EiX\r",
            "\u{1160}Xa\u{301}",
            0,
        ),
        (
            "double_trailing_mark_E",
            "\u{301}a\u{301}\u{301}\x1b0EiX\r",
            "\u{301}Xa\u{301}\u{301}",
            0,
        ),
        ("trailing_mark_change", "\u{301}a\u{301}\x1b0cEZ\r", "Z", 0),
        ("trailing_mark_delete", "\u{301}a\u{301}\x1b0dE\r", "", 0),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// A remembered multibyte search target must only widen an operator whose
/// motion is that search (including its repeat), not a later ordinary motion.
#[test]
fn bash_and_sushline_vi_char_search_scopes_operator_adjustment() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        (
            "delete_after_search",
            "a\u{e9}bc d\x1b0f\u{e9}de\r",
            "a d",
            0,
        ),
        ("to_end_after_search", "a\u{e9}bc d\x1b0f\u{e9}d$\r", "a", 0),
        (
            "plain_end_after_search",
            "a\u{e9}bc d\x1b0f\u{e9}e\r",
            "a\u{e9}bc d",
            0,
        ),
        ("ascii_control", "abc d\x1b0fbde\r", "a d", 0),
        ("till_search", "\u{e9}a\u{e9}a\x1b0dt\u{e9}\r", "\u{e9}a", 0),
        ("repeat_delete", "a\u{e9}\u{e9}bc\x1b0f\u{e9}d;\r", "abc", 0),
        (
            "repeat_yank_put",
            "a\u{e9}\u{e9}bc\x1b0f\u{e9}y;p\r",
            "a\u{e9}\u{e9}\u{e9}\u{e9}bc",
            0,
        ),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

/// Emacs word commands keep GNU's Unicode word class in C.UTF-8, even though
/// vi word commands use the ASCII class. Exact bytes pin both directions,
/// kill and case conversion.
#[test]
fn bash_and_sushline_emacs_words_keep_unicode_classes() {
    let rc = "set output-meta on\nset convert-meta off\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected) in [
        ("forward", "a\u{e9}b\x01\x1bfX\r", "a\u{e9}bX"),
        ("backward", "xx\u{e9}yy\x1bbX\r", "Xxx\u{e9}yy"),
        ("kill", "a\u{e9}b cd\x01\x1bd\r", " cd"),
        ("upcase", "a\u{e9}b cd\x01\x1bu\r", "A\u{c9}B cd"),
        ("downcase", "A\u{c9}B cd\x01\x1bl\r", "a\u{e9}b cd"),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), 0, rc, &env);
    }
}

/// In `LC_ALL=C` GNU's emacs word commands classify raw non-ASCII bytes as
/// punctuation, so a word stops at the first byte of a multibyte character.
/// The three cases mirror the C.UTF-8 control fixture above with exact bytes.
#[test]
fn bash_and_sushline_emacs_words_use_ascii_word_classes_in_c_locale() {
    let rc = "set output-meta on\nset convert-meta off\n";
    let env = [("LC_ALL", "C")];
    for (desc, keys, expected) in [
        (
            "forward",
            "a\u{e9}bc d\u{e9}fg\x01\x1bfX\r",
            "aX\u{e9}bc d\u{e9}fg",
        ),
        (
            "kill",
            "a\u{e9}bc d\u{e9}fg\x01\x1bd\r",
            "\u{e9}bc d\u{e9}fg",
        ),
        ("upcase", "a\u{e9}bc\x01\x1bu\r", "A\u{e9}bc"),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), 0, rc, &env);
    }
}

/// A trailing mark or joiner on an ordinary word is not part of GNU's
/// inclusive operator range, even though a leading-cluster end-word endpoint
/// consumes its own final grapheme. Exact bytes pin both sides.
#[test]
fn bash_and_sushline_vi_inclusive_operators_keep_plain_grapheme_tails() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected, bells) in [
        ("delete_mark_tail", "ab\u{301}\x1b0de\r", "\u{301}", 0),
        ("delete_joiner_tail", "ab\u{200d}\x1b0de\r", "\u{200d}", 0),
        ("search_mark_tail", "ab\u{301}cd\x1b0dfb\r", "\u{301}cd", 0),
        ("leading_mark_change", "\u{301}a\u{301}\x1b0cEZ\r", "Z", 0),
        ("leading_mark_delete", "\u{301}a\u{301}\x1b0dE\r", "", 0),
    ] {
        assert_bytewise_editing_bytes(desc, keys.as_bytes(), expected.as_bytes(), bells, rc, &env);
    }
}

#[test]
fn bash_and_sushline_bytewise_end_word_all_operators() {
    let rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let env = [("LC_ALL", "C")];
    let e9 = b"\xc3\xa9";
    let a_e9_b = [b"a".as_slice(), e9, b"b"].concat();
    let e9_ab = [e9.as_slice(), b"ab"].concat();
    for (desc, line, keys_tail, expected) in [
        (
            "plain_e_insert",
            a_e9_b.as_slice(),
            b"\x1b0eiX\r".as_slice(),
            [b"a\xc3".as_slice(), b"X", b"\xa9b"].concat(),
        ),
        (
            "plain_e_delete",
            a_e9_b.as_slice(),
            b"\x1b0de\r".as_slice(),
            b"b".to_vec(),
        ),
        (
            "plain_e_yank",
            a_e9_b.as_slice(),
            b"\x1b0yeP\r".as_slice(),
            [b"a".as_slice(), e9, b"a", e9, b"b"].concat(),
        ),
        (
            "leading_e_insert",
            e9_ab.as_slice(),
            b"\x1b0eiX\r".as_slice(),
            [b"\xc3".as_slice(), b"X", b"\xa9ab"].concat(),
        ),
        (
            "leading_e_delete",
            e9_ab.as_slice(),
            b"\x1b0de\r".as_slice(),
            b"ab".to_vec(),
        ),
    ] {
        let mut keys = line.to_vec();
        keys.extend_from_slice(keys_tail);
        assert_bytewise_editing_bytes(desc, &keys, &expected, 0, rc, &env);
    }
}

#[test]
fn bash_and_sushline_vi_yank_line_keeps_point_for_put() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    let e9 = b"\xc3\xa9";
    let line = [b"a".as_slice(), e9, b"b"].concat();
    for (desc, initial, tail, expected) in [
        (
            "put_after_yank",
            b"abc".as_slice(),
            b"\x1byyp\r".as_slice(),
            b"abcabc".as_slice(),
        ),
        (
            "counted_put_after_yank",
            b"abc".as_slice(),
            b"\x1byy3p\r".as_slice(),
            b"abcabcabcabc".as_slice(),
        ),
        (
            "oversized_put_after_yank",
            b"abc".as_slice(),
            b"\x1byy20p\r".as_slice(),
            b"abc".repeat(21).as_slice(),
        ),
        (
            "explicit_beginning_put",
            b"abcdef".as_slice(),
            b"\x1b0yy3p\r".as_slice(),
            b"aabcdefabcdefabcdefbcdef".as_slice(),
        ),
        (
            "multibyte_put_after_yank",
            line.as_slice(),
            b"\x1byyp\r".as_slice(),
            [b"a".as_slice(), e9, b"ba", e9, b"b"].concat().as_slice(),
        ),
    ] {
        let mut keys = initial.to_vec();
        keys.extend_from_slice(tail);
        assert_bytewise_editing_bytes(desc, &keys, expected, 0, rc, &env);
    }
}

#[test]
fn bash_and_sushline_vi_inclusive_operator_consumes_multibyte_endpoint() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    let e9 = "\u{e9}".as_bytes();
    let shi = "\u{4e16}".as_bytes();
    let mark = "\u{301}".as_bytes();
    let mut dash_e9 = b"--".to_vec();
    dash_e9.extend_from_slice(e9);
    let mut dash_shi = b"--".to_vec();
    dash_shi.extend_from_slice(shi);
    let mut marked = b"ab".to_vec();
    marked.extend_from_slice(mark);
    let mut leading = mark.to_vec();
    leading.extend_from_slice(b"a");
    leading.extend_from_slice(mark);
    for (desc, initial, tail, expected) in [
        (
            "delete_e9",
            dash_e9.as_slice(),
            b"\x1b0de\r".as_slice(),
            b"".as_slice(),
        ),
        (
            "change_e9",
            dash_e9.as_slice(),
            b"\x1b0ceZ\r".as_slice(),
            b"Z".as_slice(),
        ),
        (
            "yank_e9",
            dash_e9.as_slice(),
            b"\x1b0yep\r".as_slice(),
            [b"---".as_slice(), e9, b"-", e9].concat().as_slice(),
        ),
        (
            "delete_shi",
            dash_shi.as_slice(),
            b"\x1b0de\r".as_slice(),
            b"".as_slice(),
        ),
        (
            "keep_plain_mark",
            marked.as_slice(),
            b"\x1b0de\r".as_slice(),
            mark,
        ),
        (
            "leading_cluster_dE",
            leading.as_slice(),
            b"\x1b0dE\r".as_slice(),
            b"".as_slice(),
        ),
    ] {
        let mut keys = initial.to_vec();
        keys.extend_from_slice(tail);
        assert_bytewise_editing_bytes(desc, &keys, expected, 0, rc, &env);
    }

    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let byte_env = [("LC_ALL", "C.UTF-8")];
    let mut keys = dash_e9.clone();
    keys.extend_from_slice(b"\x1b0de\r");
    assert_bytewise_editing_bytes("bytewise_e9_endpoint", &keys, b"", 0, byte_rc, &byte_env);
}

#[test]
fn bash_and_sushline_vi_identical_replace_keeps_undo_point() {
    let rc = "set editing-mode vi\n";
    let env = [("LC_ALL", "C.UTF-8")];
    for (desc, keys, expected) in [
        (
            "same_byte_replace_undo",
            b"xxbc\x1b0rxu\r".as_slice(),
            b"xxbc".as_slice(),
        ),
        (
            "same_byte_replace_undo_point",
            b"xxbc\x1b0rxuiX\r".as_slice(),
            b"xXxbc".as_slice(),
        ),
        (
            "dot_replace_undo",
            b"abcdef\x1b0rx.u\r".as_slice(),
            b"xbcdef".as_slice(),
        ),
        (
            "dot_replace_two_undos",
            b"abcdef\x1b0rx.uu\r".as_slice(),
            b"abcdef".as_slice(),
        ),
        (
            "dot_replace_two_undos_point",
            b"abcdef\x1b0rx.uuiX\r".as_slice(),
            b"aXbcdef".as_slice(),
        ),
        (
            "dot_replace_undo_then_dot",
            b"abcdef\x1b0rx..u.\r".as_slice(),
            b"xxcdef".as_slice(),
        ),
    ] {
        assert_bytewise_editing_bytes(desc, keys, expected, 0, rc, &env);
    }

    let byte_rc = concat!(
        "set editing-mode vi\n",
        "set input-meta on\n",
        "set output-meta on\n",
        "set convert-meta off\n",
        "set byte-oriented on\n"
    );
    let byte_env = [("LC_ALL", "C")];
    for (desc, keys, expected) in [
        (
            "byte_same_replace_undo",
            b"xxbc\x1b0rxu\r".as_slice(),
            b"xxbc".as_slice(),
        ),
        (
            "byte_dot_replace_undo",
            b"abcdef\x1b0rx.u\r".as_slice(),
            b"xbcdef".as_slice(),
        ),
    ] {
        assert_bytewise_editing_bytes(desc, keys, expected, 0, byte_rc, &byte_env);
    }
}

#[test]
fn bash_readline_and_sushline_vi_search_keeps_escape_in_query() {
    let rc = "set editing-mode vi\n";
    let history = ["a\u{1b}b"];
    let keys = b"\x1b/a\x1bb\r\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, rc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, rc, &history);

    assert_eq!(accepted_line(&bash), Some("a\u{1b}b".to_string()), "{bash}");
    assert_eq!(bell_count(&bash), 0, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(bell_count(&sushline), bell_count(&bash));
}

#[test]
fn bash_readline_and_sushline_vi_search_abort_bells() {
    let rc = "set editing-mode vi\n";
    let history = ["abc", "a\u{e9}", "zero"];
    for keys in [
        b"\x1b/a\x1b\r\r".as_slice(),
        b"\x1b/a\x07\r\r".as_slice(),
        b"\x1b/\x07\xc3\xa9\r\r".as_slice(),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, rc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, rc, &history);

        assert_eq!(
            accepted_line(&bash),
            Some(String::new()),
            "keys={keys:?}\n{bash}"
        );
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
    }
}

#[test]
fn bytewise_counted_vi_replace_undo_restores_first_character_point() {
    let env = [("LC_ALL", "C.UTF-8")];
    for (keys, expected) in [
        (b"xxxx\x1b03rxuiX\r".as_slice(), b"xXxxx".as_slice()),
        (b"xxxx\x1b02rxuiX\r".as_slice(), b"xXxxx".as_slice()),
        (b"xxxx\x1b04rxuiX\r".as_slice(), b"xXxxx".as_slice()),
        (b"yxxx\x1b0l3rxuiX\r".as_slice(), b"yxXxx".as_slice()),
        (b"abcde\x1b03rXuiX\r".as_slice(), b"aXbcde".as_slice()),
        ("ééé\x1b02réuiX\r".as_bytes(), "éXéé".as_bytes()),
    ] {
        assert_bytewise_editing_bytes(
            "counted vi replace undo point",
            keys,
            expected,
            0,
            "set editing-mode vi\n",
            &env,
        );
    }
}

#[test]
fn vi_search_query_control_bytes_are_printable_not_raw_escapes() {
    let rc = "set editing-mode vi\n";
    for (history, keys, query, raw) in [
        (
            vec!["abc"],
            b"\x1b/a\x1b[2J\r\r".as_slice(),
            "/a^[[2J",
            "a\u{1b}[2J",
        ),
        (
            vec!["a\u{1b}b"],
            b"\x1b/a\x1bb\r\r".as_slice(),
            "/a^[b",
            "a\u{1b}b",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, rc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, rc, &history);

        assert!(bash.contains(query), "keys={keys:?}\nbash={bash}");
        assert!(
            sushline.contains(query),
            "keys={keys:?}\nsushline={sushline}"
        );
        let transcript = sushline
            .split("SUSHLINE_ACCEPTED:")
            .next()
            .unwrap_or(&sushline);
        assert!(
            !transcript.contains(raw),
            "raw query escape in sushline: keys={keys:?}\n{sushline}"
        );
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
    }
}

#[test]
fn vi_search_query_caret_width_advances_the_sushline_cursor() {
    let rc = "set editing-mode vi\n";
    let history = ["a\u{1b}b"];
    let keys = b"\x1b/a\x1bb\r\r";
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, rc, &history);

    assert!(
        sushline.contains("/a^[b\u{1b}[K\u{1b}[6G"),
        "caret notation must count as two cells: {sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_keep_replace_undo_point_across_history_recall() {
    let rc = "set editing-mode vi\n";
    let history = ["abcd", "zzzz"];
    for count in *b"23" {
        let mut keys = b"\x1b\x1b[A\x1b[A0".to_vec();
        keys.extend_from_slice(&[
            count, b'r', b'X', 0x1b, b'[', b'B', 0x1b, b'[', b'A', b'u', b'i', b'Y', b'\r',
        ]);
        let bash = run_bash_readline_with_inputrc_file_and_history(&keys, rc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(&keys, rc, &history);

        assert_eq!(accepted_line(&bash), Some("aYbcd".to_string()), "{bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "count={count}\nbash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), bell_count(&bash));
    }
}

#[test]
fn incremental_search_quoted_escape_uses_printable_caret_notation() {
    let keys = b"\x12\x16\x1b[2J";
    let output = run_sushline_harness_with_inputrc_and_env(keys, "", &[("LC_ALL", "C.UTF-8")]);

    assert!(
        output.contains("(failed reverse-i-search)`^[[2J': "),
        "{output}"
    );
    assert!(!output.contains("\u{1b}[2J'"), "raw query escape: {output}");
}

#[test]
fn search_prompt_does_not_duplicate_genuine_raw_byte_marker() {
    let keys = b"\x1b/\xe2\x80\x8b";
    let output = run_sushline_harness_with_inputrc_and_env(
        keys,
        "set editing-mode vi\n",
        &[("LC_ALL", "C.UTF-8")],
    );

    assert!(
        output.ends_with("\u{200b}\u{1b}[K\u{1b}[2G"),
        "one typed zero-width space must render once: {output}"
    );
}

#[test]
fn incremental_search_preserves_quoted_invalid_bytes() {
    let keys = b"\x12\x16\xff\r\r";
    let output = run_sushline_harness_with_inputrc_and_env_raw(keys, "", &[("LC_ALL", "C.UTF-8")]);

    let mut needle = b"(failed reverse-i-search)`".to_vec();
    needle.push(0xff);
    needle.extend_from_slice(b"': ");
    assert!(
        output.windows(needle.len()).any(|window| window == needle),
        "the query byte must survive redisplay: {output:?}"
    );
    assert!(
        !output.windows(3).any(|window| window == [0xef, 0xbf, 0xbd]),
        "a replacement character must not stand in for the raw byte: {output:?}"
    );
}

/// A plain query must not gain the physical-column bias that a literal TAB
/// query needs; GNU expands the matched line's TAB from the ordinary prompt
/// width in that case.
#[test]
fn bash_and_sushline_plain_search_query_keeps_prompt_column() {
    require_oracle_host();
    let keys = b"\x12a\r\r";
    let history = ["a\there"];
    let env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(keys, "", &history, &env);
    let sushline = run_sushline_harness_with_inputrc_and_history_and_env(keys, "", &history, &env);
    for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
        let marker = "a': ";
        let start = output
            .find(marker)
            .unwrap_or_else(|| panic!("{editor} has no search match: {output:?}"))
            + marker.len();
        let end = output[start..]
            .find("here")
            .unwrap_or_else(|| panic!("{editor} has no match suffix: {output:?}"))
            + start;
        let spaces = output[start..end]
            .bytes()
            .filter(|byte| *byte == b' ')
            .count();
        assert_eq!(spaces, 8, "{editor}: {output:?}");
        assert_eq!(
            accepted_line(output).as_deref(),
            Some("a\there"),
            "{editor}: {output:?}"
        );
    }
}

/// Bash 5.3 PTY evidence: a literal TAB in a reverse-search query advances
/// the physical prompt column by one beyond the query terminator it leaves
/// drawn. A TAB in the matched history line then expands from that physical
/// column, so one-, two-, three- and four-byte prefixes expand to 8, 7, 6
/// and 5 spaces. Counting the query's logical width here would drift by one.
#[test]
fn bash_and_sushline_search_match_tab_uses_physical_prompt_column() {
    require_oracle_host();
    let keys = b"\x12\x16\t\r\r";
    let env = [("LC_ALL", "C.UTF-8")];
    for prefix_len in 1..=4 {
        let entry = format!("{}\there", "a".repeat(prefix_len));
        let history = [entry.as_str()];
        let bash =
            run_bash_readline_with_inputrc_file_and_history_and_env(keys, "", &history, &env);
        let sushline =
            run_sushline_harness_with_inputrc_and_history_and_env(keys, "", &history, &env);
        for (editor, output) in [("Bash", &bash), ("Sushline", &sushline)] {
            let marker = format!("': {}", "a".repeat(prefix_len));
            let start = output
                .find(&marker)
                .unwrap_or_else(|| panic!("{editor} has no search match: {output:?}"))
                + marker.len();
            let end = output[start..]
                .find("here")
                .unwrap_or_else(|| panic!("{editor} has no match suffix: {output:?}"))
                + start;
            let spaces = output[start..end]
                .bytes()
                .filter(|byte| *byte == b' ')
                .count();
            assert_eq!(
                spaces,
                9 - prefix_len,
                "{editor} prefix_len={prefix_len}: {output:?}"
            );
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(entry.as_str()),
                "{editor} prefix_len={prefix_len}: {output:?}"
            );
        }
    }
}

#[test]
fn horizontal_search_prompt_keeps_one_raw_byte_marker() {
    let output = run_sushline_harness_with_inputrc_and_env_raw(
        b"\x12\xe2\x80\x8b\r\r",
        "set horizontal-scroll-mode on\n",
        &[("LC_ALL", "C.UTF-8")],
    );

    let count = output
        .windows(3)
        .filter(|window| *window == [0xe2, 0x80, 0x8b])
        .count();
    assert_eq!(count, 1, "one typed marker must render once: {output:?}");
}

/// The settled failed-search frame, measured at 24 columns by 80 with
/// standard tab stops. `pre_enter_end` bounds the slice before the first
/// Enter response; Bash erases the prompt with CSI 15P and Sushline with a
/// CR/LF pair. The last prompt frame in that slice is the frame a user sees.
/// A final left or home move is part of that settled frame, including a legal
/// column-zero cursor; the furthest earlier excursion is not.
fn settled_failed_search_frame(output: &str, pre_enter_end: usize) -> (String, usize, usize) {
    assert!(pre_enter_end <= output.len() && output.is_char_boundary(pre_enter_end));
    let mut settled: Option<(String, usize, usize)> = None;
    for end in 1..=pre_enter_end {
        let mut screen = super::screen::Screen::new(24, 80);
        screen.feed(&output.as_bytes()[..end]);
        let (row, column) = screen.cursor();
        let rows = screen.rows_text();
        if row != 0 || !rows[0].starts_with("(failed reverse-i-search)") || !rows[0].contains('\'')
        {
            continue;
        }
        let quote = rows[0].rfind('\'').expect("search terminator quote");
        settled = Some((rows[0].clone(), quote, column));
    }
    settled.expect("a settled failed-search frame")
}

#[test]
fn settled_failed_search_frame_prefers_the_last_prompt_frame() {
    let prompt = "(failed reverse-i-search)`':  ";
    let output = format!("\r{prompt}\x1b[C\x1b[C\x1b[D");
    let (_, _, cursor) = settled_failed_search_frame(&output, output.len());
    assert_eq!(
        cursor,
        prompt.len() + 1,
        "the final left move must settle the cursor, not the furthest excursion"
    );
}

/// Bash 5.3 PTY evidence (Readline 8.3): for `\x12\x16\t\r\r` under
/// C.UTF-8, the failed reverse-i-search redraw writes the quoted TAB as a
/// raw byte followed by three CSI-C moves, not as spaces at the tab stop.
#[test]
fn settled_failed_search_frame_keeps_a_column_zero_frame() {
    let prompt = "(failed reverse-i-search)`':  ";
    let output = format!("\r{prompt}\x1b[C\x1b[C\x1b[1G");
    let (_, _, cursor) = settled_failed_search_frame(&output, output.len());
    assert_eq!(cursor, 0, "a home move is part of the settled frame");
}

#[test]
fn settled_failed_search_frame_ignores_a_post_enter_home_move() {
    let prompt = "(failed reverse-i-search)`':  ";
    let output = format!("\r{prompt}\x1b[C\r\x1b[15P\x1b[1G");
    let pre_enter_end = output.find("\r\x1b[15P").expect("erase marker");
    let (_, _, cursor) = settled_failed_search_frame(&output, pre_enter_end);
    assert_eq!(
        cursor,
        prompt.len() + 1,
        "Enter response is outside the frame"
    );
}

#[test]
fn bash_readline_and_sushline_keep_a_literal_tab_in_the_search_query() {
    require_oracle_host();
    let keys = b"\x12\x16\t\r\r";
    let env = [("LC_ALL", "C.UTF-8")];
    let fragment: &[u8] = b"failed reverse-i-search)`\t\x1b[C\x1b[C\x1b[C";
    for (editor, output, quote_column, cursor_column) in [
        (
            "Bash",
            run_bash_readline_with_inputrc_file_and_env(keys, "", &env),
            27,
            35,
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_env(keys, "", &env),
            35,
            39,
        ),
    ] {
        assert!(
            output
                .as_bytes()
                .windows(fragment.len())
                .any(|window| window == fragment),
            "{editor} must write the raw query TAB and three cursor moves: {output:?}"
        );
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some(""),
            "{editor} must accept the empty line, not a completion: {output:?}"
        );
        let pre_enter_end = if editor == "Bash" {
            output.find("\r\u{1b}[15P").expect("bash search erase")
        } else {
            output.find("\r\r\n").expect("sushline search erase")
        };
        let (row, quote, cursor) = settled_failed_search_frame(&output, pre_enter_end);
        assert_eq!(
            quote, quote_column,
            "{editor} must settle the terminator quote at column {quote_column}: {row:?}"
        );
        assert_eq!(
            cursor, cursor_column,
            "{editor} must settle the cursor at column {cursor_column}: {row:?}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_keep_escape_in_named_command_query() {
    let inputrc = r#""\C-o": execute-named-command"#;
    for (keys, row) in [
        (b"\x0f\x1b5abc".as_slice(), "SUSHLINE_READY>!^[5abc"),
        (b"\x0f\x1b\x1babc".as_slice(), "SUSHLINE_READY>!^[^[abc"),
    ] {
        for (editor, output) in named_query_outputs(keys, inputrc, &[], row) {
            assert_screen_row(editor, &output, row);
        }
    }
}

#[test]
fn bash_readline_and_sushline_echo_named_command_control_bytes() {
    let inputrc = r#""\C-o": execute-named-command"#;
    for (controls, caret) in [
        (&b"\x01\x02\x04\x05"[..], "^A^B^D^E"),
        (&b"\x0f"[..], "^O"),
        (&b"\x0b\x0c\x0e\x10\x14\x18"[..], "^K^L^N^P^T^X"),
        (&b"\x06\x12\x19\x1d\x1e\x1f"[..], "^F^R^Y^]^^^_"),
    ] {
        let mut keys = vec![0x0f];
        keys.extend_from_slice(controls);
        keys.extend_from_slice(b"abc");
        let row = format!("SUSHLINE_READY>!{caret}abc");
        for (editor, output) in named_query_outputs(&keys, inputrc, &[], &row) {
            assert_screen_row(editor, &output, &row);
        }
    }
}

/// Ctrl-U and Ctrl-W clear the whole query and ring only when there was no
/// query to clear. The rows and their bells were measured with Ctrl-O bound to
/// execute-named-command on the Readline 8.3 host.
#[test]
fn bash_readline_and_sushline_clear_named_command_query_with_ctrl_u() {
    if !host_carries_measured_release() {
        eprintln!(
            "Named-command Ctrl-U was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    let inputrc = r#""\C-o": execute-named-command"#;
    for (keys, row, bells) in [
        (b"\x0f\x15".as_slice(), "SUSHLINE_READY>!", 1),
        (b"\x0fabc\x15def".as_slice(), "SUSHLINE_READY>!def", 0),
        (b"\x0f\x17".as_slice(), "SUSHLINE_READY>!", 1),
        (b"\x0fabc-def\x17XYZ".as_slice(), "SUSHLINE_READY>!XYZ", 0),
    ] {
        for (editor, output) in named_query_outputs(keys, inputrc, &[], row) {
            assert_screen_row(editor, &output, row);
            assert_eq!(
                bell_count(&output),
                bells,
                "{editor} keys={keys:?}: {output:?}"
            );
        }
    }
}

/// A quoted byte bypasses the query's command controls and is drawn in caret
/// notation. Quoted NUL is the measured exception that is not echoed; a
/// quoted LF still follows the existing Enter path.
#[test]
fn bash_readline_and_sushline_echo_quoted_control_bytes_in_named_query() {
    if !host_carries_measured_release() {
        eprintln!(
            "Named-command quoted insert was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    let inputrc = r#""\C-o": execute-named-command"#;
    for (prefix, quoted, row) in [
        (b"".as_slice(), b"\x07".as_slice(), "SUSHLINE_READY>!^Gabc"),
        (b"".as_slice(), b"\x08".as_slice(), "SUSHLINE_READY>!^Habc"),
        (b"".as_slice(), b"\x15".as_slice(), "SUSHLINE_READY>!^Uabc"),
        (b"".as_slice(), b"\x16".as_slice(), "SUSHLINE_READY>!^Vabc"),
        (b"".as_slice(), b"\x0d".as_slice(), "SUSHLINE_READY>!^Mabc"),
        (b"".as_slice(), b"\x7f".as_slice(), "SUSHLINE_READY>!^?abc"),
        (
            b"xyz".as_slice(),
            b"\x15".as_slice(),
            "SUSHLINE_READY>!^Uabc",
        ),
        (b"".as_slice(), b"\x00".as_slice(), "SUSHLINE_READY>!abc"),
    ] {
        let mut keys = Vec::new();
        keys.extend_from_slice(prefix);
        keys.extend_from_slice(b"\x0f\x16");
        keys.extend_from_slice(quoted);
        keys.extend_from_slice(b"abc");
        for (editor, output) in named_query_outputs(&keys, inputrc, &[], row) {
            assert_screen_row(editor, &output, row);
            assert_eq!(bell_count(&output), 0, "{editor} keys={keys:?}: {output:?}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_show_named_command_query_in_prompt() {
    let inputrc = r#""\C-o": execute-named-command"#;
    for keys in [
        b"\x0fabc".as_slice(),
        b"\x1b5\x0fabc".as_slice(),
        b"xyz\x0fabc".as_slice(),
    ] {
        for (editor, output) in named_query_outputs(keys, inputrc, &[], "SUSHLINE_READY>!abc") {
            assert_screen_row(editor, &output, "SUSHLINE_READY>!abc");
        }
    }
}

/// In horizontal-scroll mode the named-command query is one scrolled row:
/// a short query stays after the prompt, and a long one scrolls the prompt
/// out of view, leaving the same tail in both editors.
#[test]
fn bash_readline_and_sushline_keep_named_command_query_on_one_row() {
    use portable_pty::PtySize;
    let size = PtySize {
        rows: 24,
        cols: 24,
        pixel_width: 0,
        pixel_height: 0,
    };
    let inputrc = "set horizontal-scroll-mode on\n\"\\C-o\": execute-named-command\n";
    for (label, keys, row) in [
        ("short", b"\x0fabc".as_slice(), "SUSHLINE_READY>!abc"),
        (
            "long",
            b"\x0fabcdefghijklmnopqrstuvwxyzabcdefgh".as_slice(),
            "<rstuvwxyzabcdefgh",
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_inputrc_until_screen_row(keys, inputrc, &[], size, row),
            ),
            (
                "Sushline",
                run_sushline_harness_until_screen_row(keys, inputrc, &[], size, row),
            ),
        ] {
            let mut screen = Screen::new(24, 24);
            screen.feed(output.as_bytes());
            let rows: Vec<_> = screen
                .rows_text()
                .into_iter()
                .filter(|text| !text.is_empty())
                .collect();
            assert_eq!(
                rows,
                vec![row.to_string()],
                "{label} {editor} keys={keys:?}: {output:?}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_keep_named_command_decorations() {
    let inputrc = concat!(
        "set mark-modified-lines on\n",
        "set show-mode-in-prompt on\n",
        "set editing-mode vi\n",
        "set vi-ins-mode-string \"(ins)\"\n",
        "\"\\C-o\": execute-named-command\n"
    );
    let keys = b"\x1b[AX\x0fabc";
    for (editor, output) in named_query_outputs(keys, inputrc, &["abcdef"], "*SUSHLINE_READY>!abc")
    {
        assert_screen_row(editor, &output, "*SUSHLINE_READY>!abc");
    }
}

#[test]
fn bash_readline_and_sushline_keep_multibyte_named_command_query() {
    let inputrc = r#""\C-o": execute-named-command"#;
    for (keys, expected, row) in [
        (
            b"\x0f\xe2\x80\x8bab".as_slice(),
            "!\u{200b}ab",
            "SUSHLINE_READY>!\u{200b}ab",
        ),
        (
            b"\x0f\xe2\x80\x8b\x7fc".as_slice(),
            "!c",
            "SUSHLINE_READY>!c",
        ),
    ] {
        for (editor, output) in named_query_outputs(keys, inputrc, &[], row) {
            assert!(
                output.contains(expected),
                "keys={keys:?} {editor}: {output}"
            );
            assert_screen_row(editor, &output, row);
        }
    }
}

#[test]
fn bash_and_sushline_emit_different_bytes_for_quoted_escape_in_incremental_search() {
    let keys = b"\x12\x16\x1b";
    let env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_bindings_and_env(keys, "", &env);
    let sushline = run_sushline_harness_with_inputrc_and_env(keys, "", &env);
    let contains = |output: &str, needle: &[u8]| {
        output
            .as_bytes()
            .windows(needle.len())
            .any(|window| window == needle)
    };

    // Readline echoes the quoted ESC as raw bytes followed by cursor motion;
    // printing that into Sushline's prompt would inject a terminal escape.
    assert!(
        contains(&bash, b"reverse-i-search)`\x1b\x1b[C"),
        "GNU query bytes: {bash:?}"
    );
    assert!(
        contains(&sushline, b"reverse-i-search)`^[': "),
        "Sushline query bytes: {sushline:?}"
    );
    assert!(
        !contains(&sushline, b"reverse-i-search)`\x1b"),
        "Sushline must not inject a raw escape: {sushline:?}"
    );
}

#[test]
fn bash_and_sushline_emit_one_zero_width_space_in_incremental_query() {
    let keys = b"\x12\xe2\x80\x8b\r\r";
    let env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_bindings_and_env(keys, "", &env);
    let sushline = run_sushline_harness_with_inputrc_and_env(keys, "", &env);
    let contains = |output: &str, needle: &[u8]| {
        output
            .as_bytes()
            .windows(needle.len())
            .any(|window| window == needle)
    };
    let marker = b"\xe2\x80\x8b";
    let prompt_marker = [b"reverse-i-search)`".as_slice(), marker.as_slice()].concat();

    assert!(contains(&bash, &prompt_marker), "GNU query bytes: {bash:?}");
    assert!(
        contains(&sushline, &[marker.as_slice(), b"': ".as_slice()].concat()),
        "Sushline query bytes: {sushline:?}"
    );

    // GNU ends the query with cursor motion; Sushline ends it with `': `.
    let gnu_query = final_search_query_markers(bash.as_bytes(), b"\x1b");
    let sushline_query = final_search_query_markers(sushline.as_bytes(), b"': ");
    assert_eq!(gnu_query, marker, "GNU final query: {bash:?}");
    assert_eq!(sushline_query, marker, "Sushline final query: {sushline:?}");

    // A doubled marker in the query must be counted twice, unlike the old
    // prefix-inclusive count that could only match the first marker.
    let doubled = [
        b"(failed reverse-i-search)`".as_slice(),
        marker.as_slice(),
        marker.as_slice(),
        b"\x1b[C".as_slice(),
    ]
    .concat();
    assert_eq!(
        final_search_query_markers(&doubled, b"\x1b").len(),
        marker.len() * 2
    );
}

#[test]
fn bash_readline_and_sushline_fail_non_ascii_named_completion_once() {
    if !host_carries_measured_release() {
        eprintln!("Non-ASCII named completion was measured on Readline 8.3");
        return;
    }
    let keys = b"\x0f\xe2\x80\x8b\tX\r";
    let inputrc = r#""\C-o": execute-named-command"#;
    let marker = [b"SUSHLINE_READY>!".as_slice(), &[0xe2, 0x80, 0x8b]].concat();
    for (editor, output) in [
        ("Bash", run_bash_readline_with_bindings(keys, inputrc)),
        ("Sushline", run_sushline_harness_with_inputrc(keys, inputrc)),
    ] {
        assert_eq!(bell_count(&output), 2, "{editor} keys={keys:?}: {output:?}");
        assert!(
            output.as_bytes().windows(marker.len()).any(|w| w == marker),
            "{editor} must echo the zero-width-space query: {output:?}"
        );
        assert_eq!(accepted_line(&output), None, "{editor}: {output:?}");
    }
}

#[test]
fn bash_readline_and_sushline_page_named_command_completions() {
    if !host_carries_measured_release() {
        eprintln!("Named completion paging was measured on Readline 8.3");
        return;
    }
    let steps = [
        ("SUSHLINE_READY>", b"\x0f\t".as_slice()),
        ("--More--", b"q\rX\r".as_slice()),
    ];
    let inputrc = r#""\C-o": execute-named-command"#;
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_steps_with_inputrc(&steps, inputrc),
        ),
        (
            "Sushline",
            run_sushline_harness_steps_with_inputrc(&steps, inputrc),
        ),
    ] {
        assert_eq!(
            output.matches("--More--").count(),
            1,
            "{editor}: {output:?}"
        );
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("X"),
            "{editor}: {output:?}"
        );
        assert_eq!(bell_count(&output), 0, "{editor}: {output:?}");
    }
}

#[test]
fn bash_readline_and_sushline_ring_on_named_command_abort() {
    if !host_carries_measured_release() {
        eprintln!("Named-command abort bells were measured on Readline 8.3");
        return;
    }
    let inputrc = r#""\C-o": execute-named-command"#;
    for (name, keys, accepted) in [
        ("query", b"abc\x0fdef\x07X\r".as_slice(), Some("abcX")),
        ("empty", b"abc\x0f\x07X\r".as_slice(), Some("abcX")),
        ("submit", b"abc\x0fdef\x07\r".as_slice(), Some("abc")),
        ("count", b"\x1b3\x0fdef\x07X\r".as_slice(), Some("X")),
    ] {
        for (editor, output) in [
            ("Bash", run_bash_readline_with_bindings(keys, inputrc)),
            ("Sushline", run_sushline_harness_with_inputrc(keys, inputrc)),
        ] {
            let context = format!("{name} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), 1, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// An unquoted NUL inside the query submits the name just like Enter on the
/// measured Readline 8.3 host: preceding text is dispatched as the command
/// name, and later bytes edit the restored main line.
#[test]
fn bash_readline_and_sushline_submit_named_command_on_nul() {
    if !host_carries_measured_release() {
        eprintln!(
            "Named-command NUL was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    let inputrc = r#""\C-o": execute-named-command"#;
    for (keys, bells, accepted) in [
        (b"\x0f\x00abc\r".as_slice(), 0, Some("abc")),
        (b"\x0fa\x00bc\r".as_slice(), 1, Some("bc")),
        (b"\x0fabc\x00\r".as_slice(), 1, Some("")),
        (b"\x0f\x00\r".as_slice(), 0, Some("")),
    ] {
        for (editor, output) in [
            ("Bash", run_bash_readline_with_bindings(keys, inputrc)),
            ("Sushline", run_sushline_harness_with_inputrc(keys, inputrc)),
        ] {
            let context = format!("{editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// In byte-oriented mode, high bytes typed into the query stay query text:
/// GNU does not turn them into Meta-prefixed editing commands. Prepending an
/// invalid byte to `accept-line` makes the distinction observable: stripping
/// the byte would dispatch the known command silently, while retaining it
/// leaves an unknown name that rings and restores the original line. The
/// valid multibyte case additionally pins the echoed name.
#[test]
fn bash_readline_and_sushline_keep_byte_oriented_named_query_text() {
    if !host_carries_measured_release() {
        eprintln!(
            "Byte-oriented named-command text was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    let inputrc = r#""\C-o": execute-named-command"#;
    let env = [("LC_ALL", "C")];
    for (name, bytes) in [
        ("invalid_ff", &b"\xff"[..]),
        ("overlong", &b"\xc0\x80"[..]),
        ("lone_continuation", &b"\x80"[..]),
        ("truncated", &b"\xe2\x80"[..]),
        ("multibyte", &b"\xc3\xa9"[..]),
    ] {
        let mut keys = vec![b'X', 0x0f];
        keys.extend_from_slice(bytes);
        keys.extend_from_slice(b"accept-line\r\r");
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_env(&keys, inputrc, &env),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_env(&keys, inputrc, &env),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), 1, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), Some("X"), "{context}");
            if name == "multibyte" {
                assert!(
                    output.contains("!éaccept-line"),
                    "the raw multibyte query must be echoed: {context}"
                );
            }
        }
    }
}

/// The same discriminating check under C.UTF-8: invalid bytes are not
/// silently dropped from a named-command query in either editor.
#[test]
fn bash_readline_and_sushline_keep_utf8_locale_named_query_bytes() {
    if !host_carries_measured_release() {
        eprintln!(
            "UTF-8 locale named-command text was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    let inputrc = r#""\C-o": execute-named-command"#;
    let env = [("LC_ALL", "C.UTF-8")];
    for (name, bytes) in [
        ("invalid_ff", &b"\xff"[..]),
        ("lone_continuation", &b"\x80"[..]),
        ("truncated", &b"\xe2\x80"[..]),
    ] {
        let mut keys = vec![b'X', 0x0f];
        keys.extend_from_slice(bytes);
        keys.extend_from_slice(b"accept-line\r\r");
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_env(&keys, inputrc, &env),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_env(&keys, inputrc, &env),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), 1, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), Some("X"), "{context}");
        }
    }
}

/// Ctrl-U ends the search before applying its measured binding rule; Ctrl-W
/// clears the incremental-search query only when
/// it is a single word, not the line already selected by the current match.
/// Multi-word retention is covered by a separate fixture. The bell counts
/// include the failed query extensions for a needle that never matches; a
/// word rubout also rings once more when no match is active.
#[test]
fn bash_readline_and_sushline_apply_search_kills_to_query() {
    if !host_carries_measured_release() {
        eprintln!(
            "Search kill keys were measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for (name, keys, bells, accepted) in [
        (
            "ctrl_u_match",
            b"abc

"
            .as_slice(),
            1,
            Some("abc"),
        ),
        (
            "ctrl_u_no_match",
            b"zzz

"
            .as_slice(),
            4,
            Some(""),
        ),
        (
            "ctrl_u_empty",
            b"

"
            .as_slice(),
            1,
            Some(""),
        ),
        (
            "ctrl_w_match",
            b"abc

"
            .as_slice(),
            1,
            Some("abc"),
        ),
        (
            "ctrl_w_no_match",
            b"zzz

"
            .as_slice(),
            5,
            Some(""),
        ),
        (
            "ctrl_w_empty",
            b"

"
            .as_slice(),
            2,
            Some(""),
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", &["alpha", "abc"]),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", &["alpha", "abc"]),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// On the measured Readline 8.3 host, Ctrl-W retains a query containing one
/// of the measured separators (ASCII whitespace, `-`, `,`, `_`, `.`) instead
/// of clearing it. The four traces here cover whitespace alone: a retained
/// query rings once, except when trailing whitespace follows a query with no
/// other separator and a match is active, and a later failing key shows the
/// failed prompt while the selected match is accepted. These are measured
/// cases, not a general GNU word-rubout model.
#[test]
fn bash_readline_and_sushline_retain_multiword_search_query_on_ctrl_w() {
    if !host_carries_measured_release() {
        eprintln!(
            "Multi-word search Ctrl-W was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for (name, keys, bells, accepted, failed_prompt) in [
        (
            "multiword_then_q",
            b"\x12abc xyz\x17Q\r\r".as_slice(),
            2,
            Some("abc xyz"),
            true,
        ),
        (
            "two_ctrl_w_then_q",
            b"\x12abc xyz\x17\x17Q\r\r".as_slice(),
            3,
            Some("abc xyz"),
            true,
        ),
        (
            "trailing_space_then_q",
            b"\x12abc \x17Q\r\r".as_slice(),
            1,
            Some("abc xyz"),
            true,
        ),
        (
            "single_word_then_q",
            b"\x12abc\x17Q\r\r".as_slice(),
            1,
            Some("abcQ"),
            false,
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", &["abcQ", "abc xyz"]),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", &["abcQ", "abc xyz"]),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
            assert_eq!(
                output.contains("failed reverse-i-search"),
                failed_prompt,
                "{context}"
            );
        }
    }
}

/// Paired measurements for the retained-query bell and acceptance rules on
/// the Readline 8.3 host: a query with a measured separator keeps the
/// displayed line even when no key ever matched, a no-match Ctrl-W rings
/// twice, and the measured punctuation separators (`-`, `,`, `_`, `.`) retain
/// the query just as whitespace does. These traces only pin the listed keys
/// and histories; they do not claim a general GNU word-rubout model.
#[test]
fn bash_readline_and_sushline_ctrl_w_measured_separators_and_no_match() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!(
            "Search Ctrl-W separators and no-match cases were measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for (name, keys, history, bells, accepted, failed_prompt) in [
        (
            "no_match_after_match",
            b"\x12zzz www\x17\r\r".as_slice(),
            &["abc xyz"][..],
            8,
            Some("abc xyz"),
            true,
        ),
        (
            "no_match_empty_history",
            b"\x12abc xyz\x17\r\r".as_slice(),
            &[][..],
            9,
            Some(""),
            true,
        ),
        (
            "no_match_trailing_space",
            b"\x12zzz \x17\r\r".as_slice(),
            &["abc xyz"][..],
            5,
            Some("abc xyz"),
            true,
        ),
        (
            "match_trailing_space_two_words",
            b"\x12abc xyz \x17\r\r".as_slice(),
            &["abc xyz "][..],
            1,
            Some("abc xyz "),
            false,
        ),
        (
            "hyphen_then_q",
            b"\x12foo-bar\x17Q\r\r".as_slice(),
            &["foo-bar", "foo bar"][..],
            2,
            Some("foo-bar"),
            true,
        ),
        (
            "comma_then_q",
            b"\x12abc,xyz\x17Q\r\r".as_slice(),
            &["abc,xyz", "foo bar"][..],
            2,
            Some("abc,xyz"),
            true,
        ),
        (
            "underscore_then_q",
            b"\x12abc_xyz\x17Q\r\r".as_slice(),
            &["abc_xyz", "foo bar"][..],
            2,
            Some("abc_xyz"),
            true,
        ),
        (
            "dot_then_q",
            b"\x12abc.xyz\x17Q\r\r".as_slice(),
            &["abc.xyz", "foo bar"][..],
            2,
            Some("abc.xyz"),
            true,
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?} history={history:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
            assert_eq!(
                output.contains("failed reverse-i-search"),
                failed_prompt,
                "{context}"
            );
        }
    }
}

/// An unquoted NUL freezes the incremental-search query on the measured
/// Readline 8.3 host: the selected line and query stay as they were, later
/// printable bytes are ignored, and a failed freeze rings for each ignored
/// byte and shows the `failed ` prompt. The empty query never fails.
#[test]
fn bash_readline_and_sushline_freeze_search_query_on_nul() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!(
            "Incremental-search NUL was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for (name, keys, bells, accepted, failed_prompt) in [
        (
            "nul_then_abc",
            b"\x12\x00abc\r\r".as_slice(),
            0,
            Some(""),
            false,
        ),
        (
            "a_nul_then_bc",
            b"\x12a\x00bc\r\r".as_slice(),
            3,
            Some("abc"),
            true,
        ),
        (
            "abc_nul_then_x",
            b"\x12abc\x00x\r\r".as_slice(),
            0,
            Some("abc"),
            false,
        ),
        (
            "x_nul_then_abc",
            b"\x12x\x00abc\r\r".as_slice(),
            5,
            Some(""),
            true,
        ),
        ("a_nul", b"\x12a\x00\r\r".as_slice(), 1, Some("abc"), true),
        (
            "abc_nul",
            b"\x12abc\x00\r\r".as_slice(),
            0,
            Some("abc"),
            false,
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", &["alpha", "abc"]),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", &["alpha", "abc"]),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
            assert_eq!(
                output.contains("failed reverse-i-search"),
                failed_prompt,
                "{context}"
            );
        }
    }
}

/// Paired Bash 5.3 / Readline 8.3 PTY measurements (empty INPUTRC, 80x24) for
/// the NUL freeze rule: a selected match ending at end-of-line freezes
/// silently, while a match with trailing text or no match rings at NUL and
/// again for an ignored printable byte, with the `failed ` prompt. An empty
/// query keeps the pre-search point when a cursor command leaves the search.
#[test]
fn bash_readline_and_sushline_nul_freeze_uses_match_end_and_empty_query_point() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("NUL freeze was measured on Readline 8.3");
        return;
    }
    for (name, history, keys, bells, accepted, failed_prompt) in [
        (
            "match_reaches_eol",
            &["zzabc xyz"][..],
            b"\x12xyz\x00x\r\r".as_slice(),
            0,
            Some("zzabc xyz"),
            false,
        ),
        (
            "match_has_trailing_text",
            &["abc xyz"][..],
            b"\x12abc\x00x\r\r".as_slice(),
            2,
            Some("abc xyz"),
            true,
        ),
        (
            "other_occurrence_earlier_in_line",
            &["alpha"][..],
            b"\x12a\x00x\r\r".as_slice(),
            0,
            Some("alpha"),
            false,
        ),
        (
            "overlapping_match_has_trailing_text",
            &["ababX"][..],
            b"\x12ab\x00x\r\r".as_slice(),
            2,
            Some("ababX"),
            true,
        ),
        (
            "empty_query_keeps_point",
            &["nomatch"][..],
            b"XY\x12\x00\x02Q\r".as_slice(),
            0,
            Some("XQY"),
            false,
        ),
        (
            "empty_query_ctrl_a_moves_point",
            &["zzabc zz"][..],
            b"XY\x12\x00\x01Q\r".as_slice(),
            0,
            Some("QXY"),
            false,
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?} history={history:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
            assert_eq!(
                output.contains("failed reverse-i-search"),
                failed_prompt,
                "{context}"
            );
        }
    }
}

/// An unquoted NUL freezes the incremental-search query on the measured
/// Readline 8.3 host: after the freeze, further query-editing input is
/// frozen; the fixture lists the measured printable, quoted and control
/// bytes. Accept, abort, Ctrl-C and configured isearch terminators still
/// take their normal paths. The
/// unfrozen quoted case is a control: both editors keep the displayed match
/// when the quoted extension fails; the unfrozen repeat case keeps the older
/// match when the query grows. The ESC case pins the accepted line; its bell
/// count differs before the freeze as well.
#[test]
fn bash_readline_and_sushline_freeze_all_following_search_input() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!(
            "Incremental-search NUL was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for (name, keys, history, bells, bash_accepted, sushline_accepted) in [
        (
            "del_then_bc",
            b"\x12\x61\x00\x7f\x62\x63\x0d\x0d".as_slice(),
            &["alpha", "abc"][..],
            3,
            Some("abc"),
            Some("abc"),
        ),
        (
            "ctrl_r_then_bc",
            b"\x12\x61\x00\x12\x62\x63\x0d\x0d".as_slice(),
            &["alpha", "abc"][..],
            4,
            Some("abc"),
            Some("abc"),
        ),
        (
            "ctrl_r_then_bc_without_freeze",
            b"\x12\x61\x12\x62\x63\x0d\x0d".as_slice(),
            &["alpha", "abc"][..],
            2,
            Some("alpha"),
            Some("alpha"),
        ),
        (
            "ctrl_r_then_enter",
            b"\x12\x61\x00\x12\x0d\x0d".as_slice(),
            &["alpha", "abc"][..],
            2,
            Some("abc"),
            Some("abc"),
        ),
        (
            "matching_query_then_ctrl_r",
            b"\x12\x61\x62\x63\x00\x12\x0d\x0d".as_slice(),
            &["alpha", "abc"][..],
            1,
            Some("abc"),
            Some("abc"),
        ),
        (
            "ctrl_a",
            b"\x12\x61\x00\x01\x0d\x0d".as_slice(),
            &["alpha", "abc"][..],
            1,
            Some("abc"),
            Some("abc"),
        ),
        (
            "esc_terminator_then_edit",
            b"\x12\x61\x00\x1b\x01\x58\r\r".as_slice(),
            &["alpha", "abc"][..],
            2,
            Some("Xabc"),
            Some("Xabc"),
        ),
        (
            "quoted_insert_then_x",
            b"\x12\x61\x62\x00\x16\x78\x0d\x0d".as_slice(),
            &["alpha", "abc", "abd"][..],
            2,
            Some("abd"),
            Some("abd"),
        ),
        (
            "quoted_insert_without_freeze",
            b"\x12\x61\x62\x16\x78\x0d\x0d".as_slice(),
            &["alpha", "abc", "abd"][..],
            1,
            Some("abd"),
            Some("abd"),
        ),
        (
            "x_after_freeze",
            b"\x12\x61\x62\x00\x78\x0d\x0d".as_slice(),
            &["alpha", "abc", "abd"][..],
            2,
            Some("abd"),
            Some("abd"),
        ),
    ] {
        for (editor, output, accepted, expected_bells) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
                bash_accepted,
                bells,
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
                sushline_accepted,
                // The ESC terminate-without-execute path already rings once
                // in Sushline and twice in GNU before the NUL freeze; this
                // fixture pins the accepted-line fix and records that count.
                if name == "esc_terminator_then_edit" {
                    1
                } else {
                    bells
                },
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), expected_bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// A query extension that fails after a match was displayed keeps that
/// match on the measured Readline 8.3 host: Enter accepts the line the search
/// showed and rings once for the failed extension.
#[test]
fn bash_readline_and_sushline_accept_last_match_after_failed_query() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!(
            "Failed-query acceptance was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    let keys = b"\x12ab\r";
    let history = ["alpha one", "beta two", "alpha three"];
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("alpha three"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 1, "{context}");
    }
}

/// Deleting a query byte after a reverse-search repeat keeps the reached
/// entry: GNU does not jump back to the newer history line when the shortened
/// query still matches the displayed one. A failed extension keeps that entry
/// too, so its recovery deletion and the next extension both stay there; a
/// deletion that makes the query empty keeps the repeated line. The paired
/// rows pin accepted lines and bell counts, not redisplay bytes.
#[test]
fn bash_readline_and_sushline_delete_query_after_repeat_keeps_reached_match() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!(
            "Query deletion after repeat was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    for (name, keys, history, accepted, bells) in [
        (
            "initial_match_control",
            &b"\x12abc\x7f\r\r"[..],
            &["alpha", "abc"][..],
            "abc",
            0,
        ),
        (
            "failed_extension_recovery",
            &b"\x12a\x12b\x7f\r\r"[..],
            &["alpha", "abc"][..],
            "alpha",
            1,
        ),
        (
            "failed_extension_then_del",
            &b"\x12a\x12bx\x7f\r\r"[..],
            &["alpha", "abc"][..],
            "alpha",
            3,
        ),
        (
            "failed_abc_extension_then_del",
            &b"\x12a\x12bc\x7f\r\r"[..],
            &["alpha", "abc"][..],
            "alpha",
            3,
        ),
        (
            "repeat_then_empty_query",
            &b"\x12a\x12\x7f\r\r"[..],
            &["alpha", "abc"][..],
            "alpha",
            2,
        ),
        (
            "two_repeats_successful_extension_then_del",
            &b"\x12a\x12\x12l\x7f\r\r"[..],
            &["alpha", "abeta", "abc"][..],
            "alpha",
            0,
        ),
        (
            "two_repeats_failed_extension_then_del",
            &b"\x12a\x12\x12z\x7f\r\r"[..],
            &["alpha", "abeta", "abc"][..],
            "abeta",
            1,
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
            ),
        ] {
            let context = format!("{name}: {editor} keys={keys:?} history={history:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(accepted),
                "{context}"
            );
        }
    }
}

/// Paired Bash 5.3 / Readline 8.3 PTY measurements (empty INPUTRC, 80x24): a
/// repeated reverse search first walks earlier occurrences inside the
/// displayed entry, then moves to the previous matching entry and starts at
/// its last occurrence. A repeat that exhausts both keeps the line and rings;
/// the next repeat rings again. In-line occurrences may overlap by one byte.
/// Each row lists the query and the accepted line with the exact bell count
/// after zero or more repeats.
#[test]
fn bash_readline_and_sushline_repeat_reverse_search_occurrences() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!(
            "Within-entry search repeats were measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    type RepeatCase<'a> = (&'a [&'a str], &'a str, &'a [(&'a str, usize)]);
    let cases: &[RepeatCase<'_>] = &[
        (
            &["aaa"],
            "a",
            &[("aaa", 0), ("aaa", 0), ("aaa", 0), ("aaa", 1), ("aaa", 2)],
        ),
        (
            &["beta", "alpha"],
            "a",
            &[
                ("alpha", 0),
                ("alpha", 0),
                ("beta", 0),
                ("beta", 1),
                ("beta", 2),
            ],
        ),
        (
            &["alpha one", "beta two", "alpha three"],
            "a",
            &[
                ("alpha three", 0),
                ("alpha three", 0),
                ("beta two", 0),
                ("alpha one", 0),
                ("alpha one", 0),
            ],
        ),
        (
            &["xalpha", "beta"],
            "a",
            &[("beta", 0), ("xalpha", 0), ("xalpha", 0), ("xalpha", 1)],
        ),
        (
            &["a1", "x2", "a3", "x4", "a5"],
            "a",
            &[("a5", 0), ("a3", 0), ("a1", 0), ("a1", 1)],
        ),
        (
            &["aaaa"],
            "aa",
            &[
                ("aaaa", 0),
                ("aaaa", 0),
                ("aaaa", 0),
                ("aaaa", 1),
                ("aaaa", 2),
            ],
        ),
        (
            &["aXaXa"],
            "aXa",
            &[("aXaXa", 0), ("aXaXa", 0), ("aXaXa", 1), ("aXaXa", 2)],
        ),
    ];
    for (history, query, steps) in cases {
        for (repeats, (expected, bells)) in steps.iter().enumerate() {
            let mut keys = b"\x12".to_vec();
            keys.extend_from_slice(query.as_bytes());
            keys.extend(std::iter::repeat_n(0x12, repeats));
            keys.push(b'\r');
            for (editor, output) in [
                (
                    "Bash",
                    run_bash_readline_with_bindings_and_history(&keys, "", history),
                ),
                (
                    "Sushline",
                    run_sushline_harness_with_inputrc_and_history(&keys, "", history),
                ),
            ] {
                let context = format!(
                    "{editor} history={history:?} query={query:?} repeats={repeats}: {output:?}"
                );
                assert_eq!(
                    accepted_line(&output).as_deref(),
                    Some(*expected),
                    "{context}"
                );
                assert_eq!(bell_count(&output), *bells, "{context}");
            }
        }
    }
}

/// Bash 5.3 / Readline 8.3 PTY finding: Ctrl-R selects the last `a` in
/// `xayazaw`, and one repeat steps to the previous occurrence. LF terminates
/// the search without executing it, so the following `!` lands at that
/// selected byte offset. Both cases were measured with zero bells on the
/// oracle host.
#[test]
fn bash_readline_and_sushline_isearch_termination_keeps_selected_occurrence() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Search termination was measured on Readline 8.3");
        return;
    }
    let history = ["xayazaw"];
    for (repeats, expected) in [(0, "xayaz!aw"), (1, "xay!azaw")] {
        let mut keys = b"\x12a".to_vec();
        keys.extend(std::iter::repeat_n(b'\x12', repeats));
        keys.extend_from_slice(b"\n!\r");
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(&keys, "", &history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(&keys, "", &history),
            ),
        ] {
            let context = format!("{editor} repeats={repeats} keys={keys:?}: {output:?}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(&output), 0, "{context}");
        }
    }
}

/// GNU Readline 8.3 selects the later `abc` in the stored `abcabc`, but the
/// live buffer was shortened to `abc`; LF terminates at the query in the
/// accepted buffer, so the following `!` lands before it. Both hosts ring no
/// bell.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_shortened_live_edit() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Search termination after a live edit was measured on Readline 8.3");
        return;
    }
    let history = ["abcabc"];
    let keys = b"\x10\x7f\x7f\x7f\x12abc\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(accepted_line(&output).as_deref(), Some("!abc"), "{context}");
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// The stored match is the last `abc` in `abcabcabc` (byte 6), but the live
/// buffer was shortened to `abcabc`. GNU resumes at the last occurrence of
/// the query in that buffer (byte 3), so LF followed by `!` accepts
/// `abc!abc`. Both hosts ring no bell.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_shortened_live_edit_last_occurrence() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Search termination after a shortened live edit was measured on Readline 8.3");
        return;
    }
    let history = ["abcabcabc"];
    let keys = b"\x10\x7f\x7f\x7f\x12abc\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("abc!abc"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// One repeat steps from the stored last `abc` (byte 6) to byte 3 in
/// `abcabcabc`, while the live buffer was shortened to `abcabc`. GNU rebases
/// that selection to byte 0 of the accepted buffer, so `!` lands at its start.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_shortened_live_edit_one_repeat() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Search termination after a repeat was measured on Readline 8.3");
        return;
    }
    let history = ["abcabcabc"];
    let keys = b"\x10\x7f\x7f\x7f\x12abc\x12\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("!abcabc"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// A second repeat exhausts the three stored occurrences: GNU accepts the
/// buffer at byte 0 and rings once, while Sushline's older bell model rings
/// zero times. Only the accepted line and the measured bell counts are pinned.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_shortened_live_edit_two_repeats() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Search termination after two repeats was measured on Readline 8.3");
        return;
    }
    let history = ["abcabcabc"];
    let keys = b"\x10\x7f\x7f\x7f\x12abc\x12\x12\n!\r";
    for (editor, output, bells) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
            1,
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
            0,
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("!abcabc"),
            "{context}"
        );
        assert_eq!(bell_count(&output), bells, "{context}");
    }
}

/// With a prefix, the selected stored byte 4 shifts to byte 1 of the
/// shortened live `xabcabc`, and `!` lands after the prefix on both hosts.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_shortened_live_edit_one_repeat_with_prefix()
{
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Prefixed search termination after a repeat was measured on Readline 8.3");
        return;
    }
    let history = ["xabcabcabc"];
    let keys = b"\x10\x7f\x7f\x7f\x12abc\x12\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("x!abcabc"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// A prefix with the point moved to the end starts the reverse search at the
/// last occurrence in the edited line; GNU and Sushline both accept
/// `xabc!abc` with no bell.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_prepend_and_end() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Prepend with end point was measured on Readline 8.3");
        return;
    }
    let history = ["abcabc"];
    let keys = b"\x10\x01x\x1b[F\x12abc\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("xabc!abc"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// GNU's reverse byte scan starts at the search point and finds the
/// overlapping `aa` beginning at byte 1, while a first-occurrence shortcut
/// would select byte 0. Both hosts accept `a!aaX` with no bell.
#[test]
fn bash_readline_and_sushline_isearch_termination_keeps_an_overlapping_occurrence() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Overlapping search point was measured on Readline 8.3");
        return;
    }
    let history = ["aaa"];
    let keys = b"\x10X\x01\x06\x12aa\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("a!aaX"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// GNU's reverse byte scan steps backwards from the search point. When the
/// point lies strictly inside an overlapping occurrence, the occurrence that
/// starts before the point wins; a point inside a later extent selects that
/// later start. Both hosts accept the same lines with no bell.
#[test]
fn bash_readline_and_sushline_isearch_termination_inside_an_overlap() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Overlapping search point was measured on Readline 8.3");
        return;
    }
    let cases: &[(&[&str], &[u8], &str)] = &[
        (&["ababab"], b"\x10X\x01\x06\x12abab\n!\r", "!abababX"),
        (&["abababa"], b"\x10X\x01\x06\x12aba\n!\r", "!abababaX"),
        (
            &["ababab"],
            b"\x10X\x01\x06\x06\x06\x12abab\n!\r",
            "ab!ababX",
        ),
    ];
    for (history, keys, expected) in cases {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
            ),
        ] {
            let context = format!("{editor} keys={keys:?}: {output:?}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(*expected),
                "{context}"
            );
            assert_eq!(bell_count(&output), 0, "{context}");
        }
    }
}

/// A forward query can resolve in a newer history entry than the one whose
/// point started the search. That point says nothing about the newer entry,
/// so GNU rings nothing there; the accepted line already agrees.
#[test]
fn bash_readline_and_sushline_forward_search_bells_in_a_newer_entry() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Forward search bells were measured on Readline 8.3");
        return;
    }
    let history = ["abcabc", "mid", "xxxabc"];
    let bindings = r#""\C-t": forward-search-history"#;
    let keys = b"\x10\x10\x10\x1b[F\x14x\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, bindings, &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, bindings, &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("!xxxabc"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// GNU's first forward search ignores occurrences behind the start point in
/// the cursor entry: when none lie at or after the point it selects the first
/// newer matching entry, falling back to the cursor entry only when no newer
/// entry matches. A match in a newer entry keeps its own occurrence rank; the
/// cursor's point does not filter it or ring the bell. Ctrl-T stands in for
/// Ctrl-S because the PTY's XOFF flow control swallows that byte.
#[test]
fn bash_readline_and_sushline_forward_isearch_skips_the_point_entry_without_a_later_match() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Forward search entry skip was measured on Readline 8.3");
        return;
    }
    let bindings = r#""\C-t": forward-search-history"#;
    let end = b"\x1b[F".as_slice();
    let after_three = b"\x01\x1b[C\x1b[C\x1b[C".as_slice();
    let after_four = b"\x01\x1b[C\x1b[C\x1b[C\x1b[C".as_slice();
    let after_five = b"\x01\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C".as_slice();
    struct Case<'a> {
        name: &'a str,
        history: &'a [&'a str],
        backs: usize,
        point: &'a [u8],
        query: &'a [u8],
        accepted: &'a str,
        bells: usize,
    }
    let cases = [
        Case {
            name: "newer entry from the end",
            history: &["abcabc", "mid", "xxxabc"],
            backs: 3,
            point: end,
            query: b"abc",
            accepted: "xxx!abc",
            bells: 0,
        },
        Case {
            name: "matching occurrence at the point",
            history: &["abcabc", "mid", "xxxabc"],
            backs: 3,
            point: after_three,
            query: b"abc",
            accepted: "abc!abc",
            bells: 0,
        },
        Case {
            name: "point past both cursor occurrences",
            history: &["abcabc", "mid", "xxxabc"],
            backs: 3,
            point: after_four,
            query: b"abc",
            accepted: "xxx!abc",
            bells: 0,
        },
        Case {
            name: "point inside the last cursor occurrence",
            history: &["abcabc", "mid", "xxxabc"],
            backs: 3,
            point: after_five,
            query: b"abc",
            accepted: "xxx!abc",
            bells: 0,
        },
        Case {
            name: "no newer match falls back to the cursor entry",
            history: &["abcabc", "mid"],
            backs: 2,
            point: end,
            query: b"abc",
            accepted: "!abcabc",
            bells: 3,
        },
        Case {
            name: "first newer matching entry wins",
            history: &["abcabc", "mid", "yyyabc", "zzzabc"],
            backs: 4,
            point: end,
            query: b"abc",
            accepted: "yyy!abc",
            bells: 0,
        },
        Case {
            name: "shorter query in a newer entry",
            history: &["aaa", "mid", "xaa"],
            backs: 3,
            point: end,
            query: b"aa",
            accepted: "x!aa",
            bells: 0,
        },
    ];
    for case in &cases {
        let mut keys = vec![0x10; case.backs];
        keys.extend_from_slice(case.point);
        keys.extend_from_slice(b"\x14");
        keys.extend_from_slice(case.query);
        keys.extend_from_slice(b"\n!\r");
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(&keys, bindings, case.history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(&keys, bindings, case.history),
            ),
        ] {
            let context = format!("{} {editor} keys={keys:?}: {output:?}", case.name);
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(case.accepted),
                "{context}"
            );
            assert_eq!(bell_count(&output), case.bells, "{context}");
        }
    }
}

/// GNU's forward incremental search starts at the point where the search
/// began and advances one occurrence per repeat. Ctrl-T stands in for Ctrl-S
/// because the PTY's XOFF flow control otherwise swallows that byte. The
/// last cases also pin that each failed forward query byte rings once on
/// both hosts, while a successful query rings nothing.
#[test]
fn bash_readline_and_sushline_forward_isearch_termination_uses_the_start_point() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Forward search termination was measured on Readline 8.3");
        return;
    }
    let history = ["abcabc"];
    let bindings = r#""\C-t": forward-search-history"#;
    let cases: &[(&str, &[u8], &str, usize, usize)] = &[
        (
            "point before the second occurrence",
            b"\x10\x01\x1b[C\x1b[C\x1b[C\x14abc\n!\r",
            "abc!abc",
            0,
            0,
        ),
        (
            "one repeat after a prefix",
            b"\x10\x01x\x14abc\x14\n!\r",
            "xabc!abc",
            0,
            0,
        ),
        (
            "one failing byte after end",
            b"\x10\x01x\x1b[F\x14a\n!\r",
            "!xabcabc",
            1,
            1,
        ),
        (
            "two failing bytes after end",
            b"\x10\x01x\x1b[F\x14ab\n!\r",
            "!xabcabc",
            2,
            2,
        ),
        (
            "three failing bytes after end",
            b"\x10\x01x\x1b[F\x14abc\n!\r",
            "!xabcabc",
            3,
            3,
        ),
        (
            "failing query from line start",
            b"\x10\x01\x14zzz\n!\r",
            "!abcabc",
            3,
            3,
        ),
    ];
    for (name, keys, expected, bash_bells, sushline_bells) in cases {
        for (editor, output, bells) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, bindings, &history),
                *bash_bells,
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, bindings, &history),
                *sushline_bells,
            ),
        ] {
            let context = format!("{name} {editor} keys={keys:?}: {output:?}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(*expected),
                "{context}"
            );
            assert_eq!(bell_count(&output), bells, "{context}");
        }
    }
}

/// Appending the query itself makes the live line end with the stored entry.
/// The selected last occurrence stays last, so `!` lands before the appended
/// copy on both hosts with no bell.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_appended_query_copy() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Appended query copy was measured on Readline 8.3");
        return;
    }
    let history = ["abcabc"];
    let keys = b"\x10\x1b[Fabc\x12abc\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("abcabc!abc"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// A repeated reverse search on an edited multibyte line steps to the middle
/// occurrence in the stored entry and in the accepted buffer.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_multibyte_appended_suffix() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Multibyte appended suffix was measured on Readline 8.3");
        return;
    }
    let history = ["äxäxäx"];
    let keys = b"\x10\x1b[FZ\x12\xc3\xa4\x12\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("äx!äxäxZ"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// Single reverse searches after one- and two-byte tail removals, and after a
/// prefixed tail removal, keep the last occurrence in the edited buffer. These
/// shapes are where a first-occurrence fallback or a suffix guard diverge from
/// GNU; all three accept with no bell on both hosts.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_partial_tail_edits() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Partial tail edits were measured on Readline 8.3");
        return;
    }
    for (shape, entry, keys, expected) in [
        (
            "one byte removed",
            "abcabcabc",
            b"\x10\x7f\x12abc\n!\r".as_slice(),
            "abc!abcab",
        ),
        (
            "two bytes removed",
            "abcabcabc",
            b"\x10\x7f\x7f\x12abc\n!\r".as_slice(),
            "abc!abca",
        ),
        (
            "three bytes removed with a prefix",
            "xabcabcabc",
            b"\x10\x7f\x7f\x7f\x12abc\n!\r".as_slice(),
            "xabc!abc",
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", &[entry]),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", &[entry]),
            ),
        ] {
            let context = format!("{shape} {editor} keys={keys:?}: {output:?}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(&output), 0, "{context}");
        }
    }
}

/// A suffix appended to the recalled history line leaves the stored last
/// `abc` as the last occurrence of the accepted `abcabcabcX` buffer too, so
/// termination resumes at byte 6 and `!` lands before the final group.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_appended_live_edit_last_occurrence() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Search termination after an appended suffix was measured on Readline 8.3");
        return;
    }
    let history = ["abcabcabc"];
    let keys = b"\x10\x1b[FX\x12abc\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("abcabc!abcX"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// One repeat steps to the middle `abc` of the stored entry and of the
/// accepted line with its appended suffix: both hosts accept `abc!abcabcX`
/// with no bell.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_appended_live_edit_one_repeat() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!(
            "Search termination after an appended suffix and a repeat was measured on Readline 8.3"
        );
        return;
    }
    let history = ["abcabcabc"];
    let keys = b"\x10\x1b[FX\x12abc\x12\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("abc!abcabcX"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// Prepending a byte keeps the first `abc` of the accepted line selected on
/// both hosts; the stored last occurrence is not rebased across the prefix.
#[test]
fn bash_readline_and_sushline_isearch_termination_after_prepended_live_edit() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Search termination after a prepended prefix was measured on Readline 8.3");
        return;
    }
    let history = ["abcabcabc"];
    let keys = b"\x10\x01X\x12abc\n!\r";
    for (editor, output) in [
        (
            "Bash",
            run_bash_readline_with_bindings_and_history(keys, "", &history),
        ),
        (
            "Sushline",
            run_sushline_harness_with_inputrc_and_history(keys, "", &history),
        ),
    ] {
        let context = format!("{editor} keys={keys:?}: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("X!abcabcabc"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// Folded multibyte query characters arrive one at a time through a PTY. The
/// paired C.UTF-8 measurements cover equal-length folding (each Ä lowers to
/// ä) and overlapping matches inside one history entry; no general Unicode
/// folding claim is made.
#[test]
fn bash_readline_and_sushline_repeat_folded_multibyte_search_occurrences() {
    require_oracle_host();
    if !c_utf8_locale_available() || !host_carries_measured_release() {
        eprintln!("Folded multibyte repeats were measured on Readline 8.3 in C.UTF-8");
        return;
    }
    let inputrc = "set search-ignore-case on";
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for (history, bells) in [("äää", [0, 0, 1, 2]), ("ääää", [0, 0, 0, 1])] {
        for (repeats, bells) in bells.into_iter().enumerate() {
            let mut keys = b"\x12".to_vec();
            keys.extend_from_slice("ÄÄ".as_bytes());
            keys.extend(std::iter::repeat_n(0x12, repeats));
            keys.push(b'\r');
            let history = [history];
            for (editor, output) in [
                (
                    "Bash",
                    run_bash_readline_with_inputrc_file_and_history_and_env(
                        &keys,
                        inputrc,
                        &history,
                        &locale_env,
                    ),
                ),
                (
                    "Sushline",
                    run_sushline_harness_with_inputrc_and_history_and_env(
                        &keys,
                        inputrc,
                        &history,
                        &locale_env,
                    ),
                ),
            ] {
                let context = format!("{editor} history={history:?} repeats={repeats}: {output:?}");
                assert_eq!(
                    accepted_line(&output).as_deref(),
                    Some(history[0]),
                    "{context}"
                );
                assert_eq!(bell_count(&output), bells, "{context}");
            }
        }
    }
}

/// Paired C.UTF-8 measurements pin accepted lines and bell sequences for
/// cross-width folds whose raw query is longer than the match line. These
/// cases do not establish a general Unicode-folding rule.
#[test]
fn bash_readline_and_sushline_shrinking_fold_search_repeats() {
    require_oracle_host();
    if !c_utf8_locale_available() || !host_carries_measured_release() {
        eprintln!("Shrinking-fold repeats were measured on Readline 8.3 in C.UTF-8");
        return;
    }
    let inputrc = "set search-ignore-case on";
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for (query, history, bells) in [
        ("İİ", "iii", [1, 2, 3, 4]),
        ("İİ", "iiii", [0, 0, 1, 2]),
        ("KK", "kkkk", [0, 1, 2, 3]),
    ] {
        for (repeats, bells) in bells.into_iter().enumerate() {
            let mut keys = b"\x12".to_vec();
            keys.extend_from_slice(query.as_bytes());
            keys.extend(std::iter::repeat_n(0x12, repeats));
            keys.push(b'\r');
            let history = [history];
            for (editor, output) in [
                (
                    "Bash",
                    run_bash_readline_with_inputrc_file_and_history_and_env(
                        &keys,
                        inputrc,
                        &history,
                        &locale_env,
                    ),
                ),
                (
                    "Sushline",
                    run_sushline_harness_with_inputrc_and_history_and_env(
                        &keys,
                        inputrc,
                        &history,
                        &locale_env,
                    ),
                ),
            ] {
                let context = format!(
                    "{editor} query={query:?} history={history:?} repeats={repeats}: {output:?}"
                );
                assert_eq!(
                    accepted_line(&output).as_deref(),
                    Some(history[0]),
                    "{context}"
                );
                assert_eq!(bell_count(&output), bells, "{context}");
            }
        }
    }
}

/// Paired C.UTF-8 measurements for incremental search with a history entry
/// shorter than the first query character. Moved-to rows pin the skip; cursor
/// rows start on the short entry and pin that it stays reachable. Only these
/// measured shapes are pinned; no general Unicode-folding rule follows from them.
#[test]
fn bash_readline_and_sushline_short_history_fold_search() {
    require_oracle_host();
    if !c_utf8_locale_available() || !host_carries_measured_release() {
        eprintln!("Short-history folds were measured on Readline 8.3 in C.UTF-8");
        return;
    }
    let inputrc = "set search-ignore-case on";
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for (prefix, query, history, accepted, bells) in [
        (b"" as &[u8], "K", &["k", "zzzzzzzzzz"][..], "", 1),
        (b"", "KK", &["kkkk", "zzzzzzzzzz"][..], "kkkk", 0),
        (b"", "İ", &["i", "zzzzzzzzzz"][..], "", 1),
        (b"\x10\x10", "K", &["k", "zzzzzzzzzz"][..], "k", 0),
        (b"\x10\x10", "KK", &["k", "zzzzzzzzzz"][..], "k", 1),
        (b"", "K", &["k"][..], "", 1),
        (b"", "K", &["k", "k"][..], "", 1),
    ] {
        let mut keys = prefix.to_vec();
        keys.push(0x12);
        keys.extend_from_slice(query.as_bytes());
        keys.push(b'\r');
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_inputrc_file_and_history_and_env(
                    &keys,
                    inputrc,
                    history,
                    &locale_env,
                ),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history_and_env(
                    &keys,
                    inputrc,
                    history,
                    &locale_env,
                ),
            ),
        ] {
            let context = format!(
                "{editor} prefix={prefix:?} query={query:?} history={history:?}: {output:?}"
            );
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(accepted),
                "{context}"
            );
            assert_eq!(bell_count(&output), bells, "{context}");
        }
    }
}

/// Quoting a raw byte into a query after a displayed match still leaves the
/// match on the accepted line. The match line is valid UTF-8 even though the
/// query is not; the byte validity of the query must not decide retention.
#[test]
fn bash_readline_and_sushline_retain_match_after_invalid_query_bytes() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Invalid-byte retention was measured on Readline 8.3");
        return;
    }
    let history = ["alpha one", "beta two", "alpha three"];
    for (name, keys, bells) in [
        ("one_quoted_byte", b"\x12a\x16\xffb\r".as_slice(), 2),
        (
            "two_quoted_bytes",
            b"\x12a\x16\xff\x16\xfeb\r".as_slice(),
            3,
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", &history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", &history),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some("alpha three"),
                "{context}"
            );
            assert_eq!(bell_count(&output), bells, "{context}");
        }
    }
}

/// Ctrl-H and DEL inside an incremental search delete the last query
/// character on the measured Readline 8.3 host and recompute the match. A
/// failed deletion rings once, with another bell when the shortened query is
/// empty and a third when the original query was empty; the displayed line
/// stays on the accepted line even when the shortened query has no match.
#[test]
fn bash_readline_and_sushline_keep_search_line_on_ctrl_h() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Ctrl-H search behavior was measured on Readline 8.3");
        return;
    }
    let history = ["alpha one", "beta two", "alpha three"];
    for (name, keys, accepted, bells) in [
        ("match", b"\x12a\x08\r".as_slice(), Some("alpha three"), 2),
        ("empty", b"\x12\x08\r".as_slice(), Some(""), 3),
        (
            "match_twice",
            b"\x12a\x08\x08\r".as_slice(),
            Some("alpha three"),
            5,
        ),
        ("empty_twice", b"\x12\x08\x08\r".as_slice(), Some(""), 6),
        ("empty_then_d", b"\x12\x08d\r".as_slice(), Some(""), 4),
        ("failed_then_ctrl_h", b"\x12d\x08\r".as_slice(), Some(""), 3),
        (
            "match_then_d",
            b"\x12a\x08d\r".as_slice(),
            Some("alpha three"),
            3,
        ),
        (
            "failed_query_then_ctrl_h",
            b"\x12ab\x08\r".as_slice(),
            Some("alpha three"),
            1,
        ),
        (
            "ctrl_h_then_new_query",
            b"\x12a\x08b\r".as_slice(),
            Some("beta two"),
            2,
        ),
        (
            "ctrl_h_then_del",
            b"\x12a\x08\x7f\r".as_slice(),
            Some("alpha three"),
            5,
        ),
        ("empty_del", b"\x12\x7f\r".as_slice(), Some(""), 3),
        (
            "match_del",
            b"\x12a\x7f\r".as_slice(),
            Some("alpha three"),
            2,
        ),
        ("failed_del", b"\x12d\x7f\r".as_slice(), Some(""), 3),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", &history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", &history),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
            assert_eq!(bell_count(&output), bells, "{context}");
        }
    }
}

/// Deleting a still-unmatched query byte or character keeps GNU's single
/// bell when the shortened query remains non-empty, and a multibyte query
/// character is removed as a whole.
#[test]
fn bash_readline_and_sushline_delete_unmatched_search_query() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Search query deletion was measured on Readline 8.3");
        return;
    }
    let ascii: &[&str] = &["alpha one", "beta two", "alpha three"];
    let multibyte: &[&str] = &["ab", "aéb"];
    for (name, history, keys, accepted, bells) in [
        (
            "ctrl_h_unmatched",
            &ascii,
            b"\x12abc\x08\r".as_slice(),
            "alpha three",
            3,
        ),
        (
            "del_unmatched",
            &ascii,
            b"\x12abc\x7f\r".as_slice(),
            "alpha three",
            3,
        ),
        (
            "ctrl_h_twice_unmatched",
            &ascii,
            b"\x12abc\x08\x08\r".as_slice(),
            "alpha three",
            3,
        ),
        (
            "ctrl_h_thrice_unmatched",
            &ascii,
            b"\x12abc\x08\x08\x08\r".as_slice(),
            "alpha three",
            5,
        ),
        (
            "ctrl_h_then_failed",
            &ascii,
            b"\x12abd\x08\x08\r".as_slice(),
            "alpha three",
            3,
        ),
        (
            "ctrl_h_multibyte",
            &multibyte,
            b"\x12a\xc3\xa9\x08b\r".as_slice(),
            "ab",
            0,
        ),
        (
            "del_multibyte",
            &multibyte,
            b"\x12a\xc3\xa9\x7fb\r".as_slice(),
            "ab",
            0,
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(accepted),
                "{context}"
            );
            assert_eq!(bell_count(&output), bells, "{context}");
        }
    }
}

/// One measured bell deviation remains: after a failed extension, deleting
/// back to a match whose entry sits behind the displayed one leaves Sushline
/// one bell short. The accepted line and deletion granularity are the same.
#[test]
fn bash_readline_and_sushline_bell_deviation_on_search_delete_to_earlier_match() {
    require_oracle_host();
    if !host_carries_measured_release() {
        eprintln!("Search delete bell behavior was measured on Readline 8.3");
        return;
    }
    let first: &[&str] = &["alpha one", "beta two", "alpha three"];
    let reordered: &[&str] = &["beta two", "alpha one", "alpha three"];
    let fourth: &[&str] = &["alpha one", "beta two", "gamma three", "beta four"];
    for (name, history, keys, accepted, bash_bells, sushline_bells) in [
        (
            "ctrl_h_earlier_match",
            first,
            b"\x12ba\x08\r".as_slice(),
            "beta two",
            2,
            1,
        ),
        (
            "del_earlier_match",
            first,
            b"\x12ba\x7f\r".as_slice(),
            "beta two",
            2,
            1,
        ),
        (
            "ctrl_h_first_entry",
            reordered,
            b"\x12ba\x08\r".as_slice(),
            "beta two",
            1,
            1,
        ),
        (
            "ctrl_h_unmatched_shorter",
            fourth,
            b"\x12gax\x08\r".as_slice(),
            "gamma three",
            2,
            1,
        ),
        (
            "two_deletes_to_last_match",
            fourth,
            b"\x12baxx\x08\x08\r".as_slice(),
            "beta four",
            5,
            5,
        ),
    ] {
        for (editor, output, bells) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
                bash_bells,
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
                sushline_bells,
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(accepted),
                "{context}"
            );
            assert_eq!(bell_count(&output), bells, "{context}");
        }
    }
}

/// On the measured GNU Readline 8.3 host, Ctrl-U in an incremental search
/// ends the search, establishes the selected line and then applies
/// unix-line-discard from the match start. A typed key after Ctrl-U exposes
/// the point; when nothing precedes the point GNU rings instead. An empty
/// query leaves an empty line, a failed query keeps the displayed line, and
/// a NUL freeze does not disable Ctrl-U. Each case is paired on the same
/// oracle host.
#[test]
fn bash_readline_and_sushline_commit_search_line_on_ctrl_u() {
    for (name, keys, history, bells, accepted) in [
        (
            "matching_query_at_start",
            b"\x12abc xyz\x15Q\r".as_slice(),
            &["abcQ", "abc xyz"][..],
            1,
            Some("Qabc xyz"),
        ),
        (
            "empty_query",
            b"\x12\x15Q\r".as_slice(),
            &["abcQ", "abc xyz"][..],
            1,
            Some("Q"),
        ),
        (
            "failed_query",
            b"\x12zzz\x15Q\r".as_slice(),
            &["abcQ", "abc xyz"][..],
            3,
            Some("Qabc xyz"),
        ),
        (
            "no_following_key",
            b"\x12abc xyz\x15\r".as_slice(),
            &["abcQ", "abc xyz"][..],
            1,
            Some("abc xyz"),
        ),
        (
            "after_nul_freeze",
            b"\x12abc xyz\x00\x15Q\r".as_slice(),
            &["abcQ", "abc xyz"][..],
            1,
            Some("Qabc xyz"),
        ),
        (
            "matching_query_after_prefix",
            b"\x12cd\x15Q\r".as_slice(),
            &["abcdef"][..],
            0,
            Some("Qcdef"),
        ),
        (
            "matching_query_after_prefix_no_follow",
            b"\x12cd\x15\r".as_slice(),
            &["abcdef"][..],
            0,
            Some("cdef"),
        ),
        (
            "prefix_before_space",
            b"\x12abc\x15Q\r".as_slice(),
            &["xxabc yy"][..],
            0,
            Some("Qabc yy"),
        ),
        (
            "match_at_start_kills_nothing",
            b"\x12ab\x15\r".as_slice(),
            &["abcdef"][..],
            1,
            Some("abcdef"),
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// After an unquoted NUL freeze on the measured GNU Readline 8.3 host, the
/// bound cursor commands leave the search and act on the established line
/// with point at the start, even when the query matched at an interior
/// column; that interior freeze rings once. A typed key exposes the point;
/// backward movement at the start rings again. Only these measured keys
/// are claimed.
#[test]
fn bash_readline_and_sushline_apply_cursor_commands_after_nul_freeze() {
    let boundary: &[&str] = &["abcQ", "abc xyz"];
    let interior: &[&str] = &["zzabc zz"];
    for (name, history, keys, bells, accepted) in [
        (
            "beginning_of_line",
            &boundary,
            b"\x12abc xyz\x00\x01Q\r".as_slice(),
            0,
            Some("Qabc xyz"),
        ),
        (
            "end_of_line",
            &boundary,
            b"\x12abc xyz\x00\x05Q\r".as_slice(),
            0,
            Some("abc xyzQ"),
        ),
        (
            "backward_char_at_start",
            &boundary,
            b"\x12abc xyz\x00\x02Q\r".as_slice(),
            1,
            Some("Qabc xyz"),
        ),
        (
            "forward_char",
            &boundary,
            b"\x12abc xyz\x00\x06Q\r".as_slice(),
            0,
            Some("aQbc xyz"),
        ),
        (
            "interior_beginning_of_line",
            &interior,
            b"\x12abc\x00\x01Q\r".as_slice(),
            1,
            Some("Qzzabc zz"),
        ),
        (
            "interior_backward_char",
            &interior,
            b"\x12abc\x00\x02Q\r".as_slice(),
            2,
            Some("Qzzabc zz"),
        ),
        (
            "interior_forward_char",
            &interior,
            b"\x12abc\x00\x06Q\r".as_slice(),
            1,
            Some("zQzabc zz"),
        ),
        (
            "interior_end_of_line",
            &interior,
            b"\x12abc\x00\x05Q\r".as_slice(),
            1,
            Some("zzabc zzQ"),
        ),
        (
            "interior_twice_backward",
            &interior,
            b"\x12abc\x00\x02\x02Q\r".as_slice(),
            3,
            Some("Qzzabc zz"),
        ),
        (
            "interior_beginning_then_forward",
            &interior,
            b"\x12abc\x00\x01\x06Q\r".as_slice(),
            1,
            Some("zQzabc zz"),
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// Ending an incremental search with Ctrl-U dispatches a macro/string
/// rebinding on the established line. With Ctrl-U rebound to a literal `Q`,
/// the inserted text lands at the match point; command and named-command
/// rebindings still perform unix-line-discard in both editors.
#[test]
fn bash_readline_and_sushline_honor_ctrl_u_binding_in_isearch() {
    let keys = b"\x12cd\x15X\r".as_slice();
    let history = &["abcdef"][..];
    // GNU honors a macro/string rebinding of Ctrl-U during isearch, but a
    // command or named-command rebinding still performs unix-line-discard.
    for (label, bindings, expected) in [
        (r#"macro Q"#, r#""\C-u": "Q""#, "abQXcdef"),
        ("forward-char", r#""\C-u": forward-char"#, "Xcdef"),
        ("beginning-of-line", r#""\C-u": beginning-of-line"#, "Xcdef"),
        (
            "do-lowercase-version",
            r#""\C-u": do-lowercase-version"#,
            "Xcdef",
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, bindings, history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, bindings, history),
            ),
        ] {
            let context = format!("{label} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), 0, "{context}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(expected),
                "{context}"
            );
        }
    }
}

/// An ESC-prefixed key sequence in one input chunk ends incremental search as
/// a meta command: its binding runs at the match point, and an unbound meta
/// key rings once instead of inserting its character. The bell count for each
/// measured key sequence is included in the fixture.
#[test]
fn bash_readline_and_sushline_dispatch_esc_key_after_isearch() {
    let history = &["abcdef"][..];
    for (label, keys, bells, line) in [
        ("unbound meta-Q", b"\x12abc\x1bQ\r".as_slice(), 1, "abcdef"),
        ("meta-C-a", b"\x12abc\x1b\x01Q\r".as_slice(), 1, "Qabcdef"),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, "", history),
            ),
        ] {
            let context = format!("{label} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), Some(line), "{context}");
        }
    }
}

/// A lone UTF-8 lead byte followed by ESC ends an incremental search in a
/// UTF-8 locale; after the key-sequence timeout the next key edits the
/// established line.
#[test]
fn bash_and_sushline_end_search_on_escape_after_incomplete_utf8_prefix() {
    require_oracle_host();
    if !c_utf8_locale_available() {
        return;
    }

    let mut bash = portable_pty::CommandBuilder::new("bash");
    bash.env("LC_ALL", "C.UTF-8");
    bash.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    let mut sushline = sushline_harness_command();
    sushline.env("LC_ALL", "C.UTF-8");
    sushline.env("SUSHLINE_INPUTRC", "");
    sushline.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline.env("SUSHLINE_HISTORY", "");

    for (editor, command) in [("Bash", bash), ("Sushline", sushline)] {
        let output = run_pty_paused_until(
            command,
            b"\x12\xc3\x1b",
            Duration::from_millis(700),
            b"X\r",
            "SUSHLINE_ACCEPTED:",
        );
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("X"),
            "{editor} keys=[0x12, 0xc3, ESC] then X: {output:?}"
        );
    }
}

/// A stale UTF-8 prefix before ESC keeps the prior successful match on the
/// measured Readline 8.3 host: ESC ends the search without the stale byte
/// and without a bell, so Enter accepts the matched line. The point after
/// terminate-without-execute is a separate pre-existing deviation, so this
/// fixture stops at Enter instead of inserting after ESC like the finding's
/// Z case does.
#[test]
fn bash_and_sushline_end_search_on_stale_prefix_escape_with_match() {
    require_oracle_host();
    if !c_utf8_locale_available() {
        return;
    }
    if !host_carries_measured_release() {
        eprintln!(
            "Stale-prefix ESC was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    let history = &["alpha", "alphabet"][..];
    let history_commands = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");

    let mut bash = portable_pty::CommandBuilder::new("bash");
    bash.env("LC_ALL", "C.UTF-8");
    bash.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; {history_commands}; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    let mut sushline = sushline_harness_command();
    sushline.env("LC_ALL", "C.UTF-8");
    sushline.env("SUSHLINE_INPUTRC", "");
    sushline.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline.env("SUSHLINE_HISTORY", history.join("\n"));

    for (editor, command) in [("Bash", bash), ("Sushline", sushline)] {
        let output = run_pty_paused_until(
            command,
            b"\x12a\xc3\x1b",
            Duration::from_millis(700),
            b"\r",
            "SUSHLINE_ACCEPTED:",
        );
        let context = format!("{editor} keys=[0x12, b'a', 0xc3, ESC] then Enter: {output:?}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("alphabet"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

/// After an incomplete UTF-8 prefix, Ctrl-W and Ctrl-Y discard the stale
/// byte and keep the prior query. GNU rings twice for a failed query, once
/// when the selected match ends at the line's end, and not at all for an
/// interior match. Enter then accepts the original or matched line.
#[test]
fn bash_readline_and_sushline_discard_stale_prefix_on_ctrl_w_and_ctrl_y() {
    require_oracle_host();
    if !c_utf8_locale_available() {
        return;
    }
    if !host_carries_measured_release() {
        eprintln!(
            "Stale-prefix Ctrl-W/Ctrl-Y was measured on Readline 8.3; this case reports nothing on other releases"
        );
        return;
    }
    let single = &["alpha"][..];
    let interior = &["alpha", "alphabet"][..];
    let env = [("LC_ALL", "C.UTF-8")];
    for (name, keys, history, bells, accepted) in [
        (
            "ctrl_w_empty",
            b"\x12\xc3\x17\n\n".as_slice(),
            single,
            2,
            "",
        ),
        (
            "ctrl_w_terminal",
            b"\x12a\xc3\x17\n\n".as_slice(),
            single,
            1,
            "alpha",
        ),
        (
            "ctrl_y_empty",
            b"\x12\xc3\x19\n\n".as_slice(),
            single,
            2,
            "",
        ),
        (
            "ctrl_y_terminal",
            b"\x12a\xc3\x19\n\n".as_slice(),
            single,
            1,
            "alpha",
        ),
        (
            "ctrl_w_interior",
            b"\x12a\xc3\x17\n\n".as_slice(),
            interior,
            0,
            "alphabet",
        ),
        (
            "ctrl_y_interior",
            b"\x12a\xc3\x19\n\n".as_slice(),
            interior,
            0,
            "alphabet",
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_inputrc_file_and_history_and_env(keys, "", history, &env),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history_and_env(keys, "", history, &env),
            ),
        ] {
            let context = format!("{editor} {name} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(accepted),
                "{context}"
            );
        }
    }
}

/// The same frozen-search path applies six other complete ESC-prefixed
/// bindings: up, down, Delete, Page Up, Ctrl-left and Meta-b. Each case pins
/// GNU's accepted line and bell count against Sushline.
#[test]
fn bash_readline_and_sushline_apply_more_key_sequences_after_search_nul_freeze() {
    require_oracle_host();
    let history = &["abc", "abd"][..];
    for (name, seq, bells, accepted) in [
        ("up", b"\x1b[A".as_slice(), 1, Some("Xabc")),
        ("down", b"\x1b[B".as_slice(), 0, Some("abdX")),
        ("delete", b"\x1b[3~".as_slice(), 0, Some("Xbc")),
        ("page_up", b"\x1b[5~".as_slice(), 1, Some("Xabc")),
        ("ctrl_left", b"\x1b[1;5D".as_slice(), 0, Some("Xabc")),
        ("meta_b", b"\x1bb".as_slice(), 0, Some("Xabc")),
    ] {
        let mut keys = b"\x12abc\x00".to_vec();
        keys.extend_from_slice(seq);
        keys.extend_from_slice(b"X\r\r");
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(&keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(&keys, "", history),
            ),
        ] {
            let context = format!("{name} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// A bound action, not just a direct editing command, runs after the frozen
/// search is committed: GNU applies both a macro string and a named command
/// bound to the same ESC sequence.
#[test]
fn bash_readline_and_sushline_apply_bound_actions_after_search_nul_freeze() {
    require_oracle_host();
    let history = &["abc", "abd"][..];
    for (name, inputrc, keys, bells, accepted) in [
        (
            "macro",
            r#""\e[D": "X""#,
            b"\x12abc\x00\x1b[D\r\r".as_slice(),
            0,
            Some("Xabc"),
        ),
        (
            "named_command",
            r#""\e[D": beginning-of-line"#,
            b"\x12abc\x00\x1b[DX\r\r".as_slice(),
            0,
            Some("Xabc"),
        ),
    ] {
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(keys, inputrc, history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, inputrc, history),
            ),
        ] {
            let context = format!("{name} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// After an unquoted NUL freezes an incremental search on the measured Bash
/// 5.3 host, ESC-prefixed arrow bindings act on the established match line at
/// the match point: left (CSI and SS3) rings at column zero, while right, Home
/// and End move or keep point without a bell. Only these five measured keys
/// are claimed.
#[test]
fn bash_readline_and_sushline_apply_arrow_keys_after_search_nul_freeze() {
    require_oracle_host();
    let history = &["abc", "abd"][..];
    for (name, seq, bells, accepted) in [
        ("left", b"\x1b[D".as_slice(), 1, Some("Xabc")),
        ("right", b"\x1b[C".as_slice(), 0, Some("aXbc")),
        ("home", b"\x1b[H".as_slice(), 0, Some("Xabc")),
        ("end", b"\x1b[F".as_slice(), 0, Some("abcX")),
        ("ss3_left", b"\x1bOD".as_slice(), 1, Some("Xabc")),
    ] {
        let mut keys = b"\x12abc\x00".to_vec();
        keys.extend_from_slice(seq);
        keys.extend_from_slice(b"X\r\r");
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(&keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(&keys, "", history),
            ),
        ] {
            let context = format!("{name} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// After an unquoted NUL freezes an incremental search, GNU ends the search
/// and applies the measured editing, history, completion and display keys to
/// the established match line. Other measured query-editing control bytes
/// keep the frozen query and ring once for a successful freeze. This fixture
/// pins only these codes.
#[test]
fn bash_readline_and_sushline_apply_control_commands_after_search_nul_freeze() {
    require_oracle_host();
    for (name, control, bells, accepted) in [
        ("delete_char", 0x04u8, 0, Some("Xbc")),
        ("complete", 0x09, 1, Some("Xabc")),
        ("kill_line", 0x0b, 0, Some("X")),
        ("clear_screen", 0x0c, 0, Some("Xabc")),
        ("next_history", 0x0e, 0, Some("abdX")),
        ("previous_history", 0x10, 1, Some("Xabc")),
        ("transpose_chars", 0x14, 1, Some("Xabc")),
        ("unbound_caret", 0x1e, 1, Some("Xabc")),
        ("undo", 0x1f, 1, Some("Xabc")),
        ("backspace", 0x08, 1, Some("abc")),
        ("unix_word_rubout", 0x17, 1, Some("abc")),
        ("ctrl_x_prefix", 0x18, 1, Some("abc")),
        ("yank", 0x19, 1, Some("abc")),
        ("char_search_prefix", 0x1d, 1, Some("abc")),
        ("delete_backward", 0x7f, 1, Some("abc")),
    ] {
        let mut keys = b"\x12abc\x00".to_vec();
        keys.extend_from_slice(&[control, b'X', b'\r', b'\r']);
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(&keys, "", &["abc", "abd"]),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(&keys, "", &["abc", "abd"]),
            ),
        ] {
            let context = format!("{name} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// After an unquoted NUL freezes an incremental search, complete C-x bindings
/// and C-] followed by a target character act on the established match line.
/// Quoted-insert instead consumes the next byte while the query stays frozen.
/// This fixture pins only these six measured sequences.
#[test]
fn bash_readline_and_sushline_apply_control_prefixes_after_search_nul_freeze() {
    require_oracle_host();
    for (name, seq, bells, accepted) in [
        (
            "exchange_point_and_mark",
            b"\x18\x18".as_slice(),
            0,
            Some("Yabc"),
        ),
        ("undo", b"\x18\x15".as_slice(), 1, Some("Yabc")),
        ("char_search_hit", b"\x1db".as_slice(), 0, Some("aYbc")),
        ("char_search_miss", b"\x1da".as_slice(), 1, Some("Yabc")),
        ("quoted_char", b"\x16X".as_slice(), 0, Some("abc")),
        ("quoted_control", b"\x16\x01".as_slice(), 0, Some("abc")),
    ] {
        let mut keys = b"\x12abc\x00".to_vec();
        keys.extend_from_slice(seq);
        keys.extend_from_slice(b"Y\r\r");
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(&keys, "", &["abc", "abd"]),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(&keys, "", &["abc", "abd"]),
            ),
        ] {
            let context = format!("{name} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// Multibyte C-] targets must be decoded as one character; continuation bytes
/// must not leak into the established match line or the accepted input.
#[test]
fn bash_readline_and_sushline_decode_multibyte_char_search_after_search_nul_freeze() {
    require_oracle_host();
    for (name, target, history, bells, accepted) in [
        (
            "ascii_miss",
            "b".as_bytes(),
            &["abc", "abd"][..],
            1,
            Some("aYbd"),
        ),
        (
            "e_acute_miss",
            "é".as_bytes(),
            &["aéb", "abd"][..],
            2,
            Some("Yabd"),
        ),
        (
            "euro_miss",
            "€".as_bytes(),
            &["a€b", "abd"][..],
            2,
            Some("Yabd"),
        ),
    ] {
        let mut keys = b"\x12a\x00\x1d".to_vec();
        keys.extend_from_slice(target);
        keys.extend_from_slice(b"Y\r\r");
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_bindings_and_history(&keys, "", history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(&keys, "", history),
            ),
        ] {
            let context = format!("{name} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// A multibyte C-] target that lands inside the frozen match moves point there
/// in both editors and rings only for the failed freeze; a miss with a single
/// history entry rings twice. The miss-only fixture above selects the second
/// entry, so it does not exercise the hit path.
#[test]
fn bash_readline_and_sushline_multibyte_char_search_hit_after_search_nul_freeze() {
    require_oracle_host();
    let env = [("LC_ALL", "C.UTF-8")];
    for (name, history, target, bells, accepted) in [
        (
            "e_acute_hit",
            &["a\u{e9}b"][..],
            "\u{e9}".as_bytes(),
            1,
            Some("aY\u{e9}b"),
        ),
        (
            "e_acute_miss",
            &["abd"][..],
            "\u{e9}".as_bytes(),
            2,
            Some("Yabd"),
        ),
    ] {
        let mut keys = b"\x12a\x00\x1d".to_vec();
        keys.extend_from_slice(target);
        keys.extend_from_slice(b"Y\r\r");
        for (editor, output) in [
            (
                "Bash",
                run_bash_readline_with_inputrc_file_and_history_and_env(&keys, "", history, &env),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history_and_env(&keys, "", history, &env),
            ),
        ] {
            let context = format!("{name} {editor} keys={keys:?}: {output:?}");
            assert_eq!(bell_count(&output), bells, "{context}");
            assert_eq!(accepted_line(&output).as_deref(), accepted, "{context}");
        }
    }
}

/// GNU 8.3-p0 reprints a multibyte prompt from column 0 after unterminated
/// output leaves the cursor off that column, but does not clear the first
/// copy's tail; a stray multibyte glyph remains. Sushline clears the tail.
/// The fixture pins accepted-line parity and Sushline's clean screen.
#[test]
fn bash_and_sushline_reprint_prompt_after_unterminated_output() {
    require_oracle_host();
    let env = [("LC_ALL", "C.UTF-8")];
    // A multibyte glyph immediately before the invisible run is the shape
    // GNU's p0 reprint decision mishandles.
    let prompt = "SUSHLINE_READY>\u{20ac}\u{1}\u{1b}[0m\u{2} ";
    let keys = b"X\r\r";
    let bash = run_bash_readline_with_prompt(
        keys,
        prompt,
        "abc",
        &env,
        portable_pty::PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    );
    let sushline = run_sushline_harness_with_prompt(
        keys,
        prompt,
        "abc",
        &env,
        portable_pty::PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    );
    assert_eq!(
        bash.matches("SUSHLINE_READY>").count(),
        2,
        "the p0 prompt-reprint path must be exercised: {bash:?}"
    );
    assert!(
        bash.contains("\rSUSHLINE_READY>"),
        "GNU must reprint from column 0: {bash:?}"
    );
    assert_eq!(accepted_line(&bash).as_deref(), Some("X"));
    assert_eq!(accepted_line(&sushline).as_deref(), Some("X"));
    let mut screen = Screen::new(24, 80);
    screen.feed(sushline.as_bytes());
    assert_eq!(
        screen.rows_text()[0].trim_end(),
        "SUSHLINE_READY>\u{20ac} X",
        "Sushline must leave no stale prompt tail: {sushline:?}"
    );
}

#[test]
fn bash_and_sushline_long_wrapped_prompt_with_invisible_prefix_accepts_line() {
    require_oracle_host();
    let env = [("LC_ALL", "C.UTF-8")];
    let size = portable_pty::PtySize {
        rows: 24,
        cols: 12,
        pixel_width: 0,
        pixel_height: 0,
    };
    // About 240 wrapped rows at this width, with a leading invisible escape
    // run. Both p0 editors complete an edit at this length; the separate
    // abort-threshold fixture pins the shape that GNU alone cannot handle.
    let prompt = format!(
        "SUSHLINE_READY>\u{1}\u{1b}[31m\u{2}{}",
        "P".repeat(260 * 11)
    );
    let keys = b"abc\x01X\r\r";
    let bash = run_bash_readline_with_prompt(keys, &prompt, "", &env, size);
    let sushline = run_sushline_harness_with_prompt(keys, &prompt, "", &env, size);
    assert!(
        bash.contains("SUSHLINE_ACCEPTED:Xabc"),
        "GNU must complete the long-prompt edit: {bash:?}"
    );
    assert!(
        sushline.contains("SUSHLINE_ACCEPTED:Xabc"),
        "Sushline must complete the long-prompt edit: {sushline:?}"
    );
    let mut bash_screen = Screen::new(24, 12);
    bash_screen.feed(bash.as_bytes());
    let mut sushline_screen = Screen::new(24, 12);
    sushline_screen.feed(sushline.as_bytes());
    assert_eq!(
        &bash_screen.rows_text()[21..23],
        &sushline_screen.rows_text()[21..23],
        "the visible tail of the long prompt must match"
    );
}

#[test]
fn gnu_prompt_above_wrap_limit_aborts_while_sushline_accepts() {
    require_oracle_host();
    let size = portable_pty::PtySize {
        rows: 24,
        cols: 12,
        pixel_width: 0,
        pixel_height: 0,
    };
    // 268 wrapped rows at 12 columns. The pinned p0 oracle aborts while
    // rendering this prompt; Sushline completes the same edit.
    let prompt = format!("SUSHLINE_READY>\u{1}\u{1b}[31m\u{2}{}", "P".repeat(3200));
    let (status, bash) = run_bash_prompt_length_probe(&prompt, size);
    assert_eq!(
        status.as_ref().map(ToString::to_string).as_deref(),
        Some("Terminated by Aborted"),
        "GNU must abort on this prompt, not wait for input or exit otherwise: status={status:?} output={bash:?}"
    );
    let sushline =
        run_sushline_harness_with_prompt(b"X\r\r", &prompt, "", &[("LC_ALL", "C.UTF-8")], size);
    assert!(
        sushline.contains("SUSHLINE_ACCEPTED:X"),
        "Sushline must complete the long-prompt edit: {sushline:?}"
    );
}
