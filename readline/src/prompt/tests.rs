use super::*;
use unicode_width::UnicodeWidthStr;

fn shell(raw: &str) -> Prompt {
    Prompt::with_escapes(raw, PromptEscapes::Shell)
}

#[test]
fn the_prompt_text_is_kept_as_given_by_default() {
    // GNU Readline draws the prompt it is given, and a shell expands its own
    // prompt escapes before the editor sees them, so the default keeps the
    // text literal.
    let raw = "A\\[\\B\\e[31m\\C> ";
    let p = Prompt::new(raw);
    assert_eq!(p.visible(), raw);
    assert_eq!(p.width(), raw.chars().count());
    assert_eq!(p.escapes(), PromptEscapes::None);
}

#[test]
fn the_shell_spelling_is_read_when_it_is_asked_for() {
    let p = shell("A\\[\\e[31m\\]C> ");
    assert_eq!(p.visible(), "A\x1b[31mC> ");
    assert_eq!(p.width(), 4);
    assert_eq!(p.escapes(), PromptEscapes::Shell);
}

#[test]
fn the_shell_spelling_reads_octal_escapes() {
    let p = shell("A\\101B> ");
    assert_eq!(p.visible(), "AAB> ");
    assert_eq!(p.width(), 5);
    let literal = Prompt::new("A\\101B> ");
    assert_eq!(literal.visible(), "A\\101B> ");
    assert_eq!(literal.width(), 8);
}

#[test]
fn readline_markers_are_read_in_both_spellings() {
    for p in [
        Prompt::new("\x01\x1b[31m\x02red> "),
        shell("\x01\x1b[31m\x02red> "),
    ] {
        assert_eq!(p.visible(), "\x1b[31mred> ");
        assert_eq!(p.width(), 5);
    }
}

#[test]
fn strips_non_printing_prompt_markers_for_width() {
    let p = shell("\\[\u{1b}[31m\\]red> \\[\u{1b}[0m\\]");
    assert_eq!(p.visible(), "\u{1b}[31mred> \u{1b}[0m");
    assert_eq!(p.width(), 5);
}

#[test]
fn multiline_prompt_width_uses_last_visible_line() {
    let p = Prompt::new("first line\nλ> ");
    assert_eq!(p.visible(), "first line\nλ> ");
    assert_eq!(p.width(), 3);
}

#[test]
fn prompt_from_bytes_handles_readline_markers_and_cjk_width() {
    let p = Prompt::from(b"\x01\x1b[32m\x02\xe5\xaf\xbf> \x01\x1b[0m\x02".to_vec());
    assert_eq!(p.visible(), "\x1b[32m寿> \x1b[0m");
    assert_eq!(p.width(), "寿> ".width());
}

#[test]
fn prompt_counts_soh_stx_nonprinting_markers() {
    let p = Prompt::new("\x01\x1b[31m\x02寿司> \x01\x1b[0m\x02");
    assert_eq!(p.visible(), "\x1b[31m寿司> \x1b[0m");
    assert_eq!(p.width(), "寿司> ".width());
}

#[test]
fn complex_sush_prompt_ignores_nonprinting_ansi_and_counts_emoji_width() {
    let p = shell(
        r"\[\033]2;u@h: ~/repo\007\]\[\033[01;32m\]u@h\[\033[00m\]:\[\033[01;36m\]main🌵\[\033[00m\]\[\033[01;35m\]~/repo\[\033[00m\](debug)🍣",
    );
    assert!(p.visible().starts_with("\x1b]2;u@h: ~/repo\x07"));
    assert!(p.visible().contains("\x1b[01;32m"));
    assert_eq!(p.width(), "u@h:main🌵~/repo(debug)🍣".width());
}

#[test]
fn unwrapped_csi_prompt_counts_escape_bytes_unless_explicitly_hidden() {
    let bare = Prompt::new("\x1b[31mP>\x1b[0m");
    assert_eq!(bare.visible(), "\x1b[31mP>\x1b[0m");
    assert_eq!(bare.width(), 11);
    for raw in [
        "\x01\x1b[31m\x02P>\x01\x1b[0m\x02",
        r"\[\e[31m\]P>\[\e[0m\]",
    ] {
        let marked = shell(raw);
        assert_eq!(marked.visible(), bare.visible());
        assert_eq!(marked.width(), 2);
    }
    for raw in [r"\e[31mP>\e[0m", r"\033[31mP>\033[0m"] {
        let decoded = shell(raw);
        assert_eq!(decoded.visible(), bare.visible());
        assert_eq!(decoded.width(), bare.width());
    }
    // Actual terminal output still occupies only two cells, with no wrap.
    assert_eq!(crate::width::visible_width(bare.visible()), 2);
    assert_eq!(
        crate::width::measured_rows_for_output(bare.visible(), 10),
        (0, false)
    );
}

#[test]
fn unwrapped_csi_accounting_uses_only_the_last_prompt_line() {
    assert_eq!(Prompt::new("\x1b[31mfirst\x1b[0m\nP>").width(), 2);
    assert_eq!(Prompt::new("first\n\x1b[31mP>\x1b[0m").width(), 11);
    assert_eq!(Prompt::new("first\x01hidden\n\x02P>").width(), 2);
}

#[test]
fn last_line_width_ignores_earlier_lines_and_ansi() {
    assert_eq!(crate::width::last_line_width("ab\ncde"), 3);
    assert_eq!(crate::width::last_line_width("\x1b[31mab\x1b[0m\ncde"), 3);
    assert_eq!(crate::width::last_line_width(""), 0);
}

#[test]
fn generic_hidden_markers_are_zero_width_for_any_bytes() {
    let p = Prompt::new("\x01ABC\x02hi");
    assert_eq!(p.visible(), "ABChi");
    assert_eq!(p.width(), 2);
    let q = shell("\\[ABC\\]hi");
    assert_eq!(q.visible(), "ABChi");
    assert_eq!(q.width(), 2);
}

#[test]
fn control_chars_measure_zero_width() {
    assert_eq!(crate::width::char_width('\x00'), 0);
    assert_eq!(crate::width::char_width('\n'), 0);
    assert_eq!(crate::width::char_width('\x1b'), 0);
    assert_eq!(crate::width::char_width('\x7f'), 0);
}

#[test]
fn prefix_width_applies_only_without_prompt_newline() {
    assert_eq!(Prompt::new("12\n345").width_after_prefix(99), 3);
    assert_eq!(Prompt::new("XY").width_after_prefix(3), 5);
}

#[test]
fn the_shell_spelling_reads_the_escapes_the_shell_reads() {
    // Measured against GNU Bash 5.3: the shell decodes these sequences
    // before the editor sees the prompt.
    assert_eq!(shell("A\\\\B> ").visible(), "A\\B> ");
    assert_eq!(shell("A\\\"B> ").visible(), "A\"B> ");
    assert_eq!(shell("A\\$B> ").visible(), "A$B> ");
    assert_eq!(shell("A\\`B> ").visible(), "A`B> ");
    assert_eq!(shell("A\\aB> ").visible(), "A\u{7}B> ");
    assert_eq!(shell("A\\eB> ").visible(), "A\u{1b}B> ");
    assert_eq!(shell("A\\nB> ").visible(), "A\nB> ");
    assert_eq!(shell("A\\rB> ").visible(), "A\rB> ");
}

#[test]
fn the_shell_spelling_leaves_the_escapes_the_shell_leaves() {
    // Measured against GNU Bash 5.3: an escape the shell does not decode
    // stays as it was written, the backslash included.
    assert_eq!(shell("A\\qB> ").visible(), "A\\qB> ");
    assert_eq!(shell("A\\EB> ").visible(), "A\\EB> ");
    assert_eq!(shell("A\\bB> ").visible(), "A\\bB> ");
    assert_eq!(shell("A\\%B> ").visible(), "A\\%B> ");
}

#[test]
fn the_shell_spelling_reads_exactly_three_octal_digits() {
    // Measured against GNU Bash 5.3: the shell decodes an octal escape only
    // when three octal digits follow, and the value is one byte.
    assert_eq!(shell("A\\101B> ").visible(), "AAB> ");
    assert_eq!(shell("A\\012B> ").visible(), "A\nB> ");
    assert_eq!(shell("A\\007B> ").visible(), "A\u{7}B> ");
    // Fewer than three digits stay as they were written.
    assert_eq!(shell("A\\1B> ").visible(), "A\\1B> ");
    assert_eq!(shell("A\\12B> ").visible(), "A\\12B> ");
    assert_eq!(shell("A\\0B> ").visible(), "A\\0B> ");
    // The shell writes one byte, 0xff. A UTF-8 prompt string cannot hold a lone byte
    // above 0x7f, so the value is carried as the character with that code point.
    assert_eq!(shell("A\\377B> ").visible(), "A\u{ff}B> ");
    // A NUL byte cannot be in the shell's prompt.
    assert_eq!(shell("A\\000B> ").visible(), "AB> ");
}

#[test]
fn the_shell_spelling_reads_the_sequences_the_host_reads() {
    // Measured against GNU Bash 5.3: the shell reads these sequences in a
    // prompt, and the two passes it reads them in are visible in the
    // backslash runs.
    assert_eq!(shell("A\\\\B> ").visible(), "A\\B> ");
    assert_eq!(shell("A\\$B> ").visible(), "A$B> ");
    assert_eq!(shell("A\\\"B> ").visible(), "A\"B> ");
    assert_eq!(shell("A\\`B> ").visible(), "A`B> ");
    // Two backslashes are read as one, and the character after them is left
    // as it was written, because the escapes are read before the quoting is
    // removed.
    assert_eq!(shell("A\\\\eB> ").visible(), "A\\eB> ");
    assert_eq!(shell("A\\\\\\eB> ").visible(), "A\\\u{1b}B> ");
    assert_eq!(shell("A\\\\\\\\eB> ").visible(), "A\\eB> ");
}

#[test]
fn the_shell_spelling_reads_a_run_of_backslashes_before_a_quoting_character() {
    // Measured against GNU Bash 5.3: a run of backslashes before a quoting
    // character keeps the backslash the shell keeps, and the quoting the
    // shell's expansion removes is removed from the character that follows.
    // The shell then expands what is left, so the text after the run is the
    // shell's expansion of it; the Shell mode leaves it as it was written.
    assert_eq!(shell("A\\$B> ").visible(), "A$B> ");
    assert_eq!(shell("A\\\\$B> ").visible(), "A$B> ");
    assert_eq!(shell("A\\\\\\$B> ").visible(), "A\\$B> ");
    assert_eq!(shell("A\\\\\\\\$B> ").visible(), "A\\$B> ");
}
