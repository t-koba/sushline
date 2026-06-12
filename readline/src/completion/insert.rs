use crate::completion::display::common_prefix_bytes;
use crate::completion::filename::{DirectoryCompletion, filename_directory_completion};
use crate::completion::quoting::*;
use crate::completion::{CompletionAction, CompletionResponse, CompletionType};
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
            let filename_directory =
                self.filename_directory_completion_for_candidate(&response, edit, candidate);
            let append_filename_slash = append_filename_slash_for_candidate(
                candidate,
                filename_directory.as_ref(),
                completion_suffix_bytes(edit, state).first().copied(),
            );
            let replacement_bytes = self.completion_candidate_replacement_bytes(
                candidate,
                edit,
                completion_type,
                response.options.quote_filename(),
                hooks,
                append_filename_slash,
            );
            let mut replacement_bytes = replacement_bytes;
            let skipped_completed_text =
                skip_completed_text && !completion_suffix_bytes(edit, state).is_empty();
            if skip_completed_text {
                replacement_bytes = skip_completed_suffix_bytes(&replacement_bytes, edit, state);
            }
            state
                .buffer
                .replace_range_bytes(edit.start, edit.end, &replacement_bytes);
            let suppress_append_for_directory =
                filename_directory.is_some() || candidate.replacement_bytes().ends_with(b"/");
            if !suppress_append_for_directory
                && !response.options.nospace
                && !skipped_completed_text
            {
                if let Some(ch) = response.options.append_character {
                    state.buffer.insert_char(ch);
                } else if !response.options.suppress_append {
                    state.buffer.insert_char(' ');
                }
            }
        } else if !response.candidates.is_empty() {
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
                    replacement_bytes =
                        skip_completed_suffix_bytes(&replacement_bytes, edit, state);
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
        let mut braced = Vec::new();
        braced.extend_from_slice(&prefix);
        braced.push(b'{');
        for (idx, candidate) in response.candidates.iter().enumerate() {
            if idx > 0 {
                braced.push(b',');
            }
            let suffix = candidate
                .replacement_bytes()
                .strip_prefix(prefix.as_slice())
                .unwrap_or_else(|| candidate.replacement_bytes());
            braced.extend_from_slice(suffix);
        }
        braced.push(b'}');

        let mut joined = if edit.quote.is_some() {
            self.requote_completion_bytes(
                &braced,
                &edit,
                CompletionType::Complete,
                quote_filename,
                hooks,
            )
        } else {
            self.requote_completion_bytes(
                &prefix,
                &edit,
                CompletionType::Complete,
                quote_filename,
                hooks,
            )
        };
        if edit.quote.is_none() {
            joined.push(b'{');
            for (idx, candidate) in response.candidates.iter().enumerate() {
                if idx > 0 {
                    joined.push(b',');
                }
                let suffix = candidate
                    .replacement_bytes()
                    .strip_prefix(prefix.as_slice())
                    .unwrap_or_else(|| candidate.replacement_bytes());
                joined.extend(self.requote_completion_bytes(
                    suffix,
                    &edit,
                    CompletionType::Complete,
                    quote_filename,
                    hooks,
                ));
            }
            joined.push(b'}');
        }
        if !response.options.nospace {
            if let Some(ch) = response.options.append_character {
                let mut buf = [0; 4];
                joined.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            } else if !response.options.suppress_append {
                joined.push(b' ');
            }
        }
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

    pub(super) fn filename_directory_completion_for_candidate(
        &self,
        response: &CompletionResponse,
        edit: &CompletionEdit,
        candidate: &crate::completion::CompletionCandidate,
    ) -> Option<DirectoryCompletion> {
        response
            .options
            .filenames
            .then(|| {
                filename_directory_completion(
                    &edit.word_bytes,
                    candidate.replacement_bytes(),
                    &self.filename_options(),
                )
            })
            .flatten()
    }

    pub(super) fn completion_candidate_replacement_bytes(
        &self,
        candidate: &crate::completion::CompletionCandidate,
        edit: &CompletionEdit,
        completion_type: CompletionType,
        quote_filename: bool,
        hooks: &mut impl Hooks,
        append_filename_slash: bool,
    ) -> Vec<u8> {
        let mut replacement = candidate.replacement_bytes().to_vec();
        if append_filename_slash {
            replacement.push(b'/');
        }
        self.requote_completion_bytes(&replacement, edit, completion_type, quote_filename, hooks)
    }
}

pub(super) fn append_filename_slash_for_candidate(
    candidate: &crate::completion::CompletionCandidate,
    directory: Option<&DirectoryCompletion>,
    next_byte: Option<u8>,
) -> bool {
    directory.is_some_and(|directory| {
        directory.append_slash
            && !candidate.replacement_bytes().ends_with(b"/")
            && next_byte != Some(b'/')
    })
}
