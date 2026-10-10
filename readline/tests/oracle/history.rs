#![allow(unused_imports)]
use super::pty::*;
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
