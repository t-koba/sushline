use super::*;

impl KeyMap {
    pub fn emacs_default() -> Self {
        let mut this = Self {
            current: KeyMapName::EmacsStandard,
            ..Default::default()
        };
        for name in [
            KeyMapName::EmacsStandard,
            KeyMapName::EmacsMeta,
            KeyMapName::EmacsCtlx,
            KeyMapName::ViCommand,
            KeyMapName::ViInsert,
        ] {
            this.maps.entry(name).or_default();
        }
        for b in 0x20..=0x7e {
            this.bind(
                KeyMapName::EmacsStandard,
                KeySequence::new(vec![b]),
                KeyBinding::Command(EditCommand::SelfInsert),
            );
        }
        for b in 0x80..=0xff {
            this.bind(
                KeyMapName::EmacsStandard,
                KeySequence::new(vec![b]),
                KeyBinding::Command(EditCommand::SelfInsert),
            );
        }
        this.bind_builtin("\\C-a", EditCommand::BeginningOfLine);
        this.bind_builtin("\\C-b", EditCommand::BackwardChar);
        this.bind_builtin("\\C-c", EditCommand::Abort);
        this.bind_builtin("\\C-d", EditCommand::Eof);
        this.bind_builtin("\\C-e", EditCommand::EndOfLine);
        this.bind_builtin("\\C-f", EditCommand::ForwardChar);
        this.bind_builtin("\\C-g", EditCommand::Abort);
        this.bind_builtin("\\C-h", EditCommand::BackwardDeleteChar);
        this.bind_builtin("\\C-k", EditCommand::KillLine);
        this.bind_builtin("\\C-j", EditCommand::AcceptLine);
        this.bind_builtin("\\C-l", EditCommand::ClearScreen);
        this.bind_builtin("\\C-m", EditCommand::AcceptLine);
        this.bind_builtin("\\C-n", EditCommand::NextHistory);
        this.bind_builtin("\\C-p", EditCommand::PreviousHistory);
        this.bind_builtin("\\C-q", EditCommand::QuotedInsert);
        this.bind_builtin("\\C-r", EditCommand::ReverseSearchHistory);
        this.bind_builtin("\\C-t", EditCommand::TransposeChars);
        this.bind_builtin("\\C-u", EditCommand::UnixLineDiscard);
        this.bind_builtin("\\C-v", EditCommand::QuotedInsert);
        this.bind_builtin("\\C-w", EditCommand::UnixWordRubout);
        this.bind_builtin("\\C-y", EditCommand::Yank);
        this.bind_builtin("\\C-?", EditCommand::BackwardDeleteChar);
        this.bind_builtin("\\t", EditCommand::TabComplete);
        this.bind_builtin("\\e-", EditCommand::DigitArgument);
        this.bind_builtin("\\ey", EditCommand::YankPop);
        for digit in '0'..='9' {
            this.bind_builtin(&format!("\\e{digit}"), EditCommand::DigitArgument);
        }
        this.bind_builtin("\\e\\\\", EditCommand::DeleteHorizontalSpace);
        this.bind_builtin("\\e<", EditCommand::HistoryBeginning);
        this.bind_builtin("\\e>", EditCommand::HistoryEnd);
        this.bind_builtin_named("\\e.", "yank-last-arg");
        this.bind_builtin("\\eb", EditCommand::BackwardWord);
        this.bind_builtin("\\ec", EditCommand::CapitalizeWord);
        this.bind_builtin("\\ed", EditCommand::KillWord);
        this.bind_builtin("\\ef", EditCommand::ForwardWord);
        this.bind_builtin("\\el", EditCommand::DowncaseWord);
        this.bind_builtin("\\er", EditCommand::RevertLine);
        this.bind_builtin("\\et", EditCommand::TransposeWords);
        this.bind_builtin("\\eu", EditCommand::UpcaseWord);
        this.bind_builtin("\\e\\C-?", EditCommand::BackwardKillWord);
        this.bind_builtin("\\e\\C-r", EditCommand::RevertLine);
        this.bind_builtin("\\C-x\\C-?", EditCommand::BackwardKillLine);
        this.bind_builtin("\\C-x(", EditCommand::StartKbdMacro);
        this.bind_builtin("\\C-x)", EditCommand::EndKbdMacro);
        this.bind_builtin("\\C-x\\C-g", EditCommand::Abort);
        this.bind_builtin("\\C-x\\C-u", EditCommand::Undo);
        this.bind_builtin("\\C-x\\C-x", EditCommand::ExchangePointAndMark);
        this.bind_builtin("\\C-xe", EditCommand::CallLastKbdMacro);
        this.bind_builtin("\\e\\C-g", EditCommand::Abort);
        this.bind_builtin("\\C-@", EditCommand::SetMark);
        this.bind_builtin("\\C-_", EditCommand::Undo);
        this.bind_builtin("\\e ", EditCommand::SetMark);
        this.bind_builtin("\\e[1;3D", EditCommand::BackwardWord);
        this.bind_builtin("\\e[1;5D", EditCommand::BackwardWord);
        this.bind_builtin("\\e[1;3C", EditCommand::ForwardWord);
        this.bind_builtin("\\e[1;5C", EditCommand::ForwardWord);
        this.bind_builtin("\\e[3;5~", EditCommand::KillWord);
        this.bind_builtin_named("\\e[200~", "bracketed-paste-begin");
        this.bind_builtin("\\eOH", EditCommand::BeginningOfLine);
        this.bind_builtin("\\e[H", EditCommand::BeginningOfLine);
        this.bind_builtin("\\eOF", EditCommand::EndOfLine);
        this.bind_builtin("\\e[F", EditCommand::EndOfLine);
        this.bind_builtin("\\eOD", EditCommand::BackwardChar);
        this.bind_builtin("\\eOC", EditCommand::ForwardChar);
        this.bind_builtin("\\eOA", EditCommand::PreviousHistory);
        this.bind_builtin("\\eOB", EditCommand::NextHistory);
        this.bind_builtin("\\e[A", EditCommand::PreviousHistory);
        this.bind_builtin("\\e[B", EditCommand::NextHistory);
        this.bind_builtin("\\e[C", EditCommand::ForwardChar);
        this.bind_builtin("\\e[D", EditCommand::BackwardChar);
        super::vi::bind_vi_defaults(&mut this);
        this
    }

    pub(super) fn bind_builtin(&mut self, key: &str, command: EditCommand) {
        let seq = KeySequence::parse(&format!("\"{key}\"")).expect("valid builtin key binding");
        self.bind(KeyMapName::EmacsStandard, seq, KeyBinding::Command(command));
    }

    pub(super) fn bind_builtin_named(&mut self, key: &str, command: &str) {
        let seq = KeySequence::parse(&format!("\"{key}\"")).expect("valid builtin key binding");
        self.bind(
            KeyMapName::EmacsStandard,
            seq,
            KeyBinding::NamedCommand(command.to_string()),
        );
    }

    pub(super) fn bind_vi_insert(&mut self, key: &str, command: EditCommand) {
        let seq = KeySequence::parse(&format!("\"{key}\"")).expect("valid vi insert binding");
        self.bind(KeyMapName::ViInsert, seq, KeyBinding::Command(command));
    }

    pub(super) fn bind_vi_insert_named(&mut self, key: &str, command: &str) {
        let seq = KeySequence::parse(&format!("\"{key}\"")).expect("valid vi insert binding");
        self.bind(
            KeyMapName::ViInsert,
            seq,
            KeyBinding::NamedCommand(command.to_string()),
        );
    }

    pub(super) fn bind_vi_command(&mut self, key: &str, command: EditCommand) {
        let seq = KeySequence::parse(&format!("\"{key}\"")).expect("valid vi command binding");
        self.bind(KeyMapName::ViCommand, seq, KeyBinding::Command(command));
    }

    pub(super) fn bind_vi_command_named(&mut self, key: &str, command: &str) {
        let seq = KeySequence::parse(&format!("\"{key}\"")).expect("valid vi command binding");
        self.bind(
            KeyMapName::ViCommand,
            seq,
            KeyBinding::NamedCommand(command.to_string()),
        );
    }
}
