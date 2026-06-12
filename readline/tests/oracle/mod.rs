mod completion;
mod display;
mod editing;
mod history;
mod inputrc;
pub(super) mod pty;

use std::process::Command;

#[test]
#[ignore = "requires a local GNU bash/readline oracle and PTY driver"]
fn oracle_bash_version_is_available() {
    let output = Command::new("bash")
        .arg("--version")
        .output()
        .expect("bash must be available for oracle tests");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("GNU bash"));
}
