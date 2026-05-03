use super::*;
use crate::completion::CompletionType;
use history::expansion::{HistoryExpansionPolicy, command_words};

#[derive(Clone, Copy)]
pub(super) struct CompletionCommand {
    pub(super) completion_type: CompletionType,
    pub(super) record_undo: bool,
}

const NAMED_COMPLETION_COMMANDS: &[(&str, CompletionCommand)] = &[
    (
        "bash-vi-complete",
        CompletionCommand {
            completion_type: CompletionType::Command,
            record_undo: true,
        },
    ),
    (
        "complete-command",
        CompletionCommand {
            completion_type: CompletionType::Command,
            record_undo: true,
        },
    ),
    (
        "complete-filename",
        CompletionCommand {
            completion_type: CompletionType::Filename,
            record_undo: true,
        },
    ),
    (
        "complete-hostname",
        CompletionCommand {
            completion_type: CompletionType::Hostname,
            record_undo: true,
        },
    ),
    (
        "complete-username",
        CompletionCommand {
            completion_type: CompletionType::Username,
            record_undo: true,
        },
    ),
    (
        "complete-variable",
        CompletionCommand {
            completion_type: CompletionType::Variable,
            record_undo: true,
        },
    ),
    (
        "glob-complete-word",
        CompletionCommand {
            completion_type: CompletionType::GlobCompleteWord,
            record_undo: true,
        },
    ),
    (
        "glob-expand-word",
        CompletionCommand {
            completion_type: CompletionType::GlobExpandWord,
            record_undo: true,
        },
    ),
    (
        "glob-list-expansions",
        CompletionCommand {
            completion_type: CompletionType::GlobListExpansions,
            record_undo: false,
        },
    ),
    (
        "insert-completions",
        CompletionCommand {
            completion_type: CompletionType::InsertCompletions,
            record_undo: true,
        },
    ),
    (
        "menu-complete",
        CompletionCommand {
            completion_type: CompletionType::MenuComplete,
            record_undo: true,
        },
    ),
    (
        "old-menu-complete",
        CompletionCommand {
            completion_type: CompletionType::MenuComplete,
            record_undo: true,
        },
    ),
    (
        "menu-complete-backward",
        CompletionCommand {
            completion_type: CompletionType::MenuCompleteBackward,
            record_undo: true,
        },
    ),
    (
        "possible-command-completions",
        CompletionCommand {
            completion_type: CompletionType::PossibleCommandCompletions,
            record_undo: false,
        },
    ),
    (
        "possible-completions",
        CompletionCommand {
            completion_type: CompletionType::PossibleCompletions,
            record_undo: false,
        },
    ),
    (
        "possible-filename-completions",
        CompletionCommand {
            completion_type: CompletionType::PossibleFilenameCompletions,
            record_undo: false,
        },
    ),
    (
        "possible-hostname-completions",
        CompletionCommand {
            completion_type: CompletionType::PossibleHostnameCompletions,
            record_undo: false,
        },
    ),
    (
        "possible-username-completions",
        CompletionCommand {
            completion_type: CompletionType::PossibleUsernameCompletions,
            record_undo: false,
        },
    ),
    (
        "possible-variable-completions",
        CompletionCommand {
            completion_type: CompletionType::PossibleVariableCompletions,
            record_undo: false,
        },
    ),
    (
        "vi-complete",
        CompletionCommand {
            completion_type: CompletionType::ViComplete,
            record_undo: true,
        },
    ),
];

pub(super) fn named_completion_command(command: &str) -> Option<CompletionCommand> {
    NAMED_COMPLETION_COMMANDS
        .iter()
        .find_map(|(name, entry)| (*name == command).then_some(*entry))
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_completion_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            EditCommand::TabComplete => {
                state.record_undo();
                self.complete(state, key, CompletionType::Complete, hooks)?;
                state.after_non_kill_command();
                Ok(EditorOutcome::Continue)
            }
            _ => unreachable!("command group mismatch"),
        }
    }
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_named_completion_command(
        &mut self,
        state: &mut EditorState,
        command: &str,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        match command {
            "complete-into-braces" => {
                state.record_undo();
                self.complete_into_braces(state, key, hooks)?;
                state.after_non_kill_command();
            }
            "dabbrev-expand" | "dynamic-complete-history" => {
                state.record_undo();
                self.dynamic_complete_history(state, hooks);
                state.after_non_kill_command();
            }
            "export-completions" => {
                self.export_completions(state, key, hooks)?;
                state.after_non_kill_command();
            }
            _ => unreachable!("named command group mismatch"),
        }
        Ok(EditorOutcome::Continue)
    }
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(crate) fn dynamic_complete_history(
        &mut self,
        state: &mut EditorState,
        hooks: &mut impl Hooks,
    ) {
        let word_breaks = self.completion_word_breaks(hooks);
        let prefix = state.buffer.word_before_point(Some(&word_breaks));
        if prefix.is_empty() {
            return;
        }
        if let Some(entry) = self
            .history
            .entries()
            .iter()
            .rev()
            .flat_map(|entry| self.tokenize(&entry.line_bytes, hooks))
            .find(|word| word.starts_with(&prefix) && word.as_slice() != prefix.as_slice())
        {
            state.buffer.insert_bytes(&entry[prefix.len()..]);
        }
    }

    pub(crate) fn yank_history_arg(
        &mut self,
        state: &mut EditorState,
        arg: Option<i32>,
        hooks: &impl Hooks,
    ) -> Result<(), ReadlineError> {
        let entries = self.history.entries();
        if entries.is_empty() {
            return Ok(());
        };
        let repeated = state.completion.last_yank_arg.clone();
        let (history_index, n) = if let Some(previous) = repeated {
            if let Some((start, end)) = previous.range {
                state.buffer.replace_range(start, end, "");
            }
            match arg {
                Some(arg) if arg < 0 => (
                    (previous.history_index + 1).min(entries.len().saturating_sub(1)),
                    previous.arg,
                ),
                Some(arg) => (previous.history_index.saturating_sub(1), arg),
                None => (previous.history_index.saturating_sub(1), previous.arg),
            }
        } else {
            (entries.len().saturating_sub(1), arg.unwrap_or(-1))
        };
        let Some(entry) = entries.get(history_index) else {
            self.ding()?;
            return Ok(());
        };
        let words = self.tokenize(&entry.line_bytes, hooks);
        if words.is_empty() {
            return Ok(());
        }
        let idx = if n < 0 {
            words.len().saturating_sub(1)
        } else {
            (n as usize).min(words.len().saturating_sub(1))
        };
        let start = state.buffer.point();
        state.buffer.insert_bytes(&words[idx]);
        let end = state.buffer.point();
        state.completion.last_yank_arg = Some(LastYankArgState {
            history_index,
            arg: n,
            range: Some((start, end)),
        });
        Ok(())
    }

    fn tokenize(&self, line: &[u8], hooks: &impl Hooks) -> Vec<Vec<u8>> {
        hooks
            .tokenize(line)
            .unwrap_or_else(|| command_words(line, &HistoryExpansionPolicy::default()))
    }
}
