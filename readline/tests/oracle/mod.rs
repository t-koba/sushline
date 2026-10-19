//! Bash 5.3 PTY oracle groups organized by compatibility risk area. Each child
//! module documents its behavior scope; detailed claims and fixture boundaries
//! remain in `COMPATIBILITY.md`. The shared PTY harness is infrastructure, not
//! an independent compatibility claim.

mod completion;
mod display;
mod editing;
mod history;
mod horizontal_scroll;
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
