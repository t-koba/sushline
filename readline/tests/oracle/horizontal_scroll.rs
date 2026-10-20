//! Horizontal-scroll compatibility cases compare screen/cursor outcomes across
//! tested widths, input batches, and locales. The replay helper is a partial
//! screen model, so claims remain limited to the fixtures in `COMPATIBILITY.md`.

use super::pty::*;
use portable_pty::{CommandBuilder, PtySize};
use unicode_width::UnicodeWidthChar;

// Cell-width screen replay for tested ASCII and Unicode cases, not a full
// terminal emulator. It ignores SGR and attaches zero-width marks to the last
// occupied cell. Compare cells/cursor, not redisplay bytes: Bash updates incrementally.
fn screen(output: &str, columns: usize) -> (String, usize) {
    let mut rows = vec![vec![String::from(" "); columns]];
    let (mut row, mut col) = (0_usize, 0_usize);
    let mut chars = output.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            assert_eq!(chars.next(), Some('['), "unsupported escape: {output:?}");
            let mut params = String::new();
            let op = loop {
                let ch = chars.next().expect("complete CSI");
                if ('@'..='~').contains(&ch) {
                    break ch;
                }
                params.push(ch);
            };
            let n = params.parse::<usize>().unwrap_or(1).max(1);
            match op {
                'G' => col = n - 1,
                'A' => row = row.saturating_sub(n),
                'B' => row += n,
                'C' => col += n,
                'D' => col = col.saturating_sub(n),
                'K' | 'J' => {
                    for cell in &mut rows[row][col.min(columns)..] {
                        *cell = String::from(" ");
                    }
                    if op == 'J' {
                        rows.truncate(row + 1);
                    }
                }
                '@' => {
                    for _ in 0..n {
                        rows[row].insert(col, String::from(" "));
                        rows[row].pop();
                    }
                }
                'P' => {
                    for _ in 0..n {
                        rows[row].remove(col);
                        rows[row].push(String::from(" "));
                    }
                }
                'h' | 'l' | 'm' => {}
                _ => panic!("unsupported CSI {params}{op:?}: {output:?}"),
            }
        } else {
            match ch {
                '\r' => col = 0,
                '\n' => row += 1,
                '\x08' => col = col.saturating_sub(1),
                '\x07' => {}
                ch if !ch.is_control() => {
                    let width = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if width == 0 {
                        let mut base = col.min(columns).saturating_sub(1);
                        while base > 0 && rows[row][base].is_empty() {
                            base -= 1;
                        }
                        if !rows[row][base].is_empty() {
                            rows[row][base].push(ch);
                        }
                        continue;
                    }
                    if col + width > columns {
                        row += 1;
                        col = 0;
                    }
                    if rows.len() <= row {
                        rows.resize_with(row + 1, || vec![String::from(" "); columns]);
                    }
                    rows[row][col] = ch.to_string();
                    for continuation in 1..width {
                        rows[row][col + continuation].clear();
                    }
                    col += width;
                }
                _ => panic!("nonprinting screen character {ch:?}: {output:?}"),
            }
        }
        if rows.len() <= row {
            rows.resize_with(row + 1, || vec![String::from(" "); columns]);
        }
    }
    (rows[row].concat().trim_end().to_string(), col)
}

fn checkpoints(bash: bool, columns: u16, prompt: &str, steps: &[Vec<u8>]) -> Vec<(String, usize)> {
    checkpoints_with_locale(bash, columns, prompt, steps, "C")
}

fn checkpoints_with_locale(
    bash: bool,
    columns: u16,
    prompt: &str,
    steps: &[Vec<u8>],
    locale: &str,
) -> Vec<(String, usize)> {
    checkpoint_outputs_with_locale(bash, columns, prompt, steps, locale)
        .iter()
        .map(|out| screen(out, columns as usize))
        .collect()
}

fn horizontal_scroll_command(
    bash: bool,
    prompt: &str,
    dir: &std::path::Path,
    locale: &str,
) -> CommandBuilder {
    let inputrc =
        "set horizontal-scroll-mode on\nset enable-bracketed-paste off\nset bell-style audible\n";
    let mut command = if bash {
        let path = dir.join("inputrc");
        std::fs::write(&path, inputrc).unwrap();
        let mut command = CommandBuilder::new("bash");
        command.env("INPUTRC", path.to_str().unwrap());
        command.args([
            "--noprofile",
            "--norc",
            "-i",
            "-c",
            &format!("IFS= read -r -e -p {} line", shell_single_quote(prompt)),
        ]);
        command
    } else {
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_sushline-harness"));
        command.env("SUSHLINE_INPUTRC", inputrc);
        command.env("SUSHLINE_PROMPT", prompt);
        command.env("SUSHLINE_HISTORY", "");
        command
    };
    command.env("LC_ALL", locale);
    command
}

fn checkpoint_outputs_with_locale(
    bash: bool,
    columns: u16,
    prompt: &str,
    steps: &[Vec<u8>],
    locale: &str,
) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let command = horizontal_scroll_command(bash, prompt, dir.path(), locale);
    let wait_prompt = prompt.replace(['\x01', '\x02'], "");
    run_pty_checkpoints(
        command,
        &wait_prompt,
        steps,
        PtySize {
            rows: 24,
            cols: columns,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
}

/// The checkpoint frames as raw bytes, so a caller can observe output that
/// is not valid UTF-8.
fn checkpoint_outputs_raw_with_locale(
    bash: bool,
    columns: u16,
    prompt: &str,
    steps: &[Vec<u8>],
    locale: &str,
) -> Vec<Vec<u8>> {
    let dir = tempfile::tempdir().unwrap();
    let command = horizontal_scroll_command(bash, prompt, dir.path(), locale);
    let wait_prompt = prompt.replace(['\x01', '\x02'], "");
    run_pty_checkpoints_raw(
        command,
        &wait_prompt,
        steps,
        PtySize {
            rows: 24,
            cols: columns,
            pixel_width: 0,
            pixel_height: 0,
        },
    )
}

#[test]
fn bash_readline_and_sushline_horizontal_scroll_prompt_and_ascii_point_movement() {
    let mut steps = vec![vec![b'a']; 90];
    steps.push(vec![1]); // beginning-of-line restores the prompt
    for bash in [true, false] {
        let frames = checkpoints(bash, 80, READY_PROMPT, &steps);
        // Both bytewise trajectories start scrolling at logical width 78.
        assert_eq!(
            frames[89],
            (format!("<{}", "a".repeat(52)), 53),
            "bash={bash}"
        );
        assert_eq!(
            frames[90],
            (format!("{READY_PROMPT}{}>", "a".repeat(63)), 15),
            "bash={bash}"
        );
    }
    let mut steps = vec![vec![b'a'; 30], vec![1]];
    steps.extend(vec![vec![6]; 12]); // forward-char across the right edge
    steps.push(vec![1]);
    for bash in [true, false] {
        let frames = checkpoints(bash, 12, "P>", &steps);
        assert_eq!(
            frames[1],
            (format!("P>{}>", "a".repeat(8)), 2),
            "bash={bash}"
        );
        assert_eq!(
            frames[9],
            (format!("<{}>", "a".repeat(9)), 6),
            "bash={bash}"
        );
        assert_eq!(
            frames[13],
            (format!("<{}>", "a".repeat(9)), 6),
            "bash={bash}"
        );
        assert_eq!(frames[14], frames[1], "bash={bash}");
    }
}

#[test]
fn bash_readline_and_sushline_horizontal_scroll_retains_margin_between_batches() {
    let text = "0123456789".repeat(10);
    for chunk in [1, 10, 100] {
        let mut steps: Vec<Vec<u8>> = text.as_bytes().chunks(chunk).map(<[u8]>::to_vec).collect();
        let end = steps.len() - 1;
        steps.push(vec![1]);
        steps.push(vec![5]);
        for bash in [true, false] {
            let frames = checkpoints(bash, 80, "P>", &steps);
            // The built-in Sushline Terminal currently reads one byte per
            // event, even when the PTY writer sends a batch. The viewport
            // itself honors batched TerminalEvents (see display.rs), but
            // changing terminal read granularity is outside this display fix.
            let (start, col) = match if bash { chunk } else { 1 } {
                1 => (25, 76),
                10 => (29, 72),
                _ => (49, 52),
            };
            assert_eq!(
                frames[end],
                (format!("<{}", &text[start..]), col),
                "bash={bash}, chunk={chunk}"
            );
            assert_eq!(
                frames[end + 1],
                (format!("P>{}>", &text[..76]), 2),
                "bash={bash}, chunk={chunk}"
            );
            assert_eq!(
                frames[end + 2],
                (format!("<{}", &text[49..]), 52),
                "bash={bash}, chunk={chunk}"
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_horizontal_scroll_counts_hidden_ansi_prompt() {
    let prompt = "\x01\x1b[31m\x02P>\x01\x1b[0m\x02";
    let steps = vec![vec![b'a'; 90]];
    for bash in [true, false] {
        let plain = checkpoints(bash, 80, "P>", &steps);
        let ansi = checkpoints(bash, 80, prompt, &steps);
        assert_eq!(ansi, plain, "bash={bash}");
    }
}

#[test]
fn bash_readline_and_sushline_horizontal_scroll_exposes_unwrapped_ansi_prompt_width() {
    let prompt = "\x1b[31mP>\x1b[0m";
    // Bytewise typing proves logical-width alignment: unmarked CSI bytes
    // count as prompt columns, so both implementations clip to the same
    // window. The single-batch trajectory still diverges because Sushline
    // reads the batch bytewise while Bash scrolls it as one batch; that
    // granularity gap is covered separately and is not an ANSI regression.
    let bytewise: Vec<Vec<u8>> = vec![vec![b'a']; 90];
    for bash in [true, false] {
        let frames = checkpoints(bash, 80, prompt, &bytewise);
        assert_eq!(
            frames.last(),
            Some(&(format!("<{}", "a".repeat(74)), 75)),
            "bash={bash} bytewise"
        );
    }
    let batch = vec![vec![b'a'; 90]];
    let bash = checkpoints(true, 80, prompt, &batch);
    let sushline = checkpoints(false, 80, prompt, &batch);
    assert_eq!(
        bash.last(),
        Some(&(format!("<{}", "a".repeat(51)), 52)),
        "Bash batch"
    );
    assert_eq!(
        sushline.last(),
        Some(&(format!("<{}", "a".repeat(74)), 75)),
        "Sushline batch reads bytewise"
    );
}

#[test]
fn bash_and_sushline_horizontal_scroll_agree_on_a_wide_character_tail() {
    if !c_utf8_locale_available() {
        return;
    }
    // The window is measured in bytes on both sides, so the wide glyph is
    // clipped as a whole. GNU's batched path shows the last 51 bytes, while
    // Sushline's bytewise path keeps the tail visible: the built-in
    // terminal reads one byte per event, and that batching difference is
    // covered on its own.
    let text = format!("{}中{}", "a".repeat(24), "b".repeat(64));
    let steps = vec![text.into_bytes()];
    let bash = checkpoints_with_locale(true, 80, "P>", &steps, "C.UTF-8");
    let sushline = checkpoints_with_locale(false, 80, "P>", &steps, "C.UTF-8");

    assert_eq!(
        bash.last(),
        Some(&(format!("<{}", "b".repeat(51)), 52)),
        "Bash"
    );
    assert_eq!(
        sushline.last(),
        Some(&(format!("<{}", "b".repeat(64)), 65)),
        "Sushline"
    );
}

#[test]
fn bash_readline_and_sushline_horizontal_scroll_keeps_clipped_combining_marks_with_base() {
    if !c_utf8_locale_available() {
        return;
    }
    let marked = format!("{}e\u{301}{}", "a".repeat(24), "b".repeat(65));
    let plain = format!("{}e{}", "a".repeat(24), "b".repeat(65));
    let marked_steps = vec![marked.into_bytes()];
    let plain_steps = vec![plain.into_bytes()];

    for bash in [true, false] {
        let marked_output =
            checkpoint_outputs_with_locale(bash, 80, "P>", &marked_steps, "C.UTF-8");
        let plain_output = checkpoint_outputs_with_locale(bash, 80, "P>", &plain_steps, "C.UTF-8");
        let marked_screen = screen(marked_output.last().unwrap(), 80);
        let plain_screen = screen(plain_output.last().unwrap(), 80);

        assert_eq!(marked_screen, plain_screen, "bash={bash}");
        assert!(!marked_screen.0.contains('e'), "bash={bash}");
        assert!(!marked_screen.0.contains('\u{301}'), "bash={bash}");
    }
}

#[test]
fn bash_and_sushline_horizontal_scroll_differs_for_visible_combining_mark() {
    if !c_utf8_locale_available() {
        return;
    }
    let marked = format!("{}{}e\u{301}", "a".repeat(24), "b".repeat(64));
    let plain = format!("{}{}e", "a".repeat(24), "b".repeat(64));
    let marked_steps = vec![marked.into_bytes()];
    let plain_steps = vec![plain.into_bytes()];

    for bash in [true, false] {
        let marked_screen = checkpoints_with_locale(bash, 80, "P>", &marked_steps, "C.UTF-8")
            .pop()
            .unwrap();
        let plain_screen = checkpoints_with_locale(bash, 80, "P>", &plain_steps, "C.UTF-8")
            .pop()
            .unwrap();

        assert!(marked_screen.0.ends_with("e\u{301}"), "bash={bash}");
        assert!(plain_screen.0.ends_with('e'), "bash={bash}");
        let marked_b = marked_screen.0.matches('b').count();
        let plain_b = plain_screen.0.matches('b').count();
        if bash {
            assert_eq!(marked_screen.1, 50);
            assert_eq!(plain_screen.1, 52);
            assert_eq!(plain_b, marked_b + 2);
        } else {
            assert_eq!(marked_screen.1, 65);
            assert_eq!(plain_screen.1, 65);
            assert_eq!(marked_b, plain_b);
            assert_eq!(
                marked_screen
                    .0
                    .strip_suffix('\u{301}')
                    .expect("mark suffix"),
                plain_screen.0
            );
        }
    }
}

#[test]
fn bash_and_sushline_horizontal_scroll_exposes_ambiguous_width_difference() {
    if !c_utf8_locale_available() {
        return;
    }
    let ambiguous = format!("{}{}·", "a".repeat(24), "b".repeat(64));
    let plain = format!("{}{}x", "a".repeat(24), "b".repeat(64));
    let ambiguous_steps = vec![ambiguous.into_bytes()];
    let plain_steps = vec![plain.into_bytes()];

    for bash in [true, false] {
        let ambiguous_screen = checkpoints_with_locale(bash, 80, "P>", &ambiguous_steps, "C.UTF-8")
            .pop()
            .unwrap();
        let plain_screen = checkpoints_with_locale(bash, 80, "P>", &plain_steps, "C.UTF-8")
            .pop()
            .unwrap();
        assert!(ambiguous_screen.0.ends_with('·'), "bash={bash}");
        assert!(plain_screen.0.ends_with('x'), "bash={bash}");
        let ambiguous_b = ambiguous_screen.0.matches('b').count();
        let plain_b = plain_screen.0.matches('b').count();

        if bash {
            assert_eq!(ambiguous_screen.1, 51);
            assert_eq!(plain_screen.1, 52);
            assert_eq!(plain_b, ambiguous_b + 1);
        } else {
            assert_eq!(ambiguous_screen.1, 65);
            assert_eq!(plain_screen.1, 65);
            assert_eq!(ambiguous_b, plain_b);
            assert_eq!(
                ambiguous_screen
                    .0
                    .strip_suffix('·')
                    .expect("ambiguous suffix"),
                plain_screen.0.strip_suffix('x').expect("plain suffix")
            );
        }
    }
}

#[test]
fn bash_readline_and_sushline_horizontal_scroll_reset_after_deletion() {
    let text = "0123456789".repeat(10);
    let mut steps: Vec<_> = text.bytes().map(|byte| vec![byte]).collect();
    steps.push(vec![5]); // Ctrl-E, then eighty Backspaces
    steps.extend(vec![vec![0x7f]; 26]); // lengths 99 through 74, bytewise
    steps.push(vec![0x7f; 54]); // length 20
    for bash in [true, false] {
        let frames = checkpoints(bash, 80, "P>", &steps);
        // With the two-cell prompt, lengths 75/76/77 have logical widths
        // 77/78/79. Activation is at 78, restoration only at 76.
        assert_eq!(
            frames[74],
            (format!("P>{}", &text[..75]), 77),
            "bash={bash}"
        );
        assert_eq!(
            frames[75],
            (format!("<{}", &text[25..76]), 52),
            "bash={bash}"
        );
        assert_eq!(
            frames[76],
            (format!("<{}", &text[25..77]), 53),
            "bash={bash}"
        );
        for len in [77, 76, 75] {
            assert_eq!(
                frames[100 + (100 - len)],
                (format!("<{}", &text[25..len]), len + 2 - 26),
                "bash={bash}, len={len}"
            );
        }
        assert_eq!(
            frames[126],
            (format!("P>{}", &text[..74]), 76),
            "bash={bash}"
        );
        assert_eq!(
            frames.last(),
            Some(&(format!("P>{}", &text[..20]), 22)),
            "bash={bash}"
        );
    }
}

#[test]
fn bash_readline_and_sushline_horizontal_scroll_keeps_long_line_history_below_edge() {
    let text = "0123456789".repeat(10);
    for bytewise in [true, false] {
        let mut steps = if bytewise {
            let mut steps: Vec<_> = text.bytes().map(|byte| vec![byte]).collect();
            steps.push(vec![5]);
            steps
        } else {
            // Ctrl-A/Ctrl-E normalizes the PTY batch/bytewise input difference
            // to margin 50; bytewise typing without Ctrl-A has margin 26.
            vec![text.as_bytes().to_vec(), vec![1], vec![5]]
        };
        let end = steps.len() - 1;
        let margin = if bytewise { 26 } else { 50 };
        // These are buffer points; the prompt adds two logical cells.
        steps.extend(vec![vec![2]; 50]);
        for bash in [true, false] {
            let frames = checkpoints(bash, 80, "P>", &steps);
            for point in [77, 76, 75] {
                assert_eq!(
                    frames[end + (100 - point)],
                    (format!("<{}", &text[margin - 1..]), point + 2 - margin),
                    "bash={bash}, bytewise={bytewise}, point={point}"
                );
            }
            for point in [margin + 24, 50] {
                assert_eq!(
                    frames[end + (100 - point)],
                    (format!("P>{}>", &text[..76]), point + 2),
                    "bash={bash}, bytewise={bytewise}, point={point}"
                );
            }
        }
    }
}

#[test]
fn bash_and_sushline_checkpoint_barriers_follow_complete_key_redisplay() {
    // Includes batched insertion, cursor-only movement and a no-op Ctrl-E.
    // No step itself bells, so only abort can acknowledge it.
    let steps = [b"a".to_vec(), b"bc".to_vec(), vec![2], vec![5], vec![5]];
    for bash in [true, false] {
        let outputs = checkpoint_outputs_with_locale(bash, 80, "P>", &steps, "C");
        let frames: Vec<_> = outputs.iter().map(|out| screen(out, 80)).collect();
        assert_eq!(
            frames,
            [
                ("P>a", 3),
                ("P>abc", 5),
                ("P>abc", 4),
                ("P>abc", 5),
                ("P>abc", 5)
            ]
            .map(|(text, col)| (text.to_owned(), col)),
            "bash={bash}"
        );
        assert!(outputs.iter().all(|out| !out.contains('\x07')));
    }
}

#[test]
fn bash_horizontal_scroll_window_holds_the_last_line_bytes() {
    if !c_utf8_locale_available() {
        return;
    }
    // GNU Readline measures the horizontal-scroll window in bytes: at 80
    // columns the visible text is the last 51 bytes of the line, and the
    // cursor sits one column past the window's column width. A multibyte
    // tail therefore shows fewer columns than the byte count, and the
    // window starts later in the line than a column-based window would.
    let base = "0123456789".repeat(10);
    let cases: [(&str, usize, usize); 5] = [
        ("e", 50, 52),
        ("e\u{301}", 52, 50),
        ("\u{e9}", 51, 51),
        ("\u{b7}", 51, 51),
        ("\u{4e2d}", 52, 51),
    ];
    for (tail, start, cursor) in cases {
        let text = format!("{base}{tail}");
        let frames = checkpoints_with_locale(true, 80, "P>", &[text.into_bytes()], "C.UTF-8");
        let (screen, col) = frames.last().cloned().expect("checkpoint");
        assert_eq!(
            screen,
            format!("<{}{}", &base[start..], tail),
            "tail={tail:?}"
        );
        assert_eq!(col, cursor, "tail={tail:?}");
    }
}

#[test]
fn bash_and_sushline_horizontal_scroll_bytewise_window_counts_bytes() {
    if !c_utf8_locale_available() {
        return;
    }
    // GNU counts the scroll offset in bytes in every path, and so does
    // Sushline. The bytewise path scrolls to keep the cursor at the window's
    // right edge, so an ASCII tail fills the screen, while a multibyte tail
    // leaves about 50 columns visible and can leave the cursor inside the
    // window.
    let base = "0123456789".repeat(10);
    let window = |start: usize, tail: &str| format!("<{}{tail}", &base[start..]);
    let cases: [(&str, String, usize); 4] = [
        ("e", window(25, "e"), 77),
        ("e\u{301}", window(51, "e\u{301}"), 51),
        ("\u{e9}", window(51, "\u{e9}"), 51),
        ("\u{b7}", window(51, "\u{b7}"), 51),
    ];
    for (tail, expected_screen, expected_column) in cases {
        let text = format!("{base}{tail}");
        let steps: Vec<Vec<u8>> = text.as_bytes().iter().map(|byte| vec![*byte]).collect();
        for bash in [true, false] {
            let frames = checkpoints_with_locale(bash, 80, "P>", &steps, "C.UTF-8");
            let (screen, column) = frames.last().cloned().expect("checkpoint");
            assert_eq!(screen, expected_screen, "tail={tail:?} bash={bash}");
            assert_eq!(column, expected_column, "tail={tail:?} bash={bash}");
        }
    }
}

#[test]
fn bash_window_edge_splits_a_multibyte_character_that_sushline_keeps_valid() {
    if !c_utf8_locale_available() {
        return;
    }
    // The window edge falls inside the wide glyph, so GNU writes the
    // glyph's trailing bytes without its lead byte while Sushline clips the
    // cell and keeps its output valid UTF-8. The batched path keeps the
    // echoed line and the redisplay apart, so only the split makes the
    // frame invalid UTF-8.
    let text = format!("{}中{}", "0".repeat(50), "1".repeat(49));
    let steps = vec![text.into_bytes()];
    let bash = checkpoint_outputs_raw_with_locale(true, 80, "P>", &steps, "C.UTF-8");
    let sushline = checkpoint_outputs_raw_with_locale(false, 80, "P>", &steps, "C.UTF-8");

    let bash_frame = bash.last().expect("bash frame");
    let sushline_frame = sushline.last().expect("sushline frame");
    assert!(
        std::str::from_utf8(bash_frame).is_err(),
        "GNU splits the glyph: {bash_frame:?}"
    );
    assert!(
        std::str::from_utf8(sushline_frame).is_ok(),
        "Sushline keeps valid UTF-8: {sushline_frame:?}"
    );
}
