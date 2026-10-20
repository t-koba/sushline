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
fn oracle_bash_version_matches_the_documented_baseline() {
    // The oracle claims are Bash runtime observations, so the host must be
    // the documented GNU Bash 5.3.
    let output = Command::new("bash")
        .arg("--version")
        .output()
        .expect("bash must be available for oracle tests");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("GNU bash"), "{stdout}");
    assert!(
        stdout.contains("version 5.3"),
        "the oracle host must be GNU Bash 5.3: {stdout}"
    );
}
