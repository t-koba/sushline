//! Terminal/display compatibility cases for prompt widths, redisplay and cursor
//! outcomes, and terminal-mode sequences. Evidence is limited to the tested
//! Bash 5.3 PTY fixtures and backend; it is not full renderer-byte parity.

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
fn unwrapped_ansi_prompt_redisplay_counts_unmarked_escapes() {
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
    // The final C-e redraw re-emits the last wrapped row. The 23-byte
    // command ends at logical column 4 with the bare 11-column prompt,
    // versus column 5 with the explicitly hidden, 2-column prompt.
    // This is Bash 5.3 PTY runtime evidence, not a source/API assertion.
    assert!(bash_bare_redraw.contains("\rE\\n'\r\n"), "{bash_bare:?}");
    assert!(
        bash_marked_redraw.contains("\rNE\\n'\r\n"),
        "{bash_marked:?}"
    );

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
    let expected = Some("printf '\\x1fBASHDONE\\n'".to_string());
    assert_eq!(accepted_line(&sushline_bare), expected);
    assert_eq!(accepted_line(&sushline_marked), expected);
    assert!(sushline_bare.contains(visible_prompt), "{sushline_bare:?}");
    assert!(
        sushline_marked.contains(visible_prompt),
        "{sushline_marked:?}"
    );
    let sushline_bare_redraw = sushline_bare.split("SUSHLINE_ACCEPTED:").next().unwrap();
    let sushline_marked_redraw = sushline_marked.split("SUSHLINE_ACCEPTED:").next().unwrap();
    assert_ne!(sushline_bare_redraw, sushline_marked_redraw);
    // Sushline repaints rather than using Bash's incremental bytes. Check
    // the corresponding C-a and C-e logical columns (CSI G is one-based),
    // not equality of the two implementations' complete output streams.
    for (redraw, start, end) in [(sushline_bare_redraw, 2, 5), (sushline_marked_redraw, 3, 6)] {
        assert!(
            redraw.contains(&format!("\x1b[K\x1b[2A\x1b[{start}G")),
            "{redraw:?}"
        );
        let before_accept = redraw.split("\x1b[?2004l").next().unwrap();
        assert!(
            before_accept.ends_with(&format!("\x1b[{end}G\r\r\n")),
            "{redraw:?}"
        );
    }
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

#[cfg(unix)]
#[test]
fn the_prompt_is_drawn_as_given_and_the_shell_spelling_is_opt_in() {
    use portable_pty::CommandBuilder;
    use std::fs;

    // GNU Readline draws the prompt it is given: a shell expands its own
    // prompt escapes before the editor sees them, and the editor reads only
    // the invisible markers the shell hands it. Bash 5.3 PTY observations:
    // the host draws the escape the shell expanded, and it draws an escape
    // the shell left alone as the two characters it was given.
    let root = tempfile::tempdir().expect("prompt PTY root");
    let inputrc_path = root.path().join("empty-inputrc");
    fs::write(&inputrc_path, b"").expect("write empty inputrc");
    let inputrc = inputrc_path.to_string_lossy().into_owned();
    let home = root.path().to_string_lossy().into_owned();
    let history = root.path().join("history");
    let size = PtySize {
        rows: 24,
        cols: 40,
        pixel_width: 0,
        pixel_height: 0,
    };
    let keys = b"printf '\\x1fPROMPTDONE\\n'\r";

    let run_bash = |prompt: &str| {
        let mut command = CommandBuilder::new("bash");
        command.env("PS1", prompt);
        command.env("INPUTRC", &inputrc);
        command.env("HISTFILE", &history);
        command.env("HOME", &home);
        command.env("LC_ALL", "C");
        command.env("PROMPT_COMMAND", "");
        command.args(["--noprofile", "--norc", "-i"]);
        run_pty_steps_with_size_until(command, &[("B> ", keys)], size, "\u{1f}PROMPTDONE")
    };

    let run_sushline = |prompt: &str, shell_spelling: bool| {
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
        command.env("SUSHLINE_PROMPT", prompt);
        command.env("SUSHLINE_INPUTRC", "");
        command.env("INPUTRC", &inputrc);
        command.env("SUSHLINE_HISTORY", "");
        command.env("HOME", &home);
        command.env("LC_ALL", "C");
        if shell_spelling {
            command.env("SUSHLINE_PROMPT_SHELL_ESCAPES", "1");
        }
        run_pty_steps_with_size_until(command, &[("B> ", keys)], size, "SUSHLINE_ACCEPTED:")
    };

    // An escape the shell leaves alone reaches the editor as it was written,
    // and the editor draws the prompt it is given.
    let bash_unknown = run_bash(r"A\qB> ");
    assert!(bash_unknown.contains(r"A\qB> "), "{bash_unknown:?}");
    let sushline_unknown = run_sushline(r"A\qB> ", false);
    assert!(sushline_unknown.contains(r"A\qB> "), "{sushline_unknown:?}");

    // An escape the shell expands reaches the editor already expanded, and
    // the editor draws the prompt it is given.
    let bash_escape = run_bash(r"A\e[31mB> ");
    assert!(bash_escape.contains("\x1b[31mB> "), "{bash_escape:?}");
    let sushline_escape = run_sushline(r"A\e[31mB> ", false);
    assert!(
        sushline_escape.contains(r"A\e[31mB> "),
        "{sushline_escape:?}"
    );

    // The shell's spelling is read when the caller asks for it, and then the
    // prompt is drawn as the host draws it.
    let sushline_shell = run_sushline(r"A\e[31mB> ", true);
    assert!(sushline_shell.contains("\x1b[31mB> "), "{sushline_shell:?}");

    // The shell's spelling decodes what the shell decodes: an octal
    // escape with three digits is the byte the shell produces, and an
    // escape the shell leaves alone stays as it was written.
    let bash_octal = run_bash(r"A\101B> ");
    assert!(bash_octal.contains("AAB> "), "{bash_octal:?}");
    let sushline_octal = run_sushline(r"A\101B> ", true);
    assert!(sushline_octal.contains("AAB> "), "{sushline_octal:?}");
    let bash_literal = run_bash(r"A\qB> ");
    assert!(bash_literal.contains(r"A\qB> "), "{bash_literal:?}");
    let sushline_literal = run_sushline(r"A\qB> ", true);
    assert!(sushline_literal.contains(r"A\qB> "), "{sushline_literal:?}");

    // Controlled Bash 5.3 PTY comparisons for the second-pass quoting
    // spellings and backslash runs before `e`. These prompts contain no
    // active parameter/command expansion: escaped `$` and backtick stay
    // literal in Bash as well as in Sushline's opt-in spelling mode.
    for (raw, expected) in [
        (r"A\\B> ", r"A\B> "),
        (r#"A\"B> "#, "A\"B> "),
        (r"A\$B> ", "A$B> "),
        (r"A\`B> ", "A`B> "),
        (r"A\\eB> ", r"A\eB> "),
        (r"A\\\eB> ", "A\\\x1bB> "),
        (r"A\\\\eB> ", r"A\eB> "),
    ] {
        let bash = run_bash(raw);
        assert!(
            bash.contains(expected),
            "raw={raw:?}, expected={expected:?}: {bash:?}"
        );
        assert!(bash.contains("\u{1f}PROMPTDONE"), "raw={raw:?}: {bash:?}");

        let sushline = run_sushline(raw, true);
        assert!(
            sushline.contains(expected),
            "raw={raw:?}, expected={expected:?}: {sushline:?}"
        );
        assert!(
            sushline.contains("SUSHLINE_ACCEPTED:"),
            "raw={raw:?}: {sushline:?}"
        );
        assert_eq!(
            accepted_line(&sushline),
            Some("printf '\\x1fPROMPTDONE\\n'".to_string()),
            "raw={raw:?}: {sushline:?}"
        );
    }
}
