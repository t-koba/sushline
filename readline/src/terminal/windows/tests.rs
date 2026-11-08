use super::*;
use crate::{Terminal, TerminalIo};
use std::fs::{File, OpenOptions};

fn key(unit: u16, code: u16, flags: u32, repeat: u16) -> KEY_EVENT_RECORD {
    KEY_EVENT_RECORD {
        bKeyDown: 1,
        wRepeatCount: repeat,
        wVirtualKeyCode: code,
        uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
        dwControlKeyState: flags,
        ..Default::default()
    }
}

#[test]
fn native_key_encoding() {
    let mut console = Console::default();
    for (unit, code, flags, repeat, expected) in [
        (0x00e9, 0, 0, 2, "éé"),
        (3, 0x43, LEFT_CTRL_PRESSED, 1, "\x03"),
        (0, 0x20, LEFT_CTRL_PRESSED, 1, "\0"),
        (97, 0x41, LEFT_ALT_PRESSED, 1, "\x1ba"),
        (0x20ac, 0x45, RIGHT_ALT_PRESSED | LEFT_CTRL_PRESSED, 1, "€"),
        (0, 0x25, 0, 1, "\x1b[D"),
        (0, 0x27, LEFT_CTRL_PRESSED, 1, "\x1b[1;5C"),
        (0, 0x2e, 0, 1, "\x1b[3~"),
        (9, 0x09, SHIFT_PRESSED, 1, "\x1b[Z"),
        (0, 0x7b, 0, 1, "\x1b[24~"),
        (0, 0x10, SHIFT_PRESSED, 1, ""),
    ] {
        assert_eq!(
            console.key(key(unit, code, flags, repeat)),
            expected.as_bytes()
        );
    }
    assert!(
        console
            .key(KEY_EVENT_RECORD {
                bKeyDown: 0,
                ..key(97, 0, 0, 1)
            })
            .is_empty()
    );
    assert!(console.key(key(0xd83d, 0, 0, 1)).is_empty());
    assert_eq!(console.key(key(0xde80, 0, 0, 1)), "🚀".as_bytes());
    assert!(console.key(key(0xd83d, 0, 0, 1)).is_empty());
    assert_eq!(console.key(key(97, 0, 0, 1)), "�a".as_bytes());
    assert_eq!(console.key(key(0xdc00, 0, 0, 1)), "�".as_bytes());
}

// This test deliberately creates a console even on a headless CI runner.
// Never skip: a Windows job without usable console APIs is not evidence.
struct TestConsole {
    old: [HANDLE; 3],
    input: File,
    output: File,
}

impl TestConsole {
    fn new() -> Self {
        let old = unsafe {
            [
                GetStdHandle(STD_INPUT_HANDLE),
                GetStdHandle(STD_OUTPUT_HANDLE),
                GetStdHandle(STD_ERROR_HANDLE),
            ]
        };
        unsafe {
            FreeConsole();
        }
        assert_ne!(
            unsafe { AllocConsole() },
            0,
            "{}",
            io::Error::last_os_error()
        );
        let open = |name| {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(name)
                .unwrap()
        };
        let console = Self {
            old,
            input: open("CONIN$"),
            output: open("CONOUT$"),
        };
        console.handles();
        console
    }
    fn handles(&self) {
        for (which, handle) in [
            (STD_INPUT_HANDLE, self.input.as_raw_handle()),
            (STD_OUTPUT_HANDLE, self.output.as_raw_handle()),
            (STD_ERROR_HANDLE, self.output.as_raw_handle()),
        ] {
            assert_ne!(unsafe { SetStdHandle(which, handle) }, 0);
        }
    }
    fn mode(&self, input: bool) -> u32 {
        let mut mode = 0;
        let handle = if input { &self.input } else { &self.output };
        assert_ne!(
            unsafe { GetConsoleMode(handle.as_raw_handle(), &mut mode) },
            0
        );
        mode
    }
    fn send(&self, records: &[INPUT_RECORD]) {
        let mut written = 0;
        assert_ne!(
            unsafe {
                WriteConsoleInputW(
                    self.input.as_raw_handle(),
                    records.as_ptr(),
                    records.len() as u32,
                    &mut written,
                )
            },
            0
        );
        assert_eq!(written as usize, records.len());
    }
}

impl Drop for TestConsole {
    fn drop(&mut self) {
        for (which, handle) in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
            .into_iter()
            .zip(self.old)
        {
            unsafe {
                SetStdHandle(which, handle);
            }
        }
        unsafe {
            FreeConsole();
        }
    }
}

#[test]
fn native_console_lifecycle_events_and_output() {
    let console = TestConsole::new();
    let original = (console.mode(true), console.mode(false));
    let codepages = unsafe { (GetConsoleCP(), GetConsoleOutputCP()) };
    let mut terminal = Terminal::new();
    terminal.enter_raw_mode().unwrap();
    terminal.enter_raw_mode().unwrap();
    assert_eq!(
        console.mode(true)
            & (ENABLE_LINE_INPUT
                | ENABLE_ECHO_INPUT
                | ENABLE_PROCESSED_INPUT
                | ENABLE_QUICK_EDIT_MODE),
        0
    );
    assert_ne!(console.mode(true) & ENABLE_WINDOW_INPUT, 0);
    assert_ne!(console.mode(false) & ENABLE_VIRTUAL_TERMINAL_PROCESSING, 0);
    assert_eq!(
        Terminal::new().enter_raw_mode().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_ne!(
        unsafe { FlushConsoleInputBuffer(console.input.as_raw_handle()) },
        0
    );
    assert_eq!(
        terminal.read_event(Some(Duration::ZERO)).unwrap(),
        TerminalEvent::Timeout
    );
    console.send(&[
        INPUT_RECORD {
            EventType: FOCUS_EVENT as u16,
            ..Default::default()
        },
        INPUT_RECORD {
            EventType: KEY_EVENT as u16,
            Event: INPUT_RECORD_0 {
                KeyEvent: key(0x00e9, 0, 0, 2),
            },
        },
        INPUT_RECORD {
            EventType: KEY_EVENT as u16,
            Event: INPUT_RECORD_0 {
                KeyEvent: key(3, 0x43, LEFT_CTRL_PRESSED, 1),
            },
        },
    ]);
    let timeout = Some(Duration::from_secs(1));
    assert_eq!(
        terminal.read_event(timeout).unwrap(),
        TerminalEvent::Bytes("éé".as_bytes().to_vec())
    );
    assert_eq!(
        terminal.read_event(timeout).unwrap(),
        TerminalEvent::Bytes(vec![3])
    );
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    assert_ne!(
        unsafe { GetConsoleScreenBufferInfo(console.output.as_raw_handle(), &mut info) },
        0
    );
    let mut window = info.srWindow;
    window.Right -= 1;
    assert_ne!(
        unsafe { SetConsoleWindowInfo(console.output.as_raw_handle(), 1, &window) },
        0
    );
    let buffer = COORD {
        X: info.dwSize.X - 1,
        Y: info.dwSize.Y,
    };
    assert_ne!(
        unsafe { SetConsoleScreenBufferSize(console.output.as_raw_handle(), buffer) },
        0
    );
    let size = terminal.size().unwrap();
    assert_eq!(size.columns, (window.Right - window.Left + 1) as u16);
    assert_eq!(size.rows, (window.Bottom - window.Top + 1) as u16);
    assert_eq!(
        terminal.read_event(timeout).unwrap(),
        TerminalEvent::Resize(size)
    );
    terminal.write("\x1b[Hé🚀").unwrap();
    terminal.flush().unwrap();
    let mut chars = [0u16; 3];
    let mut read = 0;
    assert_ne!(
        unsafe {
            ReadConsoleOutputCharacterW(
                console.output.as_raw_handle(),
                chars.as_mut_ptr(),
                3,
                COORD { X: 0, Y: 0 },
                &mut read,
            )
        },
        0
    );
    assert_eq!(read, 3);
    assert_eq!(chars, [0x00e9, 0xd83d, 0xde80]);
    assert_eq!(
        terminal.write_bytes(&[0xff]).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(terminal.diagnostics_use_terminal());
    terminal.write_diagnostic("diagnostic\r\n").unwrap();
    terminal.restore_mode().unwrap();
    terminal.restore_mode().unwrap();
    assert_eq!((console.mode(true), console.mode(false)), original);
    terminal.enter_raw_mode().unwrap();
    drop(terminal);
    assert_eq!((console.mode(true), console.mode(false)), original);
    assert_eq!(unsafe { (GetConsoleCP(), GetConsoleOutputCP()) }, codepages);

    // A restore error retains ownership for retry, attempts the other mode,
    // and prevents an incomplete session from being reused as raw mode.
    let mut terminal = Terminal::new();
    terminal.enter_raw_mode().unwrap();
    let nonconsole: OwnedHandle = tempfile::tempfile().unwrap().into();
    let saved_input = std::mem::replace(
        &mut terminal.console.saved.as_mut().unwrap().input,
        nonconsole,
    );
    assert!(terminal.restore_mode().is_err());
    assert_eq!(console.mode(false), original.1);
    assert_eq!(
        terminal.enter_raw_mode().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    terminal.console.saved.as_mut().unwrap().input = saved_input;
    terminal.restore_mode().unwrap();
    assert_eq!((console.mode(true), console.mode(false)), original);

    // Failed entry must not change either mode or retain the raw-mode lease.
    let redirected = tempfile::tempfile().unwrap();
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE] {
        assert_ne!(
            unsafe { SetStdHandle(which, redirected.as_raw_handle()) },
            0
        );
        assert_eq!(
            Terminal::new().enter_raw_mode().unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        assert_eq!((console.mode(true), console.mode(false)), original);
        console.handles();
    }
    assert_ne!(
        unsafe { SetStdHandle(STD_ERROR_HANDLE, redirected.as_raw_handle()) },
        0
    );
    let mut terminal = Terminal::new();
    assert!(!terminal.diagnostics_use_terminal());
    terminal.write_diagnostic("redirected diagnostic").unwrap();
    console.handles();
    terminal.enter_raw_mode().unwrap();
    // Duplicated handles keep cleanup directed to the original console.
    assert_ne!(
        unsafe { SetStdHandle(STD_INPUT_HANDLE, redirected.as_raw_handle()) },
        0
    );
    terminal.restore_mode().unwrap();
    assert_eq!((console.mode(true), console.mode(false)), original);
}

#[test]
fn native_console_editor_accepts_utf8_with_utf8_defaults() {
    let console = TestConsole::new();
    let config = || crate::Config {
        inputrc_path: crate::InputrcPath::Disabled,
        ..crate::Config::default()
    };
    // The backend exchanges UTF-8 at the byte boundary, so editor defaults agree.
    let probe = crate::Editor::new(config(), Terminal::new(), crate::History::new());
    assert!(probe.variables().utf8_locale());
    assert!(probe.variables().is_on("input-meta"));
    assert!(probe.variables().is_on("output-meta"));
    assert!(!probe.variables().is_on("convert-meta"));

    let send_line = |console: &TestConsole| {
        console.send(&[
            INPUT_RECORD {
                EventType: KEY_EVENT as u16,
                Event: INPUT_RECORD_0 {
                    KeyEvent: key(0x00e9, 0, 0, 1),
                },
            },
            INPUT_RECORD {
                EventType: KEY_EVENT as u16,
                Event: INPUT_RECORD_0 {
                    KeyEvent: key(0x0d, 0x0d, 0, 1),
                },
            },
        ]);
    };

    send_line(&console);
    let mut editor = crate::Editor::new(config(), Terminal::new(), crate::History::new());
    assert_eq!(
        editor.read_line(crate::Prompt::new("> "), &mut ()).unwrap(),
        crate::ReadlineResult::Line("é".as_bytes().to_vec())
    );

    // An explicit inputrc override stays effective on the native console.
    send_line(&console);
    let mut overridden = crate::Editor::new(config(), Terminal::new(), crate::History::new());
    overridden.load_inputrc_str("set convert-meta on").unwrap();
    assert!(overridden.variables().is_on("convert-meta"));
    assert_ne!(
        overridden
            .read_line(crate::Prompt::new("> "), &mut ())
            .unwrap(),
        crate::ReadlineResult::Line("é".as_bytes().to_vec())
    );
}
