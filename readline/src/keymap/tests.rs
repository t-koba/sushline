use super::*;

#[test]
fn parses_control_and_meta_sequences() {
    assert_eq!(KeySequence::parse("\"\\C-a\"").unwrap().bytes(), &[1]);
    assert_eq!(
        KeySequence::parse("\"\\e[A\"").unwrap().bytes(),
        &[0x1b, b'[', b'A']
    );
    assert_eq!(
        KeySequence::parse("Meta-Rubout").unwrap().bytes(),
        &[0x1b, 0x7f]
    );
    assert_eq!(
        KeySequence::parse("\"\\M-\\C-a\"").unwrap().bytes(),
        &[0x1b, 0x01]
    );
    assert_eq!(
        KeySequence::parse("\"\\x18\\x01\"").unwrap().bytes(),
        &[0x18, 1]
    );
    assert_eq!(
        KeySequence::parse("\"\\030\\001\"").unwrap().bytes(),
        &[0x18, 1]
    );
    assert_eq!(KeySequence::parse("\"\\d\"").unwrap().bytes(), &[0x7f]);
}

#[test]
fn detects_key_sequence_prefixes() {
    let mut keymap = KeyMap::emacs_default();
    keymap.bind(
        KeyMapName::EmacsStandard,
        KeySequence::parse("\"\\C-x\\C-a\"").unwrap(),
        KeyBinding::Command(EditCommand::Yank),
    );
    assert!(keymap.has_prefix(KeyMapName::EmacsStandard, &[0x18]));
    assert!(!keymap.has_prefix(KeyMapName::EmacsStandard, &[0x18, 0x01]));
    assert_eq!(
        keymap
            .longest_matching_prefix(KeyMapName::EmacsStandard, &[0x18, 0x01, b'x'])
            .map(|(len, _)| len),
        Some(2)
    );
}

#[test]
fn inputrc_accepts_bindable_eof_name_without_exporting_bash_oracle_name() {
    assert_eq!(EditCommand::parse("end-of-file"), Some(EditCommand::Eof));
    assert!(is_bindable_function_name("end-of-file"));
    assert!(!BIND_FUNCTION_NAMES.contains(&"end-of-file"));
}

#[test]
fn bind_function_table_separates_editor_and_embedder_responsibility() {
    assert_eq!(
        commands::bind_function_kind("backward-char"),
        Some(commands::BindFunctionKind::Editor)
    );
    assert_eq!(
        commands::bind_function_kind("shell-backward-word"),
        Some(commands::BindFunctionKind::Editor)
    );
    assert_eq!(
        commands::bind_function_kind("shell-expand-line"),
        Some(commands::BindFunctionKind::Embedder)
    );
    assert_eq!(commands::bind_function_kind("not-a-command"), None);
}
