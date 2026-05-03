use crate::completion::CompletionType;
use crate::completion::display::common_prefix_bytes;
use crate::editor::{Editor, ReadlineError};
use crate::hooks::Hooks;
use crate::state::EditorState;
use crate::terminal::TerminalIo;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn export_completions(
        &mut self,
        state: &mut EditorState,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<(), ReadlineError> {
        let edit = self.completion_edit(state, hooks);
        let response = self.completion_response(state, key, CompletionType::Complete, &edit, hooks);
        let mut matches = Vec::new();
        if response.candidates.len() > 1
            && let Some(prefix) = common_prefix_bytes(&response.candidates)
        {
            matches.push(prefix);
        }
        matches.extend(
            response
                .candidates
                .iter()
                .map(|candidate| candidate.replacement_bytes().to_vec()),
        );
        self.terminal.write_bytes(b"\r\n")?;
        self.terminal
            .write_bytes(format!("{}\n", matches.len()).as_bytes())?;
        self.terminal.write_bytes(&edit.word_bytes)?;
        self.terminal.write_bytes(b"\n")?;
        let start = state.buffer.byte_index_for_char_index(edit.start);
        let end = state.buffer.byte_index_for_char_index(edit.end);
        self.terminal
            .write_bytes(format!("{start}:{end}\n").as_bytes())?;
        for candidate in matches {
            self.terminal.write_bytes(&candidate)?;
            self.terminal.write_bytes(b"\n")?;
        }
        state.completion.last_completion = Some(response);
        Ok(())
    }
}
