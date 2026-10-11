mod common;

use common::MemoryTerminal;
use readline::{Config, Editor, History, Hooks, Prompt, ReadlineResult};
use std::cell::Cell;

#[test]
fn signal_cleanup_and_reset_restore_terminal_modes() {
    let terminal = MemoryTerminal::with_events(Vec::new());
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste on")
        .unwrap();
    line.reset_after_signal().unwrap();
    line.cleanup_after_signal().unwrap();
    assert!(line.terminal().out.contains("\x1b[?2004h"));
    assert!(line.terminal().out.contains("\x1b[?2004l"));
    assert_eq!(line.terminal().meta_enabled, vec![true, false]);
}

#[cfg(unix)]
#[test]
fn hooks_can_report_pending_sigint() {
    struct SignalHook {
        pending: Cell<bool>,
    }

    impl Hooks for SignalHook {
        fn check_signals(&mut self) -> Option<i32> {
            self.pending.replace(false).then_some(libc::SIGINT)
        }
    }

    let terminal = MemoryTerminal::with_events(Vec::new());
    let mut hooks = SignalHook {
        pending: Cell::new(true),
    };
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal().out.contains("^C\r\n"));
}

#[cfg(unix)]
#[test]
fn failed_raw_entry_child_probe() {
    if std::env::var("SUSHLINE_FAILED_ENTRY_PROBE").as_deref() != Ok("1") {
        return;
    }
    use readline::{Terminal, TerminalIo};
    fn current_handler(signal: libc::c_int) -> usize {
        unsafe {
            let mut current: libc::sigaction = std::mem::zeroed();
            assert_eq!(
                libc::sigaction(signal, std::ptr::null(), &mut current),
                0,
                "query disposition"
            );
            current.sa_sigaction
        }
    }
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        assert_eq!(
            libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut()),
            0,
            "set prior disposition"
        );
    }
    let before = current_handler(libc::SIGTERM);
    let mut terminal = Terminal::new();
    assert!(
        terminal.enter_raw_mode().is_err(),
        "non-TTY stdin must fail raw-mode entry"
    );
    assert_eq!(
        current_handler(libc::SIGTERM),
        before,
        "failed entry must not install handlers"
    );
    let _ = terminal.restore_mode();
    assert_eq!(
        current_handler(libc::SIGTERM),
        before,
        "restore after failed entry must preserve disposition"
    );
    drop(terminal);
    assert_eq!(
        current_handler(libc::SIGTERM),
        before,
        "Drop after failed entry must preserve disposition"
    );
}

#[cfg(unix)]
#[test]
fn failed_raw_entry_preserves_prior_disposition() {
    if std::env::var("SUSHLINE_FAILED_ENTRY_PROBE").as_deref() == Ok("1") {
        return;
    }
    let exe = std::env::current_exe().expect("current test exe");
    let output = std::process::Command::new(exe)
        .arg("failed_raw_entry_child_probe")
        .arg("--exact")
        .arg("--nocapture")
        .env("SUSHLINE_FAILED_ENTRY_PROBE", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("spawn child probe");
    assert!(
        output.status.success(),
        "child probe failed: status {:?} stdout {:?} stderr {:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
