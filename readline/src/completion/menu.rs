use crate::completion::display::common_prefix_bytes;
use crate::completion::insert::append_filename_slash_for_candidate;
use crate::completion::quoting::CompletionEdit;
use crate::completion::{CompletionAction, CompletionResponse, CompletionType};
use crate::editor::{Editor, ReadlineError};
use crate::hooks::Hooks;
use crate::state::{EditorState, MenuCompletionState, repeat_count};
use crate::terminal::TerminalIo;
use crate::variables::BoolVariable;

struct MenuCompleteContext {
    start: usize,
    end: usize,
    previous_match_index: Option<usize>,
    original: Vec<u8>,
    word_bytes: Vec<u8>,
    quote: Option<char>,
    line: Vec<u8>,
    point: usize,
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn menu_complete(
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
            let completion_type = if backward {
                CompletionType::MenuCompleteBackward
            } else {
                CompletionType::MenuComplete
            };
            self.insert_completion_response(state, response, edit, completion_type, hooks)?;
            return Ok(());
        }

        let context = MenuCompleteContext {
            start: edit.start,
            end: edit.end,
            previous_match_index: None,
            original: state.buffer.range_bytes(edit.start, edit.end),
            word_bytes: edit.word_bytes.clone(),
            quote: edit.quote,
            line: edit.line.clone(),
            point: edit.point,
        };
        self.menu_complete_with_context(state, response, backward, hooks, context)
    }

    pub(super) fn menu_complete_from_previous(
        &mut self,
        state: &mut EditorState,
        previous: MenuCompletionState,
        backward: bool,
        hooks: &mut impl Hooks,
    ) -> Result<(), ReadlineError> {
        let response = previous.response;
        let context = MenuCompleteContext {
            start: previous.start,
            end: previous.end,
            previous_match_index: Some(previous.index),
            original: previous.original,
            word_bytes: previous.word_bytes,
            quote: previous.quote,
            line: previous.line,
            point: previous.point,
        };
        self.menu_complete_with_context(state, response, backward, hooks, context)
    }

    fn menu_complete_with_context(
        &mut self,
        state: &mut EditorState,
        response: CompletionResponse,
        backward: bool,
        hooks: &mut impl Hooks,
        context: MenuCompleteContext,
    ) -> Result<(), ReadlineError> {
        let next_index = self.menu_complete_cycle(
            state,
            response.candidates.len(),
            backward,
            context.previous_match_index,
        );
        let completion_type = if backward {
            CompletionType::MenuCompleteBackward
        } else {
            CompletionType::MenuComplete
        };
        let replacement_bytes = self.menu_complete_replacement(
            &response,
            &context,
            next_index,
            hooks,
            state,
            completion_type,
        );
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
            line: context.line,
            point: context.point,
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
        let arg = state.numeric_arg.take();
        let signed_arg = arg.unwrap_or(1);
        let backward = if signed_arg < 0 { !backward } else { backward };
        let steps = repeat_count(arg) as usize;
        let match_count = candidate_count + 1;
        if previous_match_index.is_none() && self.flag(BoolVariable::MenuCompleteDisplayPrefix) {
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
        hooks: &mut impl Hooks,
        completion_type: CompletionType,
    ) -> Vec<u8> {
        let Some(prefix) = common_prefix_bytes(&response.candidates) else {
            return Vec::new();
        };
        let edit = CompletionEdit {
            start: context.start,
            end: context.start + context.original.len(),
            word_bytes: context.word_bytes.clone(),
            quote: context.quote,
            line: context.line.clone(),
            point: context.point,
        };
        self.requote_completion_bytes(
            &prefix,
            &edit,
            completion_type,
            response.options.quote_filename(),
            hooks,
        )
    }

    fn menu_complete_replacement(
        &self,
        response: &CompletionResponse,
        context: &MenuCompleteContext,
        next_index: usize,
        hooks: &mut impl Hooks,
        state: &EditorState,
        completion_type: CompletionType,
    ) -> Vec<u8> {
        if next_index == 0 {
            return self.menu_complete_prefix_replacement(
                response,
                context,
                hooks,
                completion_type,
            );
        }
        let edit = CompletionEdit {
            start: context.start,
            end: context.start + context.original.len(),
            word_bytes: context.word_bytes.clone(),
            quote: context.quote,
            line: context.line.clone(),
            point: context.point,
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
            completion_type,
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
            if self.flag(BoolVariable::ShowAllIfAmbiguous) {
                self.display_completions_for_word(state, response, word_bytes)?;
            }
            if self.flag(BoolVariable::MenuCompleteDisplayPrefix) && next_index == 0 {
                self.ding()?;
            }
        } else if next_index == 0 {
            self.ding()?;
        }
        Ok(())
    }
}
