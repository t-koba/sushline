use super::*;

pub(super) fn bind_vi_defaults(this: &mut KeyMap) {
    for b in [
        0x01, 0x02, 0x03, 0x05, 0x06, 0x07, 0x0b, 0x0c, 0x0f, 0x11, 0x18, 0x1a, 0x1c, 0x1d, 0x1e,
    ] {
        this.bind(
            KeyMapName::ViInsert,
            KeySequence::new(vec![b]),
            KeyBinding::Command(EditCommand::SelfInsert),
        );
    }
    for b in 0x20..=0x7e {
        this.bind(
            KeyMapName::ViInsert,
            KeySequence::new(vec![b]),
            KeyBinding::Command(EditCommand::SelfInsert),
        );
    }
    for b in 0x80..=0xff {
        this.bind(
            KeyMapName::ViInsert,
            KeySequence::new(vec![b]),
            KeyBinding::Command(EditCommand::SelfInsert),
        );
    }

    this.bind_vi_insert("\\e", EditCommand::ViMovementMode);
    this.bind_vi_insert("\\C-i", EditCommand::TabComplete);
    this.bind_vi_insert_named("\\C-d", "vi-eof-maybe");
    this.bind_vi_insert("\\C-h", EditCommand::BackwardDeleteChar);
    this.bind_vi_insert("\\C-j", EditCommand::AcceptLine);
    this.bind_vi_insert("\\C-m", EditCommand::AcceptLine);
    this.bind_vi_insert("\\C-r", EditCommand::ReverseSearchHistory);
    this.bind_vi_insert_named("\\C-s", "forward-search-history");
    this.bind_vi_insert("\\C-t", EditCommand::TransposeChars);
    this.bind_vi_insert("\\C-u", EditCommand::UnixLineDiscard);
    this.bind_vi_insert("\\C-v", EditCommand::QuotedInsert);
    this.bind_vi_insert_named("\\C-w", "vi-unix-word-rubout");
    this.bind_vi_insert("\\C-y", EditCommand::Yank);
    this.bind_vi_insert_named("\\C-_", "vi-undo");
    this.bind_vi_insert("\\C-?", EditCommand::BackwardDeleteChar);
    this.bind_vi_insert_named("\\e[200~", "bracketed-paste-begin");
    this.bind_vi_insert("\\e[1;3D", EditCommand::BackwardWord);
    this.bind_vi_insert("\\e[1;5D", EditCommand::BackwardWord);
    this.bind_vi_insert("\\e[1;3C", EditCommand::ForwardWord);
    this.bind_vi_insert("\\e[1;5C", EditCommand::ForwardWord);
    this.bind_vi_insert("\\e[3;5~", EditCommand::KillWord);
    this.bind_vi_insert_named("\\e[2~", "overwrite-mode");
    this.bind_vi_insert("\\e[3~", EditCommand::DeleteChar);
    this.bind_vi_insert("\\e[5~", EditCommand::HistorySearchBackward);
    this.bind_vi_insert("\\e[6~", EditCommand::HistorySearchForward);
    this.bind_vi_insert("\\eOH", EditCommand::BeginningOfLine);
    this.bind_vi_insert("\\e[H", EditCommand::BeginningOfLine);
    this.bind_vi_insert("\\eOF", EditCommand::EndOfLine);
    this.bind_vi_insert("\\e[F", EditCommand::EndOfLine);
    this.bind_vi_insert("\\eOD", EditCommand::BackwardChar);
    this.bind_vi_insert("\\e[D", EditCommand::BackwardChar);
    this.bind_vi_insert("\\eOC", EditCommand::ForwardChar);
    this.bind_vi_insert("\\e[C", EditCommand::ForwardChar);
    this.bind_vi_insert("\\eOA", EditCommand::PreviousHistory);
    this.bind_vi_insert("\\e[A", EditCommand::PreviousHistory);
    this.bind_vi_insert("\\eOB", EditCommand::NextHistory);
    this.bind_vi_insert("\\e[B", EditCommand::NextHistory);
    this.bind_vi_insert_named("\\C-n", "menu-complete");
    this.bind_vi_insert_named("\\C-p", "menu-complete-backward");

    this.bind_vi_command("\\C-g", EditCommand::Abort);
    this.bind_vi_command("\\C-h", EditCommand::BackwardChar);
    this.bind_vi_command(" ", EditCommand::ForwardChar);
    this.bind_vi_command("i", EditCommand::ViInsertionMode);
    this.bind_vi_command("a", EditCommand::ViAppendMode);
    this.bind_vi_command("A", EditCommand::ViAppendEol);
    this.bind_vi_command("I", EditCommand::ViInsertBeg);
    this.bind_vi_command_named("\\C-d", "vi-eof-maybe");
    this.bind_vi_command("\\C-k", EditCommand::KillLine);
    this.bind_vi_command("\\C-l", EditCommand::ClearScreen);
    this.bind_vi_command("\\C-n", EditCommand::NextHistory);
    this.bind_vi_command("\\C-p", EditCommand::PreviousHistory);
    this.bind_vi_command("\\C-q", EditCommand::QuotedInsert);
    this.bind_vi_command("\\C-r", EditCommand::ReverseSearchHistory);
    this.bind_vi_command_named("\\C-s", "forward-search-history");
    this.bind_vi_command("\\C-t", EditCommand::TransposeChars);
    this.bind_vi_command("\\C-u", EditCommand::UnixLineDiscard);
    this.bind_vi_command("\\C-v", EditCommand::QuotedInsert);
    this.bind_vi_command_named("\\C-w", "vi-unix-word-rubout");
    this.bind_vi_command("\\C-y", EditCommand::Yank);
    this.bind_vi_command_named("\\C-_", "vi-undo");
    this.bind_vi_command("\\e[1;3D", EditCommand::BackwardWord);
    this.bind_vi_command("\\e[1;5D", EditCommand::BackwardWord);
    this.bind_vi_command("\\e[1;3C", EditCommand::ForwardWord);
    this.bind_vi_command("\\e[1;5C", EditCommand::ForwardWord);
    this.bind_vi_command("\\e[3;5~", EditCommand::KillWord);
    this.bind_vi_command_named("\\e[2~", "overwrite-mode");
    this.bind_vi_command("\\e[3~", EditCommand::DeleteChar);
    this.bind_vi_command("\\e[5~", EditCommand::HistorySearchBackward);
    this.bind_vi_command("\\e[6~", EditCommand::HistorySearchForward);
    this.bind_vi_command("\\eOH", EditCommand::BeginningOfLine);
    this.bind_vi_command("\\e[H", EditCommand::BeginningOfLine);
    this.bind_vi_command("\\eOF", EditCommand::EndOfLine);
    this.bind_vi_command("\\e[F", EditCommand::EndOfLine);
    this.bind_vi_command("\\eOD", EditCommand::BackwardChar);
    this.bind_vi_command("\\e[D", EditCommand::BackwardChar);
    this.bind_vi_command("\\eOC", EditCommand::ForwardChar);
    this.bind_vi_command("\\e[C", EditCommand::ForwardChar);
    this.bind_vi_command("\\eOA", EditCommand::PreviousHistory);
    this.bind_vi_command("\\e[A", EditCommand::PreviousHistory);
    this.bind_vi_command("\\eOB", EditCommand::NextHistory);
    this.bind_vi_command("\\e[B", EditCommand::NextHistory);
    this.bind_vi_command("h", EditCommand::BackwardChar);
    this.bind_vi_command("j", EditCommand::NextHistory);
    this.bind_vi_command("k", EditCommand::PreviousHistory);
    this.bind_vi_command("l", EditCommand::ForwardChar);
    this.bind_vi_command("+", EditCommand::NextHistory);
    this.bind_vi_command("-", EditCommand::PreviousHistory);
    this.bind_vi_command("0", EditCommand::BeginningOfLine);
    this.bind_vi_command("$", EditCommand::EndOfLine);
    this.bind_vi_command_named("#", "insert-comment");
    this.bind_vi_command_named("x", "vi-delete");
    this.bind_vi_command_named("X", "vi-rubout");
    for digit in '1'..='9' {
        this.bind_vi_command_named(&digit.to_string(), "vi-arg-digit");
    }
    this.bind_vi_command_named("b", "vi-prev-word");
    this.bind_vi_command_named("B", "vi-prev-word");
    this.bind_vi_command_named("^", "vi-first-print");
    this.bind_vi_command_named("~", "vi-change-case");
    this.bind_vi_command_named("r", "vi-change-char");
    this.bind_vi_command_named("c", "vi-change-to");
    this.bind_vi_command_named("d", "vi-delete-to");
    this.bind_vi_command_named("e", "vi-end-word");
    this.bind_vi_command_named("E", "vi-end-word");
    this.bind_vi_command_named("f", "vi-char-search");
    this.bind_vi_command_named("F", "vi-char-search");
    this.bind_vi_command_named("w", "vi-next-word");
    this.bind_vi_command_named("W", "vi-next-word");
    this.bind_vi_command_named("t", "vi-char-search");
    this.bind_vi_command_named("T", "vi-char-search");
    this.bind_vi_command_named(";", "vi-char-search");
    this.bind_vi_command_named(",", "vi-char-search");
    this.bind_vi_command_named("%", "vi-match");
    this.bind_vi_command_named("|", "vi-column");
    this.bind_vi_command_named("G", "vi-fetch-history");
    this.bind_vi_command_named("m", "vi-set-mark");
    this.bind_vi_command_named("`", "vi-goto-mark");
    this.bind_vi_command_named("p", "vi-put");
    this.bind_vi_command_named("P", "vi-put");
    this.bind_vi_command_named("D", "vi-delete-to");
    this.bind_vi_command_named("C", "vi-change-to");
    this.bind_vi_command_named("S", "vi-subst");
    this.bind_vi_command_named("Y", "vi-yank-to");
    this.bind_vi_command_named("*", "bash-vi-complete");
    this.bind_vi_command_named("=", "bash-vi-complete");
    this.bind_vi_command_named("\\\\", "bash-vi-complete");
    this.bind_vi_command_named("&", "vi-tilde-expand");
    this.bind_vi_command_named("_", "vi-yank-arg");
    this.bind_vi_command_named("v", "vi-edit-and-execute-command");
    this.bind_vi_command_named(".", "vi-redo");
    this.bind_vi_command_named("/", "vi-search");
    this.bind_vi_command_named("?", "vi-search");
    this.bind_vi_command_named("n", "vi-search-again");
    this.bind_vi_command_named("N", "vi-search-again");
    this.bind_vi_command_named("R", "vi-replace");
    this.bind_vi_command("U", EditCommand::RevertLine);
    this.bind_vi_command_named("s", "vi-subst");
    this.bind_vi_command_named("u", "vi-undo");
    this.bind_vi_command_named("y", "vi-yank-to");
    this.bind_vi_command("\\C-j", EditCommand::AcceptLine);
    this.bind_vi_command("\\C-m", EditCommand::AcceptLine);
}
