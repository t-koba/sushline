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
fn interrupt_keeps_the_input_that_arrived_with_it() {
    // Custom backends retain Interrupted by default; the suffix remains
    // available to the next read rather than being dropped.
    let terminal = MemoryTerminal::with_events(vec![TerminalEvent::Bytes(b"abc\x03X\r".to_vec())]);
    let mut line = Editor::new(Config::default(), terminal, History::new());

    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Interrupted);
    assert!(line.terminal.out.contains("^C\r\n"));

    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line(b"X".to_vec()));
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

#[test]
fn sigint_policy_defaults_are_backend_specific() {
    let builtin = Editor::new(Config::default(), crate::Terminal::new(), History::new());
    assert_eq!(builtin.sigint_policy(), SigintPolicy::Continue);
    let custom = Editor::new(Config::default(), MemoryTerminal::default(), History::new());
    assert_eq!(custom.sigint_policy(), SigintPolicy::Interrupted);
}

#[test]
fn sigint_policy_preserves_suffix_for_terminal_events_and_input_bytes() {
    for policy in [
        None,
        Some(SigintPolicy::Interrupted),
        Some(SigintPolicy::Continue),
    ] {
        for events in [
            vec![
                TerminalEvent::Bytes(b"abc".to_vec()),
                TerminalEvent::Signal(libc::SIGINT),
                TerminalEvent::Bytes(b"X\rnext\r".to_vec()),
            ],
            vec![TerminalEvent::Bytes(b"abc\x03X\rnext\r".to_vec())],
            b"abc\x03X\rnext\r"
                .iter()
                .map(|byte| TerminalEvent::Bytes(vec![*byte]))
                .collect(),
        ] {
            let terminal = MemoryTerminal::with_events(events);
            let mut line = Editor::new(Config::default(), terminal, History::new());
            if let Some(policy) = policy {
                line.set_sigint_policy(policy);
            }
            let first = line.read_line(Prompt::new("> "), &mut ()).unwrap();
            if policy == Some(SigintPolicy::Continue) {
                assert_eq!(first, ReadlineResult::Line(b"abcX".to_vec()));
            } else {
                assert_eq!(first, ReadlineResult::Interrupted);
                assert_eq!(
                    line.read_line(Prompt::new("> "), &mut ()).unwrap(),
                    ReadlineResult::Line(b"X".to_vec())
                );
            }
            assert!(line.terminal.out.contains("^C\r\n"));
            assert_eq!(
                line.read_line(Prompt::new("> "), &mut ()).unwrap(),
                ReadlineResult::Line(b"next".to_vec())
            );
        }
    }
}

#[test]
fn hook_sigint_policy_preserves_partial_line_and_queued_suffix() {
    struct AfterPrefixHook(usize);
    impl Hooks for AfterPrefixHook {
        fn check_signals(&mut self) -> Option<i32> {
            self.0 += 1;
            (self.0 == 2).then_some(libc::SIGINT)
        }
    }
    for policy in [SigintPolicy::Continue, SigintPolicy::Interrupted] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"abc".to_vec()),
            TerminalEvent::Bytes(b"X\r".to_vec()),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.set_sigint_policy(policy);
        let first = line
            .read_line(Prompt::new("> "), &mut AfterPrefixHook(0))
            .unwrap();
        if policy == SigintPolicy::Continue {
            assert_eq!(first, ReadlineResult::Line(b"abcX".to_vec()));
            assert_eq!(line.terminal.input_meta_on_raw_entry.len(), 2);
        } else {
            assert_eq!(first, ReadlineResult::Interrupted);
            assert_eq!(
                line.read_line(Prompt::new("> "), &mut ()).unwrap(),
                ReadlineResult::Line(b"X".to_vec())
            );
        }
    }
}

#[test]
fn continued_sigint_restores_editor_terminal_modes_before_reading_again() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Signal(libc::SIGINT),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.set_sigint_policy(SigintPolicy::Continue);
    line.load_inputrc_str("set enable-bracketed-paste on")
        .unwrap();

    assert_eq!(
        line.read_line(Prompt::new("> "), &mut ()).unwrap(),
        ReadlineResult::Line(Vec::new())
    );
    assert_eq!(line.terminal.out.matches("\x1b[?2004h").count(), 2);
    assert_eq!(line.terminal.out.matches("\x1b[?2004l").count(), 1);
    assert_eq!(line.terminal.meta_enabled, [true, true, false]);
}
