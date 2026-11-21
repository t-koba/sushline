use super::*;
use crate::completion::CompletionType;
use crate::hooks::{LineExpansionContext, SpellCorrectionContext};

impl<T> Editor<T>
where
    T: TerminalIo,
{
    /// GNU Readline signals end-of-file only for the terminal EOF character
    /// (Ctrl-D by default), even though other keys such as `\e[3~` may be
    /// bound to `delete-char`. Those other keys only ring on an empty line.
    /// The EOF byte comes from the terminal's `end-of-file` special binding,
    /// falling back to Ctrl-D when no terminal byte is available.
    fn is_eof_key(&self, key: &[u8]) -> bool {
        let [byte] = key else {
            return false;
        };
        self.terminal
            .tty_special_bindings()
            .into_iter()
            .find_map(|(candidate, name)| (name == "end-of-file").then_some(candidate))
            .unwrap_or(0x04)
            == *byte
    }

    /// Overwrite raw bytes one at a time when byte motion is in effect.
    /// Returns false when the editor is not bytewise so callers keep their
    /// whole-character overwrite path.
    pub(crate) fn overwrite_bytes_bytewise(&self, state: &mut EditorState, bytes: &[u8]) -> bool {
        if !self.char_motion_is_bytewise() {
            return false;
        }
        for &byte in bytes {
            let point = state.buffer.point();
            if point < state.buffer.len_chars() {
                state.buffer.replace_range_bytes(point, point + 1, &[byte]);
            } else {
                state.buffer.insert_bytes(&[byte]);
            }
        }
        true
    }

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
                    if arg == 0 {
                        if state.buffer.point() == 0 {
                            self.ding()?;
                        }
                        state.after_non_kill_command();
                    } else {
                        self.delete_chars_with_numeric_argument(state, arg, false)?;
                    }
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
                    |buffer| buffer.capitalize_word(word_breaks.as_deref()),
                    |buffer, count| {
                        buffer.capitalize_previous_words(count, word_breaks.as_deref());
                    },
                );
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::DeleteChar => {
                if state.buffer.is_empty() && state.numeric_arg.is_none() && self.is_eof_key(key) {
                    return Ok(EditorOutcome::Eof);
                }
                state.record_undo();
                if let Some(arg) = state.take_numeric_arg() {
                    if arg == 0 {
                        if state.buffer.point() >= state.buffer.len_chars() {
                            self.ding()?;
                        }
                        state.after_non_kill_command();
                    } else {
                        self.delete_chars_with_numeric_argument(state, arg, true)?;
                    }
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
                    |buffer| buffer.downcase_word(word_breaks.as_deref()),
                    |buffer, count| {
                        buffer.downcase_previous_words(count, word_breaks.as_deref());
                    },
                );
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::QuotedInsert => {
                // GNU reads |count| consecutive raw bytes for a negative
                // quoted-insert count, inserting each one once. Positive
                // counts repeat a single quoted byte.
                if let Some(arg) = state.numeric_arg.filter(|arg| *arg < 0) {
                    state.input.quoted_insert_remaining = arg.unsigned_abs();
                    state.take_numeric_arg();
                }
                state.input.quoted_insert = true;
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::RevertLine => {
                if self.keymap.current() == KeyMapName::ViCommand {
                    // GNU binds U to revert-line; in vi mode it undoes the
                    // whole undo list rather than restoring the line as it
                    // was when the read began, and leaves point at zero.
                    let mut undone = false;
                    let vi_active = self.effective_editing_mode_is_vi();
                    while state.undo(vi_active) {
                        undone = true;
                    }
                    if undone {
                        state.buffer.set_point(0);
                        state.mark = Some(0);
                    } else {
                        // GNU rings on an empty undo list without moving
                        // point or mark.
                        self.ding()?;
                    }
                } else {
                    state.record_undo();
                    if state.buffer.as_bytes() == state.original_line.as_slice() {
                        self.ding()?;
                    } else {
                        state.buffer = LineBuffer::from_bytes(state.original_line.clone());
                        state.history_modified = false;
                    }
                }
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::SelfInsert => {
                state.display.pending_text = true;
                let literal_tail = state.numeric_arg_tail != NumericArgLiteralTail::None;
                let raw = state.take_numeric_arg().unwrap_or(1);
                let raw = if literal_tail && raw == 0 {
                    state.numeric_arg_tail = NumericArgLiteralTail::Literal;
                    1
                } else {
                    raw
                };
                // Sushline treats a negative repeat count as zero inserts;
                // the pinned negative self-insert fixture accepts the empty
                // line. Only sign-only and zero-then-negative quoted-insert
                // forms matched GNU in paired PTY runs, so no general
                // negative-count rule is inferred (see COMPATIBILITY.md).
                let count: u32 = if raw < 0 { 0 } else { raw as u32 };
                state.record_insert_undo(
                    count == 1 && key.len() == 1,
                    self.keymap.current() == KeyMapName::ViInsert,
                );
                if key.iter().any(|byte| byte.is_ascii_control()) {
                    for _ in 0..count {
                        // Bytewise overwrite applies to printable keys; GNU
                        // still inserts control bytes into the line.
                        if state.overwrite_mode
                            && state.buffer.point() < state.buffer.len_chars()
                            && !self.char_motion_is_bytewise()
                        {
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
                                    let mut encoded = [0u8; 4];
                                    let bytes = ch.encode_utf8(&mut encoded).as_bytes();
                                    if !self.overwrite_bytes_bytewise(state, bytes) {
                                        state.buffer.set_char_at_point(ch);
                                        state.buffer.move_forward();
                                    }
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
                        if state.overwrite_mode && self.char_motion_is_bytewise() {
                            self.overwrite_bytes_bytewise(state, key);
                        } else {
                            state.buffer.insert_bytes(key);
                        }
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
                    |buffer| buffer.upcase_word(word_breaks.as_deref()),
                    |buffer, count| {
                        buffer.upcase_previous_words(count, word_breaks.as_deref());
                    },
                );
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            EditCommand::Eof => {
                if state.buffer.is_empty() && self.is_eof_key(key) {
                    Ok(EditorOutcome::Eof)
                } else {
                    // GNU's `end-of-file` command rings and edits nothing
                    // unless the EOF byte is invoked on an empty line.
                    self.ding()?;
                    state.after_non_kill_command();
                    Ok(EditorOutcome::Continue)
                }
            }
            EditCommand::Undo => {
                let count = state.take_numeric_arg().unwrap_or(1);
                let vi_active = self.effective_editing_mode_is_vi();
                for _ in 0..count {
                    if !state.undo(vi_active) {
                        self.ding()?;
                        break;
                    }
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
                state.record_insert_undo(count == 1, self.keymap.current() == KeyMapName::ViInsert);
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
    expand_tilde_word_with_home(
        word,
        std::env::var_os("HOME").as_deref(),
        crate::home::passwd_home_dir,
    )
}

fn expand_tilde_word_with_home(
    word: &[u8],
    home: Option<&std::ffi::OsStr>,
    lookup_home: impl FnOnce(Option<&[u8]>) -> Option<std::ffi::OsString>,
) -> Option<Vec<u8>> {
    if word == b"~" {
        return crate::home::resolve(home, None, lookup_home)
            .and_then(|home| os_string_to_bytes(home.as_os_str()));
    }
    if let Some(rest) = word.strip_prefix(b"~/") {
        let mut home = crate::home::resolve(home, None, lookup_home)
            .and_then(|home| os_string_to_bytes(home.as_os_str()))?;
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
    let mut home = crate::home::resolve(home, Some(user.as_bytes()), lookup_home)
        .and_then(|home| os_string_to_bytes(home.as_os_str()))?;
    if slash.is_some() {
        home.push(b'/');
        home.extend_from_slice(suffix);
    }
    Some(home)
}

fn pwd_bytes() -> Option<Vec<u8>> {
    let pwd = std::env::var_os("PWD");
    pwd_bytes_with_fallback(pwd.as_deref(), || {
        std::env::current_dir()
            .ok()
            .map(|path| path.into_os_string())
    })
}

fn pwd_bytes_with_fallback(
    pwd: Option<&std::ffi::OsStr>,
    current_directory: impl FnOnce() -> Option<std::ffi::OsString>,
) -> Option<Vec<u8>> {
    match pwd {
        Some(pwd) => os_string_to_bytes(pwd),
        None => current_directory().and_then(|path| os_string_to_bytes(path.as_os_str())),
    }
}

fn oldpwd_bytes() -> Option<Vec<u8>> {
    std::env::var_os("OLDPWD").and_then(|oldpwd| os_string_to_bytes(oldpwd.as_os_str()))
}

#[cfg(unix)]
fn os_string_to_bytes(value: &std::ffi::OsStr) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    Some(value.as_bytes().to_vec())
}

#[cfg(not(unix))]
fn os_string_to_bytes(value: &std::ffi::OsStr) -> Option<Vec<u8>> {
    value.to_str().map(|value| value.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn unrepresentable_pwd_does_not_fall_back_to_current_directory() {
        use std::os::windows::ffi::OsStringExt;

        let pwd = std::ffi::OsString::from_wide(&[0xd800]);
        let fallback_called = std::cell::Cell::new(false);
        let expanded = pwd_bytes_with_fallback(Some(pwd.as_os_str()), || {
            fallback_called.set(true);
            Some(std::ffi::OsString::from("C:/physical-current-directory"))
        });

        assert_eq!(expanded, None);
        assert!(!fallback_called.get());
    }

    #[test]
    fn missing_pwd_uses_current_directory_fallback() {
        let expanded = pwd_bytes_with_fallback(None, || {
            Some(std::ffi::OsString::from("/physical-current-directory"))
        });

        assert_eq!(expanded, Some(b"/physical-current-directory".to_vec()));
    }

    #[cfg(windows)]
    #[test]
    fn unpaired_surrogate_home_is_not_lossily_expanded() {
        use std::os::windows::ffi::OsStringExt;

        let home = std::ffi::OsString::from_wide(&[0xd800]);
        assert_eq!(
            expand_tilde_word_with_home(b"~", Some(home.as_os_str()), |_| None),
            None
        );
    }

    #[test]
    fn unicode_home_path_expands_to_utf8_bytes() {
        let home = std::ffi::OsString::from("/home/naïve");
        assert_eq!(
            expand_tilde_word_with_home(b"~", Some(home.as_os_str()), |_| None),
            Some("/home/naïve".as_bytes().to_vec())
        );
    }

    #[test]
    fn tilde_word_uses_current_uid_when_home_is_unset() {
        for (word, expected) in [
            (b"~".as_slice(), b"/synthetic-home/".as_slice()),
            (b"~/", b"/synthetic-home//"),
            (b"~/child/", b"/synthetic-home//child/"),
        ] {
            assert_eq!(
                expand_tilde_word_with_home(word, None, |user| {
                    assert_eq!(user, None);
                    Some(std::ffi::OsString::from("/synthetic-home/"))
                }),
                Some(expected.to_vec()),
            );
            assert_eq!(expand_tilde_word_with_home(word, None, |_| None), None);
        }
    }

    #[test]
    fn tilde_word_named_lookup_preserves_suffix_and_trailing_slash() {
        for (word, expected) in [
            (b"~synthetic-user".as_slice(), b"/nss-home".as_slice()),
            (b"~synthetic-user/", b"/nss-home/"),
            (b"~synthetic-user/child/", b"/nss-home/child/"),
        ] {
            assert_eq!(
                expand_tilde_word_with_home(word, Some(std::ffi::OsStr::new("/ignored")), |user| {
                    assert_eq!(user, Some(b"synthetic-user".as_slice()));
                    Some(std::ffi::OsString::from("/nss-home"))
                }),
                Some(expected.to_vec()),
            );
            assert_eq!(expand_tilde_word_with_home(word, None, |_| None), None);
        }
    }
}
