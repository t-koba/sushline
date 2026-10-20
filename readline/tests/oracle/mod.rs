//! Bash 5.3 PTY oracle groups organized by compatibility risk area. Each child
//! module documents its behavior scope; detailed claims and fixture boundaries
//! remain in `COMPATIBILITY.md`. The shared PTY harness is infrastructure, not
//! an independent compatibility claim.

mod completion;
mod diagnostics;
mod display;
mod editing;
mod history;
mod horizontal_scroll;
pub(super) mod host;
mod inputrc;
pub(super) mod pty;
pub(super) mod screen;
