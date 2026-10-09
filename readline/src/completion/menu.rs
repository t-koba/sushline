use crate::completion::display::common_prefix_bytes;
use crate::completion::insert::extend_replacement_with_append_char;
use crate::completion::quoting::CompletionEdit;
use crate::completion::{CompletionAction, CompletionResponse, CompletionType};
use crate::editor::{Editor, ReadlineError};
use crate::hooks::Hooks;
use crate::state::{EditorState, MenuCompletionState, repeat_count};
use crate::terminal::TerminalIo;
use crate::variables::BoolVariable;

struct MenuCompleteContext {
    edit: CompletionEdit,
    end: usize,
    previous_match_index: Option<usize>,
    original: Vec<u8>,
}

fn menu_completion_type(backward: bool) -> CompletionType {
    if backward {
        CompletionType::MenuCompleteBackward
    } else {
        CompletionType::MenuComplete
    }
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
            let completion_type = menu_completion_type(backward);
            self.insert_completion_response(state, response, edit, completion_type, hooks)?;
            return Ok(());
        }

        let context = MenuCompleteContext {
            edit: edit.clone(),
            end: edit.end,
            previous_match_index: None,
            original: state.buffer.range_bytes(edit.start, edit.end),
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
        let context = MenuCompleteContext {
            end: previous.end,
            previous_match_index: Some(previous.index),
            original: previous.original,
            edit: previous.edit,
        };
        let response = previous.response;
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
        let completion_type = menu_completion_type(backward);
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
            &context.edit.word_bytes,
            context.previous_match_index,
            next_index,
        )?;
        state
            .buffer
            .replace_range_bytes(context.edit.start, context.end, &replacement_bytes);
        state.completion.menu_completion = Some(MenuCompletionState {
            index: next_index,
            end: context.edit.start + replacement_bytes.len(),
            original: context.original,
            edit: context.edit,
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
        self.requote_completion_bytes(
            &prefix,
            &context.edit,
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
        let candidate = &response.candidates[next_index - 1];
        let (mut replacement, filename_directory) = self.completion_replacement_with_directory(
            response,
            &context.edit,
            candidate,
            completion_type,
            hooks,
            state.buffer.as_bytes().get(context.end).copied(),
        );
        extend_replacement_with_append_char(
            &mut replacement,
            &response.options,
            candidate,
            filename_directory.as_ref(),
        );
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
