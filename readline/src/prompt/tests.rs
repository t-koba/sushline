use super::*;
use unicode_width::UnicodeWidthStr;

#[test]
fn strips_non_printing_prompt_markers_for_width() {
    let p = Prompt::new("\\[\u{1b}[31m\\]red> \\[\u{1b}[0m\\]");
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
    let p = Prompt::from(b"\\[\x1b[32m\\]\xe5\xaf\xbf> \\[\x1b[0m\\]".to_vec());
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
    let p = Prompt::new(
        r"\[\033]2;u@h: ~/repo\007\]\[\033[01;32m\]u@h\[\033[00m\]:\[\033[01;36m\]main🌵\[\033[00m\]\[\033[01;35m\]~/repo\[\033[00m\](debug)🍣",
    );
    assert!(p.visible().starts_with("\x1b]2;u@h: ~/repo\x07"));
    assert!(p.visible().contains("\x1b[01;32m"));
    assert_eq!(p.width(), "u@h:main🌵~/repo(debug)🍣".width());
}
