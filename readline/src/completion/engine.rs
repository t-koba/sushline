use crate::completion::builtin::*;
use crate::completion::display::*;
use crate::completion::filename::{
    DirectoryCompletion, FilenameOptions, complete_directories_bytes, complete_filenames_bytes,
    filename_directory_completion,
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
    previous_match_index: Option<usize>,
    original: Vec<u8>,
    word_bytes: Vec<u8>,
    quote: Option<char>,
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
        if response.options.action == Some(CompletionAction::DisplayOnly) {
            self.display_completions_for_word(state, &response, &edit.word_bytes)?;
            state.completion.last_completion = Some(response);
            return Ok(());
        }
        if response.candidates.len() == 1 {
            self.insert_completion_response(
                state,
                response,
                edit,
                CompletionType::MenuComplete,
                hooks,
            )?;
            return Ok(());
        }

        let context = MenuCompleteContext {
            start: edit.start,
            end: edit.end,
            previous_match_index: None,
            original: state.buffer.range_bytes(edit.start, edit.end),
            word_bytes: edit.word_bytes.clone(),
            quote: edit.quote,
        };
        self.menu_complete_with_context(state, response, backward, hooks, context)
    }

    fn menu_complete_from_previous(
        &mut self,
        state: &mut EditorState,
        previous: MenuCompletionState,
        backward: bool,
        hooks: &impl Hooks,
    ) -> Result<(), ReadlineError> {
        let response = previous.response;
        let context = MenuCompleteContext {
            start: previous.start,
            end: previous.end,
            previous_match_index: Some(previous.index),
            original: previous.original,
            word_bytes: previous.word_bytes,
            quote: previous.quote,
        };
        self.menu_complete_with_context(state, response, backward, hooks, context)
    }

    fn menu_complete_with_context(
        &mut self,
        state: &mut EditorState,
        response: CompletionResponse,
        backward: bool,
        hooks: &impl Hooks,
        context: MenuCompleteContext,
    ) -> Result<(), ReadlineError> {
        let next_index = self.menu_complete_cycle(
            state,
            response.candidates.len(),
            backward,
            context.previous_match_index,
        );
        let replacement_bytes =
            self.menu_complete_replacement(&response, &context, next_index, hooks, state);
        self.menu_complete_display(
            state,
            &response,
            &context.word_bytes,
            context.previous_match_index,
            next_index,
        )?;
        state
            .buffer
            .replace_range_bytes(context.start, context.end, &replacement_bytes);
        state.completion.menu_completion = Some(MenuCompletionState {
            index: next_index,
            start: context.start,
            end: context.start + replacement_bytes.len(),
            original: context.original,
            word_bytes: context.word_bytes,
            quote: context.quote,
            response: response.clone(),
        });
        state.completion.last_completion = Some(response);
        Ok(())
    }

    fn menu_complete_cycle(
        &self,
        state: &mut EditorState,
        candidate_count: usize,
        backward: bool,
        previous_match_index: Option<usize>,
    ) -> usize {
        let arg = state.numeric_arg.take().unwrap_or(1);
        let backward = if arg < 0 { !backward } else { backward };
        let steps = arg.unsigned_abs().max(1) as usize;
        let match_count = candidate_count + 1;
        if previous_match_index.is_none() && self.variable_is_on("menu-complete-display-prefix") {
            return 0;
        }
        let current = previous_match_index.unwrap_or(0);
        match (backward, current) {
            (true, current) => {
                let offset = steps % match_count;
                (current + match_count - offset) % match_count
            }
            (false, current) => (current + steps) % match_count,
        }
    }

    fn menu_complete_prefix_replacement(
        &self,
        response: &CompletionResponse,
        context: &MenuCompleteContext,
        hooks: &impl Hooks,
    ) -> Vec<u8> {
        let Some(prefix) = common_prefix_bytes(&response.candidates) else {
            return Vec::new();
        };
        let edit = CompletionEdit {
            start: context.start,
            end: context.start + context.original.len(),
            word_bytes: context.word_bytes.clone(),
            quote: context.quote,
        };
        self.requote_completion_bytes(&prefix, &edit, response.options.quote_filename(), hooks)
    }

    fn menu_complete_replacement(
        &self,
        response: &CompletionResponse,
        context: &MenuCompleteContext,
        next_index: usize,
        hooks: &impl Hooks,
        state: &EditorState,
    ) -> Vec<u8> {
        if next_index == 0 {
            return self.menu_complete_prefix_replacement(response, context, hooks);
        }
        let edit = CompletionEdit {
            start: context.start,
            end: context.start + context.original.len(),
            word_bytes: context.word_bytes.clone(),
            quote: context.quote,
        };
        let candidate = &response.candidates[next_index - 1];
        let filename_directory =
            self.filename_directory_completion_for_candidate(response, &edit, candidate);
        let append_filename_slash = append_filename_slash_for_candidate(
            candidate,
            filename_directory.as_ref(),
            state.buffer.as_bytes().get(context.end).copied(),
        );
        let mut replacement = self.completion_candidate_replacement_bytes(
            candidate,
            &edit,
            response.options.quote_filename(),
            hooks,
            append_filename_slash,
        );
        let suppress_append_for_directory = filename_directory.is_some()
            || candidate.replacement_bytes().ends_with(b"/")
            || append_filename_slash;
        if !suppress_append_for_directory && !response.options.nospace {
            if let Some(ch) = response.options.append_character {
                let mut buf = [0; 4];
                replacement.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            } else if !response.options.suppress_append {
                replacement.push(b' ');
            }
        }
        replacement
    }

    fn menu_complete_display(
        &mut self,
        state: &mut EditorState,
        response: &CompletionResponse,
        word_bytes: &[u8],
        previous_match_index: Option<usize>,
        next_index: usize,
    ) -> Result<(), ReadlineError> {
        if previous_match_index.is_none() {
            if self.variable_is_on("show-all-if-ambiguous") {
                self.display_completions_for_word(state, response, word_bytes)?;
            }
            if self.variable_is_on("menu-complete-display-prefix") && next_index == 0 {
                self.ding()?;
            }
        } else if next_index == 0 {
            self.ding()?;
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
                joined.extend(self.requote_completion_bytes(
                    candidate.replacement_bytes(),
                    &edit,
                    response.options.quote_filename(),
                    hooks,
                ));
            }
            joined.push(b'}');
            state
                .buffer
                .replace_range_bytes(edit.start, edit.end, &joined);
        }
        Ok(())
    }

    fn requote_completion_bytes(
        &self,
        value: &[u8],
        edit: &CompletionEdit,
        quote_filename: bool,
        hooks: &impl Hooks,
    ) -> Vec<u8> {
        match edit.quote {
            Some('\'') => quote_single_quoted_bytes(value),
            Some('"') => quote_double_quoted_bytes(value),
            _ if quote_filename => hooks
                .quote(value)
                .unwrap_or_else(|| quote_filename_bytes(value)),
            _ => value.to_vec(),
        }
    }

    fn filename_directory_completion_for_candidate(
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

    fn completion_candidate_replacement_bytes(
        &self,
        candidate: &crate::completion::CompletionCandidate,
        edit: &CompletionEdit,
        quote_filename: bool,
        hooks: &impl Hooks,
        append_filename_slash: bool,
    ) -> Vec<u8> {
        let mut replacement = candidate.replacement_bytes().to_vec();
        if append_filename_slash {
            replacement.push(b'/');
        }
        self.requote_completion_bytes(&replacement, edit, quote_filename, hooks)
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

fn append_filename_slash_for_candidate(
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
