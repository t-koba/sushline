use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NamedCommandGroup {
    HistoryNav,
    Movement,
    Kill,
    Editing,
    Completion,
    Vi,
    Misc,
}

fn named_command_group(command: &str) -> NamedCommandGroup {
    match command {
        "alias-expand-line" => NamedCommandGroup::HistoryNav,
        "fetch-history" | "vi-fetch-history" => NamedCommandGroup::HistoryNav,
        "forward-search-history" => NamedCommandGroup::HistoryNav,
        "history-and-alias-expand-line" | "history-expand-line" => NamedCommandGroup::HistoryNav,
        "history-substring-search-backward" => NamedCommandGroup::HistoryNav,
        "history-substring-search-forward" => NamedCommandGroup::HistoryNav,
        "magic-space" => NamedCommandGroup::HistoryNav,
        "non-incremental-forward-search-history"
        | "non-incremental-forward-search-history-again" => NamedCommandGroup::HistoryNav,
        "non-incremental-reverse-search-history"
        | "non-incremental-reverse-search-history-again" => NamedCommandGroup::HistoryNav,
        "operate-and-get-next" => NamedCommandGroup::HistoryNav,
        "backward-byte" => NamedCommandGroup::Movement,
        "forward-byte" => NamedCommandGroup::Movement,
        "next-screen-line" => NamedCommandGroup::Movement,
        "previous-screen-line" => NamedCommandGroup::Movement,
        "shell-backward-word" => NamedCommandGroup::Movement,
        "shell-forward-word" => NamedCommandGroup::Movement,
        "copy-backward-word" => NamedCommandGroup::Kill,
        "copy-forward-word" => NamedCommandGroup::Kill,
        "insert-last-argument" | "yank-last-arg" => NamedCommandGroup::Kill,
        "shell-backward-kill-word" => NamedCommandGroup::Kill,
        "shell-kill-word" => NamedCommandGroup::Kill,
        "unix-filename-rubout" | "vi-unix-word-rubout" => NamedCommandGroup::Kill,
        "yank-nth-arg" | "vi-yank-arg" => NamedCommandGroup::Kill,
        "bracketed-paste-begin" => NamedCommandGroup::Editing,
        "delete-char-or-list" => NamedCommandGroup::Editing,
        "forward-backward-delete-char" => NamedCommandGroup::Editing,
        "insert-comment" => NamedCommandGroup::Editing,
        "overwrite-mode" | "vi-overstrike" => NamedCommandGroup::Editing,
        "shell-expand-line" => NamedCommandGroup::Editing,
        "shell-transpose-words" => NamedCommandGroup::Editing,
        "spell-correct-word" => NamedCommandGroup::Editing,
        "tab-insert" => NamedCommandGroup::Editing,
        "tilde-expand" | "vi-tilde-expand" => NamedCommandGroup::Editing,
        "complete-into-braces" => NamedCommandGroup::Completion,
        "dabbrev-expand" | "dynamic-complete-history" => NamedCommandGroup::Completion,
        "export-completions" => NamedCommandGroup::Completion,
        "character-search" | "character-search-backward" | "vi-char-search" => {
            NamedCommandGroup::Vi
        }
        "edit-and-execute-command" | "vi-edit-and-execute-command" => NamedCommandGroup::Vi,
        "vi-arg-digit" => NamedCommandGroup::Vi,
        "vi-bWord" | "vi-backward-bigword" => NamedCommandGroup::Vi,
        "vi-back-to-indent" | "vi-first-print" => NamedCommandGroup::Vi,
        "vi-backward-word" | "vi-bword" | "vi-prev-word" => NamedCommandGroup::Vi,
        "vi-change-case" => NamedCommandGroup::Vi,
        "vi-change-char" | "vi-replace" => NamedCommandGroup::Vi,
        "vi-change-to" => NamedCommandGroup::Vi,
        "vi-column" => NamedCommandGroup::Vi,
        "vi-delete" => NamedCommandGroup::Vi,
        "vi-delete-to" => NamedCommandGroup::Vi,
        "vi-eWord" | "vi-end-bigword" => NamedCommandGroup::Vi,
        "vi-editing-mode" => NamedCommandGroup::Vi,
        "vi-end-word" | "vi-eword" => NamedCommandGroup::Vi,
        "vi-eof-maybe" => NamedCommandGroup::Vi,
        "vi-fWord" | "vi-forward-bigword" => NamedCommandGroup::Vi,
        "vi-forward-word" | "vi-fword" | "vi-next-word" => NamedCommandGroup::Vi,
        "vi-goto-mark" => NamedCommandGroup::Vi,
        "vi-match" => NamedCommandGroup::Vi,
        "vi-overstrike-delete" | "vi-rubout" => NamedCommandGroup::Vi,
        "vi-put" => NamedCommandGroup::Vi,
        "vi-redo" => NamedCommandGroup::Vi,
        "vi-search" => NamedCommandGroup::Vi,
        "vi-search-again" => NamedCommandGroup::Vi,
        "vi-set-mark" => NamedCommandGroup::Vi,
        "vi-set-register" => NamedCommandGroup::Vi,
        "vi-subst" => NamedCommandGroup::Vi,
        "vi-undo" => NamedCommandGroup::Vi,
        "vi-yank-pop" => NamedCommandGroup::Vi,
        "vi-yank-to" => NamedCommandGroup::Vi,
        _ => NamedCommandGroup::Misc,
    }
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn apply_named_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        if let Some(command) = EditCommand::parse(command) {
            return self.apply_command(state, command, key, hooks);
        }

        if !matches!(
            command,
            "insert-last-argument" | "yank-last-arg" | "yank-nth-arg" | "vi-yank-arg"
        ) {
            state.completion.last_yank_arg = None;
        }

        if let Some(entry) = named_completion_command(command) {
            if entry.record_undo {
                state.record_undo();
            }
            self.complete(state, key, entry.completion_type, hooks)?;
            state.after_non_kill_command();
            return Ok(EditorOutcome::Continue);
        }

        match named_command_group(command) {
            NamedCommandGroup::HistoryNav => {
                self.apply_named_history_nav_command(state, command, key, hooks)
            }
            NamedCommandGroup::Movement => self.apply_named_movement_command(state, command, key),
            NamedCommandGroup::Kill => self.apply_named_kill_command(state, command, key, hooks),
            NamedCommandGroup::Editing => {
                self.apply_named_editing_command(state, command, key, hooks)
            }
            NamedCommandGroup::Completion => {
                self.apply_named_completion_command(state, command, key, hooks)
            }
            NamedCommandGroup::Vi => self.apply_vi_named_command(state, command, key, hooks),
            NamedCommandGroup::Misc => self.apply_named_misc_command(state, command, key, hooks),
        }
    }
}
