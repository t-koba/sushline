use super::pty::*;
use std::fs;

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
fn completion_ignore_case_matches_utf8_case_pairs_only_in_utf8_locale() {
    if !c_utf8_locale_available() {
        return;
    }

    // Bash 5.3 PTY runtime observations, not source proof: the accented pair
    // matches under C.UTF-8 and remains distinct under C.
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("ÄLPHA"), "").expect("write fixture");
    let typed = format!("{}/älp\t\r", dir.path().display());
    let matched = format!("{}/ÄLPHA ", dir.path().display());
    let unmatched = format!("{}/älp", dir.path().display());
    let inputrc = "set completion-ignore-case on";

    let locale_env = [("LC_ALL", "C.UTF-8")];
    let bash = run_bash_readline_with_inputrc_file_and_env(typed.as_bytes(), inputrc, &locale_env);
    let sushline =
        run_sushline_harness_with_inputrc_and_env(typed.as_bytes(), inputrc, &locale_env);
    assert_eq!(accepted_line(&bash), Some(matched.clone()), "{bash}");
    assert_eq!(accepted_line(&sushline), Some(matched), "{sushline}");

    let locale_env = [("LC_ALL", "C")];
    let c_typed = format!("{}/alp\t\r", dir.path().display());
    let c_unmatched = format!("{}/alp", dir.path().display());
    let bash =
        run_bash_readline_with_inputrc_file_and_env(c_typed.as_bytes(), inputrc, &locale_env);
    let sushline =
        run_sushline_harness_with_inputrc_and_env(c_typed.as_bytes(), inputrc, &locale_env);
    assert_eq!(accepted_line(&bash), Some(c_unmatched.clone()), "{bash}");
    assert_eq!(accepted_line(&sushline), Some(c_unmatched), "{sushline}");
    // Bash preserves the non-ASCII typed pair in C. Sushline's input-meta
    // policy is separately byte-oriented there, so its matcher is covered by
    // the direct C-locale unit test instead of this PTY input path.
    let bash = run_bash_readline_with_inputrc_file_and_env(typed.as_bytes(), inputrc, &locale_env);
    assert_eq!(accepted_line(&bash), Some(unmatched), "{bash}");
}

#[test]
fn completion_ignore_case_matches_held_out_kelvin_byte_offsets_and_rejects_non_casefolds() {
    if !c_utf8_locale_available() {
        return;
    }

    // Runtime-only Bash 5.3 PTY observations. The kzz/kzze results exercise
    // the candidate-byte-width offset rule; kzzz and final-sigma are negatives.
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("Kelvin"), "").expect("write Kelvin fixture");
    fs::write(dir.path().join("Σigma"), "").expect("write sigma fixture");
    fs::write(dir.path().join("İndex"), "").expect("write dotted-I fixture");
    let inputrc = "set completion-ignore-case on";
    let locale_env = [("LC_ALL", "C.UTF-8")];

    for (prefix, expected) in [
        ("kzz", format!("{}/Kelvin ", dir.path().display())),
        ("kzze", format!("{}/Kelvin ", dir.path().display())),
        ("kzzz", format!("{}/kzzz", dir.path().display())),
        ("i", format!("{}/i", dir.path().display())),
        ("in", format!("{}/İndex ", dir.path().display())),
        ("ς", format!("{}/ς", dir.path().display())),
    ] {
        let typed = format!("{}/{prefix}\t\r", dir.path().display());
        let bash =
            run_bash_readline_with_inputrc_file_and_env(typed.as_bytes(), inputrc, &locale_env);
        let sushline =
            run_sushline_harness_with_inputrc_and_env(typed.as_bytes(), inputrc, &locale_env);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.clone()),
            "prefix={prefix:?}\n{bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected),
            "prefix={prefix:?}\n{sushline}"
        );
    }
}

#[test]
fn completion_map_case_requires_ignore_case_and_preserves_filename_spelling() {
    if !c_utf8_locale_available() {
        return;
    }

    // Bash 5.3 PTY runtime observations under C.UTF-8 (not source proof):
    // separator mapping applies only when both variables are on, in either
    // direction. Successful completion inserts the candidate's spelling.
    let locale_env = [("LC_ALL", "C.UTF-8")];
    for (candidate, prefix) in [
        ("alpha-dir", "alpha_dir"),
        ("alpha_dir", "alpha-dir"),
        ("Alpha-DIR", "alpha_d"),
        ("Älpha-dir", "älpha_dir"),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join(candidate), "").expect("write fixture");

        for (ignore_case, map_case) in [(false, false), (true, false), (false, true), (true, true)]
        {
            let inputrc = format!(
                "set completion-ignore-case {}\nset completion-map-case {}\nset mark-directories off",
                if ignore_case { "on" } else { "off" },
                if map_case { "on" } else { "off" },
            );
            let typed = format!("{}/{prefix}\t\r", dir.path().display());
            let expected = if ignore_case && map_case {
                format!("{}/{candidate} ", dir.path().display())
            } else {
                format!("{}/{prefix}", dir.path().display())
            };

            let bash = run_bash_readline_with_inputrc_file_and_env(
                typed.as_bytes(),
                &inputrc,
                &locale_env,
            );
            let sushline =
                run_sushline_harness_with_inputrc_and_env(typed.as_bytes(), &inputrc, &locale_env);
            assert_eq!(
                accepted_line(&bash),
                Some(expected.clone()),
                "candidate={candidate:?} prefix={prefix:?} ignore_case={ignore_case} map_case={map_case}\n{bash}"
            );
            assert_eq!(
                accepted_line(&sushline),
                Some(expected),
                "candidate={candidate:?} prefix={prefix:?} ignore_case={ignore_case} map_case={map_case}\n{sushline}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_complete_same_mapped_case_directory_without_marker() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("alpha-dir")).expect("mkdir fixture");
    let typed = format!("{}/alpha_dir\t\r", dir.path().display());
    let inputrc =
        "set completion-ignore-case on\nset completion-map-case on\nset mark-directories off";
    let expected = format!("{}/alpha-dir", dir.path().display());

    let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
    let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

    assert_eq!(accepted_line(&bash), Some(expected.clone()), "{bash}");
    assert_eq!(accepted_line(&sushline), Some(expected), "{sushline}");
}

#[test]
fn bash_readline_and_sushline_mark_regular_directories_in_filename_completion() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("alpha-dir")).expect("mkdir fixture");
    let typed = format!("{}/alp\t\r", dir.path().display());

    for (inputrc, expected_suffix) in [
        ("", "/"),
        ("set mark-directories on", "/"),
        ("set mark-directories off", ""),
        (
            "set mark-directories off\nset mark-symlinked-directories on",
            "",
        ),
    ] {
        let expected = format!(
            "{}{expected_suffix}",
            dir.path().join("alpha-dir").display()
        );
        let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.clone()),
            "inputrc={inputrc:?}\n{bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected),
            "inputrc={inputrc:?}\n{sushline}"
        );
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_complete_same_symlinked_directory_markers() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("target-dir")).expect("mkdir fixture");
    symlink(dir.path().join("target-dir"), dir.path().join("alpha-link")).expect("symlink fixture");
    let typed = format!("{}/alp\t\r", dir.path().display());
    let path = dir.path().join("alpha-link").display().to_string();

    // The empty inputrc checks the Readline defaults: regular directories are
    // marked, but symlinked directories are not. The explicit matrix isolates
    // the two inputrc variables and asserts the accepted line, not just parity.
    for (inputrc, expected_suffix) in [
        ("", ""),
        (
            "set mark-directories on\nset mark-symlinked-directories on",
            "/",
        ),
        (
            "set mark-directories on\nset mark-symlinked-directories off",
            "",
        ),
        (
            "set mark-directories off\nset mark-symlinked-directories on",
            "",
        ),
        (
            "set mark-directories off\nset mark-symlinked-directories off",
            "",
        ),
    ] {
        let expected = format!("{path}{expected_suffix}");
        let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.clone()),
            "inputrc={inputrc:?}\n{bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected),
            "inputrc={inputrc:?}\n{sushline}"
        );
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_possible_completions_mark_directory_display_slashes() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("alpha-dir")).expect("mkdir regular fixture");
    fs::create_dir(dir.path().join("target-dir")).expect("mkdir symlink target");
    symlink(dir.path().join("target-dir"), dir.path().join("alpha-link")).expect("symlink fixture");
    let typed = format!("{}/al\x0f\r", dir.path().display());

    for (mark_directories, mark_symlinked_directories, display_slash) in [
        (true, false, true),
        (true, true, true),
        (false, true, false),
        (false, false, false),
    ] {
        let inputrc = format!(
            "\"\\C-o\": possible-completions\nset completion-query-items 999\nset mark-directories {}\nset mark-symlinked-directories {}",
            if mark_directories { "on" } else { "off" },
            if mark_symlinked_directories {
                "on"
            } else {
                "off"
            },
        );
        let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), &inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), &inputrc);

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            for candidate in ["alpha-dir", "alpha-link"] {
                let rendered = format!("{candidate}{}", if display_slash { "/" } else { "" });
                assert!(
                    output.contains(&rendered),
                    "missing displayed candidate {rendered:?}, implementation={implementation}, inputrc={inputrc:?}\n{output}"
                );
                if !display_slash {
                    assert!(
                        !output.contains(&format!("{candidate}/")),
                        "unexpected display slash for {candidate:?}, implementation={implementation}, inputrc={inputrc:?}\n{output}"
                    );
                }
            }
        }
        assert_eq!(
            accepted_line(&bash),
            Some(format!("{}/al", dir.path().display())),
            "inputrc={inputrc:?}\n{bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(format!("{}/al", dir.path().display())),
            "inputrc={inputrc:?}\n{sushline}"
        );

        // Display marking follows mark-directories alone; inserting a slash
        // for a symlinked directory additionally requires its own option.
        assert_eq!(display_slash, mark_directories, "test matrix setup");
    }
}

#[test]
fn bash_readline_and_sushline_complete_same_hidden_filename_cases() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join(".alpha"), "").expect("write hidden fixture");

    // Bash 5.3 PTY runtime observations for this isolated fixture only, not
    // proof of Readline source behavior.
    for (inputrc, typed, expected) in [
        (
            "",
            format!("{}/\t\r", dir.path().display()),
            format!("{}/.alpha ", dir.path().display()),
        ),
        (
            "set match-hidden-files on",
            format!("{}/\t\r", dir.path().display()),
            format!("{}/.alpha ", dir.path().display()),
        ),
        (
            "set match-hidden-files off",
            format!("{}/\t\r", dir.path().display()),
            format!("{}/", dir.path().display()),
        ),
        (
            "set match-hidden-files off",
            format!("{}/.a\t\r", dir.path().display()),
            format!("{}/.alpha ", dir.path().display()),
        ),
        (
            "set match-hidden-files on",
            format!("{}/a\t\r", dir.path().display()),
            format!("{}/a", dir.path().display()),
        ),
    ] {
        let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.clone()),
            "inputrc={inputrc:?}\ntyped={typed:?}\n{bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected),
            "inputrc={inputrc:?}\ntyped={typed:?}\n{sushline}"
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

    let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
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
fn bash_readline_and_sushline_list_completion_candidates_in_same_sorted_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    for name in ["gamma", "alpha", "beta"] {
        fs::write(dir.path().join(name), "").expect("write fixture");
    }
    let inputrc = "\"\\C-o\": possible-filename-completions\nset completion-query-items 999";
    let typed = format!("{}/\x0f\r", dir.path().display());

    let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
    let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

    assert_same_candidate_order(&bash, &sushline, &["alpha", "beta", "gamma"]);
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
fn bash_readline_and_sushline_complete_home_tilde_filenames_with_expansion_setting() {
    // Bash 5.3 PTY runtime evidence for these filename-completion forms only;
    // this does not establish Readline source behavior or other tilde forms.
    let home = tempfile::tempdir().expect("temp HOME");
    fs::write(home.path().join("Alpha"), "").expect("write HOME fixture");
    let home_path = home.path().to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    let assert_case = |word: &str, expected_off: String, expected_on: String| {
        for (expand_tilde, expected) in [("off", expected_off), ("on", expected_on)] {
            let inputrc = format!("set expand-tilde {expand_tilde}\nset mark-directories off");
            let keys = format!("{word}\t\r");
            let bash = run_bash_readline_with_inputrc_file_and_env(keys.as_bytes(), &inputrc, &env);
            let sushline =
                run_sushline_harness_with_inputrc_and_env(keys.as_bytes(), &inputrc, &env);

            assert_eq!(
                accepted_line(&bash),
                Some(expected.clone()),
                "word={word:?} expand-tilde={expand_tilde}\n{bash}"
            );
            assert_eq!(
                accepted_line(&sushline),
                Some(expected.clone()),
                "word={word:?} expand-tilde={expand_tilde}\n{sushline}"
            );
            assert_eq!(
                accepted_line(&sushline),
                accepted_line(&bash),
                "word={word:?} expand-tilde={expand_tilde}\nbash={bash}\nsushline={sushline}"
            );
        }
    };

    // Keep ~/ unambiguous, as in the measured probe, before adding sub/.
    assert_case("~/", "~/Alpha ".to_string(), format!("{home_path}/Alpha "));

    fs::create_dir(home.path().join("sub")).expect("create nested directory");
    fs::write(home.path().join("sub/Alpha"), "").expect("write nested fixture");
    assert_case(
        "~/Al",
        "~/Alpha ".to_string(),
        format!("{home_path}/Alpha "),
    );
    assert_case(
        "~/sub/Al",
        "~/sub/Alpha ".to_string(),
        format!("{home_path}/sub/Alpha "),
    );
}

#[test]
fn filename_tilde_completion_respects_raw_quote_and_escape_context() {
    // Runtime-only Bash 5.3 evidence for this isolated HOME fixture and these
    // accepted lines; it does not establish other tilde forms or unset-HOME behavior.
    let home = tempfile::tempdir().expect("temp HOME");
    fs::write(home.path().join("Alpha"), "").expect("write HOME fixture");
    let home_path = home.path().to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for (word, expand_tilde, expected) in [
        ("~/Al", "off", "~/Alpha ".to_string()),
        ("~/Al", "on", format!("{home_path}/Alpha ")),
        ("'~/Al", "off", format!("'{home_path}/Alpha' ")),
        ("\"~/Al", "off", format!("\"{home_path}/Alpha\" ")),
        ("\"~/Al", "on", format!("\"{home_path}/Alpha\" ")),
        ("\\~/Al", "off", "\\~/Al".to_string()),
        ("\\~/Al", "on", "\\~/Al".to_string()),
        ("\"\\~/Al", "off", "\"\\~/Al".to_string()),
        ("\"\\~/Al", "on", "\"\\~/Al".to_string()),
    ] {
        let inputrc = format!("set expand-tilde {expand_tilde}\nset mark-directories off");
        let keys = format!("{word}\t\r");
        let bash = run_bash_readline_with_inputrc_file_and_env(keys.as_bytes(), &inputrc, &env);
        let sushline = run_sushline_harness_with_inputrc_and_env(keys.as_bytes(), &inputrc, &env);

        assert_eq!(
            accepted_line(&bash),
            Some(expected.clone()),
            "word={word:?} expand-tilde={expand_tilde}\n{bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected.clone()),
            "word={word:?} expand-tilde={expand_tilde}\n{sushline}"
        );
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_complete_literal_escaped_tilde_in_double_quotes() {
    // Bash 5.3 PTY runtime evidence for this isolated cwd fixture and the two
    // expand-tilde settings only; it does not establish broader tilde behavior.
    let cwd = tempfile::tempdir().expect("temp cwd");
    let literal_tilde_dir_name = ['\\', '~'].iter().collect::<String>();
    let literal_tilde_dir = cwd.path().join(literal_tilde_dir_name);
    fs::create_dir(&literal_tilde_dir).expect("create literal tilde directory");
    fs::write(literal_tilde_dir.join("Alpha"), "").expect("write fixture");
    let keys = b"\"\\\\~/Al\t\r";
    let expected = "\"\\\\~/Alpha\" ".to_string();

    for expand_tilde in ["off", "on"] {
        let inputrc = format!("set expand-tilde {expand_tilde}");
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &[], cwd.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &[], cwd.path());

        assert_eq!(
            accepted_line(&bash),
            Some(expected.clone()),
            "expand-tilde={expand_tilde}\n{bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(expected.clone()),
            "expand-tilde={expand_tilde}\n{sushline}"
        );
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "expand-tilde={expand_tilde}\nbash={bash}\nsushline={sushline}"
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

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_display_colored_filename_stats_for_tested_types() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn run_pair(keys: &[u8], inputrc: &str, ls_colors: Option<&str>) -> (String, String) {
        if let Some(ls_colors) = ls_colors {
            let env = [("LS_COLORS", ls_colors)];
            (
                run_bash_readline_with_inputrc_file_and_env(keys, inputrc, &env),
                run_sushline_harness_with_inputrc_and_env(keys, inputrc, &env),
            )
        } else {
            (
                run_bash_readline_with_inputrc_file_and_env_removing(
                    keys,
                    inputrc,
                    &[],
                    &["LS_COLORS"],
                ),
                run_sushline_harness_with_inputrc_and_env_removing(
                    keys,
                    inputrc,
                    &[],
                    &["LS_COLORS"],
                ),
            )
        }
    }

    fn assert_color(output: &str, name: &str, code: &str, suffix: &str) {
        let expected = format!("\x1b[{code}m{name}\x1b[0m{suffix}");
        assert!(
            output.contains(&expected),
            "missing {expected:?} in {output:?}"
        );
    }

    fn assert_colored_executable_marker(output: &str, code: &str) {
        let colored_name = format!("\x1b[{code}malpha-exec\x1b[0m");
        let start = output
            .find(&colored_name)
            .unwrap_or_else(|| panic!("missing {colored_name:?} in {output:?}"));
        let after_reset = &output[start + colored_name.len()..];
        let after_terminal_clear = after_reset.strip_prefix("\x1b[K").unwrap_or(after_reset);
        assert!(
            after_terminal_clear.starts_with('*'),
            "executable marker must follow the filename SGR reset, outside colored text: {output:?}"
        );
    }

    fn assert_directory_color(output: &str, code: &str) {
        let colored_name = format!("\x1b[{code}malpha-dir\x1b[0m");
        let start = output
            .find(&colored_name)
            .unwrap_or_else(|| panic!("missing {colored_name:?} in {output:?}"));
        let after_reset = &output[start + colored_name.len()..];
        let after_terminal_clear = after_reset.strip_prefix("\x1b[K").unwrap_or(after_reset);
        assert!(
            after_terminal_clear.starts_with('/'),
            "directory slash must follow the filename reset, outside SGR text: {output:?}"
        );
        assert!(
            !output.contains(&format!("\x1b[{code}malpha-dir/\x1b[0m")),
            "directory slash must not be colored: {output:?}"
        );
    }

    fn assert_plain(output: &str, name: &str) {
        assert!(output.contains(name), "missing {name:?} in {output:?}");
        for code in ["31", "32", "35", "36", "01;32", "01;34", "01;36"] {
            assert!(
                !output.contains(&format!("\x1b[{code}m{name}\x1b[0m")),
                "unexpected {code:?} style on {name:?}: {output:?}"
            );
        }
    }

    fn assert_fallbacks(output: &str) {
        assert_directory_color(output, "01;34");
        assert_color(output, "alpha-exec", "01;32", "");
        assert_color(output, "alpha-link", "01;36", "");
        assert_plain(output, "alpha-file");
    }

    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("alpha-dir")).expect("mkdir fixture");
    fs::write(dir.path().join("alpha-file"), "").expect("write fixture");
    let executable = dir.path().join("alpha-exec");
    fs::write(&executable, "").expect("write executable fixture");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
        .expect("chmod executable fixture");
    symlink(dir.path().join("alpha-file"), dir.path().join("alpha-link")).expect("symlink fixture");

    let typed = format!("{}/al\x0f\r", dir.path().display());
    let binding = "\"\\C-o\": possible-filename-completions\nset completion-query-items 999";
    let enabled = format!("{binding}\nset colored-stats on");
    let explicit = "di=35:fi=31:ex=32:ln=36";

    let (bash, sushline) = run_pair(typed.as_bytes(), &enabled, Some(explicit));
    for output in [&bash, &sushline] {
        assert_directory_color(output, "35");
        assert_color(output, "alpha-file", "31", "");
        assert_color(output, "alpha-exec", "32", "");
        assert_color(output, "alpha-link", "36", "");
    }

    let colored_and_visible = format!("{binding}\nset colored-stats on\nset visible-stats on");
    let (bash, sushline) = run_pair(typed.as_bytes(), &colored_and_visible, Some(explicit));
    for output in [&bash, &sushline] {
        assert_colored_executable_marker(output, "32");
    }

    // colored-stats defaults off. Explicit off has the same behavior even
    // when LS_COLORS supplies styles for every tested kind.
    for inputrc in [
        binding.to_string(),
        format!("{binding}\nset colored-stats off"),
    ] {
        let (bash, sushline) = run_pair(typed.as_bytes(), &inputrc, Some(explicit));
        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            for name in ["alpha-dir/", "alpha-file", "alpha-exec", "alpha-link"] {
                assert_plain(output, name);
            }
            assert!(
                !["31", "32", "35", "36"]
                    .iter()
                    .any(|code| output.contains(&format!("\x1b[{code}m"))),
                "colored-stats off emitted candidate SGR ({implementation}): {output:?}"
            );
        }
    }

    // Bash 5.3 PTY runtime observations: absent and empty LS_COLORS use the
    // tested default styles; a missing type entry falls back independently.
    for ls_colors in [None, Some("")] {
        let (bash, sushline) = run_pair(typed.as_bytes(), &enabled, ls_colors);
        assert_fallbacks(&bash);
        assert_fallbacks(&sushline);
    }

    let (bash, sushline) = run_pair(typed.as_bytes(), &enabled, Some("fi=31"));
    for output in [&bash, &sushline] {
        assert_directory_color(output, "01;34");
        assert_color(output, "alpha-file", "31", "");
        assert_color(output, "alpha-exec", "01;32", "");
        assert_color(output, "alpha-link", "01;36", "");
    }

    let (bash, sushline) = run_pair(typed.as_bytes(), &enabled, Some("di=35"));
    for output in [&bash, &sushline] {
        assert_directory_color(output, "35");
        assert_plain(output, "alpha-file");
        assert_color(output, "alpha-exec", "01;32", "");
        assert_color(output, "alpha-link", "01;36", "");
    }
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
