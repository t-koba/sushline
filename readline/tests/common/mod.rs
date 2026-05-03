use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use readline::{TerminalEvent, TerminalIo, TerminalSize};

#[derive(Default)]
pub struct MemoryTerminal {
    events: VecDeque<TerminalEvent>,
    pub out: String,
    pub columns: u16,
    pub tty_special: Vec<(u8, &'static str)>,
    pub meta_enabled: Vec<bool>,
    pub keypad_enabled: Vec<bool>,
    pub moved_columns: Vec<u16>,
    pub moved_up: Vec<u16>,
    pub cleared_screen: usize,
}

impl MemoryTerminal {
    pub fn with_events(events: Vec<TerminalEvent>) -> Self {
        Self {
            events: events.into(),
            out: String::new(),
            columns: 80,
            tty_special: Vec::new(),
            meta_enabled: Vec::new(),
            keypad_enabled: Vec::new(),
            moved_columns: Vec::new(),
            moved_up: Vec::new(),
            cleared_screen: 0,
        }
    }
}

impl TerminalIo for MemoryTerminal {
    fn enter_raw_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn restore_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn read_event(&mut self, _: Option<Duration>) -> io::Result<TerminalEvent> {
        Ok(self.events.pop_front().unwrap_or(TerminalEvent::Timeout))
    }

    fn write(&mut self, text: &str) -> io::Result<()> {
        self.out.push_str(text);
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.push_str(&String::from_utf8_lossy(bytes));
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn size(&self) -> io::Result<TerminalSize> {
        Ok(TerminalSize {
            columns: self.columns,
            rows: 24,
        })
    }

    fn clear_after_cursor(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn clear_to_screen_end(&mut self) -> io::Result<()> {
        self.cleared_screen += 1;
        Ok(())
    }

    fn clear_display(&mut self) -> io::Result<()> {
        self.cleared_screen += 1;
        self.out.push_str("\r\x1b[J");
        Ok(())
    }

    fn move_to_column(&mut self, column: u16) -> io::Result<()> {
        self.moved_columns.push(column);
        Ok(())
    }

    fn set_meta_key_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.meta_enabled.push(enabled);
        Ok(())
    }

    fn set_application_keypad_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.keypad_enabled.push(enabled);
        Ok(())
    }

    fn move_up(&mut self, rows: u16) -> io::Result<()> {
        self.moved_up.push(rows);
        Ok(())
    }

    fn tty_special_bindings(&self) -> Vec<(u8, &'static str)> {
        self.tty_special.clone()
    }
}
