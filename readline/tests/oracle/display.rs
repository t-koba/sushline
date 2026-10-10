#![allow(unused_imports)]
use super::pty::*;
use portable_pty::PtySize;
use readline::History;
use std::fs;
use std::process::Command;

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
