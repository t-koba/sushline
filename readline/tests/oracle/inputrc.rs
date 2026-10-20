//! Inputrc compatibility cases for variables, bindings, conditionals, includes,
//! raw key bytes, and macro expansion. Bash 5.3 PTY observations are limited to
//! the specific parser/configuration fixtures described in `COMPATIBILITY.md`.

use super::pty::*;
use portable_pty::{CommandBuilder, PtySize};
use std::fs;
use std::path::Path;

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
fn bash_readline_and_sushline_report_invalid_set_directives() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(
        &path,
        "set bell-style no-such-style\nset no-such-variable on\n\"\\C-o\": beginning-of-line\n",
    )
    .expect("write inputrc");
    let keys = b"abc\x0fX\r";
    let bash = run_bash_readline_with_inputrc_path(keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(keys, &path);

    for expected in [
        format!(
            "readline: {}: line 1: bell-style: could not set value to `no-such-style'",
            path.display()
        ),
        format!(
            "readline: {}: line 2: no-such-variable: unknown variable name",
            path.display()
        ),
    ] {
        assert!(bash.contains(&expected), "bash={bash}");
        assert!(sushline.contains(&expected), "sushline={sushline}");
    }
    // The binding after the invalid directives still applies.
    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_report_the_same_set_value_diagnostics() {
    // The variables whose values GNU Readline validates, each with the
    // values GNU accepts and rejects, plus the directive spellings and the
    // bare `set` form.
    const VARIABLES: &[&str] = &["bell-style", "comment-begin", "editing-mode", "keymap"];
    const VALUES: &[&str] = &[
        "",
        "on",
        "off",
        "1",
        "0",
        "none",
        "visible",
        "audible",
        "vi",
        "emacs",
        "bogus",
        "bogus extra",
        "NONE",
        // GNU reads a quoted value without its surrounding quotes.
        "\"\"",
        "\"none\"",
        "\"bogus\"",
        "\"vi-insert\"",
        "\"vi-command\"",
        "\"emacs-standard\"",
        "\"##\"",
    ];

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let mut inputrc = String::new();
    for variable in VARIABLES {
        for value in VALUES {
            inputrc.push_str(&format!("set {variable} {value}\n"));
        }
    }
    inputrc.push_str("set\nSET BELL-STYLE NO-SUCH-STYLE\nSeT bell-style bogus\n");
    fs::write(&path, &inputrc).expect("write inputrc");

    let keys = b"abc\r";
    let bash = run_bash_readline_with_inputrc_path(keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(keys, &path);

    let bash_diagnostics = reported_inputrc_diagnostics(&bash, &path);
    let sushline_diagnostics = reported_inputrc_diagnostics(&sushline, &path);
    assert!(!bash_diagnostics.is_empty(), "{bash}");
    assert_eq!(bash_diagnostics, sushline_diagnostics, "{sushline}");
    assert_eq!(accepted_line(&bash).as_deref(), Some("abc"), "{bash}");
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("abc"),
        "{sushline}"
    );
}

/// The `readline: <path>: line <n>: <variable>: <message>` diagnostics in an
/// output stream, as (line, text) pairs.
fn reported_inputrc_diagnostics(output: &str, path: &Path) -> Vec<(u32, String)> {
    let prefix = format!("readline: {}: line ", path.display());
    let mut diagnostics = Vec::new();
    for line in output.split('\n') {
        let Some(rest) = line
            .find(&prefix)
            .map(|index| &line[index + prefix.len()..])
        else {
            continue;
        };
        let Some((number, rest)) = rest.split_once(':') else {
            continue;
        };
        let Ok(number) = number.trim().parse::<u32>() else {
            continue;
        };
        diagnostics.push((number, rest.trim().trim_end_matches('\r').to_string()));
    }
    diagnostics
}

#[test]
fn bash_readline_and_sushline_write_inputrc_diagnostics_to_standard_error() {
    // GNU writes the inputrc diagnostics to standard error, so an embedder's
    // standard output stays free of them.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, "set bell-style no-such-style\n").expect("write inputrc");
    let keys = b"abc\r";

    let bash_stderr = dir.path().join("bash-stderr");
    let mut bash = CommandBuilder::new("sh");
    bash.args([
        "-c",
        &format!(
            "exec 2>{}; exec bash --noprofile --norc -i -c {}",
            shell_single_quote(&bash_stderr.display().to_string()),
            shell_single_quote(&format!(
                "printf \"{READY_PROMPT}\"; IFS= read -r -e line; printf \"SUSHLINE_ACCEPTED:%s\\n\" \"$line\""
            )),
        ),
    ]);
    bash.env("INPUTRC", path.to_string_lossy().as_ref());
    let bash_output = run_pty(bash, keys);
    let bash_diagnostics = fs::read_to_string(&bash_stderr).unwrap_or_default();

    let sushline_stderr = dir.path().join("sushline-stderr");
    let mut sushline = CommandBuilder::new("sh");
    sushline.args([
        "-c",
        &format!(
            "exec 2>{}; exec {}",
            shell_single_quote(&sushline_stderr.display().to_string()),
            shell_single_quote(env!("CARGO_BIN_EXE_sushline-harness")),
        ),
    ]);
    sushline.env("SUSHLINE_INPUTRC_FILE", path.to_string_lossy().as_ref());
    sushline.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline.env("SUSHLINE_HISTORY", "");
    let sushline_output = run_pty(sushline, keys);
    let sushline_diagnostics = fs::read_to_string(&sushline_stderr).unwrap_or_default();

    let expected = format!(
        "readline: {}: line 1: bell-style: could not set value to `no-such-style'",
        path.display()
    );
    assert!(
        bash_diagnostics.contains(&format!("{expected}\n")),
        "bash stderr: {bash_diagnostics}"
    );
    assert!(
        sushline_diagnostics.contains(&format!("{expected}\n")),
        "sushline stderr: {sushline_diagnostics}"
    );
    assert!(
        !bash_output.contains("readline:"),
        "bash output: {bash_output}"
    );
    assert!(
        !sushline_output.contains("readline:"),
        "sushline output: {sushline_output}"
    );
}

#[test]
fn bash_dies_with_a_segmentation_fault_on_a_cyclic_inputrc_include() {
    // In an interactive terminal GNU's shell dies before the first prompt
    // when the inputrc includes itself, while Sushline fails the inputrc
    // load with the depth-limit error.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, format!("$include {}\n", path.display())).expect("write inputrc");
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    // The shell reports the signal death of its child as 128 + SIGSEGV, so
    // the trailing exit keeps the shell from replacing itself with the child.
    let mut bash = CommandBuilder::new("bash");
    bash.args([
        "-c",
        &format!(
            "bash --noprofile --norc -i -c {}; exit $?",
            // The prompt is split so that the shell's report of the command
            // line does not contain the marker the case looks for.
            shell_single_quote("IFS= read -r -e -p \"SUSHLINE\"\"_READY>\" line")
        ),
    ]);
    bash.env("INPUTRC", path.to_string_lossy().as_ref());
    let (output, status) = run_pty_until_exit(bash, size);
    assert!(
        !output.contains(READY_PROMPT),
        "GNU must not reach the prompt: {output}"
    );
    assert_eq!(
        status.as_ref().map(|status| status.exit_code()),
        Some(139),
        "GNU must die from SIGSEGV: {status:?}"
    );

    let mut sushline = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    sushline.env("SUSHLINE_INPUTRC_FILE", path.to_string_lossy().as_ref());
    sushline.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline.env("SUSHLINE_HISTORY", "");
    let (output, status) = run_pty_until_exit(sushline, size);
    assert!(
        !output.contains(READY_PROMPT),
        "Sushline must not reach the prompt: {output}"
    );
    assert!(
        output.contains("inputrc include depth exceeded"),
        "Sushline must report the depth limit: {output}"
    );
    assert_eq!(
        status.as_ref().map(|status| status.exit_code()),
        Some(1),
        "Sushline must fail the load: {status:?}"
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
fn bash_readline_and_sushline_match_inputrc_include_path_rules() {
    let cwd = tempfile::tempdir().expect("cwd");
    let home = tempfile::tempdir().expect("home");
    let included = "\"\\C-o\": beginning-of-line\n";
    fs::write(cwd.path().join("relative.inputrc"), included).expect("write CWD include");
    fs::write(home.path().join("home.inputrc"), included).expect("write HOME include");
    let home_path = home.path().to_str().expect("HOME path is UTF-8");
    let keys = b"abc\x0fX\r";

    let check = |label: &str, directive: &str, env: &[(&str, &str)], expected: &str| {
        // A sentinel binding proves whether the include replaced the binding.
        let inputrc = format!("\"\\C-o\": \"Q\"\n{directive}\n");
        let bash =
            run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, env, cwd.path());
        let sushline =
            run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, env, cwd.path());
        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{label}: {bash}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{label}: {sushline}"
        );
    };

    check(
        "CWD-relative include",
        "$include relative.inputrc",
        &[],
        "Xabc",
    );
    check(
        "HOME tilde include",
        "$include ~/home.inputrc",
        &[("HOME", home_path)],
        "Xabc",
    );
    check(
        "literal environment-variable include",
        "$include $HOME/home.inputrc",
        &[("HOME", home_path)],
        "abcQX",
    );
    check(
        "missing include preserves prior binding",
        "$include missing.inputrc",
        &[],
        "abcQX",
    );
}

#[test]
fn bash_readline_and_sushline_continue_after_missing_inputrc_include() {
    let cwd = tempfile::tempdir().expect("cwd");
    let inputrc = "\"\\C-o\": \"Q\"\n$include missing.inputrc\n\"\\C-o\": beginning-of-line\n";
    let keys = b"abc\x0fX\r";
    let bash = run_bash_readline_with_inputrc_file_and_env_in_dir(keys, inputrc, &[], cwd.path());
    let sushline = run_sushline_harness_with_inputrc_and_env_in_dir(keys, inputrc, &[], cwd.path());

    assert_eq!(accepted_line(&bash).as_deref(), Some("Xabc"));
    assert_eq!(accepted_line(&sushline).as_deref(), Some("Xabc"));
}

#[test]
fn bash_readline_and_sushline_skip_unknown_named_user_include() {
    let cwd = tempfile::tempdir().expect("cwd");
    let inputrc = "\"\\C-o\": \"Q\"\n$include ~no-such-inputrc-user-xyz/missing.inputrc\n\"\\C-o\": beginning-of-line\n";
    let keys = b"abc\x0fX\r";
    let bash = run_bash_readline_with_inputrc_file_and_env_in_dir(keys, inputrc, &[], cwd.path());
    let sushline = run_sushline_harness_with_inputrc_and_env_in_dir(keys, inputrc, &[], cwd.path());

    assert_eq!(accepted_line(&bash).as_deref(), Some("Xabc"));
    assert_eq!(accepted_line(&sushline).as_deref(), Some("Xabc"));
}

#[test]
fn bash_readline_and_sushline_expand_current_home_named_user_when_traversable() {
    use std::path::Path;

    let Some(home) = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .and_then(|path| fs::canonicalize(path).ok())
    else {
        eprintln!("skip named-user inputrc oracle: HOME is unavailable");
        return;
    };
    let Some((username, passwd_home)) = fs::read_to_string("/etc/passwd").ok().and_then(|passwd| {
        passwd.lines().find_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            let username = *fields.first()?;
            let account_home = *fields.get(5)?;
            let safe_username = !username.is_empty()
                && username
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte));
            let account_home = Path::new(account_home).canonicalize().ok()?;
            (safe_username && account_home == home).then(|| (username.to_string(), account_home))
        })
    }) else {
        eprintln!("skip named-user inputrc oracle: HOME has no matching local passwd entry");
        return;
    };

    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(
        dir.path().join("named-user.inputrc"),
        "\"\\C-o\": beginning-of-line\n",
    )
    .expect("write included inputrc");
    let target = fs::canonicalize(dir.path()).expect("canonicalize tempdir");
    let target = target
        .strip_prefix("/")
        .expect("tempdir has an absolute path");
    let relative = format!(
        "{}{}/named-user.inputrc",
        "../".repeat(passwd_home.components().count()),
        target.display()
    );
    if fs::metadata(passwd_home.join(&relative)).is_err() {
        eprintln!("skip named-user inputrc oracle: passwd home cannot reach the fixture");
        return;
    }
    let inputrc = format!("\"\\C-o\": \"Q\"\n$include ~{username}/{relative}\n");
    let fake_home = dir.path().join("empty-home");
    fs::create_dir(&fake_home).expect("create empty HOME");
    let Some(fake_home) = fake_home.to_str() else {
        eprintln!("skip named-user inputrc oracle: temporary HOME is not UTF-8");
        return;
    };
    let env = [("HOME", fake_home)];
    let keys = b"abc\x0fX\r";
    let bash = run_bash_readline_with_inputrc_file_and_env_in_dir(keys, &inputrc, &env, dir.path());
    let sushline =
        run_sushline_harness_with_inputrc_and_env_in_dir(keys, &inputrc, &env, dir.path());

    assert_eq!(accepted_line(&bash).as_deref(), Some("Xabc"));
    assert_eq!(accepted_line(&sushline).as_deref(), Some("Xabc"));
}

#[test]
fn bash_and_sushline_accept_nested_inputrc_depth_17() {
    let dir = tempfile::tempdir().expect("tempdir");
    let chain = (0..17)
        .map(|depth| dir.path().join(format!("chain-{depth}.inputrc")))
        .collect::<Vec<_>>();
    for depth in (0..chain.len()).rev() {
        let contents = if let Some(child) = chain.get(depth + 1) {
            format!("$include {}\n", child.display())
        } else {
            "\"\\C-o\": beginning-of-line\n".to_string()
        };
        fs::write(&chain[depth], contents).expect("write include chain");
    }
    let keys = b"abc\x0fX\r";
    let main_at_depth_16 = dir.path().join("main-depth-16.inputrc");
    fs::write(
        &main_at_depth_16,
        format!("\"\\C-o\": \"Q\"\n$include {}\n", chain[1].display()),
    )
    .expect("write depth-16 main inputrc");
    let bash_16 = run_bash_readline_with_inputrc_path(keys, &main_at_depth_16);
    let sushline_16 = run_sushline_harness_with_inputrc_path(keys, &main_at_depth_16);
    assert_eq!(accepted_line(&bash_16).as_deref(), Some("Xabc"));
    assert_eq!(accepted_line(&sushline_16).as_deref(), Some("Xabc"));

    let main_at_depth_17 = dir.path().join("main-depth-17.inputrc");
    fs::write(
        &main_at_depth_17,
        format!("\"\\C-o\": \"Q\"\n$include {}\n", chain[0].display()),
    )
    .expect("write depth-17 main inputrc");
    let bash_17 = run_bash_readline_with_inputrc_path(keys, &main_at_depth_17);
    assert_eq!(accepted_line(&bash_17).as_deref(), Some("Xabc"));

    let sushline_17 = run_sushline_harness_with_inputrc_path(keys, &main_at_depth_17);
    assert_eq!(accepted_line(&sushline_17).as_deref(), Some("Xabc"));
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
fn bash_readline_and_sushline_match_meta_prefix_input_across_locales() {
    if !c_utf8_locale_available() {
        return;
    }

    let high_bit = b"\xe1\r";
    let escape_prefix = b"\x1ba\r";
    let cases = [
        (
            "C",
            "",
            high_bit.as_slice(),
            "X",
            false,
            "C converts high-bit input",
        ),
        (
            "C.UTF-8",
            "",
            high_bit.as_slice(),
            "X",
            false,
            "UTF-8 keeps 8-bit Meta input",
        ),
        (
            "C.UTF-8",
            "",
            escape_prefix.as_slice(),
            "",
            true,
            "unforced ESC-prefixed Meta is unbound",
        ),
        (
            "C.UTF-8",
            "set force-meta-prefix on\n",
            escape_prefix.as_slice(),
            "X",
            false,
            "forced Meta prefix uses ESC",
        ),
        (
            "C.UTF-8",
            "set force-meta-prefix on\n",
            high_bit.as_slice(),
            "\u{fffd}",
            false,
            "forced Meta prefix leaves a raw high bit literal",
        ),
        (
            "C.UTF-8",
            "set convert-meta off\nset input-meta off\nset meta-flag off\n",
            high_bit.as_slice(),
            "X",
            false,
            "input flags do not strip the received high bit",
        ),
    ];

    for (locale, settings, keys, expected, expect_bell, case) in cases {
        let inputrc = format!("{settings}\"\\M-a\": \"X\"\n");
        let env = [("LC_ALL", locale)];
        let bash = run_bash_readline_with_inputrc_file_and_env(keys, &inputrc, &env);
        let sushline = run_sushline_harness_with_inputrc_and_env(keys, &inputrc, &env);

        assert_eq!(
            accepted_line(&bash).as_deref(),
            Some(expected),
            "{case}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some(expected),
            "{case}: bash={bash:?} sushline={sushline:?}"
        );
        assert_eq!(bell_count(&bash) > 0, expect_bell, "{case}: {bash:?}");
        assert_eq!(
            bell_count(&sushline) > 0,
            expect_bell,
            "{case}: {sushline:?}"
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

#[test]
fn bash_readline_and_sushline_terminal_mode_sequences_follow_runtime_boundary() {
    let keys = b"\r";
    let inputrc = "set enable-keypad on\n";
    let terms = [
        Some("xterm-256color"),
        Some("vt100"),
        Some("vt52"),
        Some("dumb"),
        Some("unknown"),
        Some(""),
        None,
    ];

    for term in terms {
        let bash = run_bash_readline_with_term(keys, inputrc, term);
        let sushline = run_sushline_harness_with_term(keys, inputrc, term);
        assert_eq!(
            accepted_line(&bash),
            Some(String::new()),
            "TERM={term:?}: {bash:?}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some(String::new()),
            "TERM={term:?}: bash={bash:?} sushline={sushline:?}"
        );

        let sequences: &[&str] = match term {
            Some("xterm-256color") | Some("vt100") => &["\x1b[?1h\x1b=", "\x1b[?1l\x1b>"],
            Some("vt52") => &["\x1b=", "\x1b>"],
            _ => &[],
        };
        for sequence in sequences {
            assert!(
                bash.contains(sequence),
                "Bash 5.3 should emit terminfo keypad sequence {sequence:?} for TERM={term:?}: {bash:?}"
            );
            assert!(
                sushline.contains(sequence),
                "Sushline should emit terminfo keypad sequence {sequence:?} for TERM={term:?}: {sushline:?}"
            );
        }
        if let [entry, exit] = sequences {
            for (name, output) in [("Bash 5.3", &bash), ("Sushline", &sushline)] {
                let entry_pos = output.find(entry).unwrap();
                let prompt_pos = output.find(READY_PROMPT).expect("ready prompt");
                let exit_pos = output.find(exit).unwrap();
                let accepted_pos = output.find("SUSHLINE_ACCEPTED:").expect("accepted marker");
                assert!(
                    entry_pos < prompt_pos && prompt_pos < exit_pos && exit_pos < accepted_pos,
                    "{name} should enter keypad mode before the prompt and leave before acceptance, TERM={term:?}: {output:?}"
                );
            }
        }
        if sequences.is_empty() {
            for forbidden in ["\x1b=", "\x1b>"] {
                assert!(
                    !bash.contains(forbidden),
                    "Bash 5.3 unexpectedly emitted keypad sequence {forbidden:?} for TERM={term:?}: {bash:?}"
                );
                assert!(
                    !sushline.contains(forbidden),
                    "Sushline emitted keypad sequence {forbidden:?} without terminfo capability for TERM={term:?}: {sushline:?}"
                );
            }
        }
        for (disabled_inputrc, behavior) in [
            ("set enable-keypad off\n", "explicit off"),
            ("", "default off"),
        ] {
            let bash = run_bash_readline_with_term(keys, disabled_inputrc, term);
            let sushline = run_sushline_harness_with_term(keys, disabled_inputrc, term);
            assert_eq!(
                accepted_line(&bash),
                Some(String::new()),
                "{behavior}, TERM={term:?}: {bash:?}"
            );
            assert_eq!(
                accepted_line(&sushline),
                Some(String::new()),
                "{behavior}, TERM={term:?}: bash={bash:?} sushline={sushline:?}"
            );
            for forbidden in ["\x1b=", "\x1b>"] {
                assert!(
                    !bash.contains(forbidden),
                    "Bash 5.3 emitted keypad sequence {forbidden:?} with {behavior}, TERM={term:?}: {bash:?}"
                );
                assert!(
                    !sushline.contains(forbidden),
                    "Sushline emitted keypad sequence {forbidden:?} with {behavior}, TERM={term:?}: {sushline:?}"
                );
            }
        }

        for (meta_inputrc, setting) in [
            ("", "default"),
            ("set enable-meta-key on\n", "explicit on"),
            ("set enable-meta-key off\n", "explicit off"),
        ] {
            let bash = run_bash_readline_with_term(keys, meta_inputrc, term);
            let sushline = run_sushline_harness_with_term(keys, meta_inputrc, term);
            assert_eq!(
                accepted_line(&bash),
                Some(String::new()),
                "{setting} enable-meta-key, TERM={term:?}: {bash:?}"
            );
            assert_eq!(
                accepted_line(&sushline),
                Some(String::new()),
                "{setting} enable-meta-key, TERM={term:?}: bash={bash:?} sushline={sushline:?}"
            );
            assert_no_meta_key_sequences(&bash, "Bash 5.3", term, setting);
            assert_no_meta_key_sequences(&sushline, "Sushline", term, setting);
        }
    }
}

fn assert_no_meta_key_sequences(
    output: &str,
    implementation: &str,
    term: Option<&str>,
    setting: &str,
) {
    for forbidden in ["\x1b[?1034h", "\x1b[?1034l"] {
        assert!(
            !output.contains(forbidden),
            "{implementation} emitted meta-key mode sequence {forbidden:?} with {setting}, TERM={term:?}: {output:?}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_apply_binding_before_non_utf8_inputrc_comment() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let mut inputrc = br##""\C-o": beginning-of-line # comment "##.to_vec();
    inputrc.push(0xff);
    inputrc.push(b'\n');
    fs::write(&path, inputrc).expect("write inputrc");
    let keys = [b"abc".as_slice(), &[0x0f, b'X', b'\r']].concat();

    let bash = run_bash_readline_with_inputrc_path(&keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&keys, &path);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(bell_count(&bash), 0, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
    assert_eq!(bell_count(&sushline), 0, "{sushline}");
}

#[test]
fn bash_and_sushline_respect_raw_inputrc_key_conditionals() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let mut inputrc = b"set convert-meta off\nset input-meta on\nset meta-flag off\n".to_vec();
    inputrc.extend_from_slice(b"$if version < 0.0\n\"");
    inputrc.push(0xff);
    inputrc.extend_from_slice(b"\": beginning-of-line\n\"\\377\": beginning-of-line\n$else\n\"");
    inputrc.push(0xfe);
    inputrc.extend_from_slice(b"\": end-of-line\n$endif\n");
    fs::write(&path, inputrc).expect("write conditional inputrc");

    let inactive_keys = [0xff, b'X', b'\r'];
    let bash = run_bash_readline_with_inputrc_path(&inactive_keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&inactive_keys, &path);
    assert_eq!(accepted_line(&bash).as_deref(), Some("\u{fffd}X"), "{bash}");
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("\u{fffd}X"),
        "{sushline}"
    );

    let active_else_keys = [b"abc".as_slice(), &[0xfe, b'X', b'\r']].concat();
    let bash = run_bash_readline_with_inputrc_path(&active_else_keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&active_else_keys, &path);
    assert_eq!(accepted_line(&bash).as_deref(), Some("abcX"), "{bash}");
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("abcX"),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_match_raw_0xff_inputrc_key_and_escape() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let prefix = b"set convert-meta off\nset input-meta on\nset meta-flag off\n";
    let mut raw_inputrc = prefix.to_vec();
    raw_inputrc.push(b'"');
    raw_inputrc.push(0xff);
    raw_inputrc.extend_from_slice(b"\": beginning-of-line\n");
    fs::write(&path, raw_inputrc).expect("write raw inputrc");
    let keys = [b"abc".as_slice(), &[0xff, b'X', b'\r']].concat();

    let bash = run_bash_readline_with_inputrc_path(&keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&keys, &path);
    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );

    let mut escaped_inputrc = prefix.to_vec();
    escaped_inputrc.extend_from_slice(b"\"\\377\": beginning-of-line\n");
    fs::write(&path, escaped_inputrc).expect("write escaped inputrc");
    let bash = run_bash_readline_with_inputrc_path(&keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&keys, &path);
    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_do_not_strip_invalid_comments_from_set_directives() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let mut inputrc = b"set bell-style none # comment ".to_vec();
    inputrc.push(0xff);
    inputrc.push(b'\n');
    fs::write(&path, inputrc).expect("write inputrc");
    let keys = [b"abc".as_slice(), &[0x07, b'\r']].concat();
    let bash = run_bash_readline_with_inputrc_path(&keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(&keys, &path);

    assert_eq!(accepted_line(&bash), Some("abc".to_string()), "{bash}");
    assert_eq!(bell_count(&bash), 1, "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("abc".to_string()),
        "{sushline}"
    );
    assert_eq!(bell_count(&sushline), 1, "{sushline}");
}

#[test]
fn bash_readline_and_sushline_apply_binding_before_bare_invalid_trailing_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    let mut inputrc = br##""\C-o": beginning-of-line trailing "##.to_vec();
    inputrc.push(0xff);
    inputrc.push(b'\n');
    fs::write(&path, inputrc).expect("write inputrc");
    let keys = b"abc\x0fX\r";
    let bash = run_bash_readline_with_inputrc_path(keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(keys, &path);

    assert_eq!(accepted_line(&bash), Some("Xabc".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabc".to_string()),
        "{sushline}"
    );
}

#[test]
fn bash_readline_and_sushline_apply_quoted_macros_before_non_utf8_trailing_bytes() {
    for line in [
        br##""\C-o": "hello" trailing "##.as_slice(),
        br##""\C-o": 'hello' # comment "##.as_slice(),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("inputrc");
        let mut inputrc = line.to_vec();
        inputrc.push(0xff);
        inputrc.push(b'\n');
        fs::write(&path, inputrc).expect("write inputrc");
        let keys = b"\x0f\r";
        let bash = run_bash_readline_with_inputrc_path(keys, &path);
        let sushline = run_sushline_harness_with_inputrc_path(keys, &path);

        assert_eq!(accepted_line(&bash).as_deref(), Some("hello"), "{bash:?}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("hello"),
            "{sushline:?}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_decode_single_quoted_macro_escapes() {
    let inputrc = br##""\C-o": '\C-aX'"##;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let keys = b"abc\x0f\r";
    let bash = run_bash_readline_with_inputrc_path(keys, &path);
    let sushline = run_sushline_harness_with_inputrc_path(keys, &path);

    assert_eq!(accepted_line(&bash).as_deref(), Some("Xabc"), "{bash:?}");
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some("Xabc"),
        "{sushline:?}"
    );
}

#[test]
fn bash_readline_and_sushline_apply_quoted_macros_before_valid_trailing_text() {
    for inputrc in [
        "\"\\C-o\": \"hello\" trailing\n",
        "\"\\C-o\": \"hello\" # comment\n",
        "\"\\C-o\": 'hello' # comment\n",
    ] {
        let keys = b"\x0f\r";
        let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
        let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

        assert_eq!(accepted_line(&bash).as_deref(), Some("hello"), "{bash:?}");
        assert_eq!(
            accepted_line(&sushline).as_deref(),
            Some("hello"),
            "{sushline:?}"
        );
    }
}

#[test]
fn bash_and_sushline_bound_direct_inputrc_macro_recursion() {
    let inputrc = "\"\\C-o\": \"x\\C-oS\"\n\"\\C-p\": \"Q\"\n";
    // The active macro suffix `S` is discarded; physical `Z` and later
    // ordinary macros survive in this batch and the following read.
    let keys = b"\x0fZ\x10R\r\x10\r";
    let bash = run_bash_readline_two_reads_with_inputrc_file_and_history(keys, inputrc, &[]);
    let sushline = run_sushline_harness_two_reads_with_inputrc_and_history(keys, inputrc, &[]);
    let expected_first = format!("{}ZQR", "x".repeat(17));

    assert_eq!(
        accepted_numbered_line(&bash, 1).as_deref(),
        Some(expected_first.as_str()),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&sushline, 1).as_deref(),
        Some(expected_first.as_str()),
        "{sushline}"
    );
    assert_eq!(
        accepted_numbered_line(&bash, 2).as_deref(),
        Some("Q"),
        "{bash}"
    );
    assert_eq!(
        accepted_numbered_line(&sushline, 2).as_deref(),
        Some("Q"),
        "{sushline}"
    );
    for output in [&bash, &sushline] {
        assert_eq!(
            output
                .matches("readline: maximum macro execution nesting level exceeded")
                .count(),
            1,
            "{output}"
        );
        assert_eq!(bell_count(output), 1, "{output}");
    }
}

#[test]
fn bash_and_sushline_write_the_macro_nesting_diagnostic_to_standard_error() {
    // GNU writes the readline error message to standard error, so an
    // embedder's standard output stays free of it.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, "\"\\C-o\": \"x\\C-oS\"\n").expect("write inputrc");
    let keys = b"\x0fZ\r";
    let message = "readline: maximum macro execution nesting level exceeded";

    let bash_stderr = dir.path().join("bash-stderr");
    let mut bash = CommandBuilder::new("bash");
    bash.args([
        "-c",
        &format!(
            "exec 2>{}; exec bash --noprofile --norc -i -c {}",
            shell_single_quote(&bash_stderr.display().to_string()),
            shell_single_quote(&format!(
                "printf \"{READY_PROMPT}\"; IFS= read -r -e line; printf \"SUSHLINE_ACCEPTED:%s\\n\" \"$line\""
            )),
        ),
    ]);
    bash.env("INPUTRC", path.to_string_lossy().as_ref());
    let bash_output = run_pty(bash, keys);
    let bash_diagnostics = fs::read_to_string(&bash_stderr).unwrap_or_default();

    let sushline_stderr = dir.path().join("sushline-stderr");
    let mut sushline = CommandBuilder::new("bash");
    sushline.args([
        "-c",
        &format!(
            "exec 2>{}; exec {}",
            shell_single_quote(&sushline_stderr.display().to_string()),
            shell_single_quote(env!("CARGO_BIN_EXE_sushline-harness")),
        ),
    ]);
    sushline.env("SUSHLINE_INPUTRC_FILE", path.to_string_lossy().as_ref());
    sushline.env("SUSHLINE_PROMPT", READY_PROMPT);
    sushline.env("SUSHLINE_HISTORY", "");
    let sushline_output = run_pty(sushline, keys);
    let sushline_diagnostics = fs::read_to_string(&sushline_stderr).unwrap_or_default();

    assert!(
        bash_diagnostics.contains(&format!("{message}\n")),
        "bash stderr: {bash_diagnostics}"
    );
    assert!(
        sushline_diagnostics.contains(&format!("{message}\n")),
        "sushline stderr: {sushline_diagnostics}"
    );
    assert!(!bash_output.contains(message), "bash output: {bash_output}");
    assert!(
        !sushline_output.contains(message),
        "sushline output: {sushline_output}"
    );
}

#[test]
fn bash_and_sushline_bound_indirect_inputrc_macro_recursion() {
    let inputrc = "\"\\C-o\": \"x\\C-p\"\n\"\\C-p\": \"\\C-o\"\n";
    // Single-batch input proves the physical suffix survives the unwind.
    let keys = b"\x0fZ\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);
    let expected = format!("{}Z", "x".repeat(9));

    assert_eq!(
        accepted_line(&bash).as_deref(),
        Some(expected.as_str()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline).as_deref(),
        Some(expected.as_str()),
        "{sushline}"
    );
    for output in [&bash, &sushline] {
        assert_eq!(
            output
                .matches("readline: maximum macro execution nesting level exceeded")
                .count(),
            1,
            "{output}"
        );
        assert_eq!(bell_count(output), 1, "{output}");
    }
}

#[test]
fn bash_and_sushline_abort_discards_remaining_inputrc_macro_expansion() {
    let inputrc = "\"\\C-t\": \"a\\C-gb\"\n";
    let keys = b"\x14Z\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    for output in [&bash, &sushline] {
        assert_eq!(accepted_line(output).as_deref(), Some("aZ"), "{output}");
        assert_eq!(bell_count(output), 1, "{output}");
        assert_eq!(
            output
                .matches("readline: maximum macro execution nesting level exceeded")
                .count(),
            0,
            "{output}"
        );
    }
}

#[test]
fn bash_and_sushline_abort_discards_nested_inputrc_macro_expansions() {
    let inputrc = "\"\\C-t\": \"a\\C-oZ\"\n\"\\C-o\": \"b\\C-gc\"\n";
    let keys = b"\x14X\r";
    let bash = run_bash_readline_with_inputrc_file(keys, inputrc);
    let sushline = run_sushline_harness_with_inputrc(keys, inputrc);

    for output in [&bash, &sushline] {
        assert_eq!(accepted_line(output).as_deref(), Some("abX"), "{output}");
        assert_eq!(bell_count(output), 1, "{output}");
        assert_eq!(
            output
                .matches("readline: maximum macro execution nesting level exceeded")
                .count(),
            0,
            "{output}"
        );
    }
}
