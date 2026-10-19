use super::insert::RequoteOptions;
use crate::completion::builtin::*;
use crate::completion::display::*;
use crate::completion::filename::{
    FilenameOptions, complete_directories_in_context, complete_filenames_in_context,
};
use crate::completion::quoting::*;
use crate::completion::{CompletionContext, CompletionRequest, CompletionResponse, CompletionType};
use crate::editor::{Editor, ReadlineError};
use crate::hooks::Hooks;
use crate::keymap::KeyMapName;
use crate::state::EditorState;
use crate::terminal::TerminalIo;

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn complete(
        &mut self,
        state: &mut EditorState,
        key: &[u8],
        mut completion_type: CompletionType,
        old_menu_complete: bool,
        hooks: &mut impl Hooks,
    ) -> Result<(), ReadlineError> {
        if self.variable_is_on("disable-completion") {
            if insert_disabled_completion_key(state, key) {
                state.record_undo();
            } else {
                self.ding()?;
            }
            return Ok(());
        }
        if completion_type == CompletionType::ViComplete {
            completion_type = match key {
                b"*" => CompletionType::InsertCompletions,
                b"=" => CompletionType::PossibleCompletions,
                _ => CompletionType::Complete,
            };
        }
        if matches!(
            completion_type,
            CompletionType::MenuComplete | CompletionType::MenuCompleteBackward
        ) && let Some(previous) = state.completion.menu_completion.take()
        {
            self.menu_complete_from_previous(
                state,
                previous,
                completion_type == CompletionType::MenuCompleteBackward,
                old_menu_complete,
                hooks,
            )?;
            return Ok(());
        }

        let mut edit = self.completion_edit(state, hooks);
        if matches!(
            completion_type,
            CompletionType::Complete | CompletionType::Filename
        ) {
            edit.filename_segment = edit.reopened_filename_segment();
        }
        let response = self.completion_response(state, key, completion_type, &mut edit, hooks);
        match completion_type {
            CompletionType::PossibleCompletions
            | CompletionType::PossibleCommandCompletions
            | CompletionType::PossibleFilenameCompletions
            | CompletionType::PossibleHostnameCompletions
            | CompletionType::PossibleUsernameCompletions
            | CompletionType::PossibleVariableCompletions
            | CompletionType::GlobListExpansions => {
                self.display_completions_for_word(state, &response, &edit.word_bytes)?;
                state.completion.last_completion = Some(response);
            }
            CompletionType::MenuComplete | CompletionType::MenuCompleteBackward => {
                self.menu_complete(
                    state,
                    response,
                    completion_type == CompletionType::MenuCompleteBackward,
                    old_menu_complete,
                    &edit,
                    hooks,
                )?;
            }
            CompletionType::InsertCompletions => {
                if response.candidates.is_empty() {
                    self.ding()?;
                    return Ok(());
                }
                state.buffer.delete_range_bytes(edit.start, edit.end);
                for candidate in &response.candidates {
                    let replacement_bytes = self.requote_completion_bytes(
                        candidate.replacement_bytes(),
                        &edit,
                        completion_type,
                        RequoteOptions {
                            quote_filename: response.options.quote_filename(),
                            filename_candidates: response.options.filenames,
                            close_quote: true,
                            close_ansi_c_quote: true,
                            slash_after_quote: false,
                        },
                        hooks,
                    );
                    state.buffer.insert_bytes(&replacement_bytes);
                    state.buffer.insert_char(' ');
                }
                state.completion.last_completion = Some(response);
            }
            CompletionType::GlobExpandWord => {
                if !response.candidates.is_empty() {
                    let mut expanded = Vec::new();
                    for (idx, candidate) in response.candidates.iter().enumerate() {
                        if idx > 0 {
                            expanded.push(b' ');
                        }
                        expanded.extend_from_slice(candidate.replacement_bytes());
                    }
                    if response.candidates.len() > 1 {
                        expanded.push(b' ');
                    }
                    state
                        .buffer
                        .replace_range_bytes(edit.start, edit.end, &expanded);
                } else {
                    self.ding()?;
                }
                state.completion.last_completion = Some(response);
            }
            _ => self.insert_completion_response(state, response, &edit, completion_type, hooks)?,
        }
        Ok(())
    }

    pub(super) fn completion_response(
        &mut self,
        state: &EditorState,
        key: &[u8],
        completion_type: CompletionType,
        edit: &mut CompletionEdit,
        hooks: &mut impl Hooks,
    ) -> CompletionResponse {
        // Callbacks always see the full word; restore this scope only if a
        // built-in provider is ultimately selected.
        let mut segment = edit.filename_segment.take();
        let request = CompletionRequest {
            context: CompletionContext {
                line: state.buffer.as_bytes().to_vec(),
                point: state.buffer.byte_point(),
                word_start: state.buffer.byte_index_for_char_index(edit.start),
                word_end: state.buffer.byte_index_for_char_index(edit.end),
                word: edit.word_bytes.clone(),
                key: key.to_vec(),
                completion_type,
            },
        };
        let mut filename_context = request.context.clone();
        if let Some(segment) = &segment {
            filename_context.word_start = segment.raw_start;
            filename_context.word = segment.word_bytes.clone();
        }
        let Some(mut response) = hooks.complete(request.clone()) else {
            if segment.is_none() {
                segment = edit.reopened_filename_segment();
                if let Some(segment) = &segment {
                    filename_context.word_start = segment.raw_start;
                    filename_context.word = segment.word_bytes.clone();
                }
            }
            let (response, builtin_filenames) =
                self.default_completion(&request, hooks, Some(&filename_context));
            if builtin_filenames {
                edit.filename_segment = segment;
                edit.builtin_filename_completion = true;
            }
            return response;
        };
        let mut empty_application_response = false;
        let mut builtin_filename_fallback = false;
        let needs_filename_context = response.options.plusdirs
            || (response.candidates.is_empty()
                && (response.options.bashdefault
                    || response.options.default
                    || response.options.dirnames));
        if needs_filename_context && segment.is_none() {
            segment = edit.reopened_filename_segment();
            if let Some(segment) = &segment {
                filename_context.word_start = segment.raw_start;
                filename_context.word = segment.word_bytes.clone();
            }
        }
        if response.candidates.is_empty()
            && (response.options.bashdefault
                || response.options.default
                || response.options.dirnames
                || response.options.plusdirs)
        {
            let options = std::mem::take(&mut response.options);
            if options.bashdefault
                && let Some(application_response) = hooks.default_complete(&request)
            {
                response = application_response;
            }
            empty_application_response = response.candidates.is_empty();
            if empty_application_response {
                if options.default {
                    response =
                        complete_filenames_in_context(&filename_context, &self.filename_options());
                } else if options.dirnames {
                    response = complete_directories_in_context(
                        &filename_context,
                        &self.filename_options(),
                    );
                }
                builtin_filename_fallback |=
                    options.default || options.dirnames || options.plusdirs;
            }
            merge_completion_options(&mut response.options, options);
        }
        if response.options.plusdirs {
            // `plusdirs` supplements COMPREPLY even when it is nonempty. Keep
            // callback replacements full-word, while matching these built-in
            // directory candidates against the active filename segment.
            response.candidates.extend(
                complete_directories_in_context(&filename_context, &self.filename_options())
                    .candidates,
            );
            builtin_filename_fallback |= empty_application_response;
        }
        if builtin_filename_fallback {
            edit.filename_segment = segment;
            edit.builtin_filename_completion = true;
        }
        apply_extended_completion_options(&mut response);
        sort_completion_response(&mut response);
        response
    }

    pub(super) fn completion_edit(
        &self,
        state: &EditorState,
        hooks: &mut impl Hooks,
    ) -> CompletionEdit {
        let word_breaks = self.completion_word_breaks(hooks);
        completion_edit(
            state,
            &word_breaks,
            matches!(self.keymap.current(), KeyMapName::ViCommand),
        )
    }

    pub(super) fn filename_options(&self) -> FilenameOptions {
        FilenameOptions::from_variables(&self.variables)
    }

    pub(crate) fn completion_word_breaks(&self, hooks: &mut impl Hooks) -> Vec<u8> {
        hooks
            .completion_word_breaks()
            .unwrap_or_else(|| b" \t\n".to_vec())
    }

    pub(crate) fn editing_word_breaks(&self, hooks: &mut impl Hooks) -> Option<String> {
        hooks
            .editing_word_breaks()
            .map(|bytes| crate::buffer::bytes_decoded_lossless(&bytes))
    }

    // The boolean identifies the built-in filename provider, not merely a
    // response whose candidates an application marked as filenames.
    pub(crate) fn default_completion(
        &mut self,
        request: &CompletionRequest,
        hooks: &mut impl Hooks,
        filename_context: Option<&CompletionContext>,
    ) -> (CompletionResponse, bool) {
        let kind = request.context.completion_type;
        let filename_context = filename_context.unwrap_or(&request.context);
        let mut builtin_filenames = false;
        let mut response = match kind {
            CompletionType::Command | CompletionType::PossibleCommandCompletions => {
                complete_commands_with_hooks_bytes(&request.context.word, hooks)
            }
            CompletionType::Username | CompletionType::PossibleUsernameCompletions => {
                complete_users(&request.context.word, hooks)
            }
            CompletionType::Variable | CompletionType::PossibleVariableCompletions => {
                complete_variables(&request.context.word, hooks)
            }
            CompletionType::Filename | CompletionType::PossibleFilenameCompletions => {
                builtin_filenames = true;
                complete_filenames_in_context(filename_context, &self.filename_options())
            }
            CompletionType::Complete
            | CompletionType::MenuComplete
            | CompletionType::MenuCompleteBackward
            | CompletionType::InsertCompletions
            | CompletionType::PossibleCompletions => {
                let (response, filenames) = default_application_completion(
                    request,
                    hooks,
                    &self.variables,
                    filename_context,
                );
                builtin_filenames = filenames;
                response
            }
            CompletionType::GlobCompleteWord
            | CompletionType::GlobExpandWord
            | CompletionType::GlobListExpansions => {
                glob_complete_bytes(&request.context.word, hooks, &self.variables)
            }
            CompletionType::Hostname | CompletionType::PossibleHostnameCompletions => {
                complete_hosts(&request.context.word, hooks)
            }
            CompletionType::DynamicHistory => CompletionResponse::default(),
            CompletionType::ViComplete => unreachable!("vi-complete is normalized before dispatch"),
        };
        sort_completion_response(&mut response);
        (response, builtin_filenames)
    }
}
