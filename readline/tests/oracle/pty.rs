use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::fs;
use std::io::{Read, Write};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub(super) const READY_PROMPT: &str = "SUSHLINE_READY>";
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

pub(super) fn run_bash_readline_with_inputrc_file(keys: &[u8], inputrc: &str) -> String {
    run_bash_readline_with_inputrc_file_and_env(keys, inputrc, &[])
}

pub(super) fn run_bash_readline_with_inputrc_file_and_env(
    keys: &[u8],
    inputrc: &str,
    env: &[(&str, &str)],
) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("inputrc");
    fs::write(&path, inputrc).expect("write inputrc");
    run_bash_readline_with_inputrc_path_and_env(keys, &path, env)
}

pub(super) fn run_bash_readline_with_inputrc_file_and_history(
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
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
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
    run_sushline_harness_with_size(
        keys,
        inputrc,
        history,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
}

pub(super) fn run_sushline_harness_with_size(
    keys: &[u8],
    inputrc: &str,
    history: &[&str],
    size: PtySize,
) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", history.join("\n"));
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

pub(super) fn run_pty_steps_with_size_until(
    mut command: CommandBuilder,
    steps: &[(&str, &[u8])],
    size: PtySize,
    stop_marker: &str,
) -> String {
    command.env("TERM", "xterm-256color");
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
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut out = Vec::new();
    let mut next_step = 0;
    let mut scan_start = 0;

    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(chunk) => {
                out.extend_from_slice(&chunk);
                let output = String::from_utf8_lossy(&out);
                let new_output = String::from_utf8_lossy(&out[scan_start..]);
                if next_step < steps.len() && new_output.contains(steps[next_step].0) {
                    writer.write_all(steps[next_step].1).expect("write keys");
                    writer.flush().expect("flush keys");
                    next_step += 1;
                    scan_start = out.len();
                }
                if output.contains(stop_marker) {
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
    String::from_utf8_lossy(&out).into_owned()
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
    mut after_prompt: Option<impl FnOnce()>,
) -> String {
    command.env("TERM", "xterm-256color");
    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(size).expect("open pty");

    let mut child = pair.slave.spawn_command(command).expect("spawn command");
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().expect("pty reader");
    let mut writer = pair.master.take_writer().expect("pty writer");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut out = Vec::new();
    let mut buf = [0_u8; 1024];
    let mut sent_keys = false;

    while Instant::now() < deadline {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if !sent_keys && String::from_utf8_lossy(&out).contains(READY_PROMPT) {
                    if let Some(after_prompt) = after_prompt.take() {
                        after_prompt();
                    }
                    writer.write_all(keys).expect("write keys");
                    writer.flush().expect("flush keys");
                    sent_keys = true;
                }
                if String::from_utf8_lossy(&out).contains(stop_marker) {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    String::from_utf8_lossy(&out).into_owned()
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
