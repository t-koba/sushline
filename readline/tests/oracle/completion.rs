use super::pty::*;
use std::fs;

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_skip_completed_filename_text_like_bash_5_3() {
    let dir = tempfile::tempdir().expect("temp completion directory");
    fs::write(dir.path().join("alpha-one"), "").expect("write completion fixture");

    // Bash 5.3 PTY runtime observations for this single filename and a cursor
    // immediately before its `one` suffix; these are not source proof.
    for (skip_completed, initial, expected) in [
        ("off", "alpha-one", "alpha-oneone"),
        ("on", "alpha-one", "alpha-one "),
        ("on", "alpha-oneX", "alpha-oneX"),
        ("on", "alpha-z", "alpha-onez"),
        ("off", "alpha-one rest", "alpha-oneone rest"),
        ("on", "alpha-one rest", "alpha-one rest"),
        ("off", "alpha- rest", "alpha-one rest"),
        ("on", "alpha- rest", "alpha-one rest"),
    ] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset skip-completed-text {skip_completed}\nset completion-query-items 999\nset page-completions off"
        );
        let mut keys = initial.as_bytes().to_vec();
        keys.extend_from_slice(b"\x01");
        keys.extend_from_slice(&[b'\x06'; b"alpha-".len()]);
        keys.extend_from_slice(b"\x0f\r");

        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(&keys, &inputrc, &[], dir.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(&keys, &inputrc, &[], dir.path());
        let expected = Some(expected.to_string());

        assert_eq!(
            accepted_line(&bash),
            expected,
            "skip-completed-text={skip_completed}, initial={initial:?}\n{bash}"
        );
        assert_eq!(
            accepted_line(&sushline),
            expected,
            "skip-completed-text={skip_completed}, initial={initial:?}\n{sushline}"
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

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_disable_completion_match_bash_5_3() {
    let dir = tempfile::tempdir().expect("temp completion directory");
    for name in ["alpha-one", "alpha-two"] {
        fs::write(dir.path().join(name), "").expect("write completion fixture");
    }

    // Bash 5.3 PTY runtime observations for this exact fixture and input only;
    // these are not source proof or a general disable-completion claim.
    for (disable_completion, expected, expected_bells) in [("off", "alpha-", 1), ("on", "al\t", 0)]
    {
        let inputrc = format!(
            "set disable-completion {disable_completion}\nset completion-query-items 999\nset page-completions off"
        );
        let keys = b"al\t\r";
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &[], dir.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &[], dir.path());
        let expected = Some(expected.to_string());

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            assert_eq!(
                accepted_line(output),
                expected,
                "disable-completion={disable_completion}, implementation={implementation}\n{output}"
            );
            assert_eq!(
                bell_count(output),
                expected_bells,
                "disable-completion={disable_completion}, implementation={implementation}\n{output}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_show_all_for_tested_ambiguous_and_unmodified_filename_completion() {
    let dir = tempfile::tempdir().expect("temp completion directory");
    for name in ["alpha-one", "alpha-two"] {
        fs::write(dir.path().join(name), "").expect("write completion fixture");
    }

    for (prefix, show_all_if_ambiguous, show_all_if_unmodified, expected_line, should_list) in [
        ("al", "on", "off", "alpha-", true),
        ("al", "off", "off", "alpha-", false),
        ("alpha-", "off", "on", "alpha-", true),
        ("alpha-", "off", "off", "alpha-", false),
    ] {
        let inputrc = format!(
            "\"\\C-o\": complete\nset completion-query-items 999\nset page-completions off\nset show-all-if-ambiguous {show_all_if_ambiguous}\nset show-all-if-unmodified {show_all_if_unmodified}"
        );
        let typed = format!("{}/{prefix}", dir.path().display());
        let keys = format!("{typed}\x0f\r");
        let expected = format!("{}/{expected_line}", dir.path().display());
        let bash = run_bash_readline_with_inputrc_file(keys.as_bytes(), &inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys.as_bytes(), &inputrc);

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            assert_eq!(
                accepted_line(output),
                Some(expected.clone()),
                "prefix={prefix:?}, show-all-if-ambiguous={show_all_if_ambiguous}, show-all-if-unmodified={show_all_if_unmodified}, implementation={implementation}\n{output}"
            );
            for candidate in ["alpha-one", "alpha-two"] {
                assert_eq!(
                    output.contains(candidate),
                    should_list,
                    "candidate={candidate:?}, prefix={prefix:?}, show-all-if-ambiguous={show_all_if_ambiguous}, show-all-if-unmodified={show_all_if_unmodified}, implementation={implementation}\n{output}"
                );
            }
        }
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "prefix={prefix:?}, show-all-if-ambiguous={show_all_if_ambiguous}, show-all-if-unmodified={show_all_if_unmodified}\nbash={bash}\nsushline={sushline}"
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

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_list_control_bytes_as_caret_tokens() {
    // Bash 5.3 PTY runtime record 78277e58dc6d49e1a12488aedb710cc2 measures
    // these isolated filename-list displays; this is not source proof.
    let dir = tempfile::tempdir().expect("temp completion directory");
    let fixtures = [
        ("escape", "safe-esc\x1b[2J", "safe-esc^[[2J"),
        ("tab", "safe-tab\t", "safe-tab^I"),
        ("newline", "safe-lf\n", "safe-lf^J"),
        ("delete", "safe-del\x7f", "safe-del^?"),
    ];
    for (_, filename, _) in fixtures {
        fs::write(dir.path().join(filename), "").expect("write control-byte fixture");
    }

    let inputrc = r#""\C-o": possible-filename-completions
set colored-stats off
set visible-stats off
set colored-completion-prefix off
set completion-query-items 999
set page-completions off"#;
    let keys = b"safe-\x0f\r";
    let bash = run_bash_readline_with_inputrc_file_and_env_in_dir(keys, inputrc, &[], dir.path());
    let sushline = run_sushline_harness_with_inputrc_and_env_in_dir(keys, inputrc, &[], dir.path());

    for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
        assert_eq!(
            accepted_line(output).as_deref(),
            Some("safe-"),
            "accepted prefix mismatch: implementation={implementation}"
        );
        assert_eq!(
            bell_count(output),
            0,
            "unexpected completion bell: implementation={implementation}"
        );
        for (kind, _, rendered) in fixtures {
            assert!(
                output.contains(rendered),
                "missing caret-rendered filename: implementation={implementation}, kind={kind}, expected={rendered:?}"
            );
        }
        assert!(
            !output.contains("\x1b[2J"),
            "raw ESC-[2J terminal sequence was emitted: implementation={implementation}"
        );
    }
}

#[cfg(unix)]
fn run_raw_filename_completion_pair(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
    cwd: &std::path::Path,
    report_sushline_accepted_hex: bool,
) -> (Vec<u8>, Vec<u8>) {
    use portable_pty::{CommandBuilder, PtySize};

    let inputrc_path = cwd.join("inputrc");
    fs::write(&inputrc_path, inputrc).expect("write inputrc");
    let mut bash_command = CommandBuilder::new("bash");
    bash_command.env("INPUTRC", inputrc_path.to_string_lossy().as_ref());
    for (name, value) in env {
        bash_command.env(name, value);
    }
    bash_command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "SUSHLINE_READY>" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#,
    ]);

    let mut sushline_command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    sushline_command.env("SUSHLINE_INPUTRC", inputrc);
    sushline_command.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline_command.env("SUSHLINE_HISTORY", "");
    if report_sushline_accepted_hex {
        sushline_command.env("SUSHLINE_ACCEPTED_HEX", "1");
    }
    for (name, value) in env {
        sushline_command.env(name, value);
    }

    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };
    let bash = run_pty_in_dir_with_size_raw(bash_command, keys, cwd, size);
    let sushline = if report_sushline_accepted_hex {
        run_pty_in_dir_with_size_raw_until(
            sushline_command,
            keys,
            cwd,
            size,
            "SUSHLINE_ACCEPTED_HEX:",
        )
    } else {
        run_pty_in_dir_with_size_raw(sushline_command, keys, cwd, size)
    };
    (bash, sushline)
}

#[cfg(unix)]
fn raw_line_after_marker<'a>(output: &'a [u8], marker: &[u8]) -> Option<&'a [u8]> {
    let start = output
        .windows(marker.len())
        .position(|window| window == marker)?
        + marker.len();
    let rest = &output[start..];
    let end = rest
        .iter()
        .position(|byte| matches!(byte, b'\r' | b'\n'))
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

#[cfg(unix)]
fn raw_accepted_line(output: &[u8]) -> Option<&[u8]> {
    raw_line_after_marker(output, b"SUSHLINE_ACCEPTED:")
}

#[cfg(unix)]
fn raw_accepted_hex_marker(output: &[u8]) -> Option<&[u8]> {
    raw_line_after_marker(output, b"SUSHLINE_ACCEPTED_HEX:")
}

#[cfg(unix)]
fn raw_accepted_hex(output: &[u8]) -> Option<Vec<u8>> {
    let hex = raw_accepted_hex_marker(output)?;
    if hex.len() % 2 != 0 {
        return None;
    }
    hex.chunks(2)
        .map(|pair| Some((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?))
        .collect()
}

#[cfg(unix)]
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(unix)]
fn before_accepted_marker(output: &[u8]) -> Option<&[u8]> {
    let marker = b"SUSHLINE_ACCEPTED:";
    let end = output
        .windows(marker.len())
        .position(|window| window == marker)?;
    Some(&output[..end])
}

#[cfg(unix)]
fn raw_bell_count(output: &[u8]) -> usize {
    output.iter().filter(|byte| **byte == b'\x07').count()
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_list_raw_invalid_byte_filename_under_tested_locales() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Prior Bash PTY evidence 6db67d1362dd4ab1a7cac8dd526d566d covered `raw-`
    // plus byte 0xFF without a suffix. This test captures raw bytes for `raw-`
    // plus byte 0xFF and suffix `-item`.
    let dir = tempfile::tempdir().expect("temp completion directory");
    let filename = OsString::from_vec(b"raw-\xff-item".to_vec());
    fs::write(dir.path().join(filename), "").expect("write raw-byte fixture");

    let inputrc = r#""\C-o": possible-filename-completions
set colored-stats off
set visible-stats off
set colored-completion-prefix off
set completion-query-items 999
set page-completions off"#;
    let keys = b"raw-\x0f\r";
    let replacement = b"\xef\xbf\xbd";

    for locale in ["C", "C.UTF-8"] {
        if locale == "C.UTF-8" && !c_utf8_locale_available() {
            continue;
        }
        let (bash, sushline) = run_raw_filename_completion_pair(
            keys,
            inputrc,
            &[("LC_ALL", locale)],
            dir.path(),
            false,
        );

        for (output, implementation) in [(&bash, "Bash"), (&sushline, "Sushline")] {
            assert_eq!(
                raw_accepted_line(output),
                Some(b"raw-".as_slice()),
                "accepted prefix bytes differ for {implementation} under LC_ALL={locale}"
            );
            assert_eq!(
                raw_bell_count(output),
                0,
                "unexpected completion bell for {implementation} under LC_ALL={locale}"
            );
            assert!(
                output
                    .windows(b"raw-\xff-item".len())
                    .any(|window| window == b"raw-\xff-item"),
                "raw filename bytes missing for {implementation} under LC_ALL={locale}"
            );
            assert!(
                !output
                    .windows(replacement.len())
                    .any(|window| window == replacement),
                "UTF-8 replacement bytes present for {implementation} under LC_ALL={locale}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_accepts_exact_raw_invalid_byte_filename_under_tested_locales() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let dir = tempfile::tempdir().expect("temp completion directory");
    fs::write(
        dir.path()
            .join(OsString::from_vec(b"raw-\xff-item".to_vec())),
        "",
    )
    .expect("write raw-byte fixture");
    let inputrc = r#""\C-o": complete-filename"#;
    let keys = b"raw-\x0f\r";
    let expected = b"raw-\xff-item ";
    let expected_hex = b"7261772dff2d6974656d20";

    for locale in ["C", "C.UTF-8"] {
        if locale == "C.UTF-8" && !c_utf8_locale_available() {
            continue;
        }
        let (bash, sushline) = run_raw_filename_completion_pair(
            keys,
            inputrc,
            &[("LC_ALL", locale)],
            dir.path(),
            true,
        );
        let bash_line = raw_accepted_line(&bash);
        let sushline_hex = raw_accepted_hex(&sushline);

        assert_eq!(
            bash_line,
            Some(expected.as_slice()),
            "Bash accepted-line bytes differed under LC_ALL={locale}"
        );
        assert_eq!(
            raw_accepted_hex_marker(&sushline),
            Some(expected_hex.as_slice()),
            "Sushline accepted-line hex marker differed under LC_ALL={locale}"
        );
        assert_eq!(
            sushline_hex.as_deref(),
            bash_line,
            "Bash raw bytes and Sushline accepted-line hex differed under LC_ALL={locale}"
        );
        assert_eq!(
            raw_bell_count(&bash),
            0,
            "unexpected completion bell for Bash under LC_ALL={locale}"
        );
        assert_eq!(
            raw_bell_count(&sushline),
            0,
            "unexpected completion bell for Sushline under LC_ALL={locale}"
        );
    }
}

#[cfg(unix)]
#[test]
fn filename_completion_redraw_shows_raw_invalid_byte_differently_by_locale() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let dir = tempfile::tempdir().expect("temp completion directory");
    fs::write(
        dir.path()
            .join(OsString::from_vec(b"raw-\xff-item".to_vec())),
        "",
    )
    .expect("write raw-byte fixture");
    let inputrc = r#""\C-o": complete-filename"#;
    let keys = b"raw-\x0f\r";
    let replacement = b"\xef\xbf\xbd";

    for locale in ["C", "C.UTF-8"] {
        if locale == "C.UTF-8" && !c_utf8_locale_available() {
            continue;
        }
        let (bash, sushline) = run_raw_filename_completion_pair(
            keys,
            inputrc,
            &[("LC_ALL", locale)],
            dir.path(),
            false,
        );
        let expected_bash: &[u8] = if locale == "C" {
            b"raw-\\377-item "
        } else {
            b"raw-\xff-item "
        };
        for (output, implementation, expected) in [
            (&bash, "Bash", expected_bash),
            (&sushline, "Sushline", b"raw-\xff-item ".as_slice()),
        ] {
            let redraw = before_accepted_marker(output).unwrap_or_else(|| {
                panic!("missing acceptance boundary for {implementation} under LC_ALL={locale}")
            });
            assert!(
                redraw
                    .windows(expected.len())
                    .any(|window| window == expected),
                "expected redraw bytes missing for {implementation} under LC_ALL={locale}"
            );
            assert!(
                !redraw
                    .windows(replacement.len())
                    .any(|window| window == replacement),
                "UTF-8 replacement bytes present before acceptance for {implementation} under LC_ALL={locale}"
            );
            assert_eq!(
                raw_bell_count(redraw),
                0,
                "unexpected completion bell before acceptance for {implementation} under LC_ALL={locale}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_pack_completion_columns_at_width_boundary() {
    use portable_pty::PtySize;

    // Bash 5.3 PTY runtime evidence only: this 100-column fixture checks the
    // exact 20/21 display-width boundary and the two observed row orders.
    fn candidate_rows(output: &str, candidates: &[String]) -> Vec<Vec<String>> {
        let mut positions = candidates
            .iter()
            .map(|candidate| {
                let position = output
                    .find(candidate)
                    .unwrap_or_else(|| panic!("missing {candidate:?} in {output:?}"));
                assert_eq!(
                    output.matches(candidate).count(),
                    1,
                    "candidate {candidate:?} appeared more than once in {output:?}"
                );
                (position, candidate.clone())
            })
            .collect::<Vec<_>>();
        positions.sort_by_key(|(position, _)| *position);

        let mut rows = Vec::<Vec<String>>::new();
        let mut previous_end = None;
        for (position, candidate) in positions {
            if previous_end.is_some_and(|end| output[end..position].contains('\n')) {
                rows.push(Vec::new());
            }
            if rows.is_empty() {
                rows.push(Vec::new());
            }
            rows.last_mut().unwrap().push(candidate.clone());
            previous_end = Some(position + candidate.len());
        }
        rows
    }

    let dir = tempfile::tempdir().expect("temp completion directory");
    let candidates = (1..=12)
        .map(|index| format!("alpha-{index:02}"))
        .collect::<Vec<_>>();
    for candidate in &candidates {
        fs::write(dir.path().join(candidate), "").expect("write completion fixture");
    }
    let size = PtySize {
        rows: 24,
        cols: 100,
        pixel_width: 0,
        pixel_height: 0,
    };

    for width in [20, 21] {
        for horizontal in [true, false] {
            let horizontal_setting = if horizontal { "on" } else { "off" };
            let inputrc = format!(
                "\"\\C-o\": possible-filename-completions\nset completion-query-items 0\nset page-completions off\nset print-completions-horizontally {horizontal_setting}\nset completion-display-width {width}"
            );
            let keys = format!("{}/a\x0f\r", dir.path().display());
            let bash = run_bash_readline_with_inputrc_file_and_env_and_size(
                keys.as_bytes(),
                &inputrc,
                &[],
                size,
            );
            let sushline = run_sushline_harness_with_size(keys.as_bytes(), &inputrc, &[], size);

            let expected = match (width, horizontal) {
                (20, _) => candidates
                    .iter()
                    .map(|candidate| vec![candidate.clone()])
                    .collect::<Vec<_>>(),
                (21, true) => candidates
                    .chunks(2)
                    .map(|row| row.to_vec())
                    .collect::<Vec<_>>(),
                (21, false) => (0..6)
                    .map(|row| vec![candidates[row].clone(), candidates[row + 6].clone()])
                    .collect::<Vec<_>>(),
                _ => unreachable!(),
            };
            for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
                assert_eq!(
                    candidate_rows(output, &candidates),
                    expected,
                    "width={width}, horizontal={horizontal_setting}, implementation={implementation}\n{output}"
                );
                assert!(
                    !output.contains("Display all 12 possibilities?"),
                    "{output}"
                );
                assert!(!output.contains("--More--"), "{output}");
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_query_completion_items_at_measured_boundaries() {
    // Bash 5.3 PTY runtime observations, not Readline source proof: one
    // candidate at threshold 1 does not query, while two at threshold 2 do.
    // Both the default and explicit-on page-completions settings were measured.
    for (candidate_names, threshold, should_query) in [
        (&["alpha"][..], 1, false),
        (&["alpha", "alpine"][..], 2, true),
    ] {
        let dir = tempfile::tempdir().expect("temp completion directory");
        for name in candidate_names {
            fs::write(dir.path().join(name), "").expect("write completion fixture");
        }

        for page_setting in ["", "\nset page-completions on"] {
            let inputrc = format!(
                "\"\\C-o\": possible-filename-completions\nset completion-query-items {threshold}{page_setting}"
            );
            let keys = b"a\x0fn\r";
            let bash =
                run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &[], dir.path());
            let sushline =
                run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &[], dir.path());
            let query = format!("Display all {} possibilities?", candidate_names.len());

            for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
                assert_eq!(
                    output.contains(&query),
                    should_query,
                    "expected query={should_query}, candidates={candidate_names:?}, threshold={threshold}, page-setting={page_setting:?}, implementation={implementation}\n{output}"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_page_filename_completions_at_narrow_terminal_size() {
    use portable_pty::PtySize;

    // Bash 5.3 PTY runtime evidence, not Readline source proof: this fixture
    // and 8x40 terminal show the first seven long, one-column candidates before
    // the pager; `q` stops the list. No general column/layout behavior is claimed.
    let dir = tempfile::tempdir().expect("temp completion directory");
    let candidates = (1..=10)
        .map(|index| format!("alpha-long-completion-item-{index:02}"))
        .collect::<Vec<_>>();
    for candidate in &candidates {
        fs::write(dir.path().join(candidate), "").expect("write completion fixture");
    }

    let size = PtySize {
        rows: 8,
        cols: 40,
        pixel_width: 0,
        pixel_height: 0,
    };
    for (page_completions, expected_count) in [("on", 7), ("off", 10)] {
        let inputrc = format!(
            "\"\\C-o\": possible-filename-completions\nset completion-query-items 0\nset print-completions-horizontally off\nset page-completions {page_completions}"
        );
        let inputrc_path = dir.path().join("inputrc");
        fs::write(&inputrc_path, &inputrc).expect("write inputrc");
        let inputrc_path = inputrc_path.to_str().expect("UTF-8 inputrc path");
        let env = [("INPUTRC", inputrc_path)];
        let keys = format!("{}/al\x0fq\r", dir.path().display());

        let bash = run_bash_readline_with_size_and_env(keys.as_bytes(), "", &[], size, &env);
        let sushline = run_sushline_harness_with_size(keys.as_bytes(), &inputrc, &[], size);
        let pager_visible = page_completions == "on";

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            for (index, candidate) in candidates.iter().enumerate() {
                let expected = if !pager_visible || index < expected_count {
                    1
                } else {
                    0
                };
                assert_eq!(
                    output.matches(candidate).count(),
                    expected,
                    "candidate={candidate:?}, page-completions={page_completions}, implementation={implementation}\n{output}"
                );
            }
            assert_eq!(
                output.contains("--More--"),
                pager_visible,
                "page-completions={page_completions}, implementation={implementation}\n{output}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_abbreviate_normal_and_hidden_completion_prefixes() {
    // Bash 5.3 PTY runtime evidence for these four names and threshold 4 only;
    // this is not Readline source proof or a claim about other names/thresholds.
    let dir = tempfile::tempdir().expect("tempdir");
    for name in [
        "longprefix-alpha",
        "longprefix-beta",
        ".longprefix-alpha",
        ".longprefix-beta",
    ] {
        fs::write(dir.path().join(name), "").expect("write completion fixture");
    }
    let inputrc = r#""\C-o": possible-filename-completions
set completion-query-items 999
set completion-prefix-display-length 4"#;

    for (prefix, candidates) in [
        ("longprefix-", ["...alpha", "...beta"]),
        (".longprefix-", ["...alpha", "...beta"]),
    ] {
        let typed = format!("{}/{prefix}\x0f\r", dir.path().display());
        let bash = run_bash_readline_with_inputrc_file(typed.as_bytes(), inputrc);
        let sushline = run_sushline_harness_with_inputrc(typed.as_bytes(), inputrc);

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            for candidate in candidates {
                assert!(
                    output.contains(candidate),
                    "missing abbreviated candidate {candidate:?}, prefix={prefix:?}, implementation={implementation}\n{output}"
                );
            }
        }
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "prefix={prefix:?}\nbash={bash}\nsushline={sushline}"
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

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_menu_completion_display_prefix_matches_bash_5_3() {
    let dir = tempfile::tempdir().expect("temp completion directory");
    for name in ["alpha-one", "alpine-two"] {
        fs::write(dir.path().join(name), "").expect("write completion fixture");
    }

    // Bash 5.3 PTY runtime observations for this exact fixture and first
    // menu-completion action only; this is not source proof or a general menu
    // semantics claim.
    for (display_prefix, command, expected_line, expected_bells) in [
        ("off", "menu-complete", "alpha-one ", 0),
        ("off", "menu-complete-backward", "alpine-two ", 0),
        ("on", "menu-complete", "alp", 1),
        ("on", "menu-complete-backward", "alp", 1),
    ] {
        let inputrc = format!(
            "\"\\C-o\": {command}\nset menu-complete-display-prefix {display_prefix}\nset page-completions off\nset completion-query-items 999"
        );
        let keys = b"al\x0f\r";
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &[], dir.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &[], dir.path());
        let expected = Some(expected_line.to_string());

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            assert_eq!(
                accepted_line(output),
                expected,
                "display-prefix={display_prefix}, command={command}, implementation={implementation}\n{output}"
            );
            assert_eq!(
                bell_count(output),
                expected_bells,
                "display-prefix={display_prefix}, command={command}, implementation={implementation}\n{output}"
            );
        }
        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "display-prefix={display_prefix}, command={command}\nbash={bash}\nsushline={sushline}"
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "display-prefix={display_prefix}, command={command}\nbash={bash}\nsushline={sushline}"
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

#[cfg(unix)]
fn root_home_is_available() -> bool {
    let Ok(passwd) = fs::read_to_string("/etc/passwd") else {
        return false;
    };
    passwd
        .lines()
        .find_map(|line| {
            let mut fields = line.split(':');
            if fields.next() != Some("root") {
                return None;
            }
            fields
                .nth(4)
                .map(|home| std::path::Path::new(home).is_dir())
        })
        .unwrap_or(false)
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_leave_bare_named_user_filename_completion_unchanged() {
    // Bash 5.3 PTY runtime evidence for this available-account case only:
    // complete-filename leaves the bare named-user word unchanged and rings
    // once with either expand-tilde setting. Other named-user forms are not
    // established here.
    if !root_home_is_available() {
        return;
    }

    let cwd = tempfile::tempdir().expect("temp cwd");
    let home = cwd.path().to_string_lossy().into_owned();
    let env = [("HOME", home.as_str())];
    let keys = b"~root\x0f\r";

    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
        );
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &env, cwd.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &env, cwd.path());

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            assert!(
                accepted_line(output).as_deref() == Some("~root"),
                "expected unchanged named-user word with expand-tilde={expand_tilde}, implementation={implementation}"
            );
            assert!(
                bell_count(output) == 1,
                "expected one completion bell with expand-tilde={expand_tilde}, implementation={implementation}"
            );
        }
    }
}

#[cfg(unix)]
fn current_account_home() -> Option<(String, std::path::PathBuf)> {
    use std::os::unix::ffi::OsStringExt;

    let uid = unsafe { libc::geteuid() };
    let passwd = fs::read("/etc/passwd").ok()?;
    passwd.split(|byte| *byte == b'\n').find_map(|line| {
        let fields = line.split(|byte| *byte == b':').collect::<Vec<_>>();
        if fields.len() < 6
            || std::str::from_utf8(fields[2])
                .ok()?
                .parse::<libc::uid_t>()
                .ok()?
                != uid
        {
            return None;
        }
        let name = std::str::from_utf8(fields[0]).ok()?.to_string();
        let home = std::path::PathBuf::from(std::ffi::OsString::from_vec(fields[5].to_vec()));
        home.is_dir().then_some((name, home))
    })
}

#[cfg(unix)]
fn relative_path_between(
    from: &std::path::Path,
    to: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let from = fs::canonicalize(from).ok()?;
    let to = fs::canonicalize(to).ok()?;
    let from_components = from.components().collect::<Vec<_>>();
    let to_components = to.components().collect::<Vec<_>>();
    let common = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(from, to)| from == to)
        .count();
    let mut relative = std::path::PathBuf::new();
    for _ in common..from_components.len() {
        relative.push("..");
    }
    for component in &to_components[common..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_preserve_empty_suffix_named_user_directory() {
    // Bash 5.3 PTY record 3c1e815457bc47698ef3c163f321d780 observed the
    // unchanged word and one bell. Compare the live implementations without
    // putting account, path, or directory-entry data in failure output.
    let Some((username, _home)) = current_account_home() else {
        return;
    };
    let Ok(cwd) = tempfile::tempdir() else {
        return;
    };
    let word = format!("~{username}/");
    let mut keys = word.as_bytes().to_vec();
    keys.extend_from_slice(b"\x0f\r");

    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
        );
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(&keys, &inputrc, &[], cwd.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(&keys, &inputrc, &[], cwd.path());

        let bash_line = accepted_line(&bash);
        let sushline_line = accepted_line(&sushline);
        assert!(
            bash_line.is_some() && bash_line == sushline_line,
            "Bash and Sushline accepted different empty-suffix named-user paths"
        );
        assert_eq!(
            bell_count(&sushline),
            bell_count(&bash),
            "Bash and Sushline rang a different number of bells for empty-suffix named-user directory completion"
        );
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_complete_named_user_slash_paths_like_bash_5_3() {
    // Bash 5.3 PTY records 7db369e8309c4661960fad56e5f7df07,
    // 86f3726bac3d4f4987f31a98539dc1bb, and e9ff0308b9324d30a18111a07bee1b3b
    // cover only these four named-user slash-path forms and both option values.
    let Some((username, home)) = current_account_home() else {
        return;
    };
    let Ok(fixture) = tempfile::tempdir() else {
        return;
    };
    let Some(relative_path) = relative_path_between(&home, fixture.path()) else {
        return;
    };
    let Ok(relative_path) = relative_path.into_os_string().into_string() else {
        return;
    };
    let nested = fixture.path().join("nested");
    if fs::create_dir(&nested).is_err() || fs::write(nested.join("alpha"), "").is_err() {
        return;
    }
    let routed_fixture = home.join(&relative_path);
    let Ok(resolved_fixture) = fs::canonicalize(&routed_fixture) else {
        return;
    };
    let Ok(actual_fixture) = fs::canonicalize(fixture.path()) else {
        return;
    };
    if resolved_fixture != actual_fixture
        || fs::read_dir(&routed_fixture).is_err()
        || fs::read_dir(routed_fixture.join("nested"))
            .and_then(|entries| entries.collect::<Result<Vec<_>, _>>())
            .is_err()
    {
        return;
    }
    let Ok(expanded_prefix) = routed_fixture
        .join("nested/al")
        .into_os_string()
        .into_string()
    else {
        return;
    };
    let tilde_prefix = format!("~{username}/{relative_path}/nested/al");
    let Ok(cwd) = tempfile::tempdir() else {
        return;
    };

    let cases = [
        (
            "unquoted",
            tilde_prefix.clone(),
            format!("{tilde_prefix}pha "),
            format!("{expanded_prefix}pha "),
            0,
        ),
        (
            "single-quoted",
            format!("'{tilde_prefix}"),
            format!("'{expanded_prefix}pha' "),
            format!("'{expanded_prefix}pha' "),
            0,
        ),
        (
            "double-quoted",
            format!("\"{tilde_prefix}"),
            format!("\"{expanded_prefix}pha\" "),
            format!("\"{expanded_prefix}pha\" "),
            0,
        ),
        (
            "escaped",
            format!("\\{tilde_prefix}"),
            format!("\\{tilde_prefix}"),
            format!("\\{tilde_prefix}"),
            1,
        ),
    ];

    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
        );
        for (form, typed, expected_off, expected_on, expected_bells) in &cases {
            let expected = if *form == "escaped" || (*form == "unquoted" && expand_tilde == "off") {
                expected_off
            } else {
                expected_on
            };
            let mut keys = typed.as_bytes().to_vec();
            keys.extend_from_slice(b"\x0f\r");
            let bash = run_bash_readline_with_inputrc_file_and_env_in_dir(
                &keys,
                &inputrc,
                &[],
                cwd.path(),
            );
            let sushline =
                run_sushline_harness_with_inputrc_and_env_in_dir(&keys, &inputrc, &[], cwd.path());

            for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
                let accepted = accepted_line(output);
                assert!(
                    accepted.as_deref() == Some(expected.as_str()),
                    "accepted-line mismatch for {form}, expand-tilde={expand_tilde}, implementation={implementation}, matched-tilde={}, matched-expanded={}",
                    accepted.as_deref() == Some(expected_off.as_str()),
                    accepted.as_deref() == Some(expected_on.as_str()),
                );
                assert!(
                    bell_count(output) == *expected_bells,
                    "bell-count mismatch for {form}, expand-tilde={expand_tilde}, implementation={implementation}"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_escape_literal_leading_tilde_filename() {
    // Bash 5.3 PTY runtime evidence for this isolated cwd/HOME fixture only:
    // a literal `~` filename is accepted escaped, without a bell, regardless
    // of expand-tilde. This is not source proof or a general tilde claim.
    let cwd = tempfile::tempdir().expect("temp cwd");
    fs::write(cwd.path().join("~"), "").expect("write literal tilde fixture");
    let home = tempfile::tempdir().expect("temp HOME");
    fs::write(home.path().join("Alpha"), "").expect("write HOME fixture");
    let home_path = home.path().to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let keys = b"~\x0f\r";

    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset mark-directories off\nset expand-tilde {expand_tilde}"
        );
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &env, cwd.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &env, cwd.path());

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            assert_eq!(
                accepted_line(output),
                Some("\\~".to_string()),
                "expand-tilde={expand_tilde}, implementation={implementation}\n{output}"
            );
            assert_eq!(
                bell_count(output),
                0,
                "expand-tilde={expand_tilde}, implementation={implementation}\n{output}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_leave_bare_tilde_filename_completion_unchanged() {
    // Bash 5.3 PTY runtime evidence for this isolated HOME/cwd fixture only:
    // complete-filename leaves a bare `~` unchanged and rings once with either
    // expand-tilde setting. This does not establish named-user behavior.
    let cwd = tempfile::tempdir().expect("temp cwd");
    let home = tempfile::tempdir().expect("temp HOME");
    fs::write(home.path().join("Alpha"), "").expect("write HOME fixture");
    let home_path = home.path().to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let keys = b"~\x0f\r";

    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off"
        );
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &env, cwd.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &env, cwd.path());

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            assert_eq!(
                accepted_line(output),
                Some("~".to_string()),
                "expand-tilde={expand_tilde}, implementation={implementation}\n{output}"
            );
            assert_eq!(
                bell_count(output),
                1,
                "expand-tilde={expand_tilde}, implementation={implementation}\n{output}"
            );
        }
    }
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
fn bash_readline_and_sushline_display_visible_filename_stats_for_tested_types() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    // Bash 5.3 PTY runtime evidence (record e5a1d1fc120e49798bd7529883774a92):
    // with possible-filename-completions, mark-directories off, and
    // colored-stats off, visible-stats marks only these tested fixture types.
    let cwd = tempfile::tempdir().expect("temp cwd");
    fs::create_dir(cwd.path().join("alpha-dir")).expect("mkdir directory fixture");
    fs::write(cwd.path().join("alpha-file"), "").expect("write regular-file fixture");
    let executable = cwd.path().join("alpha-exec");
    fs::write(&executable, "").expect("write executable fixture");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
        .expect("chmod executable fixture");
    symlink(cwd.path().join("alpha-file"), cwd.path().join("alpha-link"))
        .expect("symlink to regular-file fixture");

    let keys = b"al\x0f\r";
    let env = [("LS_COLORS", "")];
    let binding = r#""\C-o": possible-filename-completions
set completion-query-items 999
set mark-directories off
set colored-stats off"#;

    for (visible_stats, should_mark) in [(None, false), (Some("off"), false), (Some("on"), true)] {
        let inputrc = visible_stats.map_or_else(
            || binding.to_string(),
            |value| format!("{binding}\nset visible-stats {value}"),
        );
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &env, cwd.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &env, cwd.path());

        for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
            for (name, marker) in [
                ("alpha-dir", Some('/')),
                ("alpha-exec", Some('*')),
                ("alpha-link", Some('@')),
                ("alpha-file", None),
            ] {
                assert!(
                    output.contains(name),
                    "missing candidate {name:?}, implementation={implementation}, visible-stats={visible_stats:?}\n{output}"
                );
                for possible_marker in ['/', '*', '@'] {
                    let rendered = format!("{name}{possible_marker}");
                    assert_eq!(
                        output.contains(&rendered),
                        should_mark && marker == Some(possible_marker),
                        "unexpected marker state for {rendered:?}, implementation={implementation}, visible-stats={visible_stats:?}\n{output}"
                    );
                }
            }
        }
    }
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

    // Bash 5.3 PTY runtime evidence for this regular-file case only: the
    // common prefix is highlighted before the remaining filename keeps fi=31.
    let prefix_dir = tempfile::tempdir().expect("tempdir");
    for name in ["alpha-one", "alpha-two"] {
        fs::write(prefix_dir.path().join(name), "").expect("write prefix fixture");
    }
    let prefix_typed = format!("{}/al\x0f\r", prefix_dir.path().display());
    let prefix_inputrc = "\"\\C-o\": possible-filename-completions\nset completion-query-items 999\nset colored-stats on\nset colored-completion-prefix on";
    let prefix_env = [("LS_COLORS", "fi=31")];
    let bash = run_bash_readline_with_inputrc_file_and_env(
        prefix_typed.as_bytes(),
        prefix_inputrc,
        &prefix_env,
    );
    let sushline = run_sushline_harness_with_inputrc_and_env(
        prefix_typed.as_bytes(),
        prefix_inputrc,
        &prefix_env,
    );
    for (output, prefix_code) in [(&bash, "01;35"), (&sushline, "1")] {
        assert!(
            output.contains(&format!("\x1b[{prefix_code}malpha-\x1b[0m")),
            "missing highlighted full shared prefix in {output:?}"
        );
        for suffix in ["one", "two"] {
            assert!(
                output.contains(&format!("\x1b[31m{suffix}\x1b[0m")),
                "file type color must remain on suffix {suffix:?}: {output:?}"
            );
        }
    }
    assert!(
        !sushline.contains("\x1b[31malpha-"),
        "file style must not override prefix highlight: {sushline:?}"
    );

    // Runtime-only Bash 5.3 oracle for this exact regular-file duplicate: the
    // final fi assignment wins; no other duplicate token is covered here.
    let duplicate_colors = "fi=31:fi=32";
    let (bash, sushline) = run_pair(typed.as_bytes(), &enabled, Some(duplicate_colors));
    for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
        assert_color(output, "alpha-file", "32", "");
        assert!(
            !output.contains("\x1b[31malpha-file\x1b[0m"),
            "earlier fi assignment unexpectedly won ({implementation}): {output:?}"
        );
    }

    // Runtime-only Bash 5.3 oracle for a symlink to this regular-file fixture:
    // ln=target takes the target's tested fi style, without emitting `target`.
    let (bash, sushline) = run_pair(typed.as_bytes(), &enabled, Some("ln=target:fi=31"));
    for (output, implementation) in [(&bash, "bash"), (&sushline, "sushline")] {
        assert_color(output, "alpha-link", "31", "");
        assert!(
            !output.contains("\x1b[targetm"),
            "LS_COLORS target token leaked into SGR ({implementation}): {output:?}"
        );
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
