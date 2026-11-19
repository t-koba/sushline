use super::EditorState;

#[derive(Debug, Default)]
pub(crate) struct InputState {
    pub(crate) quoted_insert: bool,
    /// Remaining raw bytes for a negative-count quoted insert.
    pub(crate) quoted_insert_remaining: u32,
    pub(crate) interrupted: bool,
    pub(crate) pending_key: Vec<u8>,
    /// The bytes of a character a pending command has not taken yet.
    ///
    /// A command that takes a character waits for the whole character, so
    /// the bytes of an incomplete one are held here until the rest of it
    /// arrives.
    pub(crate) pending_char_bytes: Vec<u8>,
    pub(crate) skipping_csi: bool,
    pub(crate) csi_sequence_started: bool,
    /// The one-byte `[`/`O` introducer that started a skip, if any.
    pub(crate) csi_bare_introducer: Option<u8>,
    /// A fresh ESC inside that skip is waiting for its matching introducer.
    pub(crate) csi_introducer_pending: bool,
    pub(crate) arrow_key_prefix: bool,
    pub(crate) pending_replace: bool,
    pub(crate) named_command: Option<PendingNamedCommand>,
    pub(crate) prefix_meta: bool,
    /// Whether the last dispatched key ran a command.
    ///
    /// GNU Readline defers the redisplay of typed text until the input
    /// batch ends, and redraws after every other key. The editor redraws
    /// the same way, so a pasted line is drawn once.
    pub(crate) applied_command: bool,
    /// Whether the last dispatched key changed the display.
    ///
    /// A key that changes the display without running a command, like
    /// the query of an incremental search, redraws as well: GNU Readline
    /// redraws the search display for every query key.
    pub(crate) changed_display: bool,
    // Byte length of the synthetic Meta sequence retained in the pending key.
    pub(crate) prefix_meta_command_len: usize,
}

#[derive(Debug, Default)]
pub(crate) struct PendingNamedCommand {
    pub(crate) query: String,
    pub(crate) invoking_key: Option<u8>,
}

impl EditorState {
    /// Consumes skip-csi-sequence bytes, returning how many were taken.
    ///
    /// A byte after a fresh ESC that does not match the bare introducer is
    /// left for normal dispatch instead of being swallowed by the skip.
    pub(crate) fn consume_csi_bytes(&mut self, bytes: &[u8]) -> usize {
        for (index, byte) in bytes.iter().enumerate() {
            if self.input.csi_introducer_pending {
                self.input.csi_introducer_pending = false;
                if Some(*byte) == self.input.csi_bare_introducer {
                    self.input.csi_sequence_started = true;
                    continue;
                }
                self.input.skipping_csi = false;
                self.input.csi_sequence_started = false;
                self.input.csi_bare_introducer = None;
                return index;
            }
            if !self.input.csi_sequence_started {
                if *byte == 0x1b {
                    continue;
                }
                if matches!(*byte, b'[' | b'O') {
                    self.input.csi_sequence_started = true;
                    continue;
                }
                self.input.skipping_csi = false;
                break;
            }
            if *byte == 0x1b {
                if self.input.csi_bare_introducer.is_some() {
                    self.input.csi_introducer_pending = true;
                } else {
                    // An explicit binding has already consumed its
                    // introducer; a fresh ESC ends the skip and the next
                    // byte is ordinary input.
                    self.input.skipping_csi = false;
                    self.input.csi_sequence_started = false;
                    return index + 1;
                }
                continue;
            }
            if (0x40..=0x7e).contains(byte) {
                self.input.skipping_csi = false;
                self.input.csi_sequence_started = false;
                self.input.csi_bare_introducer = None;
                break;
            }
        }
        bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::Prompt;

    fn bare_bracket_skip() -> EditorState {
        let mut state = EditorState::new(Prompt::new(""), None);
        state.input.skipping_csi = true;
        state.input.csi_sequence_started = true;
        state.input.csi_bare_introducer = Some(b'[');
        state
    }

    #[test]
    fn nested_introducer_pending_survives_a_split_read() {
        let mut state = bare_bracket_skip();
        assert_eq!(state.consume_csi_bytes(b"\x1b"), 1);
        assert!(state.input.csi_introducer_pending);
        assert_eq!(state.consume_csi_bytes(b"[A"), 2);
        assert!(!state.input.skipping_csi);
        assert!(!state.input.csi_introducer_pending);
    }

    #[test]
    fn a_mismatched_introducer_is_left_for_dispatch() {
        let mut state = bare_bracket_skip();
        assert_eq!(state.consume_csi_bytes(b"\x1bO"), 1);
        assert!(!state.input.skipping_csi);
        assert!(!state.input.csi_sequence_started);
        assert_eq!(state.input.csi_bare_introducer, None);
    }
}
