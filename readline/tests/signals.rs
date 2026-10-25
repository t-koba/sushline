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
    let mut retry = Terminal::new();
    let retry_error = retry
        .enter_raw_mode()
        .expect_err("non-TTY raw-mode retry must still fail for the terminal, not the lease");
    assert_ne!(
        retry_error.kind(),
        std::io::ErrorKind::WouldBlock,
        "failed entry must release the process-wide raw-mode lease"
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
                    let handler = if ignore
                        && [libc::SIGALRM, libc::SIGQUIT, libc::SIGWINCH].contains(&signal)
                    {
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
                if ignore {
                    assert_eq!(unsafe { libc::raise(libc::SIGQUIT) }, 0);
                    assert_eq!(
                        terminal.read_event(Some(Duration::ZERO)).unwrap(),
                        TerminalEvent::Timeout,
                        "inherited SIG_IGN keeps SIGQUIT ignored"
                    );
                }

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

        // The process-wide lease prevents a second Terminal, even one opted
        // out of signal catching, from restoring handlers while the first is
        // still raw. Releasing the first allows a later entry.
        set_action(libc::SIGTERM, application_handler as *const () as usize);
        let original_term = current_action(libc::SIGTERM);
        let mut first = Terminal::new();
        first.enter_raw_mode().unwrap();
        assert_ne!(
            current_action(libc::SIGTERM).sa_sigaction,
            original_term.sa_sigaction
        );

        let mut second = Terminal::new();
        second.set_catch_signals(false);
        second.set_catch_sigwinch(false);
        let error = second.enter_raw_mode().unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        second.restore_mode().unwrap();
        drop(second);
        assert_ne!(
            current_action(libc::SIGTERM).sa_sigaction,
            original_term.sa_sigaction
        );
        assert_eq!(unsafe { libc::raise(libc::SIGTERM) }, 0);
        assert_eq!(
            first.read_event(Some(Duration::ZERO)).unwrap(),
            TerminalEvent::Signal(libc::SIGTERM)
        );
        first.restore_mode().unwrap();
        assert_same_action(libc::SIGTERM, &original_term);

        let mut later = Terminal::new();
        later.set_catch_signals(false);
        later.set_catch_sigwinch(false);
        later.enter_raw_mode().unwrap();
        later.restore_mode().unwrap();
        assert_same_action(libc::SIGTERM, &original_term);
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

#[cfg(unix)]
#[test]
fn failed_restore_retains_termios_child_probe() {
    if std::env::var("SUSHLINE_RESTORE_RETRY_PROBE").as_deref() != Ok("1") {
        return;
    }
    use readline::{Terminal, TerminalIo};

    fn termios_equal(first: &libc::termios, second: &libc::termios) -> bool {
        first.c_iflag == second.c_iflag
            && first.c_oflag == second.c_oflag
            && first.c_cflag == second.c_cflag
            && first.c_lflag == second.c_lflag
            && first.c_cc == second.c_cc
    }

    unsafe {
        let mut initial: libc::termios = std::mem::zeroed();
        assert_eq!(
            libc::tcgetattr(libc::STDIN_FILENO, &mut initial),
            0,
            "record initial termios"
        );
        let mut terminal = Terminal::new();
        terminal.enter_raw_mode().unwrap();

        // Raw mode must change the observable mode so the retry check below
        // can distinguish a real restore from a discarded snapshot.
        let mut raw: libc::termios = std::mem::zeroed();
        assert_eq!(
            libc::tcgetattr(libc::STDIN_FILENO, &mut raw),
            0,
            "record raw termios"
        );
        assert!(
            !termios_equal(&initial, &raw),
            "raw mode must change termios"
        );

        let pty_copy = libc::dup(libc::STDIN_FILENO);
        assert!(pty_copy >= 0, "dup PTY stdin");
        let null_fd = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
        assert!(null_fd >= 0, "open /dev/null");
        assert_eq!(
            libc::dup2(null_fd, libc::STDIN_FILENO),
            libc::STDIN_FILENO,
            "redirect stdin to /dev/null"
        );
        libc::close(null_fd);

        assert!(
            terminal.restore_mode().is_err(),
            "restore on /dev/null must fail"
        );

        assert_eq!(
            libc::dup2(pty_copy, libc::STDIN_FILENO),
            libc::STDIN_FILENO,
            "restore PTY stdin"
        );
        libc::close(pty_copy);

        terminal.restore_mode().unwrap();

        let mut current: libc::termios = std::mem::zeroed();
        assert_eq!(
            libc::tcgetattr(libc::STDIN_FILENO, &mut current),
            0,
            "record restored termios"
        );
        assert!(
            termios_equal(&initial, &current),
            "failed restore must retain the snapshot so the retry restores the original mode"
        );
    }
}

#[cfg(unix)]
#[test]
fn failed_restore_can_be_retried() {
    use std::os::fd::{FromRawFd, OwnedFd};

    if std::env::var("SUSHLINE_RESTORE_RETRY_PROBE").as_deref() == Ok("1") {
        return;
    }
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
        "open PTY for isolated restore probe"
    );
    let _master = unsafe { OwnedFd::from_raw_fd(master) };
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "failed_restore_retains_termios_child_probe",
            "--exact",
            "--nocapture",
        ])
        .env("SUSHLINE_RESTORE_RETRY_PROBE", "1")
        .stdin(std::process::Stdio::from(slave))
        .output()
        .expect("spawn isolated restore probe");
    assert!(
        output.status.success(),
        "restore probe failed: {:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
mod builtin_sigint_policy {
    use super::*;
    use readline::{SigintPolicy, Terminal};
    use std::io::Write;
    use std::os::fd::{FromRawFd, OwnedFd};

    const PROBE_ENV: &str = "SUSHLINE_SIGINT_POLICY_PROBE";
    extern "C" fn caller_handler(_: i32) {}

    struct InputHook {
        master: std::fs::File,
        checks: usize,
        report_signal: bool,
    }

    impl Hooks for InputHook {
        fn check_signals(&mut self) -> Option<i32> {
            self.checks += 1;
            match self.checks {
                1 => self.master.write_all(b"abc").unwrap(),
                2 => {
                    // The prefix has been dispatched. Queue the suffix and
                    // deliver SIGINT without timing-dependent writer threads.
                    self.master.write_all(b"X\r").unwrap();
                    assert_eq!(unsafe { libc::raise(libc::SIGINT) }, 0);
                    if self.report_signal {
                        return Some(libc::SIGINT);
                    }
                }
                _ => {}
            }
            None
        }
    }

    #[test]
    fn child_probe() {
        if std::env::var(PROBE_ENV).as_deref() != Ok("1") {
            return;
        }
        // Bound the isolated probe even if a regression leaves read_line waiting.
        unsafe { libc::alarm(10) };
        let mut master = -1;
        let mut slave = -1;
        unsafe {
            assert_eq!(
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null()
                ),
                0
            );
        }
        let master = unsafe { std::fs::File::from_raw_fd(master) };
        let _slave = unsafe { OwnedFd::from_raw_fd(slave) };
        unsafe {
            assert!(libc::setsid() >= 0);
            // Closing the controlling PTY at child exit must not kill the test.
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            assert_eq!(libc::ioctl(slave, libc::TIOCSCTTY, 0), 0);
            assert_eq!(libc::dup2(slave, libc::STDIN_FILENO), libc::STDIN_FILENO);
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = caller_handler as *const () as usize;
            libc::sigemptyset(&mut action.sa_mask);
            assert_eq!(
                libc::sigaction(libc::SIGINT, &action, std::ptr::null_mut()),
                0
            );
        }
        for report_signal in [false, true] {
            for interrupted in [false, true] {
                let mut terminal = Terminal::new();
                terminal.set_catch_signals(!report_signal);
                let mut line = Editor::new(Config::default(), terminal, History::new());
                assert_eq!(line.sigint_policy(), SigintPolicy::Continue);
                if interrupted {
                    line.set_sigint_policy(SigintPolicy::Interrupted);
                }
                let mut hook = InputHook {
                    master: master.try_clone().unwrap(),
                    checks: 0,
                    report_signal,
                };
                let first = line.read_line(Prompt::new("> "), &mut hook).unwrap();
                if interrupted {
                    assert_eq!(first, ReadlineResult::Interrupted);
                    assert_eq!(
                        line.read_line(Prompt::new("> "), &mut ()).unwrap(),
                        ReadlineResult::Line(b"X".to_vec())
                    );
                } else {
                    assert_eq!(first, ReadlineResult::Line(b"abcX".to_vec()));
                }
                let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
                assert_eq!(
                    unsafe { libc::sigaction(libc::SIGINT, std::ptr::null(), &mut action) },
                    0
                );
                assert_eq!(action.sa_sigaction, caller_handler as *const () as usize);
            }
        }
        unsafe { libc::alarm(0) };
    }

    #[test]
    fn builtin_continues_by_default_and_can_return_interrupted() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "builtin_sigint_policy::child_probe",
                "--exact",
                "--nocapture",
            ])
            .env(PROBE_ENV, "1")
            .env("INPUTRC", "/dev/null")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "SIGINT policy probe failed: {:?}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
