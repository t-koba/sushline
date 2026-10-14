use super::*;

#[test]
fn interrupted_readline_cleans_up_terminal_modes() {
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Signal(libc::SIGINT)]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set enable-bracketed-paste on")
        .unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal.out.contains("> ^C\r\n"));
    assert!(line.terminal.out.contains("\x1b[?2004h"));
    assert!(line.terminal.out.contains("\x1b[?2004l"));
}

#[test]
fn interrupted_readline_marks_current_input_line() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Signal(libc::SIGINT),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());

    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();

    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal.out.contains("> ^C\r\n"));
    assert!(line.terminal.cleared_screen > 0);
}

struct OnceSignalHook {
    signal: Option<i32>,
}

impl crate::hooks::Hooks for OnceSignalHook {
    fn check_signals(&mut self) -> Option<i32> {
        self.signal.take()
    }
}

#[test]
fn hook_sigint_takes_same_interrupt_path_as_terminal_signal() {
    // `check_signals` SIGINT must echo `^C` and interrupt exactly like a
    // terminal SIGINT event after the shared restore/echo path.
    let terminal = MemoryTerminal::default();
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let mut hooks = OnceSignalHook {
        signal: Some(libc::SIGINT),
    };
    let result = line.read_line(Prompt::new("> "), &mut hooks).unwrap();
    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal.out.contains("> ^C\r\n"));
}

#[test]
fn timeout_rechecks_embedder_signal_hook() {
    struct AfterTimeoutHook {
        checks: usize,
    }

    impl Hooks for AfterTimeoutHook {
        fn check_signals(&mut self) -> Option<i32> {
            self.checks += 1;
            (self.checks == 2).then_some(libc::SIGINT)
        }
    }

    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Timeout]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let mut hooks = AfterTimeoutHook { checks: 0 };
    assert_eq!(
        line.read_line(Prompt::new("> "), &mut hooks).unwrap(),
        ReadlineResult::Interrupted
    );
    assert_eq!(hooks.checks, 2);
    assert!(line.terminal.out.contains("^C\r\n"));
}
