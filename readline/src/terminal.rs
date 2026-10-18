//! Terminal I/O boundary.
//!
//! Terminfo-provided capabilities remain terminfo-backed where sushline
//! already used them. Cursor movement and clear-to-end operations use fixed
//! ANSI escape sequences. Parameterized terminfo expansion for those helpers is
//! intentionally not part of the terminal backend.
//!
//! The concrete terminal backend is Unix-oriented. Non-Unix targets compile,
//! but live terminal operations return `Unsupported` until a backend is added.

use std::io::{self, Stdout, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;

pub(crate) mod escape;
mod terminfo;
pub(crate) use terminfo::{DefaultCapabilities, default_capabilities};
use terminfo::{terminfo_keypad_sequence, terminfo_sequence};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// TerminalSize.
pub struct TerminalSize {
    /// Columns.
    pub columns: u16,
    /// Rows.
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// TerminalEvent.
pub enum TerminalEvent {
    /// Bytes.
    Bytes(Vec<u8>),
    /// Resize.
    Resize(TerminalSize),
    /// Signal.
    Signal(i32),
    /// Timeout.
    Timeout,
}

/// TerminalIo.
pub trait TerminalIo {
    /// Enters raw terminal mode.
    fn enter_raw_mode(&mut self) -> io::Result<()>;
    /// Restores the previous terminal mode.
    fn restore_mode(&mut self) -> io::Result<()>;
    /// Reads the next terminal event.
    fn read_event(&mut self, timeout: Option<Duration>) -> io::Result<TerminalEvent>;
    /// Writes UTF-8 text to the terminal.
    fn write(&mut self, text: &str) -> io::Result<()>;
    /// Writes bytes to the terminal.
    fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Flushes pending terminal output.
    fn flush(&mut self) -> io::Result<()>;
    /// Returns the terminal size.
    fn size(&self) -> io::Result<TerminalSize>;
    /// Clears from the cursor to the end of the line.
    fn clear_after_cursor(&mut self) -> io::Result<()>;
    /// Clears from the cursor to the end of the screen.
    fn clear_to_screen_end(&mut self) -> io::Result<()> {
        self.clear_after_cursor()
    }
    /// Clears the display from the first column.
    fn clear_display(&mut self) -> io::Result<()> {
        self.write("\r")?;
        self.write(escape::CLEAR_TO_SCREEN_END)
    }
    /// Moves the cursor to a zero-based terminal column.
    fn move_to_column(&mut self, column: u16) -> io::Result<()>;
    /// Moves the cursor up by `rows` terminal rows.
    fn move_up(&mut self, _rows: u16) -> io::Result<()> {
        Ok(())
    }
    /// Emits a visible bell.
    fn visible_bell(&mut self) -> io::Result<()> {
        self.write(escape::VISIBLE_BELL)
    }
    /// Notifies the backend that Readline meta-key handling is enabled or disabled.
    ///
    /// The built-in [`Terminal`] does not emit a terminal mode sequence for this
    /// setting. Custom backends may implement their own terminal policy.
    fn set_meta_key_enabled(&mut self, _enabled: bool) -> io::Result<()> {
        Ok(())
    }
    /// Enables or disables application keypad mode.
    fn set_application_keypad_enabled(&mut self, _enabled: bool) -> io::Result<()> {
        Ok(())
    }
    /// Returns bindings for tty special characters.
    fn tty_special_bindings(&self) -> Vec<(u8, &'static str)> {
        Vec::new()
    }
}

/// Built-in stdin/stdout terminal backend.
///
/// Only one built-in `Terminal` may hold raw mode at a time because termios and
/// signal dispositions are process-wide; a second active entry returns
/// `WouldBlock`.
pub struct Terminal {
    stdout: Stdout,
    raw: bool,
    catch_signals: bool,
    catch_sigwinch: bool,
    #[cfg(unix)]
    saved_termios: Option<libc::termios>,
}

impl Terminal {
    /// Creates a built-in terminal with both signal-catching groups enabled.
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            raw: false,
            catch_signals: true,
            catch_sigwinch: true,
            #[cfg(unix)]
            saved_termios: None,
        }
    }

    /// Sets whether the built-in terminal catches SIGINT, SIGQUIT, SIGTERM,
    /// SIGHUP, SIGALRM, SIGTSTP, SIGTTIN, and SIGTTOU on Unix.
    ///
    /// Defaults to `true`, independently of [`Self::set_catch_sigwinch`].
    /// Configure this before `Editor::read_line` or raw-mode entry; changes
    /// take effect on the next raw-mode entry, not during an active read.
    /// An inherited `SIG_IGN` is preserved for every selected signal.
    /// Disabling it leaves these process-wide dispositions to the embedder,
    /// which can report pending signals through `Hooks::check_signals`.
    /// This is built-in `Terminal` policy, not policy for custom [`TerminalIo`]
    /// implementations, and does not change the editor's SIGINT result policy.
    pub fn set_catch_signals(&mut self, catch: bool) {
        self.catch_signals = catch;
    }

    /// Sets whether the built-in terminal catches SIGWINCH on Unix.
    ///
    /// Defaults to `true`, independently of [`Self::set_catch_signals`].
    /// Configure this before `Editor::read_line` or raw-mode entry; changes
    /// take effect on the next raw-mode entry, not during an active read.
    /// When enabled, an inherited `SIG_IGN` is preserved; otherwise the saved
    /// handler is chained and resize events are reported. Disabling it leaves
    /// the process-wide disposition to the embedder. This option does not
    /// affect custom [`TerminalIo`] implementations.
    pub fn set_catch_sigwinch(&mut self, catch: bool) {
        self.catch_sigwinch = catch;
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.restore_mode();
    }
}

impl TerminalIo for Terminal {
    fn enter_raw_mode(&mut self) -> io::Result<()> {
        if self.raw {
            return Ok(());
        }
        #[cfg(unix)]
        {
            if RAW_TERMINAL_ACTIVE
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "another built-in terminal already holds raw mode",
                ));
            }
            // Acquire terminal state before installing process-global handlers
            // so a failed entry (for example ENOTTY on non-TTY stdin) leaves
            // no handlers behind and releases the process-wide raw-mode lease.
            if let Err(error) = enable_readline_mode(&mut self.saved_termios) {
                RAW_TERMINAL_ACTIVE.store(false, Ordering::SeqCst);
                return Err(error);
            }
            install_signal_handlers(self.catch_signals, self.catch_sigwinch);
            self.raw = true;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            Err(unsupported_terminal_io())
        }
    }

    fn restore_mode(&mut self) -> io::Result<()> {
        if self.raw {
            #[cfg(unix)]
            {
                restore_readline_mode(&mut self.saved_termios)?;
                restore_signal_handlers();
                self.raw = false;
                RAW_TERMINAL_ACTIVE.store(false, Ordering::SeqCst);
            }
            #[cfg(not(unix))]
            {
                return Err(unsupported_terminal_io());
            }
        }
        Ok(())
    }

    fn read_event(&mut self, timeout: Option<Duration>) -> io::Result<TerminalEvent> {
        #[cfg(unix)]
        {
            read_raw_event(timeout)
        }

        #[cfg(not(unix))]
        {
            let _ = timeout;
            Err(unsupported_terminal_io())
        }
    }

    fn write(&mut self, text: &str) -> io::Result<()> {
        self.stdout.write_all(text.as_bytes())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.stdout.write_all(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }

    fn size(&self) -> io::Result<TerminalSize> {
        query_terminal_size()
    }

    fn clear_after_cursor(&mut self) -> io::Result<()> {
        self.stdout
            .write_all(escape::CLEAR_TO_LINE_END.as_bytes())?;
        self.stdout.flush()
    }

    fn clear_to_screen_end(&mut self) -> io::Result<()> {
        self.stdout
            .write_all(escape::CLEAR_TO_SCREEN_END.as_bytes())?;
        self.stdout.flush()
    }

    fn clear_display(&mut self) -> io::Result<()> {
        if let Some(sequence) = terminfo_sequence("clear") {
            self.stdout.write_all(&sequence)?;
        } else {
            self.stdout.write_all(escape::CLEAR_ALL.as_bytes())?;
        }
        if let Some(sequence) = terminfo_sequence("E3") {
            self.stdout.write_all(&sequence)
        } else {
            self.stdout.write_all(escape::ERASE_SCROLLBACK.as_bytes())
        }
    }

    fn move_to_column(&mut self, column: u16) -> io::Result<()> {
        self.stdout
            .write_all(escape::move_to_column(column).as_bytes())
    }

    fn move_up(&mut self, rows: u16) -> io::Result<()> {
        self.stdout.write_all(escape::move_up(rows).as_bytes())
    }

    fn visible_bell(&mut self) -> io::Result<()> {
        if let Some(sequence) = terminfo_sequence("flash") {
            self.stdout.write_all(&sequence)
        } else {
            self.stdout.write_all(escape::VISIBLE_BELL.as_bytes())
        }
    }

    fn set_application_keypad_enabled(&mut self, enabled: bool) -> io::Result<()> {
        if let Some(sequence) = terminfo_keypad_sequence(enabled) {
            self.stdout.write_all(&sequence)
        } else {
            Ok(())
        }
    }

    fn tty_special_bindings(&self) -> Vec<(u8, &'static str)> {
        tty_special_bindings()
    }
}

#[cfg(unix)]
fn prepare_readline_termios(readline: &mut libc::termios) {
    // Preserve configured software flow control. When enabled, the line
    // discipline consumes VSTOP/VSTART instead of dispatching them as editor
    // keys, matching the tested Bash PTY behavior.
    // Match GNU prepare_terminal_settings for the translation bits that need
    // no policy knob: CR/NL translation off unconditionally so RET (CR) stays
    // distinct from C-J (LF) for isearch dispatch.
    readline.c_iflag &= !(libc::ICRNL | libc::INLCR);
    // Match GNU `rltty.c:prepare_terminal_settings` eight-bit preparation for
    // the CS8 case: an inherited ISTRIP would strip high bits in the kernel
    // before any input-meta decoder could recover them, corrupting UTF-8.
    // Narrower than GNU (which also covers CS7+meta_flag): CS7 behavior is
    // left unchanged.
    if readline.c_cflag & libc::CSIZE == libc::CS8 {
        readline.c_iflag &= !(libc::ISTRIP | libc::INPCK);
    }
}

#[cfg(unix)]
fn enable_readline_mode(saved: &mut Option<libc::termios>) -> io::Result<()> {
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    let rc = unsafe { libc::tcgetattr(libc::STDIN_FILENO, termios.as_mut_ptr()) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }

    let original = unsafe { termios.assume_init() };
    if saved.is_none() {
        *saved = Some(original);
    }

    let mut readline = original;
    prepare_readline_termios(&mut readline);

    readline.c_lflag &= !(libc::ICANON | libc::ECHO | libc::IEXTEN);
    readline.c_cc[libc::VMIN] = 1;
    readline.c_cc[libc::VTIME] = 0;

    let rc = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &readline) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn restore_readline_mode(saved: &mut Option<libc::termios>) -> io::Result<()> {
    let Some(termios) = saved.take() else {
        return Ok(());
    };
    let rc = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &termios) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
static SIGWINCH_RECEIVED: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static READLINE_SIGNAL_RECEIVED: AtomicI32 = AtomicI32::new(0);
#[cfg(unix)]
static RAW_TERMINAL_ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static SIGNAL_HANDLERS_INSTALLED: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static OLD_SIGWINCH_SAVED: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static mut OLD_SIGWINCH_ACTION: std::mem::MaybeUninit<libc::sigaction> =
    std::mem::MaybeUninit::uninit();
#[cfg(unix)]
static mut OLD_SIGNAL_ACTIONS: [std::mem::MaybeUninit<libc::sigaction>; 8] =
    [std::mem::MaybeUninit::uninit(); 8];
#[cfg(unix)]
static OLD_SIGNAL_ACTION_SAVED: [AtomicBool; 8] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

#[cfg(unix)]
extern "C" fn handle_sigwinch_siginfo(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    context: *mut libc::c_void,
) {
    SIGWINCH_RECEIVED.store(true, Ordering::SeqCst);
    unsafe {
        if !OLD_SIGWINCH_SAVED.load(Ordering::SeqCst) {
            return;
        }
        let old_action = std::ptr::addr_of!(OLD_SIGWINCH_ACTION)
            .cast::<libc::sigaction>()
            .read();
        call_saved_signal_action(signal, &old_action, info, context);
    }
}

#[cfg(unix)]
unsafe fn call_saved_signal_action(
    signal: libc::c_int,
    action: &libc::sigaction,
    info: *mut libc::siginfo_t,
    context: *mut libc::c_void,
) {
    let handler = action.sa_sigaction;
    if handler == libc::SIG_DFL || handler == libc::SIG_IGN || handler == 0 {
        return;
    }
    if action.sa_flags & libc::SA_SIGINFO != 0 {
        let callback: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) =
            unsafe { std::mem::transmute(handler) };
        callback(signal, info, context);
    } else {
        let callback: extern "C" fn(libc::c_int) = unsafe { std::mem::transmute(handler) };
        callback(signal);
    }
}

#[cfg(unix)]
extern "C" fn handle_readline_signal(signal: libc::c_int) {
    READLINE_SIGNAL_RECEIVED.store(signal, Ordering::SeqCst);
}

#[cfg(unix)]
const READLINE_SIGNALS: [libc::c_int; 8] = [
    libc::SIGINT,
    libc::SIGQUIT,
    libc::SIGTERM,
    libc::SIGHUP,
    libc::SIGALRM,
    libc::SIGTSTP,
    libc::SIGTTIN,
    libc::SIGTTOU,
];

#[cfg(unix)]
fn install_signal_handlers(catch_signals: bool, catch_sigwinch: bool) {
    if !catch_signals && !catch_sigwinch {
        return;
    }
    // One process-global install state covers both groups. Restoration uses
    // the saved-action flags, not policy that may have changed since entry.
    if SIGNAL_HANDLERS_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    unsafe {
        if catch_sigwinch {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = handle_sigwinch_siginfo as *const () as usize;
            libc::sigemptyset(&mut action.sa_mask);
            action.sa_flags = libc::SA_SIGINFO;
            let mut old_action: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(libc::SIGWINCH, &action, &mut old_action) == 0 {
                if old_action.sa_sigaction == libc::SIG_IGN {
                    // Mirror GNU `rl_maybe_set_sighandler`: never clobber an
                    // embedder-ignored disposition, including SIGWINCH.
                    libc::sigaction(libc::SIGWINCH, &old_action, std::ptr::null_mut());
                } else {
                    std::ptr::addr_of_mut!(OLD_SIGWINCH_ACTION)
                        .write(std::mem::MaybeUninit::new(old_action));
                    OLD_SIGWINCH_SAVED.store(true, Ordering::SeqCst);
                }
            }
        }
        if !catch_signals {
            return;
        }
        for (idx, signal) in READLINE_SIGNALS.iter().copied().enumerate() {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = handle_readline_signal as *const () as usize;
            libc::sigemptyset(&mut action.sa_mask);
            action.sa_flags = 0;
            let mut old_action: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(signal, &action, &mut old_action) == 0 {
                if old_action.sa_sigaction == libc::SIG_IGN {
                    libc::sigaction(signal, &old_action, std::ptr::null_mut());
                } else {
                    OLD_SIGNAL_ACTIONS[idx].write(old_action);
                    OLD_SIGNAL_ACTION_SAVED[idx].store(true, Ordering::SeqCst);
                }
            }
        }
    }
}

#[cfg(unix)]
fn restore_signal_handlers() {
    if !SIGNAL_HANDLERS_INSTALLED.swap(false, Ordering::SeqCst) {
        return;
    }
    unsafe {
        if OLD_SIGWINCH_SAVED.swap(false, Ordering::SeqCst) {
            let old_action = std::ptr::addr_of!(OLD_SIGWINCH_ACTION).cast::<libc::sigaction>();
            libc::sigaction(libc::SIGWINCH, old_action, std::ptr::null_mut());
        }
        for (idx, signal) in READLINE_SIGNALS.iter().copied().enumerate() {
            if OLD_SIGNAL_ACTION_SAVED[idx].swap(false, Ordering::SeqCst) {
                let old_action = OLD_SIGNAL_ACTIONS[idx].as_ptr();
                libc::sigaction(signal, old_action, std::ptr::null_mut());
            }
        }
    }
}

#[cfg(unix)]
fn tty_special_bindings() -> Vec<(u8, &'static str)> {
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    let rc = unsafe { libc::tcgetattr(libc::STDIN_FILENO, termios.as_mut_ptr()) };
    if rc != 0 {
        return Vec::new();
    }
    let termios = unsafe { termios.assume_init() };
    let mut bindings = Vec::new();
    push_tty_binding(
        &mut bindings,
        termios.c_cc[libc::VERASE],
        "backward-delete-char",
    );
    push_tty_binding(
        &mut bindings,
        termios.c_cc[libc::VKILL],
        "unix-line-discard",
    );
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
    push_tty_binding(
        &mut bindings,
        termios.c_cc[libc::VWERASE],
        "unix-word-rubout",
    );
    push_tty_binding(&mut bindings, termios.c_cc[libc::VEOF], "end-of-file");
    push_tty_binding(&mut bindings, termios.c_cc[libc::VINTR], "abort");
    bindings
}

#[cfg(unix)]
fn read_raw_event(timeout: Option<Duration>) -> io::Result<TerminalEvent> {
    if SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst) {
        return current_resize_event();
    }
    if let Some(event) = pending_signal_event() {
        return Ok(event);
    }
    let mut pollfd = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    let timeout_ms = timeout
        .map(|duration| duration.as_millis().min(i32::MAX as u128) as i32)
        .unwrap_or(-1);
    let ready = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
    if ready < 0 {
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted
            && let Some(event) = pending_signal_event()
        {
            return Ok(event);
        }
        return read_error_event(err);
    }
    if ready == 0 {
        return Ok(TerminalEvent::Timeout);
    }
    let mut buf = [0u8; 4];
    let read = unsafe {
        libc::read(
            libc::STDIN_FILENO,
            buf.as_mut_ptr().cast::<libc::c_void>(),
            1,
        )
    };
    if read < 0 {
        return read_error_event(io::Error::last_os_error());
    }
    if read == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "terminal input reached EOF",
        ));
    }
    Ok(TerminalEvent::Bytes(buf[..read as usize].to_vec()))
}

#[cfg(unix)]
fn read_error_event(err: io::Error) -> io::Result<TerminalEvent> {
    if err.kind() != io::ErrorKind::Interrupted {
        return Err(err);
    }
    if SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst) {
        return current_resize_event();
    }
    if let Some(event) = pending_signal_event() {
        return Ok(event);
    }
    // An embedder-owned handler may have interrupted poll/read without
    // setting our flags. Yield so the editor rechecks Hooks::check_signals.
    Ok(TerminalEvent::Timeout)
}

#[cfg(unix)]
fn pending_signal_event() -> Option<TerminalEvent> {
    match READLINE_SIGNAL_RECEIVED.swap(0, Ordering::SeqCst) {
        0 => None,
        signal => Some(TerminalEvent::Signal(signal)),
    }
}

#[cfg(unix)]
fn current_resize_event() -> io::Result<TerminalEvent> {
    query_terminal_size().map(TerminalEvent::Resize)
}

#[cfg(unix)]
fn query_terminal_size() -> io::Result<TerminalSize> {
    let stdout = io::stdout();
    if let Ok(size) = query_terminal_size_fd(stdout.as_raw_fd()) {
        return Ok(size);
    }
    let stdin = io::stdin();
    if let Ok(size) = query_terminal_size_fd(stdin.as_raw_fd()) {
        return Ok(size);
    }
    let tty = std::fs::OpenOptions::new().read(true).open("/dev/tty")?;
    query_terminal_size_fd(tty.as_raw_fd())
}

#[cfg(unix)]
fn query_terminal_size_fd(fd: std::os::fd::RawFd) -> io::Result<TerminalSize> {
    let mut winsize = std::mem::MaybeUninit::<libc::winsize>::zeroed();
    let rc = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, winsize.as_mut_ptr()) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    let winsize = unsafe { winsize.assume_init() };
    Ok(TerminalSize {
        columns: winsize.ws_col,
        rows: winsize.ws_row,
    })
}

#[cfg(not(unix))]
fn query_terminal_size() -> io::Result<TerminalSize> {
    // Err.
    Err(unsupported_terminal_io())
}

#[cfg(not(unix))]
fn unsupported_terminal_io() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "terminal input is not supported on this platform",
    )
}
#[cfg(unix)]
fn push_tty_binding(
    bindings: &mut Vec<(u8, &'static str)>,
    value: libc::cc_t,
    command: &'static str,
) {
    if value != libc::_POSIX_VDISABLE as libc::cc_t {
        bindings.push((value, command));
    }
}

#[cfg(not(unix))]
fn tty_special_bindings() -> Vec<(u8, &'static str)> {
    Vec::new()
}

#[cfg(all(test, unix))]
mod tests {
    use super::{TerminalEvent, prepare_readline_termios, read_error_event};

    #[test]
    fn interrupted_poll_or_read_without_caught_event_yields_timeout() {
        // Both syscall EINTR branches use this fallback. Disposition/event
        // tests run in isolated integration-test children, so this unit test
        // needs neither a real signal nor process-global handler changes.
        assert_eq!(
            read_error_event(std::io::Error::from_raw_os_error(libc::EINTR)).unwrap(),
            TerminalEvent::Timeout
        );
        assert_eq!(
            read_error_event(std::io::Error::from_raw_os_error(libc::EIO))
                .unwrap_err()
                .raw_os_error(),
            Some(libc::EIO),
            "other I/O errors must not become timeouts"
        );
    }

    #[test]
    fn cs8_clears_istrip_and_inpck_while_cs7_preserves() {
        unsafe {
            let mut cs8: libc::termios = std::mem::zeroed();
            cs8.c_cflag = libc::CS8;
            cs8.c_iflag = libc::ISTRIP | libc::INPCK | libc::IXON;
            prepare_readline_termios(&mut cs8);
            assert_eq!(cs8.c_iflag & libc::ISTRIP, 0, "CS8 must clear ISTRIP");
            assert_eq!(cs8.c_iflag & libc::INPCK, 0, "CS8 must clear INPCK");

            let mut cs7: libc::termios = std::mem::zeroed();
            cs7.c_cflag = libc::CS7;
            cs7.c_iflag = libc::ISTRIP | libc::INPCK;
            prepare_readline_termios(&mut cs7);
            assert_ne!(
                cs7.c_iflag & libc::ISTRIP,
                0,
                "CS7 keeps ISTRIP (narrower than GNU meta case)"
            );
        }
    }
}
