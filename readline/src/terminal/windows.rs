//! Native console records in, UTF-16 console writes out; no code-page changes.
//! The byte output boundary accepts UTF-8, rejecting invalid bytes explicitly.
use super::{TerminalEvent, TerminalSize};
use std::io;
use std::os::windows::io::{AsRawHandle, BorrowedHandle, OwnedHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CompareObjectHandles, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};

static ACTIVE: AtomicBool = AtomicBool::new(false);
const KEY_EVENT_TYPE: u32 = KEY_EVENT as u32;
const WINDOW_BUFFER_SIZE_EVENT_TYPE: u32 = WINDOW_BUFFER_SIZE_EVENT as u32;

#[derive(Default)]
pub(super) struct Console {
    saved: Option<Session>,
    surrogate: Option<u16>,
    ready: bool,
}

struct Session {
    input: OwnedHandle,
    output: OwnedHandle,
    input_mode: u32,
    output_mode: u32,
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "a Windows console with VT support is required",
    )
}

pub(super) fn stderr() -> HANDLE {
    unsafe { GetStdHandle(STD_ERROR_HANDLE) }
}

pub(super) fn is_console(handle: HANDLE) -> bool {
    let mut mode = 0;
    // GetConsoleMode also rejects null, invalid and redirected handles.
    unsafe { GetConsoleMode(handle, &mut mode) != 0 }
}

fn snapshot(which: STD_HANDLE) -> io::Result<(OwnedHandle, u32)> {
    let handle = unsafe { GetStdHandle(which) };
    let mut mode = 0;
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        return Err(unsupported());
    }
    // Duplicate the validated borrowed handle: restoration must still reach
    // this console even if the embedder replaces its standard handles.
    Ok((
        unsafe { BorrowedHandle::borrow_raw(handle) }.try_clone_to_owned()?,
        mode,
    ))
}

fn set_mode(handle: &OwnedHandle, mode: u32) -> io::Result<()> {
    if unsafe { SetConsoleMode(handle.as_raw_handle(), mode) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

impl Console {
    pub(super) fn active(&self) -> bool {
        self.saved.is_some()
    }

    pub(super) fn enter(&mut self) -> io::Result<()> {
        if self.active() {
            return if self.ready {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "console restoration must be retried",
                ))
            };
        }
        if ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "another terminal holds console modes",
            ));
        }
        let capture = || {
            let (input, input_mode) = snapshot(STD_INPUT_HANDLE)?;
            let (output, output_mode) = snapshot(STD_OUTPUT_HANDLE)?;
            Ok(Session {
                input,
                output,
                input_mode,
                output_mode,
            })
        };
        self.saved = match capture() {
            Ok(saved) => Some(saved),
            Err(error) => {
                ACTIVE.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };
        let saved = self.saved.as_ref().unwrap();
        let input = (saved.input_mode | ENABLE_WINDOW_INPUT | ENABLE_EXTENDED_FLAGS)
            & !(ENABLE_LINE_INPUT
                | ENABLE_ECHO_INPUT
                | ENABLE_PROCESSED_INPUT
                | ENABLE_QUICK_EDIT_MODE
                | ENABLE_MOUSE_INPUT
                | ENABLE_VIRTUAL_TERMINAL_INPUT);
        let output =
            saved.output_mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        if let Err(error) =
            set_mode(&saved.input, input).and_then(|()| set_mode(&saved.output, output))
        {
            // Attempt both restorations, retaining the snapshot and lease if
            // either fails so restore_mode/Drop can retry.
            if let Err(cleanup) = self.restore() {
                return Err(io::Error::other(format!(
                    "console setup: {error}; restoration: {cleanup}"
                )));
            }
            return Err(io::Error::new(io::ErrorKind::Unsupported, error));
        }
        self.ready = true;
        Ok(())
    }

    pub(super) fn restore(&mut self) -> io::Result<()> {
        self.ready = false;
        if let Some(saved) = &self.saved {
            let input = set_mode(&saved.input, saved.input_mode);
            let output = set_mode(&saved.output, saved.output_mode);
            input.and(output)?;
            self.saved = None;
            self.surrogate = None;
            ACTIVE.store(false, Ordering::SeqCst);
        }
        Ok(())
    }

    fn output(&self) -> HANDLE {
        self.saved.as_ref().map_or_else(
            || unsafe { GetStdHandle(STD_OUTPUT_HANDLE) },
            |s| s.output.as_raw_handle(),
        )
    }

    pub(super) fn write(&self, bytes: &[u8]) -> io::Result<()> {
        write(self.output(), bytes)
    }

    pub(super) fn size(&self) -> io::Result<TerminalSize> {
        let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
        if unsafe { GetConsoleScreenBufferInfo(self.output(), &mut info) } == 0 {
            return Err(unsupported());
        }
        // The viewport, not the scrollback buffer, is the editor's screen.
        Ok(TerminalSize {
            columns: (i32::from(info.srWindow.Right) - i32::from(info.srWindow.Left) + 1) as u16,
            rows: (i32::from(info.srWindow.Bottom) - i32::from(info.srWindow.Top) + 1) as u16,
        })
    }

    pub(super) fn read_event(&mut self, timeout: Option<Duration>) -> io::Result<TerminalEvent> {
        if !self.ready {
            return Err(unsupported());
        }
        let input = self
            .saved
            .as_ref()
            .ok_or_else(unsupported)?
            .input
            .as_raw_handle();
        let start = Instant::now();
        loop {
            // Win32 reserves MAXDWORD for INFINITE. Round finite waits up to
            // milliseconds and split larger durations without truncation.
            let wait = timeout.map_or(INFINITE, |d| {
                d.saturating_sub(start.elapsed())
                    .as_nanos()
                    .div_ceil(1_000_000)
                    .min(u128::from(INFINITE - 1)) as u32
            });
            match unsafe { WaitForSingleObject(input, wait) } {
                WAIT_TIMEOUT if timeout.is_some_and(|d| start.elapsed() >= d) => {
                    return Ok(TerminalEvent::Timeout);
                }
                WAIT_TIMEOUT => continue,
                WAIT_OBJECT_0 => {}
                _ => return Err(io::Error::last_os_error()),
            }
            let mut record = INPUT_RECORD::default();
            let mut read = 0;
            if unsafe { ReadConsoleInputW(input, &mut record, 1, &mut read) } == 0 {
                return Err(io::Error::last_os_error());
            }
            // The discriminant determines which union member is initialized.
            match u32::from(record.EventType) {
                KEY_EVENT_TYPE => {
                    let bytes = self.key(unsafe { record.Event.KeyEvent });
                    if !bytes.is_empty() {
                        return Ok(TerminalEvent::Bytes(bytes));
                    }
                }
                WINDOW_BUFFER_SIZE_EVENT_TYPE => return self.size().map(TerminalEvent::Resize),
                _ => {} // Key-up, mouse, menu and focus records are not editor input.
            }
            if timeout.is_some_and(|d| start.elapsed() >= d) {
                return Ok(TerminalEvent::Timeout);
            }
        }
    }

    fn key(&mut self, key: KEY_EVENT_RECORD) -> Vec<u8> {
        if key.bKeyDown == 0 || key.wRepeatCount == 0 {
            return Vec::new();
        }
        let unit = unsafe { key.uChar.UnicodeChar };
        let flags = key.dwControlKeyState;
        let ctrl = flags & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0;
        // AltGr produces text, not a Readline meta prefix.
        let alt = flags & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0
            && !(flags & RIGHT_ALT_PRESSED != 0 && ctrl && unit >= 32);
        let shift = flags & SHIFT_PRESSED != 0;
        let mut text = String::new();
        if let Some(high) = self.surrogate.take() {
            if (0xdc00..=0xdfff).contains(&unit) {
                if alt {
                    text.push('\x1b');
                }
                text.push(
                    char::from_u32(
                        0x10000 + ((u32::from(high) - 0xd800) << 10) + u32::from(unit) - 0xdc00,
                    )
                    .unwrap(),
                );
            } else {
                text.push(char::REPLACEMENT_CHARACTER);
            }
        }
        if (0xd800..=0xdbff).contains(&unit) {
            self.surrogate = Some(unit);
        } else if !(0xdc00..=0xdfff).contains(&unit) || text.is_empty() {
            // Virtual-key values below are intrinsic Win32 protocol constants.
            let special = match key.wVirtualKeyCode {
                0x25 => Some(("D", false)),
                0x26 => Some(("A", false)),
                0x27 => Some(("C", false)),
                0x28 => Some(("B", false)),
                0x24 => Some(("H", false)),
                0x23 => Some(("F", false)),
                0x2d => Some(("2", true)),
                0x2e => Some(("3", true)),
                0x21 => Some(("5", true)),
                0x22 => Some(("6", true)),
                0x70 => Some(("11", true)),
                0x71 => Some(("12", true)),
                0x72 => Some(("13", true)),
                0x73 => Some(("14", true)),
                0x74 => Some(("15", true)),
                0x75 => Some(("17", true)),
                0x76 => Some(("18", true)),
                0x77 => Some(("19", true)),
                0x78 => Some(("20", true)),
                0x79 => Some(("21", true)),
                0x7a => Some(("23", true)),
                0x7b => Some(("24", true)),
                _ => None,
            };
            if let Some((code, tilde)) = special {
                let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
                text.push_str("\x1b[");
                if modifier != 1 {
                    text.push_str(&format!("{};{modifier}", if tilde { code } else { "1" }));
                    if !tilde {
                        text.push_str(code);
                    }
                } else {
                    text.push_str(code);
                }
                if tilde {
                    text.push('~');
                }
            } else if key.wVirtualKeyCode == 0x09 && shift {
                text.push_str("\x1b[Z");
            } else if unit != 0 || (ctrl && key.wVirtualKeyCode == 0x20) {
                if alt {
                    text.push('\x1b');
                }
                text.push(char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER));
            }
        }
        text.repeat(usize::from(key.wRepeatCount)).into_bytes()
    }
}

pub(super) fn write(handle: HANDLE, bytes: &[u8]) -> io::Result<()> {
    if !is_console(handle) {
        return Err(unsupported());
    }
    let text =
        std::str::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut rest = wide.as_slice();
    while !rest.is_empty() {
        let mut written = 0;
        // The API's character count is a DWORD, not usize.
        let count = rest.len().min(u32::MAX as usize) as u32;
        if unsafe { WriteConsoleW(handle, rest.as_ptr(), count, &mut written, std::ptr::null()) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        if written == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        rest = &rest[written as usize..];
    }
    Ok(())
}

pub(super) fn diagnostics_use_terminal() -> bool {
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let error = stderr();
    is_console(output)
        && is_console(error)
        && (output == error || unsafe { CompareObjectHandles(output, error) != 0 })
}

#[cfg(test)]
mod tests;
