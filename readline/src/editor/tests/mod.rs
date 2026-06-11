use super::*;
use crate::completion::builtin::{complete_commands_with_hooks_bytes, glob_complete};
use crate::completion::display::{
    color_completion_prefix, common_prefix_bytes, format_completion_items_with_trailing,
};
use crate::completion::filename::{
    FilenameOptions, complete_directories_bytes, complete_filenames_bytes, glob_match,
    ls_color_code_from_spec,
};
use crate::completion::{CompletionContext, CompletionRequest, CompletionResponse, CompletionType};
use crate::terminal::{TerminalEvent, TerminalSize};
use ::history::expansion::{
    HistoryChars, HistoryExpansion, HistoryExpansionError, HistoryExpansionPolicy, expand_history,
};
use std::collections::VecDeque;

mod completion;
mod history;
mod lifecycle;
mod signals;

const NAMED_READLINE_COMMAND_DISPATCH: &[&str] = &[
    "alias-expand-line",
    "arrow-key-prefix",
    "backward-byte",
    "bash-vi-complete",
    "bracketed-paste-begin",
    "character-search",
    "character-search-backward",
    "clear-display",
    "complete-command",
    "complete-filename",
    "complete-hostname",
    "complete-into-braces",
    "complete-username",
    "complete-variable",
    "copy-backward-word",
    "copy-forward-word",
    "dabbrev-expand",
    "delete-char-or-list",
    "display-shell-version",
    "do-lowercase-version",
    "dump-functions",
    "dump-macros",
    "dump-variables",
    "dynamic-complete-history",
    "edit-and-execute-command",
    "emacs-editing-mode",
    "execute-named-command",
    "export-completions",
    "fetch-history",
    "forward-backward-delete-char",
    "forward-byte",
    "forward-search-history",
    "glob-complete-word",
    "glob-expand-word",
    "glob-list-expansions",
    "history-and-alias-expand-line",
    "history-expand-line",
    "history-substring-search-backward",
    "history-substring-search-forward",
    "insert-comment",
    "insert-completions",
    "insert-last-argument",
    "magic-space",
    "menu-complete",
    "menu-complete-backward",
    "next-screen-line",
    "non-incremental-forward-search-history",
    "non-incremental-forward-search-history-again",
    "non-incremental-reverse-search-history",
    "non-incremental-reverse-search-history-again",
    "old-menu-complete",
    "operate-and-get-next",
    "overwrite-mode",
    "possible-command-completions",
    "possible-completions",
    "possible-filename-completions",
    "possible-hostname-completions",
    "possible-username-completions",
    "possible-variable-completions",
    "previous-screen-line",
    "re-read-init-file",
    "redraw-current-line",
    "shell-backward-kill-word",
    "shell-backward-word",
    "shell-expand-line",
    "shell-forward-word",
    "shell-kill-word",
    "shell-transpose-words",
    "skip-csi-sequence",
    "spell-correct-word",
    "tab-insert",
    "tilde-expand",
    "tty-status",
    "unix-filename-rubout",
    "vi-arg-digit",
    "vi-bWord",
    "vi-back-to-indent",
    "vi-backward-bigword",
    "vi-backward-word",
    "vi-bword",
    "vi-change-case",
    "vi-change-char",
    "vi-change-to",
    "vi-char-search",
    "vi-column",
    "vi-complete",
    "vi-delete",
    "vi-delete-to",
    "vi-eWord",
    "vi-edit-and-execute-command",
    "vi-editing-mode",
    "vi-end-bigword",
    "vi-end-word",
    "vi-eof-maybe",
    "vi-eword",
    "vi-fWord",
    "vi-fetch-history",
    "vi-first-print",
    "vi-forward-bigword",
    "vi-forward-word",
    "vi-fword",
    "vi-goto-mark",
    "vi-match",
    "vi-next-word",
    "vi-overstrike",
    "vi-overstrike-delete",
    "vi-prev-word",
    "vi-put",
    "vi-redo",
    "vi-replace",
    "vi-rubout",
    "vi-search",
    "vi-search-again",
    "vi-set-mark",
    "vi-subst",
    "vi-tilde-expand",
    "vi-undo",
    "vi-unix-word-rubout",
    "vi-yank-arg",
    "vi-yank-pop",
    "vi-yank-to",
    "yank-last-arg",
    "yank-nth-arg",
];

fn expand_history_for_test(line: &str, history: &History) -> String {
    String::from_utf8(
        expand_history(
            line.as_bytes(),
            history,
            HistoryChars::parse("!^#"),
            &HistoryExpansionPolicy::default(),
            |_| false,
        )
        .unwrap(),
    )
    .unwrap()
}

fn expand_history_with_chars_for_test(
    line: &str,
    history: &History,
    histchars: HistoryChars,
) -> Result<String, HistoryExpansionError> {
    expand_history(
        line.as_bytes(),
        history,
        histchars,
        &HistoryExpansionPolicy::default(),
        |_| false,
    )
    .map(|bytes| String::from_utf8(bytes).unwrap())
}

struct TestHistoryHook;

impl Hooks for TestHistoryHook {
    fn expand_history(
        &mut self,
        context: crate::hooks::HistoryExpansionContext<'_>,
    ) -> Result<HistoryExpansion, String> {
        expand_history(
            context.line,
            context.history,
            context.histchars,
            context.policy,
            |_| false,
        )
        .map(|line| HistoryExpansion {
            line,
            print_only: false,
        })
        .map_err(|err| err.message())
    }
}

#[derive(Default)]
struct MemoryTerminal {
    events: VecDeque<TerminalEvent>,
    out: String,
    columns: u16,
    tty_special: Vec<(u8, &'static str)>,
    meta_enabled: Vec<bool>,
    keypad_enabled: Vec<bool>,
    moved_columns: Vec<u16>,
    moved_up: Vec<u16>,
    cleared_screen: usize,
}

impl MemoryTerminal {
    fn with_events(events: Vec<TerminalEvent>) -> Self {
        Self {
            events: events.into(),
            out: String::new(),
            columns: 80,
            tty_special: Vec::new(),
            meta_enabled: Vec::new(),
            keypad_enabled: Vec::new(),
            moved_columns: Vec::new(),
            moved_up: Vec::new(),
            cleared_screen: 0,
        }
    }
}

impl TerminalIo for MemoryTerminal {
    fn enter_raw_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn restore_mode(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn read_event(&mut self, _: Option<Duration>) -> io::Result<TerminalEvent> {
        Ok(self.events.pop_front().unwrap_or(TerminalEvent::Timeout))
    }

    fn write(&mut self, text: &str) -> io::Result<()> {
        self.out.push_str(text);
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.push_str(&String::from_utf8_lossy(bytes));
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn size(&self) -> io::Result<TerminalSize> {
        Ok(TerminalSize {
            columns: self.columns,
            rows: 24,
        })
    }

    fn clear_after_cursor(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn clear_to_screen_end(&mut self) -> io::Result<()> {
        self.cleared_screen += 1;
        Ok(())
    }

    fn clear_display(&mut self) -> io::Result<()> {
        self.cleared_screen += 1;
        self.out.push_str("\r\x1b[J");
        Ok(())
    }

    fn move_to_column(&mut self, column: u16) -> io::Result<()> {
        self.moved_columns.push(column);
        Ok(())
    }

    fn set_meta_key_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.meta_enabled.push(enabled);
        Ok(())
    }

    fn set_application_keypad_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.keypad_enabled.push(enabled);
        Ok(())
    }

    fn move_up(&mut self, rows: u16) -> io::Result<()> {
        self.moved_up.push(rows);
        Ok(())
    }

    fn tty_special_bindings(&self) -> Vec<(u8, &'static str)> {
        self.tty_special.clone()
    }
}
