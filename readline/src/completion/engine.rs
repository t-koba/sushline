use crate::completion::builtin::*;
use crate::completion::display::*;
use crate::completion::filename::{
    FilenameOptions, complete_directories_bytes, complete_filenames_bytes,
};
use crate::completion::quoting::*;
use crate::completion::{
    CompletionAction, CompletionContext, CompletionOptions, CompletionRequest, CompletionResponse,
    CompletionType,
};
use crate::editor::{Editor, ReadlineError};
use crate::hooks::Hooks;
use crate::state::{CompletionAttemptState, EditorState, MenuCompletionState};
use crate::terminal::TerminalIo;

struct MenuCompleteContext {
    start: usize,
    end: usize,
    previous_index: Option<usize>,
    original: Vec<u8>,
}

fn menu_complete_context(state: &mut EditorState, edit: &CompletionEdit) -> MenuCompleteContext {
    if let Some(previous) = state.completion.menu_completion.take() {
        MenuCompleteContext {
            start: previous.start,
            end: previous.end,
            previous_index: Some(previous.index),
            original: previous.original,
        }
    } else {
        MenuCompleteContext {
            start: edit.start,
            end: edit.end,
            previous_index: None,
            original: state.buffer.range_bytes(edit.start, edit.end),
        }
    }
}

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
                self.display_completions(&response)?;
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
                    let quoted = self.requote_completion(
                        &candidate.replacement_string(),
                        &edit,
                        response.options.quote_filename(),
                        hooks,
                    );
                    let replacement_bytes = completion_replacement_bytes(
                        candidate,
                        quoted.as_bytes(),
                        &edit,
                        response.options.quote_filename(),
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
    fn insert_completion_response(
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
            self.display_completions(&response)?;
            return Ok(());
        }
        let skip_completed_text = self.variable_is_on("skip-completed-text");
        if response.candidates.len() == 1 {
            let replacement = self.requote_completion(
                &response.candidates[0].replacement_string(),
                edit,
                response.options.quote_filename(),
                hooks,
            );
            let mut replacement_bytes = completion_replacement_bytes(
                &response.candidates[0],
                replacement.as_bytes(),
                edit,
                response.options.quote_filename(),
            );
            let skipped_completed_text =
                skip_completed_text && !completion_suffix_bytes(edit, state).is_empty();
            if skip_completed_text {
                replacement_bytes = skip_completed_suffix_bytes(&replacement_bytes, edit, state);
            }
            state
                .buffer
                .replace_range_bytes(edit.start, edit.end, &replacement_bytes);
            if !response.options.nospace && !skipped_completed_text {
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
                let mut replacement_bytes =
                    if response.options.quote_filename() && edit.quote.is_none() {
                        quote_filename_bytes(&prefix_bytes)
                    } else {
                        prefix_bytes
                    };
                if skip_completed_text {
                    replacement_bytes =
                        skip_completed_suffix_bytes(&replacement_bytes, edit, state);
                }
                state
                    .buffer
                    .replace_range_bytes(edit.start, edit.end, &replacement_bytes);
            }
            if self.variable_is_on("show-all-if-ambiguous")
                || (self.variable_is_on("show-all-if-unmodified")
                    && state.buffer.as_bytes() == before_line.as_slice())
                || (repeated_unmodified_completion
                    && state.buffer.as_bytes() == before_line.as_slice())
            {
                self.display_completions(&response)?;
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

    fn menu_complete(
        &mut self,
        state: &mut EditorState,
        response: CompletionResponse,
        backward: bool,
        edit: &CompletionEdit,
        hooks: &mut impl Hooks,
    ) -> Result<(), ReadlineError> {
        if response.candidates.is_empty() {
            self.ding()?;
            return Ok(());
        }

        let context = menu_complete_context(state, edit);
        let Some(next_index) =
            self.menu_complete_cycle(state, response.candidates.len(), backward, &context)?
        else {
            return Ok(());
        };
        let replacement_bytes = self.menu_complete_replacement(&response, edit, next_index, hooks);
        self.menu_complete_display(state, &response, context.previous_index)?;
        state
            .buffer
            .replace_range_bytes(context.start, context.end, &replacement_bytes);
        state.completion.menu_completion = Some(MenuCompletionState {
            index: next_index,
            start: context.start,
            end: context.start + replacement_bytes.len(),
            original: context.original,
        });
        state.completion.last_completion = Some(response);
        Ok(())
    }

    fn menu_complete_cycle(
        &mut self,
        state: &mut EditorState,
        candidate_count: usize,
        backward: bool,
        context: &MenuCompleteContext,
    ) -> Result<Option<usize>, ReadlineError> {
        let steps = state.numeric_arg.take().unwrap_or(1).unsigned_abs().max(1) as usize;
        let next_index = match (context.previous_index, backward) {
            (Some(index), true) => {
                if steps > index {
                    self.restore_menu_completion(state, context)?;
                    return Ok(None);
                }
                index - steps
            }
            (Some(index), false) => {
                let Some(next) = index.checked_add(steps) else {
                    self.restore_menu_completion(state, context)?;
                    return Ok(None);
                };
                if next >= candidate_count {
                    self.restore_menu_completion(state, context)?;
                    return Ok(None);
                }
                next
            }
            (None, true) => candidate_count.saturating_sub(steps),
            (None, false) => steps.saturating_sub(1).min(candidate_count - 1),
        };
        Ok(Some(next_index))
    }

    fn restore_menu_completion(
        &mut self,
        state: &mut EditorState,
        context: &MenuCompleteContext,
    ) -> Result<(), ReadlineError> {
        self.ding()?;
        state
            .buffer
            .replace_range_bytes(context.start, context.end, &context.original);
        Ok(())
    }

    fn menu_complete_replacement(
        &self,
        response: &CompletionResponse,
        edit: &CompletionEdit,
        next_index: usize,
        hooks: &impl Hooks,
    ) -> Vec<u8> {
        let replacement = self.requote_completion(
            &response.candidates[next_index].replacement_string(),
            edit,
            response.options.quote_filename(),
            hooks,
        );
        completion_replacement_bytes(
            &response.candidates[next_index],
            replacement.as_bytes(),
            edit,
            response.options.quote_filename(),
        )
    }

    fn menu_complete_display(
        &mut self,
        _state: &mut EditorState,
        response: &CompletionResponse,
        previous_index: Option<usize>,
    ) -> Result<(), ReadlineError> {
        if previous_index.is_none()
            && self.variable_is_on("menu-complete-display-prefix")
            && let Some(prefix) = common_prefix_bytes(&response.candidates)
        {
            self.terminal.write("\r\n")?;
            let prefix = String::from_utf8_lossy(&prefix);
            self.terminal.write(prefix.as_ref())?;
            self.terminal.write("\r\n")?;
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
        if !response.candidates.is_empty() {
            let mut joined = Vec::new();
            joined.push(b'{');
            for (idx, candidate) in response.candidates.iter().enumerate() {
                if idx > 0 {
                    joined.push(b',');
                }
                let quoted = self.requote_completion(
                    &candidate.replacement_string(),
                    &edit,
                    response.options.quote_filename(),
                    hooks,
                );
                joined.extend(completion_replacement_bytes(
                    candidate,
                    quoted.as_bytes(),
                    &edit,
                    response.options.quote_filename(),
                ));
            }
            joined.push(b'}');
            state
                .buffer
                .replace_range_bytes(edit.start, edit.end, &joined);
        }
        Ok(())
    }

    fn requote_completion(
        &self,
        value: &str,
        edit: &CompletionEdit,
        quote_filename: bool,
        hooks: &impl Hooks,
    ) -> String {
        match edit.quote {
            Some('\'') => quote_single_quoted(value),
            Some('"') => quote_double_quoted(value),
            _ if quote_filename => hooks
                .quote(value.as_bytes())
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_else(|| quote_unquoted_filename(value)),
            _ => value.to_string(),
        }
    }

    pub(super) fn completion_edit(
        &self,
        state: &EditorState,
        hooks: &impl Hooks,
    ) -> CompletionEdit {
        let word_breaks = self.completion_word_breaks(hooks);
        completion_edit(state, &word_breaks)
    }

    fn filename_options(&self) -> FilenameOptions {
        FilenameOptions::from_variables(&self.variables)
    }

    pub(crate) fn completion_word_breaks(&self, hooks: &impl Hooks) -> Vec<u8> {
        hooks
            .completion_word_breaks()
            .unwrap_or_else(|| b" \t\n".to_vec())
    }

    pub(crate) fn editing_word_breaks(&self, hooks: &impl Hooks) -> Option<String> {
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
