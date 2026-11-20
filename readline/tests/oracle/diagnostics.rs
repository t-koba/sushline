//! Screen-level cases for the diagnostic sink.
//!
//! The cases redirect the diagnostic sink to a file, so the terminal screen
//! shows the editor's display and the lines the program printed before the
//! editor started, without the diagnostic text. A diagnostic that does not
//! reach the terminal's screen must not consume a terminal row, or the
//! redisplay after it starts above the rendered line and erases what the
//! terminal already showed.

use super::pty::*;
use super::screen::Screen;
use portable_pty::{CommandBuilder, PtySize};
use std::fs;
use std::path::Path;

/// The terminal size of the cases.
///
/// The narrow terminal wraps the rendered line, so the display spans more
/// than one row when the diagnostic arrives.
const SCREEN_ROWS: u16 = 24;
const SCREEN_COLUMNS: u16 = 20;

/// The lines the program prints before the editor starts.
const BANNERS: [&str; 2] = ["BANNER ONE", "BANNER TWO"];

/// The screen the terminal shows after the editor ran.
fn screen(output: &[u8]) -> Screen {
    let mut screen = Screen::new(SCREEN_ROWS as usize, SCREEN_COLUMNS as usize);
    screen.feed(output);
    screen
}

/// The shell command that prints the banner and then runs the editor.
fn banner_command() -> String {
    format!(
        "printf 'BANNER ONE\\nBANNER TWO\\n'; printf '{READY_PROMPT}'; IFS= read -r -e line; printf 'SUSHLINE_ACCEPTED:%s\\n' \"$line\""
    )
}

/// Runs Bash with the diagnostic sink redirected to a file.
fn run_bash_with_redirected_sink(dir: &Path, inputrc: &Path, keys: &[u8], sink: &Path) -> Vec<u8> {
    let mut command = CommandBuilder::new("sh");
    isolate_startup_inputrc(&mut command);
    command.args([
        "-c",
        &format!(
            "exec 2>{}; exec bash --noprofile --norc -i -c {}",
            shell_single_quote(&sink.display().to_string()),
            shell_single_quote(&banner_command()),
        ),
    ]);
    command.env("INPUTRC", inputrc.to_string_lossy().as_ref());
    run_pty_in_dir_with_size_raw_until(command, keys, dir, pty_size(), "SUSHLINE_ACCEPTED:")
}

/// Runs Sushline with the diagnostic sink redirected to a file.
fn run_sushline_with_redirected_sink(
    dir: &Path,
    inputrc: &Path,
    ambient_inputrc: &Path,
    keys: &[u8],
    sink: &Path,
) -> Vec<u8> {
    let mut command = CommandBuilder::new("sh");
    // Simulate a host with a conflicting startup file: the wrapper must clear
    // it before the harness applies its explicit inputrc overlay.
    command.env("INPUTRC", ambient_inputrc.to_string_lossy().as_ref());
    isolate_startup_inputrc(&mut command);
    command.args([
        "-c",
        &format!(
            "printf 'BANNER ONE\\nBANNER TWO\\n'; exec 2>{}; exec {}",
            shell_single_quote(&sink.display().to_string()),
            shell_single_quote(env!("CARGO_BIN_EXE_sushline-harness")),
        ),
    ]);
    command.env("SUSHLINE_INPUTRC_FILE", inputrc.to_string_lossy().as_ref());
    command.env("SUSHLINE_PROMPT", READY_PROMPT);
    command.env("SUSHLINE_HISTORY", "");
    run_pty_in_dir_with_size_raw_until(command, keys, dir, pty_size(), "SUSHLINE_ACCEPTED:")
}

/// The line the editor accepted, without the terminal's carriage return.
fn accepted_line(output: &[u8]) -> String {
    let output = String::from_utf8_lossy(output);
    output
        .split_once("SUSHLINE_ACCEPTED:")
        .map(|(_, rest)| rest.lines().next().unwrap_or_default().trim())
        .unwrap_or_default()
        .to_string()
}

fn pty_size() -> PtySize {
    PtySize {
        rows: SCREEN_ROWS,
        cols: SCREEN_COLUMNS,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// A diagnostic that does not reach the terminal's screen must not consume
/// a terminal row.
///
/// The banner lines are on the screen before the editor starts. A redisplay
/// that starts above the rendered line erases them, so both editors must
/// leave them on the screen when the diagnostic goes to a file.
#[test]
fn bash_and_sushline_keep_the_screen_when_the_diagnostic_sink_is_a_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let inputrc = dir.path().join("inputrc");
    fs::write(&inputrc, "\"\\C-o\": \"x\\C-oS\"\n").expect("write inputrc");
    let ambient_inputrc = dir.path().join("ambient-inputrc");
    fs::write(&ambient_inputrc, "\"Z\": \"zzz\"\n").expect("write ambient inputrc");
    // The line fills the narrow terminal, so the rendered line spans two
    // rows when the diagnostic arrives.
    let keys = b"0123456789ABCDEFGHIJ\x0fZ\r";
    let message = "readline: maximum macro execution nesting level exceeded";

    let bash_sink = dir.path().join("bash-sink");
    let bash_output = run_bash_with_redirected_sink(dir.path(), &inputrc, keys, &bash_sink);
    let sushline_sink = dir.path().join("sushline-sink");
    let sushline_output = run_sushline_with_redirected_sink(
        dir.path(),
        &inputrc,
        &ambient_inputrc,
        keys,
        &sushline_sink,
    );

    let bash_accepted = accepted_line(&bash_output);
    let sushline_accepted = accepted_line(&sushline_output);
    assert_eq!(bash_accepted, sushline_accepted);
    // The conflicting ambient binding would expand Z to zzz if the wrapped
    // launch did not clear it before applying the fixture's inputrc.
    assert!(bash_accepted.ends_with('Z'), "bash: {bash_accepted}");
    for (editor, output) in [("Bash", &bash_output), ("Sushline", &sushline_output)] {
        let screen = screen(output);
        for banner in BANNERS {
            assert!(
                screen.shows(banner),
                "{editor} screen: {:?}",
                screen.rows_text()
            );
        }
        // The diagnostic went to the sink, so it is not on the screen.
        assert!(
            !screen.shows(message),
            "{editor} screen: {:?}",
            screen.rows_text()
        );
    }
    for (editor, sink) in [("Bash", &bash_sink), ("Sushline", &sushline_sink)] {
        let diagnostics = fs::read_to_string(sink).unwrap_or_default();
        assert!(
            diagnostics.contains(&format!("{message}\n")),
            "{editor} sink: {diagnostics}"
        );
    }
    // The host redirects the line editor's output to standard error, while
    // the built-in terminal writes the display to standard output, so the
    // host's sink holds its line editor output and Sushline's sink holds
    // only the diagnostic.
    let bash_sink = fs::read_to_string(&bash_sink).unwrap_or_default();
    let sushline_sink = fs::read_to_string(&sushline_sink).unwrap_or_default();
    assert!(
        bash_sink.contains("0123456789ABCDEFGHIJ"),
        "bash sink: {bash_sink}"
    );
    assert!(
        !sushline_sink.contains("0123456789ABCDEFGHIJ"),
        "sushline sink: {sushline_sink}"
    );
}
