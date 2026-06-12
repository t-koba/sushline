use crate::completion::builtin::*;
use crate::completion::display::*;
use crate::completion::filename::{
    FilenameOptions, complete_directories_bytes, complete_filenames_bytes,
};
use crate::completion::quoting::*;
use crate::completion::{
    CompletionContext, CompletionOptions, CompletionRequest, CompletionResponse, CompletionType,
};
use crate::editor::{Editor, ReadlineError};
use crate::hooks::Hooks;
use crate::keymap::KeyMapName;
use crate::state::EditorState;
use crate::terminal::TerminalIo;

fn merge_completion_options(target: &mut CompletionOptions, source: CompletionOptions) {
    target.nospace |= source.nospace;
    target.noquote |= source.noquote;
    target.nosort |= source.nosort;
    target.filenames |= source.filenames;
    target.fullquote |= source.fullquote;
    target.plusdirs |= source.plusdirs;
    target.default |= source.default;
    target.bashdefault |= source.bashdefault;
    target.dirnames |= source.dirnames;
    target.suppress_append |= source.suppress_append;
    target.append_character = target.append_character.or(source.append_character);
    merge_extended_completion_options(target, source);
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn complete(
        &mut self,
        state: &mut EditorState,
        key: &[u8],
        mut completion_type: CompletionType,
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
                hooks,
            )?;
            return Ok(());
        }

        let edit = self.completion_edit(state, hooks);
        let response = self.completion_response(state, key, completion_type, &edit, hooks);
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
                    &edit,
                    hooks,
                )?;
            }
            CompletionType::InsertCompletions => {
                state.buffer.delete_range_bytes(edit.start, edit.end);
                for candidate in &response.candidates {
                    let replacement_bytes = self.requote_completion_bytes(
                        candidate.replacement_bytes(),
                        &edit,
                        completion_type,
                        response.options.quote_filename(),
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
        edit: &CompletionEdit,
        hooks: &mut impl Hooks,
    ) -> CompletionResponse {
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
        let Some(mut response) = hooks.complete(request.clone()) else {
            return self.default_completion(&request, hooks);
        };
        if response.candidates.is_empty()
            && (response.options.bashdefault
                || response.options.default
                || response.options.dirnames
                || response.options.plusdirs)
        {
            let options = response.options.clone();
            response = CompletionResponse::default();
            if options.bashdefault
                && let Some(application_response) = hooks.default_complete(&request)
            {
                response = application_response;
            }
            if response.candidates.is_empty() && options.default {
                response =
                    complete_filenames_bytes(&request.context.word, &self.filename_options());
            } else if response.candidates.is_empty() && options.dirnames {
                response =
                    complete_directories_bytes(&request.context.word, &self.filename_options());
            }
            merge_completion_options(&mut response.options, options);
        }
        if response.options.plusdirs {
            response.candidates.extend(
                complete_directories_bytes(&request.context.word, &self.filename_options())
                    .candidates,
            );
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
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    pub(crate) fn default_completion(
        &mut self,
        request: &CompletionRequest,
        hooks: &mut impl Hooks,
    ) -> CompletionResponse {
        let kind = request.context.completion_type;
        let mut response = match kind {
            CompletionType::Command | CompletionType::PossibleCommandCompletions => {
                complete_commands_with_hooks_bytes(&request.context.word, hooks)
            }
            CompletionType::Username | CompletionType::PossibleUsernameCompletions => {
                complete_users(&String::from_utf8_lossy(&request.context.word), hooks)
            }
            CompletionType::Variable | CompletionType::PossibleVariableCompletions => {
                complete_variables(&String::from_utf8_lossy(&request.context.word), hooks)
            }
            CompletionType::Filename | CompletionType::PossibleFilenameCompletions => {
                complete_filenames_bytes(&request.context.word, &self.filename_options())
            }
            CompletionType::Complete
            | CompletionType::MenuComplete
            | CompletionType::MenuCompleteBackward
            | CompletionType::InsertCompletions => {
                default_application_completion(request, hooks, &self.variables)
            }
            CompletionType::GlobCompleteWord
            | CompletionType::GlobExpandWord
            | CompletionType::GlobListExpansions => {
                glob_complete_bytes(&request.context.word, hooks, &self.variables)
            }
            CompletionType::Hostname | CompletionType::PossibleHostnameCompletions => {
                complete_hosts(&String::from_utf8_lossy(&request.context.word), hooks)
            }
            CompletionType::PossibleCompletions => {
                default_application_completion(request, hooks, &self.variables)
            }
            CompletionType::DynamicHistory => CompletionResponse::default(),
            CompletionType::ViComplete => unreachable!("vi-complete is normalized before dispatch"),
        };
        sort_completion_response(&mut response);
        response
    }
}
