//! Shared Bash/Sushline PTY setup used by the oracle groups; these helpers are
//! test infrastructure and do not assert an independent compatibility claim.

use super::host::require_oracle_host;
use portable_pty::{
    Child, CommandBuilder, ExitStatus, MasterPty, NativePtySystem, PtySize, PtySystem,
};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
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

/// Runs the GNU host's interrupt case, retrying a host hang.
///
/// GNU Readline re-raises the interrupt for the shell, and the shell's
/// handler can land in the startup of its next session, so the host
/// sometimes waits instead of showing the next prompt. The host then
/// reaches no stop marker and the driver waits for its bound. That race
/// belongs to the host: Sushline's editor returns the interrupt without
/// re-raising it. The case therefore retries the host run, accepts only a
/// run that shows everything the case asserts, and reports the attempts it
/// made.
pub(super) fn run_bash_interactive_after_ctrl_c() -> String {
    let mut output = String::new();
    for attempt in 1..=BASH_INTERRUPT_ATTEMPTS {
        output = run_bash_interactive_after_ctrl_c_once();
        if bash_interrupt_run_is_complete(&output) {
            if attempt > 1 {
                eprintln!("the GNU host run needed {attempt} attempts");
            }
            return output;
        }
        eprintln!("the GNU host run attempt {attempt} did not reach the next prompt");
    }
    output
}

/// Attempts for the GNU host run of the interrupt case.
const BASH_INTERRUPT_ATTEMPTS: usize = 5;

/// The markers the interrupt case asserts about the host run.
const BASH_INTERRUPT_MARKERS: [&str; 4] = [
    "^C",
    "SUSHLINE_READY1>^C",
    "SUSHLINE_READY2>",
    "SUSHLINE_ACCEPTED:ok",
];

/// Whether the host run shows everything the interrupt case asserts.
fn bash_interrupt_run_is_complete(output: &str) -> bool {
    BASH_INTERRUPT_MARKERS
        .iter()
        .all(|marker| output.contains(marker))
}

fn run_bash_interactive_after_ctrl_c_once() -> String {
    run_bash_interactive_after_ctrl_c_once_with_keys(b"abc\x03")
}

fn run_bash_interactive_after_ctrl_c_once_with_keys(keys: &[u8]) -> String {
    // Unique prompts prevent partial-line redisplay from releasing the next PTY step.
    let mut command = CommandBuilder::new("bash");
    command.env("PS1", "SUSHLINE_READY1>");
    command.env("__sushline_prompt_count", "0");
    command.env(
        "PROMPT_COMMAND",
        "__sushline_prompt_count=$((__sushline_prompt_count + 1)); PS1=\"SUSHLINE_READY${__sushline_prompt_count}>\"",
    );
    command.args(["--noprofile", "--norc", "-i"]);
    run_pty_steps_until_with_timeout(
        command,
        &[
            ("SUSHLINE_READY1>", keys),
            (
                "SUSHLINE_READY2>",
                b"printf 'SUSHLINE_ACCEPTED:%s\\n' ok\r".as_slice(),
            ),
        ],
        "SUSHLINE_ACCEPTED:ok",
        INTERRUPT_HANG_BOUND,
    )
}

/// The hang bound the PTY drivers use.
///
/// The bound only limits a hang: a driver returns as soon as its own
/// evidence is complete.
pub(super) const PTY_HANG_BOUND: Duration = Duration::from_secs(15);

/// A PTY session that owns the child's lifetime.
///
/// The helpers share the openpty, spawn, reader thread, deadline and kill
/// body, so the lifecycle lives here and each helper decides when its own
/// evidence is complete.
struct PtySession {
    child: Box<dyn Child + Send + Sync>,
    /// The PTY master, kept open for the session's lifetime so that its
    /// file descriptor stays valid for a driver that has to change the
    /// terminal's settings.
    #[cfg_attr(not(unix), allow(dead_code))]
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    output: mpsc::Receiver<Vec<u8>>,
    reader: Option<thread::JoinHandle<()>>,
}

/// What a read from a [`PtySession`] returned.
enum PtyOutput {
    /// The child wrote these bytes.
    Chunk(Vec<u8>),
    /// The child wrote nothing within the timeout.
    Timeout,
    /// The child closed its output.
    Closed,
}

impl PtySession {
    fn spawn(command: CommandBuilder, size: PtySize) -> Self {
        let pair = NativePtySystem::default().openpty(size).expect("open pty");
        let child = pair.slave.spawn_command(command).expect("spawn command");
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().expect("pty reader");
        let writer = pair.master.take_writer().expect("pty writer");
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
        Self {
            child,
            master: pair.master,
            writer,
            output: rx,
            reader: Some(reader_thread),
        }
    }

    /// Writes the bytes to the child's terminal.
    fn write(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write keys");
        self.writer.flush().expect("flush keys");
    }

    /// Reads what the child wrote, waiting at most `timeout`.
    fn read(&mut self, timeout: Duration) -> PtyOutput {
        match self.output.recv_timeout(timeout) {
            Ok(chunk) => PtyOutput::Chunk(chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => PtyOutput::Timeout,
            Err(mpsc::RecvTimeoutError::Disconnected) => PtyOutput::Closed,
        }
    }

    /// Whether the child has exited.
    fn exited(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }

    /// The PTY master's file descriptor.
    ///
    /// The master stays open for the session's lifetime, so the descriptor
    /// is valid while the session runs. A driver that has to change the
    /// terminal's settings needs it.
    #[cfg(unix)]
    fn master_fd(&self) -> std::os::unix::io::RawFd {
        self.master.as_raw_fd().expect("PTY master file descriptor")
    }

    /// The child's exit status, if it has exited.
    ///
    /// The check does not block: a driver that waits for the child polls
    /// it, and a driver that only asks whether the child is still alive
    /// reads `None`.
    fn child_exit_status(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Ends the session: the child is killed and the reader thread joined.
    fn finish(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

/// The hang bound for the interrupt case.
///
/// The host reaches its second prompt in a fraction of a second, so a
/// shorter bound keeps a host that waits from taking the whole suite's
/// budget.
const INTERRUPT_HANG_BOUND: Duration = Duration::from_secs(5);

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

/// Runs the shell's own history expansion and returns the separated streams.
fn bash_history_expand_output(expansion: &str, history: &[&str]) -> std::process::Output {
    let history_commands = history
        .iter()
        .map(|entry| format!("history -s {}", shell_single_quote(entry)))
        .collect::<Vec<_>>()
        .join("; ");
    require_oracle_host();
    Command::new("bash")
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
        .expect("bash must be available for history oracle tests")
}

pub(super) fn run_bash_history_expand_bytes(expansion: &str, history: &[&str]) -> Vec<u8> {
    let output = bash_history_expand_output(expansion, history);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
        .stdout
        .strip_suffix(b"\n")
        .unwrap_or(&output.stdout)
        .to_vec()
}

pub(super) fn run_bash_history_expand(expansion: &str, history: &[&str]) -> String {
    String::from_utf8_lossy(&run_bash_history_expand_bytes(expansion, history)).into_owned()
}

/// The shell's own streams for a failing history expansion, separated.
pub(super) fn run_bash_history_expand_failure(
    expansion: &str,
    history: &[&str],
) -> (String, String) {
    let output = bash_history_expand_output(expansion, history);
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
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

pub(super) fn run_bash_readline_three_reads_with_inputrc_file(
    keys: &[u8],
    inputrc: &str,
) -> String {
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
            r#"stty status undef dsusp undef lnext undef 2>/dev/null || true; IFS= read -r -e -p "{READY_PROMPT}" line1; printf 'SUSHLINE_ACCEPTED_1:%s\n' "$line1"; IFS= read -r -e -p "{READY_PROMPT}" line2; printf 'SUSHLINE_ACCEPTED_2:%s\n' "$line2"; IFS= read -r -e -p "{READY_PROMPT}" line3; printf 'SUSHLINE_ACCEPTED_3:%s\n' "$line3""#
        ),
    ]);
    run_pty_until(command, keys, "SUSHLINE_ACCEPTED_3:")
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
    // These interrupt fixtures exercise the explicit return-to-caller policy.
    command.env("SUSHLINE_SIGINT_POLICY", "interrupted");
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    run_pty_until(command, keys, stop_marker)
}

/// Runs the host's interrupt case with the given keys in the first step.
///
/// The keys may carry input that arrives with the interrupt, so the case can
/// observe whether the host accepts it in the next read.
pub(super) fn run_bash_interactive_after_ctrl_c_with_keys(keys: &[u8]) -> String {
    let mut output = String::new();
    for attempt in 1..=BASH_INTERRUPT_ATTEMPTS {
        output = run_bash_interactive_after_ctrl_c_once_with_keys(keys);
        if bash_interrupt_run_is_complete(&output) {
            if attempt > 1 {
                eprintln!("the GNU host run needed {attempt} attempts");
            }
            return output;
        }
        eprintln!("the GNU host run attempt {attempt} did not reach the next prompt");
    }
    output
}

pub(super) fn run_sushline_harness_after_ctrl_c() -> String {
    run_sushline_harness_after_ctrl_c_with_keys(b"abc\x03")
}

/// Runs the harness's interrupt case with the given keys in the first step.
///
/// The keys may carry input that arrives with the interrupt, so the case can
/// observe whether the editor accepts it in the next read.
pub(super) fn run_sushline_harness_after_ctrl_c_with_keys(keys: &[u8]) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", "");
    command.env("SUSHLINE_PROMPT", "SUSHLINE_READY");
    command.env("SUSHLINE_NUMBERED_PROMPTS", "1");
    command.env("SUSHLINE_HISTORY", "");
    command.env("SUSHLINE_READS", "2");
    command.env("SUSHLINE_CONTINUE_ON_INTERRUPT", "1");
    command.env("SUSHLINE_SIGINT_POLICY", "interrupted");
    run_pty_steps_until(
        command,
        &[
            ("SUSHLINE_READY1:", keys),
            ("SUSHLINE_READY2:", b"ok\r".as_slice()),
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

pub(super) fn run_sushline_harness_three_reads_with_inputrc(keys: &[u8], inputrc: &str) -> String {
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
    command.env("SUSHLINE_INPUTRC", inputrc);
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    command.env("SUSHLINE_READS", "3");
    run_pty_until(command, keys, "SUSHLINE_ACCEPTED_3:")
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
    require_oracle_host();
    command.env("TERM", "xterm-256color");
    command.cwd("/");
    let mut session = PtySession::spawn(
        command,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    );
    let master_fd = session.master_fd();
    let deadline = Instant::now() + PTY_HANG_BOUND;
    let mut output = Vec::new();
    let mut sent_eof_input = false;
    let mut eof_checkpoint_seen = false;
    let mut child_alive_at_eof_checkpoint = false;
    let mut master_open_at_eof_checkpoint = false;
    let mut accepted_before_followup = false;
    let mut sent_followup = false;

    while Instant::now() < deadline {
        match session.read(Duration::from_millis(20)) {
            PtyOutput::Chunk(chunk) => {
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
                    session.write(&paste);
                    sent_eof_input = true;
                }
                if sent_eof_input && !sent_followup && find_bytes(&output, b"partial").is_some() {
                    eof_checkpoint_seen = true;
                    accepted_before_followup = find_bytes(&output, b"SUSHLINE_ACCEPTED:").is_some();
                    child_alive_at_eof_checkpoint = session.child_exit_status().is_none();
                    let mut probe = std::mem::MaybeUninit::<libc::termios>::uninit();
                    master_open_at_eof_checkpoint =
                        unsafe { libc::tcgetattr(master_fd, probe.as_mut_ptr()) == 0 };
                    session.write(b"\n");
                    sent_followup = true;
                }
                if stop_marker_line_complete(&output, "SUSHLINE_ACCEPTED:") {
                    break;
                }
            }
            PtyOutput::Timeout => {}
            PtyOutput::Closed => break,
        }
    }

    let saw_accepted_line = stop_marker_line_complete(&output, "SUSHLINE_ACCEPTED:");
    let output_text = lossy_output(&output);
    session.finish();

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

/// Run a command under a PTY until it exits, and report its exit status.
///
/// The command is expected to exit on its own, so the helper waits for no
/// prompt and for no stop marker.
pub(super) fn run_pty_until_exit(
    mut command: CommandBuilder,
    size: PtySize,
) -> (String, Option<ExitStatus>) {
    require_oracle_host();
    command.env("TERM", "xterm-256color");
    command.cwd("/");
    let mut session = PtySession::spawn(command, size);
    let deadline = Instant::now() + PTY_HANG_BOUND;
    let mut out = Vec::new();
    while Instant::now() < deadline {
        match session.read(Duration::from_millis(50)) {
            PtyOutput::Chunk(chunk) => out.extend_from_slice(&chunk),
            PtyOutput::Timeout => {
                if session.child_exit_status().is_some() {
                    break;
                }
            }
            PtyOutput::Closed => break,
        }
    }
    // The child has exited, but the reader may still hold chunks that the
    // loop did not receive, so the channel is drained until the reader ends.
    if session.child_exit_status().is_some() {
        let drain_deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < drain_deadline {
            match session.read(Duration::from_millis(50)) {
                PtyOutput::Chunk(chunk) => out.extend_from_slice(&chunk),
                PtyOutput::Timeout => continue,
                PtyOutput::Closed => break,
            }
        }
    }
    // The reader stops at the end of the output, which can precede the
    // child's exit, so the status is polled for a short while.
    let mut status = session.child_exit_status();
    let status_deadline = Instant::now() + Duration::from_secs(5);
    while status.is_none() && Instant::now() < status_deadline {
        thread::sleep(Duration::from_millis(20));
        status = session.child_exit_status();
    }
    session.finish();
    (String::from_utf8_lossy(&out).to_string(), status)
}

pub(super) fn run_pty_with_size(command: CommandBuilder, keys: &[u8], size: PtySize) -> String {
    run_pty_with_size_until(command, keys, size, "SUSHLINE_ACCEPTED:")
}

/// The time the sink has to stay unchanged before the evidence is complete.
///
/// The host writes the failure and redraws the line right after it, so the
/// sink is read once it has settled.
const SINK_QUIET_PERIOD: Duration = Duration::from_millis(500);

/// Runs an interactive Bash whose standard error goes to a file, typing the
/// given keys one at a time, and returns the file's contents.
///
/// The interactive shell is the only harness in which the host's
/// `history-expand-line` command is bound and runs; the `read -r -e`
/// harness leaves it inert. The prompt goes to standard error with the rest
/// of the shell's output, so the keys are separated by a pause rather than
/// by a prompt.
///
/// The wait ends when the sink carries `expect` and has stopped changing:
/// what the host writes after the expected text belongs to the evidence,
/// because the host redraws the line after a failed expansion and that
/// redraw shows that the line was left unchanged. The deadline only bounds
/// a hang.
pub(super) fn run_bash_interactive_with_sink(
    keys: &[Vec<u8>],
    inputrc: &Path,
    sink: &Path,
    expect: &str,
) -> String {
    require_oracle_host();
    let mut command = CommandBuilder::new("sh");
    command.args([
        "-c",
        &format!(
            "exec 2>{}; exec bash --noprofile --norc -i",
            shell_single_quote(&sink.display().to_string())
        ),
    ]);
    command.env("INPUTRC", inputrc.to_string_lossy().as_ref());
    command.env("TERM", "xterm-256color");
    command.cwd("/");
    let mut session = PtySession::spawn(
        command,
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    );
    for key in keys {
        session.write(key);
        // The shell's prompt is on the redirected stream, so the next key
        // waits for the editor to draw it.
        thread::sleep(Duration::from_millis(300));
    }
    let deadline = Instant::now() + PTY_HANG_BOUND;
    let mut last = String::new();
    let mut changed = Instant::now();
    let mut seen = false;
    while Instant::now() < deadline {
        let text = sink_text(sink);
        let carries = text.contains(expect);
        if !seen || text != last {
            last = text;
            changed = Instant::now();
            seen = true;
        } else if carries && changed.elapsed() >= SINK_QUIET_PERIOD {
            // The expected text is there and nothing follows it.
            break;
        }
        if session.exited() {
            break;
        }
        // The host writes to the redirected stream, so the terminal output
        // carries none of the interaction; it is read to keep the child
        // from blocking on a full terminal.
        match session.read(Duration::from_millis(50)) {
            PtyOutput::Chunk(_) => {}
            PtyOutput::Timeout => {}
            PtyOutput::Closed => break,
        }
    }
    session.finish();
    sink_text(sink)
}

/// The current contents of the sink file.
fn sink_text(sink: &Path) -> String {
    std::fs::read_to_string(sink).unwrap_or_default()
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
    command: CommandBuilder,
    steps: &[(&str, &[u8])],
    size: PtySize,
    stop_marker: &str,
) -> Vec<u8> {
    run_pty_steps_in_dir_with_size_raw_until_count(
        command,
        steps,
        std::path::Path::new("/"),
        size,
        stop_marker,
        1,
        PTY_HANG_BOUND,
    )
}

/// Runs a command under a PTY with a bounded wait, in steps.
pub(super) fn run_pty_steps_until_with_timeout(
    command: CommandBuilder,
    steps: &[(&str, &[u8])],
    stop_marker: &str,
    timeout: Duration,
) -> String {
    lossy_output(&run_pty_steps_in_dir_with_size_raw_until_count(
        command,
        steps,
        std::path::Path::new("/"),
        PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
        stop_marker,
        1,
        timeout,
    ))
}

/// Cwd-aware multi-step PTY driver, shared by ordinary and batched oracle
/// cases. Steps are written in order as their markers appear. For batched
/// reads, use a distinct prompt per read: redisplay can repeat the current
/// prompt, so another occurrence of a shared prompt is not a new-read barrier.
/// Success returns once `stop_marker` has appeared `stop_count` times with the
/// final occurrence line-complete. The hang bound is the one the other
/// drivers use.
pub(super) fn run_pty_steps_in_dir_with_size_raw_until_count(
    mut command: CommandBuilder,
    steps: &[(&str, &[u8])],
    cwd: &std::path::Path,
    size: PtySize,
    stop_marker: &str,
    stop_count: usize,
    timeout: Duration,
) -> Vec<u8> {
    assert!(stop_count > 0, "stop marker count must be positive");
    require_oracle_host();
    command.env("TERM", "xterm-256color");
    command.cwd(cwd);
    let mut session = PtySession::spawn(command, size);
    let deadline = Instant::now() + timeout;
    let mut out = Vec::new();
    let mut next_step = 0;
    let mut scan_start = 0;

    while Instant::now() < deadline {
        match session.read(Duration::from_millis(20)) {
            PtyOutput::Chunk(chunk) => {
                out.extend_from_slice(&chunk);
                if let Some(keys) = take_pty_step(&out, steps, &mut next_step, &mut scan_start) {
                    session.write(keys);
                }
                if stop_marker_count_line_complete(&out, stop_marker, stop_count) {
                    break;
                }
            }
            PtyOutput::Timeout => {}
            PtyOutput::Closed => break,
        }
    }

    session.finish();
    out
}

// Isolate step selection so framing can be tested with exact PTY read chunks.
fn take_pty_step<'a>(
    output: &[u8],
    steps: &[(&str, &'a [u8])],
    next_step: &mut usize,
    scan_start: &mut usize,
) -> Option<&'a [u8]> {
    let (marker, keys) = steps.get(*next_step)?;
    find_bytes(&output[*scan_start..], marker.as_bytes())?;
    *next_step += 1;
    *scan_start = output.len();
    Some(keys)
}

fn stop_marker_count_line_complete(output: &[u8], marker: &str, count: usize) -> bool {
    let needle = marker.as_bytes();
    if needle.is_empty() {
        return false;
    }
    let mut found = 0;
    let mut pos = 0;
    let mut last_start: Option<usize> = None;
    while pos + needle.len() <= output.len() {
        if let Some(offset) = find_bytes(&output[pos..], needle) {
            let start = pos + offset;
            found += 1;
            if found == count {
                last_start = Some(start);
                break;
            }
            pos = start + needle.len();
        } else {
            break;
        }
    }
    let Some(start) = last_start else {
        return false;
    };
    output[start + needle.len()..]
        .iter()
        .any(|byte| matches!(byte, b'\r' | b'\n'))
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
    require_oracle_host();
    let mut session = PtySession::spawn(command, size);
    // Poll the reader thread so the deadline stays effective while the child
    // starts slowly on a loaded runner; a blocking read would ignore it.
    // The hang bound only binds the failure path: success returns on the
    // stop marker in well under a second.
    let deadline = Instant::now() + PTY_HANG_BOUND;
    let mut out = Vec::new();
    let mut sent_keys = false;

    while Instant::now() < deadline {
        match session.read(Duration::from_millis(20)) {
            PtyOutput::Chunk(chunk) => {
                out.extend_from_slice(&chunk);
                if !sent_keys && find_bytes(&out, READY_PROMPT.as_bytes()).is_some() {
                    if let Some(after_prompt) = after_prompt.take() {
                        after_prompt();
                    }
                    session.write(keys);
                    sent_keys = true;
                }
                if stop_marker_line_complete(&out, stop_marker) {
                    break;
                }
            }
            PtyOutput::Timeout => {}
            PtyOutput::Closed => break,
        }
    }

    session.finish();
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

/// Capture redisplay before dispatching the next complete key step.
///
/// Each step is followed by Ctrl-G with its default abort binding and audible
/// bell. Both editors finish ordinary key redisplay before dispatching abort;
/// the BEL is thus a causal barrier, not an output-idle guess. Capture only
/// bytes preceding that BEL, even when a read also contains abort's redisplay.
/// The built-in Sushline terminal reads one byte per event; batching that read
/// would require revalidating this ordering.
///
/// Callers must use complete, non-belling commands in the default Emacs map.
/// Do not use this inside paste, key prefixes, searches, quoted insert, numeric
/// arguments or macros, or for Enter/EOF: abort is not inert in those states.
/// Paste callers instead barrier after the terminator and wait for the accepted
/// line after Enter. The deadline is only a hang bound, never a success signal.
pub(super) fn run_pty_checkpoints(
    command: CommandBuilder,
    prompt: &str,
    steps: &[Vec<u8>],
    size: PtySize,
) -> Vec<String> {
    run_pty_checkpoints_raw(command, prompt, steps, size)
        .iter()
        .map(|frame| lossy_output(frame))
        .collect()
}

/// The checkpoint frames as raw bytes, so a caller can observe output that
/// is not valid UTF-8.
pub(super) fn run_pty_checkpoints_raw(
    mut command: CommandBuilder,
    prompt: &str,
    steps: &[Vec<u8>],
    size: PtySize,
) -> Vec<Vec<u8>> {
    require_oracle_host();
    command.env("TERM", "xterm-256color");
    command.cwd("/");
    let mut session = PtySession::spawn(command, size);
    let deadline = Instant::now() + PTY_HANG_BOUND;
    let mut output = Vec::new();
    let mut frames = Vec::new();
    let mut sent = 0;
    let mut scan_start = 0;
    let mut ready = false;
    while frames.len() < steps.len() && Instant::now() < deadline {
        match session.read(deadline.saturating_duration_since(Instant::now())) {
            PtyOutput::Chunk(chunk) => {
                output.extend_from_slice(&chunk);
                let send_next = if !ready {
                    ready = find_bytes(&output, prompt.as_bytes()).is_some();
                    ready
                } else if let Some(frame) = checkpoint_before_bell_raw(&mut output, scan_start) {
                    frames.push(frame);
                    true
                } else {
                    false
                };
                if send_next && sent < steps.len() {
                    scan_start = output.len();
                    // The step and its barrier travel together, so the
                    // barrier cannot arrive before the step's redisplay.
                    let mut keys = steps[sent].clone();
                    keys.extend_from_slice(b"\x07");
                    session.write(&keys);
                    sent += 1;
                }
            }
            PtyOutput::Timeout | PtyOutput::Closed => break,
        }
    }
    session.finish();
    assert_eq!(
        frames.len(),
        steps.len(),
        "PTY checkpoints did not receive all abort barriers"
    );
    frames
}

// Remove the barrier itself but retain post-BEL output for the next frame.
fn checkpoint_before_bell(output: &mut Vec<u8>, scan_start: usize) -> Option<String> {
    Some(lossy_output(&checkpoint_before_bell_raw(
        output, scan_start,
    )?))
}

fn checkpoint_before_bell_raw(output: &mut Vec<u8>, scan_start: usize) -> Option<Vec<u8>> {
    let bell = scan_start
        + output[scan_start..]
            .iter()
            .position(|byte| *byte == b'\x07')?;
    let frame = output[..bell].to_vec();
    output.remove(bell);
    Some(frame)
}

#[test]
fn checkpoint_barrier_excludes_post_bell_bytes_and_survives_split_output() {
    require_oracle_host();
    let mut output = b"prompt".to_vec();
    let start = output.len();
    for chunk in [b"re".as_slice(), b"display"] {
        output.extend_from_slice(chunk);
        assert!(checkpoint_before_bell(&mut output, start).is_none());
    }
    output.extend_from_slice(b"\x07abort redisplay");
    assert_eq!(
        checkpoint_before_bell(&mut output, start).as_deref(),
        Some("promptredisplay")
    );
    assert_eq!(output, b"promptredisplayabort redisplay");
    let start = output.len();
    output.extend_from_slice(b"next\x07not part of this frame");
    assert_eq!(
        checkpoint_before_bell(&mut output, start).as_deref(),
        Some("promptredisplayabort redisplaynext")
    );
}

#[test]
fn numbered_step_markers_ignore_redisplay_and_split_accepted_hex() {
    require_oracle_host();
    let steps = [
        ("SUSHLINE_READY>1:", b"first\r".as_slice()),
        ("SUSHLINE_READY>2:", b"second\r".as_slice()),
    ];
    let next_read = b"\r\nSUSHLINE_READY>2:";
    // Exercise every boundary, including CR/LF and inside the next prompt.
    for split in 0..=next_read.len() {
        let mut output = b"SUSHLINE_READY>1:".to_vec();
        let (mut step, mut start) = (0, 0);
        assert_eq!(
            take_pty_step(&output, &steps, &mut step, &mut start),
            Some(steps[0].1)
        );
        // A redraw arrives separately, followed by an accepted hex payload
        // whose newline has not arrived yet. A shared prompt would send the
        // second edit here, allowing its tty echo into that same hex line.
        for chunk in [
            b"\rSUSHLINE_READY>1:first".as_slice(),
            b"\r\nSUSHLINE_ACCEPTED_HEX:6669727374",
        ] {
            output.extend_from_slice(chunk);
            assert_eq!(take_pty_step(&output, &steps, &mut step, &mut start), None);
        }
        output.extend_from_slice(&next_read[..split]);
        let sent = take_pty_step(&output, &steps, &mut step, &mut start);
        assert_eq!(sent, (split == next_read.len()).then_some(steps[1].1));
        output.extend_from_slice(&next_read[split..]);
        assert_eq!(
            take_pty_step(&output, &steps, &mut step, &mut start),
            (split != next_read.len()).then_some(steps[1].1)
        );
        assert_eq!(take_pty_step(&output, &steps, &mut step, &mut start), None);
        assert_eq!(step, steps.len());
    }
}
