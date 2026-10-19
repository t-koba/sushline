use super::*;
use crate::completion::CompletionType;
use crate::hooks::{LineExpansionContext, SpellCorrectionContext};

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_editing_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            EditCommand::BackwardDeleteChar => {
                state.record_undo();
                if let Some(arg) = state.take_numeric_arg() {
                    self.delete_chars_with_numeric_argument(state, arg, false)?;
                } else if state.overwrite_mode {
                    state.buffer.backward_replace_char_with_space();
                    state.after_non_kill_command();
                } else if !state.buffer.backward_delete_char() {
                    self.ding()?;
                    state.after_non_kill_command();
                } else {
                    state.after_non_kill_command();
                }
                Ok(EditorOutcome::Continue)
            }
            EditCommand::CapitalizeWord => {
                state.record_undo();
                let word_breaks = self.editing_word_breaks(hooks);
                repeat_case_word(
                    state,
                    |state| state.buffer.capitalize_word(word_breaks.as_deref()),
                    |state| {
                        state
                            .buffer
                            .capitalize_previous_word_preserving_point(word_breaks.as_deref())
                    },
                );
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::DeleteChar => {
                if state.buffer.is_empty() && state.numeric_arg.is_none() {
                    return Ok(EditorOutcome::Eof);
                }
                state.record_undo();
                if let Some(arg) = state.take_numeric_arg() {
                    self.delete_chars_with_numeric_argument(state, arg, true)?;
                } else if !state.buffer.delete_char() {
                    self.ding()?;
                    state.after_non_kill_command();
                } else {
                    state.after_non_kill_command();
                }
                Ok(EditorOutcome::Continue)
            }
            EditCommand::DeleteHorizontalSpace => {
                state.record_undo();
                state.buffer.delete_horizontal_space();
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::DowncaseWord => {
                state.record_undo();
                let word_breaks = self.editing_word_breaks(hooks);
                repeat_case_word(
                    state,
                    |state| state.buffer.downcase_word(word_breaks.as_deref()),
                    |state| {
                        state
                            .buffer
                            .downcase_previous_word_preserving_point(word_breaks.as_deref())
                    },
                );
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::QuotedInsert => {
                state.input.quoted_insert = true;
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::RevertLine => {
                state.record_undo();
                if state.buffer.as_bytes() == state.original_line.as_slice() {
                    self.ding()?;
                } else {
                    state.buffer = LineBuffer::from_bytes(state.original_line.clone());
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::SelfInsert => {
                let raw = state.take_numeric_arg().unwrap_or(1);
                // GNU inserts nothing for a negative repeat count (tested
                // M-- M-1 M-0); only non-negative counts repeat the insert.
                let count: u32 = if raw < 0 { 0 } else { raw as u32 };
                state.record_insert_undo(count == 1 && key.len() == 1);
                if key.iter().any(|byte| byte.is_ascii_control()) {
                    for _ in 0..count {
                        if state.overwrite_mode && state.buffer.point() < state.buffer.len_chars() {
                            let point = state.buffer.point();
                            state.buffer.replace_char_at_point_bytes(key);
                            state.buffer.set_point(point + key.len());
                        } else {
                            state.buffer.insert_bytes(key);
                        }
                    }
                    state.record_vi_insert_bytes(key);
                    state.after_self_insert();
                    return Ok(EditorOutcome::Continue);
                }
                if let Ok(text) = std::str::from_utf8(key) {
                    for _ in 0..count {
                        for ch in text.chars() {
                            if !ch.is_control() {
                                if state.overwrite_mode
                                    && state.buffer.point() < state.buffer.len_chars()
                                {
                                    state.buffer.set_char_at_point(ch);
                                    state.buffer.move_forward();
                                } else {
                                    state.buffer.insert_char(ch);
                                }
                            }
                        }
                    }
                    state.record_vi_insert_bytes(key);
                    if self.variable_is_on("blink-matching-paren") {
                        self.blink_matching_paren(state, text)?;
                    }
                } else {
                    for _ in 0..count {
                        state.buffer.insert_bytes(key);
                    }
                    state.record_vi_insert_bytes(key);
                }
                state.after_self_insert();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::TransposeChars => {
                let count = state.take_numeric_arg().unwrap_or(1);
                if count != 0 {
                    state.record_undo();
                    let transposed = state.buffer.transpose_chars_by_count(count);
                    if count > 0 && !transposed {
                        self.ding()?;
                    }
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::TransposeWords => {
                state.record_undo();
                let word_breaks = self.editing_word_breaks(hooks);
                let count = state.take_numeric_arg().unwrap_or(1);
                // The count selects one target pair rather than repeating
                // adjacent word swaps; zero remains a silent no-op.
                let transposed = state
                    .buffer
                    .transpose_words_by_count(count, word_breaks.as_deref());
                if count != 0 && !transposed {
                    self.ding()?;
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::UpcaseWord => {
                state.record_undo();
                let word_breaks = self.editing_word_breaks(hooks);
                repeat_case_word(
                    state,
                    |state| state.buffer.upcase_word(word_breaks.as_deref()),
                    |state| {
                        state
                            .buffer
                            .upcase_previous_word_preserving_point(word_breaks.as_deref())
                    },
                );
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::Eof => {
                if state.buffer.is_empty() {
                    Ok(EditorOutcome::Eof)
                } else {
                    state.record_undo();
                    state.buffer.delete_char();
                    state.after_non_kill_command();
                    Ok(EditorOutcome::Continue)
                }
            }
            EditCommand::Undo => {
                if !state.undo() {
                    self.ding()?;
                }
                Ok(EditorOutcome::Continue)
            }
            _ => unreachable!("command group mismatch"),
        }
    }

    // Explicitly counted deletes are directional kills; uncounted deletes are not.
    fn delete_chars_with_numeric_argument(
        &mut self,
        state: &mut EditorState,
        arg: i32,
        nonnegative_is_forward: bool,
    ) -> Result<(), ReadlineError> {
        let forward = (arg >= 0) == nonnegative_is_forward;
        let mut killed = Vec::new();
        for _ in 0..arg.unsigned_abs() {
            let part = if forward {
                state.buffer.delete_char_bytes()
            } else {
                state.buffer.backward_delete_char_bytes()
            };
            let Some(part) = part else {
                break;
            };
            if forward {
                killed.extend(part);
            } else {
                killed.splice(0..0, part);
            }
        }
        if killed.is_empty() {
            self.ding()?;
        }
        let direction = if forward {
            KillDirection::Forward
        } else {
            KillDirection::Backward
        };
        state.push_kill(killed, direction, self.kill_append_allowed());
        Ok(())
    }
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_named_editing_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "bracketed-paste-begin" => {
                // The variable gates terminal DEC mode only (editor.rs);
                // injected begin sequences paste even when off, matching
                // the patch 0 baseline oracle (framed controls stay
                // literal, unterminated begin holds the line).
                state.paste.bracketed_paste = true;
                state.paste.bracketed_paste_start = Some(state.buffer.point());
                state.paste.bracketed_paste_pending.clear();
            }
            "delete-char-or-list" => {
                let mut counted_kill = false;
                if state.buffer.point() >= state.buffer.len_chars() {
                    self.complete(
                        state,
                        key,
                        CompletionType::PossibleCompletions,
                        false,
                        hooks,
                    )?;
                } else {
                    state.record_undo();
                    if let Some(arg) = state.take_numeric_arg() {
                        if arg != 0 {
                            self.delete_chars_with_numeric_argument(state, arg, true)?;
                            counted_kill = true;
                        }
                    } else {
                        state.buffer.delete_char();
                    }
                }
                if !counted_kill {
                    state.after_non_kill_command();
                }
            }
            "forward-backward-delete-char" => {
                state.record_undo();
                let numeric_arg = state.take_numeric_arg();
                let arg = numeric_arg.unwrap_or(1);
                let at_end = state.buffer.point() >= state.buffer.len_chars();
                // Normal counts delete forward except at EOL; negative counts reverse it.
                let forward = if arg < 0 { at_end } else { !at_end };
                let mut counted_kill = false;
                if let Some(arg) = numeric_arg {
                    let can_delete = if forward {
                        state.buffer.point() < state.buffer.len_chars()
                    } else {
                        state.buffer.point() > 0
                    };
                    if arg != 0 && can_delete {
                        // Using byte deletion preserves this command's measured
                        // overwrite-mode behavior while saving counted text for yank.
                        self.delete_chars_with_numeric_argument(state, arg, !at_end)?;
                        counted_kill = true;
                    }
                } else if forward {
                    state.buffer.delete_char();
                } else {
                    state.buffer.backward_delete_char();
                }
                if !counted_kill {
                    state.after_non_kill_command();
                }
            }
            "insert-comment" => {
                state.record_undo();
                let comment = self
                    .variables
                    .get("comment-begin")
                    .map(String::as_str)
                    .unwrap_or("#");
                if state.take_numeric_arg().is_some() {
                    state.buffer.toggle_comment(comment);
                } else {
                    state.buffer.insert_comment(comment);
                }
                return Ok(EditorOutcome::Accepted(state.buffer.as_bytes().to_vec()));
            }
            "overwrite-mode" => {
                if let Some(arg) = state.take_numeric_arg() {
                    state.overwrite_mode = arg > 0;
                } else {
                    state.overwrite_mode = !state.overwrite_mode;
                }
                state.after_non_kill_command();
            }
            "vi-overstrike" => {
                state.overwrite_mode = true;
                self.keymap.set_current(KeyMapName::ViInsert);
                state.begin_vi_insert_change(key);
                state.after_non_kill_command();
            }
            "shell-expand-line" => {
                state.record_undo();
                if let Some(edit) = hooks.expand_line(LineExpansionContext {
                    line: state.buffer.as_bytes(),
                    point: state.buffer.byte_point(),
                }) {
                    if let Some(line) = edit.line {
                        state.buffer = LineBuffer::from_bytes(line);
                    }
                    if let Some(point) = edit.point {
                        state.buffer.set_point(point);
                    }
                    if let Some(mark) = edit.mark {
                        state.mark = mark;
                    }
                    state.after_non_kill_command();
                } else {
                    self.ding()?;
                }
            }
            "shell-transpose-words" => {
                state.record_undo();
                let count = state.take_numeric_arg().unwrap_or(1);
                shell_words::transpose_shell_words_by_count(&mut state.buffer, hooks, count);
                state.after_non_kill_command();
            }
            "spell-correct-word" => {
                state.record_undo();
                let word_breaks = self.completion_word_breaks(hooks);
                let word = state.buffer.word_before_point(Some(&word_breaks));
                let end = state.buffer.point();
                let start = end.saturating_sub(word.len());
                if let Some(corrected) = hooks.spell_correct(SpellCorrectionContext {
                    line: state.buffer.as_bytes(),
                    point: state.buffer.byte_point(),
                    word_start: start,
                    word_end: end,
                    word: &word,
                }) {
                    state.buffer.replace_range_bytes(start, end, &corrected);
                } else {
                    self.ding()?;
                }
                state.after_non_kill_command();
            }
            "tab-insert" => {
                let raw = state.take_numeric_arg().unwrap_or(1);
                // Like `self-insert`, negative repeat counts insert nothing.
                let count: u32 = if raw < 0 { 0 } else { raw as u32 };
                state.record_insert_undo(count == 1);
                for _ in 0..count {
                    state.buffer.insert_char('\t');
                }
                state.after_self_insert();
            }
            "tilde-expand" | "vi-tilde-expand" => {
                state.record_undo();
                expand_tilde_at_point(&mut state.buffer);
                state.after_non_kill_command();
            }
            _ => unreachable!("named command group mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }
}

fn expand_tilde_at_point(buffer: &mut LineBuffer) {
    let bytes = buffer.as_bytes();
    let point = buffer.byte_point().min(bytes.len());
    let start = bytes[..point]
        .iter()
        .rposition(|byte| byte.is_ascii_whitespace())
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let end = bytes[point..]
        .iter()
        .position(|byte| byte.is_ascii_whitespace())
        .map(|idx| point + idx)
        .unwrap_or(bytes.len());
    if let Some(expanded) = expand_tilde_word(&bytes[start..end]) {
        buffer.replace_range_bytes(start, end, &expanded);
    }
}

fn expand_tilde_word(word: &[u8]) -> Option<Vec<u8>> {
    if word == b"~" {
        return std::env::var_os("HOME").map(|home| os_string_to_bytes(home.as_os_str()));
    }
    if let Some(rest) = word.strip_prefix(b"~/") {
        let mut home = std::env::var_os("HOME").map(|home| os_string_to_bytes(home.as_os_str()))?;
        home.push(b'/');
        home.extend_from_slice(rest);
        return Some(home);
    }
    if word == b"~+" {
        return pwd_bytes();
    }
    if let Some(rest) = word.strip_prefix(b"~+/") {
        let mut pwd = pwd_bytes()?;
        pwd.push(b'/');
        pwd.extend_from_slice(rest);
        return Some(pwd);
    }
    if word == b"~-" {
        return oldpwd_bytes();
    }
    if let Some(rest) = word.strip_prefix(b"~-/") {
        let mut oldpwd = oldpwd_bytes()?;
        oldpwd.push(b'/');
        oldpwd.extend_from_slice(rest);
        return Some(oldpwd);
    }
    let rest = word.strip_prefix(b"~")?;
    let slash = rest.iter().position(|byte| *byte == b'/');
    let (user, suffix) = if let Some(pos) = slash {
        (&rest[..pos], &rest[pos + 1..])
    } else {
        (rest, &b""[..])
    };
    let user = std::str::from_utf8(user).ok()?;
    if user.is_empty() {
        return None;
    }
    let mut home = user_home_dir(user)?;
    if slash.is_some() {
        home.push(b'/');
        home.extend_from_slice(suffix);
    }
    Some(home)
}

fn pwd_bytes() -> Option<Vec<u8>> {
    std::env::var_os("PWD")
        .map(|pwd| os_string_to_bytes(pwd.as_os_str()))
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|pwd| os_string_to_bytes(pwd.as_os_str()))
        })
}

fn oldpwd_bytes() -> Option<Vec<u8>> {
    std::env::var_os("OLDPWD").map(|oldpwd| os_string_to_bytes(oldpwd.as_os_str()))
}

fn user_home_dir(user: &str) -> Option<Vec<u8>> {
    if let Ok(passwd) = std::fs::read("/etc/passwd") {
        for line in passwd.split(|byte| *byte == b'\n') {
            let mut fields = line.split(|byte| *byte == b':');
            if fields.next() == Some(user.as_bytes()) {
                return fields.nth(4).map(|field| field.to_vec());
            }
        }
    }
    let output = std::process::Command::new("getent")
        .arg("passwd")
        .arg(user)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .next()
        .and_then(|line| line.split(|byte| *byte == b':').nth(5))
        .map(|field| field.to_vec())
}

#[cfg(unix)]
fn os_string_to_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().to_vec()
}

#[cfg(not(unix))]
fn os_string_to_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.to_string_lossy().as_bytes().to_vec()
}
