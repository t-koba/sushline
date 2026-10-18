use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::fs;
use std::io::{Read, Write};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub(super) const READY_PROMPT: &str = "SUSHLINE_READY>";

pub(super) fn c_utf8_locale_available() -> bool {
    let Ok(locale) = Command::new("locale")
        .arg("charmap")
        .env("LC_ALL", "C.UTF-8")
        .output()
    else {
        return false;
    };
    locale.status.success()
        && String::from_utf8_lossy(&locale.stdout)
            .trim()
            .eq_ignore_ascii_case("UTF-8")
}
pub(super) fn run_bash_readline(keys: &[u8]) -> String {
    run_bash_readline_with_bindings(keys, "")
}

pub(super) fn run_bash_readline_with_bindings(keys: &[u8], bindings: &str) -> String {
    run_bash_readline_with_bindings_and_history(keys, bindings, &[])
}

pub(super) fn run_bash_interactive_after_ctrl_c() -> String {
    let mut command = CommandBuilder::new("bash");
    command.env("PS1", READY_PROMPT);
    command.args(["--noprofile", "--norc", "-i"]);
    run_pty_steps_until(
        command,
        &[
            (READY_PROMPT, b"abc\x03".as_slice()),
            (
                READY_PROMPT,
                b"printf 'SUSHLINE_ACCEPTED:%s\\n' ok\r".as_slice(),
            ),
        ],
        "SUSHLINE_ACCEPTED:ok",
    )
}

pub(super) fn run_bash_readline_eof_with_inputrc(keys: &[u8], inputrc: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; if IFS= read -r -e -p "{READY_PROMPT}" line; then printf 'SUSHLINE_ACCEPTED:%s\n' "$line"; else printf 'SUSHLINE_EOF\n'; fi"#
        ),
    ]);
    run_pty_until(command, keys, "SUSHLINE_EOF")
}

pub(super) fn run_bash_readline_with_bindings_and_env(
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

pub(super) fn run_bash_history_expand(expansion: &str, history: &[&str]) -> String {
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

pub(super) fn run_bash_readline_with_term(
    keys: &[u8],
    inputrc: &str,
    term: Option<&str>,
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    if let Some(term) = term {
        command.env("TERM", term);
    } else {
        command.env_remove("TERM");
    }
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty_with_size_until_preserving_term(
        command,
        keys,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        "SUSHLINE_ACCEPTED:",
    )
}

pub(super) fn run_sushline_harness_with_term(
    keys: &[u8],
    inputrc: &str,
    term: Option<&str>,
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    if let Some(term) = term {
        command.env("TERM", term);
    } else {
        command.env_remove("TERM");
    }
    run_pty_with_size_until_preserving_term(
        command,
        keys,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        "SUSHLINE_ACCEPTED:",
    )
}

pub(super) fn run_bash_readline_with_inputrc_file(keys: &[u8], inputrc: &str) -> String {
    run_bash_readline_with_inputrc_file_and_env(keys, inputrc, &[])
}

pub(super) fn run_bash_readline_with_inputrc_file_and_env(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
) -> String {
    run_bash_readline_with_inputrc_file_and_env_removing(keys, inputrc, env, &[])
}

pub(super) fn run_bash_readline_with_inputrc_file_and_env_and_size(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
    size: PtySize,
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
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
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty_with_size(command, keys, size)
}

pub(super) fn run_bash_readline_with_inputrc_file_and_env_in_dir(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
    cwd: &std::path::Path,
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
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
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty_in_dir(command, keys, cwd)
}

pub(super) fn run_bash_programmable_completion_in_dir(
    keys: &[u8],
    inputrc: &str,
    options: &str,
    reply: Option<&str>,
    cwd: &std::path::Path,
) -> Vec<u8> {
    // Programmable completion must run at Bash's command prompt, not in `read -e`.
    let dir = tempfile::tempdir().expect("tempdir");
    let inputrc_path = dir.path().join("inputrc");
    fs::write(&inputrc_path, inputrc).expect("write inputrc");
    let rcfile_path = dir.path().join("bashrc");
    // Unit separator in BASH_COMPLETION_REPLY splits fixture candidates for multi-match probes.
    let rcfile = format!(
        r#"PS1='{READY_PROMPT}'
stty status undef dsusp undef lnext undef 2>/dev/null || true
_none() {{ BASH_COMPLETION_INVOKED=1; COMPREPLY=(); if [[ -n $BASH_COMPLETION_REPLY ]]; then IFS=$'\x1f' read -r -a COMPREPLY <<< "$BASH_COMPLETION_REPLY"; fi; }}
complete -F _none {options} echo
bind -x '"\C-g": printf "BASH_COMPLETION_INVOKED:%s\n" "$BASH_COMPLETION_INVOKED"; printf "BASH_READLINE_HEX:"; printf "%s" "$READLINE_LINE" | od -An -tx1 | tr -d " \n"; printf "\n"'"#
    );
    fs::write(&rcfile_path, rcfile).expect("write Bash rcfile");
    let inputrc_path = inputrc_path.to_string_lossy().into_owned();
    let rcfile_path = rcfile_path.to_string_lossy().into_owned();
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", inputrc_path);
    command.env("PS1", READY_PROMPT);
    command.env("BASH_COMPLETION_REPLY", reply.unwrap_or(""));
    command.args(["--noprofile", "--rcfile", &rcfile_path, "-i"]);
    run_pty_in_dir_with_size_raw_until(
        command,
        keys,
        cwd,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        "BASH_READLINE_HEX:",
    )
}

pub(super) fn run_bash_readline_with_inputrc_file_and_env_removing(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
    removed_env: &[&str],
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    for name in removed_env {
        command.env_remove(name);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty(command, keys)
}

pub(super) fn run_bash_readline_with_inputrc_file_and_history(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
) -> String {
    run_bash_readline_with_inputrc_file_and_history_and_env(keys, inputrc, history, &[])
}

pub(super) fn run_bash_readline_with_inputrc_file_and_history_and_env(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
    env: &[(&str, &str)],
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let mut history_commands = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    if history_commands.is_empty() {
        history_commands = ":".to_string();
    }
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
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; {history_commands}; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty(command, keys)
}

pub(super) fn run_bash_readline_with_reloaded_inputrc(
    keys: &[u8],
    initial_inputrc: &str,
    reloaded_inputrc: &str,
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, initial_inputrc).expect("write initial inputrc");
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty_after_prompt(command, keys, || {
        fs::write(&path, reloaded_inputrc).expect("write reloaded inputrc");
    })
}

pub(super) fn run_bash_readline_with_inputrc_path(keys: &[u8], path: &std::path::Path) -> String {
    run_bash_readline_with_inputrc_path_and_env(keys, path, &[])
}

pub(super) fn run_bash_readline_with_inputrc_path_and_env(
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
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty(command, keys)
}

pub(super) fn run_bash_readline_two_reads_with_inputrc_file_and_history(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let mut history_commands = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    if history_commands.is_empty() {
        history_commands = ":".to_string();
    }
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; {history_commands}; IFS= read -r -e -p "{READY_PROMPT}" line1; printf 'SUSHLINE_ACCEPTED_1:%s\n' "$line1"; IFS= read -r -e -p "{READY_PROMPT}" line2; printf 'SUSHLINE_ACCEPTED_2:%s\n' "$line2""#
        ),
    ]);
    run_pty_until(command, keys, "SUSHLINE_ACCEPTED_2:")
}

pub(super) fn run_bash_readline_with_bindings_and_history(
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

pub(super) fn run_bash_readline_with_size(
    keys: &[u8],
    bindings: &str,
    history: &[&str],
    size: PtySize,
) -> String {
    run_bash_readline_with_size_and_env(keys, bindings, history, size, &[])
}

pub(super) fn run_bash_readline_with_size_and_env(
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
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; {history_commands}; if [ -n {bind_command} ]; then bind {bind_command}; fi; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty_with_size(command, keys, size)
}

pub(super) fn run_sushline_harness(keys: &[u8]) -> String {
    run_sushline_harness_with_inputrc(keys, "")
}

pub(super) fn run_sushline_harness_with_inputrc(keys: &[u8], inputrc: &str) -> String {
    run_sushline_harness_with_inputrc_and_history(keys, inputrc, &[])
}

pub(super) fn run_sushline_harness_until(keys: &[u8], inputrc: &str, stop_marker: &str) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    run_pty_until(command, keys, stop_marker)
}

pub(super) fn run_sushline_harness_after_ctrl_c() -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", "");
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    command.env("SUSHLINE_READS", "2");
    command.env("SUSHLINE_CONTINUE_ON_INTERRUPT", "1");
    run_pty_steps_until(
        command,
        &[
            (READY_PROMPT, b"abc\x03".as_slice()),
            (READY_PROMPT, b"ok\r".as_slice()),
        ],
        "SUSHLINE_ACCEPTED_2:",
    )
}

pub(super) fn run_sushline_harness_with_inputrc_and_env(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
) -> String {
    run_sushline_harness_with_inputrc_and_env_removing(keys, inputrc, env, &[])
}

pub(super) fn run_sushline_harness_with_inputrc_and_env_in_dir(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
    cwd: &std::path::Path,
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    for (name, value) in env {
        command.env(name, value);
    }
    run_pty_in_dir(command, keys, cwd)
}

pub(super) fn run_sushline_harness_with_inputrc_and_env_removing(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
    removed_env: &[&str],
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    for name in removed_env {
        command.env_remove(name);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    run_pty(command, keys)
}

pub(super) fn run_sushline_harness_with_inputrc_path(
    keys: &[u8],
    path: &std::path::Path,
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC_FILE", path.to_string_lossy().as_ref());
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    run_pty(command, keys)
}

pub(super) fn run_sushline_harness_with_reloaded_inputrc(
    keys: &[u8],
    initial_inputrc: &str,
    reloaded_inputrc: &str,
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, initial_inputrc).expect("write initial inputrc");
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC_FILE", path.to_string_lossy().as_ref());
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    run_pty_after_prompt(command, keys, || {
        fs::write(&path, reloaded_inputrc).expect("write reloaded inputrc");
    })
}

pub(super) fn run_sushline_harness_with_inputrc_and_history(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
) -> String {
    run_sushline_harness_with_inputrc_and_history_and_env(keys, inputrc, history, &[])
}

pub(super) fn run_sushline_harness_with_inputrc_and_history_and_env(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
    env: &[(&str, &str)],
) -> String {
    run_sushline_harness_with_size_and_env(
        keys,
        inputrc,
        history,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        env,
    )
}

pub(super) fn run_sushline_harness_with_size(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
    size: PtySize,
) -> String {
    run_sushline_harness_with_size_and_env(keys, inputrc, history, size, &[])
}

fn run_sushline_harness_with_size_and_env(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
    size: PtySize,
    env: &[(&str, &str)],
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", history.join("\n"));
    for (name, value) in env {
        command.env(name, value);
    }
    run_pty_with_size(command, keys, size)
}

pub(super) fn run_sushline_harness_two_reads_with_inputrc_and_history(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", history.join("\n"));
    command.env("SUSHLINE_READS", "2");
    run_pty_until(command, keys, "SUSHLINE_ACCEPTED_2:")
}

/// Drive Bash at a real interactive prompt (no `-c`): seed history, read
/// one line with `read -e`, and report it without executing it.
///
/// `read -e` under `bash -i -c` never history-expands, and `history -p`
/// scans `;` into the event prefix while the prompt path terminates on it,
/// so prompt-path oracle cases need this shape. The prompt and marker are
/// assembled from variables so the echoed command line never contains them.
pub(super) fn run_bash_prompt_with_history(keys: &[u8], inputrc: &str, history: &[&str]) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", path.to_string_lossy().as_ref());
    command.env("PS1", READY_PROMPT);
    command.args(["--noprofile", "--norc", "-i"]);
    let mut owned: Vec<(String, Vec<u8>)> = history
        .iter()
        .map(|entry| {
            (
                READY_PROMPT.to_string(),
                format!("history -s {}\r", shell_single_quote(entry)).into_bytes(),
            )
        })
        .collect();
    owned.push((
        READY_PROMPT.to_string(),
        b"p=RDP; IFS= read -r -e -p \"${p}>\" line; m=SUSHLINE_ACCEPT; printf \"${m}ED:%s\\n\" \"$line\"\r"
            .to_vec(),
    ));
    owned.push(("RDP>".to_string(), keys.to_vec()));
    let steps: Vec<(&str, &[u8])> = owned
        .iter()
        .map(|(marker, step_keys)| (marker.as_str(), step_keys.as_slice()))
        .collect();
    run_pty_steps_until(command, &steps, "SUSHLINE_ACCEPTED:")
}

#[cfg(target_os = "linux")]
pub(super) fn run_bash_readline_with_canonical_paste_eof(suffix: &[u8]) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let inputrc_path = dir.path().join("inputrc");
    fs::write(&inputrc_path, "set enable-bracketed-paste on\n").expect("write inputrc");
    let mut command = CommandBuilder::new("bash");
    command.env("INPUTRC", inputrc_path.to_string_lossy().as_ref());
    command.args([
        "--noprofile",
        "--norc",
        "-i",
        "-c",
        &format!(
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line; printf 'SUSHLINE_ACCEPTED:%s\n' "$line""#
        ),
    ]);
    run_pty_with_canonical_paste_eof(command, suffix)
}

#[cfg(target_os = "linux")]
pub(super) fn run_sushline_harness_with_canonical_paste_eof(suffix: &[u8]) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", "set enable-bracketed-paste on\n");
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    run_pty_with_canonical_paste_eof(command, suffix)
}

#[cfg(target_os = "linux")]
fn run_pty_with_canonical_paste_eof(mut command: CommandBuilder, suffix: &[u8]) -> String {
    command.env("TERM", "xterm-256color");
    command.cwd("/");
    let pty_system = NativePtySystem::default();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("open pty");
    let mut child = pair.slave.spawn_command(command).expect("spawn command");
    drop(pair.slave);

    let master_fd = pair.master.as_raw_fd().expect("PTY master file descriptor");
    let mut reader = pair.master.try_clone_reader().expect("pty reader");
    let mut writer = pair.master.take_writer().expect("pty writer");
    let (tx, rx) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let mut buf = [0_u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut output = Vec::new();
    let mut sent_eof_input = false;
    let mut eof_checkpoint_seen = false;
    let mut child_alive_at_eof_checkpoint = false;
    let mut master_open_at_eof_checkpoint = false;
    let mut accepted_before_followup = false;
    let mut sent_followup = false;

    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(chunk) => {
                output.extend_from_slice(&chunk);
                if !sent_eof_input && find_bytes(&output, READY_PROMPT.as_bytes()).is_some() {
                    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
                    let get_rc = unsafe { libc::tcgetattr(master_fd, termios.as_mut_ptr()) };
                    assert_eq!(
                        get_rc,
                        0,
                        "read PTY termios: {}",
                        std::io::Error::last_os_error()
                    );
                    let mut termios = unsafe { termios.assume_init() };
                    termios.c_lflag |= libc::ICANON;
                    termios.c_lflag &= !libc::ECHO;
                    termios.c_cc[libc::VEOF] = 0x04;
                    let set_rc = unsafe { libc::tcsetattr(master_fd, libc::TCSANOW, &termios) };
                    assert_eq!(
                        set_rc,
                        0,
                        "set PTY termios: {}",
                        std::io::Error::last_os_error()
                    );
                    // In canonical mode, the first VEOF releases the queued paste bytes;
                    // the second creates a zero-byte read without closing the PTY master.
                    let mut paste = b"\x1b[200~partial".to_vec();
                    paste.extend_from_slice(suffix);
                    paste.extend_from_slice(b"\x04\x04");
                    writer
                        .write_all(&paste)
                        .expect("write paste and VEOF bytes");
                    writer.flush().expect("flush paste and VEOF bytes");
                    sent_eof_input = true;
                }
                if sent_eof_input && !sent_followup && find_bytes(&output, b"partial").is_some() {
                    eof_checkpoint_seen = true;
                    accepted_before_followup = find_bytes(&output, b"SUSHLINE_ACCEPTED:").is_some();
                    child_alive_at_eof_checkpoint = child
                        .try_wait()
                        .expect("check child after true EOF")
                        .is_none();
                    let mut probe = std::mem::MaybeUninit::<libc::termios>::uninit();
                    master_open_at_eof_checkpoint =
                        unsafe { libc::tcgetattr(master_fd, probe.as_mut_ptr()) == 0 };
                    writer.write_all(b"\n").expect("write post-EOF newline");
                    writer.flush().expect("flush post-EOF newline");
                    sent_followup = true;
                }
                if stop_marker_line_complete(&output, "SUSHLINE_ACCEPTED:") {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let saw_accepted_line = stop_marker_line_complete(&output, "SUSHLINE_ACCEPTED:");
    let output_text = lossy_output(&output);
    let _ = child.kill();
    let _ = child.wait();
    drop(writer);
    drop(pair.master);
    let _ = reader_thread.join();

    assert!(sent_eof_input, "PTY prompt not observed: {output_text}");
    assert!(
        eof_checkpoint_seen,
        "partial paste was not redisplayed after EOF: {output_text}"
    );
    assert!(
        child_alive_at_eof_checkpoint,
        "child terminated at the EOF checkpoint: {output_text}"
    );
    assert!(
        master_open_at_eof_checkpoint,
        "PTY master closed at the EOF checkpoint: {output_text}"
    );
    assert!(
        !accepted_before_followup,
        "line accepted before the follow-up LF: {output_text}"
    );
    assert!(sent_followup, "follow-up LF was not sent: {output_text}");
    assert!(
        saw_accepted_line,
        "accepted line not observed: {output_text}"
    );
    output_text
}

pub(super) fn run_pty(command: CommandBuilder, keys: &[u8]) -> String {
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

pub(super) fn run_pty_in_dir(
    command: CommandBuilder,
    keys: &[u8],
    cwd: &std::path::Path,
) -> String {
    lossy_output(&run_pty_in_dir_with_size_raw(
        command,
        keys,
        cwd,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    ))
}

pub(super) fn run_pty_in_dir_with_size_raw(
    command: CommandBuilder,
    keys: &[u8],
    cwd: &std::path::Path,
    size: PtySize,
) -> Vec<u8> {
    run_pty_in_dir_with_size_raw_until(command, keys, cwd, size, "SUSHLINE_ACCEPTED:")
}

pub(super) fn run_pty_in_dir_with_size_raw_until(
    mut command: CommandBuilder,
    keys: &[u8],
    cwd: &std::path::Path,
    size: PtySize,
    stop_marker: &str,
) -> Vec<u8> {
    command.env("TERM", "xterm-256color");
    command.cwd(cwd);
    run_pty_with_size_until_after_prompt_inner_raw(command, keys, size, stop_marker, None::<fn()>)
}

pub(super) fn run_pty_with_size(command: CommandBuilder, keys: &[u8], size: PtySize) -> String {
    run_pty_with_size_until(command, keys, size, "SUSHLINE_ACCEPTED:")
}

pub(super) fn run_pty_until(command: CommandBuilder, keys: &[u8], stop_marker: &str) -> String {
    run_pty_with_size_until(
        command,
        keys,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        stop_marker,
    )
}

pub(super) fn run_pty_steps_until(
    command: CommandBuilder,
    steps: &[(&str, &[u8])],
    stop_marker: &str,
) -> String {
    run_pty_steps_with_size_until(
        command,
        steps,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        stop_marker,
    )
}

/// True once the stop marker plus the end of its line have arrived.
///
/// PTY output can split the accepted line across reads; breaking on the bare
/// marker risks parsing a truncated `SUSHLINE_ACCEPTED:*` line. Every harness
/// marker is printed with a trailing newline, so wait for `\r`/`\n` after
/// the marker. The outer deadline still bounds the failure path.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn stop_marker_line_complete(output: &[u8], marker: &str) -> bool {
    let Some(start) = find_bytes(output, marker.as_bytes()) else {
        return false;
    };
    output[start + marker.len()..]
        .iter()
        .any(|byte| matches!(byte, b'\r' | b'\n'))
}

pub(super) fn run_pty_steps_with_size_until(
    command: CommandBuilder,
    steps: &[(&str, &[u8])],
    size: PtySize,
    stop_marker: &str,
) -> String {
    lossy_output(&run_pty_steps_with_size_until_raw(
        command,
        steps,
        size,
        stop_marker,
    ))
}

fn run_pty_steps_with_size_until_raw(
    mut command: CommandBuilder,
    steps: &[(&str, &[u8])],
    size: PtySize,
    stop_marker: &str,
) -> Vec<u8> {
    command.env("TERM", "xterm-256color");
    // portable-pty uses $HOME as the child cwd; the sandbox HOME may not
    // exist (ENOENT on spawn), and oracle cases use absolute paths only.
    command.cwd("/");
    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(size).expect("open pty");

    let mut child = pair.slave.spawn_command(command).expect("spawn command");
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().expect("pty reader");
    let mut writer = pair.master.take_writer().expect("pty writer");
    let (tx, rx) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let mut buf = [0_u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    // Generous bound for a loaded runner; success returns on the marker far earlier.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut out = Vec::new();
    let mut next_step = 0;
    let mut scan_start = 0;

    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(chunk) => {
                out.extend_from_slice(&chunk);
                if next_step < steps.len()
                    && find_bytes(&out[scan_start..], steps[next_step].0.as_bytes()).is_some()
                {
                    writer.write_all(steps[next_step].1).expect("write keys");
                    writer.flush().expect("flush keys");
                    next_step += 1;
                    scan_start = out.len();
                }
                if stop_marker_line_complete(&out, stop_marker) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    let _ = reader_thread.join();
    out
}

pub(super) fn run_pty_after_prompt(
    command: CommandBuilder,
    keys: &[u8],
    after_prompt: impl FnOnce(),
) -> String {
    run_pty_with_size_until_after_prompt(
        command,
        keys,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        "SUSHLINE_ACCEPTED:",
        Some(after_prompt),
    )
}

pub(super) fn run_pty_with_size_until(
    command: CommandBuilder,
    keys: &[u8],
    size: PtySize,
    stop_marker: &str,
) -> String {
    run_pty_with_size_until_after_prompt(command, keys, size, stop_marker, None::<fn()>)
}

pub(super) fn run_pty_with_size_until_after_prompt(
    mut command: CommandBuilder,
    keys: &[u8],
    size: PtySize,
    stop_marker: &str,
    after_prompt: Option<impl FnOnce()>,
) -> String {
    // Existing oracle helpers always use xterm, even if their caller sets TERM.
    command.env("TERM", "xterm-256color");
    command.cwd("/");
    run_pty_with_size_until_after_prompt_inner(command, keys, size, stop_marker, after_prompt)
}

// Only the TERM matrix bypasses the usual xterm override, including preserving
// an explicitly empty or removed TERM in the command environment.
fn run_pty_with_size_until_preserving_term(
    mut command: CommandBuilder,
    keys: &[u8],
    size: PtySize,
    stop_marker: &str,
) -> String {
    command.cwd("/");
    run_pty_with_size_until_after_prompt_inner(command, keys, size, stop_marker, None::<fn()>)
}

fn run_pty_with_size_until_after_prompt_inner<F: FnOnce()>(
    command: CommandBuilder,
    keys: &[u8],
    size: PtySize,
    stop_marker: &str,
    after_prompt: Option<F>,
) -> String {
    lossy_output(&run_pty_with_size_until_after_prompt_inner_raw(
        command,
        keys,
        size,
        stop_marker,
        after_prompt,
    ))
}

fn run_pty_with_size_until_after_prompt_inner_raw<F: FnOnce()>(
    command: CommandBuilder,
    keys: &[u8],
    size: PtySize,
    stop_marker: &str,
    mut after_prompt: Option<F>,
) -> Vec<u8> {
    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(size).expect("open pty");

    let mut child = pair.slave.spawn_command(command).expect("spawn command");
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().expect("pty reader");
    let mut writer = pair.master.take_writer().expect("pty writer");
    let (tx, rx) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let mut buf = [0_u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    // Poll the reader thread so the deadline stays effective while the child
    // starts slowly on a loaded runner; a blocking read would ignore it.
    // The generous timeout only binds the failure path: success returns on
    // the stop marker in well under a second.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut out = Vec::new();
    let mut sent_keys = false;

    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(chunk) => {
                out.extend_from_slice(&chunk);
                if !sent_keys && find_bytes(&out, READY_PROMPT.as_bytes()).is_some() {
                    if let Some(after_prompt) = after_prompt.take() {
                        after_prompt();
                    }
                    writer.write_all(keys).expect("write keys");
                    writer.flush().expect("flush keys");
                    sent_keys = true;
                }
                if stop_marker_line_complete(&out, stop_marker) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    let _ = reader_thread.join();
    out
}

fn lossy_output(output: &[u8]) -> String {
    String::from_utf8_lossy(output).into_owned()
}

pub(super) fn accepted_line(output: &str) -> Option<String> {
    let marker = "SUSHLINE_ACCEPTED:";
    accepted_line_after_marker(output, marker)
}

pub(super) fn accepted_numbered_line(output: &str, number: usize) -> Option<String> {
    accepted_line_after_marker(output, &format!("SUSHLINE_ACCEPTED_{number}:"))
}

pub(super) fn accepted_line_after_marker(output: &str, marker: &str) -> Option<String> {
    let start = output.find(marker)? + marker.len();
    let rest = &output[start..];
    let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

pub(super) fn bell_count(output: &str) -> usize {
    output.bytes().filter(|byte| *byte == b'\x07').count()
}

pub(super) fn assert_same_candidate_order(bash: &str, sushline: &str, candidates: &[&str]) {
    let bash_order = candidate_order(bash, candidates);
    let sushline_order = candidate_order(sushline, candidates);
    assert_eq!(
        sushline_order, bash_order,
        "bash={bash}\nsushline={sushline}"
    );
    assert_eq!(
        bash_order,
        candidates
            .iter()
            .map(|candidate| candidate.to_string())
            .collect::<Vec<_>>(),
        "bash={bash}"
    );
}

pub(super) fn candidate_order(output: &str, candidates: &[&str]) -> Vec<String> {
    let mut positions = candidates
        .iter()
        .map(|candidate| {
            (
                output.find(candidate).unwrap_or_else(|| {
                    panic!("candidate {candidate:?} missing from output {output:?}")
                }),
                (*candidate).to_string(),
            )
        })
        .collect::<Vec<_>>();
    positions.sort_by_key(|(position, _)| *position);
    positions
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect()
}

pub(super) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Capture settled redisplay checkpoints without accepting the line. Each
/// write is separated by an output-idle interval, so a byte-at-a-time test
/// does not accidentally become a single batched-input test.
pub(super) fn run_pty_checkpoints(
    mut command: CommandBuilder,
    prompt: &str,
    steps: &[Vec<u8>],
    size: PtySize,
) -> Vec<String> {
    command.env("TERM", "xterm-256color");
    command.cwd("/");
    let pair = NativePtySystem::default().openpty(size).expect("open pty");
    let mut child = pair.slave.spawn_command(command).expect("spawn command");
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().expect("pty reader");
    let mut writer = pair.master.take_writer().expect("pty writer");
    let (tx, rx) = mpsc::channel();
    let reader_thread = thread::spawn(move || {
        let mut buf = [0; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    // A short idle period can snapshot before delayed PTY redisplay arrives
    // when several oracle processes are scheduled concurrently.
    let idle_period = Duration::from_millis(100);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();
    let mut frames = Vec::new();
    let mut sent = 0;
    let mut ready = false;
    while Instant::now() < deadline {
        match rx.recv_timeout(idle_period) {
            Ok(chunk) => {
                output.extend_from_slice(&chunk);
                ready |= find_bytes(&output, prompt.as_bytes()).is_some();
            }
            Err(mpsc::RecvTimeoutError::Timeout) if ready => {
                if sent > 0 {
                    frames.push(lossy_output(&output));
                }
                if sent == steps.len() {
                    break;
                }
                writer.write_all(&steps[sent]).expect("write keys");
                writer.flush().expect("flush keys");
                sent += 1;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader_thread.join();
    assert_eq!(
        frames.len(),
        steps.len(),
        "PTY checkpoints did not settle all requested steps"
    );
    frames
}
