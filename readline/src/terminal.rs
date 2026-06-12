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
pub(crate) use terminfo::{active_region_default_sequence_bytes, active_region_default_sequences};
use terminfo::{terminfo_keypad_sequence, terminfo_meta_sequence, terminfo_sequence};

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
    /// Enables or disables terminal meta-key handling.
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

/// Terminal.
pub struct Terminal {
    stdout: Stdout,
    raw: bool,
    #[cfg(unix)]
    saved_termios: Option<libc::termios>,
}

impl Terminal {
    /// New.
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            raw: false,
            #[cfg(unix)]
            saved_termios: None,
        }
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
            install_signal_handlers();
            enable_readline_mode(&mut self.saved_termios)?;
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

    fn set_meta_key_enabled(&mut self, enabled: bool) -> io::Result<()> {
        if let Some(sequence) = terminfo_meta_sequence(enabled) {
            self.stdout.write_all(&sequence)
        } else if std::env::var("TERM").is_ok_and(|term| term.starts_with("xterm")) {
            if enabled {
                self.stdout.write_all(escape::XTERM_META_ON.as_bytes())
            } else {
                self.stdout.write_all(escape::XTERM_META_OFF.as_bytes())
            }
        } else {
            Ok(())
        }
    }

    fn set_application_keypad_enabled(&mut self, enabled: bool) -> io::Result<()> {
        if let Some(sequence) = terminfo_keypad_sequence(enabled) {
            self.stdout.write_all(&sequence)
        } else if enabled {
            self.stdout
                .write_all(escape::APPLICATION_KEYPAD_ON.as_bytes())
        } else {
            self.stdout
                .write_all(escape::APPLICATION_KEYPAD_OFF.as_bytes())
        }
    }

    fn tty_special_bindings(&self) -> Vec<(u8, &'static str)> {
        tty_special_bindings()
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
    readline.c_iflag &= !(libc::IXON | libc::IXOFF);
    readline.c_lflag &= !(libc::ICANON | libc::ECHO | libc::IEXTEN);
    readline.c_cc[libc::VMIN] = 1;
    readline.c_cc[libc::VTIME] = 0;

    let rc = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &readline) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    // Ok.
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
    // Ok.
    Ok(())
}

#[cfg(unix)]
static SIGWINCH_RECEIVED: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static READLINE_SIGNAL_RECEIVED: AtomicI32 = AtomicI32::new(0);
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
fn install_signal_handlers() {
    if SIGNAL_HANDLERS_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = handle_sigwinch_siginfo as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = libc::SA_SIGINFO;
        let mut old_action: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(libc::SIGWINCH, &action, &mut old_action) == 0 {
            std::ptr::addr_of_mut!(OLD_SIGWINCH_ACTION)
                .write(std::mem::MaybeUninit::new(old_action));
            OLD_SIGWINCH_SAVED.store(true, Ordering::SeqCst);
        }
        for (idx, signal) in READLINE_SIGNALS.iter().copied().enumerate() {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = handle_readline_signal as *const () as usize;
            libc::sigemptyset(&mut action.sa_mask);
            action.sa_flags = 0;
            let mut old_action: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(signal, &action, &mut old_action) == 0 {
                OLD_SIGNAL_ACTIONS[idx].write(old_action);
                OLD_SIGNAL_ACTION_SAVED[idx].store(true, Ordering::SeqCst);
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
        if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted
            && SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst)
        {
            return current_resize_event();
        }
        if let Some(event) = pending_signal_event() {
            return Ok(event);
        }
        return Err(io::Error::last_os_error());
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
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::Interrupted {
            if SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst) {
                return current_resize_event();
            }
            if let Some(event) = pending_signal_event() {
                return Ok(event);
            }
        }
        return Err(err);
    }
    if read == 0 {
        return Ok(TerminalEvent::Timeout);
    }
    // Ok.
    Ok(TerminalEvent::Bytes(buf[..read as usize].to_vec()))
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
    // Ok.
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
