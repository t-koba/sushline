use std::io;
use std::time::Duration;

use readline::{
    Config, Editor, History, Prompt, ReadlineResult, TerminalEvent, TerminalIo, TerminalSize,
};

struct FiniteInput {
    bytes: Option<Vec<u8>>,
}

impl TerminalIo for FiniteInput {
    fn enter_raw_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn restore_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn read_event(&mut self, _: Option<Duration>) -> io::Result<TerminalEvent> {
        if let Some(bytes) = self.bytes.take()
            && !bytes.is_empty()
        {
            return Ok(TerminalEvent::Bytes(bytes));
        }
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "finite input exhausted",
        ))
    }

    fn write(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }

    fn write_bytes(&mut self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn size(&self) -> io::Result<TerminalSize> {
        Ok(TerminalSize {
            columns: 80,
            rows: 24,
        })
    }

    fn clear_after_cursor(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn move_to_column(&mut self, _: u16) -> io::Result<()> {
        Ok(())
    }
}

fn read_two_lines(bytes: &[u8]) -> (ReadlineResult, ReadlineResult) {
    let terminal = FiniteInput {
        bytes: Some(bytes.to_vec()),
    };
    let mut editor = Editor::new(Config::default(), terminal, History::new());
    let first = editor.read_line(Prompt::new("probe> "), &mut ()).unwrap();
    let second = editor.read_line(Prompt::new("probe> "), &mut ()).unwrap();
    (first, second)
}

#[test]
fn unexpected_eof_accepts_partial_input_and_preserves_empty_eof() {
    // Pinned GNU Readline 8.3 primary and fresh-replica fixtures both return
    // `abc` at stream EOF. CR represents the terminal's Enter event for the
    // newline-terminated control.
    assert_eq!(
        read_two_lines(b""),
        (ReadlineResult::Eof, ReadlineResult::Eof)
    );
    assert_eq!(
        read_two_lines(b"abc"),
        (ReadlineResult::Line(b"abc".to_vec()), ReadlineResult::Eof)
    );
    assert_eq!(
        read_two_lines(b"abc\r"),
        (ReadlineResult::Line(b"abc".to_vec()), ReadlineResult::Eof)
    );
}
