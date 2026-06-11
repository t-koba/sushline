use super::*;

#[test]
fn editor_new_retains_initial_inputrc_error_and_try_new_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inputrc");
    std::fs::write(&path, "$else\n").unwrap();
    let config = Config {
        inputrc_path: crate::config::InputrcPath::Path(path),
        ..Config::default()
    };

    let line = Editor::new(config.clone(), MemoryTerminal::default(), History::new());
    assert!(
        line.initial_inputrc_error()
            .is_some_and(|err| err.contains("$else without $if")),
        "{:?}",
        line.initial_inputrc_error()
    );

    let err = match Editor::try_new(config, MemoryTerminal::default(), History::new()) {
        Ok(_) => panic!("try_new unexpectedly accepted invalid inputrc"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("$else without $if"), "{err:?}");
}

#[test]
fn every_readline_command_is_in_typed_or_named_dispatch_table() {
    for command in crate::keymap::BIND_FUNCTION_NAMES {
        assert!(
            EditCommand::parse(command).is_some()
                || NAMED_READLINE_COMMAND_DISPATCH
                    .binary_search(command)
                    .is_ok(),
            "{command} must have an explicit dispatch classification"
        );
    }
}
