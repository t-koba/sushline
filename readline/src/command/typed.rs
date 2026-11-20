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

/// Vi motions that may follow a pending operator. Other keys cancel the
/// operator in GNU vi mode; movement commands still move point, while other
/// commands are dropped by the dispatch guard.
fn is_vi_operator_motion_key(key: &[u8]) -> bool {
    key.len() == 1
        && matches!(
            key[0],
            b'0'..=b'9'
                | b' '
                | b'$'
                | b'%'
                | b'\''
                | b'`'
                | b'^'
                | b'b'
                | b'B'
                | b'c'
                | b'd'
                | b'e'
                | b'E'
                | b'f'
                | b'F'
                | b'g'
                | b'G'
                | b'h'
                | b'l'
                | b'w'
                | b'W'
                | b't'
                | b'T'
                | b'y'
                | b'|'
                | b','
                | b';'
                | b'"'
        )
}

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
    pub(super) fn vi_operator_blocks_command(&self, state: &EditorState, key: &[u8]) -> bool {
        matches!(self.keymap.current(), KeyMapName::ViCommand)
            && state.vi.vi_operator.is_some()
            && !is_vi_operator_motion_key(key)
    }

    pub(super) fn cancel_vi_operator(&self, state: &mut EditorState) {
        state.take_vi_operator();
        state.take_numeric_arg();
        state.mark_vi_redo_cancelled_operator();
    }

    pub(super) fn apply_command(
        &mut self,
        state: &mut EditorState,
        command: EditCommand,
        key: &[u8],
        hooks: &mut impl Hooks,
    ) -> Result<EditorOutcome, ReadlineError> {
        if command == EditCommand::DigitArgument
            && key.last() == Some(&b'-')
            && state.numeric_arg.is_some()
            && !state.numeric_arg_sign_only
        {
            if state.numeric_arg_composition == NumericArgComposition::Pending {
                state.numeric_arg = Some(1);
                state.numeric_arg_negative = false;
                state.numeric_arg_universal = false;
                self.ding()?;
                return self.apply_command(state, EditCommand::SelfInsert, b"-", hooks);
            }
            if !state.numeric_arg_universal {
                return self.apply_command(state, EditCommand::SelfInsert, b"-", hooks);
            }
        }
        let builds_arg = matches!(
            command,
            EditCommand::DigitArgument | EditCommand::UniversalArgument
        );
        if self.vi_operator_blocks_command(state, key)
            && typed_command_group(command) != TypedCommandGroup::Movement
        {
            self.cancel_vi_operator(state);
            return Ok(EditorOutcome::Continue);
        }
        if !builds_arg {
            state.completion.menu_completion = None;
        }
        let outcome = match typed_command_group(command) {
            TypedCommandGroup::Movement => self.apply_movement_command(state, command, key, hooks),
            TypedCommandGroup::Editing => self.apply_editing_command(state, command, key, hooks),
            TypedCommandGroup::Kill => self.apply_kill_command(state, command, key, hooks),
            TypedCommandGroup::HistoryNav => self.apply_history_nav_command(state, command, key),
            TypedCommandGroup::Vi => self.apply_vi_command(state, command, key),
            TypedCommandGroup::Completion => {
                self.apply_completion_command(state, command, key, hooks)
            }
            TypedCommandGroup::Misc => self.apply_misc_command(state, command, key, hooks),
        };
        if !builds_arg {
            state.consume_numeric_arg_unless_prefix();
        }
        outcome
    }
}
