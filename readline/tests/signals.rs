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

#[cfg(unix)]
mod terminal_signal_options {
    use readline::{Terminal, TerminalEvent, TerminalIo};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    const PROBE_ENV: &str = "SUSHLINE_SIGNAL_OPTIONS_PROBE";
    const SIGNALS: [i32; 9] = [
        libc::SIGINT,
        libc::SIGQUIT,
        libc::SIGTERM,
        libc::SIGHUP,
        libc::SIGALRM,
        libc::SIGTSTP,
        libc::SIGTTIN,
        libc::SIGTTOU,
        libc::SIGWINCH,
    ];
    static WINCH_CALLS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn application_handler(signal: i32) {
        if signal == libc::SIGWINCH {
            WINCH_CALLS.fetch_add(1, Ordering::SeqCst);
        }
    }

    extern "C" fn replacement_handler(_: i32) {}

    fn current_action(signal: i32) -> libc::sigaction {
        unsafe {
            let mut action = std::mem::zeroed();
            assert_eq!(libc::sigaction(signal, std::ptr::null(), &mut action), 0);
            action
        }
    }

    fn set_action(signal: i32, handler: usize) {
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = handler;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaddset(&mut action.sa_mask, libc::SIGUSR1);
            assert_eq!(libc::sigaction(signal, &action, std::ptr::null_mut()), 0);
        }
    }

    fn assert_same_action(signal: i32, expected: &libc::sigaction) {
        let actual = current_action(signal);
        assert_eq!(
            actual.sa_sigaction, expected.sa_sigaction,
            "signal {signal}"
        );
        assert_eq!(actual.sa_flags, expected.sa_flags, "signal {signal} flags");
        // Compare members, not padding in the platform's sigset_t.
        for member in SIGNALS.into_iter().chain([libc::SIGUSR1, libc::SIGUSR2]) {
            unsafe {
                assert_eq!(
                    libc::sigismember(&actual.sa_mask, member),
                    libc::sigismember(&expected.sa_mask, member),
                    "signal {signal} mask member {member}"
                );
            }
        }
    }

    #[test]
    fn child_probe() {
        if std::env::var(PROBE_ENV).as_deref() != Ok("1") {
            return;
        }
        // This child is the only test running in its process and has PTY
        // stdin. Exercise defaults and all four combinations in sequence to
        // cover resetting the shared installed state between the two groups.
        for ignore in [false, true] {
            for flags in [
                None,
                Some((false, false)),
                Some((true, false)),
                Some((false, true)),
                Some((true, true)),
            ] {
                for signal in SIGNALS {
                    let handler = if ignore && [libc::SIGALRM, libc::SIGWINCH].contains(&signal) {
                        libc::SIG_IGN
                    } else if signal == libc::SIGQUIT {
                        libc::SIG_DFL
                    } else {
                        application_handler as *const () as usize
                    };
                    set_action(signal, handler);
                }
                let mut expected: Vec<_> = SIGNALS.into_iter().map(current_action).collect();
                let mut terminal = Terminal::new();
                let (catch_signals, catch_sigwinch) = flags.unwrap_or((true, true));
                if flags.is_some() {
                    terminal.set_catch_signals(catch_signals);
                    terminal.set_catch_sigwinch(catch_sigwinch);
                }
                terminal.enter_raw_mode().unwrap();
                let changed: Vec<_> = SIGNALS
                    .iter()
                    .zip(&expected)
                    .map(|(&signal, action)| {
                        let selected = if signal == libc::SIGWINCH {
                            catch_sigwinch
                        } else {
                            catch_signals
                        };
                        let changed = selected && action.sa_sigaction != libc::SIG_IGN;
                        if changed {
                            assert_ne!(current_action(signal).sa_sigaction, action.sa_sigaction);
                        } else {
                            assert_same_action(signal, action);
                        }
                        changed
                    })
                    .collect();

                for signal in [libc::SIGINT, libc::SIGTERM] {
                    assert_eq!(unsafe { libc::raise(signal) }, 0);
                    assert_eq!(
                        terminal.read_event(Some(Duration::ZERO)).unwrap(),
                        if catch_signals {
                            TerminalEvent::Signal(signal)
                        } else {
                            TerminalEvent::Timeout
                        }
                    );
                }
                let winch_calls = WINCH_CALLS.load(Ordering::SeqCst);
                assert_eq!(unsafe { libc::raise(libc::SIGWINCH) }, 0);
                assert_eq!(
                    WINCH_CALLS.load(Ordering::SeqCst) - winch_calls,
                    usize::from(!ignore),
                    "saved SIGWINCH callback is chained only when not ignored"
                );
                assert_eq!(
                    terminal.read_event(Some(Duration::ZERO)).unwrap(),
                    if catch_sigwinch && !ignore {
                        TerminalEvent::Resize(terminal.size().unwrap())
                    } else {
                        TerminalEvent::Timeout
                    }
                );

                // Setters do not change an active installation, nor do they
                // determine which saved actions restoration must restore.
                terminal.set_catch_signals(!catch_signals);
                terminal.set_catch_sigwinch(!catch_sigwinch);
                let active: Vec<_> = SIGNALS.into_iter().map(current_action).collect();
                terminal.enter_raw_mode().unwrap();
                for (&signal, action) in SIGNALS.iter().zip(&active) {
                    assert_same_action(signal, action);
                }
                // Sushline must not restore actions it never changed, even if
                // the embedder replaces a disabled action mid-read. Leave
                // SIG_IGN unchanged to verify it also survives restoration.
                for (idx, &signal) in SIGNALS.iter().enumerate() {
                    if !changed[idx] && expected[idx].sa_sigaction != libc::SIG_IGN {
                        set_action(signal, replacement_handler as *const () as usize);
                        expected[idx] = current_action(signal);
                    }
                }
                if catch_signals == catch_sigwinch {
                    terminal.restore_mode().unwrap();
                    terminal.restore_mode().unwrap();
                }
                // Also exercise Drop restoration for the independently
                // selected groups.
                drop(terminal);
                for (&signal, action) in SIGNALS.iter().zip(&expected) {
                    assert_same_action(signal, action);
                }
            }
        }
    }

    #[test]
    fn all_flag_combinations_restore_dispositions() {
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    &size,
                )
            },
            0,
            "open PTY for isolated terminal probe"
        );
        let _master = unsafe { OwnedFd::from_raw_fd(master) };
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "terminal_signal_options::child_probe",
                "--exact",
                "--nocapture",
            ])
            .env(PROBE_ENV, "1")
            .stdin(Stdio::from(slave))
            .output()
            .expect("spawn isolated signal probe");
        assert!(
            output.status.success(),
            "signal probe failed: {:?}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
