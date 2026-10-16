use super::pty::*;
use std::fs;

#[cfg(unix)]
#[test]
fn empty_initial_single_quote_prefix_matches_bash_5_3() {
    // Keep the inputrc file outside cwd so it cannot affect the empty-prefix matches.
    // X marks the insertion point; these are fixture-scoped Bash PTY observations.
    let root = tempfile::tempdir().expect("temp completion root");
    let home = root.path().join("home");
    fs::create_dir(&home).expect("create fixture HOME");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let fixtures: [(&str, &[&str], &[u8], usize); 4] = [
        ("safe-prefix", &["safe-one", "safe-two"], b"echo 'safe-X", 1),
        (
            "eagle-prefix",
            &["eagle-one", "eagle-two"],
            b"echo 'eagle-X",
            1,
        ),
        ("unique-match", &["safe-one"], b"echo 'safe-one' X", 0),
        ("no-common-prefix", &["safe-one", "tall-two"], b"echo 'X", 1),
    ];
    for (fixture, names, expected, expected_bells) in fixtures {
        let cwd = root.path().join(fixture);
        fs::create_dir(&cwd).expect("create fixture cwd");
        for name in names.iter().copied() {
            fs::write(cwd.join(name), b"").expect("write fixture file");
        }
        for command in ["complete", "complete-filename"] {
            for skip_completed_text in ["off", "on"] {
                let inputrc = format!(
                    "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
                );
                let keys = b"echo '\x0fX\r";
                let bash =
                    run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &env, &cwd);
                let sushline =
                    run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &env, &cwd);
                let context =
                    format!("{fixture}, {command}, skip-completed-text={skip_completed_text}");
                let expected = Some(String::from_utf8(expected.to_vec()).unwrap());
                assert_eq!(accepted_line(&bash), expected, "Bash line for {context}");
                assert_eq!(
                    accepted_line(&sushline),
                    expected,
                    "Sushline line for {context}"
                );
                assert_eq!(
                    bell_count(&bash),
                    expected_bells,
                    "Bash bells for {context}"
                );
                assert_eq!(
                    bell_count(&sushline),
                    expected_bells,
                    "Sushline bells for {context}"
                );
            }
        }
    }
}

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

#[cfg(unix)]
#[test]
fn bash_directory_fallback_options_use_reopened_segment_scope() {
    // Bash 5.3 PTY runtime observations for these isolated fixtures only.
    let root = tempfile::tempdir().expect("temp completion root");
    let cases: [(&str, &str, &[&str], &str); 3] = [
        (
            "default-plusdirs",
            "-o default -o plusdirs",
            &["safe-one", "safe-two", "eagle-one", "eagle-two", "safe-dir"],
            "echo 'sa'f'eagle-X",
        ),
        (
            "dirnames",
            "-o dirnames",
            &["safe-one", "safe-two", "eagle-dir-one", "eagle-dir-two"],
            "echo 'sa'f'eagle-dir-X",
        ),
        (
            "plusdirs-only",
            "-o plusdirs",
            &["eagle-dir-one", "eagle-dir-two", "safe-dir-one"],
            "echo 'sa'f'eagle-dir-X",
        ),
    ];
    let inputrc =
        "set show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off";
    for (kind, options, names, expected) in cases {
        let cwd = root.path().join(kind);
        fs::create_dir(&cwd).expect("create cwd");
        for name in names.iter().copied() {
            let path = cwd.join(name);
            if name.contains("-dir-") || name.ends_with("-dir") {
                fs::create_dir(&path).expect("create directory fixture");
            } else {
                fs::write(&path, b"").expect("write file fixture");
            }
        }
        let bash = run_bash_completion_in_dir(b"echo 'sa'f'e\tX\r", inputrc, options, &cwd);
        assert_eq!(
            accepted_line(&bash),
            Some(expected.to_string()),
            "option={options}\n{bash}"
        );
        assert_eq!(bell_count(&bash), 1, "option={options}\n{bash}");
    }
}

#[cfg(unix)]
#[test]
fn bash_default_filename_fallback_uses_reopened_segment_scope() {
    // Bash 5.3 PTY runtime evidence for the exact programmable-completion
    // fallback: callbacks return no candidates and `-o default` completes the
    // reopened `e` segment. Keep fixtures isolated to this temporary cwd.
    let dir = tempfile::tempdir().expect("temp completion directory");
    for name in ["safe-one", "safe-two", "eagle-one", "eagle-two"] {
        fs::write(dir.path().join(name), "").expect("write completion fixture");
    }
    let inputrc =
        "set show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off";
    let bash = run_bash_completion_in_dir(b"echo 'sa'f'e\tX\r", inputrc, "-o default", dir.path());

    assert_eq!(
        accepted_line(&bash),
        Some("echo 'sa'f'eagle-X".to_string()),
        "{bash}"
    );
    assert_eq!(bell_count(&bash), 1, "{bash}");
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
#[test]
fn complete_filename_quotes_line_feed_filenames_like_bash_5_3() {
    use portable_pty::{CommandBuilder, PtySize};
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    // Runtime-only Bash 5.3 PTY probes fdb5a964e24d4ad6bc6313421cce0fec,
    // 78931adcb4c944b1932c17fa0b16ee91, c392ccb26daf46fcb818547056f093d7,
    // and f7ff7e453eaa4d968210c733ebac70ea under LC_ALL=C. Capture the live
    // READLINE_LINE via bind -x because `read` truncates at embedded LF.
    let cwd = tempfile::tempdir().expect("temp completion directory");
    let home = tempfile::tempdir().expect("temp HOME directory");
    let basename = b"safe-lf\nitem";
    fs::write(cwd.path().join(OsString::from_vec(basename.to_vec())), "")
        .expect("write cwd LF fixture");
    fs::write(home.path().join(OsString::from_vec(basename.to_vec())), "")
        .expect("write HOME LF fixture");
    let home_path = home.path().to_string_lossy().into_owned();
    let inputrc_path = cwd.path().join("inputrc");
    let bash_script = r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; bind -x '"\C-g": printf "BASH_READLINE_HEX:"; printf "%s" "$READLINE_LINE" | od -An -tx1 | tr -d " \n"; printf "\n"'; IFS= read -r -e -p "SUSHLINE_READY>" line; :"#;

    let run_bash = |keys: &[u8], inputrc: &str| {
        fs::write(&inputrc_path, inputrc).expect("write inputrc");
        let mut command = CommandBuilder::new("bash");
        command.env("INPUTRC", inputrc_path.to_string_lossy().as_ref());
        command.env("LC_ALL", "C");
        command.env("HOME", &home_path);
        command.args(["--noprofile", "--norc", "-i", "-c", bash_script]);
        run_pty_in_dir_with_size_raw_until(
            command,
            keys,
            cwd.path(),
            PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            },
            "BASH_READLINE_HEX:",
        )
    };
    let run_sushline = |keys: &[u8], inputrc: &str| {
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
        command.env("SUSHLINE_INPUTRC", inputrc);
        command.env("SUSHLINE_PROMPT", READY_PROMPT);
        command.env("SUSHLINE_HISTORY", "");
        command.env("SUSHLINE_ACCEPTED_HEX", "1");
        command.env("LC_ALL", "C");
        command.env("HOME", &home_path);
        run_pty_in_dir_with_size_raw_until(
            command,
            keys,
            cwd.path(),
            PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            },
            "SUSHLINE_ACCEPTED_HEX:",
        )
    };
    let assert_line =
        |case: &str, bash_keys: &[u8], sushline_keys: &[u8], inputrc: &str, expected: &[u8]| {
            let bash = run_bash(bash_keys, inputrc);
            let sushline = run_sushline(sushline_keys, inputrc);
            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:");
            let sushline_hex = raw_accepted_hex_marker(&sushline);
            let sushline_bytes = raw_accepted_hex(&sushline);

            assert!(
                bash_hex.is_some() && bash_hex == sushline_hex,
                "Bash and Sushline line-buffer hex differed for {case}"
            );
            assert!(
                sushline_bytes.as_deref() == Some(expected),
                "accepted filename bytes differed for {case}"
            );
            assert!(
                raw_bell_count(&bash) == 0 && raw_bell_count(&sushline) == 0,
                "unexpected completion bell for {case}"
            );
        };

    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;
    assert_line(
        "cwd filename",
        b"safe-\x0f\x07\r",
        b"safe-\x0f\r",
        inputrc,
        b"'safe-lf\nitem' ",
    );

    let home_bytes = home.path().as_os_str().as_bytes();
    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
        );
        for (form, prefix) in [
            ("unquoted", b"~/safe-".as_slice()),
            ("single-quoted", b"'~/safe-".as_slice()),
            ("double-quoted", b"\"~/safe-".as_slice()),
        ] {
            let mut bash_keys = prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let expected = if expand_tilde == "off" && form == "unquoted" {
                b"~/safe-lf\\\nitem ".to_vec()
            } else {
                let quote = if form == "double-quoted" { b'"' } else { b'\'' };
                let mut expected = vec![quote];
                expected.extend_from_slice(home_bytes);
                expected.push(b'/');
                expected.extend_from_slice(basename);
                expected.push(quote);
                expected.push(b' ');
                expected
            };
            let case = format!("HOME {form}, expand-tilde={expand_tilde}");
            assert_line(&case, &bash_keys, &sushline_keys, &inputrc, &expected);
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_quotes_backslashes_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probes b8620c97f7a64e82bf8f948d3b846943 and
    // 14d0d010a29d42b2a1b2b3dc96a6eddb under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "internal",
            b"safe-back\\slash".as_slice(),
            [
                b"safe-back\\\\slash ".as_slice(),
                b"'safe-back\\slash' ".as_slice(),
                b"\"safe-back\\\\slash\" ".as_slice(),
            ],
        ),
        (
            "trailing",
            b"safe-trail\\".as_slice(),
            [
                b"safe-trail\\\\ ".as_slice(),
                b"'safe-trail\\' ".as_slice(),
                b"\"safe-trail\\\\\" ".as_slice(),
            ],
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, expected) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create backslash fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write backslash fixture");
        for ((form, prefix), expected) in [
            ("unquoted", b"safe-".as_slice()),
            ("single-quoted", b"'safe-".as_slice()),
            ("double-quoted", b"\"safe-".as_slice()),
        ]
        .into_iter()
        .zip(expected)
        {
            let mut bash_keys = prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) =
                run_filename_line_buffer_hex_pair(&dir, inputrc, &bash_keys, &sushline_keys, "");
            let context = format!("{fixture}/{form} backslash completion");
            assert_filename_line_buffer_bytes(&bash, &sushline, expected, &context);
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_quotes_apostrophes_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probes 0f01ce0d8770412c8614f3529b122810 and
    // 1f699a78d2094d90ab8c57f6eb42e9a7 under LC_ALL=C.
    let dir = tempfile::tempdir().expect("temp completion directory");
    fs::write(dir.path().join("safe-o'clock"), "").expect("write apostrophe fixture");
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (form, prefix, expected) in [
        (
            "unquoted",
            b"safe-".as_slice(),
            b"safe-o\\'clock ".as_slice(),
        ),
        (
            "single-quoted",
            b"'safe-".as_slice(),
            b"'safe-o'\\''clock' ".as_slice(),
        ),
        (
            "double-quoted",
            b"\"safe-".as_slice(),
            b"\"safe-o'clock\" ".as_slice(),
        ),
    ] {
        let mut bash_keys = prefix.to_vec();
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        let mut sushline_keys = prefix.to_vec();
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) =
            run_filename_line_buffer_hex_pair(dir.path(), inputrc, &bash_keys, &sushline_keys, "");
        let context = format!("{form} apostrophe completion");
        assert_filename_line_buffer_bytes(&bash, &sushline, expected, &context);
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_preserves_double_quoted_expansion_chars_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probes ec3edb033ef8468b9a40853b149d0fbc,
    // 71471b23cb424d6dad8c7492e124602c, 0bd483637223455c9391b038ea5ed5fe,
    // and 7876ffb301424d8b9cc2172a7faa0b1b under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "dollar-internal",
            b"safe-dollar$value".as_slice(),
            [
                b"safe-dollar\\$value ".as_slice(),
                b"'safe-dollar$value' ".as_slice(),
                b"\"safe-dollar$value\" ".as_slice(),
            ],
        ),
        (
            "dollar-trailing",
            b"safe-trail$".as_slice(),
            [
                b"safe-trail\\$ ".as_slice(),
                b"'safe-trail$' ".as_slice(),
                b"\"safe-trail$\" ".as_slice(),
            ],
        ),
        (
            "backtick-internal",
            b"safe-back`tick".as_slice(),
            [
                b"safe-back\\`tick ".as_slice(),
                b"'safe-back`tick' ".as_slice(),
                b"\"safe-back`tick\" ".as_slice(),
            ],
        ),
        (
            "backtick-trailing",
            b"safe-trail`".as_slice(),
            [
                b"safe-trail\\` ".as_slice(),
                b"'safe-trail`' ".as_slice(),
                b"\"safe-trail`\" ".as_slice(),
            ],
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, expected) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create expansion-character fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write expansion-character fixture");
        for ((form, prefix), expected) in [
            ("unquoted", b"safe-".as_slice()),
            ("single-quoted", b"'safe-".as_slice()),
            ("double-quoted", b"\"safe-".as_slice()),
        ]
        .into_iter()
        .zip(expected)
        {
            let mut bash_keys = prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) =
                run_filename_line_buffer_hex_pair(&dir, inputrc, &bash_keys, &sushline_keys, "");
            let context = format!("{fixture}/{form} completion");
            assert_filename_line_buffer_bytes(&bash, &sushline, expected, &context);
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_quotes_shell_operator_characters_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probe d1ae9790e3794a9993c5ae9f93f40f5c under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "semicolon-internal",
            b"safe-semi-;item".as_slice(),
            b"safe-semi-\\;item ".as_slice(),
        ),
        (
            "semicolon-trailing",
            b"safe-trail;".as_slice(),
            b"safe-trail\\; ".as_slice(),
        ),
        (
            "ampersand-internal",
            b"safe-amp-&item".as_slice(),
            b"safe-amp-\\&item ".as_slice(),
        ),
        (
            "ampersand-trailing",
            b"safe-trail&".as_slice(),
            b"safe-trail\\& ".as_slice(),
        ),
        (
            "pipe-internal",
            b"safe-pipe-|item".as_slice(),
            b"safe-pipe-\\|item ".as_slice(),
        ),
        (
            "pipe-trailing",
            b"safe-trail|".as_slice(),
            b"safe-trail\\| ".as_slice(),
        ),
        (
            "less-internal",
            b"safe-less-<item".as_slice(),
            b"safe-less-\\<item ".as_slice(),
        ),
        (
            "less-trailing",
            b"safe-trail<".as_slice(),
            b"safe-trail\\< ".as_slice(),
        ),
        (
            "greater-internal",
            b"safe-greater->item".as_slice(),
            b"safe-greater-\\>item ".as_slice(),
        ),
        (
            "greater-trailing",
            b"safe-trail>".as_slice(),
            b"safe-trail\\> ".as_slice(),
        ),
        (
            "open-paren-internal",
            b"safe-open-(item".as_slice(),
            b"safe-open-\\(item ".as_slice(),
        ),
        (
            "open-paren-trailing",
            b"safe-trail(".as_slice(),
            b"safe-trail\\( ".as_slice(),
        ),
        (
            "close-paren-internal",
            b"safe-close-)item".as_slice(),
            b"safe-close-\\)item ".as_slice(),
        ),
        (
            "close-paren-trailing",
            b"safe-trail)".as_slice(),
            b"safe-trail\\) ".as_slice(),
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, expected_unquoted) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create shell-operator fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write shell-operator fixture");
        for (form, prefix, quote) in [
            ("unquoted", b"safe-".as_slice(), None),
            ("single-quoted", b"'safe-".as_slice(), Some(b'\'')),
            ("double-quoted", b"\"safe-".as_slice(), Some(b'"')),
        ] {
            let mut expected = Vec::new();
            if let Some(quote) = quote {
                expected.push(quote);
                expected.extend_from_slice(basename);
                expected.push(quote);
                expected.push(b' ');
            } else {
                expected.extend_from_slice(expected_unquoted);
            }
            let mut bash_keys = prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) =
                run_filename_line_buffer_hex_pair(&dir, inputrc, &bash_keys, &sushline_keys, "");
            let context = format!("{fixture}/{form} completion");
            assert_filename_line_buffer_bytes(&bash, &sushline, &expected, &context);
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_quotes_brace_expansion_characters_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probes 88af856c22964c71ad647ce4a603d87b,
    // 6cd608b17a944ff7a450044b226085c3, dcf436468d2144c0b8fb0c9dbe3f6524,
    // and 123a10571d184bfd9907dc04c198dd89 under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "open-internal",
            b"safe-left{item".as_slice(),
            b"safe-left\\{item ".as_slice(),
        ),
        (
            "open-trailing",
            b"safe-trail{".as_slice(),
            b"safe-trail\\{ ".as_slice(),
        ),
        (
            "close-internal",
            b"safe-right}item".as_slice(),
            b"safe-right}item ".as_slice(),
        ),
        (
            "close-trailing",
            b"safe-trail}".as_slice(),
            b"safe-trail} ".as_slice(),
        ),
        (
            "brace-list",
            b"safe-pair{a,b}".as_slice(),
            b"safe-pair\\{a\\,b\\} ".as_slice(),
        ),
        (
            "brace-range",
            b"safe-range{1..3}".as_slice(),
            b"safe-range\\{1..3\\} ".as_slice(),
        ),
        (
            "brace-empty",
            b"safe-empty{}".as_slice(),
            b"safe-empty\\{\\} ".as_slice(),
        ),
        (
            "brace-unmatched-open",
            b"safe-open{a,b".as_slice(),
            b"safe-open\\{a\\,b ".as_slice(),
        ),
        (
            "close-before-open",
            b"safe-rev}a,b{".as_slice(),
            b"safe-rev\\}a\\,b\\{ ".as_slice(),
        ),
        (
            "comma-without-brace",
            b"safe-a,b".as_slice(),
            b"safe-a,b ".as_slice(),
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, expected_unquoted) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create brace fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write brace fixture");
        for (form, prefix, quote) in [
            ("unquoted", b"safe-".as_slice(), None),
            ("single-quoted", b"'safe-".as_slice(), Some(b'\'')),
            ("double-quoted", b"\"safe-".as_slice(), Some(b'"')),
        ] {
            let mut expected = Vec::new();
            if let Some(quote) = quote {
                expected.push(quote);
                expected.extend_from_slice(basename);
                expected.push(quote);
                expected.push(b' ');
            } else {
                expected.extend_from_slice(expected_unquoted);
            }
            let mut bash_keys = prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) =
                run_filename_line_buffer_hex_pair(&dir, inputrc, &bash_keys, &sushline_keys, "");
            let context = format!("{fixture}/{form} completion");
            assert_filename_line_buffer_bytes(&bash, &sushline, &expected, &context);
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_quotes_glob_characters_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probes f1b3cb92aaf149018ad0d6d0986ee52a,
    // 2f13e689917b4fac8e4310b647fdfb94, af1d4a4c0484459e9106ae06e0a1a2ef,
    // and e6baaf57f6e643a9a5e5aed8ef1e1bf3 under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "star-internal",
            b"safe-star*item".as_slice(),
            b"safe-star\\*item ".as_slice(),
        ),
        (
            "star-trailing",
            b"safe-trail*".as_slice(),
            b"safe-trail\\* ".as_slice(),
        ),
        (
            "question-internal",
            b"safe-q?item".as_slice(),
            b"safe-q\\?item ".as_slice(),
        ),
        (
            "question-trailing",
            b"safe-trail?".as_slice(),
            b"safe-trail\\? ".as_slice(),
        ),
        (
            "open-bracket-internal",
            b"safe-left[item".as_slice(),
            b"safe-left\\[item ".as_slice(),
        ),
        (
            "open-bracket-trailing",
            b"safe-trail[".as_slice(),
            b"safe-trail\\[ ".as_slice(),
        ),
        (
            "close-bracket-internal",
            b"safe-right]item".as_slice(),
            b"safe-right]item ".as_slice(),
        ),
        (
            "close-bracket-trailing",
            b"safe-trail]".as_slice(),
            b"safe-trail] ".as_slice(),
        ),
        (
            "bracket-pair",
            b"safe-set[abc]".as_slice(),
            b"safe-set\\[abc\\] ".as_slice(),
        ),
        (
            "close-before-open",
            b"safe-rev]open[".as_slice(),
            b"safe-rev\\]open\\[ ".as_slice(),
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, expected_unquoted) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create glob fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "").expect("write glob fixture");
        for (form, prefix, quote) in [
            ("unquoted", b"safe-".as_slice(), None),
            ("single-quoted", b"'safe-".as_slice(), Some(b'\'')),
            ("double-quoted", b"\"safe-".as_slice(), Some(b'"')),
        ] {
            let mut expected = Vec::new();
            if let Some(quote) = quote {
                expected.push(quote);
                expected.extend_from_slice(basename);
                expected.push(quote);
                expected.push(b' ');
            } else {
                expected.extend_from_slice(expected_unquoted);
            }
            let mut bash_keys = prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) =
                run_filename_line_buffer_hex_pair(&dir, inputrc, &bash_keys, &sushline_keys, "");
            let context = format!("{fixture}/{form} completion");
            assert_filename_line_buffer_bytes(&bash, &sushline, &expected, &context);
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_escapes_a_word_leading_hash_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probes b4efd8c1202d40f4967822ed71c1965c and
    // cafd32cffb0747fb81bac4695c327947 under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let dir = root.path();
    let basename = b"#safe-item";
    fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
        .expect("write leading-hash fixture");
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;
    let cases = [
        (
            "word-start",
            b"#safe-".as_slice(),
            b"\\#safe-item ".as_slice(),
        ),
        (
            "after-space",
            b"echo #safe-".as_slice(),
            b"echo \\#safe-item ".as_slice(),
        ),
        (
            "single-quoted",
            b"'#safe-".as_slice(),
            b"'#safe-item' ".as_slice(),
        ),
        (
            "double-quoted",
            b"\"#safe-".as_slice(),
            b"\"#safe-item\" ".as_slice(),
        ),
    ];

    for (context, prefix, expected) in cases {
        let mut bash_keys = prefix.to_vec();
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        let mut sushline_keys = prefix.to_vec();
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) =
            run_filename_line_buffer_hex_pair(dir, inputrc, &bash_keys, &sushline_keys, "");
        assert_filename_line_buffer_bytes(&bash, &sushline, expected, context);
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_preserves_internal_and_trailing_hashes_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probes fa06c41b591c4d5a838d0d59c6450f3c and
    // ad800e48a26e47e59e276fae761e86b3 under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        ("internal", b"safe-hash#item".as_slice()),
        ("trailing", b"safe-trail#".as_slice()),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create hash fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "").expect("write hash fixture");
        for (form, prefix, quote) in [
            ("unquoted", b"safe-".as_slice(), None),
            ("single-quoted", b"'safe-".as_slice(), Some(b'\'')),
            ("double-quoted", b"\"safe-".as_slice(), Some(b'"')),
        ] {
            let mut expected = Vec::new();
            if let Some(quote) = quote {
                expected.push(quote);
            }
            expected.extend_from_slice(basename);
            if let Some(quote) = quote {
                expected.push(quote);
            }
            expected.push(b' ');
            let mut bash_keys = prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) =
                run_filename_line_buffer_hex_pair(&dir, inputrc, &bash_keys, &sushline_keys, "");
            let context = format!("{fixture}/{form} completion");
            assert_filename_line_buffer_bytes(&bash, &sushline, &expected, &context);
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_handles_ansi_c_bangs_with_bash_history_expansion() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probe b9f43c4a6db740e6aa98a8d40b72e1be.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        ("internal", b"safe-bang!item".as_slice()),
        ("trailing", b"safe-trail!".as_slice()),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create ANSI-C history fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write ANSI-C history fixture");
        for history in ["on", "off"] {
            let bash_setup = if history == "on" { "set -H" } else { "set +H" };
            let mut expected = b"echo $'".to_vec();
            expected.extend_from_slice(basename);
            expected.extend_from_slice(b"' ");
            let bash_keys = b"echo $'safe-\x0f\x07\r";
            let sushline_keys = b"echo $'safe-\x0f\r";
            let (bash, sushline) = run_filename_line_buffer_hex_pair(
                &dir,
                inputrc,
                bash_keys,
                sushline_keys,
                bash_setup,
            );
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                &expected,
                &format!("ANSI-C {fixture}, history expansion {history}"),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_expands_ansi_c_home_tilde_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probe 3eae40cdd3cb4f9db4d1fad2a7e4cc1b.
    use std::os::unix::ffi::OsStrExt;

    let root = tempfile::tempdir().expect("temp completion root");
    let home = root.path().join("home");
    let cwd = root.path().join("cwd");
    fs::create_dir(&home).expect("create isolated HOME");
    fs::create_dir(&cwd).expect("create isolated cwd");
    fs::write(home.join("safe-home-file"), "").expect("write HOME fixture");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let expected = [
        b"echo $'".as_slice(),
        home.as_os_str().as_bytes(),
        b"/safe-home-file' ",
    ]
    .concat();

    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
        );
        let bash_keys = b"echo $'~/safe-\x0f\x07\r";
        let sushline_keys = b"echo $'~/safe-\x0f\r";
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            &inputrc,
            bash_keys,
            sushline_keys,
            "set +H",
            &env,
        );

        assert_filename_line_buffer_bytes(
            &bash,
            &sushline,
            &expected,
            &format!("ANSI-C quoted HOME tilde with expand-tilde {expand_tilde}"),
        );
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_expands_ansi_c_special_directory_tildes_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probe 7c66e05154d340bdb0fb6c5ba20cd7a5.
    use std::os::unix::ffi::OsStrExt;

    let root = tempfile::tempdir().expect("temp completion root");
    let home = root.path().join("home");
    let cwd = root.path().join("current");
    let oldpwd = root.path().join("previous");
    for directory in [&home, &cwd, &oldpwd] {
        fs::create_dir(directory).expect("create isolated tilde fixture directory");
    }
    for (directory, filename) in [(&cwd, "safe-plus-file"), (&oldpwd, "safe-minus-file")] {
        fs::write(directory.join(filename), "").expect("write tilde completion fixture");
    }
    let home_path = home.to_string_lossy().into_owned();
    let cwd_path = cwd.to_string_lossy().into_owned();
    let oldpwd_path = oldpwd.to_string_lossy().into_owned();
    let env = [
        ("HOME", home_path.as_str()),
        ("PWD", cwd_path.as_str()),
        ("OLDPWD", oldpwd_path.as_str()),
    ];

    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
        );
        for (form, directory, filename) in [
            ("+", &cwd, "safe-plus-file"),
            ("-", &oldpwd, "safe-minus-file"),
        ] {
            let bash_keys = format!("echo $'~{form}/safe-\x0f\x07\r");
            let sushline_keys = format!("echo $'~{form}/safe-\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                bash_keys.as_bytes(),
                sushline_keys.as_bytes(),
                "set +H",
                &env,
            );
            let expected = [
                b"echo $'".as_slice(),
                directory.as_os_str().as_bytes(),
                b"/".as_slice(),
                filename.as_bytes(),
                b"' ".as_slice(),
            ]
            .concat();

            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                &expected,
                &format!("ANSI-C quoted ~{form}/ with expand-tilde {expand_tilde}"),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_expands_ansi_c_named_user_path_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probe d1b1ab02ccaa44a3b5c355868e1e5584.
    use std::os::unix::ffi::OsStrExt;

    let Some((username, home)) = current_account_home() else {
        return;
    };
    let Ok(fixture) = tempfile::tempdir() else {
        return;
    };
    let Some(relative_path) = relative_path_between(&home, fixture.path()) else {
        return;
    };
    let relative_bytes = relative_path.as_os_str().as_bytes();
    let username_bytes = username.as_bytes();
    let home_bytes = home.as_os_str().as_bytes();
    let safe_ansi_c_bytes = |bytes: &[u8]| {
        !bytes
            .iter()
            .any(|byte| matches!(*byte, b'\'' | b'\\') || byte.is_ascii_control())
    };
    if !safe_ansi_c_bytes(relative_bytes)
        || !safe_ansi_c_bytes(username_bytes)
        || !safe_ansi_c_bytes(home_bytes)
    {
        return;
    }

    fs::write(fixture.path().join("safe-named-file"), "").expect("write named-user fixture");
    let routed_fixture = home.join(&relative_path);
    if fs::canonicalize(&routed_fixture).ok().as_deref()
        != fs::canonicalize(fixture.path()).ok().as_deref()
        || fs::read_dir(&routed_fixture).is_err()
    {
        return;
    }
    let cwd = tempfile::tempdir().expect("temp cwd");
    let isolated_home = tempfile::tempdir().expect("temp isolated HOME");
    let isolated_home_path = isolated_home.path().to_string_lossy().into_owned();
    let env = [("HOME", isolated_home_path.as_str())];
    let mut prefix = b"echo $'~".to_vec();
    prefix.extend_from_slice(username_bytes);
    prefix.push(b'/');
    prefix.extend_from_slice(relative_bytes);
    prefix.extend_from_slice(b"/safe-");

    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
        );
        let mut bash_keys = prefix.clone();
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        let mut sushline_keys = prefix.clone();
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            cwd.path(),
            &inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let expected = [
            b"echo $'".as_slice(),
            home_bytes,
            b"/".as_slice(),
            relative_bytes,
            b"/safe-named-file' ".as_slice(),
        ]
        .concat();

        assert_filename_line_buffer_bytes(
            &bash,
            &sushline,
            &expected,
            &format!("ANSI-C named-user path with expand-tilde {expand_tilde}"),
        );
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_handles_escaped_and_literal_ansi_c_tildes_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probes 71687d476b6d4639b08a2c3c0814bcd0,
    // c81c921d3256408780d8baa41f9255fd, and eaa40ffcfb354dd68f561294d9ffc448.
    let root = tempfile::tempdir().expect("temp completion root");
    #[derive(Clone, Copy)]
    struct Fixture {
        name: &'static str,
        prefix: &'static [u8],
        directory: &'static str,
        basename: &'static str,
        expected: &'static [u8],
        has_candidate: bool,
        target_home: bool,
    }
    let cases = [
        Fixture {
            name: "literal-basename",
            prefix: b"echo $'~safe-",
            directory: "",
            basename: "~safe-literal-file",
            expected: b"echo $'~safe-literal-file' ",
            has_candidate: true,
            target_home: false,
        },
        Fixture {
            name: "escaped-basename",
            prefix: br"echo $'\~safe-",
            directory: "",
            basename: r"\~safe-escaped-file",
            expected: br"echo $'\~safe-escaped-file' ",
            has_candidate: true,
            target_home: false,
        },
        Fixture {
            name: "escaped-directory",
            prefix: br"echo $'\~/safe-",
            directory: r"\~",
            basename: "safe-escaped-directory-file",
            expected: br"echo $'\~/safe-escaped-directory-file' ",
            has_candidate: true,
            target_home: false,
        },
        Fixture {
            name: "literal-directory",
            prefix: b"echo $'~/safe-",
            directory: "~",
            basename: "safe-literal-directory-file",
            expected: b"echo $'~/safe-",
            has_candidate: false,
            target_home: false,
        },
        Fixture {
            name: "hex-tilde-escape",
            prefix: br"echo $'\x7e/safe-",
            directory: "",
            basename: "safe-home-file",
            expected: br"echo $'\x7e/safe-",
            has_candidate: false,
            target_home: true,
        },
        Fixture {
            name: "octal-tilde-escape",
            prefix: br"echo $'\176/safe-",
            directory: "",
            basename: "safe-home-file",
            expected: br"echo $'\176/safe-",
            has_candidate: false,
            target_home: true,
        },
    ];

    for expand_tilde in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
        );
        for fixture in cases {
            let scenario = root.path().join(fixture.name);
            let cwd = scenario.join("cwd");
            let home = scenario.join("home");
            fs::create_dir_all(&cwd).expect("create fixture cwd");
            fs::create_dir_all(&home).expect("create fixture HOME");
            let target = if fixture.target_home {
                home.clone()
            } else {
                cwd.join(fixture.directory)
            };
            fs::create_dir_all(&target).expect("create tilde fixture directory");
            fs::write(target.join(fixture.basename), "").expect("write tilde fixture");

            let home_path = home.to_string_lossy().into_owned();
            let env = [("HOME", home_path.as_str())];
            let mut bash_keys = fixture.prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = fixture.prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let context = format!("ANSI-C {} with expand-tilde {expand_tilde}", fixture.name);
            if fixture.has_candidate {
                assert_filename_line_buffer_bytes(&bash, &sushline, fixture.expected, &context);
            } else {
                let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:");
                let sushline_hex = raw_accepted_hex_marker(&sushline);
                assert!(
                    bash_hex.is_some() && bash_hex == sushline_hex,
                    "Bash and Sushline line buffers differed for {context}"
                );
                assert_eq!(
                    raw_accepted_hex(&sushline).as_deref(),
                    Some(fixture.expected),
                    "line buffer changed for {context}"
                );
                assert_eq!(raw_bell_count(&bash), 1, "Bash bell count for {context}");
                assert_eq!(
                    raw_bell_count(&sushline),
                    1,
                    "Sushline bell count for {context}"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_inserts_ansi_c_directory_suffixes_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probes 62bc4dbb95404d379844650b2240cfb3
    // and ff8743cd38384f12815b0a6bc84d80a3.
    #[derive(Clone, Copy)]
    struct Fixture {
        name: &'static str,
        prefix: &'static [u8],
        parent: &'static str,
        basename: &'static str,
        directory: bool,
        expected_off: &'static [u8],
        expected_on: &'static [u8],
    }
    let fixtures = [
        Fixture {
            name: "plain-directory",
            prefix: b"echo $'only",
            parent: "",
            basename: "only-dir",
            directory: true,
            expected_off: b"echo $'only-dir",
            expected_on: b"echo $'only-dir/",
        },
        Fixture {
            name: "space-directory",
            prefix: b"echo $'only",
            parent: "",
            basename: "only space-dir",
            directory: true,
            expected_off: b"echo $'only space-dir'",
            expected_on: b"echo $'only space-dir'/",
        },
        Fixture {
            name: "child-file",
            prefix: b"echo $'only-dir/leaf",
            parent: "only-dir",
            basename: "leaf-file",
            directory: false,
            expected_off: b"echo $'only-dir/leaf-file' ",
            expected_on: b"echo $'only-dir/leaf-file' ",
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");

    for mark_directories in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset mark-directories {mark_directories}\nset completion-query-items 999\nset page-completions off"
        );
        for fixture in fixtures {
            let scenario = root.path().join(fixture.name).join(mark_directories);
            let cwd = scenario.join("cwd");
            let home = scenario.join("home");
            fs::create_dir_all(&cwd).expect("create fixture cwd");
            fs::create_dir_all(&home).expect("create fixture HOME");
            let target = cwd.join(fixture.parent);
            if fixture.directory {
                fs::create_dir(target.join(fixture.basename)).expect("create directory fixture");
            } else {
                fs::create_dir_all(&target).expect("create child directory");
                fs::write(target.join(fixture.basename), "").expect("write child fixture");
            }
            let home_path = home.to_string_lossy().into_owned();
            let env = [("HOME", home_path.as_str())];
            let mut bash_keys = fixture.prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = fixture.prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let expected = if mark_directories == "on" {
                fixture.expected_on
            } else {
                fixture.expected_off
            };
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                expected,
                &format!(
                    "ANSI-C {} with mark-directories {mark_directories}",
                    fixture.name
                ),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn single_candidate_regular_completion_preserves_quoted_suffix_like_bash_5_3() {
    // Runtime-only Bash 5.3 observations for these two completion commands,
    // quote forms, prefix spellings, and option combinations; not source proof.
    let quotes = [
        ("single", b"'".as_slice(), b'\''),
        ("double", b"\"".as_slice(), b'\"'),
        ("ansi-c", b"$'".as_slice(), b'\''),
    ];
    let prefixes = [
        ("exact", b"safe-Al".as_slice(), true),
        ("case-different", b"safe-al".as_slice(), false),
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe-Alpha"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (quote_name, opening, closing) in &quotes {
            for (prefix_name, prefix, exact_prefix) in &prefixes {
                for ignore_case in ["off", "on"] {
                    if !exact_prefix && ignore_case == "off" {
                        continue;
                    }
                    for skip_completed_text in ["off", "on"] {
                        let inputrc = format!(
                            "\"\\C-o\": {command}\nset completion-ignore-case {ignore_case}\nset skip-completed-text {skip_completed_text}\nset completion-query-items 999\nset page-completions off"
                        );
                        let mut line = b"echo ".to_vec();
                        line.extend_from_slice(opening);
                        line.extend_from_slice(prefix);
                        line.extend_from_slice(b"pha");
                        line.push(*closing);
                        let mut bash_keys = line.clone();
                        let mut sushline_keys = line;
                        for _ in 0..4 {
                            bash_keys.push(0x02);
                            sushline_keys.push(0x02);
                        }
                        bash_keys.extend_from_slice(b"\x0f\x07\r");
                        sushline_keys.extend_from_slice(b"\x0f\r");

                        let mut expected = b"echo ".to_vec();
                        expected.extend_from_slice(opening);
                        expected.extend_from_slice(b"safe-Alpha");
                        if skip_completed_text == "off" || !exact_prefix {
                            expected.extend_from_slice(b"pha");
                        }
                        expected.push(*closing);
                        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                            &cwd,
                            &inputrc,
                            &bash_keys,
                            &sushline_keys,
                            "set +H",
                            &env,
                        );
                        assert_filename_line_buffer_bytes(
                            &bash,
                            &sushline,
                            &expected,
                            &format!(
                                "single-candidate {command} suffix quote={quote_name} prefix={prefix_name} ignore-case={ignore_case} skip={skip_completed_text}"
                            ),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_regular_completion_suffix_and_point_match_bash_5_3() {
    // The typed marker exposes the completion point as well as inserted bytes.
    // These are fixture-scoped Bash 5.3 runtime comparisons, not source proof.
    let quotes = [
        ("unquoted", b"".as_slice(), None),
        ("single", b"'".as_slice(), Some(b'\'')),
        ("double", b"\"".as_slice(), Some(b'\"')),
        ("ansi-c", b"$'".as_slice(), Some(b'\'')),
    ];
    let prefixes = [
        ("exact", b"safe-Al".as_slice(), true),
        ("case-different", b"safe-al".as_slice(), false),
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe-Alpha"), b"").expect("write fixture file");
    fs::write(cwd.join("safe-Alphabeta"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (quote_name, opening, closing) in &quotes {
            for (prefix_name, prefix, exact_prefix) in &prefixes {
                for ignore_case in ["off", "on"] {
                    if !exact_prefix && ignore_case == "off" {
                        continue;
                    }
                    for skip_completed_text in ["off", "on"] {
                        let inputrc = format!(
                            "\"\\C-o\": {command}\nset completion-ignore-case {ignore_case}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
                        );
                        let mut line = b"echo ".to_vec();
                        line.extend_from_slice(opening);
                        line.extend_from_slice(prefix);
                        line.extend_from_slice(b"pha");
                        if let Some(closing) = closing {
                            line.push(*closing);
                        }
                        let mut bash_keys = line.clone();
                        let mut sushline_keys = line;
                        for _ in 0..(3 + usize::from(closing.is_some())) {
                            bash_keys.push(0x02);
                            sushline_keys.push(0x02);
                        }
                        bash_keys.extend_from_slice(b"\x0fX\x07\r");
                        sushline_keys.extend_from_slice(b"\x0fX\r");

                        let mut expected = b"echo ".to_vec();
                        expected.extend_from_slice(opening);
                        expected.extend_from_slice(b"safe-AlphaX");
                        if skip_completed_text == "off" || !exact_prefix {
                            expected.extend_from_slice(b"pha");
                        }
                        if let Some(closing) = closing {
                            expected.push(*closing);
                        }
                        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                            &cwd,
                            &inputrc,
                            &bash_keys,
                            &sushline_keys,
                            "set +H",
                            &env,
                        );
                        let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                            .expect("Bash line-buffer marker");
                        let (bash_hex_pairs, bash_hex_remainder) = bash_hex.as_chunks::<2>();
                        assert!(bash_hex_remainder.is_empty(), "odd Bash hex marker");
                        let bash_bytes = bash_hex_pairs
                            .iter()
                            .map(|pair| {
                                (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                                    | hex_digit(pair[1]).expect("Bash hex digit")
                            })
                            .collect::<Vec<_>>();
                        let sushline_bytes =
                            raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
                        let context = format!(
                            "multi-match {command} quote={quote_name} prefix={prefix_name} ignore-case={ignore_case} skip={skip_completed_text}"
                        );
                        assert_eq!(bash_bytes, expected, "Bash bytes for {context}");
                        assert_eq!(sushline_bytes, expected, "Sushline bytes for {context}");
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_special_filename_suffix_matches_bash_5_3() {
    // Runtime-only Bash 5.3 observations; the inserted marker checks point placement.
    let quotes = [
        ("unquoted", b"".as_slice(), None),
        ("single", b"'".as_slice(), Some(b'\'')),
        ("double", b"\"".as_slice(), Some(b'\"')),
        ("ansi-c", b"$'".as_slice(), Some(b'\'')),
    ];
    let prefixes = [
        ("exact", b"safe-Al".as_slice(), true),
        ("case-different", b"safe-al".as_slice(), false),
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe-Alpha beta"), b"").expect("write fixture file");
    fs::write(cwd.join("safe-Alpha betamax"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (quote_name, opening, closing) in &quotes {
            for (prefix_name, prefix, exact_prefix) in &prefixes {
                for ignore_case in ["off", "on"] {
                    if !exact_prefix && ignore_case == "off" {
                        continue;
                    }
                    for skip_completed_text in ["off", "on"] {
                        let inputrc = format!(
                            "\"\\C-o\": {command}\nset completion-ignore-case {ignore_case}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
                        );
                        let mut line = b"echo ".to_vec();
                        line.extend_from_slice(opening);
                        line.extend_from_slice(prefix);
                        line.extend_from_slice(b"pha");
                        if let Some(closing) = closing {
                            line.push(*closing);
                        }
                        let mut bash_keys = line.clone();
                        let mut sushline_keys = line;
                        for _ in 0..(3 + usize::from(closing.is_some())) {
                            bash_keys.push(0x02);
                            sushline_keys.push(0x02);
                        }
                        bash_keys.extend_from_slice(b"\x0fX\x07\r");
                        sushline_keys.extend_from_slice(b"\x0fX\r");

                        let mut expected = b"echo ".to_vec();
                        expected.extend_from_slice(opening);
                        if closing.is_none() {
                            expected.extend_from_slice(b"safe-Alpha\\ betaX");
                        } else {
                            expected.extend_from_slice(b"safe-Alpha betaX");
                        }
                        if skip_completed_text == "off" || !exact_prefix {
                            expected.extend_from_slice(b"pha");
                        }
                        if let Some(closing) = closing {
                            expected.push(*closing);
                        }
                        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                            &cwd,
                            &inputrc,
                            &bash_keys,
                            &sushline_keys,
                            "set +H",
                            &env,
                        );
                        let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                            .expect("Bash line-buffer marker");
                        let (bash_hex_pairs, bash_hex_remainder) = bash_hex.as_chunks::<2>();
                        assert!(bash_hex_remainder.is_empty(), "odd Bash hex marker");
                        let bash_bytes = bash_hex_pairs
                            .iter()
                            .map(|pair| {
                                (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                                    | hex_digit(pair[1]).expect("Bash hex digit")
                            })
                            .collect::<Vec<_>>();
                        let sushline_bytes =
                            raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
                        let context = format!(
                            "special filename {command} quote={quote_name} prefix={prefix_name} ignore-case={ignore_case} skip={skip_completed_text}"
                        );
                        assert_eq!(bash_bytes, expected, "Bash bytes for {context}");
                        assert_eq!(sushline_bytes, expected, "Sushline bytes for {context}");
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_escaped_filename_suffix_matches_bash_5_3() {
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe space-one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe space-two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (skip_completed_text, expected) in [
            ("off", b"echo safe\\ space-X\\ space-".as_slice()),
            ("on", b"echo safe\\ space-X".as_slice()),
        ] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo safe\\ space-".to_vec();
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..8 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );

            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            assert_eq!(
                bash_hex.len() % 2,
                0,
                "Bash hex for {command}, skip={skip_completed_text}"
            );
            let bash_bytes = bash_hex
                .chunks(2)
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line hex");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            assert_eq!(bash_bytes, expected, "Bash bytes for {context}; {bash:?}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes for {context}; {sushline:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_escaped_glob_suffix_matches_bash_5_3() {
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe*one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe*two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (skip_completed_text, expected) in [
            ("off", b"echo safe\\*X\\*".as_slice()),
            ("on", b"echo safe\\*X".as_slice()),
        ] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo safe\\*".to_vec();
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..2 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );

            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            assert_eq!(
                bash_hex.len() % 2,
                0,
                "Bash hex for {command}, skip={skip_completed_text}"
            );
            let bash_bytes = bash_hex
                .chunks(2)
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line hex");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            assert_eq!(bash_bytes, expected, "Bash bytes for {context}; {bash:?}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes for {context}; {sushline:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_escaped_backslash_suffix_matches_bash_5_3() {
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe\\one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe\\two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (skip_completed_text, expected) in [
            ("off", b"echo safe\\\\X\\\\".as_slice()),
            ("on", b"echo safe\\\\X".as_slice()),
        ] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo safe\\\\".to_vec();
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..2 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );

            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            assert_eq!(
                bash_hex.len() % 2,
                0,
                "Bash hex for {command}, skip={skip_completed_text}"
            );
            let bash_bytes = bash_hex
                .chunks(2)
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line hex");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            assert_eq!(bash_bytes, expected, "Bash bytes for {context}; {bash:?}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes for {context}; {sushline:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_partial_escaped_glob_suffix_matches_bash_5_3() {
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe*one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe*two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (skip_completed_text, expected) in [
            ("off", b"echo safe\\*Y\\*X".as_slice()),
            ("on", b"echo safe\\*YX".as_slice()),
        ] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo safe\\*X".to_vec();
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..3 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fY\x07\r");
            sushline_keys.extend_from_slice(b"\x0fY\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );

            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            assert_eq!(
                bash_hex.len() % 2,
                0,
                "Bash hex for {command}, skip={skip_completed_text}"
            );
            let bash_bytes = bash_hex
                .chunks(2)
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line hex");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            assert_eq!(bash_bytes, expected, "Bash bytes for {context}; {bash:?}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes for {context}; {sushline:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_double_quoted_escaped_quote_suffix_matches_bash_5_3() {
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe\"one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe\"two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (skip_completed_text, expected) in [
            ("off", br#"echo "safe\"X\"""#.as_slice()),
            ("on", br#"echo "safe\"X""#.as_slice()),
        ] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo \"safe\\\"\"".to_vec();
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..3 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            assert_eq!(
                bash_hex.len() % 2,
                0,
                "Bash hex for {command}, skip={skip_completed_text}"
            );
            let bash_bytes = bash_hex
                .chunks(2)
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line hex");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            assert_eq!(bash_bytes, expected, "Bash bytes for {context}; {bash:?}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes for {context}; {sushline:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_double_quoted_escaped_quote_with_space_suffix_matches_bash_5_3() {
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe\" a-one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe\" a-two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (skip_completed_text, expected) in [
            ("off", br#"echo "safe\" a-X\" a-""#.as_slice()),
            ("on", br#"echo "safe\" a-X""#.as_slice()),
        ] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo \"safe\\\" a-\"".to_vec();
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..6 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            assert_eq!(
                bash_hex.len() % 2,
                0,
                "Bash hex for {command}, skip={skip_completed_text}"
            );
            let bash_bytes = bash_hex
                .chunks(2)
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line hex");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            assert_eq!(bash_bytes, expected, "Bash bytes for {context}; {bash:?}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes for {context}; {sushline:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_ansi_c_backslash_suffix_matches_bash_5_3() {
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe\\one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe\\two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for (skip_completed_text, expected) in [
            ("off", b"echo $'safe\\X\\\\'".as_slice()),
            ("on", b"echo $'safe\\X\\'".as_slice()),
        ] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo $'safe\\\\'".to_vec();
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..3 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );

            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            assert_eq!(
                bash_hex.len() % 2,
                0,
                "Bash hex for {command}, skip={skip_completed_text}"
            );
            let bash_bytes = bash_hex
                .chunks(2)
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line hex");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            assert_eq!(bash_bytes, expected, "Bash bytes for {context}; {bash:?}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes for {context}; {sushline:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_single_quoted_apostrophe_suffix_matches_bash_5_3() {
    // Fixture-scoped Bash 5.3 runtime comparison, not source proof.
    // X exposes the insertion point as well as the exact raw shell spelling.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe'one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe'two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    struct SuffixCase {
        raw_suffix: &'static [u8],
        left_count: usize,
        expected_skip_off: &'static [u8],
        expected_skip_on: &'static [u8],
    }
    let suffix_cases = [
        SuffixCase {
            raw_suffix: b"'\\'''",
            left_count: 5,
            expected_skip_off: br"echo 'safe'\''X\'''",
            expected_skip_on: br"echo 'safe'\''X'",
        },
        SuffixCase {
            raw_suffix: b"'\\''z'",
            left_count: 6,
            expected_skip_off: br"echo 'safe'\''X\''z'",
            expected_skip_on: br"echo 'safe'\''Xz'",
        },
    ];

    for SuffixCase {
        raw_suffix,
        left_count,
        expected_skip_off,
        expected_skip_on,
    } in suffix_cases
    {
        for command in ["complete", "complete-filename"] {
            for (skip_completed_text, expected) in
                [("off", expected_skip_off), ("on", expected_skip_on)]
            {
                let inputrc = format!(
                    "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
                );
                let mut bash_keys = b"echo 'safe".to_vec();
                bash_keys.extend_from_slice(raw_suffix);
                let mut sushline_keys = bash_keys.clone();
                for _ in 0..left_count {
                    bash_keys.push(0x02);
                    sushline_keys.push(0x02);
                }
                bash_keys.extend_from_slice(b"\x0fX\x07\r");
                sushline_keys.extend_from_slice(b"\x0fX\r");
                let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                    &cwd,
                    &inputrc,
                    &bash_keys,
                    &sushline_keys,
                    "set +H",
                    &env,
                );
                let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                    .expect("Bash line-buffer marker");
                let (bash_hex_pairs, bash_hex_remainder) = bash_hex.as_chunks::<2>();
                assert!(bash_hex_remainder.is_empty(), "odd Bash hex marker");
                let bash_bytes = bash_hex_pairs
                    .iter()
                    .map(|pair| {
                        (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                            | hex_digit(pair[1]).expect("Bash hex digit")
                    })
                    .collect::<Vec<_>>();
                let sushline_bytes =
                    raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
                let context = format!(
                    "suffix={raw_suffix:?}, {command}, skip-completed-text={skip_completed_text}"
                );
                assert_eq!(bash_bytes, expected, "Bash bytes and point for {context}");
                assert_eq!(
                    sushline_bytes, expected,
                    "Sushline bytes and point for {context}"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_consecutive_single_quoted_apostrophes_match_bash_5_3() {
    // Fixture-scoped Bash 5.3 runtime comparison, not source proof.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe''one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe''two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    struct SuffixCase {
        raw_suffix: &'static [u8],
        left_count: usize,
        expected_skip_off: &'static [u8],
        expected_skip_on: &'static [u8],
    }
    let suffix_cases = [
        SuffixCase {
            raw_suffix: concat!("'\\''", "'\\''", "'").as_bytes(),
            left_count: 9,
            expected_skip_off: concat!("echo 'safe", "'\\''", "'\\''", "X", "\\'''", "\\'''")
                .as_bytes(),
            expected_skip_on: concat!("echo 'safe", "'\\''", "'\\''", "X'").as_bytes(),
        },
        SuffixCase {
            raw_suffix: concat!("'\\''", "'\\''", "z'").as_bytes(),
            left_count: 10,
            expected_skip_off: concat!("echo 'safe", "'\\''", "'\\''", "X", "\\'''", "\\''", "z'")
                .as_bytes(),
            expected_skip_on: concat!("echo 'safe", "'\\''", "'\\''", "Xz'").as_bytes(),
        },
    ];

    for SuffixCase {
        raw_suffix,
        left_count,
        expected_skip_off,
        expected_skip_on,
    } in suffix_cases
    {
        for command in ["complete", "complete-filename"] {
            for (skip_completed_text, expected) in
                [("off", expected_skip_off), ("on", expected_skip_on)]
            {
                let inputrc = format!(
                    "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
                );
                let mut bash_keys = b"echo 'safe".to_vec();
                bash_keys.extend_from_slice(raw_suffix);
                let mut sushline_keys = bash_keys.clone();
                for _ in 0..left_count {
                    bash_keys.push(0x02);
                    sushline_keys.push(0x02);
                }
                bash_keys.extend_from_slice(b"\x0fX\x07\r");
                sushline_keys.extend_from_slice(b"\x0fX\r");
                let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                    &cwd,
                    &inputrc,
                    &bash_keys,
                    &sushline_keys,
                    "set +H",
                    &env,
                );
                let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                    .expect("Bash line-buffer marker");
                let (bash_hex_pairs, bash_hex_remainder) = bash_hex.as_chunks::<2>();
                assert!(bash_hex_remainder.is_empty(), "odd Bash hex marker");
                let bash_bytes = bash_hex_pairs
                    .iter()
                    .map(|pair| {
                        (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                            | hex_digit(pair[1]).expect("Bash hex digit")
                    })
                    .collect::<Vec<_>>();
                let sushline_bytes =
                    raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
                let context = format!(
                    "suffix={raw_suffix:?}, {command}, skip-completed-text={skip_completed_text}"
                );
                assert_eq!(bash_bytes, expected, "Bash bytes and point for {context}");
                assert_eq!(
                    sushline_bytes, expected,
                    "Sushline bytes and point for {context}"
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn initial_open_single_quote_common_prefix_matches_bash_5_3() {
    // Fixture-scoped Bash 5.3 PTY runtime comparison, not GNU source proof.
    // Inserting X marks the point after ambiguous common-prefix completion.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe-one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe-two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for skip_completed_text in ["off", "on"] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                b"echo 'safe\x0fX\x07\r",
                b"echo 'safe\x0fX\r",
                "set +H",
                &env,
            );
            let bash_hex =
                raw_line_after_marker(&bash, b"BASH_READLINE_HEX:").expect("Bash line marker");
            let (pairs, remainder) = bash_hex.as_chunks::<2>();
            assert!(remainder.is_empty(), "odd Bash hex marker");
            let bash_bytes = pairs
                .iter()
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes =
                raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            let expected = b"echo 'safe-X";
            assert_eq!(bash_bytes, expected, "Bash bytes and point for {context}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes and point for {context}"
            );
            assert_eq!(raw_bell_count(&bash), 1, "Bash bell for {context}");
            assert_eq!(raw_bell_count(&sushline), 1, "Sushline bell for {context}");
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_single_quoted_closer_matches_bash_5_3() {
    // Fixture-scoped Bash 5.3 runtime comparison, not source proof.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe-one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe-two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for skip_completed_text in ["off", "on"] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo 'safe'".to_vec();
            let mut sushline_keys = bash_keys.clone();
            bash_keys.push(0x02);
            sushline_keys.push(0x02);
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            let (bash_hex_pairs, bash_hex_remainder) = bash_hex.as_chunks::<2>();
            assert!(bash_hex_remainder.is_empty(), "odd Bash hex marker");
            let bash_bytes = bash_hex_pairs
                .iter()
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes =
                raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            let expected = br"echo 'safe-X'";
            assert_eq!(bash_bytes, expected, "Bash bytes and point for {context}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes and point for {context}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_single_quote_closer_before_adjacent_text_matches_bash_5_3() {
    // Fixture-scoped Bash 5.3 runtime comparison, not source proof.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe-one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe-two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for skip_completed_text in ["off", "on"] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo 'safe'bar".to_vec();
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..4 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            let (bash_hex_pairs, bash_hex_remainder) = bash_hex.as_chunks::<2>();
            assert!(bash_hex_remainder.is_empty(), "odd Bash hex marker");
            let bash_bytes = bash_hex_pairs
                .iter()
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes =
                raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            let expected = br"echo 'safe-X'bar";
            assert_eq!(bash_bytes, expected, "Bash bytes and point for {context}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes and point for {context}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_candidate_consecutive_apostrophes_before_space_and_word_match_bash_5_3() {
    // Fixture-scoped Bash 5.3 runtime comparison, not source proof.
    // Moving left nine bytes puts point after the second group's closing quote,
    // before its backslash (the following word adds five bytes to the line).
    // X exposes the insertion point as well as the exact raw shell spelling.
    let expected = br"echo safe\'\'X\''' rest";
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe''one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe''two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename"] {
        for skip_completed_text in ["off", "on"] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo 'safe".to_vec();
            bash_keys.extend_from_slice(b"'\\''");
            bash_keys.extend_from_slice(b"'\\''");
            bash_keys.push(b'\'');
            bash_keys.extend_from_slice(b" rest");
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..9 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            let (pairs, remainder) = bash_hex.as_chunks::<2>();
            assert!(remainder.is_empty(), "odd Bash hex marker");
            let bash_bytes = pairs
                .iter()
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes =
                raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
            let context = format!("{command}, skip-completed-text={skip_completed_text}");
            assert_eq!(bash_bytes, expected, "Bash bytes and point for {context}");
            assert_eq!(
                sushline_bytes, expected,
                "Sushline bytes and point for {context}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn reopened_single_quote_text_matches_bash_5_3() {
    // Fixture-scoped Bash 5.3 runtime observations, not GNU source proof.
    // X exposes both the exact spelling and point after ambiguous completion.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    for (fixture, names, expected) in [
        (
            "whole-word",
            ["safe-one", "safe-two"],
            b"echo 'sa'f'eX".as_slice(),
        ),
        (
            "current-segment",
            ["eagle-one", "eagle-two"],
            b"echo 'sa'f'eagle-X".as_slice(),
        ),
    ] {
        for name in names {
            fs::write(cwd.join(name), b"").expect("write fixture file");
        }
        for command in ["complete", "complete-filename"] {
            for skip_completed_text in ["off", "on"] {
                let inputrc = format!(
                    "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
                );
                let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                    &cwd,
                    &inputrc,
                    b"echo 'sa'f'e\x0fX\x07\r",
                    b"echo 'sa'f'e\x0fX\r",
                    "set +H",
                    &env,
                );
                let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                    .expect("Bash line-buffer marker");
                let (pairs, remainder) = bash_hex.as_chunks::<2>();
                assert!(remainder.is_empty(), "odd Bash hex marker");
                let bash_bytes = pairs
                    .iter()
                    .map(|pair| {
                        (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                            | hex_digit(pair[1]).expect("Bash hex digit")
                    })
                    .collect::<Vec<_>>();
                let sushline_bytes =
                    raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
                let context =
                    format!("{fixture}, {command}, skip-completed-text={skip_completed_text}");
                assert_eq!(bash_bytes, expected, "Bash bytes and point for {context}");
                assert_eq!(
                    sushline_bytes, expected,
                    "Sushline bytes and point for {context}"
                );
                assert_eq!(raw_bell_count(&bash), 1, "Bash bell for {context}");
                assert_eq!(raw_bell_count(&sushline), 1, "Sushline bell for {context}");
            }
        }
        for name in names {
            fs::remove_file(cwd.join(name)).expect("remove fixture file");
        }
    }
}

#[cfg(unix)]
#[test]
fn reopened_single_quote_unique_and_path_cases_match_bash_5_3() {
    // These additional active-segment cases were verified with Bash 5.3 PTYs.
    let root = tempfile::tempdir().expect("temp completion root");
    let home = root.path().join("home");
    fs::create_dir(&home).expect("create fixture HOME");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let inputrc = "\"\\C-o\": complete\nset skip-completed-text off\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off";
    for (fixture, input, names, expected, bells) in [
        (
            "unique-current-segment",
            b"echo 'sa'f'e".as_slice(),
            &["eagle-one"][..],
            b"echo 'sa'f'eagle-one' X".as_slice(),
            0,
        ),
        (
            "path-prefix-with-root-candidates",
            b"echo 'sub/sa'f'e".as_slice(),
            &["eagle-one", "eagle-two"][..],
            b"echo 'sub/sa'f'eagle-X".as_slice(),
            1,
        ),
        (
            "path-prefix-with-subdirectory-candidates",
            b"echo 'sub/sa'f'e".as_slice(),
            &["sub/safe-one", "sub/safe-two"][..],
            b"echo 'sub/sa'f'eX".as_slice(),
            1,
        ),
    ] {
        let cwd = root.path().join(fixture);
        fs::create_dir(&cwd).expect("create fixture cwd");
        for name in names.iter().copied() {
            let path = cwd.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create candidate parent");
            }
            fs::write(path, b"").expect("write fixture file");
        }
        let mut bash_keys = input.to_vec();
        let mut sushline_keys = bash_keys.clone();
        bash_keys.extend_from_slice(b"\x0fX\x07\r");
        sushline_keys.extend_from_slice(b"\x0fX\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let bash_hex =
            raw_line_after_marker(&bash, b"BASH_READLINE_HEX:").expect("Bash line marker");
        let (pairs, remainder) = bash_hex.as_chunks::<2>();
        assert!(remainder.is_empty(), "odd Bash line marker");
        let bash_bytes = pairs
            .iter()
            .map(|pair| {
                (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                    | hex_digit(pair[1]).expect("Bash hex digit")
            })
            .collect::<Vec<_>>();
        let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
        assert_eq!(bash_bytes, expected, "Bash bytes for {fixture}");
        assert_eq!(sushline_bytes, expected, "Sushline bytes for {fixture}");
        assert_eq!(raw_bell_count(&bash), bells, "Bash bells for {fixture}");
        assert_eq!(
            raw_bell_count(&sushline),
            bells,
            "Sushline bells for {fixture}"
        );
    }
}

#[cfg(unix)]
#[test]
fn single_quote_apostrophe_escape_cursor_boundaries_match_bash_5_3() {
    // Fixture-scoped Bash 5.3 runtime comparison, not GNU source proof.
    // Exercise every byte boundary through the two apostrophe escapes and their
    // final closer. X exposes point as well as the raw spelling before ` rest`.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let inputrc = "\"\\C-o\": complete\nset skip-completed-text off\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off";
    fs::write(cwd.join("safe''one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe''two"), b"").expect("write fixture file");
    let suffix = b"'\\'''\\''' rest";
    let expected_lines: [&[u8]; 10] = [
        br"echo 'safe'\'''\''X\'''\''' rest",
        br"echo safe\'\'X\'''\''' rest",
        br"echo safe\'\'X'''\''' rest",
        br"echo safe\'\'X''\''' rest",
        br"echo 'safe'\''X'\''' rest",
        br"echo safe\'\'X\''' rest",
        br"echo safe\'\'X''' rest",
        br"echo safe\'\'X'' rest",
        br"echo 'safe'\'''\''X' rest",
        br"echo safe\'\'X rest",
    ];
    let cases = expected_lines
        .into_iter()
        .enumerate()
        .map(|(point, expected)| (suffix.as_slice(), point, expected))
        // The empty reopened segment at boundary four is unchanged even without
        // another apostrophe escape or adjacent word to its right.
        .chain([
            (br"'\''".as_slice(), 4, br"echo 'safe'\''X".as_slice()),
            (br"'\'' rest", 4, br"echo 'safe'\''X rest"),
            (br"'\''z' rest", 4, br"echo 'safe'\''Xz' rest"),
            (br"'\''' rest", 4, br"echo 'safe'\''X' rest"),
        ]);
    for (suffix, cursor_in_suffix, expected) in cases {
        let mut bash_keys = b"echo 'safe".to_vec();
        bash_keys.extend_from_slice(suffix);
        let mut sushline_keys = bash_keys.clone();
        for _ in 0..suffix.len() - cursor_in_suffix {
            bash_keys.push(0x02);
            sushline_keys.push(0x02);
        }
        bash_keys.extend_from_slice(b"\x0fX\x07\r");
        sushline_keys.extend_from_slice(b"\x0fX\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let bash_hex =
            raw_line_after_marker(&bash, b"BASH_READLINE_HEX:").expect("Bash line-buffer marker");
        let (pairs, remainder) = bash_hex.as_chunks::<2>();
        assert!(remainder.is_empty(), "odd Bash hex marker");
        let bash_bytes = pairs
            .iter()
            .map(|pair| {
                (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                    | hex_digit(pair[1]).expect("Bash hex digit")
            })
            .collect::<Vec<_>>();
        let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
        assert_eq!(
            bash_bytes, expected,
            "Bash bytes and point at boundary {cursor_in_suffix} of {suffix:?}"
        );
        assert_eq!(
            sushline_bytes, expected,
            "Sushline bytes and point at boundary {cursor_in_suffix} of {suffix:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn single_quote_apostrophe_escape_boundary_matches_bash_commands_and_skip_setting() {
    // The unchanged empty quote segment and its bell are fixture-scoped Bash
    // 5.3 runtime observations, not GNU source proof.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe''one"), b"").expect("write fixture file");
    fs::write(cwd.join("safe''two"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let suffix = b"'\\'''\\''' rest";
    let expected = br"echo 'safe'\''X'\''' rest";

    for command in ["complete", "complete-filename"] {
        for skip_completed_text in ["off", "on"] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset skip-completed-text {skip_completed_text}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo 'safe".to_vec();
            bash_keys.extend_from_slice(suffix);
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..suffix.len() - 4 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0fX\x07\r");
            sushline_keys.extend_from_slice(b"\x0fX\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:")
                .expect("Bash line-buffer marker");
            let (pairs, remainder) = bash_hex.as_chunks::<2>();
            assert!(remainder.is_empty(), "odd Bash hex marker");
            let bash_bytes = pairs
                .iter()
                .map(|pair| {
                    (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                        | hex_digit(pair[1]).expect("Bash hex digit")
                })
                .collect::<Vec<_>>();
            let sushline_bytes =
                raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
            let context = format!("command={command}, skip-completed-text={skip_completed_text}");
            assert_eq!(bash_bytes, expected, "Bash bytes for {context}");
            assert_eq!(sushline_bytes, expected, "Sushline bytes for {context}");
            assert_eq!(raw_bell_count(&bash), 1, "Bash bell count for {context}");
            assert_eq!(
                raw_bell_count(&sushline),
                1,
                "Sushline bell count for {context}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn explicit_dot_filename_candidates_match_bash_5_3() {
    // Bash 5.3 PTY baseline 6f8e49fe55674f738ba7204ff46697d0 and this paired
    // assertion are limited to this fixture and these prefixes/options; the
    // baseline is runtime evidence, not GNU source proof.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join(".alpha"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for (prefix, match_hidden_files, expected) in [
        (".", "on", b"echo cwd/.".as_slice()),
        (".", "off", b"echo cwd/.".as_slice()),
        ("..", "on", b"echo cwd/../".as_slice()),
        ("..", "off", b"echo cwd/../".as_slice()),
        ("./.", "on", b"echo cwd/./.".as_slice()),
        ("./.", "off", b"echo cwd/./.".as_slice()),
        ("./..", "on", b"echo cwd/./../".as_slice()),
        ("./..", "off", b"echo cwd/./../".as_slice()),
        (".a", "on", b"echo cwd/.alpha ".as_slice()),
        (".a", "off", b"echo cwd/.alpha ".as_slice()),
        ("", "on", b"echo cwd/.alpha ".as_slice()),
        ("", "off", b"echo cwd/".as_slice()),
    ] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset match-hidden-files {match_hidden_files}\nset show-all-if-ambiguous off\nset completion-query-items 999\nset page-completions off"
        );
        let mut bash_keys = format!("echo cwd/{prefix}").into_bytes();
        let mut sushline_keys = bash_keys.clone();
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            root.path(),
            &inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let bash_hex =
            raw_line_after_marker(&bash, b"BASH_READLINE_HEX:").expect("Bash line-buffer marker");
        let (pairs, remainder) = bash_hex.as_chunks::<2>();
        assert!(remainder.is_empty(), "odd Bash hex marker: {bash:?}");
        let bash_bytes = pairs
            .iter()
            .map(|pair| {
                (hex_digit(pair[0]).expect("Bash hex digit") << 4)
                    | hex_digit(pair[1]).expect("Bash hex digit")
            })
            .collect::<Vec<_>>();
        let sushline_bytes = raw_accepted_hex(&sushline).expect("Sushline accepted-line marker");
        let context = format!("prefix={prefix:?}, match-hidden-files={match_hidden_files}");
        assert_eq!(bash_bytes, expected, "Bash bytes for {context}: {bash:?}");
        assert_eq!(
            sushline_bytes, expected,
            "Sushline bytes for {context}: {sushline:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_skips_ansi_c_suffix_overlap_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probes fd9b4cde2d1a4d6ebfc1d0e8e6f4e9ac,
    // 310c48ab3ecc46babeacb3932af2f387, and 6388a4ec690c43c9b13fa0c83cd637c7.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::create_dir(cwd.join("only space-dir")).expect("create fixture directory");
    fs::write(cwd.join("only space-dir/leaf-file"), b"").expect("create fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    struct SuffixOverlapCase {
        line: &'static [u8],
        left_count: usize,
        expected_skip_off: &'static [u8],
        expected_skip_on: &'static [u8],
    }
    let cases = [
        SuffixOverlapCase {
            line: b"echo $'only space-dir/leaf-file",
            left_count: 4,
            expected_skip_off: b"echo $'only space-dir/leaf-file'file",
            expected_skip_on: b"echo $'only space-dir/leaf-file' ",
        },
        SuffixOverlapCase {
            line: b"echo $'only space-dir/leaf-fileX",
            left_count: 3,
            expected_skip_off: b"echo $'only space-dir/leaf-file'leX",
            expected_skip_on: b"echo $'only space-dir/leaf-file'X",
        },
        SuffixOverlapCase {
            line: b"echo $'only space-dir/leaf-fX",
            left_count: 1,
            expected_skip_off: b"echo $'only space-dir/leaf-file'X",
            expected_skip_on: b"echo $'only space-dir/leaf-file'X",
        },
    ];

    for skip_completed_text in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset mark-directories on\nset skip-completed-text {skip_completed_text}\nset completion-query-items 999\nset page-completions off"
        );
        for (index, case) in cases.iter().enumerate() {
            let mut bash_keys = case.line.to_vec();
            let mut sushline_keys = case.line.to_vec();
            for _ in 0..case.left_count {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                if skip_completed_text == "on" {
                    case.expected_skip_on
                } else {
                    case.expected_skip_off
                },
                &format!(
                    "ANSI-C filename suffix overlap case {index} with skip-completed-text {skip_completed_text}"
                ),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_skips_ansi_c_directory_suffix_overlap_without_duplicate_slashes() {
    // Runtime-only Bash 5.3 PTY probe af4155e11d9a4e4882c564b8166c4566.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::create_dir(cwd.join("only space-dir")).expect("create fixture directory");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for mark_directories in ["off", "on"] {
        for skip_completed_text in ["off", "on"] {
            let inputrc = format!(
                "\"\\C-o\": complete-filename\nset mark-directories {mark_directories}\nset skip-completed-text {skip_completed_text}\nset completion-query-items 999\nset page-completions off"
            );
            let mut bash_keys = b"echo $'only space-dir/".to_vec();
            let mut sushline_keys = bash_keys.clone();
            for _ in 0..4 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let expected = match (mark_directories, skip_completed_text) {
                (_, "on") => b"echo $'only space-dir'/".as_slice(),
                ("on", "off") => b"echo $'only space-dir'/dir/".as_slice(),
                _ => b"echo $'only space-dir'dir/".as_slice(),
            };
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                expected,
                &format!(
                    "ANSI-C directory suffix overlap with mark-directories {mark_directories} and skip-completed-text {skip_completed_text}"
                ),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_consumes_ansi_c_closing_quote_suffix_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probes 9113d5b9ec9c4c73abccc2faa54f1671,
    // c141d4bd6e1045038a4fb5a012dac862, and 63551375f86f443d99061582cbe8c825.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::create_dir(cwd.join("only space-dir")).expect("create fixture directory");
    fs::write(cwd.join("only space-dir/leaf-file"), b"").expect("create fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let line = b"echo $'only space-dir/leaf-file'";
    for (skip_completed_text, left_count) in [("off", 1), ("on", 5), ("on", 1)] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset skip-completed-text {skip_completed_text}\nset mark-directories on\nset page-completions off"
        );
        let mut bash_keys = line.to_vec();
        let mut sushline_keys = line.to_vec();
        for _ in 0..left_count {
            bash_keys.push(0x02);
            sushline_keys.push(0x02);
        }
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            &inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        assert_filename_line_buffer_bytes(
            &bash,
            &sushline,
            b"echo $'only space-dir/leaf-file' ",
            &format!(
                "ANSI-C existing closing quote suffix with skip-completed-text {skip_completed_text} at cursor offset {left_count}"
            ),
        );
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_handles_ansi_c_directory_quote_suffix_and_slash_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probe 307dd44870264eda894a46a55a1290de.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::create_dir(cwd.join("only space-dir")).expect("create fixture directory");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for mark_directories in ["off", "on"] {
        for existing_slash in [false, true] {
            let inputrc = format!(
                "\"\\C-o\": complete-filename\nset skip-completed-text on\nset mark-directories {mark_directories}\nset page-completions off"
            );
            let mut line = b"echo $'only space-dir'".to_vec();
            if existing_slash {
                line.push(b'/');
            }
            let mut bash_keys = line.clone();
            let mut sushline_keys = line;
            for _ in 0..(4 + usize::from(existing_slash)) {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let expected = if mark_directories == "on" || existing_slash {
                b"echo $'only space-dir'/".as_slice()
            } else {
                b"echo $'only space-dir'".as_slice()
            };
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                expected,
                &format!(
                    "ANSI-C directory quote suffix with mark-directories {mark_directories} and existing slash {existing_slash}"
                ),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_keeps_plain_ansi_c_directory_quote_open_after_suffix() {
    // Runtime-only Bash 5.3 PTY probe 3c501ee211f1406494c0468526c76fec.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::create_dir(cwd.join("plain-dir")).expect("create fixture directory");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for mark_directories in ["off", "on"] {
        for existing_slash in [false, true] {
            let inputrc = format!(
                "\"\\C-o\": complete-filename\nset skip-completed-text on\nset mark-directories {mark_directories}\nset page-completions off"
            );
            let mut line = b"echo $'plain-dir'".to_vec();
            if existing_slash {
                line.push(b'/');
            }
            let mut bash_keys = line.clone();
            let mut sushline_keys = line;
            for _ in 0..(4 + usize::from(existing_slash)) {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let expected = match (mark_directories, existing_slash) {
                ("on", false) => b"echo $'plain-dir/'".as_slice(),
                ("on", true) => b"echo $'plain-dir/'/".as_slice(),
                ("off", true) => b"echo $'plain-dir'/".as_slice(),
                _ => b"echo $'plain-dir'".as_slice(),
            };
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                expected,
                &format!(
                    "ANSI-C plain directory quote suffix with mark-directories {mark_directories} and existing slash {existing_slash}"
                ),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_leaves_incomplete_ansi_c_radix_prefixes_unmatched_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probes 80c4454bfcc245f4b15a91092255eac1
    // and 3ab5e8aa8b8e4766a6a0ec7048a0f287.
    struct EscapeCursorCase {
        line: &'static [u8],
        left_count: usize,
    }
    let cases = [
        EscapeCursorCase {
            line: b"echo $'safe\\x",
            left_count: 0,
        },
        EscapeCursorCase {
            line: b"echo $'safe\\x2",
            left_count: 0,
        },
        EscapeCursorCase {
            line: b"echo $'safe\\0",
            left_count: 0,
        },
        EscapeCursorCase {
            line: b"echo $'safe\\04",
            left_count: 0,
        },
        EscapeCursorCase {
            line: b"echo $'safe\\x20tail",
            left_count: 7,
        },
        EscapeCursorCase {
            line: b"echo $'safe\\x20tail",
            left_count: 5,
        },
        EscapeCursorCase {
            line: b"echo $'safe\\040tail",
            left_count: 5,
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe space-tail"), b"").expect("create fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let inputrc = "\"\\C-o\": complete-filename\nset mark-directories on\nset page-completions off";

    for (index, case) in cases.iter().enumerate() {
        let mut bash_keys = case.line.to_vec();
        let mut sushline_keys = case.line.to_vec();
        for _ in 0..case.left_count {
            bash_keys.push(0x02);
            sushline_keys.push(0x02);
        }
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:");
        let sushline_hex = raw_accepted_hex_marker(&sushline);
        assert!(
            bash_hex.is_some() && bash_hex == sushline_hex,
            "line-buffer hex differed for incomplete ANSI-C radix case {index}"
        );
        assert_eq!(
            raw_accepted_hex(&sushline).as_deref(),
            Some(case.line),
            "accepted bytes differed for incomplete ANSI-C radix case {index}"
        );
        assert_eq!(raw_bell_count(&bash), 1);
        assert_eq!(raw_bell_count(&sushline), 1);
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_does_not_match_fully_typed_ansi_c_radix_escape_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probe 3f56ed53cdbe4c428acc7d1227ab2a43.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe space-tail"), b"").expect("create fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let inputrc = "\"\\C-o\": complete-filename\nset mark-directories on\nset page-completions off";

    for (index, line) in [
        b"echo $'safe\\x20tail".as_slice(),
        b"echo $'safe\\040tail".as_slice(),
    ]
    .iter()
    .enumerate()
    {
        let mut bash_keys = line.to_vec();
        let mut sushline_keys = line.to_vec();
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:");
        let sushline_hex = raw_accepted_hex_marker(&sushline);
        assert!(
            bash_hex.is_some() && bash_hex == sushline_hex,
            "line-buffer hex differed for fully typed ANSI-C radix escape case {index}"
        );
        assert_eq!(
            raw_accepted_hex(&sushline).as_deref(),
            Some(*line),
            "accepted bytes differed for fully typed ANSI-C radix escape case {index}"
        );
        assert_eq!(raw_bell_count(&bash), 1);
        assert_eq!(raw_bell_count(&sushline), 1);
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_preserves_ansi_c_hex_and_octal_suffix_spellings() {
    // Runtime-only Bash 5.3 PTY probe 3be78325bd16420bb9643eb8af65358e.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe space-tail"), b"").expect("create fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for skip_completed_text in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset skip-completed-text {skip_completed_text}\nset page-completions off"
        );
        for escape in [b"x20tail".as_slice(), b"040tail".as_slice()] {
            let mut line = b"echo $'safe\\".to_vec();
            line.extend_from_slice(escape);
            let mut expected = b"echo $'safe space-tail'\\".to_vec();
            expected.extend_from_slice(escape);
            let mut bash_keys = line.clone();
            let mut sushline_keys = line;
            for _ in 0..(escape.len() + 1) {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                &expected,
                &format!(
                    "ANSI-C radix escape suffix with skip-completed-text {skip_completed_text}"
                ),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_keeps_ansi_c_escape_spelling_distinct_from_raw_suffix_overlap() {
    // Runtime-only Bash 5.3 PTY probe fc18a96b33bc4c1b9b85ec25e861466d.
    use std::os::unix::ffi::OsStringExt;

    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(
        cwd.join(std::ffi::OsString::from_vec(b"safe\nfile".to_vec())),
        b"",
    )
    .expect("create fixture with LF in its name");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];
    let mut expected = b"echo $'safe".to_vec();
    expected.push(b'\n');
    expected.extend_from_slice(b"file'\\nfile");

    for skip_completed_text in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset skip-completed-text {skip_completed_text}\nset completion-query-items 999\nset page-completions off"
        );
        let mut bash_keys = b"echo $'safe\\nfile".to_vec();
        let mut sushline_keys = bash_keys.clone();
        for _ in 0..6 {
            bash_keys.push(0x02);
            sushline_keys.push(0x02);
        }
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            &inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        assert_filename_line_buffer_bytes(
            &bash,
            &sushline,
            &expected,
            &format!("ANSI-C escape suffix with skip-completed-text {skip_completed_text}"),
        );
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_skips_ansi_c_ambiguous_common_prefix_overlap_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probe 23742dd34f5c4b058fa6f5969675005c.
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe-prefix-one"), b"").expect("create first fixture file");
    fs::write(cwd.join("safe-prefix-two"), b"").expect("create second fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for skip_completed_text in ["off", "on"] {
        let inputrc = format!(
            "\"\\C-o\": complete-filename\nset skip-completed-text {skip_completed_text}\nset completion-query-items 999\nset page-completions off"
        );
        let mut bash_keys = b"echo $'safe-prefix-".to_vec();
        let mut sushline_keys = bash_keys.clone();
        for _ in 0..7 {
            bash_keys.push(0x02);
            sushline_keys.push(0x02);
        }
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            &inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let expected = if skip_completed_text == "on" {
            b"echo $'safe-prefix-".as_slice()
        } else {
            b"echo $'safe-prefix-prefix-".as_slice()
        };
        let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:");
        let sushline_hex = raw_accepted_hex_marker(&sushline);
        assert!(
            bash_hex.is_some() && bash_hex == sushline_hex,
            "Bash and Sushline line-buffer hex differed for ANSI-C ambiguous suffix overlap with skip-completed-text {skip_completed_text}"
        );
        assert_eq!(
            raw_accepted_hex(&sushline).as_deref(),
            Some(expected),
            "accepted bytes differed for ANSI-C ambiguous suffix overlap with skip-completed-text {skip_completed_text}"
        );
        assert_eq!(raw_bell_count(&bash), 1);
        assert_eq!(raw_bell_count(&sushline), 1);
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_handles_ansi_c_symlink_directories_and_existing_slash_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probe ae85f3af04694cea9a8bb46015a75f64.
    for mark_directories in ["off", "on"] {
        for mark_symlinked_directories in ["off", "on"] {
            let root = tempfile::tempdir().expect("temp completion root");
            let cwd = root.path().join("cwd");
            let home = root.path().join("home");
            fs::create_dir(&cwd).expect("create fixture cwd");
            fs::create_dir(&home).expect("create fixture HOME");
            fs::create_dir(cwd.join("real-dir")).expect("create symlink target directory");
            std::os::unix::fs::symlink("real-dir", cwd.join("link-dir"))
                .expect("create symlink fixture");
            let home_path = home.to_string_lossy().into_owned();
            let env = [("HOME", home_path.as_str())];
            let inputrc = format!(
                "\"\\C-o\": complete-filename\nset mark-directories {mark_directories}\nset mark-symlinked-directories {mark_symlinked_directories}\nset completion-query-items 999\nset page-completions off"
            );
            let unique_bash_keys = b"echo $'link\x0f\x07\r";
            let unique_sushline_keys = b"echo $'link\x0f\r";
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                unique_bash_keys,
                unique_sushline_keys,
                "set +H",
                &env,
            );
            let expected = if mark_directories == "on" && mark_symlinked_directories == "on" {
                b"echo $'link-dir/".as_slice()
            } else {
                b"echo $'link-dir".as_slice()
            };
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                expected,
                &format!(
                    "ANSI-C symlink directory with mark-directories {mark_directories} and mark-symlinked-directories {mark_symlinked_directories}"
                ),
            );

            let existing_slash_bash_keys = b"echo $'link-dir/\x02\x0f\x07\r";
            let existing_slash_sushline_keys = b"echo $'link-dir/\x02\x0f\r";
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                existing_slash_bash_keys,
                existing_slash_sushline_keys,
                "set +H",
                &env,
            );
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                b"echo $'link-dir/",
                &format!(
                    "ANSI-C existing slash with mark-directories {mark_directories} and mark-symlinked-directories {mark_symlinked_directories}"
                ),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_keeps_ambiguous_ansi_c_quote_open_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probe 5495fc787df04b9599f9836b9177fd86.
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let root = tempfile::tempdir().expect("temp completion root");
    let dir = root.path().join("ambiguous-ansi-c");
    fs::create_dir(&dir).expect("create ambiguous ANSI-C fixture directory");
    for basename in [b"safe-alpha".as_slice(), b"safe-alpine".as_slice()] {
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write ambiguous ANSI-C fixture");
    }
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;
    let bash_keys = b"echo $'safe-a\x0f\x07\r";
    let sushline_keys = b"echo $'safe-a\x0f\r";
    let (bash, sushline) =
        run_filename_line_buffer_hex_pair(&dir, inputrc, bash_keys, sushline_keys, "set +H");
    let expected = b"echo $'safe-alp";
    let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:");
    let sushline_hex = raw_accepted_hex_marker(&sushline);
    assert!(
        bash_hex.is_some() && bash_hex == sushline_hex,
        "Bash and Sushline line buffers differed for ambiguous ANSI-C completion"
    );
    assert_eq!(
        raw_accepted_hex(&sushline).as_deref(),
        Some(expected.as_slice()),
        "ambiguous completion did not leave the ANSI-C quote open"
    );
    assert_eq!(
        raw_bell_count(&bash),
        1,
        "unexpected Bash ambiguity bell count"
    );
    assert_eq!(
        raw_bell_count(&sushline),
        1,
        "unexpected Sushline ambiguity bell count"
    );
}

#[cfg(unix)]
#[test]
fn complete_filename_handles_ansi_c_quoted_words_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probes 67152a0e1459401c90a38eaad29a6b99,
    // eae6778c995d4ff98f3e6125e3e9693f, e120dd1dc41241d6919c7393c13651ab,
    // and 59f8b916141343f3b2d52d061b4c4ac5.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "plain",
            b"safe-plain".as_slice(),
            b"$'safe-plain' ".as_slice(),
        ),
        (
            "space",
            b"safe-space name".as_slice(),
            b"$'safe-space name' ".as_slice(),
        ),
        (
            "apostrophe",
            b"safe-apostrophe'name".as_slice(),
            b"$'safe-apostrophe'\\''name' ".as_slice(),
        ),
        (
            "backslash",
            b"safe-back\\slash".as_slice(),
            b"$'safe-back\\slash' ".as_slice(),
        ),
        (
            "backslash-escape-letter",
            b"safe-back\\n".as_slice(),
            b"$'safe-back\\n' ".as_slice(),
        ),
        (
            "dollar",
            b"safe-dollar$name".as_slice(),
            b"$'safe-dollar$name' ".as_slice(),
        ),
        (
            "lf",
            b"safe-lf\nitem".as_slice(),
            b"$'safe-lf\nitem' ".as_slice(),
        ),
        (
            "tab",
            b"safe-tab\titem".as_slice(),
            b"$'safe-tab\titem' ".as_slice(),
        ),
        (
            "esc",
            b"safe-esc\x1bitem".as_slice(),
            b"$'safe-esc\x1bitem' ".as_slice(),
        ),
        (
            "del",
            b"safe-del\x7fitem".as_slice(),
            b"$'safe-del\x7fitem' ".as_slice(),
        ),
        (
            "cr",
            b"safe-cr\ritem".as_slice(),
            b"$'safe-cr\ritem' ".as_slice(),
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, expected_suffix) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create ANSI-C quote fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write ANSI-C quote fixture");
        let mut expected = b"echo ".to_vec();
        expected.extend_from_slice(expected_suffix);
        let bash_keys = b"echo $'safe-\x0f\x07\r";
        let sushline_keys = b"echo $'safe-\x0f\r";
        let (bash, sushline) =
            run_filename_line_buffer_hex_pair(&dir, inputrc, bash_keys, sushline_keys, "set +H");
        assert_filename_line_buffer_bytes(
            &bash,
            &sushline,
            &expected,
            &format!("{fixture}/ANSI-C quoted completion"),
        );
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_quotes_extglob_patterns_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probe b05892bb3ddf4ba5ab1d5cd49b8eb47a under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "at",
            b"safe-extglob-@(a|b)".as_slice(),
            b"safe-extglob-@\\(a\\|b\\) ".as_slice(),
        ),
        (
            "plus",
            b"safe-extglob-+(a|b)".as_slice(),
            b"safe-extglob-+\\(a\\|b\\) ".as_slice(),
        ),
        (
            "question",
            b"safe-extglob-?(a|b)".as_slice(),
            b"safe-extglob-\\?\\(a\\|b\\) ".as_slice(),
        ),
        (
            "star",
            b"safe-extglob-*(a|b)".as_slice(),
            b"safe-extglob-\\*\\(a\\|b\\) ".as_slice(),
        ),
        (
            "bang",
            b"safe-extglob-!(a|b)".as_slice(),
            b"safe-extglob-\\!\\(a\\|b\\) ".as_slice(),
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, expected_unquoted) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create extglob fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write extglob fixture");
        for extglob in ["on", "off"] {
            let bash_setup = if extglob == "on" {
                "shopt -s extglob; set +H"
            } else {
                "shopt -u extglob; set +H"
            };
            for (form, filename_prefix, quote) in [
                ("unquoted", b"safe-".as_slice(), None),
                ("single-quoted", b"safe-".as_slice(), Some(b'\'')),
                ("double-quoted", b"safe-".as_slice(), Some(b'"')),
            ] {
                let mut prefix = Vec::new();
                if let Some(quote) = quote {
                    prefix.push(quote);
                }
                prefix.extend_from_slice(filename_prefix);
                let mut expected = Vec::new();
                if let Some(quote) = quote {
                    expected.push(quote);
                    expected.extend_from_slice(basename);
                    expected.push(quote);
                    expected.push(b' ');
                } else {
                    expected.extend_from_slice(expected_unquoted);
                }
                let mut bash_keys = prefix.clone();
                bash_keys.extend_from_slice(b"\x0f\x07\r");
                let mut sushline_keys = prefix;
                sushline_keys.extend_from_slice(b"\x0f\r");
                let (bash, sushline) = run_filename_line_buffer_hex_pair(
                    &dir,
                    inputrc,
                    &bash_keys,
                    &sushline_keys,
                    bash_setup,
                );
                let context = format!("{fixture}/extglob-{extglob}/{form}");
                assert_filename_line_buffer_bytes(&bash, &sushline, &expected, &context);
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_handles_equals_and_colon_word_breaks_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probe 88986588e94942af921e2a9188e49246 under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let cases = [
        (
            "equal-internal",
            b"safe-eq=item".as_slice(),
            "before",
            b"safe-".as_slice(),
        ),
        (
            "equal-internal",
            b"safe-eq=item".as_slice(),
            "at",
            b"safe-eq=".as_slice(),
        ),
        (
            "equal-internal",
            b"safe-eq=item".as_slice(),
            "after",
            b"safe-eq=i".as_slice(),
        ),
        (
            "equal-trailing",
            b"safe-trail=".as_slice(),
            "before",
            b"safe-trail".as_slice(),
        ),
        (
            "equal-trailing",
            b"safe-trail=".as_slice(),
            "at",
            b"safe-trail=".as_slice(),
        ),
        (
            "colon-internal",
            b"safe-colon:item".as_slice(),
            "before",
            b"safe-".as_slice(),
        ),
        (
            "colon-internal",
            b"safe-colon:item".as_slice(),
            "at",
            b"safe-colon:".as_slice(),
        ),
        (
            "colon-internal",
            b"safe-colon:item".as_slice(),
            "after",
            b"safe-colon:i".as_slice(),
        ),
        (
            "colon-trailing",
            b"safe-trail:".as_slice(),
            "before",
            b"safe-trail".as_slice(),
        ),
        (
            "colon-trailing",
            b"safe-trail:".as_slice(),
            "at",
            b"safe-trail:".as_slice(),
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, position, filename_prefix) in cases {
        let dir = root.path().join(fixture);
        fs::create_dir_all(&dir).expect("create equals/colon fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write equals/colon fixture");
        for (form, quote) in [
            ("unquoted", None),
            ("single-quoted", Some(b'\'')),
            ("double-quoted", Some(b'"')),
        ] {
            let mut prefix = Vec::new();
            if let Some(quote) = quote {
                prefix.push(quote);
            }
            prefix.extend_from_slice(filename_prefix);
            let mut expected = Vec::new();
            if let Some(quote) = quote {
                expected.push(quote);
            }
            expected.extend_from_slice(basename);
            if let Some(quote) = quote {
                expected.push(quote);
            }
            expected.push(b' ');
            let mut bash_keys = prefix.clone();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = prefix;
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) =
                run_filename_line_buffer_hex_pair(&dir, inputrc, &bash_keys, &sushline_keys, "");
            let context = format!("{fixture}/{position}/{form} completion");
            assert_filename_line_buffer_bytes(&bash, &sushline, &expected, &context);
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_preserves_carets_with_bash_history_expansion() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probe b44d41e4c15146e48f74ba51af6a2d6c under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        ("leading", b"^safe-item".as_slice(), b"^safe-".as_slice()),
        (
            "internal",
            b"safe-caret^item".as_slice(),
            b"safe-".as_slice(),
        ),
        ("trailing", b"safe-trail^".as_slice(), b"safe-".as_slice()),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, unquoted_prefix) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create caret fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write caret fixture");
        for history in ["on", "off"] {
            let bash_setup = if history == "on" { "set -H" } else { "set +H" };
            for (form, quote) in [
                ("unquoted", None),
                ("single-quoted", Some(b'\'')),
                ("double-quoted", Some(b'"')),
            ] {
                let mut prefix = Vec::new();
                if let Some(quote) = quote {
                    prefix.push(quote);
                }
                prefix.extend_from_slice(unquoted_prefix);
                let mut expected = Vec::new();
                if let Some(quote) = quote {
                    expected.push(quote);
                }
                expected.extend_from_slice(basename);
                if let Some(quote) = quote {
                    expected.push(quote);
                }
                expected.push(b' ');
                let mut bash_keys = prefix.clone();
                bash_keys.extend_from_slice(b"\x0f\x07\r");
                let mut sushline_keys = prefix;
                sushline_keys.extend_from_slice(b"\x0f\r");
                let (bash, sushline) = run_filename_line_buffer_hex_pair(
                    &dir,
                    inputrc,
                    &bash_keys,
                    &sushline_keys,
                    bash_setup,
                );
                let context = format!("{fixture}/{form}, history expansion {history}");
                assert_filename_line_buffer_bytes(&bash, &sushline, &expected, &context);
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_quotes_exclamation_marks_with_bash_history_expansion() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probes fd00643bc4884d47891859def4c4a35e and
    // 0ab22fd1610847feb11392dc08bfda78 under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "internal",
            b"safe-bang!item".as_slice(),
            [
                b"safe-bang\\!item ".as_slice(),
                b"'safe-bang!item' ".as_slice(),
                b"\"safe-bang!item\" ".as_slice(),
            ],
        ),
        (
            "trailing",
            b"safe-trail!".as_slice(),
            [
                b"safe-trail\\! ".as_slice(),
                b"'safe-trail!' ".as_slice(),
                b"\"safe-trail!\" ".as_slice(),
            ],
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, expected) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create exclamation fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write exclamation fixture");
        for history in ["on", "off"] {
            let bash_setup = if history == "on" { "set -H" } else { "set +H" };
            for ((form, prefix), expected) in [
                ("unquoted", b"safe-".as_slice()),
                ("single-quoted", b"'safe-".as_slice()),
                ("double-quoted", b"\"safe-".as_slice()),
            ]
            .into_iter()
            .zip(expected)
            {
                let mut bash_keys = prefix.to_vec();
                bash_keys.extend_from_slice(b"\x0f\x07\r");
                let mut sushline_keys = prefix.to_vec();
                sushline_keys.extend_from_slice(b"\x0f\r");
                let (bash, sushline) = run_filename_line_buffer_hex_pair(
                    &dir,
                    inputrc,
                    &bash_keys,
                    &sushline_keys,
                    bash_setup,
                );
                let context = format!("{fixture}/{form}, history expansion {history}");
                assert_filename_line_buffer_bytes(&bash, &sushline, expected, &context);
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_quotes_double_quotes_like_bash_5_3() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probe f95df956446e48ed8734e1dde62a127e and
    // exact spelling probe b1de0c6b21cd4067addb3a3789994245 under LC_ALL=C.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "internal",
            b"safe-quo\"te".as_slice(),
            [
                b"safe-quo\\\"te ".as_slice(),
                b"'safe-quo\"te' ".as_slice(),
                b"\"safe-quo\\\"te\" ".as_slice(),
            ],
        ),
        (
            "trailing",
            b"safe-trail\"".as_slice(),
            [
                b"safe-trail\\\" ".as_slice(),
                b"'safe-trail\"' ".as_slice(),
                b"\"safe-trail\\\"\" ".as_slice(),
            ],
        ),
    ];
    let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;

    for (fixture, basename, expected) in fixtures {
        let dir = root.path().join(fixture);
        fs::create_dir(&dir).expect("create double-quote fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write double-quote fixture");
        for ((form, prefix), expected) in [
            ("unquoted", b"safe-".as_slice()),
            ("single-quoted", b"'safe-".as_slice()),
            ("double-quoted", b"\"safe-".as_slice()),
        ]
        .into_iter()
        .zip(expected)
        {
            let mut bash_keys = prefix.to_vec();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = prefix.to_vec();
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) =
                run_filename_line_buffer_hex_pair(&dir, inputrc, &bash_keys, &sushline_keys, "");
            let context = format!("{fixture}/{form} double-quote completion");
            assert_filename_line_buffer_bytes(&bash, &sushline, expected, &context);
        }
    }
}

#[cfg(unix)]
#[test]
fn complete_filename_preserves_control_bytes_in_unquoted_and_quoted_contexts_like_bash_5_3() {
    use portable_pty::{CommandBuilder, PtySize};
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    // Runtime-only Bash 5.3 PTY probes ac9449bb1c7a447f93437deaab2e77b1,
    // 496db5f36c334a9497d23d391d1f1852, eb8c02ebbbf74beb86788e6edb3a6719,
    // 3149c58d018b4cd0babdaf7fe6d69c7f, and b73735c10b82474098cbf5745cd0c2e9.
    // Capture Bash's live line buffer and Sushline's accepted bytes as hex.
    let root = tempfile::tempdir().expect("temp completion root");
    let fixtures = [
        (
            "tab",
            b"safe-tab\titem".as_slice(),
            b"safe-tab\\\titem ".as_slice(),
        ),
        (
            "lf",
            b"safe-lf\nitem".as_slice(),
            b"'safe-lf\nitem' ".as_slice(),
        ),
        (
            "esc",
            b"safe-esc\x1b[2J".as_slice(),
            b"safe-esc\x1b\\[2J ".as_slice(),
        ),
        (
            "del",
            b"safe-del\x7f".as_slice(),
            b"safe-del\x7f ".as_slice(),
        ),
        (
            "cr",
            b"safe-cr\ritem".as_slice(),
            b"safe-cr\ritem ".as_slice(),
        ),
        (
            "bel",
            b"safe-bel\x07item".as_slice(),
            b"safe-bel\x07item ".as_slice(),
        ),
        (
            "backspace",
            b"safe-bs\x08item".as_slice(),
            b"safe-bs\x08item ".as_slice(),
        ),
        (
            "vertical-tab",
            b"safe-vt\x0bitem".as_slice(),
            b"safe-vt\x0bitem ".as_slice(),
        ),
        (
            "form-feed",
            b"safe-ff\x0citem".as_slice(),
            b"safe-ff\x0citem ".as_slice(),
        ),
    ];
    let bash_script = r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; bind -x '"\C-g": printf "BASH_READLINE_HEX:"; printf "%s" "$READLINE_LINE" | od -An -tx1 | tr -d " \n"; printf "\n"'; IFS= read -r -e -p "SUSHLINE_READY>" line; :"#;

    for (control_kind, basename, expected_unquoted) in fixtures {
        let dir = root.path().join(control_kind);
        fs::create_dir(&dir).expect("create control-byte fixture directory");
        fs::write(dir.join(OsString::from_vec(basename.to_vec())), "")
            .expect("write control-byte fixture");
        let inputrc = r#""\C-o": complete-filename
set expand-tilde off
set mark-directories off
set completion-query-items 999
set page-completions off"#;
        let inputrc_path = dir.join("inputrc");
        fs::write(&inputrc_path, inputrc).expect("write inputrc");

        for (quote_kind, quote_byte) in [
            ("unquoted", None),
            ("single", Some(b'\'')),
            ("double", Some(b'"')),
        ] {
            let mut typed = Vec::new();
            if let Some(quote_byte) = quote_byte {
                typed.push(quote_byte);
            }
            typed.extend_from_slice(b"safe-");
            let mut bash_keys = typed.clone();
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            let mut sushline_keys = typed;
            sushline_keys.extend_from_slice(b"\x0f\r");

            let mut bash_command = CommandBuilder::new("bash");
            bash_command.env("INPUTRC", inputrc_path.to_string_lossy().as_ref());
            bash_command.env("LC_ALL", "C");
            bash_command.args(["--noprofile", "--norc", "-i", "-c", bash_script]);
            let bash = run_pty_in_dir_with_size_raw_until(
                bash_command,
                &bash_keys,
                &dir,
                PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                },
                "BASH_READLINE_HEX:",
            );

            let mut sushline_command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
            sushline_command.env("SUSHLINE_INPUTRC", inputrc);
            sushline_command.env("SUSHLINE_PROMPT", READY_PROMPT);
            sushline_command.env("SUSHLINE_HISTORY", "");
            sushline_command.env("SUSHLINE_ACCEPTED_HEX", "1");
            sushline_command.env("LC_ALL", "C");
            let sushline = run_pty_in_dir_with_size_raw_until(
                sushline_command,
                &sushline_keys,
                &dir,
                PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                },
                "SUSHLINE_ACCEPTED_HEX:",
            );

            let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:");
            let sushline_hex = raw_accepted_hex_marker(&sushline);
            assert!(
                bash_hex.is_some() && bash_hex == sushline_hex,
                "line-buffer hex mismatch for control={control_kind}, quote={quote_kind}"
            );
            let expected = if let Some(quote_byte) = quote_byte {
                let mut expected = vec![quote_byte];
                expected.extend_from_slice(basename);
                expected.push(quote_byte);
                expected.push(b' ');
                expected
            } else {
                expected_unquoted.to_vec()
            };
            assert!(
                raw_accepted_hex(&sushline).as_deref() == Some(expected.as_slice()),
                "accepted bytes mismatch for control={control_kind}, quote={quote_kind}"
            );
            if control_kind != "bel" {
                assert!(
                    raw_bell_count(&bash) == 0 && raw_bell_count(&sushline) == 0,
                    "unexpected completion bell for control={control_kind}, quote={quote_kind}"
                );
            }
        }
    }
}

#[cfg(unix)]
fn run_filename_line_buffer_hex_pair(
    cwd: &std::path::Path,
    inputrc: &str,
    bash_keys: &[u8],
    sushline_keys: &[u8],
    bash_setup: &str,
) -> (Vec<u8>, Vec<u8>) {
    run_filename_line_buffer_hex_pair_with_env(
        cwd,
        inputrc,
        bash_keys,
        sushline_keys,
        bash_setup,
        &[],
    )
}

#[cfg(unix)]
fn run_filename_line_buffer_hex_pair_with_env(
    cwd: &std::path::Path,
    inputrc: &str,
    bash_keys: &[u8],
    sushline_keys: &[u8],
    bash_setup: &str,
    env: &[(&str, &str)],
) -> (Vec<u8>, Vec<u8>) {
    use portable_pty::{CommandBuilder, PtySize};

    let inputrc_path = cwd.join("inputrc");
    fs::write(&inputrc_path, inputrc).expect("write inputrc");
    let bash_script = format!(
        "{}{}{}",
        bash_setup,
        if bash_setup.is_empty() { "" } else { "; " },
        r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; bind -x '"\C-g": printf "BASH_READLINE_HEX:"; printf "%s" "$READLINE_LINE" | od -An -tx1 | tr -d " \n"; printf "\n"'; IFS= read -r -e -p "SUSHLINE_READY>" line; :"#
    );
    let mut bash_command = CommandBuilder::new("bash");
    bash_command.env("INPUTRC", inputrc_path.to_string_lossy().as_ref());
    bash_command.env("LC_ALL", "C");
    for (name, value) in env {
        bash_command.env(name, value);
    }
    bash_command.args(["--noprofile", "--norc", "-i", "-c", &bash_script]);

    let mut sushline_command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    sushline_command.env("SUSHLINE_INPUTRC", inputrc);
    sushline_command.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline_command.env("SUSHLINE_HISTORY", "");
    sushline_command.env("SUSHLINE_ACCEPTED_HEX", "1");
    sushline_command.env("LC_ALL", "C");
    for (name, value) in env {
        sushline_command.env(name, value);
    }

    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };
    let bash = run_pty_in_dir_with_size_raw_until(
        bash_command,
        bash_keys,
        cwd,
        size,
        "BASH_READLINE_HEX:",
    );
    let sushline = run_pty_in_dir_with_size_raw_until(
        sushline_command,
        sushline_keys,
        cwd,
        size,
        "SUSHLINE_ACCEPTED_HEX:",
    );
    (bash, sushline)
}

#[cfg(unix)]
fn assert_filename_line_buffer_bytes(bash: &[u8], sushline: &[u8], expected: &[u8], context: &str) {
    let bash_hex = raw_line_after_marker(bash, b"BASH_READLINE_HEX:");
    let sushline_hex = raw_accepted_hex_marker(sushline);
    assert!(
        bash_hex.is_some() && bash_hex == sushline_hex,
        "Bash and Sushline line-buffer hex differed for {context}"
    );
    assert!(
        raw_accepted_hex(sushline).as_deref() == Some(expected),
        "accepted bytes differed for {context}"
    );
    assert!(
        raw_bell_count(bash) == 0 && raw_bell_count(sushline) == 0,
        "unexpected completion bell for {context}"
    );
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
fn menu_completion_consumes_ansi_c_quote_suffixes_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probes 6337f4fb9e244a6e952505d10e822711,
    // cfd0399fccaf49ba95e23bdd60e847a7, d50fd14b11104698aece7ecc8f08e661,
    // cd46d8b7af5b42b1ad46f0e2288609d4, and 2caacd23ca3c42c388894d95117a3ab1.
    struct MenuCompletionCase {
        line: &'static [u8],
        command: &'static str,
        count: usize,
        left_count: usize,
        expected: &'static [u8],
        bells: usize,
    }
    let cases = [
        MenuCompletionCase {
            line: b"echo $'single space-file'",
            command: "menu-complete",
            count: 1,
            left_count: 1,
            expected: b"echo $'single space-file' ",
            bells: 0,
        },
        MenuCompletionCase {
            line: b"echo $'single space-file'",
            command: "menu-complete-backward",
            count: 1,
            left_count: 1,
            expected: b"echo $'single space-file' ",
            bells: 0,
        },
        MenuCompletionCase {
            line: b"echo $'multi space-'",
            command: "menu-complete",
            count: 1,
            left_count: 1,
            expected: b"echo $'multi space-one' ",
            bells: 0,
        },
        MenuCompletionCase {
            line: b"echo $'multi space-'",
            command: "menu-complete",
            count: 2,
            left_count: 1,
            expected: b"echo $'multi space-two' ",
            bells: 0,
        },
        MenuCompletionCase {
            line: b"echo $'multi space-'",
            command: "menu-complete",
            count: 3,
            left_count: 1,
            expected: b"echo $'multi space-",
            bells: 1,
        },
        MenuCompletionCase {
            line: b"echo $'multi space-'",
            command: "menu-complete-backward",
            count: 1,
            left_count: 1,
            expected: b"echo $'multi space-two' ",
            bells: 0,
        },
        MenuCompletionCase {
            line: b"echo $'dir '",
            command: "menu-complete",
            count: 1,
            left_count: 1,
            expected: b"echo $'dir one'/",
            bells: 0,
        },
        MenuCompletionCase {
            line: b"echo $'dir '",
            command: "menu-complete-backward",
            count: 1,
            left_count: 1,
            expected: b"echo $'dir two'/",
            bells: 0,
        },
        MenuCompletionCase {
            line: b"echo $'dir '/",
            command: "menu-complete",
            count: 1,
            left_count: 2,
            expected: b"echo $'dir one'/",
            bells: 0,
        },
        MenuCompletionCase {
            line: b"echo $'dir '/",
            command: "menu-complete-backward",
            count: 1,
            left_count: 2,
            expected: b"echo $'dir two'/",
            bells: 0,
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("single space-file"), b"").expect("create fixture file");
    for name in ["multi space-one", "multi space-two"] {
        fs::write(cwd.join(name), b"").expect("create fixture file");
    }
    for name in ["dir one", "dir two"] {
        fs::create_dir(cwd.join(name)).expect("create fixture directory");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for (index, case) in cases.iter().enumerate() {
        let inputrc = format!(
            "\"\\C-o\": {}\nset show-all-if-ambiguous off\nset menu-complete-display-prefix off\nset mark-directories on\nset page-completions off\nset completion-query-items 999",
            case.command
        );
        let mut bash_keys = case.line.to_vec();
        let mut sushline_keys = case.line.to_vec();
        for _ in 0..case.left_count {
            bash_keys.push(0x02);
            sushline_keys.push(0x02);
        }
        for _ in 0..case.count {
            bash_keys.push(0x0f);
            sushline_keys.push(0x0f);
        }
        bash_keys.extend_from_slice(b"\x07\r");
        sushline_keys.push(b'\r');
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            &inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:");
        let sushline_hex = raw_accepted_hex_marker(&sushline);
        assert!(
            bash_hex.is_some() && bash_hex == sushline_hex,
            "line-buffer hex differed for ANSI-C menu case {index}"
        );
        assert_eq!(
            raw_accepted_hex(&sushline).as_deref(),
            Some(case.expected),
            "accepted bytes differed for ANSI-C menu case {index}"
        );
        assert_eq!(raw_bell_count(&bash), case.bells);
        assert_eq!(raw_bell_count(&sushline), case.bells);
    }
}

#[cfg(unix)]
#[test]
fn menu_completion_handles_ansi_c_filename_tails_like_bash_5_3() {
    // Runtime-only Bash 5.3 PTY probe eb68da770876498696a8406e27d8be8a.
    struct MenuTailCase {
        line: &'static [u8],
        command: &'static str,
        skip_completed_text: &'static str,
        left_count: usize,
        expected: &'static [u8],
    }
    let cases = [
        MenuTailCase {
            line: b"echo $'only space-dir/leaf-file'",
            command: "menu-complete",
            skip_completed_text: "on",
            left_count: 5,
            expected: b"echo $'only space-dir/leaf-file' ",
        },
        MenuTailCase {
            line: b"echo $'only space-dir/leaf-file'",
            command: "menu-complete",
            skip_completed_text: "off",
            left_count: 5,
            expected: b"echo $'only space-dir/leaf-file'file'",
        },
        MenuTailCase {
            line: b"echo $'only space-dir/leaf-file'",
            command: "menu-complete-backward",
            skip_completed_text: "on",
            left_count: 5,
            expected: b"echo $'only space-dir/leaf-file' ",
        },
        MenuTailCase {
            line: b"echo $'only space-dir/leaf-file'",
            command: "menu-complete-backward",
            skip_completed_text: "off",
            left_count: 5,
            expected: b"echo $'only space-dir/leaf-file'file'",
        },
        MenuTailCase {
            line: b"echo $'multi space-dir/leaf-one'",
            command: "menu-complete",
            skip_completed_text: "on",
            left_count: 4,
            expected: b"echo $'multi space-dir/leaf-one' ",
        },
        MenuTailCase {
            line: b"echo $'multi space-dir/leaf-one'",
            command: "menu-complete",
            skip_completed_text: "off",
            left_count: 4,
            expected: b"echo $'multi space-dir/leaf-one'one'",
        },
        MenuTailCase {
            line: b"echo $'multi space-dir/leaf-one'",
            command: "menu-complete-backward",
            skip_completed_text: "on",
            left_count: 4,
            expected: b"echo $'multi space-dir/leaf-two'one'",
        },
        MenuTailCase {
            line: b"echo $'multi space-dir/leaf-one'",
            command: "menu-complete-backward",
            skip_completed_text: "off",
            left_count: 4,
            expected: b"echo $'multi space-dir/leaf-two'one'",
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::create_dir(cwd.join("only space-dir")).expect("create fixture directory");
    fs::write(cwd.join("only space-dir/leaf-file"), b"").expect("create fixture file");
    fs::create_dir(cwd.join("multi space-dir")).expect("create fixture directory");
    for name in ["leaf-one", "leaf-two"] {
        fs::write(cwd.join("multi space-dir").join(name), b"").expect("create fixture file");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for (index, case) in cases.iter().enumerate() {
        let inputrc = format!(
            "\"\\C-o\": {}\nset skip-completed-text {}\nset show-all-if-ambiguous off\nset menu-complete-display-prefix off\nset mark-directories on\nset page-completions off\nset completion-query-items 999",
            case.command, case.skip_completed_text
        );
        let mut bash_keys = case.line.to_vec();
        let mut sushline_keys = case.line.to_vec();
        for _ in 0..case.left_count {
            bash_keys.push(0x02);
            sushline_keys.push(0x02);
        }
        bash_keys.extend_from_slice(b"\x0f\x07\r");
        sushline_keys.extend_from_slice(b"\x0f\r");
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            &inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let bash_hex = raw_line_after_marker(&bash, b"BASH_READLINE_HEX:");
        let sushline_hex = raw_accepted_hex_marker(&sushline);
        assert!(
            bash_hex.is_some() && bash_hex == sushline_hex,
            "line-buffer hex differed for ANSI-C menu-tail case {index}"
        );
        assert_eq!(
            raw_accepted_hex(&sushline).as_deref(),
            Some(case.expected),
            "accepted bytes differed for ANSI-C menu-tail case {index}"
        );
        assert_eq!(raw_bell_count(&bash), 0);
        assert_eq!(raw_bell_count(&sushline), 0);
    }
}

#[cfg(unix)]
#[test]
fn menu_completion_cycles_ansi_c_filename_tails_like_bash_5_3() {
    // Runtime-only Bash 5.3 observations for this fixture, cursor position,
    // menu direction, number of actions, and skip-completed-text setting.
    struct MenuCycleCase {
        command: &'static str,
        skip_completed_text: &'static str,
        actions: usize,
        expected: &'static [u8],
    }
    let cases = [
        MenuCycleCase {
            command: "menu-complete",
            skip_completed_text: "on",
            actions: 1,
            expected: b"echo $'multi space-dir/leaf-one' ",
        },
        MenuCycleCase {
            command: "menu-complete",
            skip_completed_text: "on",
            actions: 2,
            expected: b"echo $'multi space-dir/leaf-two' ",
        },
        MenuCycleCase {
            command: "menu-complete",
            skip_completed_text: "off",
            actions: 1,
            expected: b"echo $'multi space-dir/leaf-one'one'",
        },
        MenuCycleCase {
            command: "menu-complete",
            skip_completed_text: "off",
            actions: 2,
            expected: b"echo $'multi space-dir/leaf-two'one'",
        },
        MenuCycleCase {
            command: "menu-complete-backward",
            skip_completed_text: "on",
            actions: 1,
            expected: b"echo $'multi space-dir/leaf-two'one'",
        },
        MenuCycleCase {
            command: "menu-complete-backward",
            skip_completed_text: "on",
            actions: 2,
            expected: b"echo $'multi space-dir/leaf-one'one'",
        },
        MenuCycleCase {
            command: "menu-complete-backward",
            skip_completed_text: "off",
            actions: 1,
            expected: b"echo $'multi space-dir/leaf-two'one'",
        },
        MenuCycleCase {
            command: "menu-complete-backward",
            skip_completed_text: "off",
            actions: 2,
            expected: b"echo $'multi space-dir/leaf-one'one'",
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::create_dir(cwd.join("multi space-dir")).expect("create fixture directory");
    for name in ["leaf-one", "leaf-two"] {
        fs::write(cwd.join("multi space-dir").join(name), b"").expect("create fixture file");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for (index, case) in cases.iter().enumerate() {
        let inputrc = format!(
            "\"\\C-o\": {}\nset skip-completed-text {}\nset show-all-if-ambiguous off\nset menu-complete-display-prefix off\nset mark-directories on\nset page-completions off\nset completion-query-items 999",
            case.command, case.skip_completed_text
        );
        let mut bash_keys = b"echo $'multi space-dir/leaf-one'".to_vec();
        let mut sushline_keys = bash_keys.clone();
        for _ in 0..4 {
            bash_keys.push(0x02);
            sushline_keys.push(0x02);
        }
        for _ in 0..case.actions {
            bash_keys.push(0x0f);
            sushline_keys.push(0x0f);
        }
        bash_keys.extend_from_slice(b"\x07\r");
        sushline_keys.push(b'\r');
        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
            &cwd,
            &inputrc,
            &bash_keys,
            &sushline_keys,
            "set +H",
            &env,
        );
        let context = format!(
            "menu cycle case {index}: command={}, skip-completed-text={}, actions={}",
            case.command, case.skip_completed_text, case.actions
        );
        assert_filename_line_buffer_bytes(&bash, &sushline, case.expected, &context);
        assert_eq!(raw_bell_count(&bash), 0, "{context}");
        assert_eq!(raw_bell_count(&sushline), 0, "{context}");
    }
}

#[cfg(unix)]
#[test]
fn menu_completion_skips_filename_suffix_only_after_exact_prefix_like_bash_5_3() {
    // Runtime-only Bash 5.3 observations for these files, prefix spellings,
    // quote forms, and option combinations; not source proof.
    let quotes = [
        ("single", b"'".as_slice(), b'\''),
        ("double", b"\"".as_slice(), b'\"'),
        ("ansi-c", b"$'".as_slice(), b'\''),
    ];
    let prefixes = [
        ("exact", b"safe-Al".as_slice(), true),
        ("case-different", b"safe-al".as_slice(), false),
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    for name in ["safe-Alpha", "safe-Alpine"] {
        fs::write(cwd.join(name), b"").expect("create fixture file");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for (quote_name, opening, closing) in &quotes {
        for (prefix_name, prefix, exact_prefix) in &prefixes {
            for ignore_case in ["off", "on"] {
                if !exact_prefix && ignore_case == "off" {
                    continue;
                }
                for skip_completed_text in ["off", "on"] {
                    let inputrc = format!(
                        "\"\\C-o\": menu-complete\nset completion-ignore-case {ignore_case}\nset skip-completed-text {skip_completed_text}\nset menu-complete-display-prefix off\nset show-all-if-ambiguous off\nset page-completions off\nset completion-query-items 999"
                    );
                    let mut line = b"echo ".to_vec();
                    line.extend_from_slice(opening);
                    line.extend_from_slice(prefix);
                    line.extend_from_slice(b"pha");
                    line.push(*closing);
                    let mut bash_keys = line.clone();
                    let mut sushline_keys = line;
                    for _ in 0..4 {
                        bash_keys.push(0x02);
                        sushline_keys.push(0x02);
                    }
                    bash_keys.extend_from_slice(b"\x0f\x07\r");
                    sushline_keys.extend_from_slice(b"\x0f\r");

                    let mut expected = b"echo ".to_vec();
                    expected.extend_from_slice(opening);
                    expected.extend_from_slice(b"safe-Alpha");
                    if skip_completed_text == "off" || !exact_prefix {
                        expected.extend_from_slice(b"pha");
                    }
                    expected.push(*closing);
                    let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                        &cwd,
                        &inputrc,
                        &bash_keys,
                        &sushline_keys,
                        "set +H",
                        &env,
                    );
                    assert_filename_line_buffer_bytes(
                        &bash,
                        &sushline,
                        &expected,
                        &format!(
                            "menu suffix quote={quote_name} prefix={prefix_name} ignore-case={ignore_case} skip={skip_completed_text}"
                        ),
                    );
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn multi_match_backward_and_old_menu_suffix_overlap_match_bash_5_3() {
    // Runtime-only Bash 5.3 observations for the backward and old menu actions;
    // the selected candidate is deliberately first/last in its sorted fixture.
    let quotes = [
        ("single", b"'".as_slice(), b'\''),
        ("double", b"\"".as_slice(), b'\"'),
        ("ansi-c", b"$'".as_slice(), b'\''),
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let home = root.path().join("home");
    fs::create_dir(&home).expect("create fixture HOME");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for (command, filenames, expected_candidate) in [
        (
            "menu-complete-backward",
            ["safe-Zph0", "safe-Zpha"],
            "safe-Zpha",
        ),
        (
            "old-menu-complete",
            ["safe-Alpha", "safe-Alpine"],
            "safe-Alpha",
        ),
    ] {
        let cwd = root.path().join(command);
        fs::create_dir(&cwd).expect("create fixture cwd");
        for filename in filenames {
            fs::write(cwd.join(filename), b"").expect("write fixture file");
        }
        let (exact_prefix, case_different_prefix) = if command == "old-menu-complete" {
            (b"safe-Al".as_slice(), b"safe-al".as_slice())
        } else {
            (b"safe-Z".as_slice(), b"safe-z".as_slice())
        };
        let prefixes = [
            ("exact", exact_prefix, true),
            ("case-different", case_different_prefix, false),
        ];

        for (quote_name, opening, closing) in &quotes {
            for (prefix_name, prefix, exact_prefix) in &prefixes {
                for ignore_case in ["off", "on"] {
                    if !exact_prefix && ignore_case == "off" {
                        continue;
                    }
                    for skip_completed_text in ["off", "on"] {
                        let inputrc = format!(
                            "\"\\C-o\": {command}\nset completion-ignore-case {ignore_case}\nset skip-completed-text {skip_completed_text}\nset menu-complete-display-prefix off\nset show-all-if-ambiguous off\nset page-completions off\nset completion-query-items 999"
                        );
                        let mut line = b"echo ".to_vec();
                        line.extend_from_slice(opening);
                        line.extend_from_slice(prefix);
                        line.extend_from_slice(b"pha");
                        line.push(*closing);
                        let mut bash_keys = line.clone();
                        let mut sushline_keys = line;
                        for _ in 0..4 {
                            bash_keys.push(0x02);
                            sushline_keys.push(0x02);
                        }
                        bash_keys.extend_from_slice(b"\x0f\x07\r");
                        sushline_keys.extend_from_slice(b"\x0f\r");

                        let mut expected = b"echo ".to_vec();
                        expected.extend_from_slice(opening);
                        expected.extend_from_slice(expected_candidate.as_bytes());
                        if skip_completed_text == "off" || !exact_prefix {
                            expected.extend_from_slice(b"pha");
                        }
                        expected.push(*closing);
                        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                            &cwd,
                            &inputrc,
                            &bash_keys,
                            &sushline_keys,
                            "set +H",
                            &env,
                        );
                        assert_filename_line_buffer_bytes(
                            &bash,
                            &sushline,
                            &expected,
                            &format!(
                                "multi-match {command} suffix quote={quote_name} prefix={prefix_name} ignore-case={ignore_case} skip={skip_completed_text}"
                            ),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn single_match_menu_actions_preserve_case_insensitive_suffix_like_bash_5_3() {
    // Runtime-only Bash 5.3 observations for these menu actions, quote forms,
    // prefix spellings, and option combinations; not source proof.
    let quotes = [
        ("single", b"'".as_slice(), b'\''),
        ("double", b"\"".as_slice(), b'\"'),
        ("ansi-c", b"$'".as_slice(), b'\''),
    ];
    let prefixes = [
        ("exact", b"safe-Al".as_slice(), true),
        ("case-different", b"safe-al".as_slice(), false),
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    fs::write(cwd.join("safe-Alpha"), b"").expect("write fixture file");
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in [
        "menu-complete",
        "menu-complete-backward",
        "old-menu-complete",
    ] {
        for (quote_name, opening, closing) in &quotes {
            for (prefix_name, prefix, exact_prefix) in &prefixes {
                for ignore_case in ["off", "on"] {
                    if !exact_prefix && ignore_case == "off" {
                        continue;
                    }
                    for skip_completed_text in ["off", "on"] {
                        let inputrc = format!(
                            "\"\\C-o\": {command}\nset completion-ignore-case {ignore_case}\nset skip-completed-text {skip_completed_text}\nset menu-complete-display-prefix off\nset show-all-if-ambiguous off\nset page-completions off\nset completion-query-items 999"
                        );
                        let mut line = b"echo ".to_vec();
                        line.extend_from_slice(opening);
                        line.extend_from_slice(prefix);
                        line.extend_from_slice(b"pha");
                        line.push(*closing);
                        let mut bash_keys = line.clone();
                        let mut sushline_keys = line;
                        for _ in 0..4 {
                            bash_keys.push(0x02);
                            sushline_keys.push(0x02);
                        }
                        bash_keys.extend_from_slice(b"\x0f\x07\r");
                        sushline_keys.extend_from_slice(b"\x0f\r");

                        let mut expected = b"echo ".to_vec();
                        expected.extend_from_slice(opening);
                        expected.extend_from_slice(b"safe-Alpha");
                        if skip_completed_text == "off" || !exact_prefix {
                            expected.extend_from_slice(b"pha");
                        }
                        expected.push(*closing);
                        let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                            &cwd,
                            &inputrc,
                            &bash_keys,
                            &sushline_keys,
                            "set +H",
                            &env,
                        );
                        assert_filename_line_buffer_bytes(
                            &bash,
                            &sushline,
                            &expected,
                            &format!(
                                "single-match {command} suffix quote={quote_name} prefix={prefix_name} ignore-case={ignore_case} skip={skip_completed_text}"
                            ),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn menu_completion_appends_quoted_filename_separators_like_bash_5_3() {
    // Runtime-only Bash 5.3 observations for these filenames, quote forms,
    // cursor positions, and first menu-completion action; not source proof.
    struct Case {
        line: &'static [u8],
        left_count: usize,
        expected: &'static [u8],
    }
    let cases = [
        Case {
            line: b"echo plain-Al",
            left_count: 0,
            expected: b"echo plain-Alpha ",
        },
        Case {
            line: b"echo 'plain-Al'",
            left_count: 1,
            expected: b"echo 'plain-Alpha'",
        },
        Case {
            line: b"echo \"plain-Al\"",
            left_count: 1,
            expected: b"echo \"plain-Alpha\"",
        },
        Case {
            line: b"echo $'plain-Al'",
            left_count: 1,
            expected: b"echo $'plain-Alpha'",
        },
        Case {
            line: b"echo 'space A'",
            left_count: 1,
            expected: b"echo 'space Alpha' ",
        },
        Case {
            line: b"echo \"space A\"",
            left_count: 1,
            expected: b"echo \"space Alpha\" ",
        },
        Case {
            line: b"echo $'space A'",
            left_count: 1,
            expected: b"echo $'space Alpha' ",
        },
        Case {
            line: b"echo 'meta;Al'",
            left_count: 1,
            expected: b"echo 'meta;Alpha' ",
        },
        Case {
            line: b"echo \"meta;Al\"",
            left_count: 1,
            expected: b"echo \"meta;Alpha\" ",
        },
        Case {
            line: b"echo $'meta;Al'",
            left_count: 1,
            expected: b"echo $'meta;Alpha' ",
        },
        Case {
            line: b"echo 'only-Al'",
            left_count: 1,
            expected: b"echo 'only-Alpha'",
        },
        Case {
            line: b"echo \"only-Al\"",
            left_count: 1,
            expected: b"echo \"only-Alpha\"",
        },
        Case {
            line: b"echo $'only-Al'",
            left_count: 1,
            expected: b"echo $'only-Alpha'",
        },
        Case {
            line: b"echo 'solo space A'",
            left_count: 1,
            expected: b"echo 'solo space Alpha' ",
        },
        Case {
            line: b"echo \"solo space A\"",
            left_count: 1,
            expected: b"echo \"solo space Alpha\" ",
        },
        Case {
            line: b"echo $'solo space A'",
            left_count: 1,
            expected: b"echo $'solo space Alpha' ",
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    for name in [
        "plain-Alpha",
        "plain-Alpine",
        "space Alpha",
        "space Alpine",
        "meta;Alpha",
        "meta;Alpine",
        "only-Alpha",
        "solo space Alpha",
    ] {
        fs::write(cwd.join(name), b"").expect("create fixture file");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for skip_completed_text in ["on", "off"] {
        let inputrc = format!(
            "\"\\C-o\": menu-complete\nset skip-completed-text {skip_completed_text}\nset menu-complete-display-prefix off\nset show-all-if-ambiguous off\nset page-completions off\nset completion-query-items 999"
        );
        for (index, case) in cases.iter().enumerate() {
            let mut bash_keys = case.line.to_vec();
            let mut sushline_keys = case.line.to_vec();
            for _ in 0..case.left_count {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                case.expected,
                &format!("quoted menu filename separator skip={skip_completed_text} case={index}"),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn menu_completion_preserves_existing_quoted_suffixes_like_bash_5_3() {
    // Runtime-only Bash 5.3 observations for these files, cursor offsets, and
    // first menu-completion action; not source proof or a general claim.
    struct Case {
        line: &'static [u8],
        expected_skip_on: &'static [u8],
        expected_skip_off: &'static [u8],
    }
    let cases = [
        Case {
            line: b"echo 'tail-Alpha'",
            expected_skip_on: b"echo 'tail-Alpha'",
            expected_skip_off: b"echo 'tail-Alphapha'",
        },
        Case {
            line: b"echo \"tail-Alpha\"",
            expected_skip_on: b"echo \"tail-Alpha\"",
            expected_skip_off: b"echo \"tail-Alphapha\"",
        },
        Case {
            line: b"echo $'tail-Alpha'",
            expected_skip_on: b"echo $'tail-Alpha'",
            expected_skip_off: b"echo $'tail-Alphapha'",
        },
        Case {
            line: b"echo 'tail space-Alpha'",
            expected_skip_on: b"echo 'tail space-Alpha' ",
            expected_skip_off: b"echo 'tail space-Alpha'pha'",
        },
        Case {
            line: b"echo \"tail space-Alpha\"",
            expected_skip_on: b"echo \"tail space-Alpha\" ",
            expected_skip_off: b"echo \"tail space-Alpha\"pha\"",
        },
        Case {
            line: b"echo $'tail space-Alpha'",
            expected_skip_on: b"echo $'tail space-Alpha' ",
            expected_skip_off: b"echo $'tail space-Alpha'pha'",
        },
        Case {
            line: b"echo 'unique-Alpha'",
            expected_skip_on: b"echo 'unique-Alpha'",
            expected_skip_off: b"echo 'unique-Alphapha'",
        },
        Case {
            line: b"echo \"unique-Alpha\"",
            expected_skip_on: b"echo \"unique-Alpha\"",
            expected_skip_off: b"echo \"unique-Alphapha\"",
        },
        Case {
            line: b"echo $'unique-Alpha'",
            expected_skip_on: b"echo $'unique-Alpha'",
            expected_skip_off: b"echo $'unique-Alphapha'",
        },
        Case {
            line: b"echo 'unique space-Alpha'",
            expected_skip_on: b"echo 'unique space-Alpha' ",
            expected_skip_off: b"echo 'unique space-Alpha'pha'",
        },
        Case {
            line: b"echo \"unique space-Alpha\"",
            expected_skip_on: b"echo \"unique space-Alpha\" ",
            expected_skip_off: b"echo \"unique space-Alpha\"pha\"",
        },
        Case {
            line: b"echo $'unique space-Alpha'",
            expected_skip_on: b"echo $'unique space-Alpha' ",
            expected_skip_off: b"echo $'unique space-Alpha'pha'",
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    for name in [
        "tail-Alpha",
        "tail-Alpine",
        "tail space-Alpha",
        "tail space-Alpine",
        "unique-Alpha",
        "unique space-Alpha",
    ] {
        fs::write(cwd.join(name), b"").expect("create fixture file");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for skip_completed_text in ["on", "off"] {
        let inputrc = format!(
            "\"\\C-o\": menu-complete\nset skip-completed-text {skip_completed_text}\nset menu-complete-display-prefix off\nset show-all-if-ambiguous off\nset page-completions off\nset completion-query-items 999"
        );
        for (index, case) in cases.iter().enumerate() {
            let mut bash_keys = case.line.to_vec();
            let mut sushline_keys = case.line.to_vec();
            for _ in 0..4 {
                bash_keys.push(0x02);
                sushline_keys.push(0x02);
            }
            bash_keys.extend_from_slice(b"\x0f\x07\r");
            sushline_keys.extend_from_slice(b"\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            let expected = if skip_completed_text == "on" {
                case.expected_skip_on
            } else {
                case.expected_skip_off
            };
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                expected,
                &format!("quoted existing suffix skip={skip_completed_text} case={index}"),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn menu_completion_appends_separators_after_quoted_leading_hash_and_tilde_filenames() {
    // Runtime-only Bash 5.3 observations for these leading bytes, quote forms,
    // cursor positions, and first menu-completion action; not source proof.
    struct Case {
        line: &'static [u8],
        expected: &'static [u8],
    }
    let cases = [
        Case {
            line: b"echo '#sa'",
            expected: b"echo '#safe-alpha' ",
        },
        Case {
            line: b"echo \"#sa\"",
            expected: b"echo \"#safe-alpha\" ",
        },
        Case {
            line: b"echo $'#sa'",
            expected: b"echo $'#safe-alpha' ",
        },
        Case {
            line: b"echo '~sa'",
            expected: b"echo '~safe-alpha' ",
        },
        Case {
            line: b"echo \"~sa\"",
            expected: b"echo \"~safe-alpha\" ",
        },
        Case {
            line: b"echo $'~sa'",
            expected: b"echo $'~safe-alpha' ",
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    for name in ["#safe-alpha", "#same-alpha", "~safe-alpha", "~same-alpha"] {
        fs::write(cwd.join(name), b"").expect("create fixture file");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for skip_completed_text in ["on", "off"] {
        let inputrc = format!(
            "\"\\C-o\": menu-complete\nset skip-completed-text {skip_completed_text}\nset menu-complete-display-prefix off\nset show-all-if-ambiguous off\nset page-completions off\nset completion-query-items 999"
        );
        for (index, case) in cases.iter().enumerate() {
            let mut bash_keys = case.line.to_vec();
            let mut sushline_keys = case.line.to_vec();
            bash_keys.extend_from_slice(b"\x02\x0f\x07\r");
            sushline_keys.extend_from_slice(b"\x02\x0f\r");
            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                &cwd,
                &inputrc,
                &bash_keys,
                &sushline_keys,
                "set +H",
                &env,
            );
            assert_filename_line_buffer_bytes(
                &bash,
                &sushline,
                case.expected,
                &format!("quoted leading #/~ separator skip={skip_completed_text} case={index}"),
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn menu_completion_handles_partial_and_unmatched_quoted_suffixes_like_bash_5_3() {
    // Runtime-only Bash 5.3 observations for these suffixes, quote forms,
    // cursor positions, and first menu-completion action; not source proof.
    struct Quote {
        name: &'static str,
        opening: &'static [u8],
        closing: u8,
    }
    struct Filename {
        name: &'static str,
        typed_prefix: &'static [u8],
        candidate: &'static [u8],
        closes_before_suffix: bool,
    }
    struct Suffix {
        name: &'static str,
        full: &'static [u8],
        remaining_when_skipping: &'static [u8],
    }
    let quotes = [
        Quote {
            name: "single",
            opening: b"'",
            closing: b'\'',
        },
        Quote {
            name: "double",
            opening: b"\"",
            closing: b'\"',
        },
        Quote {
            name: "ansi-c",
            opening: b"$'",
            closing: b'\'',
        },
    ];
    let filenames = [
        Filename {
            name: "plain",
            typed_prefix: b"tail-Al",
            candidate: b"tail-Alpha",
            closes_before_suffix: false,
        },
        Filename {
            name: "space",
            typed_prefix: b"tail space-Al",
            candidate: b"tail space-Alpha",
            closes_before_suffix: true,
        },
    ];
    let suffixes = [
        Suffix {
            name: "partial",
            full: b"phX",
            remaining_when_skipping: b"X",
        },
        Suffix {
            name: "unmatched",
            full: b"ZZ",
            remaining_when_skipping: b"ZZ",
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    for name in [
        "tail-Alpha",
        "tail-Alpine",
        "tail space-Alpha",
        "tail space-Alpine",
    ] {
        fs::write(cwd.join(name), b"").expect("create fixture file");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for skip_completed_text in ["on", "off"] {
        let inputrc = format!(
            "\"\\C-o\": menu-complete\nset skip-completed-text {skip_completed_text}\nset menu-complete-display-prefix off\nset show-all-if-ambiguous off\nset page-completions off\nset completion-query-items 999"
        );
        for quote in &quotes {
            for filename in &filenames {
                for suffix in &suffixes {
                    let mut line = b"echo ".to_vec();
                    line.extend_from_slice(quote.opening);
                    line.extend_from_slice(filename.typed_prefix);
                    line.extend_from_slice(suffix.full);
                    line.push(quote.closing);
                    let mut bash_keys = line.clone();
                    let mut sushline_keys = line;
                    for _ in 0..suffix.full.len() + 1 {
                        bash_keys.push(0x02);
                        sushline_keys.push(0x02);
                    }
                    bash_keys.extend_from_slice(b"\x0f\x07\r");
                    sushline_keys.extend_from_slice(b"\x0f\r");

                    let mut expected = b"echo ".to_vec();
                    expected.extend_from_slice(quote.opening);
                    expected.extend_from_slice(filename.candidate);
                    if filename.closes_before_suffix {
                        expected.push(quote.closing);
                    }
                    expected.extend_from_slice(if skip_completed_text == "on" {
                        suffix.remaining_when_skipping
                    } else {
                        suffix.full
                    });
                    expected.push(quote.closing);
                    let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                        &cwd,
                        &inputrc,
                        &bash_keys,
                        &sushline_keys,
                        "set +H",
                        &env,
                    );
                    assert_filename_line_buffer_bytes(
                        &bash,
                        &sushline,
                        &expected,
                        &format!(
                            "quoted suffix skip={skip_completed_text} quote={} filename={} suffix={}",
                            quote.name, filename.name, suffix.name
                        ),
                    );
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn quoted_directory_slashes_match_bash_5_3_for_completion_commands() {
    // Runtime-only Bash 5.3 observations for these directory names, quote
    // forms, cursor positions, commands, and both completion settings; not source proof.
    struct Quote {
        name: &'static str,
        opening: &'static [u8],
        closing: u8,
    }
    struct Directory {
        name: &'static str,
        typed_prefix: &'static [u8],
        candidate: &'static [u8],
        needs_shell_escape: bool,
    }
    #[derive(Clone, Copy)]
    enum CursorPosition {
        BeforeCloser,
        BeforeSlash,
        OpenQuote,
    }
    struct CursorCase {
        name: &'static str,
        position: CursorPosition,
        left_count: usize,
    }
    let quotes = [
        Quote {
            name: "single",
            opening: b"'",
            closing: b'\'',
        },
        Quote {
            name: "double",
            opening: b"\"",
            closing: b'\"',
        },
        Quote {
            name: "ansi-c",
            opening: b"$'",
            closing: b'\'',
        },
    ];
    let directories = [
        Directory {
            name: "plain",
            typed_prefix: b"plain-d",
            candidate: b"plain-dir",
            needs_shell_escape: false,
        },
        Directory {
            name: "space",
            typed_prefix: b"only spa",
            candidate: b"only space-dir",
            needs_shell_escape: true,
        },
        Directory {
            name: "leading-hash",
            typed_prefix: b"#sa",
            candidate: b"#safe-dir",
            needs_shell_escape: true,
        },
        Directory {
            name: "leading-tilde",
            typed_prefix: b"~sa",
            candidate: b"~safe-dir",
            needs_shell_escape: true,
        },
    ];
    let cursors = [
        CursorCase {
            name: "before-closer",
            position: CursorPosition::BeforeCloser,
            left_count: 1,
        },
        CursorCase {
            name: "before-slash",
            position: CursorPosition::BeforeSlash,
            left_count: 2,
        },
        CursorCase {
            name: "open-quote",
            position: CursorPosition::OpenQuote,
            left_count: 0,
        },
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    for directory in &directories {
        fs::create_dir(cwd.join(std::str::from_utf8(directory.candidate).unwrap()))
            .expect("create fixture directory");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename", "menu-complete"] {
        let skip_values: &[&str] = if command == "menu-complete" {
            &["off"]
        } else {
            &["off", "on"]
        };
        for skip_completed_text in skip_values {
            for mark_directories in ["on", "off"] {
                let inputrc = format!(
                    "\"\\C-o\": {command}\nset mark-directories {mark_directories}\nset skip-completed-text {skip_completed_text}\nset menu-complete-display-prefix off\nset show-all-if-ambiguous off\nset page-completions off\nset completion-query-items 999"
                );
                for quote in &quotes {
                    for directory in &directories {
                        for cursor in &cursors {
                            if *skip_completed_text == "on"
                                && matches!(cursor.position, CursorPosition::OpenQuote)
                            {
                                continue;
                            }
                            let mut line = b"echo ".to_vec();
                            line.extend_from_slice(quote.opening);
                            match cursor.position {
                                CursorPosition::BeforeCloser | CursorPosition::OpenQuote => {
                                    line.extend_from_slice(directory.typed_prefix);
                                }
                                CursorPosition::BeforeSlash => {
                                    line.extend_from_slice(directory.candidate);
                                    line.push(b'/');
                                }
                            }
                            if !matches!(cursor.position, CursorPosition::OpenQuote) {
                                line.push(quote.closing);
                            }
                            let mut bash_keys = line.clone();
                            let mut sushline_keys = line;
                            for _ in 0..cursor.left_count {
                                bash_keys.push(0x02);
                                sushline_keys.push(0x02);
                            }
                            bash_keys.extend_from_slice(b"\x0f\x07\r");
                            sushline_keys.extend_from_slice(b"\x0f\r");

                            let mark_slash = mark_directories == "on";
                            let mut expected = b"echo ".to_vec();
                            expected.extend_from_slice(quote.opening);
                            expected.extend_from_slice(directory.candidate);
                            match cursor.position {
                                CursorPosition::BeforeCloser => {
                                    if mark_slash && directory.needs_shell_escape {
                                        expected.push(quote.closing);
                                        expected.push(b'/');
                                    } else {
                                        if mark_slash {
                                            expected.push(b'/');
                                        }
                                        expected.push(quote.closing);
                                    }
                                }
                                CursorPosition::BeforeSlash => {
                                    if directory.needs_shell_escape {
                                        expected.push(quote.closing);
                                    }
                                    expected.push(b'/');
                                    expected.push(quote.closing);
                                }
                                CursorPosition::OpenQuote => {
                                    if directory.needs_shell_escape {
                                        expected.push(quote.closing);
                                    }
                                    if mark_slash {
                                        expected.push(b'/');
                                    }
                                }
                            }
                            let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                                &cwd,
                                &inputrc,
                                &bash_keys,
                                &sushline_keys,
                                "set +H",
                                &env,
                            );
                            assert_filename_line_buffer_bytes(
                                &bash,
                                &sushline,
                                &expected,
                                &format!(
                                    "quoted directory slash command={command} skip={skip_completed_text} mark={mark_directories} quote={} directory={} cursor={}",
                                    quote.name, directory.name, cursor.name
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn quoted_directory_completion_preserves_adjacent_suffix_bytes_like_bash_5_3() {
    // Runtime-only Bash 5.3 observations for quoted directories followed by
    // an adjacent word suffix; not source proof.
    let quotes = [
        ("single", b"'".as_slice(), b'\''),
        ("double", b"\"".as_slice(), b'\"'),
    ];
    let directories = [
        (
            "plain",
            b"plain-d".as_slice(),
            b"plain-dir".as_slice(),
            false,
        ),
        (
            "space",
            b"only spa".as_slice(),
            b"only space-dir".as_slice(),
            true,
        ),
    ];
    let root = tempfile::tempdir().expect("temp completion root");
    let cwd = root.path().join("cwd");
    let home = root.path().join("home");
    fs::create_dir(&cwd).expect("create fixture cwd");
    fs::create_dir(&home).expect("create fixture HOME");
    for (_, _, candidate, _) in &directories {
        fs::create_dir(cwd.join(std::str::from_utf8(candidate).unwrap()))
            .expect("create fixture directory");
    }
    let home_path = home.to_string_lossy().into_owned();
    let env = [("HOME", home_path.as_str())];

    for command in ["complete", "complete-filename", "menu-complete"] {
        for mark_directories in ["on", "off"] {
            let inputrc = format!(
                "\"\\C-o\": {command}\nset mark-directories {mark_directories}\nset skip-completed-text off\nset menu-complete-display-prefix off\nset show-all-if-ambiguous off\nset page-completions off\nset completion-query-items 999"
            );
            for (quote_name, opening, closing) in &quotes {
                for (directory_name, prefix, candidate, needs_shell_escape) in &directories {
                    let mut line = b"echo ".to_vec();
                    line.extend_from_slice(opening);
                    line.extend_from_slice(prefix);
                    line.push(*closing);
                    line.extend_from_slice(b"tail");
                    let mut bash_keys = line.clone();
                    let mut sushline_keys = line;
                    for _ in 0..5 {
                        bash_keys.push(0x02);
                        sushline_keys.push(0x02);
                    }
                    bash_keys.extend_from_slice(b"\x0f\x07\r");
                    sushline_keys.extend_from_slice(b"\x0f\r");

                    let mut expected = b"echo ".to_vec();
                    expected.extend_from_slice(opening);
                    expected.extend_from_slice(candidate);
                    if mark_directories == "on" && *needs_shell_escape {
                        expected.push(*closing);
                        expected.push(b'/');
                    } else {
                        if mark_directories == "on" {
                            expected.push(b'/');
                        }
                        expected.push(*closing);
                    }
                    expected.extend_from_slice(b"tail");
                    let (bash, sushline) = run_filename_line_buffer_hex_pair_with_env(
                        &cwd,
                        &inputrc,
                        &bash_keys,
                        &sushline_keys,
                        "set +H",
                        &env,
                    );
                    assert_filename_line_buffer_bytes(
                        &bash,
                        &sushline,
                        &expected,
                        &format!(
                            "quoted directory adjacent suffix command={command} mark={mark_directories} quote={quote_name} directory={directory_name}"
                        ),
                    );
                }
            }
        }
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
#[test]
fn bash_readline_and_sushline_complete_special_shell_directory_tilde_paths() {
    // Runtime-only Bash 5.3 PTY evidence from probe 199d89bd17224e0f832c8fb0b88c120f.
    // Keep paths and PTY transcripts out of assertion diagnostics.
    let cwd = tempfile::tempdir().expect("temp cwd");
    let oldpwd = tempfile::tempdir().expect("temp OLDPWD");
    for (directory, filename) in [(cwd.path(), "alpha-item"), (oldpwd.path(), "beta-item")] {
        fs::write(directory.join(filename), "").expect("write tilde completion fixture");
    }
    let pwd_value = cwd.path().display().to_string();
    let oldpwd_value = oldpwd.path().display().to_string();
    let env = [
        ("LC_ALL", "C"),
        ("PWD", pwd_value.as_str()),
        ("OLDPWD", oldpwd_value.as_str()),
    ];

    for (tilde_prefix, directory, filename, partial) in [
        ("~+/", cwd.path(), "alpha-item", "al"),
        ("~-/", oldpwd.path(), "beta-item", "be"),
    ] {
        let typed_word = format!("{tilde_prefix}{partial}");
        let expanded = format!("{}/{filename}", directory.display());
        for form in ["unquoted", "single-quoted", "double-quoted", "escaped"] {
            let typed = match form {
                "single-quoted" => format!("'{typed_word}"),
                "double-quoted" => format!("\"{typed_word}"),
                "escaped" => format!("\\{typed_word}"),
                _ => typed_word.clone(),
            };
            for expand_tilde in ["off", "on"] {
                let expected = match form {
                    "single-quoted" => format!("'{expanded}' "),
                    "double-quoted" => format!("\"{expanded}\" "),
                    "escaped" => typed.clone(),
                    _ if expand_tilde == "off" => format!("{tilde_prefix}{filename} "),
                    _ => format!("{expanded} "),
                };
                let expected_bells = usize::from(form == "escaped");
                let inputrc = format!(
                    "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
                );
                let keys = format!("{typed}\x0f\r");
                let bash = run_bash_readline_with_inputrc_file_and_env_in_dir(
                    keys.as_bytes(),
                    &inputrc,
                    &env,
                    cwd.path(),
                );
                let sushline = run_sushline_harness_with_inputrc_and_env_in_dir(
                    keys.as_bytes(),
                    &inputrc,
                    &env,
                    cwd.path(),
                );

                for (output, implementation) in [(&bash, "Bash"), (&sushline, "Sushline")] {
                    assert!(
                        accepted_line(output).as_deref() == Some(expected.as_str()),
                        "accepted-line mismatch for {form}, prefix={tilde_prefix}, expand-tilde={expand_tilde}, implementation={implementation}"
                    );
                    assert!(
                        bell_count(output) == expected_bells,
                        "bell-count mismatch for {form}, prefix={tilde_prefix}, expand-tilde={expand_tilde}, implementation={implementation}"
                    );
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn bash_readline_and_sushline_validate_pwd_for_shell_directory_tilde_completion() {
    // Runtime-only Bash 5.3 probes 19380ec09f8846328a58a5698bcb371a and
    // 639f95c6daff4d78a2e99976348c5d6e: reject stale PWD, preserve valid symlinks.
    // Keep paths and PTY transcripts out of assertion diagnostics.
    let root = tempfile::tempdir().expect("temp PWD root");
    let cwd = root.path().join("cwd");
    let stale = root.path().join("stale");
    let logical = root.path().join("logical");
    fs::create_dir(&cwd).expect("create cwd fixture");
    fs::create_dir(&stale).expect("create stale PWD fixture");
    std::os::unix::fs::symlink(&cwd, &logical).expect("create logical PWD symlink");
    fs::write(cwd.join("alpha-actual"), "").expect("write cwd completion fixture");
    fs::write(stale.join("alpha-wrong"), "").expect("write stale completion fixture");

    for (pwd_case, pwd, expanded_directory) in [
        ("stale", stale.as_path(), cwd.as_path()),
        ("logical", logical.as_path(), logical.as_path()),
    ] {
        let pwd_value = pwd.display().to_string();
        let env = [("LC_ALL", "C"), ("PWD", pwd_value.as_str())];
        for expand_tilde in ["off", "on"] {
            let inputrc = format!(
                "\"\\C-o\": complete-filename\nset expand-tilde {expand_tilde}\nset mark-directories off\nset completion-query-items 999\nset page-completions off"
            );
            let keys = b"~+/alpha-\x0f\r";
            let bash =
                run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &env, &cwd);
            let sushline =
                run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &env, &cwd);
            let expected = if expand_tilde == "off" {
                "~+/alpha-actual ".to_string()
            } else {
                format!("{}/alpha-actual ", expanded_directory.display())
            };

            for (output, implementation) in [(&bash, "Bash"), (&sushline, "Sushline")] {
                assert!(
                    accepted_line(output).as_deref() == Some(expected.as_str()),
                    "accepted-line mismatch for PWD case={pwd_case}, expand-tilde={expand_tilde}, implementation={implementation}"
                );
                assert!(
                    bell_count(output) == 0,
                    "unexpected bell for PWD case={pwd_case}, expand-tilde={expand_tilde}, implementation={implementation}"
                );
            }
        }
    }
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
