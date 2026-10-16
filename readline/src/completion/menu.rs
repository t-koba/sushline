use crate::completion::filename::filename_candidate_prefix_len;
use crate::completion::insert::{
    CompletionReplacement, ansi_c_quote_suffix_bytes, closed_ansi_c_safe_candidate,
    extend_with_trailing_char, leave_quoted_filename_suffix_open, preserve_quoted_filename_closer,
    should_append_completion_char,
};
use crate::completion::quoting::{
    CompletionEdit, completion_suffix_bytes_at, completion_suffix_overlap_len,
};
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
    ansi_c_quote_closer_available: bool,
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

        let original = state.buffer.range_bytes(edit.start, edit.end);
        let context = MenuCompleteContext {
            edit: edit.clone(),
            end: edit.end,
            previous_match_index: None,
            ansi_c_quote_closer_available: ansi_c_quote_suffix_bytes(&original) > 0,
            original,
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
            ansi_c_quote_closer_available: previous.ansi_c_quote_closer_available,
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
        let skip_completed_text = self.variable_is_on("skip-completed-text");
        let suffix = completion_suffix_bytes_at(context.end, state);
        let quoted_filename_completion = context.edit.quote.is_some()
            && response.options.filenames
            && response.options.quote_filename();
        let ansi_c_filename_completion = context.edit.ansi_c_quote && quoted_filename_completion;
        let completion_suffix_overlap = if skip_completed_text
            && quoted_filename_completion
            && next_index > 0
            && context.previous_match_index.is_none()
        {
            let candidate = response.candidates[next_index - 1].replacement_bytes();
            let candidate_prefix_len = filename_candidate_prefix_len(
                candidate,
                &context.edit.word_bytes,
                &self.filename_options(),
            );
            // Do not skip right-side bytes when only a case-insensitive prefix match exists.
            let candidate_suffix = match candidate_prefix_len {
                Some(prefix_len)
                    if candidate.get(..prefix_len) == Some(context.edit.word_bytes.as_slice()) =>
                {
                    candidate.get(prefix_len..).unwrap_or_default()
                }
                Some(_) => &[],
                None => candidate,
            };
            completion_suffix_overlap_len(candidate_suffix, &suffix)
        } else {
            0
        };
        let next_byte = if completion_suffix_overlap > 0 {
            suffix.get(completion_suffix_overlap).copied()
        } else {
            suffix.first().copied()
        };
        let (mut replacement_bytes, is_directory, ansi_c_suffix_bytes, quote_hook_used) = self
            .menu_complete_replacement(
                &response,
                &context,
                next_index,
                hooks,
                completion_type,
                next_byte,
            );
        if next_index > 0 {
            let candidate = &response.candidates[next_index - 1];
            if let Some(unquoted) = closed_ansi_c_safe_candidate(
                &context.edit,
                candidate,
                completion_type,
                is_directory,
                quote_hook_used,
                &suffix,
            ) {
                replacement_bytes = unquoted;
            }
        }
        let mut ansi_c_suffix_overlap = completion_suffix_overlap;
        let quoted_suffix_overlap = if next_index > 0 {
            completion_suffix_overlap
                + preserve_quoted_filename_closer(
                    &response,
                    &context.edit,
                    is_directory,
                    suffix.get(completion_suffix_overlap..).unwrap_or_default(),
                    &mut replacement_bytes,
                )
        } else {
            0
        };
        let has_quote_suffix = suffix.get(ansi_c_suffix_overlap) == Some(&b'\'')
            && suffix.len() == ansi_c_suffix_overlap + 1;
        let has_existing_ansi_c_quote = ansi_c_filename_completion
            && next_index > 0
            && ansi_c_suffix_bytes > 0
            && suffix.get(ansi_c_suffix_overlap) == Some(&b'\'');
        let mut preserves_existing_ansi_c_quote = has_existing_ansi_c_quote;
        if has_existing_ansi_c_quote {
            ansi_c_suffix_overlap += 1;
            if ansi_c_suffix_bytes > 1 && suffix.get(ansi_c_suffix_overlap) == Some(&b'/') {
                ansi_c_suffix_overlap += 1;
            }
        } else if ansi_c_filename_completion
            && next_index > 0
            && ansi_c_suffix_bytes == 0
            && (has_quote_suffix
                || context.ansi_c_quote_closer_available
                || ansi_c_quote_suffix_bytes(&context.original) > 0)
        {
            // Carry the existing closer forward when the quote hook leaves it open,
            // so any appended completion character stays outside the quote.
            replacement_bytes.push(b'\'');
            if has_quote_suffix {
                ansi_c_suffix_overlap += 1;
            }
            preserves_existing_ansi_c_quote = true;
        }
        self.menu_complete_display(
            state,
            &response,
            &context.edit.word_bytes,
            context.previous_match_index,
            next_index,
        )?;
        let suffix_overlap = if (skip_completed_text && ansi_c_filename_completion)
            || preserves_existing_ansi_c_quote
        {
            ansi_c_suffix_overlap
        } else {
            quoted_suffix_overlap
        };
        if next_index > 0 && !is_directory {
            leave_quoted_filename_suffix_open(
                &response,
                &context.edit,
                completion_type,
                &suffix,
                suffix_overlap,
                response.candidates[next_index - 1].replacement_bytes(),
                &mut replacement_bytes,
            );
        }
        let replacement_end = context.end + suffix_overlap;
        let append_completion_char = next_index > 0
            && should_append_completion_char(
                &response,
                &context.edit,
                response.candidates[next_index - 1].replacement_bytes(),
            );
        if next_index > 0
            && !is_directory
            && append_completion_char
            && (!quoted_filename_completion
                || suffix_overlap == suffix.len()
                || response.options.append_character.is_some())
            && !state
                .buffer
                .as_bytes()
                .get(replacement_end)
                .is_some_and(u8::is_ascii_whitespace)
        {
            extend_with_trailing_char(&mut replacement_bytes, &response.options);
        }
        state
            .buffer
            .replace_range_bytes(context.edit.start, replacement_end, &replacement_bytes);
        state.completion.menu_completion = Some(MenuCompletionState {
            index: next_index,
            end: context.edit.start + replacement_bytes.len(),
            original: context.original,
            ansi_c_quote_closer_available: context.ansi_c_quote_closer_available
                || preserves_existing_ansi_c_quote,
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

    fn menu_complete_replacement(
        &self,
        response: &CompletionResponse,
        context: &MenuCompleteContext,
        next_index: usize,
        hooks: &mut impl Hooks,
        completion_type: CompletionType,
        next_byte: Option<u8>,
    ) -> (Vec<u8>, bool, usize, bool) {
        if next_index == 0 {
            return (
                self.requote_common_prefix(response, &context.edit, completion_type, hooks)
                    .unwrap_or_default(),
                false,
                0,
                false,
            );
        }
        let candidate = &response.candidates[next_index - 1];
        let CompletionReplacement {
            bytes: replacement,
            is_directory,
            ansi_c_suffix_bytes,
            quote_hook_used,
        } = self.completion_replacement_with_directory(
            response,
            &context.edit,
            candidate,
            completion_type,
            hooks,
            next_byte,
        );
        (
            replacement,
            is_directory,
            ansi_c_suffix_bytes,
            quote_hook_used,
        )
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
