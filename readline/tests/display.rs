mod common;

use common::MemoryTerminal;
use readline::{Config, Editor, History, Prompt, ReadlineResult, TerminalEvent, TerminalSize};

#[test]
fn mark_modified_lines_adds_prompt_marker_for_changed_history_line() {
    let mut history = History::new();
    history.push("abcdef");
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x10]),
        TerminalEvent::Bytes(b"Z".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str("set mark-modified-lines on").unwrap();
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("abcdefZ".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("*> abcdefZ"));
}

#[test]
fn horizontal_scroll_mode_renders_window_around_point() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdefghijkl".to_vec()),
        TerminalEvent::Bytes(vec![1]),
        TerminalEvent::Bytes(vec![5]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 8;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str("set horizontal-scroll-mode on")
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert!(line.terminal().out.contains("<jkl"));
    assert!(line.terminal().out.contains("> abcd>"));
    assert_eq!(line.terminal().moved_columns.last(), Some(&4));
    assert!(!line.terminal().out.contains("> abcdefghijkl"));
}

#[test]
fn horizontal_scroll_mode_retains_margin_between_render_events() {
    let text = "0123456789".repeat(10);
    for chunk in [1, 10, 100] {
        let mut events: Vec<_> = text
            .as_bytes()
            .chunks(chunk)
            .map(|bytes| TerminalEvent::Bytes(bytes.to_vec()))
            .collect();
        events.push(TerminalEvent::Bytes(vec![1]));
        events.push(TerminalEvent::Bytes(vec![5]));
        events.push(TerminalEvent::Bytes(vec![b'\r']));
        let terminal = MemoryTerminal::with_events(events);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set horizontal-scroll-mode on")
            .unwrap();
        assert_eq!(
            line.read_line(Prompt::new("P>"), &mut ()).unwrap(),
            ReadlineResult::Line(text.as_bytes().to_vec())
        );
        let end = text.len() / chunk;
        let col = match chunk {
            1 => 76,
            10 => 72,
            _ => 52,
        };
        // Two column moves per redisplay: return to origin, then place point.
        assert_eq!(line.terminal().moved_columns[2 * end + 1], col);
        assert_eq!(line.terminal().moved_columns[2 * (end + 1) + 1], 2);
        assert_eq!(line.terminal().moved_columns[2 * (end + 2) + 1], 52);
        assert!(line.terminal().out.contains(&format!("P>{}>", &text[..76])));
        assert!(line.terminal().out.contains(&format!("<{}", &text[49..])));
    }
}

#[test]
fn horizontal_scroll_mode_matches_bytewise_ascii_prompt_checkpoint() {
    let mut events = vec![TerminalEvent::Bytes(vec![b'a']); 90];
    events.push(TerminalEvent::Bytes(vec![b'\r']));
    let mut line = Editor::new(
        Config::default(),
        MemoryTerminal::with_events(events),
        History::new(),
    );
    line.load_inputrc_str("set horizontal-scroll-mode on")
        .unwrap();
    assert_eq!(
        line.read_line(Prompt::new("SUSHLINE_READY>"), &mut ())
            .unwrap(),
        ReadlineResult::Line(vec![b'a'; 90])
    );
    assert!(
        line.terminal()
            .out
            .ends_with(&format!("<{}\r\n", "a".repeat(52)))
    );
    assert_eq!(line.terminal().moved_columns.last(), Some(&53));
}

#[test]
fn horizontal_scroll_mode_restores_full_line_after_deletion() {
    let text = "0123456789".repeat(10);
    let mut events: Vec<_> = text
        .bytes()
        .map(|byte| TerminalEvent::Bytes(vec![byte]))
        .collect();
    events.push(TerminalEvent::Bytes(vec![5])); // Ctrl-E
    events.extend((0..80).map(|_| TerminalEvent::Bytes(vec![0x7f])));
    events.push(TerminalEvent::Bytes(vec![b'\r']));
    let mut line = Editor::new(
        Config::default(),
        MemoryTerminal::with_events(events),
        History::new(),
    );
    line.load_inputrc_str("set horizontal-scroll-mode on")
        .unwrap();
    assert_eq!(
        line.read_line(Prompt::new("P>"), &mut ()).unwrap(),
        ReadlineResult::Line(text.as_bytes()[..20].to_vec())
    );
    // Initial activation at width 78, before the line overflows the row.
    for (len, column) in [(75, 77), (76, 52), (77, 53)] {
        assert_eq!(line.terminal().moved_columns[2 * len + 1], column);
    }
    // Deletion retains the scrolled margin at widths 79/78/77. Width 76
    // restores the full prompt; merely fitting columns-1 is not enough.
    for len in [77, 76, 75, 74] {
        let column = if len >= 75 { len + 2 - 26 } else { len + 2 };
        let render = 101 + (100 - len);
        assert_eq!(
            line.terminal().moved_columns[2 * render + 1] as usize,
            column
        );
    }
    assert!(
        line.terminal()
            .out
            .contains(&format!("P>{}\r\n", &text[..20]))
    );
    assert_eq!(line.terminal().moved_columns.last(), Some(&22));
}

#[test]
fn horizontal_scroll_mode_keeps_long_line_history_below_activation_edge() {
    let text = "0123456789".repeat(10);
    let mut events = vec![
        TerminalEvent::Bytes(text.as_bytes().to_vec()),
        TerminalEvent::Bytes(vec![1]),
        TerminalEvent::Bytes(vec![5]),
    ];
    events.extend(vec![TerminalEvent::Bytes(vec![2]); 50]);
    events.push(TerminalEvent::Bytes(vec![b'\r']));
    let mut line = Editor::new(
        Config::default(),
        MemoryTerminal::with_events(events),
        History::new(),
    );
    line.load_inputrc_str("set horizontal-scroll-mode on")
        .unwrap();
    assert_eq!(
        line.read_line(Prompt::new("P>"), &mut ()).unwrap(),
        ReadlineResult::Line(text.as_bytes().to_vec())
    );
    for point in [77, 76, 75, 74, 50] {
        let column = if point >= 75 {
            point + 2 - 50
        } else {
            point + 2
        };
        let render = 3 + (100 - point);
        assert_eq!(
            line.terminal().moved_columns[2 * render + 1] as usize,
            column
        );
    }
    assert!(
        line.terminal()
            .out
            .ends_with(&format!("P>{}>\r\n", &text[..76]))
    );
}

#[test]
fn horizontal_scroll_mode_restores_full_line_after_widening() {
    let text = "0123456789".repeat(10);
    let mut events: Vec<_> = text
        .bytes()
        .map(|byte| TerminalEvent::Bytes(vec![byte]))
        .collect();
    events.push(TerminalEvent::Resize(TerminalSize {
        columns: 120,
        rows: 24,
    }));
    events.push(TerminalEvent::Bytes(vec![b'\r']));
    let mut line = Editor::new(
        Config::default(),
        MemoryTerminal::with_events(events),
        History::new(),
    );
    line.load_inputrc_str("set horizontal-scroll-mode on")
        .unwrap();
    assert_eq!(
        line.read_line(Prompt::new("P>"), &mut ()).unwrap(),
        ReadlineResult::Line(text.as_bytes().to_vec())
    );
    assert!(line.terminal().out.contains(&format!("P>{text}\r\n")));
    assert_eq!(line.terminal().moved_columns.last(), Some(&102));
}

#[test]
fn horizontal_scroll_mode_preserves_hidden_spans_in_the_mode_prefix() {
    for horizontal in [false, true] {
        let terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(b"abc".to_vec()),
            TerminalEvent::Bytes(vec![b'\r']),
        ]);
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str(&format!(
            "set horizontal-scroll-mode {}\nset show-mode-in-prompt on\nset emacs-mode-string \"\\001hidden\\002M\"",
            if horizontal { "on" } else { "off" }
        )).unwrap();
        assert_eq!(
            line.read_line(Prompt::new("P>"), &mut ()).unwrap(),
            ReadlineResult::Line(b"abc".to_vec())
        );
        assert!(line.terminal().out.contains("hiddenMP>abc"));
        assert_eq!(line.terminal().moved_columns[1], 3);
        assert_eq!(line.terminal().moved_columns.last(), Some(&6));
        assert!(!line.terminal().out.contains('\x01'));
        assert!(!line.terminal().out.contains('\x02'));
    }
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdefghi".to_vec()),
        TerminalEvent::Bytes(vec![1]),
        TerminalEvent::Bytes(vec![b'\r']),
    ]);
    terminal.columns = 12;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(
        "set horizontal-scroll-mode on\nset show-mode-in-prompt on\nset emacs-mode-string \"\\001hidden\\002M\"",
    ).unwrap();
    let _ = line.read_line(Prompt::new("P>"), &mut ()).unwrap();
    assert!(line.terminal().out.contains("hiddenMP>abcdefg>"));
    assert_eq!(line.terminal().moved_columns.last(), Some(&3));
}

#[test]
fn horizontal_scroll_mode_keeps_modified_marker_after_a_hidden_mode_prefix() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x10]),
        TerminalEvent::Bytes(vec![b'Z']),
        TerminalEvent::Bytes(vec![b'\r']),
    ]);
    let mut history = History::new();
    history.push("abcdef");
    let mut line = Editor::new(Config::default(), terminal, history);
    line.load_inputrc_str(
        "set horizontal-scroll-mode on\nset mark-modified-lines on\nset show-mode-in-prompt on\nset emacs-mode-string \"\\001hidden\\002M\"",
    ).unwrap();
    assert_eq!(
        line.read_line(Prompt::new("P>"), &mut ()).unwrap(),
        ReadlineResult::Line(b"abcdefZ".to_vec())
    );
    assert!(line.terminal().out.contains("hiddenM*P>abcdefZ"));
    assert_eq!(line.terminal().moved_columns.last(), Some(&11));
}

#[test]
fn horizontal_scroll_blink_maps_a_visible_opener_after_scrolling() {
    for (length, tab) in [(0, false), (100, false), (100, true)] {
        let text = "a".repeat(length);
        let mut events = vec![TerminalEvent::Bytes(text.as_bytes().to_vec())];
        if tab {
            events.push(TerminalEvent::Bytes(vec![0x16])); // quoted TAB
            events.push(TerminalEvent::Bytes(vec![b'\t']));
        }
        events.extend([
            TerminalEvent::Bytes(vec![b'(']),
            TerminalEvent::Bytes(vec![b')']),
            TerminalEvent::Bytes(vec![b'\r']),
        ]);
        let mut line = Editor::new(
            Config::default(),
            MemoryTerminal::with_events(events),
            History::new(),
        );
        line.load_inputrc_str("set horizontal-scroll-mode on\nset blink-matching-paren on")
            .unwrap();
        let result = line.read_line(Prompt::new("P>"), &mut ()).unwrap();
        assert_eq!(
            result,
            ReadlineResult::Line(format!("{text}{}()", if tab { "\t" } else { "" }).into_bytes())
        );
        // Short input blinks at column 2. After scrolling, the absolute opener
        // is at 102 (104 with the prompt-based TAB), outside the row; its
        // viewport column is 52 (54), not the absolute buffer column.
        let column = match (length, tab) {
            (0, _) => 2,
            (_, false) => 52,
            (_, true) => 54,
        };
        let moved = &line.terminal().moved_columns;
        assert_eq!(&moved[moved.len() - 3..], &[column, 0, column + 2]);
        assert!(moved.iter().all(|col| *col < 80));
        assert!(line.terminal().out.contains("\x1b[s\x1b[u"));
        assert!(line.terminal().moved_up.is_empty());
    }
}

#[test]
fn horizontal_scroll_blink_does_not_move_to_a_clipped_opener() {
    for columns in [1, 80] {
        let text = format!("({}", "a".repeat(100));
        let mut terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(text.as_bytes().to_vec()),
            TerminalEvent::Bytes(vec![b')']),
            TerminalEvent::Bytes(vec![b'\r']),
        ]);
        terminal.columns = columns;
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set horizontal-scroll-mode on\nset blink-matching-paren on")
            .unwrap();
        assert_eq!(
            line.read_line(Prompt::new("P>"), &mut ()).unwrap(),
            ReadlineResult::Line(format!("{text})").into_bytes())
        );
        assert!(!line.terminal().out.contains("\x1b[s"));
        assert!(!line.terminal().out.contains("\x1b[u"));
        assert!(
            line.terminal()
                .moved_columns
                .iter()
                .all(|col| *col < columns)
        );
    }
}

#[test]
fn redisplay_accounts_for_multiline_prompt_and_wrap_column() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 6;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let _ = line
        .read_line(Prompt::new("ignored\nλ> "), &mut ())
        .unwrap();
    assert!(line.terminal().out.contains("ignored\nλ> abcdef"));
    assert_eq!(line.terminal().moved_columns.last(), Some(&3));
    assert!(line.terminal().moved_up.iter().any(|rows| *rows > 0));
    assert!(line.terminal().cleared_screen > 0);
}

#[test]
fn redisplay_after_point_moves_to_previous_visual_row_does_not_overshoot_origin() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\x02".to_vec()),
        TerminalEvent::Bytes(b"\x02".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 8;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();

    assert_eq!(result, ReadlineResult::Line(b"abcdef".to_vec()));
    assert_eq!(line.terminal().moved_up, vec![1, 1, 1]);
}

#[test]
fn accept_line_from_previous_visual_row_moves_below_rendered_input() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Bytes(b"\x02".to_vec()),
        TerminalEvent::Bytes(b"\x02".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 8;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();

    assert_eq!(result, ReadlineResult::Line(b"abcdef".to_vec()));
    assert!(line.terminal().out.contains("\r\n\r\n"));
}

#[test]
fn multiline_history_entry_renders_as_multiple_terminal_lines() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"\x1b[A".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut history = History::new();
    history.push_bytes(b"echo one\necho two".to_vec());
    let mut line = Editor::new(Config::default(), terminal, history);

    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();

    assert_eq!(result, ReadlineResult::Line(b"echo one\necho two".to_vec()));
    assert!(line.terminal().out.contains("> echo one\r\necho two"));
    assert!(!line.terminal().out.contains("echo one^Jecho two"));
}

#[test]
fn quoted_insert_newline_renders_as_multiline_input() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"one".to_vec()),
        TerminalEvent::Bytes(b"\x16".to_vec()),
        TerminalEvent::Bytes(b"\n".to_vec()),
        TerminalEvent::Bytes(b"two".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());

    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();

    assert_eq!(result, ReadlineResult::Line(b"one\ntwo".to_vec()));
    assert!(line.terminal().out.contains("> one\r\ntwo"));
    assert!(!line.terminal().out.contains("one^Jtwo"));
}

#[test]
fn redisplay_forces_newline_when_point_lands_on_wrap_boundary() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 4;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();

    assert_eq!(result, ReadlineResult::Line(b"ab".to_vec()));
    assert!(line.terminal().out.contains("> ab\r\n\r\n"));
}

#[test]
fn redisplay_normalizes_wrap_boundary_before_returning_to_previous_visual_row() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"ab".to_vec()),
        TerminalEvent::Bytes(b"\x02".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 4;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();

    assert_eq!(result, ReadlineResult::Line(b"ab".to_vec()));
    assert!(line.terminal().out.contains("> ab\r\n"));
    assert!(line.terminal().moved_up.contains(&1));
}

#[test]
fn resize_event_recomputes_wrap_using_new_terminal_size() {
    let mut terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abcdef".to_vec()),
        TerminalEvent::Resize(TerminalSize {
            columns: 12,
            rows: 24,
        }),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    terminal.columns = 4;
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let _ = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(line.terminal().moved_columns.last(), Some(&8));
    assert!(line.terminal().moved_up.iter().any(|rows| *rows >= 1));
}

#[test]
fn enable_active_region_highlights_marked_region() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(b"abc".to_vec()),
        TerminalEvent::Bytes(b"\x01".to_vec()),
        TerminalEvent::Bytes(b"\x00".to_vec()),
        TerminalEvent::Bytes(b"\x06".to_vec()),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    line.load_inputrc_str(
        "set enable-active-region on\nset active-region-start-color \"\\e[7m\"\nset active-region-end-color \"\\e[0m\"",
    )
        .unwrap();
    let _ = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert!(line.terminal().out.contains("\x1b[7ma\x1b[0m"));
}

#[test]
fn redisplay_renders_control_characters_visibly() {
    let terminal = MemoryTerminal::with_events(vec![
        TerminalEvent::Bytes(vec![0x11]),
        TerminalEvent::Bytes(vec![0x01]),
        TerminalEvent::Bytes(b"\r".to_vec()),
    ]);
    let mut line = Editor::new(Config::default(), terminal, History::new());
    let result = line.read_line(Prompt::new("> "), &mut ()).unwrap();
    assert_eq!(result, ReadlineResult::Line("\x01".as_bytes().to_vec()));
    assert!(line.terminal().out.contains("^A"));
}

#[test]
fn horizontal_scroll_mode_measures_the_window_in_bytes() {
    // The display layer measures the horizontal-scroll window in bytes, as
    // GNU Readline does: the window holds the last 51 bytes of the line, so
    // a multibyte tail leaves fewer columns visible and the cursor sits
    // one column past the window's column width. The oracle test
    // `bash_horizontal_scroll_window_holds_the_last_line_bytes` records the
    // GNU side of the same fixtures.
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
        let mut terminal = MemoryTerminal::with_events(vec![
            TerminalEvent::Bytes(text.clone().into_bytes()),
            TerminalEvent::Bytes(vec![b'\r']),
        ]);
        terminal.columns = 80;
        let mut line = Editor::new(Config::default(), terminal, History::new());
        line.load_inputrc_str("set horizontal-scroll-mode on")
            .unwrap();
        assert_eq!(
            line.read_line(Prompt::new("P>"), &mut ()).unwrap(),
            ReadlineResult::Line(text.into_bytes())
        );
        assert!(
            line.terminal()
                .out
                .contains(&format!("<{}{}", &base[start..], tail)),
            "tail={tail:?}"
        );
        // Two column moves per redisplay: return to origin, then place point.
        // The prompt render moves to its end first; the redisplay after the
        // batched input then returns to the origin and places the point.
        assert_eq!(
            usize::from(line.terminal().moved_columns[3]),
            cursor,
            "tail={tail:?}"
        );
    }
}
