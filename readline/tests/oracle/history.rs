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
fn history_navigation_search_ignore_case_matches_locale_case_in_utf8_locale() {
    if !c_utf8_locale_available() {
        return;
    }

    // Bash 5.3 PTY runtime evidence under C.UTF-8; this does not establish the
    // host's linked Readline version.
    let history = ["ÄLPHA old"];
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for command in [
        "history-search-backward",
        "history-substring-search-backward",
    ] {
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
    // GNU inhibits `!` expansion inside single quotes by default with no
    // variable involved; the unknown variable is ignored on both sides.
    // The real-prompt path is required: `read -e` under `-c` never expands.
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
    let (screen, sink) = run_bash_interactive_with_sink(
        &[b"!missing".to_vec(), vec![0x0f]],
        &inputrc,
        &sink,
        "!missing: event not found",
    );
    assert!(
        sink.contains("!missing: event not found"),
        "bash sink: {sink}"
    );
    // The failure text went to the sink, so the terminal did not show it.
    assert!(!screen.contains("event not found"), "bash screen: {screen}");
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
