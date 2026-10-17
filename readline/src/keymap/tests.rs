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
fn vi_register_bind_name_is_added_only_to_the_enabled_sorted_table() {
    assert!(!commands::bind_function_names(false).contains(&"vi-set-register"));
    let enabled = commands::bind_function_names(true);
    assert!(enabled.contains(&"vi-set-register"));
    assert!(enabled.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(!commands::is_bindable_function_name_with_registers(
        "vi-set-register",
        false
    ));
    assert!(commands::is_bindable_function_name_with_registers(
        "vi-set-register",
        true
    ));
}

#[test]
fn inputrc_accepts_bindable_eof_name_without_exporting_readline_bind_name() {
    assert_eq!(EditCommand::parse("end-of-file"), Some(EditCommand::Eof));
    assert!(commands::is_bindable_function_name("end-of-file"));
    assert!(!BIND_FUNCTION_NAMES.contains(&"end-of-file"));
}

#[test]
fn typed_edit_commands_round_trip_through_public_names() {
    for command in EditCommand::ALL {
        let name = command.as_str();
        assert_eq!(
            EditCommand::parse(name),
            Some(*command),
            "{name} must parse back to its typed command"
        );
        assert!(
            commands::is_bindable_function_name(name),
            "{name} must be accepted by bind/inputrc"
        );
    }
}

#[test]
fn bind_function_name_tables_stay_sorted_and_disjoint() {
    assert!(
        BIND_FUNCTION_NAMES.windows(2).all(|pair| pair[0] < pair[1]),
        "bind -l command names must stay sorted for binary_search"
    );
    assert!(
        commands::EXTRA_BIND_FUNCTION_NAMES
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "extra bind command names must stay sorted for binary_search"
    );
    for name in commands::EXTRA_BIND_FUNCTION_NAMES {
        assert!(
            !BIND_FUNCTION_NAMES.contains(name),
            "{name} must not be duplicated in bind -l names"
        );
        assert!(
            EditCommand::parse(name).is_some(),
            "{name} must map to a typed command"
        );
    }
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

#[test]
fn octal_escapes_wrap_to_low_eight_bits_like_gnu() {
    // GNU accumulates `\\nnn` into an eight-bit byte: verified against the
    // Bash 5.3 oracle (`"\\400"` binds `\\C-@`, `"\\777"` binds `\\377`).
    assert_eq!(KeySequence::parse("\"\\400\"").unwrap().bytes(), &[0x00]);
    assert_eq!(KeySequence::parse("\"\\777\"").unwrap().bytes(), &[0xff]);
    assert_eq!(KeySequence::parse("\"\\377\"").unwrap().bytes(), &[0xff]);
}

#[test]
fn empty_hex_escape_is_literal_x_like_gnu() {
    // GNU binds `"\\x"` to `x` (`"x": beginning-of-line` in `bind -p`).
    assert_eq!(KeySequence::parse("\"\\x\"").unwrap().bytes(), b"x");
    assert_eq!(KeySequence::parse("\"\\x41\"").unwrap().bytes(), &[0x41]);
    assert_eq!(
        KeySequence::parse("\"\\xFFF\"").unwrap().bytes(),
        &[0xff, b'F']
    );
}
