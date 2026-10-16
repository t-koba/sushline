use crate::completion::display::common_prefix_bytes;
use crate::completion::filename::{
    filename_candidate_prefix_len, filename_directory_completion, is_named_user_tilde_path,
};
use crate::completion::quoting::*;
use crate::completion::{
    CompletionAction, CompletionCandidate, CompletionOptions, CompletionResponse, CompletionType,
};
use crate::editor::{Editor, ReadlineError};
use crate::hooks::{Hooks, QuoteContext};
use crate::state::{CompletionAttemptState, EditorState};
use crate::terminal::TerminalIo;
use crate::variables::BoolVariable;

pub(super) struct CompletionReplacement {
    pub(super) bytes: Vec<u8>,
    pub(super) is_directory: bool,
    pub(super) ansi_c_suffix_bytes: usize,
}

pub(super) fn should_append_completion_char(
    response: &CompletionResponse,
    edit: &CompletionEdit,
    candidate: &[u8],
) -> bool {
    if response.options.filenames
        && edit.quote.is_some()
        && response.options.quote_filename()
        && response.options.append_character.is_none()
    {
        // Tested Bash 5.3 menu behavior appends a separator for these leading
        // bytes even though the quoted replacement itself leaves them raw.
        quote_filename_bytes(candidate, true, true, true).as_slice() != candidate
    } else {
        true
    }
}

pub(super) fn preserve_quoted_filename_closer(
    response: &CompletionResponse,
    edit: &CompletionEdit,
    is_directory: bool,
    suffix: &[u8],
    replacement: &mut Vec<u8>,
) -> usize {
    if edit.ansi_c_quote || !response.options.filenames || !response.options.quote_filename() {
        return 0;
    }
    let Some(quote @ ('\'' | '"')) = edit.quote else {
        return 0;
    };
    let quote_byte = quote as u8;
    // For directories, consume only the closer and preserve adjacent word bytes.
    if suffix.first() != Some(&quote_byte) || (!is_directory && suffix.len() != 1) {
        return 0;
    }
    if replacement.last() != Some(&quote_byte)
        && !(is_directory && replacement.ends_with(&[quote_byte, b'/']))
    {
        replacement.push(quote_byte);
    }
    1
}

pub(super) fn leave_quoted_filename_suffix_open(
    response: &CompletionResponse,
    edit: &CompletionEdit,
    completion_type: CompletionType,
    suffix: &[u8],
    suffix_overlap: usize,
    candidate: &[u8],
    replacement: &mut Vec<u8>,
) {
    // Menu completion retains its special-filename separator policy; regular
    // filename completion keeps an unconsumed suffix inside its original quote.
    if suffix_overlap >= suffix.len()
        || (matches!(
            completion_type,
            CompletionType::MenuComplete | CompletionType::MenuCompleteBackward
        ) && should_append_completion_char(response, edit, candidate))
        || !response.options.filenames
        || !response.options.quote_filename()
    {
        return;
    }
    let Some(quote) = edit.quote else {
        return;
    };
    let quote_byte = quote as u8;
    if suffix.last() == Some(&quote_byte) && replacement.last() == Some(&quote_byte) {
        replacement.pop();
    }
}

#[derive(Clone, Copy)]
pub(super) struct RequoteOptions {
    pub(super) quote_filename: bool,
    pub(super) filename_candidates: bool,
    pub(super) close_quote: bool,
    pub(super) close_ansi_c_quote: bool,
    pub(super) slash_after_quote: bool,
}

// Include the actual closer, but not adjacent shell words. Whitespace and escaped
// quotes within the open double-quoted filename are still part of its raw suffix.
fn double_quoted_filename_suffix(raw_suffix: &[u8]) -> &[u8] {
    let mut index = 0;
    while let Some(byte) = raw_suffix.get(index) {
        match byte {
            b'\\' => index += 2,
            b'"' => return &raw_suffix[..=index],
            _ => index += 1,
        }
    }
    raw_suffix
}

fn shell_filename_suffix_overlap_len(
    completion: &[u8],
    raw_suffix: &[u8],
    quote: Option<char>,
) -> Option<usize> {
    let mut semantic_suffix = Vec::new();
    let mut raw_prefix_lengths = Vec::new();
    let mut index = 0;
    while let Some(byte) = raw_suffix.get(index).copied() {
        match byte {
            b'\'' if quote == Some('\'') => {
                // A quoted apostrophe closes the quote, escapes the byte, and reopens it.
                if !raw_suffix[index..].starts_with(b"'\\''") {
                    break; // Keep a standalone shell closer out of the semantic overlap.
                }
                semantic_suffix.push(b'\'');
                index += 4;
                raw_prefix_lengths.push(index);
            }
            b'\\' if quote != Some('\'') => match raw_suffix.get(index + 1).copied() {
                Some(b'\n') => index += 2,
                Some(escaped)
                    if quote.is_none() || matches!(escaped, b'"' | b'\\' | b'$' | b'`') =>
                {
                    semantic_suffix.push(escaped);
                    index += 2;
                    raw_prefix_lengths.push(index);
                }
                _ => {
                    semantic_suffix.push(b'\\');
                    index += 1;
                    raw_prefix_lengths.push(index);
                }
            },
            b'"' if quote == Some('"') => break, // Keep the existing shell closer.
            b'\'' | b'"' if quote.is_none() => return None,
            byte if quote.is_none() && byte.is_ascii_whitespace() => break,
            byte => {
                semantic_suffix.push(byte);
                index += 1;
                raw_prefix_lengths.push(index);
            }
        }
    }

    let overlap = completion_suffix_overlap_len(completion, &semantic_suffix);
    Some(
        overlap
            .checked_sub(1)
            .and_then(|index| raw_prefix_lengths.get(index).copied())
            .unwrap_or(0),
    )
}

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
            self.display_completions_for_word(state, &response, edit.matching_word())?;
            return Ok(());
        }
        let raw_word = edit.line.get(edit.start..edit.end).unwrap_or_default();
        if response.options.filenames
            && response.options.quote_filename()
            && matches!(
                completion_type,
                CompletionType::Complete | CompletionType::Filename
            )
            && !edit.ansi_c_quote
            && edit.quote == Some('\'')
            && raw_word.len() > 1
            && raw_word.first() == Some(&b'\'')
            && raw_word.last() == Some(&b'\'')
        {
            // Point immediately after a reopened single-quote delimiter has no
            // text in that segment. Bash runtime probes leave this boundary
            // unchanged, rather than requoting the preceding concatenation.
            self.ding()?;
            return Ok(());
        }
        let skip_completed_text = self.variable_is_on("skip-completed-text");
        if response.candidates.len() == 1 {
            let candidate = &response.candidates[0];
            let suffix = completion_suffix_bytes(edit, state);
            let ansi_c_filename_completion = edit.ansi_c_quote
                && response.options.filenames
                && response.options.quote_filename();
            // Only an exact typed filename prefix can justify consuming its right suffix.
            let filename_case_insensitive_prefix_only_match = skip_completed_text
                && response.options.filenames
                && response.options.quote_filename()
                && filename_candidate_prefix_len(
                    candidate.replacement_bytes(),
                    edit.matching_word(),
                    &self.filename_options(),
                )
                .is_some()
                && !candidate
                    .replacement_bytes()
                    .starts_with(edit.matching_word());
            let mut ansi_c_suffix_overlap = if skip_completed_text
                && ansi_c_filename_completion
                && !filename_case_insensitive_prefix_only_match
            {
                completed_suffix_overlap_len(candidate.replacement_bytes(), &suffix)
            } else {
                0
            };
            let next_byte = if ansi_c_suffix_overlap > 0 {
                suffix.get(ansi_c_suffix_overlap).copied()
            } else {
                suffix.first().copied()
            };
            let original = state.buffer.range_bytes(edit.start, edit.end);
            let CompletionReplacement {
                bytes: mut replacement_bytes,
                is_directory,
                ansi_c_suffix_bytes,
            } = self.completion_replacement_with_directory(
                &response,
                edit,
                candidate,
                completion_type,
                hooks,
                next_byte,
            );
            let has_quote_suffix = suffix.get(ansi_c_suffix_overlap) == Some(&b'\'')
                && suffix.len() == ansi_c_suffix_overlap + 1;
            let has_existing_ansi_c_quote = ansi_c_filename_completion
                && ansi_c_suffix_bytes > 0
                && suffix.get(ansi_c_suffix_overlap) == Some(&b'\'');
            let mut preserves_existing_ansi_c_quote = has_existing_ansi_c_quote;
            if has_existing_ansi_c_quote {
                ansi_c_suffix_overlap += 1;
                if ansi_c_suffix_bytes > 1 && suffix.get(ansi_c_suffix_overlap) == Some(&b'/') {
                    ansi_c_suffix_overlap += 1;
                }
            } else if ansi_c_filename_completion
                && ansi_c_suffix_bytes == 0
                && (has_quote_suffix || ansi_c_quote_suffix_bytes(&original) > 0)
            {
                replacement_bytes.push(b'\'');
                if has_quote_suffix {
                    ansi_c_suffix_overlap += 1;
                }
                preserves_existing_ansi_c_quote = true;
            }
            let quoted_filename_suffix_overlap = if is_directory
                || matches!(
                    completion_type,
                    CompletionType::MenuComplete | CompletionType::MenuCompleteBackward
                ) {
                preserve_quoted_filename_closer(
                    &response,
                    edit,
                    is_directory,
                    &suffix,
                    &mut replacement_bytes,
                )
            } else {
                0
            };
            let append_completion_char =
                !matches!(
                    completion_type,
                    CompletionType::MenuComplete | CompletionType::MenuCompleteBackward
                ) || should_append_completion_char(&response, edit, candidate.replacement_bytes());
            let mut replacement_end = edit.end;
            if quoted_filename_suffix_overlap > 0 {
                replacement_end += quoted_filename_suffix_overlap;
                if !state
                    .buffer
                    .as_bytes()
                    .get(replacement_end)
                    .is_some_and(u8::is_ascii_whitespace)
                    && !is_directory
                    && append_completion_char
                {
                    extend_with_trailing_char(&mut replacement_bytes, &response.options);
                }
            } else if skip_completed_text && !suffix.is_empty() {
                let overlap = if filename_case_insensitive_prefix_only_match {
                    0
                } else if ansi_c_filename_completion {
                    ansi_c_suffix_overlap
                } else {
                    completed_suffix_overlap_len(&replacement_bytes, &suffix)
                };
                if overlap == suffix.len() {
                    replacement_end += suffix.len();
                    let suffix_ends_with_quote = edit
                        .quote
                        .is_some_and(|quote| suffix.last() == Some(&(quote as u8)));
                    // Keep the existing delimiter behavior for a closer-only suffix,
                    // but do not append another delimiter after consuming a quoted filename tail.
                    if !state
                        .buffer
                        .as_bytes()
                        .get(replacement_end)
                        .is_some_and(u8::is_ascii_whitespace)
                        && !is_directory
                        && append_completion_char
                        && (!suffix_ends_with_quote
                            || suffix.len() == 1
                            || should_append_completion_char(
                                &response,
                                edit,
                                candidate.replacement_bytes(),
                            ))
                    {
                        extend_with_trailing_char(&mut replacement_bytes, &response.options);
                    }
                } else if ansi_c_filename_completion {
                    // Keep the quoted completion intact and consume only the
                    // overlapping part of the existing suffix.
                    replacement_end += overlap;
                } else {
                    replacement_bytes.truncate(replacement_bytes.len().saturating_sub(overlap));
                }
            } else if preserves_existing_ansi_c_quote {
                // The closing quote is syntax, not completed filename text;
                // consume it even when skip-completed-text is disabled.
                replacement_end += ansi_c_suffix_overlap;
                if suffix.len() == ansi_c_suffix_overlap
                    && !state
                        .buffer
                        .as_bytes()
                        .get(replacement_end)
                        .is_some_and(u8::is_ascii_whitespace)
                    && !is_directory
                    && append_completion_char
                {
                    extend_with_trailing_char(&mut replacement_bytes, &response.options);
                }
            } else if suffix.is_empty()
                && !state
                    .buffer
                    .as_bytes()
                    .get(edit.end)
                    .is_some_and(u8::is_ascii_whitespace)
                && !is_directory
                && append_completion_char
            {
                extend_with_trailing_char(&mut replacement_bytes, &response.options);
            }
            if !is_directory {
                leave_quoted_filename_suffix_open(
                    &response,
                    edit,
                    completion_type,
                    &suffix,
                    replacement_end.saturating_sub(edit.end),
                    candidate.replacement_bytes(),
                    &mut replacement_bytes,
                );
            }
            state
                .buffer
                .replace_range_bytes(edit.start, replacement_end, &replacement_bytes);
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
            if let Some((common_prefix, mut replacement_bytes)) =
                self.requote_common_prefix_with_prefix(&response, edit, completion_type, hooks)
            {
                let shell_escaped_regular_filename = response.options.filenames
                    && !edit.ansi_c_quote
                    && !matches!(
                        completion_type,
                        CompletionType::MenuComplete | CompletionType::MenuCompleteBackward
                    )
                    && (matches!(edit.quote, None | Some('"'))
                        || (edit.quote == Some('\'') && response.options.quote_filename()));
                let single_quoted_regular_filename =
                    shell_escaped_regular_filename && edit.quote == Some('\'');
                let raw_suffix = state.buffer.as_bytes().get(edit.end..).unwrap_or_default();
                let suffix = if shell_escaped_regular_filename && edit.quote == Some('"') {
                    double_quoted_filename_suffix(raw_suffix).to_vec()
                } else {
                    completion_suffix_bytes(edit, state)
                };
                let mut replacement_end = edit.end;
                if skip_completed_text {
                    if response.options.filenames {
                        let overlap = filename_candidate_prefix_len(
                            &common_prefix,
                            edit.matching_word(),
                            &self.filename_options(),
                        )
                        .filter(|prefix_len| {
                            common_prefix.get(..*prefix_len) == Some(edit.matching_word())
                        })
                        .map(|prefix_len| {
                            let completion_suffix =
                                common_prefix.get(prefix_len..).unwrap_or_default();
                            let overlap = completion_suffix_overlap_len(completion_suffix, &suffix);
                            if shell_escaped_regular_filename {
                                // Match shell-decoded bytes but consume the encoded buffer length.
                                shell_filename_suffix_overlap_len(
                                    completion_suffix,
                                    if edit.quote == Some('"') {
                                        &suffix
                                    } else {
                                        raw_suffix
                                    },
                                    edit.quote,
                                )
                                .unwrap_or(overlap)
                            } else {
                                overlap
                            }
                        })
                        .unwrap_or(0);
                        replacement_end += overlap;
                    } else {
                        replacement_bytes =
                            skip_completed_suffix_bytes(&replacement_bytes, &suffix);
                    }
                }
                if single_quoted_regular_filename && raw_suffix.starts_with(b"'\\''") {
                    // Reconcile the closer at point even with skipping disabled.
                    // Semantic overlap may consume the whole apostrophe concatenation,
                    // but the newly inserted common prefix still leaves its quote open.
                    replacement_end = replacement_end.max(edit.end + 1);
                    if replacement_bytes.last() == Some(&b'\'') {
                        replacement_bytes.pop();
                    }
                } else if single_quoted_regular_filename
                    && suffix.first() == Some(&b'\'')
                    && suffix.len() > 1
                {
                    // Keep a standalone closer before adjacent text at point, but leave
                    // the inserted common prefix open so the marker lands before it.
                    if replacement_bytes.last() == Some(&b'\'') {
                        replacement_bytes.pop();
                    }
                } else if response.options.filenames {
                    leave_quoted_filename_suffix_open(
                        &response,
                        edit,
                        completion_type,
                        &suffix,
                        replacement_end.saturating_sub(edit.end),
                        &common_prefix,
                        &mut replacement_bytes,
                    );
                }
                state
                    .buffer
                    .replace_range_bytes(edit.start, replacement_end, &replacement_bytes);
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
                self.display_completions_for_word(state, &response, edit.matching_word())?;
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
        let mut edit = self.completion_edit(state, hooks);
        let response =
            self.completion_response(state, key, CompletionType::Complete, &mut edit, hooks);
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
        let quote_options = RequoteOptions {
            quote_filename: response.options.quote_filename(),
            filename_candidates: response.options.filenames,
            close_quote: true,
            close_ansi_c_quote: true,
            slash_after_quote: false,
        };
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
                quote_options,
                hooks,
            )
        } else {
            let mut joined = self.requote_completion_bytes(
                &prefix,
                &edit,
                CompletionType::Complete,
                quote_options,
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
                    quote_options,
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
        options: RequoteOptions,
        hooks: &mut impl Hooks,
    ) -> Vec<u8> {
        // Quote hooks still replace the full callback word. Present a full
        // semantic value, not the built-in provider's segment-relative candidate.
        let full_value = edit.filename_segment.as_ref().map(|segment| {
            let mut full = segment.word_prefix.clone();
            full.extend_from_slice(value);
            full
        });
        if let Some(quoted) = hooks.quote_completion(QuoteContext {
            value: full_value.as_deref().unwrap_or(value),
            line: &edit.line,
            point: edit.point,
            word_start: edit.start,
            word_end: edit.end,
            word: &edit.word_bytes,
            quote: edit.quote,
            completion_type,
            quote_filename: options.quote_filename,
        }) {
            return quoted;
        }
        let raw_start = edit
            .filename_segment
            .as_ref()
            .map_or(edit.start, |segment| segment.raw_start);
        let raw_word = edit.line.get(raw_start..edit.end).unwrap_or_default();
        let is_unescaped_named_user_path =
            is_named_user_tilde_path(edit.matching_word()) && !raw_word.starts_with(b"\\~");
        let escape_leading_tilde = options.filename_candidates
            && !edit.matching_word().starts_with(b"~/")
            && !is_unescaped_named_user_path
            && matches!(
                completion_type,
                CompletionType::Complete
                    | CompletionType::Filename
                    | CompletionType::MenuComplete
                    | CompletionType::MenuCompleteBackward
                    | CompletionType::InsertCompletions
            );
        // Bash escapes a leading filename `#` at line start or after whitespace.
        let starts_shell_word = edit.start == 0
            || edit
                .line
                .get(edit.start.saturating_sub(1))
                .is_some_and(u8::is_ascii_whitespace);
        let escape_leading_hash =
            options.filename_candidates && starts_shell_word && value.first() == Some(&b'#');
        // Preserve Bash's backslash-LF spelling for an unexpanded `~/` path.
        let single_quote_line_feeds = !(raw_word.starts_with(b"~/") && value.starts_with(b"~/"));
        let replacement = match edit.quote {
            Some('\'') if edit.ansi_c_quote => {
                let slash_after_quote = options.filename_candidates
                    && options.slash_after_quote
                    && value.ends_with(b"/");
                let quoted_value = if slash_after_quote {
                    &value[..value.len() - 1]
                } else {
                    value
                };
                let mut quoted = if options.quote_filename {
                    quote_ansi_c_quoted_filename_bytes(quoted_value)
                } else {
                    // Keep the active ANSI-C syntax while passing candidate bytes
                    // through when filename quoting is disabled.
                    let mut raw = b"$'".to_vec();
                    raw.extend_from_slice(quoted_value);
                    raw.push(b'\'');
                    raw
                };
                if !options.close_ansi_c_quote {
                    quoted.pop();
                }
                if slash_after_quote {
                    quoted.push(b'/');
                }
                quoted
            }
            Some('\'') => {
                let slash_after_quote = options.slash_after_quote && value.ends_with(b"/");
                let quoted_value = if slash_after_quote {
                    &value[..value.len() - 1]
                } else {
                    value
                };
                let mut quoted = quote_single_quoted_bytes(quoted_value);
                if !options.close_quote {
                    quoted.pop();
                }
                if slash_after_quote {
                    quoted.push(b'/');
                }
                quoted
            }
            Some('"') if options.filename_candidates => {
                let slash_after_quote = options.slash_after_quote && value.ends_with(b"/");
                let quoted_value = if slash_after_quote {
                    &value[..value.len() - 1]
                } else {
                    value
                };
                let mut quoted = quote_double_quoted_filename_bytes(quoted_value);
                if !options.close_quote {
                    quoted.pop();
                }
                if slash_after_quote {
                    quoted.push(b'/');
                }
                quoted
            }
            Some('"') => quote_double_quoted_bytes(value),
            _ if options.quote_filename => quote_filename_bytes(
                value,
                escape_leading_tilde,
                escape_leading_hash,
                single_quote_line_feeds,
            ),
            _ => value.to_vec(),
        };
        if edit.filename_segment.is_some() {
            let mut full = edit.line[edit.start..raw_start].to_vec();
            full.extend_from_slice(&replacement);
            full
        } else {
            replacement
        }
    }

    pub(super) fn requote_common_prefix(
        &self,
        response: &CompletionResponse,
        edit: &CompletionEdit,
        completion_type: CompletionType,
        hooks: &mut impl Hooks,
    ) -> Option<Vec<u8>> {
        self.requote_common_prefix_with_prefix(response, edit, completion_type, hooks)
            .map(|(_, replacement)| replacement)
    }

    fn requote_common_prefix_with_prefix(
        &self,
        response: &CompletionResponse,
        edit: &CompletionEdit,
        completion_type: CompletionType,
        hooks: &mut impl Hooks,
    ) -> Option<(Vec<u8>, Vec<u8>)> {
        let prefix = common_prefix_bytes(&response.candidates)?;
        let replacement = self.requote_completion_bytes(
            &prefix,
            edit,
            completion_type,
            RequoteOptions {
                quote_filename: response.options.quote_filename(),
                filename_candidates: response.options.filenames,
                // Built-in completion leaves an initial open quote open after
                // ambiguous common-prefix insertion; reopened segments remain open too.
                close_quote: edit.filename_segment.is_none()
                    && !(edit.builtin_filename_completion && edit.has_initial_open_quote()),
                // Bash also leaves ANSI-C quotes open after ambiguous common-prefix insertion.
                close_ansi_c_quote: false,
                slash_after_quote: false,
            },
            hooks,
        );
        Some((prefix, replacement))
    }

    pub(super) fn completion_replacement_with_directory(
        &self,
        response: &CompletionResponse,
        edit: &CompletionEdit,
        candidate: &CompletionCandidate,
        completion_type: CompletionType,
        hooks: &mut impl Hooks,
        next_byte: Option<u8>,
    ) -> CompletionReplacement {
        let filename_directory = if response.options.filenames {
            filename_directory_completion(
                edit.matching_word(),
                candidate.replacement_bytes(),
                &self.filename_options(),
            )
        } else {
            None
        };
        let append_slash = filename_directory.as_ref().is_some_and(|directory| {
            directory.append_slash
                && !candidate.replacement_bytes().ends_with(b"/")
                && next_byte != Some(b'/')
        });
        let quoted_directory = !edit.ansi_c_quote
            && edit.quote.is_some()
            && response.options.filenames
            && response.options.quote_filename()
            && filename_directory.is_some();
        let ansi_c_directory = edit.ansi_c_quote
            && response.options.filenames
            && response.options.quote_filename()
            && filename_directory.is_some();
        // Bash 5.3 PTY observations leave safe quoted directories open and
        // put a marked slash after the quote when the name needs shell escaping.
        let directory_needs_quote = (quoted_directory || ansi_c_directory)
            && quote_filename_bytes(candidate.replacement_bytes(), true, true, true).as_slice()
                != candidate.replacement_bytes();
        let close_quote = !quoted_directory || directory_needs_quote;
        // Unquoted insertion must leave an existing ANSI-C closer in place,
        // rather than inserting a second one before the cursor.
        let preserve_existing_ansi_c_closer =
            edit.ansi_c_quote && !response.options.quote_filename() && next_byte == Some(b'\'');
        let close_ansi_c_quote =
            (!ansi_c_directory || directory_needs_quote) && !preserve_existing_ansi_c_closer;
        let mut raw = candidate.replacement_bytes().to_vec();
        if append_slash {
            raw.push(b'/');
        }
        let replacement = self.requote_completion_bytes(
            &raw,
            edit,
            completion_type,
            RequoteOptions {
                quote_filename: response.options.quote_filename(),
                filename_candidates: response.options.filenames,
                close_quote,
                close_ansi_c_quote,
                slash_after_quote: append_slash && directory_needs_quote,
            },
            hooks,
        );
        let is_directory =
            filename_directory.is_some() || candidate.replacement_bytes().ends_with(b"/");
        let ansi_c_suffix_bytes =
            if edit.ansi_c_quote && response.options.filenames && response.options.quote_filename()
            {
                ansi_c_quote_suffix_bytes(&replacement)
            } else {
                0
            };
        CompletionReplacement {
            bytes: replacement,
            is_directory,
            ansi_c_suffix_bytes,
        }
    }
}

pub(super) fn ansi_c_quote_suffix_bytes(bytes: &[u8]) -> usize {
    let (quote_index, suffix_bytes) = if bytes.ends_with(b"'/") {
        (bytes.len() - 2, 2)
    } else if bytes.last() == Some(&b'\'') {
        (bytes.len() - 1, 1)
    } else {
        return 0;
    };
    let preceding_backslashes = bytes[..quote_index]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'\\')
        .count();
    if preceding_backslashes % 2 == 0 {
        suffix_bytes
    } else {
        0
    }
}

fn candidate_suffix<'a>(candidate: &'a CompletionCandidate, prefix: &[u8]) -> &'a [u8] {
    candidate
        .replacement_bytes()
        .strip_prefix(prefix)
        .unwrap_or_else(|| candidate.replacement_bytes())
}

pub(super) fn extend_with_trailing_char(replacement: &mut Vec<u8>, options: &CompletionOptions) {
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

#[cfg(test)]
mod tests {
    use super::{double_quoted_filename_suffix, shell_filename_suffix_overlap_len};

    #[test]
    fn double_quoted_filename_suffix_includes_spaces_and_only_the_unescaped_closer() {
        for (raw, expected) in [
            (br#"\" a-" rest"#.as_slice(), br#"\" a-""#.as_slice()),
            (br#"\\" rest"#, br#"\\""#),
            (br#"\\\" a-"tail"#, br#"\\\" a-""#),
            (br#"\q a-" rest"#, br#"\q a-""#),
            (br#"a-" rest"#, br#"a-""#),
            (br#"\" a-"#, br#"\" a-"#),
            (b"a\\", b"a\\"),
        ] {
            assert_eq!(double_quoted_filename_suffix(raw), expected, "{raw:?}");
        }
    }

    #[test]
    fn double_quoted_suffix_overlap_consumes_raw_escape_lengths_not_the_closer() {
        for (completion, raw_suffix, expected) in [
            (b"\"".as_slice(), b"\\\"\"".as_slice(), 2),
            (b"\" a-", b"\\\" a-\"", 5),
            (b"\" a-", b"\\\" a-X\"", 5),
            (b"\" a-", b"\\\" b-\"", 3),
            (b"\" a-", b"\\\"\" a-", 2),
            (b"\\", b"\\\\\"", 2),
            (b"$", b"\\$\"", 2),
            (b"`", b"\\`\"", 2),
            (b"\\q", b"\\q\"", 2),
            (b"q", b"\\q\"", 0),
            (b"\"", b"\"", 0),
            (b"\"", b"\\\"X\"", 2),
            (b" '", b" '\"", 2),
            (b"ab", b"a\\\nb\"", 4),
            (b"b", b"\\\nb\"", 3),
            (b"x", b"\\\nb\"", 0),
        ] {
            assert_eq!(
                shell_filename_suffix_overlap_len(completion, raw_suffix, Some('"')),
                Some(expected),
                "completion={completion:?}, raw_suffix={raw_suffix:?}"
            );
        }
    }

    #[test]
    fn single_quoted_suffix_overlap_consumes_concatenation_not_the_closer() {
        for (completion, raw_suffix, expected) in [
            (b"'".as_slice(), b"'\\'''".as_slice(), 4),
            (b"'tail", b"'\\''tail'", 8),
            (b"'tail", b"'\\''tall'", 6),
            (b"''", b"'\\'''\\'''", 8),
            (b"x", b"'\\'''", 0),
            (b"'", b"'", 0),
            (b"\\", b"\\'", 1),
            (b" a", b" a' rest", 2),
            (b"\"", b"\"'", 1),
        ] {
            assert_eq!(
                shell_filename_suffix_overlap_len(completion, raw_suffix, Some('\'')),
                Some(expected),
                "completion={completion:?}, raw_suffix={raw_suffix:?}"
            );
        }
    }

    #[test]
    fn unquoted_suffix_overlap_keeps_its_existing_escape_and_quote_rules() {
        for (completion, raw_suffix, expected) in [
            (b"*".as_slice(), b"\\*X".as_slice(), Some(2)),
            (b"\\", b"\\\\", Some(2)),
            (b"q", b"\\q", Some(2)),
            (b"a b", b"a b", Some(1)),
            (b"ab", b"a\\\nb", Some(4)),
            (b"\"", b"\"", None),
            (b"'", b"'", None),
        ] {
            assert_eq!(
                shell_filename_suffix_overlap_len(completion, raw_suffix, None),
                expected,
                "completion={completion:?}, raw_suffix={raw_suffix:?}"
            );
        }
    }
}
