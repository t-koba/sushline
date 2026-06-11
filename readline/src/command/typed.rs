use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypedCommandGroup {
    Movement,
    Editing,
    Kill,
    HistoryNav,
    Vi,
    Completion,
    Misc,
}

const TYPED_COMMAND_TABLE: &[(EditCommand, TypedCommandGroup)] = &[
    (EditCommand::Abort, TypedCommandGroup::Misc),
    (EditCommand::AcceptLine, TypedCommandGroup::Misc),
    (EditCommand::BackwardChar, TypedCommandGroup::Movement),
    (EditCommand::BackwardDeleteChar, TypedCommandGroup::Editing),
    (EditCommand::BackwardKillLine, TypedCommandGroup::Kill),
    (EditCommand::BackwardKillWord, TypedCommandGroup::Kill),
    (EditCommand::UnixWordRubout, TypedCommandGroup::Kill),
    (EditCommand::BackwardWord, TypedCommandGroup::Movement),
    (EditCommand::CapitalizeWord, TypedCommandGroup::Editing),
    (EditCommand::BeginningOfLine, TypedCommandGroup::Movement),
    (EditCommand::CallLastKbdMacro, TypedCommandGroup::Misc),
    (EditCommand::ClearScreen, TypedCommandGroup::Misc),
    (EditCommand::CopyRegionAsKill, TypedCommandGroup::Kill),
    (EditCommand::DeleteChar, TypedCommandGroup::Editing),
    (
        EditCommand::DeleteHorizontalSpace,
        TypedCommandGroup::Editing,
    ),
    (EditCommand::DigitArgument, TypedCommandGroup::Misc),
    (EditCommand::DowncaseWord, TypedCommandGroup::Editing),
    (EditCommand::EndKbdMacro, TypedCommandGroup::Misc),
    (EditCommand::EndOfLine, TypedCommandGroup::Movement),
    (
        EditCommand::ExchangePointAndMark,
        TypedCommandGroup::Movement,
    ),
    (EditCommand::ForwardChar, TypedCommandGroup::Movement),
    (EditCommand::ForwardWord, TypedCommandGroup::Movement),
    (EditCommand::HistoryBeginning, TypedCommandGroup::HistoryNav),
    (EditCommand::HistoryEnd, TypedCommandGroup::HistoryNav),
    (
        EditCommand::HistorySearchBackward,
        TypedCommandGroup::HistoryNav,
    ),
    (
        EditCommand::HistorySearchForward,
        TypedCommandGroup::HistoryNav,
    ),
    (EditCommand::KillLine, TypedCommandGroup::Kill),
    (EditCommand::KillRegion, TypedCommandGroup::Kill),
    (EditCommand::KillWholeLine, TypedCommandGroup::Kill),
    (EditCommand::KillWord, TypedCommandGroup::Kill),
    (EditCommand::UniversalArgument, TypedCommandGroup::Misc),
    (EditCommand::UnixLineDiscard, TypedCommandGroup::Kill),
    (EditCommand::ViAppendEol, TypedCommandGroup::Vi),
    (EditCommand::ViAppendMode, TypedCommandGroup::Vi),
    (EditCommand::ViInsertBeg, TypedCommandGroup::Vi),
    (EditCommand::ViInsertionMode, TypedCommandGroup::Vi),
    (EditCommand::ViMovementMode, TypedCommandGroup::Vi),
    (EditCommand::NextHistory, TypedCommandGroup::HistoryNav),
    (EditCommand::PreviousHistory, TypedCommandGroup::HistoryNav),
    (EditCommand::QuotedInsert, TypedCommandGroup::Editing),
    (
        EditCommand::ReverseSearchHistory,
        TypedCommandGroup::HistoryNav,
    ),
    (EditCommand::RevertLine, TypedCommandGroup::Editing),
    (EditCommand::SelfInsert, TypedCommandGroup::Editing),
    (EditCommand::SetMark, TypedCommandGroup::Movement),
    (EditCommand::PrintLastKbdMacro, TypedCommandGroup::Misc),
    (EditCommand::StartKbdMacro, TypedCommandGroup::Misc),
    (EditCommand::TabComplete, TypedCommandGroup::Completion),
    (EditCommand::PrefixMeta, TypedCommandGroup::Misc),
    (EditCommand::TransposeChars, TypedCommandGroup::Editing),
    (EditCommand::TransposeWords, TypedCommandGroup::Editing),
    (EditCommand::UpcaseWord, TypedCommandGroup::Editing),
    (EditCommand::Yank, TypedCommandGroup::Kill),
    (EditCommand::YankPop, TypedCommandGroup::Kill),
    (EditCommand::Eof, TypedCommandGroup::Editing),
    (EditCommand::Unknown, TypedCommandGroup::Misc),
    (EditCommand::Undo, TypedCommandGroup::Editing),
];

fn typed_command_group(command: EditCommand) -> TypedCommandGroup {
    TYPED_COMMAND_TABLE
        .iter()
        .find_map(|(entry, group)| (*entry == command).then_some(*group))
        .unwrap_or(TypedCommandGroup::Misc)
}

impl<T> Editor<T>
where
    T: TerminalIo,
{
    pub(super) fn apply_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        if !matches!(
            command,
            EditCommand::DigitArgument | EditCommand::UniversalArgument
        ) {
            state.completion.menu_completion = None;
        }
        match typed_command_group(command) {
            TypedCommandGroup::Movement => self.apply_movement_command(state, command, key, hooks),
            TypedCommandGroup::Editing => self.apply_editing_command(state, command, key, hooks),
            TypedCommandGroup::Kill => self.apply_kill_command(state, command, key, hooks),
            TypedCommandGroup::HistoryNav => self.apply_history_nav_command(state, command, key),
            TypedCommandGroup::Vi => self.apply_vi_command(state, command, key),
            TypedCommandGroup::Completion => {
                self.apply_completion_command(state, command, key, hooks)
            }
            TypedCommandGroup::Misc => self.apply_misc_command(state, command, key, hooks),
        }
    }
}
