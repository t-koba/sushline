use super::pty::*;
use portable_pty::{CommandBuilder, PtySize};

// Deliberately ASCII-only screen replay for these PTY observations, not a
// Unicode terminal emulator. Compare visible cells/cursor, not redisplay bytes:
// Bash incrementally updates the screen and Sushline repaints the viewport.
fn screen(output: &str, columns: usize) -> (String, usize) {
    let mut rows = vec![vec![b' '; columns]];
    let (mut row, mut col) = (0_usize, 0_usize);
    let mut bytes = output.bytes().peekable();
    while let Some(byte) = bytes.next() {
        if byte == 0x1b {
            assert_eq!(bytes.next(), Some(b'['), "unsupported escape: {output:?}");
            let mut params = String::new();
            let op = loop {
                let byte = bytes.next().expect("complete CSI");
                if (b'@'..=b'~').contains(&byte) {
                    break byte;
                }
                params.push(byte as char);
            };
            let n = params.parse::<usize>().unwrap_or(1).max(1);
            match op {
                b'G' => col = n - 1,
                b'A' => row = row.saturating_sub(n),
                b'B' => row += n,
                b'C' => col += n,
                b'D' => col = col.saturating_sub(n),
                b'K' | b'J' => {
                    rows[row][col.min(columns)..].fill(b' ');
                    if op == b'J' {
                        rows.truncate(row + 1);
                    }
                }
                b'@' => {
                    for _ in 0..n {
                        rows[row].insert(col, b' ');
                        rows[row].pop();
                    }
                }
                b'P' => {
                    for _ in 0..n {
                        rows[row].remove(col);
                        rows[row].push(b' ');
                    }
                }
                b'h' | b'l' | b'm' => {}
                _ => panic!("unsupported CSI {params}{op:?}: {output:?}"),
            }
        } else {
            match byte {
                b'\r' => col = 0,
                b'\n' => row += 1,
                b'\x08' => col = col.saturating_sub(1),
                b' '..=b'~' => {
                    if col >= columns {
                        row += 1;
                        col = 0;
                    }
                    rows.resize_with(row.max(rows.len() - 1) + 1, || vec![b' '; columns]);
                    rows[row][col] = byte;
                    col += 1;
                }
                b'\x07' => {}
                _ => panic!("non-ASCII screen byte {byte}: {output:?}"),
            }
        }
        if rows.len() <= row {
            rows.resize(row + 1, vec![b' '; columns]);
        }
    }
    (
        String::from_utf8(rows[row].clone())
            .unwrap()
            .trim_end()
            .to_string(),
        col,
    )
}

fn checkpoints(bash: bool, columns: u16, prompt: &str, steps: &[Vec<u8>]) -> Vec<(String, usize)> {
    let dir = tempfile::tempdir().unwrap();
    let inputrc = "set horizontal-scroll-mode on\nset enable-bracketed-paste off\n";
    let mut command = if bash {
        let path = dir.path().join("inputrc");
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
    command.env("LC_ALL", "C");
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
    .iter()
    .map(|out| screen(out, columns as usize))
    .collect()
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
