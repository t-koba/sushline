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
fn prefix_meta_is_not_bound_in_the_default_keymap() {
    let keymap = KeyMap::emacs_default();
    assert!(keymap.maps.values().all(|map| {
        map.values()
            .all(|binding| !matches!(binding, KeyBinding::Command(EditCommand::PrefixMeta)))
    }));
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
fn octal_escapes_wrap_to_low_eight_bits() {
    // Regression coverage for octal values accumulating into an eight-bit byte.
    assert_eq!(KeySequence::parse("\"\\400\"").unwrap().bytes(), &[0x00]);
    assert_eq!(KeySequence::parse("\"\\777\"").unwrap().bytes(), &[0xff]);
    assert_eq!(KeySequence::parse("\"\\377\"").unwrap().bytes(), &[0xff]);
}

#[test]
fn empty_hex_escape_is_literal_x() {
    // Regression coverage for an empty hexadecimal escape producing a literal `x`.
    assert_eq!(KeySequence::parse("\"\\x\"").unwrap().bytes(), b"x");
    assert_eq!(KeySequence::parse("\"\\x41\"").unwrap().bytes(), &[0x41]);
    assert_eq!(
        KeySequence::parse("\"\\xFFF\"").unwrap().bytes(),
        &[0xff, b'F']
    );
}

#[test]
fn emacs_submaps_share_storage_and_user_override_identity() {
    for (map, prefix) in [(KeyMapName::EmacsMeta, 0x1b), (KeyMapName::EmacsCtlx, 0x18)] {
        let mut keymap = KeyMap::emacs_default();
        let relative = KeySequence::new(vec![0x18, b'a']);
        let root = KeySequence::new(vec![prefix, 0x18, b'a']);
        let binding = KeyBinding::Macro(b"NESTED".to_vec());
        keymap.bind_user(map, relative.clone(), binding.clone());
        assert_eq!(
            keymap.lookup(KeyMapName::Emacs, root.bytes()),
            Some(&binding)
        );
        assert_eq!(keymap.lookup(map, relative.bytes()), Some(&binding));
        assert!(keymap.has_prefix(map, &[0x18]));
        assert!(keymap.has_prefix(KeyMapName::Emacs, &[prefix, 0x18]));
        assert_eq!(
            keymap.longest_matching_prefix(map, &[0x18, b'a', b'b']),
            Some((2, &binding))
        );
        assert_eq!(
            keymap.longest_matching_prefix(KeyMapName::Emacs, &[prefix, 0x18, b'a', b'b']),
            Some((3, &binding))
        );
        assert!(keymap.is_user_binding(KeyMapName::Emacs, &root));
        assert_eq!(keymap.iter().filter(|(_, seq, _)| *seq == &root).count(), 1);
        assert!(!keymap.maps.contains_key(&map));

        keymap.bind_user(
            KeyMapName::Emacs,
            root.clone(),
            KeyBinding::Command(EditCommand::Yank),
        );
        assert!(keymap.is_user_binding(map, &relative));
        assert_eq!(
            keymap
                .bindings_for_command_name_in_map(map, "yank")
                .iter()
                .filter(|seq| *seq == &relative)
                .count(),
            1
        );
        let before = keymap.iter().count();
        let removed = keymap.unbind_command_in_map(map, "yank");
        assert!(removed >= 1);
        assert_eq!(keymap.iter().count(), before - removed);
        assert_eq!(keymap.lookup(KeyMapName::Emacs, root.bytes()), None);
        assert!(keymap.is_user_binding(map, &relative));
        assert!(keymap.is_user_binding(KeyMapName::Emacs, &root));

        keymap.bind(map, relative.clone(), binding.clone());
        assert_eq!(keymap.unbind_key(KeyMapName::Emacs, &root), Some(binding));
        assert_eq!(keymap.lookup(map, relative.bytes()), None);
    }
}

#[test]
fn emacs_submap_views_include_defaults_and_relative_iteration() {
    let mut keymap = KeyMap::emacs_default();
    assert_eq!(
        keymap.lookup(KeyMapName::EmacsMeta, b"b"),
        keymap.lookup(KeyMapName::Emacs, b"\x1bb")
    );
    assert!(keymap.lookup(KeyMapName::EmacsMeta, b"b").is_some());
    assert!(
        keymap
            .iter_map(KeyMapName::EmacsMeta)
            .any(|(_, seq, _)| seq.bytes() == b"b")
    );
    assert!(keymap.unbind_command("backward-word") >= 1);
    assert_eq!(keymap.lookup(KeyMapName::EmacsMeta, b"b"), None);
    assert!(keymap.is_user_binding(KeyMapName::EmacsMeta, &KeySequence::new(b"b".to_vec())));
    // Removing a prefix's own binding does not delete its descendants.
    keymap.unbind_key(KeyMapName::Emacs, &KeySequence::new(vec![0x1b]));
    assert!(keymap.lookup(KeyMapName::EmacsMeta, b"f").is_some());
}

#[test]
fn emacs_submap_root_fallback_is_visible_but_never_a_zero_length_match() {
    for (map, prefix) in [(KeyMapName::EmacsMeta, 0x1b), (KeyMapName::EmacsCtlx, 0x18)] {
        let mut keymap = KeyMap::default();
        let fallback = KeyBinding::Macro(b"ROOT".to_vec());
        keymap.bind(
            KeyMapName::Emacs,
            KeySequence::new(vec![prefix]),
            fallback.clone(),
        );
        keymap.bind(
            map,
            KeySequence::new(b"ab".to_vec()),
            KeyBinding::Macro(b"CHILD".to_vec()),
        );

        // Bash bind -m emacs-meta/-ctlx -s exposes the root fallback as "".
        assert_eq!(keymap.lookup(map, b""), Some(&fallback));
        assert!(
            keymap
                .iter_map(map)
                .any(|(_, seq, binding)| seq.bytes().is_empty() && binding == &fallback)
        );
        // Test the query directly, never dispatch a potentially recursive macro.
        for bytes in [b"".as_slice(), b"z", b"a"] {
            assert_eq!(keymap.longest_matching_prefix(map, bytes), None);
        }
        assert_eq!(
            keymap
                .longest_matching_prefix(map, b"abc")
                .map(|(len, _)| len),
            Some(2)
        );
        assert_eq!(
            keymap.longest_matching_prefix(KeyMapName::Emacs, &[prefix, b'z']),
            Some((1, &fallback))
        );
    }
}

#[test]
fn longest_matching_prefix_never_returns_a_direct_empty_binding() {
    let mut keymap = KeyMap::default();
    let map = KeyMapName::Emacs;
    let binding = KeyBinding::Macro(b"EMPTY".to_vec());
    keymap.bind(map, KeySequence::new(Vec::new()), binding.clone());
    assert_eq!(keymap.lookup(map, b""), Some(&binding));
    assert_eq!(keymap.longest_matching_prefix(map, b""), None);
    assert_eq!(keymap.longest_matching_prefix(map, b"z"), None);
    keymap.bind(map, KeySequence::new(b"a".to_vec()), binding.clone());
    assert_eq!(
        keymap.longest_matching_prefix(map, b"ab"),
        Some((1, &binding))
    );
}
