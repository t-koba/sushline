use crate::completion::display::common_prefix_bytes;
use crate::completion::filename::{DirectoryCompletion, filename_directory_completion};
use crate::completion::quoting::*;
use crate::completion::{
    CompletionAction, CompletionCandidate, CompletionOptions, CompletionResponse, CompletionType,
};
use crate::editor::{Editor, ReadlineError};
use crate::hooks::{Hooks, QuoteContext};
use crate::state::{CompletionAttemptState, EditorState};
use crate::terminal::TerminalIo;
use crate::variables::BoolVariable;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn insert_completion_response(
        &mut self,
        state: &mut EditorState,
        response: CompletionResponse,
        edit: &CompletionEdit,
        completion_type: CompletionType,
        hooks: &mut impl Hooks,
    ) -> Result<(), ReadlineError> {
        if response.candidates.is_empty() {
            self.ding()?;
            return Ok(());
        }
        if response.options.action == Some(CompletionAction::DisplayOnly) {
            self.display_completions_for_word(state, &response, &edit.word_bytes)?;
            return Ok(());
        }
        let skip_completed_text = self.variable_is_on("skip-completed-text");
        if response.candidates.len() == 1 {
            let candidate = &response.candidates[0];
            let suffix = completion_suffix_bytes(edit, state);
            let (mut replacement_bytes, filename_directory) = self
                .completion_replacement_with_directory(
                    &response,
                    edit,
                    candidate,
                    completion_type,
                    hooks,
                    suffix.first().copied(),
                );
            let skipped_completed_text = skip_completed_text && !suffix.is_empty();
            if skip_completed_text {
                replacement_bytes = skip_completed_suffix_bytes(&replacement_bytes, &suffix);
            }
            if !skipped_completed_text {
                extend_replacement_with_append_char(
                    &mut replacement_bytes,
                    &response.options,
                    candidate,
                    filename_directory.as_ref(),
                );
            }
            state
                .buffer
                .replace_range_bytes(edit.start, edit.end, &replacement_bytes);
        } else {
            let before_line = state.buffer.as_bytes().to_vec();
            let before_point = state.buffer.byte_point();
            let repeated_unmodified_completion = state
                .completion
                .last_attempt
                .as_ref()
                .is_some_and(|attempt| {
                    completion_type == CompletionType::Complete
                        && attempt.completion_type == completion_type
                        && attempt.unmodified
                        && attempt.point == before_point
                        && attempt.line == before_line
                });
            if let Some(prefix_bytes) = common_prefix_bytes(&response.candidates) {
                let mut replacement_bytes = self.requote_completion_bytes(
                    &prefix_bytes,
                    edit,
                    completion_type,
                    response.options.quote_filename(),
                    hooks,
                );
                if skip_completed_text {
                    let suffix = completion_suffix_bytes(edit, state);
                    replacement_bytes = skip_completed_suffix_bytes(&replacement_bytes, &suffix);
                }
                state
                    .buffer
                    .replace_range_bytes(edit.start, edit.end, &replacement_bytes);
            }
            let unmodified_after_prefix = state.buffer.as_bytes() == before_line.as_slice();
            if matches!(
                completion_type,
                CompletionType::Complete
                    | CompletionType::Command
                    | CompletionType::Filename
                    | CompletionType::Hostname
                    | CompletionType::Username
                    | CompletionType::Variable
            ) {
                self.ding()?;
            }
            if self.flag(BoolVariable::ShowAllIfAmbiguous)
                || (self.flag(BoolVariable::ShowAllIfUnmodified) && unmodified_after_prefix)
                || (repeated_unmodified_completion && unmodified_after_prefix)
            {
                self.display_completions_for_word(state, &response, &edit.word_bytes)?;
            }
            state.completion.last_attempt = Some(CompletionAttemptState {
                completion_type,
                line: state.buffer.as_bytes().to_vec(),
                point: state.buffer.byte_point(),
                unmodified: state.buffer.as_bytes() == before_line.as_slice(),
            });
            state.completion.last_completion = Some(response);
        }
        Ok(())
    }

    pub(crate) fn complete_into_braces(
        &mut self,
        state: &mut EditorState,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<(), ReadlineError> {
        let edit = self.completion_edit(state, hooks);
        let response = self.completion_response(state, key, CompletionType::Complete, &edit, hooks);
        if response.candidates.is_empty() {
            self.ding()?;
            return Ok(());
        }
        if response.candidates.len() == 1 {
            return self.insert_completion_response(
                state,
                response,
                &edit,
                CompletionType::Complete,
                hooks,
            );
        }

        let prefix = common_prefix_bytes(&response.candidates).unwrap_or_default();
        let quote_filename = response.options.quote_filename();
        let mut joined = if edit.quote.is_some() {
            let mut braced = Vec::with_capacity(prefix.len() + 2);
            braced.extend_from_slice(&prefix);
            braced.push(b'{');
            for (idx, candidate) in response.candidates.iter().enumerate() {
                if idx > 0 {
                    braced.push(b',');
                }
                braced.extend_from_slice(candidate_suffix(candidate, &prefix));
            }
            braced.push(b'}');
            self.requote_completion_bytes(
                &braced,
                &edit,
                CompletionType::Complete,
                quote_filename,
                hooks,
            )
        } else {
            let mut joined = self.requote_completion_bytes(
                &prefix,
                &edit,
                CompletionType::Complete,
                quote_filename,
                hooks,
            );
            joined.push(b'{');
            for (idx, candidate) in response.candidates.iter().enumerate() {
                if idx > 0 {
                    joined.push(b',');
                }
                joined.extend(self.requote_completion_bytes(
                    candidate_suffix(candidate, &prefix),
                    &edit,
                    CompletionType::Complete,
                    quote_filename,
                    hooks,
                ));
            }
            joined.push(b'}');
            joined
        };
        extend_with_trailing_char(&mut joined, &response.options);
        state
            .buffer
            .replace_range_bytes(edit.start, edit.end, &joined);
        Ok(())
    }

    pub(super) fn requote_completion_bytes(
        &self,
        value: &[u8],
        edit: &CompletionEdit,
        completion_type: CompletionType,
        quote_filename: bool,
        hooks: &mut impl Hooks,
    ) -> Vec<u8> {
        if let Some(quoted) = hooks.quote_completion(QuoteContext {
            value,
            line: &edit.line,
            point: edit.point,
            word_start: edit.start,
            word_end: edit.end,
            word: &edit.word_bytes,
            quote: edit.quote,
            completion_type,
            quote_filename,
        }) {
            return quoted;
        }
        match edit.quote {
            Some('\'') => quote_single_quoted_bytes(value),
            Some('"') => quote_double_quoted_bytes(value),
            _ if quote_filename => quote_filename_bytes(value),
            _ => value.to_vec(),
        }
    }

    pub(super) fn completion_replacement_with_directory(
        &self,
        response: &CompletionResponse,
        edit: &CompletionEdit,
        candidate: &CompletionCandidate,
        completion_type: CompletionType,
        hooks: &mut impl Hooks,
        next_byte: Option<u8>,
    ) -> (Vec<u8>, Option<DirectoryCompletion>) {
        let filename_directory = if response.options.filenames {
            filename_directory_completion(
                &edit.word_bytes,
                candidate.replacement_bytes(),
                &self.filename_options(),
            )
        } else {
            None
        };
        let mut raw = candidate.replacement_bytes().to_vec();
        if append_filename_slash_for_candidate(candidate, filename_directory.as_ref(), next_byte) {
            raw.push(b'/');
        }
        let replacement = self.requote_completion_bytes(
            &raw,
            edit,
            completion_type,
            response.options.quote_filename(),
            hooks,
        );
        (replacement, filename_directory)
    }
}

fn candidate_suffix<'a>(candidate: &'a CompletionCandidate, prefix: &[u8]) -> &'a [u8] {
    candidate
        .replacement_bytes()
        .strip_prefix(prefix)
        .unwrap_or_else(|| candidate.replacement_bytes())
}

pub(super) fn extend_replacement_with_append_char(
    replacement: &mut Vec<u8>,
    options: &CompletionOptions,
    candidate: &CompletionCandidate,
    directory: Option<&DirectoryCompletion>,
) {
    if directory.is_some() || candidate.replacement_bytes().ends_with(b"/") {
        return;
    }
    extend_with_trailing_char(replacement, options);
}

fn extend_with_trailing_char(replacement: &mut Vec<u8>, options: &CompletionOptions) {
    let trailing = if options.nospace {
        None
    } else if let Some(ch) = options.append_character {
        Some(ch)
    } else if !options.suppress_append {
        Some(' ')
    } else {
        None
    };
    if let Some(ch) = trailing {
        let mut buf = [0; 4];
        replacement.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    }
}

pub(super) fn append_filename_slash_for_candidate(
    candidate: &CompletionCandidate,
    directory: Option<&DirectoryCompletion>,
    next_byte: Option<u8>,
) -> bool {
    directory.is_some_and(|directory| {
        directory.append_slash
            && !candidate.replacement_bytes().ends_with(b"/")
            && next_byte != Some(b'/')
    })
}
