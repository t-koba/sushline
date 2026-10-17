use super::pty::*;
use portable_pty::PtySize;

#[test]
fn bash_readline_and_sushline_accept_same_edit_under_narrow_terminal() {
    let keys = b"abcdef ghijkl\x01X\r";
    let bash = run_bash_readline_with_size(
        keys,
        "",
        &[],
        PtySize {
            rows: 4,
            cols: 12,
            pixel_width: 0,
            pixel_height: 0,
        },
    );
    let sushline = run_sushline_harness_with_size(
        keys,
        "",
        &[],
        PtySize {
            rows: 4,
            cols: 12,
            pixel_width: 0,
            pixel_height: 0,
        },
    );

    assert_eq!(
        accepted_line(&bash),
        Some("Xabcdef ghijkl".to_string()),
        "{bash}"
    );
    assert_eq!(
        accepted_line(&sushline),
        Some("Xabcdef ghijkl".to_string()),
        "{sushline}"
    );
}

#[cfg(unix)]
#[test]
fn unwrapped_ansi_prompt_redisplay_differs_from_bash_markers_only_in_bash() {
    use portable_pty::CommandBuilder;
    use std::fs;

    // Bash 5.3 PTY observation: at 10 columns, the same color prompt produces
    // different redisplay when its escapes are not marked as nonprinting.
    let root = tempfile::tempdir().expect("prompt PTY root");
    // Keep host inputrc and shell history settings out of this prompt oracle.
    let inputrc_path = root.path().join("empty-inputrc");
    fs::write(&inputrc_path, b"").expect("write empty inputrc");
    let inputrc = inputrc_path.to_string_lossy().into_owned();
    let home = root.path().to_string_lossy().into_owned();
    let history = root.path().join("history").to_string_lossy().into_owned();
    let size = PtySize {
        rows: 24,
        cols: 10,
        pixel_width: 0,
        pixel_height: 0,
    };
    // The shell emits a control-byte sentinel; C-a/C-e force redraw across the
    // narrow wrap before Enter accepts the command.
    let keys = b"printf '\\x1fBASHDONE\\n'\x01\x05\r";

    let run_bash = |prompt: &str| {
        let mut command = CommandBuilder::new("bash");
        command.env("PS1", prompt);
        command.env("INPUTRC", &inputrc);
        command.env("HISTFILE", &history);
        command.env("HOME", &home);
        command.env("LC_ALL", "C");
        command.env("PROMPT_COMMAND", "");
        command.args(["--noprofile", "--norc", "-i"]);
        run_pty_steps_with_size_until(command, &[("P>", keys)], size, "\u{1f}BASHDONE")
    };
    let bash_bare = run_bash(r"\e[31mP>\e[0m");
    let bash_marked = run_bash(r"\[\e[31m\]P>\[\e[0m\]");
    let visible_prompt = "\x1b[31mP>\x1b[0m";
    assert!(bash_bare.contains(visible_prompt), "{bash_bare:?}");
    assert!(bash_marked.contains(visible_prompt), "{bash_marked:?}");
    assert!(bash_bare.contains("\u{1f}BASHDONE"), "{bash_bare:?}");
    assert!(bash_marked.contains("\u{1f}BASHDONE"), "{bash_marked:?}");
    let bash_bare_redraw = bash_bare.split("\u{1f}BASHDONE").next().unwrap();
    let bash_marked_redraw = bash_marked.split("\u{1f}BASHDONE").next().unwrap();
    assert_ne!(bash_bare_redraw, bash_marked_redraw);

    let run_sushline = |prompt: &str| {
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
        command.env("SUSHLINE_PROMPT", prompt);
        command.env("SUSHLINE_INPUTRC", "");
        command.env("INPUTRC", &inputrc);
        command.env("SUSHLINE_HISTORY", "");
        command.env("HOME", &home);
        command.env("LC_ALL", "C");
        run_pty_steps_with_size_until(command, &[("P>", keys)], size, "SUSHLINE_ACCEPTED:")
    };
    let sushline_bare = run_sushline("\x1b[31mP>\x1b[0m");
    let sushline_marked = run_sushline("\x01\x1b[31m\x02P>\x01\x1b[0m\x02");
    assert!(
        sushline_bare.contains("SUSHLINE_ACCEPTED:"),
        "{sushline_bare:?}"
    );
    assert!(
        sushline_marked.contains("SUSHLINE_ACCEPTED:"),
        "{sushline_marked:?}"
    );
    assert_eq!(
        accepted_line(&sushline_bare),
        accepted_line(&sushline_marked)
    );
    assert!(sushline_bare.contains(visible_prompt), "{sushline_bare:?}");
    assert!(
        sushline_marked.contains(visible_prompt),
        "{sushline_marked:?}"
    );
    let sushline_bare_redraw = sushline_bare.split("SUSHLINE_ACCEPTED:").next().unwrap();
    let sushline_marked_redraw = sushline_marked.split("SUSHLINE_ACCEPTED:").next().unwrap();
    assert_eq!(sushline_bare_redraw, sushline_marked_redraw);
}

#[test]
fn bash_readline_and_sushline_accept_same_screen_line_motion() {
    let size = PtySize {
        rows: 4,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    for (command, keys) in [
        (
            "previous-screen-line",
            b"abcdefghij klmnopqrst uvwxyz\x0fX\r".as_slice(),
        ),
        (
            "next-screen-line",
            b"abcdefghij klmnopqrst uvwxyz\x01\x0fX\r".as_slice(),
        ),
    ] {
        let inputrc = format!(r#""\C-o": {command}"#);
        let bash = run_bash_readline_with_size(keys, &inputrc, &[], size);
        let sushline = run_sushline_harness_with_size(keys, &inputrc, &[], size);

        assert_eq!(
            accepted_line(&sushline),
            accepted_line(&bash),
            "command={command}\nbash={bash}\nsushline={sushline}"
        );
    }
}

#[test]
fn bash_and_sushline_expand_embedded_tab_to_tab_stops() {
    // GNU `DISPLAY_TABS` (patch 0 Bash 5.3 PTY oracle): a literal TAB via
    // quoted-insert expands to spaces up to the next multiple-of-8 stop
    // instead of rendering as `^I`. `SUSHLINE_READY>` is 15 columns, so
    // `a<TAB>b` renders 8 spaces and a lone TAB renders one space.
    let bash = run_bash_readline(b"a\x16\tb\r");
    let sushline = run_sushline_harness(b"a\x16\tb\r");
    assert_eq!(accepted_line(&bash), Some("a\tb".to_string()), "{bash}");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert!(
        bash.contains("a        b"),
        "bash must expand a<TAB>b to 8 spaces, got {bash:?}"
    );
    assert!(
        sushline.contains("a        b"),
        "sushline must expand a<TAB>b to 8 spaces, got {sushline:?}"
    );
    assert!(
        !sushline.contains("^I"),
        "sushline must not render ^I, got {sushline:?}"
    );

    let bash = run_bash_readline(b"\x16\ta\r");
    let sushline = run_sushline_harness(b"\x16\ta\r");
    assert_eq!(
        accepted_line(&sushline),
        accepted_line(&bash),
        "bash={bash}\nsushline={sushline}"
    );
    assert!(
        bash.contains("> a"),
        "bash must expand a lone TAB to one space, got {bash:?}"
    );
    assert!(
        sushline.contains("> a"),
        "sushline must expand a lone TAB to one space, got {sushline:?}"
    );
    assert!(
        !sushline.contains("^I"),
        "sushline must not render ^I, got {sushline:?}"
    );
}
