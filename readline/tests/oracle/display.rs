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
