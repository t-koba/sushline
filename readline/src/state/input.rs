use super::EditorState;

#[derive(Debug, Default)]
pub(crate) struct InputState {
    pub(crate) quoted_insert: bool,
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
    pub(crate) pending_replace: bool,
    pub(crate) named_command: Option<String>,
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

impl EditorState {
    pub(crate) fn consume_csi_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
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
            if (0x40..=0x7e).contains(byte) {
                self.input.skipping_csi = false;
                self.input.csi_sequence_started = false;
                break;
            }
        }
    }
}
