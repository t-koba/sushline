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
        fn check_signals(&self) -> Option<i32> {
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
