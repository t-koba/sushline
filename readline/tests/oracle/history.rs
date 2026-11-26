//! Bash 5.3 runtime comparisons for editor history navigation, search,
//! expansion, and persistence. PTY cases and `history -p` output are distinct
//! evidence types from Rust History API tests; `COMPATIBILITY.md` scopes them.

use super::host::require_oracle_host;
use super::pty::*;
use history::expansion::{HistoryChars, HistoryExpansionPolicy, expand_history};
use readline::History;
use std::fs;
use std::process::Command;

#[test]
fn bash_readline_and_sushline_numeric_history_navigation_counts() {
    let history = ["one", "two", "three", "four", "five"];
    for (entries, keys, expected, bells) in [
        (&history[..], &b"\x1b2\x10\r"[..], "four", 0),
        (&history[..], &b"\x1b3\x10\r"[..], "three", 0),
        (&history[..], &b"\x1b9\x10\r"[..], "one", 0),
        (&history[..], &b"\x1b0\x10\r"[..], "", 0),
        (&history[..], &b"\x1b-1\x10\r"[..], "", 1),
        (&history[..], &b"\x10\x1b-1\x10\r"[..], "", 0),
        (&history[..], &b"\x10\x10\x10\x1b-1\x10\r"[..], "four", 0),
        (&history[..], &b"\x10\x10\x10\x1b-2\x10\r"[..], "five", 0),
        (
            &history[..],
            &b"\x10\x10\x10\x10\x10\x1b-2\x10\r"[..],
            "three",
            0,
        ),
        (&history[..], &b"\x10\r"[..], "five", 0),
        (&history[..], &b"\x10\x10\x10\x1b9\x10\r"[..], "one", 0),
        (&history[..], &b"\x10\x10\x10\x1b0\x10\r"[..], "three", 0),
        (&history[..], &b"\x10\x10\x10\x1b9\x0e\r"[..], "", 0),
        (&history[..], &b"\x10\x10\x10\x1b2\x0e\r"[..], "five", 0),
        (&history[..], &b"\x10\x10\x10\x1b0\x0e\r"[..], "three", 0),
        (&history[..], &b"\x10\x10\x10\x0e\r"[..], "four", 0),
        (&history[..], &b"\x10\x10\x10\x1b-1\x0e\r"[..], "two", 0),
        (&history[..], &b"\x10\x10\x10\x1b-2\x0e\r"[..], "one", 0),
        (&history[..], &b"\x1b-1\x0e\r"[..], "five", 0),
        (&history[..], &b"\x1b-2\x0e\r"[..], "four", 0),
        (&history[..], &b"\x10X\x1b2\x10\r"[..], "three", 0),
        (&history[..], &b"\x10\x10\x10X\x1b2\x0e\r"[..], "five", 0),
        (&history[..], &b"\x10\x10\x10X\x1b-1\x0eY\r"[..], "twoY", 0),
        (&[][..], &b"\x1b2\x10\r"[..], "", 0),
        (&[][..], &b"\x1b-1\x10\r"[..], "", 1),
        (&[][..], &b"\x1b2\x0e\r"[..], "", 1),
        (&[][..], &b"\x1b-1\x0e\r"[..], "", 0),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, "", entries);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", entries);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{editor}: keys={keys:?} history={entries:?}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_numeric_accept_and_revert_parity() {
    let inputrc = r#""\er": revert-line"#;
    for (desc, keys, expected, bells) in [
        ("accept_2", &b"abc\x1b2\r"[..], "abc", 0),
        ("accept_0", &b"abc\x1b0\r"[..], "abc", 0),
        ("accept_neg1", &b"abc\x1b-1\r"[..], "abc", 0),
        ("revert_2", &b"abc\x01X\x1b2\x1br\r"[..], "", 0),
        ("revert_0", &b"abc\x01X\x1b0\x1br\r"[..], "", 0),
        ("revert_neg1", &b"abc\x01X\x1b-1\x1br\r"[..], "", 0),
        ("revert_uncounted", &b"abc\x01X\x1br\r"[..], "", 0),
        ("revert_clean_2", &b"abc\x1b2\x1br\r"[..], "", 0),
        ("revert_then_insert", &b"abc\x01X\x1b2\x1brY\r"[..], "Y", 0),
        ("revert_twice", &b"abc\x01X\x1br\x1b2\x1brY\r"[..], "Y", 1),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &[]);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &[]);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{desc}: {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_numeric_undo_parity() {
    for (desc, keys, expected, bells) in [
        ("undo_1", &b"abc\x01X\x05Y\x1b1\x1f\r"[..], "Xabc", 0),
        ("undo_2", &b"abc\x01X\x05Y\x1b2\x1f\r"[..], "abc", 0),
        ("undo_3", &b"abc\x01X\x05Y\x1b3\x1f\r"[..], "", 0),
        ("undo_4", &b"abc\x01X\x05Y\x1b4\x1f\r"[..], "", 1),
        ("undo_0", &b"abc\x01X\x05Y\x1b0\x1f\r"[..], "XabcY", 0),
        ("undo_neg1", &b"abc\x01X\x05Y\x1b-1\x1f\r"[..], "XabcY", 0),
        ("undo_neg2", &b"abc\x01X\x05Y\x1b-2\x1f\r"[..], "XabcY", 0),
        ("undo_uncounted", &b"abc\x01X\x05Y\x1f\r"[..], "Xabc", 0),
        ("undo_empty", &b"\x1f\r"[..], "", 1),
        (
            "kill_then_undo_1",
            &b"abc\x01X\x05Y\x15\x1b1\x1f\r"[..],
            "XabcY",
            0,
        ),
        (
            "kill_then_undo_2",
            &b"abc\x01X\x05Y\x15\x1b2\x1f\r"[..],
            "Xabc",
            0,
        ),
        (
            "kill_then_undo_3",
            &b"abc\x01X\x05Y\x15\x1b3\x1f\r"[..],
            "abc",
            0,
        ),
        (
            "kill_then_undo_4",
            &b"abc\x01X\x05Y\x15\x1b4\x1f\r"[..],
            "",
            0,
        ),
        (
            "kill_then_undo_uncounted",
            &b"abc\x01X\x05Y\x15\x1f\r"[..],
            "XabcY",
            0,
        ),
        (
            "kill_then_undo_0",
            &b"abc\x01X\x05Y\x15\x1b0\x1f\r"[..],
            "",
            0,
        ),
        (
            "kill_then_undo_neg1",
            &b"abc\x01X\x05Y\x15\x1b-1\x1f\r"[..],
            "",
            0,
        ),
        (
            "unix_word_rubout_then_undo_1",
            &b"abc\x01X\x05Y\x17\x1b1\x1f\r"[..],
            "XabcY",
            0,
        ),
        (
            "unix_word_rubout_then_undo_uncounted",
            &b"abc\x01X\x05Y\x17\x1f\r"[..],
            "XabcY",
            0,
        ),
        (
            "backward_kill_word_then_undo_1",
            &b"abc\x01X\x05Y\x1b\x7f\x1b1\x1f\r"[..],
            "XabcY",
            0,
        ),
        (
            "backward_kill_word_then_undo_uncounted",
            &b"abc\x01X\x05Y\x1b\x7f\x1f\r"[..],
            "XabcY",
            0,
        ),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, "", &[]);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &[]);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{desc}: {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_numeric_character_search_parity() {
    for (keys, expected, bells) in [
        (&b"aXbXc\x1daZ\r"[..], "aXbXcZ", 1),
        (&b"aXbXc\x1b1\x1daZ\r"[..], "aXbXcZ", 1),
        (&b"aXbXc\x1b2\x1daZ\r"[..], "aXbXcZ", 1),
        (&b"aXbXc\x1b3\x1daZ\r"[..], "aXbXcZ", 1),
        (&b"aXbXc\x1b4\x1daZ\r"[..], "aXbXcZ", 1),
        (&b"aXbXc\x1b0\x1daZ\r"[..], "aXbXcZ", 0),
        (&b"aXbXc\x1b-1\x1daZ\r"[..], "ZaXbXc", 0),
        (&b"aXbXc\x1b-2\x1daZ\r"[..], "ZaXbXc", 1),
        (&b"aXbXc\x1b\x1daZ\r"[..], "ZaXbXc", 0),
        (&b"aXbXc\x1b1\x1b\x1daZ\r"[..], "ZaXbXc", 0),
        (&b"aXbXc\x1b2\x1b\x1daZ\r"[..], "ZaXbXc", 1),
        (&b"aXbXc\x1b0\x1b\x1daZ\r"[..], "aXbXcZ", 0),
        (&b"aXbXc\x1b-1\x1b\x1daZ\r"[..], "aXbXcZ", 1),
        (&b"aXbXc\x1b-2\x1b\x1daZ\r"[..], "aXbXcZ", 1),
        (&b"abcabc\x01\x1b2\x1daX\r"[..], "abcXabc", 1),
        (&b"abcabc\x01\x1b0\x1daX\r"[..], "Xabcabc", 0),
        (&b"abcabc\x01\x1b-1\x1daX\r"[..], "Xabcabc", 1),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, "", &[]);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &[]);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{editor}: keys={keys:?}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
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
fn bash_readline_and_sushline_prefix_history_search_point_and_mark() {
    let inputrc = r#""\C-]": history-search-backward"#;
    let history = ["alpha one", "beta", "alpha two"];
    for (keys, expected) in [
        (&b"alp\x01\x00\x05\x1dX\r"[..], "alpXha two"),
        (&b"alp\x01\x00\x05\x1d\x18\x18X\r"[..], "alpha twoX"),
        (&b"alp\x01\x00\x05\x10\x18\x18X\r"[..], "Xalpha two"),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{editor} keys={keys:?}\n{output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), 0, "{context}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_substring_history_search_point_and_mark() {
    for (history, point_only, mark_exchange) in [
        (
            &["needle old", "needle mid", "needle new"][..],
            "Xneedle mid",
            "needle midX",
        ),
        (
            &["xxneedle old", "other", "xxneedle mid", "xxneedle new"][..],
            "xxXneedle mid",
            "xxneedle midX",
        ),
    ] {
        for (command, start) in [
            ("history-substring-search-backward", &b"\x10"[..]),
            ("history-substring-search-forward", &b"\x10\x10\x10"[..]),
        ] {
            let inputrc = format!("\"\\C-o\": {command}");
            for (finish, expected) in [
                (&b"\x18\x18X\r"[..], mark_exchange),
                (&b"X\r"[..], point_only),
            ] {
                let keys = [start, b"\x01\x0bneedle\x01\x00\x05\x0f", finish].concat();
                let bash = run_bash_readline_with_bindings_and_history(&keys, &inputrc, history);
                let sushline =
                    run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, history);
                for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                    let context = format!(
                        "{editor} command={command} history={history:?} keys={keys:?}\n{output}"
                    );
                    assert_eq!(
                        accepted_line(output).as_deref(),
                        Some(expected),
                        "{context}"
                    );
                    assert_eq!(bell_count(output), 0, "{context}");
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_backward_substring_search_uses_last_occurrence() {
    let inputrc = r#""\C-o": history-substring-search-backward"#;
    let history = ["echo alpha-first alpha-last"];
    let keys = b"alpha\x0fX\r";
    for (editor, output) in [
        (
            "bash",
            run_bash_readline_with_bindings_and_history(keys, inputrc, &history),
        ),
        (
            "sushline",
            run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history),
        ),
    ] {
        let context = format!("{editor} history={history:?} keys={keys:?}\n{output}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("echo alpha-first Xalpha-last"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 0, "{context}");
    }
}

#[test]
fn bash_readline_and_sushline_substring_search_point_follows_effective_direction() {
    let history = [
        "alpha aa alpha old",
        "alpha bb alpha mid",
        "alpha cc alpha new",
    ];
    for (command, ups, point) in [
        ("history-substring-search-forward", 0, "alpha cc Xalpha new"),
        (
            "history-substring-search-backward",
            2,
            "Xalpha cc alpha new",
        ),
    ] {
        let inputrc = format!("\"\\C-o\": {command}");
        for (finish, expected) in [
            (&b"X\r"[..], point),
            (&b"\x18\x18X\r"[..], "alpha cc alpha newX"),
        ] {
            let keys = [
                &b"\x10".repeat(ups)[..],
                b"\x01\x0balpha\x01\x00\x05\x1b-1\x0f".as_slice(),
                finish,
            ]
            .concat();
            for (editor, output) in [
                (
                    "bash",
                    run_bash_readline_with_bindings_and_history(&keys, &inputrc, &history),
                ),
                (
                    "sushline",
                    run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history),
                ),
            ] {
                let context =
                    format!("{editor} command={command} ups={ups} keys={keys:?}\n{output}");
                assert_eq!(
                    accepted_line(&output).as_deref(),
                    Some(expected),
                    "{context}"
                );
                assert_eq!(bell_count(&output), 0, "{context}");
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_substring_search_count_past_newest_keeps_point_at_end() {
    let history = [
        "alpha aa alpha old",
        "alpha bb alpha mid",
        "alpha cc alpha new",
    ];
    for (command, ups, arg) in [
        ("history-substring-search-backward", 2, "-2"),
        ("history-substring-search-forward", 3, "3"),
    ] {
        let inputrc = format!("\"\\C-o\": {command}");
        for finish in [&b"X\r"[..], &b"\x18\x18X\r"[..]] {
            let keys = [
                &b"\x10".repeat(ups)[..],
                format!("\x01\x0balpha\x01\x00\x05\x1b{arg}\x0f").as_bytes(),
                finish,
            ]
            .concat();
            for (editor, output) in [
                (
                    "bash",
                    run_bash_readline_with_bindings_and_history(&keys, &inputrc, &history),
                ),
                (
                    "sushline",
                    run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history),
                ),
            ] {
                let context = format!(
                    "{editor} command={command} ups={ups} arg={arg} keys={keys:?}\n{output}"
                );
                assert_eq!(
                    accepted_line(&output).as_deref(),
                    Some("alpha cc alpha newX"),
                    "{context}"
                );
                assert_eq!(bell_count(&output), 0, "{context}");
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_repeated_history_search_keeps_original_needle() {
    let backward = r#""\C-]": history-search-backward"#;
    let forward = r#""\C-^": history-search-forward"#;
    let substring_backward = r#""\C-o": history-substring-search-backward"#;
    let substring_forward = r#""\C-o": history-substring-search-forward"#;
    for (inputrc, history, keys, expected) in [
        (
            backward,
            ["alpha one", "beta", "alpha two"],
            &b"alp\x1d\x1d\r"[..],
            "alpha one",
        ),
        (
            backward,
            ["alpha one", "beta", "alpha two"],
            &b"\x1d\x1d\r"[..],
            "beta",
        ),
        (
            backward,
            ["alpha one", "alphabet two", "alpha two"],
            &b"alpha \x1d\x1d\r"[..],
            "alpha one",
        ),
        (
            backward,
            ["alpha one", "alphabet two", "alpha two"],
            &b"alpha\x1d\x1d\r"[..],
            "alphabet two",
        ),
        // Select the oldest entry, replace it with the needle, then search forward twice.
        (
            forward,
            ["alpha one", "alphabet two", "alpha two"],
            &b"\x10\x10\x10\x01\x0balpha\x1e\x1e\r"[..],
            "alpha two",
        ),
        (
            substring_backward,
            ["old alp one", "beta", "new alp two"],
            &b"alp\x0f\x0f\r"[..],
            "old alp one",
        ),
        (
            substring_forward,
            ["old alp one", "mid alp", "new alp two"],
            &b"\x10\x10\x10\x01\x0balp\x0f\x0f\r"[..],
            "new alp two",
        ),
        // Editing the recalled line must start a new search, not reuse `alp`.
        (
            backward,
            ["alpha one", "beta", "alpha two"],
            &b"alp\x1d\x01\x0bb\x1d\r"[..],
            "beta",
        ),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        let context =
            format!("keys={keys:?} history={history:?}\nbash={bash}\nsushline={sushline}");
        assert_eq!(accepted_line(&bash).as_deref(), Some(expected), "{context}");
        assert_eq!(accepted_line(&sushline), accepted_line(&bash), "{context}");
    }
}

#[test]
fn bash_readline_and_sushline_revert_line_after_prefix_history_search() {
    let keys = b"alp\x1d\x1br\r";
    let inputrc = r#""\C-]": history-search-backward
"\er": revert-line"#;
    let history = ["alpha one", "beta", "gamma"];
    // The 8.3-alpha report supplied no reproducer. This fixture checks only
    // typing `alp`, recalling the sole prefix match, reverting, and accepting;
    // pinned final-8.3 CI must establish the expected accepted buffer below.
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
    let context = format!(
        "keys={keys:?}\ninputrc={inputrc}\nseed={history:?}\n\
         bash={bash}\nsushline={sushline}"
    );

    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some("alpha one"),
        "unexpected Bash accepted buffer: {context}"
    );
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("alpha one"),
        "unexpected Sushline accepted buffer: {context}"
    );
}

#[test]
fn bash_readline_and_sushline_render_same_reverse_isearch_query_display() {
    let history = ["alpha one", "alpha two"];
    let query_marker = "(reverse-i-search)`a': alpha two";
    // Wait for the terminal model to show the exact nonempty query row before
    // aborting; the empty-query redraw cannot satisfy this screen-state barrier.
    let steps_before_query = [
        (READY_PROMPT, &b"\x12"[..]),
        ("reverse-i-search", &b"a"[..]),
    ];
    let steps_after_query = [(READY_PROMPT, &b"\r"[..])];

    let dir = tempfile::tempdir().expect("isearch display tempdir");
    let inputrc_path = dir.path().join("inputrc");
    fs::write(&inputrc_path, "").expect("write isearch display inputrc");
    let seed = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    let mut bash_command = portable_pty::CommandBuilder::new("bash");
    bash_command.env("INPUTRC", inputrc_path.as_os_str());
    bash_command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; unset HISTTIMEFORMAT HISTCONTROL HISTIGNORE; history -c; {seed}; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    let bash = run_pty_steps_until_screen_row(
        bash_command,
        &steps_before_query,
        query_marker,
        b"\x07",
        &steps_after_query,
        "SUSHLINE_ACCEPTED:",
    );

    let mut sushline_command = sushline_harness_command();
    sushline_command.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline_command.env("SUSHLINE_HISTORY", history.join("\n"));
    let sushline = run_pty_steps_until_screen_row(
        sushline_command,
        &steps_before_query,
        query_marker,
        b"\x07",
        &steps_after_query,
        "SUSHLINE_ACCEPTED:",
    );

    let screen = super::screen::Screen::new(24, 80);
    let bash_rows = screen.last_rows_starting_with(bash.as_bytes(), "(");
    let sushline_rows = screen.last_rows_starting_with(sushline.as_bytes(), "(");
    let bash_search = bash_rows.and_then(|rows| {
        rows.into_iter()
            .find(|row| row.contains("reverse-i-search"))
    });
    let sushline_search = sushline_rows.and_then(|rows| {
        rows.into_iter()
            .find(|row| row.contains("reverse-i-search"))
    });

    assert_eq!(bash_search.as_deref(), Some(query_marker), "{bash}");
    assert_eq!(sushline_search.as_deref(), Some(query_marker), "{sushline}");
    assert_eq!(accepted_line(&bash).as_deref(), Some(""), "{bash}");
    assert_eq!(accepted_line(&sushline).as_deref(), Some(""), "{sushline}");
}

#[test]
fn bash_readline_and_sushline_history_search_forward_newest_failure() {
    let inputrc = r#""\C-t": history-search-forward"#;
    let history = ["alpha one", "beta", "alpha two", "alpha three"];
    for (keys, expected, bells) in [
        (&b"\x10\x14\r"[..], "alpha three", 1),
        (&b"\x10\x1b1\x14\r"[..], "alpha three", 1),
        (&b"\x10\x1b0\x14\r"[..], "alpha three", 0),
        (&b"al\x14\r"[..], "al", 1),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{editor}: keys={keys:?}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
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
fn bash_readline_and_sushline_history_navigation_preserves_edited_entry() {
    let inputrc = "set revert-all-at-newline off";
    let history = ["alpha old", "alpha new"];
    // Edit the newest entry, visit the older one, and return. Readline 8.3
    // preserves the modified history line while traversing away from it.
    let keys = b"\x10\x05 EDIT\x10\x0e\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(
        accepted_line(&bash),
        Some("alpha new EDIT".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_prefix_search_preserves_edited_entry() {
    let inputrc = r#"set revert-all-at-newline off
set history-preserve-point on
"\C-]": history-search-backward
"\C-^": history-search-forward"#;
    let history = ["alpha old", "alpha new"];
    // Search to the newest match, edit it, then search away and back using
    // the same prefix. Readline 8.3 preserves the edit on its history entry.
    let keys = b"alp\x1d\x05 EDIT\x01\x06\x06\x06\x1d\x1e\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(
        accepted_line(&bash),
        Some("alpha new EDIT".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

fn assert_saved_history_projection(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
    expected_accepted: &str,
    expected_history: &[u8],
) {
    let dir = tempfile::tempdir().expect("history projection tempdir");
    let inputrc_path = dir.path().join("inputrc");
    fs::write(&inputrc_path, inputrc).expect("write history projection inputrc");
    let bash_path = dir.path().join("bash.history");
    let sushline_path = dir.path().join("sushline.history");

    let mut bash_command = portable_pty::CommandBuilder::new("bash");
    bash_command.env("INPUTRC", inputrc_path.as_os_str());
    bash_command.env("SUSHLINE_HISTORY_OUT", bash_path.as_os_str());
    bash_command.env("HISTFILE", bash_path.as_os_str());
    bash_command.env("HISTSIZE", "100");
    bash_command.env("HISTFILESIZE", "100");
    let seed = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    bash_command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; unset HISTTIMEFORMAT HISTCONTROL HISTIGNORE; history -c; {seed}; IFS= read -r -e -p "{READY_PROMPT}" line && history -w "$SUSHLINE_HISTORY_OUT" && unset HISTFILE && printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    let bash = run_pty_until(bash_command, keys, "SUSHLINE_ACCEPTED:");

    let mut sushline_command = sushline_harness_command();
    sushline_command.env("SUSHLINE_INPUTRC", inputrc);
    sushline_command.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline_command.env("SUSHLINE_HISTORY", history.join("\n"));
    sushline_command.env("SUSHLINE_HISTORY_OUT", sushline_path.as_os_str());
    let sushline = run_pty_until(sushline_command, keys, "SUSHLINE_ACCEPTED:");

    let bash_history = fs::read(&bash_path);
    let sushline_history = fs::read(&sushline_path);
    let context = format!(
        "keys={keys:?}\ninputrc={inputrc}\nseed={history:?}\n\
         bash={bash}\nsushline={sushline}\n\
         bash history={bash_history:?}\nsushline history={sushline_history:?}"
    );
    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some(expected_accepted),
        "unexpected Bash accepted buffer: {context}"
    );
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some(expected_accepted),
        "unexpected Sushline accepted buffer: {context}"
    );
    let bash_history =
        bash_history.unwrap_or_else(|err| panic!("read Bash history: {err}; {context}"));
    let sushline_history =
        sushline_history.unwrap_or_else(|err| panic!("read Sushline history: {err}; {context}"));
    assert_eq!(
        bash_history.as_slice(),
        expected_history,
        "unexpected Bash saved-history projection: {context}"
    );
    assert_eq!(
        sushline_history.as_slice(),
        expected_history,
        "unexpected Sushline saved-history projection: {context}"
    );
}

#[test]
fn bash_readline_and_sushline_prefix_search_saves_edited_history() {
    let inputrc = r#"set editing-mode emacs
set keymap emacs-standard
set revert-all-at-newline off
"\e[5~": history-search-backward
"#;
    let history = ["alpha old", "beta", "alpha new"];
    // Recall the newest entry, append an edit, move point after `alp`, and
    // PageUp to the older prefix match before accepting it. This checks the
    // saved entry we left behind, not just the accepted buffer. Neither host
    // adds the accepted `read -e` buffer as a new history entry here.
    let keys = b"\x10\x05 EDIT\x01\x06\x06\x06\x1b[5~\r";
    assert_saved_history_projection(
        keys,
        inputrc,
        &history,
        "alpha old",
        b"alpha old\nbeta\nalpha new EDIT\n",
    );
}

#[test]
fn bash_readline_and_sushline_prefix_search_saves_partial_edit() {
    let inputrc = r#""\e[5~": history-search-backward"#;
    // Seed the history state of the report's second prompt: the final
    // `echo hello` entry is recalled, edited to `echo w`, then PageUp searches
    // backward to `echo world`. The read harness does not execute the accepted
    // buffer, so its file isolates the partial entry before shell execution.
    let history = ["echo hello", "echo world", "echo hello"];
    let keys = b"\x10\x1bb\x0bw\x1b[5~\r";
    assert_saved_history_projection(
        keys,
        inputrc,
        &history,
        "echo world",
        b"echo hello\necho world\necho w\n",
    );
}

#[test]
fn bash_readline_and_sushline_prefix_search_steps_past_current_entry() {
    let inputrc = r#""\C-]": history-search-backward
"\C-^": history-search-forward"#;
    let history = ["alpha old", "beta", "alpha new"];

    // Starting from `alpha new`, backward prefix search must skip the current
    // entry and find `alpha old`.
    let backward_keys = b"\x10\x01\x0balp\x1d\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(backward_keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(backward_keys, inputrc, &history);
    assert_eq!(
        accepted_line(&bash),
        Some("alpha old".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // Starting from `alpha old`, forward prefix search must skip the current
    // entry and find `alpha new`.
    let forward_keys = b"\x10\x10\x10\x01\x0balp\x1e\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(forward_keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(forward_keys, inputrc, &history);
    assert_eq!(
        accepted_line(&bash),
        Some("alpha new".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_forward_substring_search_at_end_preserves_edit() {
    let inputrc = r#""\C-o": history-substring-search-forward"#;
    let history = ["needle old", "other", "needle new"];
    // From the end cursor, forward search must not wrap to `needle new`; Up
    // then Down must still restore the typed edit.
    let keys = b"needle\x0f\x10\x0e\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(accepted_line(&bash), Some("needle".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_substring_search_skips_selected_match() {
    let history = ["needle old", "needle mid", "needle new"];

    // Forward from the selected oldest matching entry advances to the middle.
    let forward_inputrc = r#""\C-o": history-substring-search-forward"#;
    let forward_keys = b"\x10\x10\x10\x01\x0bneedle\x0f\r";
    let bash =
        run_bash_readline_with_inputrc_file_and_history(forward_keys, forward_inputrc, &history);
    let sushline =
        run_sushline_harness_with_inputrc_and_history(forward_keys, forward_inputrc, &history);
    assert_eq!(
        accepted_line(&bash),
        Some("needle mid".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // Backward from the selected newest matching entry also skips itself.
    let backward_inputrc = r#""\C-o": history-substring-search-backward"#;
    let backward_keys = b"\x10\x01\x0bneedle\x0f\r";
    let bash =
        run_bash_readline_with_inputrc_file_and_history(backward_keys, backward_inputrc, &history);
    let sushline =
        run_sushline_harness_with_inputrc_and_history(backward_keys, backward_inputrc, &history);
    assert_eq!(
        accepted_line(&bash),
        Some("needle mid".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn search_ignore_case_applies_to_history_navigation_commands() {
    if !c_utf8_locale_available() {
        return;
    }

    let locale_env = [("LC_ALL", "C.UTF-8")];
    type SearchCase<'a> = (
        &'a str,
        &'a str,
        &'a [&'a str],
        &'a [u8],
        &'a str,
        u8,
        &'a str,
    );
    let cases: &[SearchCase<'_>] = &[
        (
            r#""\C-o""#,
            "history-search-backward",
            &["ALPHA old"],
            b"",
            "alpha",
            0x0f,
            "ALPHA old",
        ),
        (
            r#""\C-o""#,
            "history-substring-search-backward",
            &["ALPHA old"],
            b"",
            "alpha",
            0x0f,
            "ALPHA old",
        ),
        (
            r#""\C-^""#,
            "history-search-forward",
            &["alpha old", "ALPHA new"],
            b"\x10\x10\x01\x0b",
            "alpha",
            0x1e,
            "ALPHA new",
        ),
        (
            r#""\C-o""#,
            "history-substring-search-forward",
            &["alpha old", "ALPHA new"],
            b"\x10\x10\x01\x0b",
            "alpha",
            0x0f,
            "ALPHA new",
        ),
    ];
    for (binding, command, history, prefix, query, search_key, expected) in cases {
        let inputrc = format!("set search-ignore-case on\n{binding}: {command}");
        let mut keys = prefix.to_vec();
        keys.extend_from_slice(query.as_bytes());
        keys.push(*search_key);
        keys.push(b'\r');
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            &inputrc,
            history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            &inputrc,
            history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some((*expected).to_string()),
            "{command}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some((*expected).to_string()),
            "{command}: {sushline}"
        );
    }
}

#[test]
fn history_navigation_search_is_case_sensitive_by_default() {
    if !c_utf8_locale_available() {
        return;
    }

    // Default `search-ignore-case off` stays case-sensitive, even in a UTF-8
    // locale. Fixture-scoped Bash 5.3 PTY observation, not GNU source proof.
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for (history, query) in [(["ALPHA old"], "alpha"), (["ÄLPHA old"], "älpha")] {
        let history: &[&str] = &history;
        for command in [
            "history-search-backward",
            "history-substring-search-backward",
        ] {
            let inputrc = format!("\"\\C-o\": {command}");
            let mut keys = query.as_bytes().to_vec();
            keys.extend_from_slice(b"\x0f\r");
            let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
                &keys,
                &inputrc,
                history,
                &locale_env,
            );
            let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
                &keys,
                &inputrc,
                history,
                &locale_env,
            );
            assert_eq!(
                accepted_line(&bash),
                Some(query.to_string()),
                "{command} {query}: {bash}"
            );
            assert_eq!(
                accepted_line(&sushline),
                accepted_line(&bash),
                "{command} {query}: bash={bash}\nsushline={sushline}"
            );
        }
    }
}

#[test]
fn history_navigation_forward_search_is_case_sensitive_by_default() {
    if !c_utf8_locale_available() {
        return;
    }

    // Default `search-ignore-case off` stays case-sensitive for forward
    // navigation as well. Fixture-scoped Bash 5.3 PTY observation, not GNU
    // source proof.
    let locale_env = [("LC_ALL", "C.UTF-8")];
    type ForwardCase<'a> = (&'a str, &'a str, &'a [&'a str], &'a [u8], &'a str, u8);
    let cases: &[ForwardCase<'_>] = &[
        (
            "\"\\C-^\"",
            "history-search-forward",
            &["alpha old", "ALPHA new"],
            b"\x10\x10\x01\x0b",
            "alpha",
            0x1e,
        ),
        (
            "\"\\C-o\"",
            "history-substring-search-forward",
            &["alpha old", "ALPHA new"],
            b"\x10\x10\x01\x0b",
            "alpha",
            0x0f,
        ),
        (
            "\"\\C-^\"",
            "history-search-forward",
            &["älpha old", "ÄLPHA new"],
            b"\x10\x10\x01\x0b",
            "älpha",
            0x1e,
        ),
        (
            "\"\\C-o\"",
            "history-substring-search-forward",
            &["älpha old", "ÄLPHA new"],
            b"\x10\x10\x01\x0b",
            "älpha",
            0x0f,
        ),
    ];
    for (binding, command, history, prefix, query, search_key) in cases {
        let inputrc = format!("{binding}: {command}");
        let mut keys = prefix.to_vec();
        keys.extend_from_slice(query.as_bytes());
        keys.push(*search_key);
        keys.push(b'\r');
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            &inputrc,
            history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            &inputrc,
            history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some((*query).to_string()),
            "{command} {query}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{command} {query}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn history_prefix_search_ignore_case_matches_pinned_utf8_fixture() {
    if !c_utf8_locale_available() {
        return;
    }

    // The pinned Readline 8.3 primary and fresh-replica reports agree for this
    // exact fixture; this separately guards the Sushline editor behavior.
    let history = ["ÄLPHA old"];
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let inputrc = "set search-ignore-case on\n\"\\C-o\": history-search-backward";
    let mut keys = "älpha".as_bytes().to_vec();
    keys.extend_from_slice(b"\x0f\r");
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );
    assert_eq!(
        accepted_line(&bash),
        Some("ÄLPHA old".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("ÄLPHA old".to_string()),
        "{sushline}"
    );
}

#[test]
fn history_substring_search_ignore_case_matches_locale_case_in_utf8_locale() {
    if !c_utf8_locale_available() {
        return;
    }

    // Bash 5.3 PTY runtime evidence under C.UTF-8; this does not establish the
    // host's linked Readline version.
    let history = ["ÄLPHA old"];
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let command = "history-substring-search-backward";
    let inputrc = format!("set search-ignore-case on\n\"\\C-o\": {command}");
    let mut keys = "älpha".as_bytes().to_vec();
    keys.extend_from_slice(b"\x0f\r");
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        &keys,
        &inputrc,
        &history,
        &locale_env,
    );
    let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
        &keys,
        &inputrc,
        &history,
        &locale_env,
    );
    assert_eq!(
        accepted_line(&bash),
        Some("ÄLPHA old".to_string()),
        "{command}: {bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("ÄLPHA old".to_string()),
        "{command}: {sushline}"
    );
}

#[test]
fn history_navigation_search_ignore_case_matches_locale_case_in_forward_commands() {
    if !c_utf8_locale_available() {
        return;
    }

    // Bash 5.3 PTY runtime evidence under C.UTF-8; this is not source evidence
    // for the host-linked Readline version.
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let history = ["älpha old", "ÄLPHA new"];
    let cases = [
        ("history-search-backward", "history-search-forward"),
        (
            "history-substring-search-backward",
            "history-substring-search-forward",
        ),
    ];
    for (backward, forward) in cases {
        let inputrc =
            format!("set search-ignore-case on\n\"\\C-o\": {backward}\n\"\\C-^\": {forward}");
        // The longer query selects the older lowercase entry; then search
        // forward with `älpha` to the uppercase entry.
        let mut keys = "älpha o".as_bytes().to_vec();
        keys.extend_from_slice(b"\x0f\x01\x0b");
        keys.extend_from_slice("älpha".as_bytes());
        keys.extend_from_slice(b"\x1e\r");
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some("ÄLPHA new".to_string()),
            "{forward}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some("ÄLPHA new".to_string()),
            "{forward}: {sushline}"
        );
    }
}

#[test]
fn search_ignore_case_matches_non_ascii_in_utf8_locale() {
    if !c_utf8_locale_available() {
        return;
    }

    // Direct Readline 8.3 primary and fresh-replica reports agree for this
    // exact fixture; this Bash 5.3/Sushline PTY test guards editor behavior.
    // Readline 8.3 isearch.c uses _rl_mb_strcaseeqn when MB_CUR_MAX > 1.
    let inputrc = "set search-ignore-case on";
    let history = ["ÄLPHA old"];
    let mut keys = vec![0x12];
    keys.extend_from_slice("älpha".as_bytes());
    keys.push(b'\r');
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );

    assert_eq!(
        accepted_line(&bash),
        Some("ÄLPHA old".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("ÄLPHA old".to_string()),
        "{sushline}"
    );
}

#[test]
fn history_forward_incremental_search_ignore_case_matches_pinned_utf8_fixture() {
    if !c_utf8_locale_available() {
        return;
    }

    // Direct Readline 8.3 primary and fresh-replica reports agree for this
    // exact fixture; this separately guards Bash/Sushline editor behavior.
    let history = ["prior", "ÄLPHA old"];
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let inputrc = "set search-ignore-case on\n\"\\C-o\": forward-search-history";
    let mut keys = vec![0x10, 0x10, 0x0f];
    keys.extend_from_slice("älpha".as_bytes());
    keys.extend_from_slice(b"\r\r");
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );
    assert_eq!(
        accepted_line(&bash),
        Some("ÄLPHA old".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("ÄLPHA old".to_string()),
        "{sushline}"
    );
}

#[test]
fn history_forward_incremental_search_matches_pinned_utf8_no_expansion_fixture() {
    if !c_utf8_locale_available() {
        return;
    }

    // Match only the direct Readline 8.3 primary/fresh-replica fixture:
    // C-p twice selects `prior`; the failed `ss` search leaves it accepted.
    let history = ["prior", "ß entry"];
    let keys = b"\x10\x10\x0fss\n\n";
    let inputrc = "set search-ignore-case on\n\"\\C-o\": forward-search-history\n";
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline =
        run_sushline_harness_with_inputrc_and_history_and_env(keys, inputrc, &history, &locale_env);

    assert_eq!(accepted_line(&bash), Some("prior".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("prior".to_string()),
        "{sushline}"
    );
}

#[test]
fn history_reverse_incremental_search_matches_pinned_utf8_no_expansion_fixture() {
    if !c_utf8_locale_available() {
        return;
    }

    // Match the direct Readline 8.3 primary/fresh-replica fixture, including
    // C-r to repeat the failed search and two newlines to exit and accept.
    let history = ["ß entry"];
    let keys = b"\x12ss\x12\n\n";
    let inputrc = "set search-ignore-case on";
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline =
        run_sushline_harness_with_inputrc_and_history_and_env(keys, inputrc, &history, &locale_env);

    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(accepted_line(&sushline), Some(String::new()), "{sushline}");
}

#[test]
fn history_reverse_incremental_search_matches_pinned_utf8_query_expansion_fixture() {
    if !c_utf8_locale_available() {
        return;
    }

    // Direct Readline 8.3 primary/fresh-replica fixture: query ß against SS
    // fails, leaving a non-null empty line after two newlines.
    let history = ["SS"];
    let keys = b"\x12\xc3\x9f\n\n";
    let inputrc = "set search-ignore-case on";
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline =
        run_sushline_harness_with_inputrc_and_history_and_env(keys, inputrc, &history, &locale_env);

    assert_eq!(accepted_line(&bash), Some(String::new()), "{bash}");
    assert_eq!(accepted_line(&sushline), Some(String::new()), "{sushline}");
}

#[test]
fn history_forward_incremental_search_matches_pinned_utf8_query_expansion_fixture() {
    if !c_utf8_locale_available() {
        return;
    }

    // Direct Readline 8.3 primary/fresh-replica fixture: C-p twice selects
    // prior; the failed query ß against SS leaves prior accepted.
    let history = ["prior", "SS"];
    let keys = b"\x10\x10\x0f\xc3\x9f\n\n";
    let inputrc = "set search-ignore-case on\n\"\\C-o\": forward-search-history\n";
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline =
        run_sushline_harness_with_inputrc_and_history_and_env(keys, inputrc, &history, &locale_env);

    assert_eq!(accepted_line(&bash), Some("prior".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("prior".to_string()),
        "{sushline}"
    );
}

#[test]
fn search_ignore_case_matches_measured_utf8_single_scalar_pairs() {
    if !c_utf8_locale_available() {
        return;
    }

    let inputrc = "set search-ignore-case on";
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for (uppercase, lowercase) in [("Ä", "ä"), ("Σ", "σ"), ("K", "k"), ("İ", "i"), ("ẞ", "ß")]
    {
        let history_line = format!("{uppercase} entry");
        let history = [history_line.as_str()];
        let mut keys = vec![0x12];
        keys.extend_from_slice(lowercase.as_bytes());
        keys.push(b'\r');
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            inputrc,
            &history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            inputrc,
            &history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some(history_line.clone()),
            "{uppercase}/{lowercase}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{uppercase}/{lowercase}: bash={bash}\nsushline={sushline}"
        );
    }

    for (entry, query) in [("Σ entry", "ς"), ("ß entry", "ss")] {
        let history = [entry];
        let mut keys = vec![0x12];
        keys.extend_from_slice(query.as_bytes());
        keys.push(b'\r');
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            inputrc,
            &history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            inputrc,
            &history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some(String::new()),
            "{entry}/{query}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{entry}/{query}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn history_reverse_incremental_search_matches_pinned_c_locale_meta_input_fixture() {
    let inputrc = "set search-ignore-case on";
    let locale_env = [("LC_ALL", "C")];
    let history = ["ÄLPHA old"];
    let keys = b"\x12\xc3\xa4lpha\n\n";
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline =
        run_sushline_harness_with_inputrc_and_history_and_env(keys, inputrc, &history, &locale_env);
    // Pinned Readline 8.3 primary and fresh replica accept lpha: meta
    // conversion exits the empty search, rather than testing Unicode folding.
    for (name, output) in [("bash", &bash), ("sushline", &sushline)] {
        assert_eq!(
            accepted_line(output).as_deref(),
            Some("lpha"),
            "{name}={output}"
        );
    }
}

#[test]
fn history_reverse_incremental_search_matches_pinned_c_locale_input_meta_fixture() {
    let inputrc = "set search-ignore-case on\nset convert-meta off\nset input-meta on";
    let locale_env = [("LC_ALL", "C")];
    let history = ["ÄLPHA old"];
    let keys = b"\x12\xc3\xa4lpha\n\n";
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline =
        run_sushline_harness_with_inputrc_and_history_and_env(keys, inputrc, &history, &locale_env);
    // Both pinned Readline 8.3 reports leave the empty search and accept
    // the raw query as ordinary input; this does not test Unicode folding.
    for (name, output) in [("bash", &bash), ("sushline", &sushline)] {
        assert_eq!(
            accepted_line(output).as_deref().map(str::as_bytes),
            Some(&b"\xc3\xa4lpha"[..]),
            "{name}={output}"
        );
    }
    assert!(!sushline.contains("failed reverse-i-search"), "{sushline}");
}

#[test]
fn history_reverse_incremental_search_matches_pinned_c_locale_input_meta_ascii_prefix_fixture() {
    let inputrc = "set search-ignore-case on\nset convert-meta off\nset input-meta on";
    let locale_env = [("LC_ALL", "C")];
    let history = ["ÄLPHA old"];
    let keys = b"\x12a\xc3\xa4lpha\n\n";
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline =
        run_sushline_harness_with_inputrc_and_history_and_env(keys, inputrc, &history, &locale_env);
    // Both pinned Readline 8.3 reports match ASCII a, then leave search
    // on the first high byte and insert ordinary input at the match start.
    // This fixture does not establish Unicode search behavior.
    for (name, output) in [("bash", &bash), ("sushline", &sushline)] {
        assert_eq!(
            accepted_line(output).as_deref().map(str::as_bytes),
            Some(&b"\xc3\x84LPH\xc3\xa4lphaA old"[..]),
            "{name}={output}"
        );
    }
}

#[test]
fn search_ignore_case_falls_back_to_ascii_in_c_locale() {
    let inputrc = "set search-ignore-case on";
    let locale_env = [("LC_ALL", "C")];
    let history = ["ALPHA old"];
    let keys = b"\x12alpha\r";
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline =
        run_sushline_harness_with_inputrc_and_history_and_env(keys, inputrc, &history, &locale_env);
    assert_eq!(
        accepted_line(&bash),
        Some("ALPHA old".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
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
fn nonincremental_search_ignore_case_matches_non_ascii_in_utf8_locale() {
    if !c_utf8_locale_available() {
        return;
    }

    let inputrc = r#""\C-o": non-incremental-reverse-search-history
set search-ignore-case on"#;
    let history = ["ÄLPHA old"];
    let mut keys = vec![0x0f];
    keys.extend_from_slice("älpha".as_bytes());
    // The first Enter completes the search; the second accepts the found line.
    keys.extend_from_slice(b"\r\r");
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );

    assert_eq!(
        accepted_line(&bash),
        Some("ÄLPHA old".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("ÄLPHA old".to_string()),
        "{sushline}"
    );
}

#[test]
fn nonincremental_search_ignore_case_repeat_matches_in_utf8_locale() {
    if !c_utf8_locale_available() {
        return;
    }

    let inputrc = r#""\C-o": non-incremental-reverse-search-history
"\C-]": non-incremental-reverse-search-history-again
set search-ignore-case on"#;
    let history = ["ÄLPHA old", "ÄLPHA new"];
    let mut keys = vec![0x0f];
    keys.extend_from_slice("älpha".as_bytes());
    // Execute the newest match, repeat backward to the older match, then accept.
    keys.extend_from_slice(b"\r\x1d\r");
    let locale_env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );
    let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
        &keys,
        inputrc,
        &history,
        &locale_env,
    );

    assert_eq!(
        accepted_line(&bash),
        Some("ÄLPHA old".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_repeat_completed_search_across_reads() {
    // GNU retains the completed non-incremental search across reads: the
    // repeat on the second read finds the older match instead of dinging.
    let inputrc = "\"\\C-o\": non-incremental-reverse-search-history\n\"\\C-]\": non-incremental-reverse-search-history-again";
    let history = ["foo old", "foo new"];
    let keys = b"\x0ffoo\r\r\x1d\r";
    let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_two_reads_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(
        accepted_numbered_line(&sushline, 1),
        accepted_numbered_line(&bash, 1),
        "first read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 1),
        Some("foo new".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&sushline, 2),
        accepted_numbered_line(&bash, 2),
        "second read\nbash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 2),
        Some("foo old".to_string()),
        "{bash}"
    );
}

#[test]
fn bash_readline_and_sushline_keep_incremental_and_nonincremental_search_memory_separate() {
    let inputrc = r#""\C-o": non-incremental-reverse-search-history
"\C-]": non-incremental-reverse-search-history-again"#;
    let history = ["foo old", "bar old", "foo new", "bar new"];
    let cases = [
        (
            "incremental then empty nonincremental",
            b"\x12foo\r\x0f\r\r".as_slice(),
            "foo new",
            "",
        ),
        (
            "nonincremental then empty incremental repeat",
            b"\x0ffoo\r\r\x12\x12\r".as_slice(),
            "foo new",
            "",
        ),
        (
            "incremental and nonincremental memories survive intervening search",
            b"\x12foo\n\x0fbar\r\r\x12\x12\r".as_slice(),
            "bar old",
            "foo new",
        ),
        (
            "nonincremental repeat keeps its query after incremental search",
            b"\x0ffoo\r\x12bar\r\x1d\r".as_slice(),
            "bar old",
            "foo old",
        ),
    ];

    for (case, keys, expected_first, expected_second) in cases {
        let bash =
            run_bash_readline_two_reads_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline =
            run_sushline_harness_two_reads_with_inputrc_and_history(keys, inputrc, &history);

        assert_eq!(
            accepted_numbered_line(&bash, 1),
            Some(expected_first.to_string()),
            "{case}: first GNU read\n{bash}"
        );
        assert_eq!(
            accepted_numbered_line(&bash, 2),
            Some(expected_second.to_string()),
            "{case}: second GNU read\n{bash}"
        );
        for read in [1, 2] {
            assert_eq!(
                accepted_numbered_line(&sushline, read),
                accepted_numbered_line(&bash, read),
                "{case}: read {read}\nbash={bash}\nsushline={sushline}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_nonincremental_search_again_steps_past_match() {
    let inputrc = r#""\C-o": non-incremental-reverse-search-history
"\C-]": non-incremental-reverse-search-history-again"#;
    let history = ["foo old", "bar old", "foo new", "bar new"];
    let keys = b"\x10\x0ffoo\r\x1d\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(accepted_line(&bash), Some("foo old".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_nonincremental_forward_search_again_steps_past_match() {
    let inputrc = r#""\C-o": non-incremental-forward-search-history
"\C-]": non-incremental-forward-search-history-again"#;
    let history = ["foo old", "bar old", "foo new", "bar new"];
    // C-p x4 starts on `foo old`; forward search finds `bar old`, then -again
    // must pass that match and find `bar new`.
    let keys = b"\x10\x10\x10\x10\x0fbar\r\x1d\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(accepted_line(&bash), Some("bar new".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_recall_incremental_query_on_empty_repeat() {
    // GNU recalls the previous completed incremental search when repeating
    // with an empty query on a later read; the recalled query then searches
    // inclusively from the cursor like a fresh search.
    // Forward repeat uses C-o (bound to forward-search-history): a raw
    // Ctrl-S would freeze PTY output via XOFF flow control. The forward
    // case starts mid-history so the recalled query finds a newer match.
    let inputrc = r#""\C-o": forward-search-history"#;
    let history = ["foo old", "bar old", "foo new", "bar new"];
    for (keys, second) in [
        (b"\x12foo\r\x12\x12\r".as_slice(), "foo new"),
        (b"\x12foo\r\x10\x10\x10\x0f\x0f\r".as_slice(), "foo new"),
    ] {
        let bash =
            run_bash_readline_two_reads_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline =
            run_sushline_harness_two_reads_with_inputrc_and_history(keys, inputrc, &history);

        assert_eq!(
            accepted_numbered_line(&bash, 1),
            Some("foo new".to_string()),
            "first read\nbash={bash}"
        );
        assert_eq!(
            accepted_numbered_line(&sushline, 1),
            accepted_numbered_line(&bash, 1),
            "first read\nbash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            accepted_numbered_line(&bash, 2),
            Some(second.to_string()),
            "second read\nbash={bash}"
        );
        assert_eq!(
            accepted_numbered_line(&sushline, 2),
            accepted_numbered_line(&bash, 2),
            "second read\nbash={bash}\nsushline={sushline}"
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
        ("!*", "/tmp/foo.txt alpha beta"),
        ("!!:1-2", "/tmp/foo.txt alpha"),
        ("!!:1-", "/tmp/foo.txt alpha"),
        // The designator ends where its grammar ends; the text that
        // follows it is literal, as GNU leaves it.
        ("!!:1/2", "/tmp/foo.txt/2"),
        ("!!:1.txt", "/tmp/foo.txt.txt"),
        ("!!:1,", "/tmp/foo.txt,"),
        ("!!:1]", "/tmp/foo.txt]"),
        ("!!:1=", "/tmp/foo.txt="),
        ("!!:1h", "/tmp/foo.txth"),
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
fn bash_readline_and_sushline_expand_bare_percent_last_match() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["alpha beta", "gamma delta"];

    for (typed, expected) in [
        ("!%", ""),
        ("!%:h", ""),
        ("!%:q", "''"),
        ("!?alpha? !%", "alpha beta alpha"),
        ("!?alpha? !%:h", "alpha beta alpha"),
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
fn bash_readline_and_sushline_keep_short_form_history_suffixes_literal() {
    // Bash 5.3 runtime fixture, not a claim about other GNU releases.
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["alpha beta", "gamma delta"];

    for (typed, expected) in [
        ("!%$", "$"),
        ("!%*", "*"),
        ("!%^", "^"),
        ("!%-1", "-1"),
        ("!$*", "delta*"),
        ("!$^", "delta^"),
        ("!*$", "delta$"),
        ("!*%", "delta%"),
        ("!^*$", "delta$"),
        ("!!$", "delta"),
        ("!!*", "delta"),
        ("!%:h", ""),
        ("!$:h", "delta"),
        ("!?alpha? !%*", "alpha beta alpha*"),
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
fn bash_readline_and_sushline_expand_short_form_caret_ranges() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    for (entry, cases, failures) in [
        (
            "cmd a b c d",
            vec![
                ("!^-", "a b c"),
                ("!^-1", "a"),
                ("!^-2", "a b"),
                ("!^-3", "a b c"),
                ("!^-4", "a b c d"),
                ("!^-2$", "a b$"),
                ("!^-$", "a b c d"),
                ("!^-^", "a"),
                ("!^-^^", "a^"),
                ("!^-^aa", "aaa"),
                ("!^-x", "a b cx"),
                ("!^", "a"),
                ("!^*", "a b c d"),
                ("!^*$", "a b c d$"),
            ],
            vec!["!^-0"],
        ),
        (
            "gamma delta",
            vec![("!^-", ""), ("!^-$", "delta"), ("!^-1", "delta")],
            vec!["!^-2"],
        ),
    ] {
        let history = [entry];
        for (typed, expected) in cases {
            let keys = format!("{typed}\x18\x01\r");
            let bash = run_bash_history_expand(typed, &history);
            let sushline =
                run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);
            assert_eq!(bash, expected, "{entry}: {typed}");
            assert_eq!(
                accepted_line(&sushline),
                Some(expected.to_string()),
                "{entry}: {typed}: {sushline}"
            );
        }
        // Interactive failure hooks differ; use history -p's status and error.
        for typed in failures {
            let (stdout, stderr) = run_bash_history_expand_failure(typed, &history);
            assert!(stdout.is_empty(), "{entry}: {typed}: {stdout}");
            assert!(
                stderr.contains(&format!("{typed}: history expansion failed")),
                "{entry}: {typed}: {stderr}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_expand_colon_form_caret_ranges() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    for (entry, cases, failures) in [
        (
            "cmd a b c d",
            vec![
                ("!:^-", "a b c"),
                ("!:^-1", "a"),
                ("!:^-2", "a b"),
                ("!:^-4", "a b c d"),
                ("!:^-$", "a b c d"),
                ("!:^-^", "a"),
                ("!:^-^^", "a^"),
                ("!:^-x", "a b cx"),
                ("!:^-*", "a b c*"),
                ("!:^-$x", "a b c dx"),
            ],
            vec!["!:^-5"],
        ),
        (
            "gamma delta",
            vec![("!:^-", ""), ("!:^-1", "delta"), ("!:^-$", "delta")],
            vec!["!:^-2"],
        ),
    ] {
        let history = [entry];
        for (typed, expected) in cases {
            let keys = format!("{typed}\x18\x01\r");
            let bash = run_bash_history_expand(typed, &history);
            let sushline =
                run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);
            assert_eq!(bash, expected, "{entry}: {typed}");
            assert_eq!(
                accepted_line(&sushline),
                Some(expected.to_string()),
                "{entry}: {typed}: {sushline}"
            );
        }
        for typed in failures {
            let (stdout, stderr) = run_bash_history_expand_failure(typed, &history);
            assert!(stdout.is_empty(), "{entry}: {typed}: {stdout}");
            assert!(
                stderr.contains(&format!("{typed}: history expansion failed")),
                "{entry}: {typed}: {stderr}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_select_default_policy_word_designators() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    for (entry, cases) in [
        (
            r#"echo "a b" c"#,
            vec![
                ("!!:1", r#""a b""#),
                ("!!:2", "c"),
                ("!!:$", "c"),
                ("!!:*", r#""a b" c"#),
                ("!!:1-2", r#""a b" c"#),
            ],
        ),
        (
            "echo 'a b' c",
            vec![
                ("!!:1", "'a b'"),
                ("!!:2", "c"),
                ("!!:$", "c"),
                ("!!:*", "'a b' c"),
            ],
        ),
        (
            r"echo a\ b c",
            vec![
                ("!!:1", r"a\ b"),
                ("!!:2", "c"),
                ("!!:$", "c"),
                ("!!:*", r"a\ b c"),
            ],
        ),
        (
            "echo a|b c",
            vec![
                ("!!:1", "a"),
                ("!!:2", "|"),
                ("!!:$", "c"),
                ("!!:*", "a | b c"),
                ("!!:1-2", "a |"),
            ],
        ),
        (
            r"echo a\;b c",
            vec![("!!:1", r"a\;b"), ("!!:2", "c"), ("!!:$", "c")],
        ),
    ] {
        let history = [entry];
        for (typed, expected) in cases {
            let keys = format!("{typed}\x18\x01\r");
            let bash = run_bash_history_expand(typed, &history);
            let sushline =
                run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);
            assert_eq!(bash, expected, "{entry}: {typed}");
            assert_eq!(
                accepted_line(&sushline),
                Some(expected.to_string()),
                "{entry}: {typed}: {sushline}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_expand_prefixed_paths_with_punctuation() {
    // GNU scans a `!string` prefix until a delimiter, so dots and slashes
    // belong to the event; a dash terminates it with `-` applying the
    // all-but-last-word range.
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = [
        "foo wrong",
        "foo.bar correct",
        "./tool local",
        "/usr/bin/tool absolute",
        "foo/bar slash",
    ];

    for (typed, expected) in [
        ("!foo.bar", "foo.bar correct"),
        ("!./tool", "./tool local"),
        ("!/usr/bin/tool", "/usr/bin/tool absolute"),
        ("!foo/bar", "foo/bar slash"),
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

    let keys = b"!foo-bar\x18\x01\r";
    let bash = run_bash_history_expand("!foo-bar", &["foo wrong"]);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &["foo wrong"]);

    assert_eq!(bash, "foobar", "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("foobar".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_terminate_event_prefix_on_metacharacters() {
    // GNU ends a `!string` prefix on shell metacharacters: the event
    // expands and the metacharacter with the rest stays literal. This uses
    // the real-prompt Bash path because `history -p` scans `;` into the
    // prefix while the prompt path terminates on it.
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["foo wrong"];

    for (typed, expected) in [
        ("!foo;bar", "foo wrong;bar"),
        ("!foo&bar", "foo wrong&bar"),
        ("!foo|bar", "foo wrong|bar"),
        ("!foo<in", "foo wrong<in"),
        ("!foo>out", "foo wrong>out"),
        ("!foo(bar", "foo wrong(bar"),
        ("!foo)bar", "foo wrong)bar"),
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_prompt_with_history(keys.as_bytes(), inputrc, &history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{typed}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{typed}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_scan_omitted_question_needle_to_end_of_line() {
    // Without a closing `?`, GNU scans the needle to the end of the line,
    // so a trailing space or `:p` belongs to the needle instead of staying
    // literal or applying as a modifier. The real-prompt path also covers
    // the failing expansions, which leave the line unchanged on both sides.
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["alpha beta", "gamma delta here", "/usr/local/bin/tool arg"];

    for (typed, expected) in [
        ("!?delta here", "gamma delta here"),
        ("!?delta? here", "gamma delta here here"),
        ("!?delta:p", "!?delta:p"),
        ("!?arg1 qux", "!?arg1 qux"),
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_prompt_with_history(keys.as_bytes(), inputrc, &history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{typed}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{typed}: bash={bash}\nsushline={sushline}"
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
        "!?needle middle?:%",
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
    // Bash 5.3's Readline prompt path leaves this single-quoted `!!`
    // unchanged; direct GNU History 8.3 library expansion is measured
    // separately. The unknown variable is ignored on both sides. The
    // real-prompt path is required: `read -e` under `-c` never expands.
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["echo alpha"];
    let keys = b"'!!'\x18\x01\r";
    let bash = run_bash_prompt_with_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);

    assert_eq!(accepted_line(&bash), Some("'!!'".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_expand_quoted_search_designators() {
    // The Bash prompt harness appends its own command to history, so a bare
    // `!!` would name different events in the two editors. A unique search
    // designator selects the seeded entry unambiguously on both sides.
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["echo UNIQUEMARKER alpha"];
    let expanded = "echo UNIQUEMARKER alpha";
    for (typed, expected) in [
        ("\"!?UNIQUEMARKER?\"", format!("\"{expanded}\"")),
        ("echo \"!?UNIQUEMARKER?\"", format!("echo \"{expanded}\"")),
        ("echo 'a'!?UNIQUEMARKER?", format!("echo 'a'{expanded}")),
        (
            "echo a\"!?UNIQUEMARKER?\"b",
            format!("echo a\"{expanded}\"b"),
        ),
        ("\"!?UNIQUEMARKER?\":q", format!("\"{expanded}\":q")),
        (
            "'!?UNIQUEMARKER?' !?UNIQUEMARKER?",
            format!("'!?UNIQUEMARKER?' {expanded}"),
        ),
        (
            "echo \\!?UNIQUEMARKER?",
            "echo \\!?UNIQUEMARKER?".to_string(),
        ),
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_prompt_with_history(keys.as_bytes(), inputrc, &history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected.as_str()),
            "{typed}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected.as_str()),
            "{typed}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_terminate_bare_history_designators_at_active_quote() {
    // These rows are measured with this fixture's two-entry history; the
    // search and single-quote rows are controls from the adjacent fixtures.
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["UNIQUEMARKER alpha", "BRACKETMATCH beta"];
    for (typed, expected) in [
        ("echo \"!UNIQUEMARKER\"", "echo \"UNIQUEMARKER alpha\""),
        ("echo a\"!UNIQUEMARKER\"b", "echo a\"UNIQUEMARKER alpha\"b"),
        (
            "echo [!BRACKETMATCH ] \"!UNIQUEMARKER\"",
            "echo [BRACKETMATCH beta ] \"UNIQUEMARKER alpha\"",
        ),
        ("echo !UNIQUEMARKER", "echo UNIQUEMARKER alpha"),
        ("echo \"!?UNIQUEMARKER?\"", "echo \"UNIQUEMARKER alpha\""),
        ("echo '!UNIQUEMARKER'", "echo '!UNIQUEMARKER'"),
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_prompt_with_history(keys.as_bytes(), inputrc, &history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{typed}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{typed}: {sushline}"
        );
    }
}

/// Bash's shell prompt suppresses a lone history designator after any
/// immediately preceding backslash, including an even-length run. An even run
/// becomes expandable again when another separated ordinary event drives the
/// expansion pass. These rows pin that behavior for the measured forms; the
/// pure History library keeps its own escape policy.
#[test]
fn bash_readline_and_sushline_inhibit_history_expansion_after_backslash_runs() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let setup = r#"p=RDP; IFS= read -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#;
    let history = [
        "UNIQUEMARKER alpha",
        "BRACKETMATCH beta",
        "FIND: suffix",
        setup,
    ];
    for (typed, expected) in [
        (r#"echo \!UNIQUEMARKER"#, r#"echo \!UNIQUEMARKER"#),
        (r#"echo \!!"#, r#"echo \!!"#),
        (r#"echo "\\!UNIQUEMARKER""#, r#"echo "\\!UNIQUEMARKER""#),
        (r#"echo \\!UNIQUEMARKER"#, r#"echo \\!UNIQUEMARKER"#),
        (r#"echo \\\\!UNIQUEMARKER"#, r#"echo \\\\!UNIQUEMARKER"#),
        (r#"echo \\\!UNIQUEMARKER"#, r#"echo \\\!UNIQUEMARKER"#),
        (r#"echo "\\!!""#, r#"echo "\\!!""#),
        (r#"echo a\\!b!UNIQUEMARKER"#, r#"echo a\\!b!UNIQUEMARKER"#),
        (r#"echo \\!! "!""#, r#"echo \\!! "!""#),
        (r#"echo \\!! "a!""#, r#"echo \\!! "a!""#),
        (r#"echo !:"#, r#"echo !:"#),
        (r#"echo \\!UNIQUEMARKER !:"#, r#"echo \\!UNIQUEMARKER !:"#),
        (
            r#"echo \\!UNIQUEMARKER !:0"#,
            r#"echo \\UNIQUEMARKER alpha p=RDP"#,
        ),
        (
            r#"echo \\!UNIQUEMARKER !BRACKETMATCH"#,
            r#"echo \\UNIQUEMARKER alpha BRACKETMATCH beta"#,
        ),
        (r#"echo \\!UNIQUEMARKER !"#, r#"echo \\!UNIQUEMARKER !"#),
        (r#"echo \\!UNIQUEMARKER ! "#, r#"echo \\!UNIQUEMARKER ! "#),
        (r#"echo \\!! !!"#, r#"echo \\{setup} {setup}"#),
        (r#"echo !!:"#, r#"echo !!:"#),
        (r#"echo !UNIQUEMARKER:"#, r#"echo !UNIQUEMARKER:"#),
        (r#"echo !$:"#, r#"echo !$:"#),
        (r#"echo !$:0"#, r#"echo !$:0"#),
        (r#"echo !$:gs/l/L/"#, r#"echo "$Line""#),
        (r#"echo !$:as/l/L/"#, r#"echo "$Line""#),
        (r#"echo !$:s/l/L/"#, r#"echo "$Line""#),
        (r#"echo !!\:"#, r#"echo {setup}\:"#),
        (r#"echo !!\:s/a/x:"#, r#"echo {setup}\:s/a/x:"#),
        (r#"echo !!\!x:"#, r#"echo {setup}\!x:"#),
        (r#"echo !!\!:0:"#, r#"echo {setup}\!:0:"#),
        (r#"echo !!:h\!:"#, r#"echo {setup}\!:"#),
        (r#"echo !!\\a:"#, r#"echo {setup}\\a:"#),
        (r#"echo !!\\:"#, r#"echo {setup}\\:"#),
        (r#"echo !!::"#, r#"echo !!::"#),
        (r#"echo !!:0"#, r#"echo p=RDP"#),
        (r#"echo !UNIQUEMARKER:0"#, r#"echo UNIQUEMARKER"#),
        (r#"echo !: !UNIQUEMARKER"#, r#"echo !: !UNIQUEMARKER"#),
        (r#"echo !: !!"#, r#"echo !: !!"#),
        (r#"echo !: !:0"#, r#"echo !: !:0"#),
        (r#"echo !: !BRACKETMATCH"#, r#"echo !: !BRACKETMATCH"#),
        (r#"echo !: !?UNIQUEMARKER?"#, r#"echo !: !?UNIQUEMARKER?"#),
        (r#"echo !: !-1"#, r#"echo !: !-1"#),
        (r#"echo !:  !UNIQUEMARKER"#, r#"echo !:  !UNIQUEMARKER"#),
        (
            r#"echo \\!UNIQUEMARKER !: !BRACKETMATCH"#,
            r#"echo \\!UNIQUEMARKER !: !BRACKETMATCH"#,
        ),
        (
            r#"echo !:0 !UNIQUEMARKER"#,
            r#"echo p=RDP UNIQUEMARKER alpha"#,
        ),
        (r#"echo !:; !UNIQUEMARKER"#, r#"echo !:; !UNIQUEMARKER"#),
        (r#"echo !?FIND: suffix?:0"#, r#"echo FIND:"#),
        (r#"echo !?FIND: suffix?:"#, r#"echo !?FIND: suffix?:"#),
        (r#"echo !?FIND: suffix?:0:"#, r#"echo !?FIND: suffix?:0:"#),
        (r#"echo !?FIND: suffix?:1:"#, r#"echo !?FIND: suffix?:1:"#),
        (r#"echo !?FIND: suffix?:^:"#, r#"echo !?FIND: suffix?:^:"#),
        (
            r#"echo !!:s/a/x:"#,
            r#"echo p=RDP; IFS= rex:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:s/a/b:"#,
            r#"echo p=RDP; IFS= reb:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:s/a/x/y:"#,
            r#"echo p=RDP; IFS= rexd -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line"y:"#,
        ),
        (
            r#"echo !!:s/a/x::"#,
            r#"echo p=RDP; IFS= rex::d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:h:s/a/x:"#,
            r#"echo p=RDP; IFS= rex:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !UNIQUEMARKER:s/a/x:"#,
            r#"echo UNIQUEMARKER x:lpha"#,
        ),
        (
            r#"echo !!:gs/a/x:"#,
            r#"echo p=RDP; IFS= rex:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:as/a/x:"#,
            r#"echo p=RDP; IFS= rex:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:s/a/x:0:"#,
            r#"echo p=RDP; IFS= rex:0:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:s/a/x: !UNIQUEMARKER"#,
            r#"echo p=RDP; IFS= rex: !UNIQUEMARKERd -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:s/a/x: !UNIQUEMARKER:"#,
            r#"echo p=RDP; IFS= rex: !UNIQUEMARKER:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:s/a/x: !:"#,
            r#"echo p=RDP; IFS= rex: !:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:gs/a/x: !BRACKETMATCH:"#,
            r#"echo p=RDP; IFS= rex: !BRACKETMATCH:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:s/a/!UNIQUEMARKER:"#,
            r#"echo p=RDP; IFS= re!UNIQUEMARKER:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo !!:s/a/x: !?FIND: suffix?:"#,
            r#"echo p=RDP; IFS= rex: !?FIND: suffix?:d -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
        (
            r#"echo foo:s/a/b !UNIQUEMARKER"#,
            r#"echo foo:s/a/b UNIQUEMARKER alpha"#,
        ),
        (r#"echo x:s/a/b !!"#, r#"echo x:s/a/b {setup}"#),
        (
            r#"echo \!:s/a/b !UNIQUEMARKER"#,
            r#"echo \!:s/a/b UNIQUEMARKER alpha"#,
        ),
        (
            r#"echo foo:s/a/b !UNIQUEMARKER !BRACKETMATCH"#,
            r#"echo foo:s/a/b UNIQUEMARKER alpha BRACKETMATCH beta"#,
        ),
        (
            r#"echo foo:s/a/b/ !UNIQUEMARKER"#,
            r#"echo foo:s/a/b/ UNIQUEMARKER alpha"#,
        ),
        (r#"echo !!:s/a/x/:"#, r#"echo !!:s/a/x/:"#),
        (
            r#"echo !!:s/a/b:c:d"#,
            r#"echo p=RDP; IFS= reb:c:dd -r -e -p "${p}>" line; m=SUSHLINE_ACCEPT; printf "${m}ED:%s\n" "$line""#,
        ),
    ] {
        let expected = expected.replace("{setup}", setup);
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_prompt_with_history(keys.as_bytes(), inputrc, &history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected.as_str()),
            "{typed}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected.as_str()),
            "{typed}: {sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_inhibit_shell_history_expansion() {
    // Bash's prompt installs a shell-specific inhibit predicate; Sushline's
    // harness installs the same narrow policy in its Hooks implementation.
    // The pure History library keeps the neutral per-call predicate.
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let history = ["UNIQUEMARKER alpha", "BRACKETMATCH beta"];
    for (typed, expected) in [
        ("$!UNIQUEMARKER", "$UNIQUEMARKER alpha"),
        ("echo $!UNIQUEMARKER", "echo $!UNIQUEMARKER"),
        ("echo x$!UNIQUEMARKER", "echo x$!UNIQUEMARKER"),
        ("echo [!UNIQUEMARKER]", "echo [!UNIQUEMARKER]"),
        ("echo [!UNIQUEMARKER ]", "echo [!UNIQUEMARKER ]"),
        ("echo a[!UNIQUEMARKER ]b", "echo a[!UNIQUEMARKER ]b"),
        ("echo ${!UNIQUEMARKER}", "echo ${!UNIQUEMARKER}"),
        ("echo ${!UNIQUEMARKER }", "echo ${!UNIQUEMARKER }"),
        ("echo [!UNIQUEMARKER", "echo [UNIQUEMARKER alpha"),
        ("echo ${!UNIQUEMARKER", "echo ${UNIQUEMARKER alpha"),
        (
            "echo [!UNIQUEMARKER]!UNIQUEMARKER",
            "echo [!UNIQUEMARKER]!UNIQUEMARKER",
        ),
        ("echo [!ab] !UNIQUEMARKER", "echo [!ab] !UNIQUEMARKER"),
        ("echo !UNIQUEMARKER [!ab]", "echo !UNIQUEMARKER [!ab]"),
        ("echo ${!ab}!UNIQUEMARKER", "echo ${!ab}!UNIQUEMARKER"),
        ("echo $!x!UNIQUEMARKER", "echo $!xUNIQUEMARKER alpha"),
        (
            "echo '[!ab]' !UNIQUEMARKER",
            "echo '[!ab]' UNIQUEMARKER alpha",
        ),
        (
            "echo \"[!ab]\" !UNIQUEMARKER",
            "echo \"[!ab]\" !UNIQUEMARKER",
        ),
        (
            "echo '${!ab}' !UNIQUEMARKER",
            "echo '${!ab}' UNIQUEMARKER alpha",
        ),
        (
            "echo !UNIQUEMARKER '[!ab]'",
            "echo UNIQUEMARKER alpha '[!ab]'",
        ),
        (
            "echo '[!ab]!' !UNIQUEMARKER",
            "echo '[!ab]!' UNIQUEMARKER alpha",
        ),
        (
            "echo [! ab] !UNIQUEMARKER",
            "echo [! ab] UNIQUEMARKER alpha",
        ),
        (
            "echo [!=ab] !UNIQUEMARKER",
            "echo [!=ab] UNIQUEMARKER alpha",
        ),
        (
            "echo [!;ab] !UNIQUEMARKER",
            "echo [!;ab] UNIQUEMARKER alpha",
        ),
        ("echo !UNIQUEMARKER [!]", "echo UNIQUEMARKER alpha [!]"),
        ("echo !UNIQUEMARKER ${!}", "echo UNIQUEMARKER alpha ${!}"),
        (
            "echo !UNIQUEMARKER [!]!UNIQUEMARKER",
            "echo UNIQUEMARKER alpha [!]UNIQUEMARKER alpha",
        ),
        ("echo [!]!UNIQUEMARKER", "echo [!]UNIQUEMARKER alpha"),
        ("echo [!] !UNIQUEMARKER", "echo [!] UNIQUEMARKER alpha"),
        (
            "echo !UNIQUEMARKER ${!}!UNIQUEMARKER",
            "echo UNIQUEMARKER alpha ${!}UNIQUEMARKER alpha",
        ),
        ("echo =#[!BRACKETMATCH ]", "echo =#[!BRACKETMATCH ]"),
        (
            "echo !UNIQUEMARKER =#[!BRACKETMATCH ]",
            "echo UNIQUEMARKER alpha =#[BRACKETMATCH beta ]",
        ),
        (
            "echo !UNIQUEMARKER # [!ab]",
            "echo UNIQUEMARKER alpha # [!ab]",
        ),
        (
            "echo !UNIQUEMARKER # ${!ab}",
            "echo UNIQUEMARKER alpha # ${!ab}",
        ),
        ("echo !UNIQUEMARKER x#[!ab]", "echo !UNIQUEMARKER x#[!ab]"),
        (
            "echo !UNIQUEMARKER #'[!ab]'",
            "echo UNIQUEMARKER alpha #'[!ab]'",
        ),
        (
            "echo !UNIQUEMARKER [!BRACKETMATCH ]",
            "echo UNIQUEMARKER alpha [BRACKETMATCH beta ]",
        ),
        (
            "echo [!BRACKETMATCH ] !UNIQUEMARKER",
            "echo [BRACKETMATCH beta ] UNIQUEMARKER alpha",
        ),
        (
            "echo !UNIQUEMARKER ${!BRACKETMATCH }",
            "echo UNIQUEMARKER alpha ${BRACKETMATCH beta }",
        ),
        ("echo [!BRACKETMATCH ]", "echo [!BRACKETMATCH ]"),
        ("echo !UNIQUEMARKER", "echo UNIQUEMARKER alpha"),
    ] {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_prompt_with_history(keys.as_bytes(), inputrc, &history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{typed}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{typed}: {sushline}"
        );
    }
}

#[test]
fn bash_history_x_modifier_preserves_separators_and_empty_fields() {
    // Runtime observations through Bash 5.3's history -p/GNU History path;
    // compare raw bytes so tabs, newlines, and CR/FF remain distinguishable.
    let cases: &[(&[&str], &str, &[u8])] = &[
        (&["echo  one   two"], "!!:x", b"'echo' '' 'one' '' '' 'two'"),
        (&["  echo  one "], "!!:x", b"'' '' 'echo' '' 'one' ''"),
        (
            &["echo\tone\t\ttwo\nlast"],
            "!!:x",
            b"'echo'\t'one'\t''\t'two'\n'last'",
        ),
        (&["echo\n"], "!!:x", b"'echo'\n''"),
        (
            &["echo\rone\x0c two\x0bone"],
            "!!:x",
            b"'echo\rone\x0c' 'two\x0bone'",
        ),
        (&["don't stop"], "!!:x", b"'don'\\''t' 'stop'"),
        (&[""], "!!:x", b"''"),
        (&["   "], "!!:x", b"'' '' '' ''"),
        (
            &["echo  one   two"],
            "^one^new^:x",
            b"'echo' '' 'new' '' '' 'two'",
        ),
    ];

    for (history, typed, expected) in cases {
        assert_eq!(
            run_bash_history_expand_bytes(typed, history),
            *expected,
            "{typed}: history={history:?}"
        );
    }
}

#[test]
fn bash_history_quote_modifiers_apply_once_after_the_modifier_chain() {
    let inputrc = r#""\C-x\C-a": history-expand-line"#;
    let word_history = ["echo one two"];
    let path_history = ["echo /tmp/a.txt"];
    let cases: &[(&str, &[&str], &str)] = &[
        ("!!:q:q", &word_history, "'echo one two'"),
        ("!!:x:x", &word_history, "'echo' 'one' 'two'"),
        ("!!:q:x", &word_history, "'echo' 'one' 'two'"),
        ("!!:x:q", &word_history, "'echo one two'"),
        ("!!:q:t", &path_history, "'a.txt'"),
        ("!!:t:q", &path_history, "'a.txt'"),
    ];
    for (typed, history, expected) in cases {
        let keys = format!("{typed}\x18\x01\r");
        let bash = run_bash_history_expand(typed, history);
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, history);

        assert_eq!(bash, *expected, "{typed}: Bash");
        assert_eq!(
            accepted_line(&sushline),
            Some(bash.clone()),
            "{typed}: Sushline"
        );
    }
}

#[test]
fn bash_history_quick_substitution_quote_modes_apply_once_after_the_chain() {
    let history_entries = ["echo one two"];
    let mut history = History::new();
    history.push(history_entries[0]);

    for (typed, expected) in [
        ("^one^new^:q:x", b"'echo' 'new' 'two'".as_slice()),
        ("^one^new^:x:q", b"'echo new two'".as_slice()),
    ] {
        let bash = run_bash_history_expand(typed, &history_entries);
        let expanded = expand_history(
            typed.as_bytes(),
            &history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap();

        assert_eq!(bash.as_bytes(), expected, "{typed}: Bash");
        assert_eq!(expanded, expected, "{typed}: History API");
    }
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
fn bash_and_sushline_insert_last_argument_alias_repeats_and_resets_like_bash() {
    let inputrc = "\"\\C-o\": insert-last-argument";
    for (desc, history, keys, expected, bells) in [
        (
            "alias",
            &["cmd one two"][..],
            b"\x0f\r".as_slice(),
            "two",
            0,
        ),
        (
            "repeat_at_oldest",
            &["cmd one two"][..],
            b"\x0f\x0f\r".as_slice(),
            "",
            1,
        ),
        (
            "self_insert_resets",
            &["cmd one two", "cmd alpha beta"][..],
            b"\x0f \x0f\r".as_slice(),
            "beta beta",
            0,
        ),
        (
            "movement_resets",
            &["cmd one two", "cmd alpha beta"][..],
            b"\x0f\x01\x0f\r".as_slice(),
            "betabeta",
            0,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some(expected.to_string()),
                "{desc}: {editor}: {output}"
            );
            assert_eq!(bell_count(output), bells, "{desc}: {editor}: {output}");
        }
    }
}

#[test]
fn bash_and_sushline_repeated_yank_keeps_word_number_with_numeric_prefix() {
    let inputrc = "\"\\C-o\": insert-last-argument";
    for (desc, history, keys, expected, bells) in [
        (
            "repeat_with_one",
            &["cmd one two", "cmd alpha beta"][..],
            b"\x1b.\x1b1\x0f\r".as_slice(),
            "two",
            0,
        ),
        (
            "repeat_with_zero",
            &["cmd one two", "cmd alpha beta"][..],
            b"\x1b.\x1b0\x0f\r".as_slice(),
            "two",
            0,
        ),
        (
            "repeat_with_five",
            &["cmd one two", "cmd alpha beta"][..],
            b"\x1b.\x1b5\x0f\r".as_slice(),
            "two",
            0,
        ),
        (
            "repeat_with_three_entries",
            &["cmd one two", "cmd alpha beta", "cmd gamma delta"][..],
            b"\x1b.\x1b1\x0f\r".as_slice(),
            "beta",
            0,
        ),
        (
            "repeat_after_numbered_first_yank",
            &["cmd one two", "cmd alpha beta"][..],
            b"\x1b1\x0f\x1b2\x0f\r".as_slice(),
            "one",
            0,
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output),
                Some(expected.to_string()),
                "{desc}: {editor}: {output}"
            );
            assert_eq!(bell_count(output), bells, "{desc}: {editor}: {output}");
        }
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
    require_oracle_host();
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

/// Vi command-mode recall puts the cursor on the first byte, so an immediate
/// `x` deletes it. Emacs recall and `history-preserve-point` keep their own
/// placement rules; neither is changed by this case.
#[test]
fn bash_readline_and_sushline_start_edits_at_the_first_byte_of_vi_recall() {
    let history = ["one", "two", "three", "four", "five"];
    for (inputrc, keys, expected) in [
        ("set editing-mode vi\n", b"\x1bkxAX\r".as_slice(), "iveX"),
        ("set editing-mode vi\n", b"\x1b2GxAX\r".as_slice(), "woX"),
        (
            "set editing-mode vi\n",
            b"\x1b\x1b[AxAX\r".as_slice(),
            "iveX",
        ),
        (
            "set editing-mode vi\nset history-preserve-point on\n",
            b"\x1bkxAX\r".as_slice(),
            "iveX",
        ),
        (
            "set editing-mode vi\nset history-preserve-point on\n",
            b"abc\x1b0lkxAX\r".as_slice(),
            "iveX",
        ),
        ("", b"\x10AX\r".as_slice(), "fiveAX"),
    ] {
        for (name, output) in [
            (
                "Bash",
                run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history),
            ),
        ] {
            let context = format!("{name} inputrc={inputrc:?} keys={keys:?}: {output}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(&output), 0, "{context}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_isearch_terminators_terminate_without_execute() {
    let history = ["alpha one", "beta", "alpha two"];
    // Default C-J (LF) terminates without executing; the match becomes the
    // line, the terminator inserts nothing, and a trailing edit applies
    // before RET (CR) accepts. RET stays distinct from C-J on the live PTY
    // via unconditional ICRNL/INLCR clearing.
    let keys = b"\x12alpha\n!\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, "", &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);
    assert_eq!(
        accepted_line(&bash),
        Some("!alpha two".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // Custom isearch-terminators terminate without executing either.
    let inputrc = "set isearch-terminators z";
    let keys = b"\x12alpz!\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
    assert_eq!(
        accepted_line(&bash),
        Some("!alpha two".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // An empty query restores the original line and point.
    let keys = b"draft\x12\n!\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, "", &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);
    assert_eq!(accepted_line(&bash), Some("draft!".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // A non-empty query with no match leaves point at 0.
    let keys = b"draft\x12zzz\n!\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, "", &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);
    assert_eq!(accepted_line(&bash), Some("!draft".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // A case-insensitive match leaves point at the match start.
    let inputrc = "set search-ignore-case on";
    let keys = b"\x12TWO\n!\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
    assert_eq!(
        accepted_line(&bash),
        Some("alpha !two".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_and_sushline_pin_quoted_terminator_in_incremental_search() {
    let history = ["alpzone one", "alptwo"];
    let inputrc = "set isearch-terminators z";
    // Although `z` is a terminator, C-v inserts it into the query;
    // the scoped display fragment shows the quoted byte reached the query.
    let keys = b"\x12alp\x16z\r";
    let query_fragment = "(reverse-i-search)`alpz'";
    for (editor, output) in [
        (
            "bash",
            run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history),
        ),
        (
            "sushline",
            run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history),
        ),
    ] {
        let context =
            format!("{editor}: inputrc={inputrc:?} keys={keys:?} history={history:?}: {output}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("alpzone one"),
            "unexpected accepted line: {context}"
        );
        assert_eq!(bell_count(&output), 0, "unexpected bell: {context}");
        // Readline redraws the query with inline cursor motion, so the
        // fragment is visible in the rendered screen, not contiguous bytes.
        let screen = super::screen::Screen::new(24, 80);
        let search_rows = screen.last_rows_starting_with(output.as_bytes(), "(");
        assert!(
            search_rows
                .as_ref()
                .is_some_and(|rows| rows.iter().any(|row| row.contains(query_fragment))),
            "query display lacks {query_fragment:?}: rows={search_rows:?}: {context}"
        );
    }
}

#[test]
fn bash_and_sushline_pin_quoted_control_in_nonincremental_search() {
    let inputrc = r#""\C-o": non-incremental-reverse-search-history"#;
    let history = ["alp\u{16}zone one", "alptwo"];
    // Ctrl-V quotes the next Ctrl-V byte into the query; the first Return
    // completes the search and the second accepts the matched entry.
    let keys = b"\x0falp\x16\x16\r\r";
    for (editor, output) in [
        (
            "bash",
            run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history),
        ),
        (
            "sushline",
            run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history),
        ),
    ] {
        let context =
            format!("{editor}: inputrc={inputrc:?} keys={keys:?} history={history:?}: {output}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("alp\u{16}zone one"),
            "unexpected accepted line: {context}"
        );
        assert_eq!(bell_count(&output), 0, "unexpected bell: {context}");
        // Bash renders the quoted byte as ^V while Sushline emits the raw
        // control byte; display rendering is outside this fixture's scope.
    }
}

#[test]
fn bash_readline_and_sushline_treat_return_as_custom_isearch_terminator() {
    if !super::host_readline::host_carries(super::host_readline::MEASURED_RELEASE) {
        eprintln!("Custom isearch terminator behavior was measured on Readline 8.3");
        return;
    }
    let history = ["alpha one", "beta", "alpha two"];
    let inputrc = r#"set isearch-terminators "\r\033""#;
    for (keys, expected, bells) in [
        (b"\x12alpha\r!\r".as_slice(), "!alpha two", 0),
        (b"\x12alpha\r\r".as_slice(), "alpha two", 0),
    ] {
        for (name, output) in [
            (
                "Bash",
                run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history),
            ),
            (
                "Sushline",
                run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history),
            ),
        ] {
            let context = format!("{name} keys={keys:?}: {output}");
            assert_eq!(
                accepted_line(&output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(&output), bells, "{context}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_forward_isearch_starts_from_cursor() {
    let history = ["alpha one", "beta", "alpha two"];
    // Backward from the middle searches before the cursor: Up Up lands on
    // `beta`, so reverse `alpha` finds `alpha one`.
    let keys = b"\x10\x10\x12alpha\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, "", &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);
    assert_eq!(
        accepted_line(&bash),
        Some("alpha one".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // Forward uses a remapped C-o so the PTY does not swallow C-s as XOFF.
    let bindings = r#""\C-o": forward-search-history"#;
    // Forward from the end has nothing ahead and keeps the empty line.
    let keys = b"\x0falpha\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, bindings, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, bindings, &history);
    assert_eq!(accepted_line(&bash), Some("".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // Forward from the middle searches ahead inclusively: Up Up lands on
    // `beta`, so `alpha` finds `alpha two`.
    let keys = b"\x10\x10\x0falpha\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, bindings, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, bindings, &history);
    assert_eq!(
        accepted_line(&bash),
        Some("alpha two".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // Forward match leaves point at the match start.
    let keys = b"\x10\x10\x0ftwo\n!\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, bindings, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, bindings, &history);
    assert_eq!(
        accepted_line(&bash),
        Some("alpha !two".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );

    // Forward with no match from the end keeps the original point.
    let keys = b"draft\x0fzzz\n!\r";
    let bash = run_bash_readline_with_bindings_and_history(keys, bindings, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, bindings, &history);
    assert_eq!(accepted_line(&bash), Some("draft!".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn failed_history_navigation_search_rings_bell_like_bash() {
    if !c_utf8_locale_available() {
        return;
    }

    // Failed prefix/substring navigation rings once in the tested Bash 5.3
    // PTY fixtures; this is runtime observation, not GNU source proof.
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for command in [
        "history-search-backward",
        "history-substring-search-backward",
    ] {
        let inputrc = format!("\"\\C-o\": {command}");
        let history = ["ALPHA old"];
        let mut keys = b"alpha".to_vec();
        keys.extend_from_slice(b"\x0f\r");
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some("alpha".to_string()),
            "{command}: {bash}"
        );
        assert_eq!(bell_count(&bash), 1, "{command}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{command}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 1, "{command}: {sushline}");
    }

    for (binding, command, key) in [
        ("\"\\C-^\"", "history-search-forward", 0x1eu8),
        ("\"\\C-o\"", "history-substring-search-forward", 0x0fu8),
    ] {
        let inputrc = format!("{binding}: {command}");
        let history = ["alpha old", "ALPHA new"];
        let mut keys = b"\x10\x10\x01\x0balpha".to_vec();
        keys.push(key);
        keys.push(b'\r');
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some("alpha".to_string()),
            "{command}: {bash}"
        );
        assert_eq!(bell_count(&bash), 1, "{command}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{command}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 1, "{command}: {sushline}");
    }
}

#[test]
fn empty_history_substring_search_matches_like_bash() {
    // Empty substring matches every entry like an empty prefix in the tested
    // Bash 5.3 PTY fixtures: from the end backward finds the newest, from the
    // oldest forward finds the newer; at a dead end it rings once and
    // preserves the empty line. Runtime observation, not GNU source proof.
    for (desc, inputrc, keys, history, expected, bells) in [
        (
            "back_from_end",
            "\"\\C-o\": history-substring-search-backward",
            b"\x0f\r".as_slice(),
            &["alpha one", "beta"][..],
            "beta",
            0,
        ),
        (
            "back_from_oldest",
            "\"\\C-o\": history-substring-search-backward",
            b"\x10\x10\x01\x0b\x0f\r".as_slice(),
            &["alpha one", "beta"][..],
            "",
            1,
        ),
        (
            "fwd_from_oldest",
            "\"\\C-o\": history-substring-search-forward",
            b"\x10\x10\x01\x0b\x0f\r".as_slice(),
            &["alpha one", "beta"][..],
            "beta",
            0,
        ),
        (
            "fwd_from_end",
            "\"\\C-o\": history-substring-search-forward",
            b"\x0f\r".as_slice(),
            &["alpha one", "beta"][..],
            "",
            1,
        ),
    ] {
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
fn successful_history_navigation_search_stays_silent_like_bash() {
    if !c_utf8_locale_available() {
        return;
    }

    // Successful prefix/substring navigation stays silent in the tested Bash
    // 5.3 PTY fixtures; this guards the failure bell without over-ringing.
    // Runtime observation, not GNU source proof.
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for command in [
        "history-search-backward",
        "history-substring-search-backward",
    ] {
        let inputrc = format!("set search-ignore-case on\n\"\\C-o\": {command}");
        let history = ["ALPHA old"];
        let mut keys = b"alpha".to_vec();
        keys.extend_from_slice(b"\x0f\r");
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some("ALPHA old".to_string()),
            "{command}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "{command}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{command}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "{command}: {sushline}");
    }

    for (binding, command, key) in [
        ("\"\\C-^\"", "history-search-forward", 0x1eu8),
        ("\"\\C-o\"", "history-substring-search-forward", 0x0fu8),
    ] {
        let inputrc = format!("set search-ignore-case on\n{binding}: {command}");
        let history = ["alpha old", "ALPHA new"];
        let mut keys = b"\x10\x10\x01\x0balpha".to_vec();
        keys.push(key);
        keys.push(b'\r');
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some("ALPHA new".to_string()),
            "{command}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "{command}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{command}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "{command}: {sushline}");
    }
}

#[test]
fn history_navigation_search_uses_text_before_cursor_like_bash() {
    if !c_utf8_locale_available() {
        return;
    }

    // With point after "alpha" in "alphax", navigation uses the
    // before-cursor "alpha" in the tested Bash 5.3 PTY fixtures with
    // ignore-case on. Runtime observation, not GNU source proof.
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for command in [
        "history-search-backward",
        "history-substring-search-backward",
    ] {
        let inputrc = format!("set search-ignore-case on\n\"\\C-o\": {command}");
        let history = ["ALPHA old"];
        let keys = b"alphax\x02\x0f\r";
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            keys,
            &inputrc,
            &history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            keys,
            &inputrc,
            &history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some("ALPHA old".to_string()),
            "{command}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "{command}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{command}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "{command}: {sushline}");
    }

    for (binding, command, key) in [
        ("\"\\C-^\"", "history-search-forward", 0x1eu8),
        ("\"\\C-o\"", "history-substring-search-forward", 0x0fu8),
    ] {
        let inputrc = format!("set search-ignore-case on\n{binding}: {command}");
        let history = ["alpha old", "ALPHA new"];
        let mut keys = b"\x10\x10\x01\x0balphax\x02".to_vec();
        keys.push(key);
        keys.push(b'\r');
        let bash = run_bash_readline_with_inputrc_file_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        let sushline = run_sushline_harness_with_inputrc_and_history_and_env(
            &keys,
            &inputrc,
            &history,
            &locale_env,
        );
        assert_eq!(
            accepted_line(&bash),
            Some("ALPHA new".to_string()),
            "{command}: {bash}"
        );
        assert_eq!(bell_count(&bash), 0, "{command}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{command}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), 0, "{command}: {sushline}");
    }
}

#[test]
fn history_navigation_search_honors_numeric_argument_like_bash() {
    // A numeric argument repeats prefix/substring navigation in the tested
    // Bash 5.3 PTY fixtures. Runtime observation, not GNU source proof.
    for (desc, inputrc, keys, history, expected) in [
        (
            "back_prefix",
            "\"\\C-o\": history-search-backward",
            b"alpha\x1b2\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha two",
        ),
        (
            "back_substring",
            "\"\\C-o\": history-substring-search-backward",
            b"alpha\x1b2\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha two",
        ),
        (
            "fwd_prefix",
            "\"\\C-^\": history-search-forward",
            b"\x10\x10\x10\x01\x0balpha\x1b2\x1e\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha three",
        ),
        (
            "fwd_substring",
            "\"\\C-o\": history-substring-search-forward",
            b"\x10\x10\x10\x01\x0balpha\x1b2\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha three",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
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
        assert_eq!(bell_count(&sushline), 0, "{desc}: {sushline}");
    }
}

#[test]
fn history_navigation_search_excessive_count_stays_silent_like_bash() {
    // An excessive count stops at the oldest/newest match without ringing in
    // the tested Bash 5.3 PTY fixtures; only a total miss rings.
    // Runtime observation, not GNU source proof.
    for (desc, inputrc, keys, history, expected) in [
        (
            "back_excess",
            "\"\\C-o\": history-search-backward",
            b"alpha\x1b5\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha one",
        ),
        (
            "back_partial",
            "\"\\C-o\": history-search-backward",
            b"alpha\x1b2\x0f\r".as_slice(),
            &["alpha one", "beta"][..],
            "alpha one",
        ),
        (
            "fwd_excess",
            "\"\\C-^\": history-search-forward",
            b"\x10\x10\x10\x01\x0balpha\x1b5\x1e\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha three",
        ),
        (
            "back_substring_excess",
            "\"\\C-o\": history-substring-search-backward",
            b"alpha\x1b5\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha one",
        ),
        (
            "fwd_substring_excess",
            "\"\\C-o\": history-substring-search-forward",
            b"\x10\x10\x10\x01\x0balpha\x1b5\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha three",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
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
        assert_eq!(bell_count(&sushline), 0, "{desc}: {sushline}");
    }
}

#[test]
fn history_navigation_search_negative_count_reverses_direction_like_bash() {
    // A negative count runs the opposite direction's search in the tested
    // Bash 5.3 PTY fixtures. Runtime observation, not GNU source proof.
    for (desc, inputrc, keys, history, expected, bells) in [
        (
            "back_prefix_neg1_from_oldest_finds_newer",
            "\"\\C-o\": history-search-backward",
            b"\x10\x10\x10\x01\x0balpha\x1b-1\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha two",
            0,
        ),
        (
            "back_prefix_neg2_from_end_bells",
            "\"\\C-o\": history-search-backward",
            b"alpha\x1b-2\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha",
            1,
        ),
        (
            "back_substring_neg1_from_oldest_finds_newer",
            "\"\\C-o\": history-substring-search-backward",
            b"\x10\x10\x10\x01\x0balpha\x1b-1\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha two",
            0,
        ),
        (
            "back_substring_neg2_from_end_bells",
            "\"\\C-o\": history-substring-search-backward",
            b"alpha\x1b-2\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha",
            1,
        ),
        (
            "fwd_prefix_neg1_from_end_finds_newer",
            "\"\\C-^\": history-search-forward",
            b"alpha\x1b-1\x1e\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha three",
            0,
        ),
        (
            "fwd_substring_neg1_from_end_finds_newer",
            "\"\\C-o\": history-substring-search-forward",
            b"alpha\x1b-1\x0f\r".as_slice(),
            &["alpha one", "alpha two", "alpha three"][..],
            "alpha three",
            0,
        ),
    ] {
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
fn history_navigation_search_zero_count_is_silent_noop_like_bash() {
    // A zero count leaves the line unchanged without ringing in the tested
    // Bash 5.3 PTY fixtures. Runtime observation, not GNU source proof.
    for (desc, inputrc, keys, history) in [
        (
            "back_prefix",
            "\"\\C-o\": history-search-backward",
            b"alpha\x1b0\x0f\r".as_slice(),
            &["alpha one", "beta"][..],
        ),
        (
            "back_substring",
            "\"\\C-o\": history-substring-search-backward",
            b"alpha\x1b0\x0f\r".as_slice(),
            &["alpha one", "beta"][..],
        ),
        (
            "fwd_prefix",
            "\"\\C-^\": history-search-forward",
            b"\x10\x10\x01\x0balpha\x1b0\x1e\r".as_slice(),
            &["alpha one", "beta"][..],
        ),
        (
            "fwd_substring",
            "\"\\C-o\": history-substring-search-forward",
            b"\x10\x10\x01\x0balpha\x1b0\x0f\r".as_slice(),
            &["alpha one", "beta"][..],
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
        assert_eq!(
            accepted_line(&bash),
            Some("alpha".to_string()),
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
fn fetch_history_out_of_range_fetches_first_entry_like_bash() {
    // An out-of-range fetch-history number (including 0) fetches the first
    // entry silently in the tested Bash 5.3 PTY fixtures; an empty history
    // stays put. Runtime observation, not GNU source proof.
    for (desc, inputrc, keys, history, expected) in [
        (
            "arg0",
            "\"\\C-o\": fetch-history",
            b"\x1b0\x0f\r".as_slice(),
            &["one", "two", "three"][..],
            "one",
        ),
        (
            "arg4",
            "\"\\C-o\": fetch-history",
            b"\x1b4\x0f\r".as_slice(),
            &["one", "two", "three"][..],
            "one",
        ),
        (
            "arg6",
            "\"\\C-o\": fetch-history",
            b"\x1b6\x0f\r".as_slice(),
            &["one", "two", "three"][..],
            "one",
        ),
        (
            "empty",
            "\"\\C-o\": fetch-history",
            b"\x1b1\x0f\r".as_slice(),
            &[][..],
            "",
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
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
        assert_eq!(bell_count(&sushline), 0, "{desc}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_yank_last_arg_empty_rings_like_bash() {
    // yank-last-arg with nothing to yank rings once in the tested Bash 5.3
    // PTY fixtures: empty history or a wordless newest entry. A successful
    // yank stays silent. Runtime observation, not GNU source proof.
    for (desc, history, expected, bells) in [
        ("empty", &[][..], "", 1),
        ("wordless", &["   "][..], "", 1),
        ("yanks", &["cmd one two"][..], "two", 0),
    ] {
        let keys = b"\x1b.\r";
        let bash = run_bash_readline_with_bindings_and_history(keys, "", history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", history);
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
fn bash_readline_and_sushline_yank_nth_arg_out_of_range_rings_like_bash() {
    // yank-nth-arg with no such word rings once in the tested Bash 5.3 PTY
    // fixtures instead of clamping to the last word; an in-range number or
    // 0 (the command word itself) stays silent. Runtime observation, not GNU
    // source proof.
    let inputrc = "\"\\C-o\": yank-nth-arg";
    let history = ["cmd one two"];
    for (desc, keys, expected, bells) in [
        ("out_of_range", b"\x1b5\x0f\r".as_slice(), "", 1),
        ("in_range", b"\x1b2\x0f\r".as_slice(), "two", 0),
        ("zero_is_command", b"\x1b0\x0f\r".as_slice(), "cmd", 0),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
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
fn bash_readline_and_sushline_negative_yank_word_number_rings_like_bash() {
    // An explicitly negative word number rings instead of yanking in the
    // tested Bash 5.3 PTY fixtures; only the default (no argument) means
    // the last word. An in-range number still yanks silently. Runtime
    // observation, not GNU source proof.
    let cases = &[
        (
            "nth_neg1",
            "\"\\C-o\": yank-nth-arg",
            b"\x1b-\x1b1\x0f\r".as_slice(),
            &["cmd one two"][..],
            "",
            1,
        ),
        (
            "nth_neg2",
            "\"\\C-o\": yank-nth-arg",
            b"\x1b-\x1b2\x0f\r".as_slice(),
            &["cmd one two"][..],
            "",
            1,
        ),
        (
            "nth_in_range",
            "\"\\C-o\": yank-nth-arg",
            b"\x1b2\x0f\r".as_slice(),
            &["cmd one two"][..],
            "two",
            0,
        ),
        (
            "last_neg",
            "",
            b"\x1b-\x1b2\x1b.\r".as_slice(),
            &["cmd one two", "cmd alpha beta"][..],
            "",
            1,
        ),
        (
            // vi-yank-arg shares the helper: exercise it in emacs mode to
            // avoid vi initial-mode flake in this harness.
            "vi_neg",
            "\"\\C-o\": vi-yank-arg",
            b"\x1b-\x1b2\x0f\r".as_slice(),
            &["cmd one two", "cmd alpha beta"][..],
            "",
            1,
        ),
    ];
    for (desc, inputrc, keys, history, expected, bells) in cases {
        let keys: &[u8] = keys;
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, history);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{desc}: {bash}"
        );
        assert_eq!(bell_count(&bash), *bells, "{desc}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{desc}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&sushline), *bells, "{desc}: {sushline}");
    }
}

#[test]
fn bash_host_reports_a_failing_expansion_on_standard_error() {
    // The Bash host's `history-expand-line` command is the interaction the
    // editor mirrors: it leaves the line unchanged and reports the failure
    // on standard error, the sink the built-in terminal uses. The host's
    // command is only bound in an interactive shell, so the case drives one
    // and keeps its standard error in a file.
    let dir = tempfile::tempdir().expect("tempdir");
    let inputrc = dir.path().join("inputrc");
    fs::write(
        &inputrc,
        "set enable-bracketed-paste off\n\"\\C-o\": history-expand-line\n",
    )
    .expect("write inputrc");
    let sink = dir.path().join("sink");
    let sink = run_bash_interactive_with_sink(
        &[b"!missing".to_vec(), vec![0x0f]],
        &inputrc,
        &sink,
        "!missing: event not found",
    );
    assert!(
        sink.contains("!missing: event not found"),
        "bash sink: {sink}"
    );
    // The command left the line unchanged, so the host redraws it after the
    // failure. The shell's own expansion runs when the line is accepted and
    // consumes the line instead, so the prompt that follows the failure
    // would be empty. The redrawn line is therefore what tells the host's
    // command apart from the shell's own expansion, and the case fails when
    // the binding is dropped.
    let after_failure = sink
        .split_once("!missing: event not found")
        .expect("the failure text must be on the sink")
        .1;
    let redrawn = after_failure.trim_start_matches(['\r', '\n']);
    assert!(
        redrawn.trim_end().ends_with("!missing"),
        "the failed expansion must leave the line unchanged: bash sink: {sink}"
    );
}

#[test]
fn bash_reports_a_failing_history_expansion_on_standard_error() {
    // The shell's own history expansion reports the failure on standard
    // error and writes nothing to standard output. The Bash host's
    // `history-expand-line` command, the interaction the editor mirrors,
    // leaves the line unchanged and reports the failure on the same stream.
    let (stdout, stderr) = run_bash_history_expand_failure("!missing", &["echo one"]);
    assert!(stdout.is_empty(), "bash stdout: {stdout}");
    assert!(
        stderr.contains("!missing: history expansion failed"),
        "bash stderr: {stderr}"
    );
}

#[test]
fn history_search_preserves_origin_edit_after_overlay_navigation_like_bash() {
    let inputrc = "set revert-all-at-newline off\n\"\\C-o\": non-incremental-forward-search-history\n\"\\C-]\": forward-search-history";
    let history = ["AAA", "BBB"];
    for (case, search_keys) in [
        ("incremental", &b"\x1dBBB\n\x10"[..]),
        ("nonincremental", &b"\x0fBBB\r\x1b<"[..]),
        ("incremental abort", &b"\x1dBBB\x07\x0e\x10"[..]),
        ("nonincremental abort", &b"\x0fBBB\x07\x0e\x10"[..]),
        ("failed incremental command", &b"\x1dBBBZ\x06\x1b<"[..]),
    ] {
        let keys = [b"\x10\x10\x05 EDIT".as_slice(), search_keys, b"\r"].concat();
        let bash = run_bash_readline_with_inputrc_file_and_history(&keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(&keys, inputrc, &history);
        assert_eq!(
            accepted_line(&bash),
            Some("AAA EDIT".to_string()),
            "{case}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{case}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_beginning_of_history_selects_the_oldest_entry() {
    let history = ["AAA", "BBB"];
    for (case, keys, expected) in [
        ("from current line", &b"\x1b<\r"[..], "AAA"),
        ("after one Ctrl-P", &b"\x10\x1b<\r"[..], "AAA"),
        ("after two Ctrl-P", &b"\x10\x10\x1b<\r"[..], "AAA"),
        ("edited oldest entry", &b"\x1b< EDIT\r"[..], "AAA EDIT"),
        ("edit before beginning", &b"\x10 EDIT\x1b<\r"[..], "AAA"),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, "", &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{case}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{case}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_end_of_history_returns_to_the_empty_current_line() {
    let inputrc = "";
    let history = ["AAA", "BBB"];
    for (case, keys) in [
        ("two previous entries", &b"\x10\x10\x1b>\r"[..]),
        ("one previous entry", &b"\x10\x1b>\r"[..]),
        ("edited previous entry", &b"\x10 EDIT\x1b>\r"[..]),
        ("beginning then end", &b"\x10\x10\x1b<\x1b>\r"[..]),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        assert_eq!(accepted_line(&bash), Some(String::new()), "{case}: {bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{case}: bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn incremental_history_search_keeps_the_edit_when_matching_its_own_entry_like_bash() {
    let inputrc = "set revert-all-at-newline off";
    for terminator in *b"\n\x06\r" {
        let keys = [b"\x10\x05 EDIT\x12A".as_slice(), &[terminator], b"\r"].concat();
        let bash = run_bash_readline_with_inputrc_file_and_history(&keys, inputrc, &["AAA"]);
        let sushline = run_sushline_harness_with_inputrc_and_history(&keys, inputrc, &["AAA"]);
        assert_eq!(accepted_line(&bash), Some("AAA EDIT".to_string()), "{bash}");
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "bash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_magic_space_expands_on_space() {
    // The shared `read -e -c` harness does not engage Bash's history
    // expansion, so this fixture drives a real interactive shell. It covers
    // a bound Space that inserts a literal space, expands common event and
    // word forms, and preserves text typed after the expansion. Failing
    // expansions and quoted designators are outside this fixture.
    let inputrc = r#"" ": magic-space"#;
    let history = ["echo alpha", "beta gamma"];
    for (typed, expected) in [
        ("abc ", "abc "),
        ("!! ", "beta gamma "),
        ("!!:1 ", "gamma "),
        ("echo !$ ", "echo gamma "),
        ("!ec ", "echo alpha "),
        ("!! more", "beta gamma more"),
        ("!!  ", "beta gamma  "),
        ("! ", "! "),
        ("\\!! ", "\\!! "),
        ("!?beta? ", "beta gamma "),
        ("!-1 ", "beta gamma "),
        ("!* ", "gamma "),
    ] {
        let bash = run_bash_interactive_magic_space(typed.as_bytes(), inputrc, &history);
        let keys = format!("{typed}\r");
        let sushline =
            run_sushline_harness_with_inputrc_and_history(keys.as_bytes(), inputrc, &history);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "{typed}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "{typed}: bash={bash}\nsushline={sushline}"
        );
        assert_eq!(bell_count(&bash), 0, "{typed}: {bash}");
        assert_eq!(bell_count(&sushline), 0, "{typed}: {sushline}");
    }
}

#[test]
fn bash_readline_and_sushline_history_search_abort_keeps_the_selected_line() {
    let backward = r#""\C-]": history-search-backward"#;
    let history = ["alpha one", "beta", "alpha two"];
    for (keys, expected) in [
        (&b"alp\x1d\x07\r"[..], "alpha two"),
        (&b"missing\x1d\x07\r"[..], "missing"),
        (&b"alp\x1d\x1d\x07\r"[..], "alpha one"),
        (&b"\x1d\x07\r"[..], "alpha two"),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, backward, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, backward, &history);
        let context =
            format!("keys={keys:?} history={history:?}\nbash={bash}\nsushline={sushline}");
        assert_eq!(accepted_line(&bash).as_deref(), Some(expected), "{context}");
        assert_eq!(accepted_line(&sushline), accepted_line(&bash), "{context}");
    }
}

#[test]
fn bash_readline_and_sushline_numeric_prefix_search_point_and_mark() {
    let history = ["alpha aa", "alpha bb", "alpha cc"];
    for (desc, inputrc, keys, expected_prefix, expected_mark) in [
        (
            "back2",
            "\"\\C-o\": history-search-backward",
            &b"alpha\x1b2\x0f"[..],
            "alphaX bb",
            "alpha bbX",
        ),
        (
            "back3",
            "\"\\C-o\": history-search-backward",
            &b"alpha\x1b3\x0f"[..],
            "alphaX aa",
            "alpha aaX",
        ),
        (
            "back4",
            "\"\\C-o\": history-search-backward",
            &b"alpha\x1b4\x0f"[..],
            "alphaX aa",
            "alpha aaX",
        ),
        (
            "backneg1",
            "\"\\C-o\": history-search-backward",
            &b"\x10\x10\x10\x01\x0balpha\x1b-1\x0f"[..],
            "alphaX bb",
            "alpha bbX",
        ),
        (
            "fwd2",
            "\"\\C-^\": history-search-forward",
            &b"\x10\x10\x10\x01\x0balpha\x1b2\x1e"[..],
            "alphaX cc",
            "alpha ccX",
        ),
        (
            "fwd3",
            "\"\\C-^\": history-search-forward",
            &b"\x10\x10\x10\x01\x0balpha\x1b3\x1e"[..],
            "alphaX cc",
            "alpha ccX",
        ),
        (
            "fwd4",
            "\"\\C-^\": history-search-forward",
            &b"\x10\x10\x10\x01\x0balpha\x1b4\x1e"[..],
            "alphaX cc",
            "alpha ccX",
        ),
        (
            "fwdneg1",
            "\"\\C-^\": history-search-forward",
            &b"alpha\x1b-1\x1e"[..],
            "alphaX cc",
            "alpha ccX",
        ),
    ] {
        for (finish, expected) in [
            (&b"X\r"[..], expected_prefix),
            (&b"\x18\x18X\r"[..], expected_mark),
        ] {
            let keys = [keys, finish].concat();
            let bash = run_bash_readline_with_bindings_and_history(&keys, inputrc, &history);
            let sushline = run_sushline_harness_with_inputrc_and_history(&keys, inputrc, &history);
            for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                let context = format!("{editor} {desc} finish={finish:?} keys={keys:?}\n{output}");
                assert_eq!(
                    accepted_line(output).as_deref(),
                    Some(expected),
                    "{context}"
                );
                assert_eq!(bell_count(output), 0, "{context}");
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_preserve_point_substring_search_point_and_mark() {
    let history = ["xxneedle old", "xxneedle mid", "xxneedle new"];
    for (setting, binding) in [("off", ""), ("on", "set history-preserve-point on\n")] {
        for (direction, inputrc, key, prefix, first, second) in [
            (
                "backward",
                format!("{binding}\"\\C-]\": history-substring-search-backward"),
                0x1du8,
                &b""[..],
                "xxXneedle new",
                "xxXneedle mid",
            ),
            (
                "forward",
                format!("{binding}\"\\C-^\": history-substring-search-forward"),
                0x1eu8,
                &b"\x10\x10\x10\x01\x0b"[..],
                "xxXneedle mid",
                "xxXneedle new",
            ),
        ] {
            for (count, arg, expected) in [("one", "", first), ("two", "\x1b2", second)] {
                for (finish, line_end) in [(&b"X\r"[..], false), (&b"\x18\x18X\r"[..], true)] {
                    let keys = [
                        prefix,
                        b"needle".as_slice(),
                        arg.as_bytes(),
                        &[key][..],
                        finish,
                    ]
                    .concat();
                    let bash =
                        run_bash_readline_with_inputrc_file_and_history(&keys, &inputrc, &history);
                    let sushline =
                        run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history);
                    let expected = if line_end {
                        expected.replacen("xxX", "xx", 1) + "X"
                    } else {
                        expected.to_string()
                    };
                    for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                        let context = format!(
                            "{editor} setting={setting} direction={direction} count={count} finish={finish:?} keys={keys:?}\n{output}"
                        );
                        assert_eq!(
                            accepted_line(output).as_deref(),
                            Some(expected.as_str()),
                            "{context}"
                        );
                        assert_eq!(bell_count(output), 0, "{context}");
                    }
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_preserve_point_numeric_history_navigation() {
    let history = ["alpha one", "alpha two", "alpha three"];
    for (setting, inputrc) in [("off", ""), ("on", "set history-preserve-point on\n")] {
        for (desc, keys, expected, bells) in [
            (
                "back2",
                &b"xxxx\x1b[D\x1b[D\x1b2\x10X\r"[..],
                if setting == "on" {
                    "alXpha two"
                } else {
                    "alpha twoX"
                },
                0,
            ),
            (
                "fwd2",
                &b"\x10\x10\x10\x01\x0bxxxx\x1b[D\x1b[D\x1b2\x0eX\r"[..],
                if setting == "on" {
                    "alXpha three"
                } else {
                    "alpha threeX"
                },
                0,
            ),
            (
                "savededit",
                &b"alpha\x10\x05 EDIT\x01\x1b[D\x1b[D\x1b[D\x1b2\x10\x1b2\x0eX\r"[..],
                if setting == "on" {
                    "Xalpha three EDIT"
                } else {
                    "alpha three EDITX"
                },
                3,
            ),
        ] {
            let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
            let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
            for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                let context = format!("{editor} setting={setting} {desc} keys={keys:?}\n{output}");
                assert_eq!(
                    accepted_line(output).as_deref(),
                    Some(expected),
                    "{context}"
                );
                assert_eq!(bell_count(output), bells, "{context}");
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_ignore_case_numeric_substring_search() {
    let history = [
        "Alpha aa alpha old",
        "ALPHA bb Alpha mid",
        "alpha cc ALPHA new",
    ];
    for (setting, on) in [("off", ""), ("on", "set search-ignore-case on\n")] {
        for (direction, command, key, prefix) in [
            (
                "backward",
                "history-substring-search-backward",
                0x1du8,
                &b""[..],
            ),
            (
                "forward",
                "history-substring-search-forward",
                0x1eu8,
                &b"\x10\x10\x10\x01\x0b"[..],
            ),
        ] {
            let inputrc = format!(
                "{on}\"\\C-{}\": {command}",
                if key == 0x1d { "]" } else { "^" }
            );
            for (count, arg, point, mark) in match (setting, direction) {
                ("off", "backward") => [
                    ("one", "", "Xalpha cc ALPHA new", "alpha cc ALPHA newX"),
                    ("two", "\x1b2", "Alpha aa Xalpha old", "Alpha aa alpha oldX"),
                    (
                        "three",
                        "\x1b3",
                        "Alpha aa Xalpha old",
                        "Alpha aa alpha oldX",
                    ),
                ],
                ("off", "forward") => [
                    ("one", "", "Xalpha cc ALPHA new", "alpha cc ALPHA newX"),
                    ("two", "\x1b2", "alpha cc ALPHA newX", "alpha cc ALPHA newX"),
                    (
                        "three",
                        "\x1b3",
                        "alpha cc ALPHA newX",
                        "alpha cc ALPHA newX",
                    ),
                ],
                ("on", "backward") => [
                    ("one", "", "alpha cc XALPHA new", "alpha cc ALPHA newX"),
                    ("two", "\x1b2", "ALPHA bb XAlpha mid", "ALPHA bb Alpha midX"),
                    (
                        "three",
                        "\x1b3",
                        "Alpha aa Xalpha old",
                        "Alpha aa alpha oldX",
                    ),
                ],
                ("on", "forward") => [
                    ("one", "", "XALPHA bb Alpha mid", "ALPHA bb Alpha midX"),
                    ("two", "\x1b2", "Xalpha cc ALPHA new", "alpha cc ALPHA newX"),
                    (
                        "three",
                        "\x1b3",
                        "alpha cc ALPHA newX",
                        "alpha cc ALPHA newX",
                    ),
                ],
                _ => unreachable!(),
            } {
                for (finish, expected) in [(&b"X\r"[..], point), (&b"\x18\x18X\r"[..], mark)] {
                    let keys = [
                        prefix,
                        b"alpha".as_slice(),
                        arg.as_bytes(),
                        &[key][..],
                        finish,
                    ]
                    .concat();
                    let bash =
                        run_bash_readline_with_inputrc_file_and_history(&keys, &inputrc, &history);
                    let sushline =
                        run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history);
                    for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                        let context = format!(
                            "{editor} setting={setting} {direction} count={count} finish={finish:?} keys={keys:?}\n{output}"
                        );
                        assert_eq!(
                            accepted_line(output).as_deref(),
                            Some(expected),
                            "{context}"
                        );
                        assert_eq!(bell_count(output), 0, "{context}");
                    }
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_ignore_case_numeric_prefix_search() {
    let history = ["Alpha aa old", "ALPHA bb mid", "alpha cc new"];
    for setting in ["off", "on"] {
        for (direction, command, key, prefix) in [
            ("backward", "history-search-backward", 0x0fu8, &b""[..]),
            (
                "forward",
                "history-search-forward",
                0x1eu8,
                &b"\x10\x10\x10\x01\x0b"[..],
            ),
        ] {
            let inputrc = format!(
                "set search-ignore-case {setting}\n\"\\C-{}\": {command}",
                if key == 0x0f { "o" } else { "^" }
            );
            for (count, arg) in [("one", ""), ("two", "\x1b2"), ("three", "\x1b3")] {
                let (point, mark) = match (setting, direction, count) {
                    ("on", "backward", "three") => ("AlphaX aa old", "Alpha aa oldX"),
                    ("on", "backward", "two") | ("on", "forward", "one") => {
                        ("ALPHAX bb mid", "ALPHA bb midX")
                    }
                    _ => ("alphaX cc new", "alpha cc newX"),
                };
                for (finish, expected) in [(&b"X\r"[..], point), (&b"\x18\x18X\r"[..], mark)] {
                    let keys = [
                        prefix,
                        b"alpha".as_slice(),
                        arg.as_bytes(),
                        &[key][..],
                        finish,
                    ]
                    .concat();
                    let bash =
                        run_bash_readline_with_inputrc_file_and_history(&keys, &inputrc, &history);
                    let sushline =
                        run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history);
                    for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                        let context = format!(
                            "{editor} setting={setting} direction={direction} count={count} finish={finish:?} keys={keys:?}\n{output}"
                        );
                        assert_eq!(
                            accepted_line(output).as_deref(),
                            Some(expected),
                            "{context}"
                        );
                        assert_eq!(bell_count(output), 0, "{context}");
                    }
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_preserve_point_numeric_prefix_search() {
    let history = ["Alpha aa old", "ALPHA bb mid", "alpha cc new"];
    for setting in ["off", "on"] {
        let inputrc_prefix =
            format!("set search-ignore-case on\nset history-preserve-point {setting}\n");
        for (direction, command, key, prefix) in [
            ("backward", "history-search-backward", 0x0fu8, &b""[..]),
            (
                "forward",
                "history-search-forward",
                0x1eu8,
                &b"\x10\x10\x10\x01\x0b"[..],
            ),
        ] {
            let inputrc = format!(
                "{inputrc_prefix}\"\\C-{}\": {command}",
                if key == 0x0f { "o" } else { "^" }
            );
            for (count, arg) in [("one", ""), ("two", "\x1b2"), ("three", "\x1b3")] {
                let (point, mark) = match (direction, count) {
                    ("backward", "three") => ("AlpXha aa old", "Alpha aa oldX"),
                    ("backward", "two") | ("forward", "one") => ("ALPXHA bb mid", "ALPHA bb midX"),
                    _ => ("alpXha cc new", "alpha cc newX"),
                };
                for (finish, expected) in [(&b"X\r"[..], point), (&b"\x18\x18X\r"[..], mark)] {
                    let keys = [
                        prefix,
                        b"alpha".as_slice(),
                        b"\x1b[D\x1b[D".as_slice(),
                        arg.as_bytes(),
                        &[key][..],
                        finish,
                    ]
                    .concat();
                    let bash =
                        run_bash_readline_with_inputrc_file_and_history(&keys, &inputrc, &history);
                    let sushline =
                        run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history);
                    for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                        let context = format!(
                            "{editor} preserve={setting} direction={direction} count={count} finish={finish:?} keys={keys:?}\n{output}"
                        );
                        assert_eq!(
                            accepted_line(output).as_deref(),
                            Some(expected),
                            "{context}"
                        );
                        assert_eq!(bell_count(output), 0, "{context}");
                    }
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_ignore_case_negative_numeric_prefix_search() {
    let history = ["Alpha aa old", "ALPHA bb mid", "alpha cc new"];
    let oldest = &b"\x10\x10\x10\x01\x0b"[..];
    for (setting, on) in [("off", ""), ("on", "set search-ignore-case on\n")] {
        let inputrc =
            format!("{on}\"\\C-o\": history-search-backward\n\"\\C-^\": history-search-forward\n");
        for (direction, key, negative_prefix) in
            [("backward", 0x0fu8, oldest), ("forward", 0x1eu8, &b""[..])]
        {
            for (count, arg) in [(-1, "\x1b-1"), (-2, "\x1b-2"), (-3, "\x1b-3"), (0, "\x1b0")] {
                // Negative counts reverse direction; zero uses the oldest edit
                // in both directions to keep the starting mark identical.
                let prefix = if count == 0 { oldest } else { negative_prefix };
                let (point, mark) = match (setting, direction, count) {
                    (_, _, 0) => ("alphaX", "Xalpha"),
                    ("on", "backward", -1) | ("on", "forward", -2) => {
                        ("ALPHAX bb mid", "ALPHA bb midX")
                    }
                    ("on", "forward", -3) => ("AlphaX aa old", "Alpha aa oldX"),
                    _ => ("alphaX cc new", "alpha cc newX"),
                };
                for (finish, expected) in [(&b"X\r"[..], point), (&b"\x18\x18X\r"[..], mark)] {
                    let keys = [
                        prefix,
                        b"alpha".as_slice(),
                        arg.as_bytes(),
                        &[key][..],
                        finish,
                    ]
                    .concat();
                    let bash =
                        run_bash_readline_with_inputrc_file_and_history(&keys, &inputrc, &history);
                    let sushline =
                        run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history);
                    for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                        let context = format!(
                            "{editor} setting={setting} direction={direction} count={count} finish={finish:?} keys={keys:?}\n{output}"
                        );
                        assert_eq!(
                            accepted_line(output).as_deref(),
                            Some(expected),
                            "{context}"
                        );
                        assert_eq!(bell_count(output), 0, "{context}");
                    }
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_ignore_case_negative_numeric_substring_search() {
    let history = [
        "Alpha aa alpha old",
        "ALPHA bb Alpha mid",
        "alpha cc ALPHA new",
    ];
    for (setting, on) in [("off", ""), ("on", "set search-ignore-case on\n")] {
        // Negative counts reverse direction: backward starts at the oldest
        // saved edit, forward at the fresh edit.
        for (direction, command, key, prefix) in [
            (
                "backward",
                "history-substring-search-backward",
                0x1du8,
                &b"\x10\x10\x10\x01\x0b"[..],
            ),
            (
                "forward",
                "history-substring-search-forward",
                0x1eu8,
                &b""[..],
            ),
        ] {
            let inputrc = format!(
                "{on}\"\\C-{}\": {command}",
                if key == 0x1d { "]" } else { "^" }
            );
            for (count, arg, point, mark) in match (setting, direction) {
                ("off", "forward") => [
                    (-1, "\x1b-1", "Xalpha cc ALPHA new", "alpha cc ALPHA newX"),
                    (-2, "\x1b-2", "Alpha aa Xalpha old", "Alpha aa alpha oldX"),
                    (-3, "\x1b-3", "Alpha aa Xalpha old", "Alpha aa alpha oldX"),
                ],
                ("off", "backward") => [
                    (-1, "\x1b-1", "Xalpha cc ALPHA new", "alpha cc ALPHA newX"),
                    (-2, "\x1b-2", "alpha cc ALPHA newX", "alpha cc ALPHA newX"),
                    (-3, "\x1b-3", "alpha cc ALPHA newX", "alpha cc ALPHA newX"),
                ],
                ("on", "forward") => [
                    (-1, "\x1b-1", "alpha cc XALPHA new", "alpha cc ALPHA newX"),
                    (-2, "\x1b-2", "ALPHA bb XAlpha mid", "ALPHA bb Alpha midX"),
                    (-3, "\x1b-3", "Alpha aa Xalpha old", "Alpha aa alpha oldX"),
                ],
                ("on", "backward") => [
                    (-1, "\x1b-1", "XALPHA bb Alpha mid", "ALPHA bb Alpha midX"),
                    (-2, "\x1b-2", "Xalpha cc ALPHA new", "alpha cc ALPHA newX"),
                    (-3, "\x1b-3", "alpha cc ALPHA newX", "alpha cc ALPHA newX"),
                ],
                _ => unreachable!(),
            } {
                for (finish, expected) in [(&b"X\r"[..], point), (&b"\x18\x18X\r"[..], mark)] {
                    let keys = [
                        prefix,
                        b"alpha".as_slice(),
                        arg.as_bytes(),
                        &[key][..],
                        finish,
                    ]
                    .concat();
                    let bash =
                        run_bash_readline_with_inputrc_file_and_history(&keys, &inputrc, &history);
                    let sushline =
                        run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history);
                    for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                        let context = format!(
                            "{editor} setting={setting} {direction} count={count} finish={finish:?} keys={keys:?}\n{output}"
                        );
                        assert_eq!(
                            accepted_line(output).as_deref(),
                            Some(expected),
                            "{context}"
                        );
                        assert_eq!(bell_count(output), 0, "{context}");
                    }
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_zero_numeric_substring_search() {
    let history = [
        "Alpha aa alpha old",
        "ALPHA bb Alpha mid",
        "alpha cc ALPHA new",
    ];
    for setting in ["off", "on"] {
        for (direction, command, key) in [
            ("backward", "history-substring-search-backward", 0x1du8),
            ("forward", "history-substring-search-forward", 0x1eu8),
        ] {
            let inputrc = format!(
                "set search-ignore-case {setting}\n\"\\C-{}\": {command}",
                if key == 0x1d { "]" } else { "^" }
            );
            for (setup, prefix) in [
                ("fresh", &b""[..]),
                ("oldest", &b"\x10\x10\x10\x01\x0b"[..]),
            ] {
                for (finish, expected) in [(&b"X\r"[..], "alphaX"), (&b"\x18\x18X\r"[..], "Xalpha")]
                {
                    let keys = [prefix, b"alpha\x1b0".as_slice(), &[key][..], finish].concat();
                    let bash =
                        run_bash_readline_with_inputrc_file_and_history(&keys, &inputrc, &history);
                    let sushline =
                        run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history);
                    for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                        let context = format!(
                            "{editor} setting={setting} direction={direction} setup={setup} count=0 finish={finish:?} keys={keys:?}\n{output}"
                        );
                        assert_eq!(
                            accepted_line(output).as_deref(),
                            Some(expected),
                            "{context}"
                        );
                        assert_eq!(bell_count(output), 0, "{context}");
                    }
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_zero_numeric_prefix_search_fresh_buffer() {
    let history = ["Alpha aa old", "ALPHA bb mid", "alpha cc new"];
    for (setting, on) in [("off", ""), ("on", "set search-ignore-case on\n")] {
        let inputrc =
            format!("{on}\"\\C-o\": history-search-backward\n\"\\C-^\": history-search-forward\n");
        for (direction, key) in [("backward", 0x0fu8), ("forward", 0x1eu8)] {
            for (finish, expected) in [(&b"X\r"[..], "alphaX"), (&b"\x18\x18X\r"[..], "Xalpha")] {
                let keys = [b"alpha\x1b0".as_slice(), &[key][..], finish].concat();
                let bash =
                    run_bash_readline_with_inputrc_file_and_history(&keys, &inputrc, &history);
                let sushline =
                    run_sushline_harness_with_inputrc_and_history(&keys, &inputrc, &history);
                for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                    let context = format!(
                        "{editor} setting={setting} direction={direction} count=0 finish={finish:?} keys={keys:?}\n{output}"
                    );
                    assert_eq!(
                        accepted_line(output).as_deref(),
                        Some(expected),
                        "{context}"
                    );
                    assert_eq!(bell_count(output), 0, "{context}");
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_zero_numeric_search_preserves_explicit_mark() {
    let history = [
        "Alpha aa alpha old",
        "ALPHA bb Alpha mid",
        "alpha cc ALPHA new",
    ];
    for (family, inputrc, backward, forward) in [
        (
            "prefix",
            "\"\\C-o\": history-search-backward\n\"\\C-^\": history-search-forward\n",
            0x0fu8,
            0x1eu8,
        ),
        (
            "substring",
            "\"\\C-]\": history-substring-search-backward\n\"\\C-^\": history-substring-search-forward\n",
            0x1du8,
            0x1eu8,
        ),
    ] {
        for (direction, key) in [("backward", backward), ("forward", forward)] {
            for (lefts, mark, expected) in [(1u8, 4, "alphXa"), (4u8, 1, "aXlpha")] {
                for (with_search, label) in [(true, "search"), (false, "control")] {
                    let mut keys = b"alpha".to_vec();
                    keys.extend(std::iter::repeat_n([0x1b, b'[', b'D'], lefts as usize).flatten());
                    keys.push(0x00);
                    if with_search {
                        keys.extend_from_slice(b"\x1b0");
                        keys.push(key);
                    }
                    keys.extend_from_slice(b"\x18\x18X\r");
                    let bash =
                        run_bash_readline_with_inputrc_file_and_history(&keys, inputrc, &history);
                    let sushline =
                        run_sushline_harness_with_inputrc_and_history(&keys, inputrc, &history);
                    for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
                        let context = format!(
                            "{editor} family={family} direction={direction} mark={mark} {label} keys={keys:?}\n{output}"
                        );
                        assert_eq!(
                            accepted_line(output).as_deref(),
                            Some(expected),
                            "{context}"
                        );
                        assert_eq!(bell_count(output), 0, "{context}");
                    }
                }
            }
        }
    }
}

#[test]
fn bash_readline_and_sushline_exchange_with_unset_mark() {
    let history = ["Alpha aa old", "ALPHA bb mid", "alpha cc new"];
    for (label, keys, expected) in [
        ("empty", &b"\x18\x18X\r"[..], "X"),
        ("typed", &b"alpha\x18\x18X\r"[..], "Xalpha"),
        ("left1", &b"alpha\x1b[D\x18\x18X\r"[..], "Xalpha"),
        (
            "left4",
            &b"alpha\x1b[D\x1b[D\x1b[D\x1b[D\x18\x18X\r"[..],
            "Xalpha",
        ),
        ("ctrl_a", &b"alpha\x01\x18\x18X\r"[..], "Xalpha"),
        ("ctrl_e", &b"alpha\x05\x18\x18X\r"[..], "Xalpha"),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, "", &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{editor} {label} keys={keys:?}\n{output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), 0, "{context}");
        }
    }
}

#[test]
fn bash_and_sushline_ring_on_redundant_history_extreme_navigation() {
    let history = ["AAA", "BBB"];
    for (entries, keys, expected, bells) in [
        // beginning-of-history at the oldest entry rings once per invocation.
        (&history[..], &b"\x1b<\x1b<X\r"[..], "AAAX", 1),
        (&history[..], &b"\x10\x10\x1b<X\r"[..], "AAAX", 1),
        // The first move from the current line stays silent.
        (&history[..], &b"\x1b<X\r"[..], "AAAX", 0),
        // end-of-history at the end rings once, including an empty history.
        (&history[..], &b"\x1b>X\r"[..], "X", 1),
        (&history[..], &b"\x10\x10\x1b>\x1b>X\r"[..], "X", 1),
        (&[][..], &b"\x1b>X\r"[..], "X", 1),
        // A moving end and a beginning-then-end stay silent with entries.
        (&history[..], &b"\x10\x1b>X\r"[..], "X", 0),
        (&history[..], &b"\x1b<\x1b>X\r"[..], "X", 0),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, "", entries);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", entries);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{editor}: keys={keys:?} history={entries:?}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

#[test]
fn bash_and_sushline_ring_on_redundant_previous_history() {
    let history = ["AAA", "BBB"];
    let single = ["AAA"];
    for (entries, keys, expected, bells) in [
        // Starting at the oldest entry rings once per invocation.
        (&history[..], &b"\x10\x10\x10X\r"[..], "AAAX", 1),
        (&history[..], &b"\x10\x10\x10\x10X\r"[..], "AAAX", 2),
        (&history[..], &b"\x10\x10\x1b2\x10X\r"[..], "AAAX", 1),
        (&history[..], &b"\x10\x10\x1b9\x10X\r"[..], "AAAX", 1),
        (&single[..], &b"\x10\x10X\r"[..], "AAAX", 1),
        (&single[..], &b"\x10\x1b2\x10X\r"[..], "AAAX", 1),
        // Reaching the oldest mid-command, a zero count, an empty history,
        // and a negative count that reverses direction stay silent.
        (&history[..], &b"\x10\x1b2\x10X\r"[..], "AAAX", 0),
        (&history[..], &b"\x1b2\x10X\r"[..], "AAAX", 0),
        (&history[..], &b"\x10\x10\x1b0\x10X\r"[..], "AAAX", 0),
        (&[][..], &b"\x10X\r"[..], "X", 0),
        (&history[..], &b"\x10\x10\x1b-\x31\x10X\r"[..], "BBBX", 0),
    ] {
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, "", entries);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", entries);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{editor}: keys={keys:?} history={entries:?}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

#[test]
fn bash_and_sushline_fetch_history_counts_follow_the_cursor() {
    let fetch = r#""\C-o": fetch-history"#;
    let vi_fetch = r#""\C-o": vi-fetch-history"#;
    let five = ["one", "two", "three", "four", "five"];
    let empty: [&str; 0] = [];
    for (binding, entries, prefix, arg, suffix, expected) in [
        // From the end of the history, numbers are absolute.
        (fetch, &five[..], "", Some("1"), "X\r", "oneX"),
        (fetch, &five[..], "", Some("3"), "X\r", "threeX"),
        (fetch, &five[..], "", Some("5"), "X\r", "fiveX"),
        (fetch, &five[..], "", Some("6"), "X\r", "oneX"),
        (fetch, &five[..], "", Some("0"), "X\r", "oneX"),
        (fetch, &five[..], "", None, "X\r", "oneX"),
        (fetch, &five[..], "", Some("-1"), "X\r", "fiveX"),
        (fetch, &five[..], "", Some("-5"), "X\r", "oneX"),
        (fetch, &five[..], "", Some("-6"), "X\r", "oneX"),
        // After one Ctrl-P the position bounds positive numbers and anchors
        // negative ones; the fetched entry becomes the current one.
        (fetch, &five[..], "\x10", Some("4"), "X\r", "fourX"),
        (fetch, &five[..], "\x10", Some("5"), "X\r", "oneX"),
        (fetch, &five[..], "\x10", Some("-1"), "\x10X\r", "threeX"),
        (fetch, &five[..], "\x10", None, "X\r", "oneX"),
        // After two Ctrl-P steps, negative counts step backward from four.
        (fetch, &five[..], "\x10\x10", Some("1"), "X\r", "oneX"),
        (fetch, &five[..], "\x10\x10", Some("2"), "X\r", "twoX"),
        (fetch, &five[..], "\x10\x10", Some("3"), "X\r", "threeX"),
        (fetch, &five[..], "\x10\x10", Some("4"), "X\r", "oneX"),
        (fetch, &five[..], "\x10\x10", Some("-1"), "X\r", "threeX"),
        (fetch, &five[..], "\x10\x10", Some("-2"), "\x10X\r", "oneX"),
        (fetch, &five[..], "\x10\x10", Some("-3"), "X\r", "oneX"),
        (fetch, &five[..], "\x10\x10", Some("-4"), "X\r", "oneX"),
        (fetch, &five[..], "\x10\x10", None, "X\r", "oneX"),
        (fetch, &five[..], "\x10\x10", Some("1"), "\x0eX\r", "twoX"),
        (
            fetch,
            &five[..],
            "\x10\x10\x10\x10",
            Some("-1"),
            "X\r",
            "oneX",
        ),
        (vi_fetch, &five[..], "", Some("-1"), "X\r", "fiveX"),
        (vi_fetch, &five[..], "\x10\x10", Some("-1"), "X\r", "threeX"),
        (fetch, &empty[..], "", Some("-1"), "X\r", "X"),
    ] {
        let mut keys = prefix.to_string();
        if let Some(arg) = arg {
            keys.push('\x1b');
            keys.push_str(arg);
        }
        keys.push('\x0f');
        keys.push_str(suffix);
        let keys = keys.as_bytes();
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, binding, entries);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, binding, entries);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!(
                "{editor}: prefix={prefix:?} arg={arg:?} suffix={suffix:?} history={entries:?}: {output}"
            );
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), 0, "{context}");
        }
    }
}

#[test]
fn bash_and_sushline_operate_and_get_next_signed_counts() {
    let inputrc = r#""\C-o": operate-and-get-next"#;
    let history = ["one", "two", "three", "four", "five"];
    let empty: [&str; 0] = [];
    for (entries, prefix, arg, first, second) in [
        (&history[..], "", "0", "", "one"),
        (&history[..], "\x10", "0", "five", "one"),
        (&history[..], "\x10\x10", "0", "four", "one"),
        (&history[..], "", "-1", "", ""),
        (&history[..], "", "-3", "", ""),
        (&history[..], "\x10", "-1", "five", ""),
        (&history[..], "\x10", "-5", "five", ""),
        (&history[..], "", "1", "", "one"),
        (&history[..], "", "2", "", "two"),
        (&history[..], "", "6", "", ""),
        (&empty[..], "", "0", "", ""),
        (&empty[..], "", "-1", "", ""),
    ] {
        let keys = format!("{prefix}\x1b{arg}\x0f\r\r");
        let keys = keys.as_bytes();
        let bash =
            run_bash_readline_two_reads_with_inputrc_file_and_history(keys, inputrc, entries);
        let sushline =
            run_sushline_harness_two_reads_with_inputrc_and_history(keys, inputrc, entries);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context =
                format!("{editor}: prefix={prefix:?} arg={arg} history={entries:?}: {output}");
            assert_eq!(
                accepted_numbered_line(output, 1).as_deref(),
                Some(first),
                "{context}"
            );
            assert_eq!(
                accepted_numbered_line(output, 2).as_deref(),
                Some(second),
                "{context}"
            );
            assert_eq!(bell_count(output), 0, "{context}");
        }
    }
}

#[test]
fn bash_and_sushline_vi_fetch_history_out_of_range_counts_ring() {
    let inputrc = "set editing-mode vi\n";
    let history = ["one", "two", "three", "four", "five"];
    for (navigation, count, after_g, expected, bells) in [
        ("", "", "", "oneX", 0),
        ("", "1", "", "oneX", 0),
        ("", "2", "", "twoX", 0),
        ("", "5", "", "fiveX", 0),
        ("", "6", "", "X", 1),
        ("", "7", "", "X", 1),
        ("k", "4", "", "fourX", 0),
        ("k", "5", "", "fiveX", 1),
        ("k", "9", "", "fiveX", 1),
        ("kk", "3", "", "threeX", 0),
        ("kk", "4", "", "fourX", 1),
        ("kk", "4", "j", "fiveX", 1),
        ("kk", "4", "k", "threeX", 1),
        ("kkkk", "2", "", "twoX", 1),
    ] {
        let keys = format!("\x1b{navigation}{count}G{after_g}AX\r");
        let keys = keys.as_bytes();
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{editor} navigation={navigation:?} count={count:?}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }

    // A typed line is not a history entry: an excessive count rings but
    // leaves that fresh line in place, unlike a line already owned by the
    // entry at the current history position.
    let fresh_keys = b"fresh\x1b6GAX\r";
    for (editor, output) in [
        (
            "bash",
            run_bash_readline_with_inputrc_file_and_history(fresh_keys, inputrc, &history),
        ),
        (
            "sushline",
            run_sushline_harness_with_inputrc_and_history(fresh_keys, inputrc, &history),
        ),
    ] {
        let context = format!("{editor} fresh line: {output}");
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("freshX"),
            "{context}"
        );
        assert_eq!(bell_count(&output), 1, "{context}");
    }
}

#[test]
fn bash_and_sushline_vi_fetch_history_empty_history_explicit_one_rings() {
    let inputrc = "set editing-mode vi\n";
    let empty: [&str; 0] = [];
    for (count, bells) in [("", 0), ("1", 1), ("2", 1)] {
        let keys = format!("\x1b{count}GAX\r");
        let keys = keys.as_bytes();
        let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &empty);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &empty);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{editor} count={count:?}: {output}");
            assert_eq!(accepted_line(output).as_deref(), Some("X"), "{context}");
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_and_sushline_pin_incremental_search_continuation_after_resize() {
    use portable_pty::{CommandBuilder, PtySize};

    // The resize is a stressor only: this pins that the pending query survives
    // and accepts the same match, not post-resize repaint or redraw bytes.
    let dir = tempfile::tempdir().expect("search resize tempdir");
    let history = ["needle alpha", "other beta"];
    let mut bash_command = CommandBuilder::new("bash");
    bash_command.env("HISTFILE", dir.path().join("history").as_os_str());
    bash_command.env("HISTSIZE", "100");
    bash_command.env("HISTFILESIZE", "100");
    let seed = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    bash_command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; unset HISTTIMEFORMAT HISTCONTROL HISTIGNORE; history -c; {seed}; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    let mut sushline_command = sushline_harness_command();
    sushline_command.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline_command.env("SUSHLINE_HISTORY", history.join("\n"));

    for (editor, command) in [("Bash", bash_command), ("Sushline", sushline_command)] {
        let output = run_pty_steps_then_resize_until(
            command,
            &[(READY_PROMPT, &b"\x12nee"[..])],
            dir.path(),
            PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            },
            PtyResizeAction {
                marker: "nee",
                size: PtySize {
                    rows: 12,
                    cols: 20,
                    pixel_width: 0,
                    pixel_height: 0,
                },
                post_resize_marker: None,
                after_keys: b"dle\r",
            },
            "SUSHLINE_ACCEPTED:",
        );
        assert_eq!(
            accepted_line(&output).as_deref(),
            Some("needle alpha"),
            "{editor}: resize dropped incremental search state\n{output:?}"
        );
        assert_eq!(
            bell_count(&output),
            0,
            "{editor}: unexpected bell\n{output:?}"
        );
    }
}

#[test]
fn bash_and_sushline_pin_history_size_on_seeded_history() {
    require_oracle_host();
    // A setting loaded after history exists is the case under test: three
    // seeds, a cap of two, and navigation back past the oldest surviving entry.
    let inputrc = "set history-size 2";
    let history = ["one", "two", "three"];
    let keys = b"\x10\x10\x10\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some("two"),
        "unexpected Bash accepted buffer: bash={bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_and_sushline_pin_bind_set_history_size_on_seeded_history() {
    require_oracle_host();
    // The shell runs `bind 'set history-size N'` after seeding three entries;
    // Sushline applies the same line through `Editor::bind_api()`. The first
    // case trims, so a third backward move stays at the oldest survivor; the
    // second does not trim, so a forward move returns to the newest entry.
    let history = ["one", "two", "three"];
    for (set, keys, expected) in [
        ("set history-size 2", &b"\x10\x10\x10\r"[..], "two"),
        ("set history-size 5", &b"\x10\x10\x0e\r"[..], "three"),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, set, &history);
        let sushline = run_sushline_harness_with_bind_lines_and_history(keys, set, &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{set} {editor}: {output}"
            );
        }
    }
}

#[test]
fn bash_and_sushline_pin_history_size_reload_during_navigation() {
    require_oracle_host();
    // A cap above the seeded length trims nothing, so a reload in the middle
    // of backward navigation must leave the selected entry and edit intact.
    let inputrc = "set history-size 5";
    let history = ["one", "two", "three"];
    let keys = b"\x10\x18\x12\x10\r";
    let bash = run_bash_readline_with_inputrc_file_and_history(keys, inputrc, &history);
    let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some("two"),
        "unexpected Bash accepted buffer: bash={bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
}

#[test]
fn bash_and_sushline_pin_history_size_reload_trim_during_navigation() {
    require_oracle_host();
    // Two backward moves select the third of four entries before a reload
    // trims the oldest two. The measured GNU outcomes distinguish an
    // unpositioned history from one translated onto the surviving entries.
    let initial = "set history-size 5";
    let history = ["one", "two", "three", "four"];
    for (label, reloaded, keys, expected) in [
        (
            "no_key",
            "set history-size 2",
            &b"\x10\x10\x18\x12\r"[..],
            "three",
        ),
        (
            "next",
            "set history-size 2",
            &b"\x10\x10\x18\x12\x0e\r"[..],
            "",
        ),
        (
            "previous",
            "set history-size 2",
            &b"\x10\x10\x18\x12\x10\r"[..],
            "four",
        ),
        (
            "no_trim_next",
            "set history-size 5",
            &b"\x10\x10\x18\x12\x0e\r"[..],
            "four",
        ),
        (
            "previous_then_next",
            "set history-size 2",
            &b"\x10\x10\x18\x12\x10\x0e\r"[..],
            "",
        ),
    ] {
        let bash =
            run_bash_readline_with_reloaded_inputrc_and_history(keys, initial, reloaded, &history);
        let sushline = run_sushline_harness_with_reloaded_inputrc_and_history(
            keys, initial, reloaded, &history,
        );
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{label} {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), 0, "{context}");
        }
    }
}

#[test]
fn bash_readline_and_sushline_reverse_search_typed_unsaved_line_first() {
    let history = ["abcabc", "mid", "xabc"];
    for (desc, keys, expected, bells) in [
        ("first_bang", &b"abcabc\x01\x12abc!\r"[..], "abcabc", 1),
        ("first_enter", &b"abcabc\x01\x12abc\r"[..], "abcabc", 0),
        ("repeat_bang", &b"abcabc\x01\x12abc\x12!\r"[..], "xabc", 1),
        ("repeat_enter", &b"abcabc\x01\x12abc\x12\r"[..], "xabc", 0),
        // The recalled path from an empty buffer stays with history.
        ("empty_buffer", &b"\x12abc\r"[..], "xabc", 0),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, "", &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{desc}: {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

/// The unsaved-line match is point-aware and a repeat stays in that line
/// when an earlier occurrence remains. C-j terminates without executing so
/// a following `!` observes the point; only accepted lines and bells are
/// pinned, not redisplay bytes.
#[test]
fn bash_readline_and_sushline_reverse_search_typed_unsaved_line_point_and_repeat() {
    // Deficiency 1: the initial unsaved offset is the last occurrence
    // starting at or before the search point (GNU probe a647c344).
    let history = ["abcabc", "mid", "xabc"];
    for (desc, keys, expected, bells) in [
        ("point0", &b"abcabc\x01\x12abc\x0a!\r"[..], "!abcabc", 0),
        ("point1", &b"abcabc\x01\x06\x12abc\x0a!\r"[..], "!abcabc", 0),
        (
            "point2",
            &b"abcabc\x01\x06\x06\x12abc\x0a!\r"[..],
            "!abcabc",
            0,
        ),
        (
            "point3",
            &b"abcabc\x01\x06\x06\x06\x12abc\x0a!\r"[..],
            "abc!abc",
            0,
        ),
        (
            "point6",
            &b"abcabc\x01\x06\x06\x06\x06\x06\x06\x12abc\x0a!\r"[..],
            "abc!abc",
            0,
        ),
        // A repeat from point 0 has no earlier in-line occurrence, so it
        // steps to history (the measured C-a point-0 repeat still holds).
        (
            "repeat_from_point0_to_history",
            &b"abcabc\x01\x12abc\x12\x0a!\r"[..],
            "x!abc",
            0,
        ),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, "", &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{desc}: {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }

    // Deficiency 2: a repeat from an unsaved-line match first looks for
    // another occurrence in that same line (GNU probe f33b34c3).
    for (history, desc, keys, expected, bells) in [
        (
            &["mid"][..],
            "repeat_stays_single",
            &b"abcabc\x12abc\x12\x0a!\r"[..],
            "!abcabc",
            0,
        ),
        (
            &["xabc", "mid"][..],
            "repeat_stays_over_history",
            &b"abcabc\x12abc\x12\x0a!\r"[..],
            "!abcabc",
            0,
        ),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, "", history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{desc}: {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

/// A reverse scan cannot start before point 0 of `xabc`, so the only match in
/// the unsaved line is not searchable. GNU then accepts a matching history
/// entry, or keeps the typed line and rings once per failing query byte when
/// no entry matches. C-j plus `!` observes the accepted point without a bell.
#[test]
fn bash_readline_and_sushline_reverse_search_skips_unsaved_line_when_match_follows_point() {
    for (history, desc, keys, expected, bells) in [
        (
            &["abcabc", "mid", "zzabczz"][..],
            "history_match",
            &b"xabc\x01\x12abc\r"[..],
            "zzabczz",
            0,
        ),
        (
            &["abcabc", "mid", "zzabczz"][..],
            "history_match_point",
            &b"xabc\x01\x12abc\x0a!\r"[..],
            "zz!abczz",
            0,
        ),
        (
            &["mid"][..],
            "no_history_match",
            &b"xabc\x01\x12abc\r"[..],
            "xabc",
            3,
        ),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, "", history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, "", history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{desc}: {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

/// GNU falls back to an unsaved current line when forward search starts at the
/// end of history: the first occurrence at or after the point wins. C-a plus
/// C-f sets the point, C-t starts the search, and `!` extends the query and
/// rings once when that query had a match. C-j instead terminates before `!`,
/// so the accepted line observes the match point.
#[test]
fn bash_readline_and_sushline_forward_search_typed_unsaved_line_first() {
    let history = ["abcabc", "mid", "xabc"];
    let inputrc = r#""\C-t": forward-search-history"#;
    for (desc, keys, expected, bells) in [
        ("point0", &b"abcabc\x01\x14abc!\r"[..], "abcabc", 1),
        ("point1", &b"abcabc\x01\x06\x14abc!\r"[..], "abcabc", 1),
        (
            "point3",
            &b"abcabc\x01\x06\x06\x06\x14abc!\r"[..],
            "abcabc",
            1,
        ),
        (
            "point4_no_match",
            &b"abcabc\x01\x06\x06\x06\x06\x14abc!\r"[..],
            "abcabc",
            4,
        ),
        ("query_a_point0", &b"abcabc\x01\x14a!\r"[..], "abcabc", 1),
        ("point0_cj", &b"abcabc\x01\x14abc\x0a!\r"[..], "!abcabc", 0),
        (
            "point1_cj",
            &b"abcabc\x01\x06\x14abc\x0a!\r"[..],
            "abc!abc",
            0,
        ),
        (
            "point3_cj",
            &b"abcabc\x01\x06\x06\x06\x14abc\x0a!\r"[..],
            "abc!abc",
            0,
        ),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{desc}: {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

/// A repeat first takes the next occurrence in the unsaved line; exhausting
/// those occurrences rings once. When the first search found no occurrence at
/// or after the point, each failed forward repeat rings twice. The measured
/// start points 4/5/6 have no later `abc`, so their first search rings four
/// times (once per query byte) and one repeat rings six times.
#[test]
fn bash_readline_and_sushline_forward_search_typed_unsaved_line_repeat() {
    let history = ["abcabc", "mid", "xabc"];
    let inputrc = r#""\C-t": forward-search-history"#;
    for (desc, keys, bells) in [
        ("next_occurrence", &b"abcabc\x01\x14abc\x14!\r"[..], 1),
        ("in_line_exhausted", &b"abcabc\x01\x14abc\x14\x14!\r"[..], 2),
        ("a_next_occurrence", &b"abcabc\x01\x14a\x14!\r"[..], 1),
        (
            "no_match_point4",
            &b"abcabc\x01\x06\x06\x06\x06\x14abc\x14!\r"[..],
            6,
        ),
        (
            "no_match_point5",
            &b"abcabc\x01\x06\x06\x06\x06\x06\x14abc\x14!\r"[..],
            6,
        ),
        (
            "no_match_point6",
            &b"abcabc\x01\x06\x06\x06\x06\x06\x06\x14abc\x14!\r"[..],
            6,
        ),
        (
            "no_match_query_a_point4",
            &b"abcabc\x01\x06\x06\x06\x06\x14a\x14!\r"[..],
            4,
        ),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{desc}: {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some("abcabc"),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}

/// A direction switch from an unsaved-line match keeps that occurrence: GNU
/// does not advance in-line nor step into history, staying silent. With
/// history `ZZZabcZZZ`, `mid`, `abcabcabc` and `C-t` bound to
/// forward-search-history, typing `abcabcabc` then `C-a`, `C-r`, `abc`,
/// `C-t` stays at the reverse match (0 from point 0, 3 from point 3). `C-j`
/// plus `!` observes the point without a bell; extending the query with `!`
/// instead fails once. Points and bells are the measured Bash 5.3 values.
#[test]
fn bash_readline_and_sushline_search_direction_switch_keeps_unsaved_match() {
    let history = ["ZZZabcZZZ", "mid", "abcabcabc"];
    let inputrc = r#""\C-t": forward-search-history"#;
    for (desc, keys, expected, bells) in [
        (
            "point0",
            &b"abcabcabc\x01\x12abc\x14\x0a!\r"[..],
            "!abcabcabc",
            0,
        ),
        (
            "point3",
            &b"abcabcabc\x01\x06\x06\x06\x12abc\x14\x0a!\r"[..],
            "abc!abcabc",
            0,
        ),
        (
            "point0_query_bang",
            &b"abcabcabc\x01\x12abc\x14!\r"[..],
            "abcabcabc",
            1,
        ),
        (
            "point3_query_bang",
            &b"abcabcabc\x01\x06\x06\x06\x12abc\x14!\r"[..],
            "abcabcabc",
            1,
        ),
    ] {
        let bash = run_bash_readline_with_bindings_and_history(keys, inputrc, &history);
        let sushline = run_sushline_harness_with_inputrc_and_history(keys, inputrc, &history);
        for (editor, output) in [("bash", &bash), ("sushline", &sushline)] {
            let context = format!("{desc}: {editor}: {output}");
            assert_eq!(
                accepted_line(output).as_deref(),
                Some(expected),
                "{context}"
            );
            assert_eq!(bell_count(output), bells, "{context}");
        }
    }
}
