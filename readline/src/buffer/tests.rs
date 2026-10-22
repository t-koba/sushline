use super::*;

#[test]
fn edits_unicode_buffer_by_graphemes() {
    let mut b = LineBuffer::from("a界b");
    assert_eq!(b.len_chars(), "a界b".len());
    let (rendered, point_width) = b.render_text(None, RenderOptions::default(), 0);
    assert_eq!(rendered, "a界b");
    assert_eq!(point_width, 4);
    b.move_backward();
    b.backward_delete_char();
    assert_eq!(b.as_string(), "ab");
    assert_eq!(b.point(), 1);

    let mut b = LineBuffer::from("e\u{301}x");
    b.move_beginning();
    b.move_forward();
    assert_eq!(b.point(), "e\u{301}".len());
    b.delete_char();
    assert_eq!(b.as_string(), "e\u{301}");
}

#[test]
fn preserves_invalid_utf8_bytes_through_normal_edits() {
    let mut b = LineBuffer::from_bytes(vec![b'a', 0xff, b'b']);
    b.set_point(1);
    b.insert_char('X');
    assert_eq!(b.as_bytes(), &[b'a', b'X', 0xff, b'b']);
    assert!(b.delete_char());
    assert_eq!(b.as_bytes(), b"aXb");
    b.insert_bytes(&[0xfe]);
    b.set_point(3);
    assert!(b.backward_delete_char());
    assert_eq!(b.as_bytes(), b"aXb");
}

#[test]
fn kills_and_moves_by_words() {
    let mut b = LineBuffer::from("one two-three");
    assert_eq!(b.backward_kill_word(None), b"three");
    assert_eq!(b.as_string(), "one two-");
    assert_eq!(b.backward_kill_word(None), b"two-");
    assert_eq!(b.as_string(), "one ");

    let mut b = LineBuffer::from("one two");
    b.move_beginning();
    assert!(b.forward_word(None));
    assert_eq!(b.point(), 3);
    assert!(b.forward_word(None));
    assert_eq!(b.point(), 7);
}

#[test]
fn copies_current_word_when_point_is_inside_word() {
    let mut b = LineBuffer::from("one two three");
    b.set_point("one two thre".len());
    assert_eq!(b.copy_backward_word(None), b"three");
    assert_eq!(b.point(), "one two thre".len());

    let mut b = LineBuffer::from("one two three");
    b.set_point(1);
    assert_eq!(b.copy_forward_word(None), b"one");
    assert_eq!(b.point(), 1);
}

#[test]
fn replaces_ranges_and_updates_point() {
    let mut b = LineBuffer::from("abcdef");
    b.replace_range(2, 5, "XY");
    assert_eq!(b.as_string(), "abXYf");
    assert_eq!(b.point(), 4);
}

#[test]
fn edits_words_and_horizontal_space() {
    let mut b = LineBuffer::from("one   two");
    b.set_point(3);
    b.delete_horizontal_space();
    assert_eq!(b.as_string(), "onetwo");

    let mut b = LineBuffer::from("one two");
    b.move_beginning();
    assert!(b.upcase_word(None));
    assert_eq!(b.as_string(), "ONE two");
    assert!(b.capitalize_word(None));
    assert_eq!(b.as_string(), "ONE Two");

    let mut b = LineBuffer::from("one two");
    b.move_end();
    assert!(b.transpose_words(None));
    assert_eq!(b.as_string(), "two one");
}

#[test]
fn command_word_motion_treats_command_metacharacters_as_separators() {
    let mut b = LineBuffer::from("echo foo|bar 'baz qux'");
    b.move_beginning();
    assert!(b.forward_command_word());
    assert_eq!(b.point(), 4);
    assert!(b.forward_command_word());
    assert_eq!(b.point(), 8);
    assert!(b.forward_command_word());
    assert_eq!(b.point(), 12);
    assert!(b.forward_command_word());
    assert_eq!(b.point(), 22);
    assert!(b.backward_command_word());
    assert_eq!(b.point(), 13);
    assert!(b.backward_command_word());
    assert_eq!(b.point(), 9);
}

#[test]
fn tab_expands_to_next_eight_column_stop() {
    // GNU `DISPLAY_TABS` (patch 0 Bash 5.3 PTY oracle): TAB expands to
    // spaces up to the next multiple-of-8 stop, never `^I`.
    let mut b = LineBuffer::from("a\tb");
    let (rendered, point_width) = b.render_text(None, RenderOptions::default(), 0);
    assert_eq!(rendered, "a       b");
    assert_eq!(point_width, 9);
    // Prompt-offset basis: `a` ends at column 16, so TAB takes 8 spaces.
    let (rendered, _) = b.render_text(None, RenderOptions::default(), 15);
    assert_eq!(rendered, "a        b");
    // Lone TAB at column 15 takes a single space.
    let t = LineBuffer::from("\t");
    let (rendered, point_width) = t.render_text(None, RenderOptions::default(), 15);
    assert_eq!(rendered, " ");
    assert_eq!(point_width, 1);
    // Point inside the expansion measures displayed cells.
    b.set_point(1);
    let (_, point_width) = b.render_text(None, RenderOptions::default(), 0);
    assert_eq!(point_width, 1);
    b.set_point(2);
    let (_, point_width) = b.render_text(None, RenderOptions::default(), 0);
    assert_eq!(point_width, 8);
    b.set_point(3);
    let (rendered, _) = b.render_text(None, RenderOptions::default(), 0);
    assert_eq!(rendered, "a       b");
}

#[test]
fn tab_cursor_column_agrees_between_render_and_positions() {
    let b = LineBuffer::from("a\tb");
    let options = RenderOptions::default();
    let (rendered, point_width) = b.render_text(None, options.clone(), 15);
    assert_eq!(rendered, "a        b");
    let (last_row, point_row, point_col) = b.rendered_rows_and_point(15, 80, options);
    assert_eq!((last_row, point_row), (0, 0));
    assert_eq!(point_col, 15 + point_width);
}

#[test]
fn tab_basis_agrees_after_newline_and_in_horizontal_line() {
    // Multiline: TAB after `\\n` restarts at column 0, matching
    // `screen_positions` (8 spaces), not the pre-newline width (6).
    let b = LineBuffer::from("ab\n\t");
    let (rendered, _) = b.render_text(None, RenderOptions::default(), 0);
    assert_eq!(rendered, "ab\r\n        ");
    let (_, _, point_col) = b.rendered_rows_and_point(0, 80, RenderOptions::default());
    assert_eq!(point_col, 8);
    // H-scroll window sizing is cumulative window-relative: `a\\tb`
    // renders 9 cells (1 + 7 + 1) and reports the same point width.
    let mut w = LineBuffer::from("a\tb");
    w.move_end();
    let line = w.horizontal_line(None, RenderOptions::default(), 0);
    let (visible, point_width) =
        crate::display::horizontal::HorizontalViewport::default().layout(&line, 80);
    assert_eq!(visible, "a       b");
    assert_eq!(point_width, 9);
}

#[test]
fn interior_point_keeps_bytes_and_maps_prefix_in_byte_split_modes() {
    // This checks the logical byte-split render model, not physical glyph width.
    let bytes = b"\xe4\xb8\xadx";
    for (byte_oriented, utf8_locale) in [(false, false), (true, false), (true, true)] {
        for output_meta in [true, false] {
            let options = RenderOptions {
                byte_oriented,
                utf8_locale,
                output_meta,
                ..RenderOptions::default()
            };
            let expected_bytes: &[u8] = if output_meta {
                bytes
            } else {
                br"\344\270\255x"
            };
            let expected_widths = if output_meta {
                [0, 1, 2, 3, 4]
            } else {
                [0, 4, 8, 12, 13]
            };
            for (point, expected_width) in expected_widths.into_iter().enumerate() {
                let mut buffer = LineBuffer::from_bytes(bytes.to_vec());
                buffer.set_point(point);
                let (rendered, point_width) = buffer.render_text(None, options.clone(), 1);
                assert_eq!(rendered_string_to_bytes(&rendered), expected_bytes);
                assert_eq!(point_width, expected_width);

                let line = buffer.horizontal_line(None, options.clone(), 1);
                let (visible, visible_point) =
                    crate::display::horizontal::HorizontalViewport::default().layout(&line, 80);
                assert_eq!(rendered_string_to_bytes(&visible), expected_bytes);
                assert_eq!(line.point, expected_width);
                assert_eq!(visible_point, expected_width);

                for columns in [2, 80] {
                    let expected_position = (
                        (1 + expected_width) / columns,
                        (1 + expected_width) % columns,
                    );
                    let (total_row, row, col) =
                        buffer.rendered_rows_and_point(1, columns, options.clone());
                    assert_eq!((row, col), expected_position);
                    assert_eq!(total_row, (1 + expected_bytes.len()) / columns);
                    assert_eq!(
                        buffer.rendered_position(point, 1, columns, options.clone()),
                        expected_position
                    );
                }
                assert_eq!(buffer.as_bytes(), bytes);
            }
        }
    }
}

#[test]
fn interior_utf8_scalar_point_maps_to_scalar_start_on_wrapped_line() {
    // Internal coordinate consistency: interior bytes of a valid scalar
    // share the scalar-start screen position, matching `render_text`.
    let bytes = "abc中x".as_bytes().to_vec();
    let options = RenderOptions::default();
    let prompt_width = 2;
    let columns = 4;

    let mut at_start = LineBuffer::from_bytes(bytes.clone());
    at_start.set_point(3);
    let (start_rendered, start_width) = at_start.render_text(None, options.clone(), prompt_width);
    let start_pos = at_start.rendered_position(3, prompt_width, columns, options.clone());
    // Nonzero prompt with wrapping puts the scalar start on row 1.
    assert_eq!(start_width, 3);
    assert_eq!(start_pos, (1, 1));
    let start_line = at_start.horizontal_line(None, options.clone(), prompt_width);
    assert_eq!(start_line.point, start_width);

    for point in [4, 5] {
        let mut buffer = LineBuffer::from_bytes(bytes.clone());
        buffer.set_point(point);
        let (rendered, point_width) = buffer.render_text(None, options.clone(), prompt_width);
        assert_eq!(rendered, start_rendered);
        assert_eq!(rendered_string_to_bytes(&rendered), bytes);
        assert_eq!(point_width, start_width);
        let line = buffer.horizontal_line(None, options.clone(), prompt_width);
        assert_eq!(line.point, start_line.point);
        assert_eq!(
            buffer.rendered_position(point, prompt_width, columns, options.clone()),
            start_pos
        );
        let (total_row, row, col) =
            buffer.rendered_rows_and_point(prompt_width, columns, options.clone());
        assert_eq!((row, col), start_pos);
        assert_eq!(total_row, 2);
        assert_eq!(buffer.as_bytes(), bytes);

        buffer.move_screen_line(prompt_width, columns, 1, options.clone());
        assert_eq!(buffer.point(), bytes.len());
        assert_eq!(buffer.as_bytes(), bytes);
    }
}

#[test]
fn interior_point_in_utf8_scalar_mode_keeps_bytes_at_unit_start() {
    // UTF-8 `byte-oriented off` keeps valid scalars intact; an interior byte
    // point is only reachable via byte commands and must not change rendered bytes.
    let bytes = "中x".as_bytes().to_vec();
    let options = RenderOptions::default();
    let mut baseline = None;
    let mut widths = Vec::new();
    for point in 0..=bytes.len() {
        let mut buffer = LineBuffer::from_bytes(bytes.clone());
        buffer.set_point(point);
        let (rendered, point_width) = buffer.render_text(None, options.clone(), 0);
        let rendered_bytes = rendered_string_to_bytes(&rendered);
        if let Some(first) = &baseline {
            assert_eq!(&rendered_bytes, first, "point={point}");
        } else {
            baseline = Some(rendered_bytes);
        }
        widths.push(point_width);
    }
    assert_eq!(widths[1], widths[0], "widths={widths:?}");
    assert_eq!(widths[2], widths[0], "widths={widths:?}");
    assert!(widths[3] > widths[0], "widths={widths:?}");
    assert!(widths[4] > widths[3], "widths={widths:?}");
}

#[test]
fn vertical_move_down_lands_on_scalar_boundary() {
    // Interior bytes share the scalar-start cell for coordinate lookup;
    // the lower-column fallback must keep the first index at that cell,
    // which is the scalar boundary, so editing keeps the scalar intact.
    let bytes = "a中".as_bytes().to_vec();
    let mut buffer = LineBuffer::from_bytes(bytes.clone());
    buffer.set_point(0);
    buffer.move_screen_line(1, 2, 1, RenderOptions::default());
    assert_eq!(buffer.point(), 1);
    buffer.insert_char('X');
    assert_eq!(buffer.as_bytes(), "aX中".as_bytes());
    assert_eq!(std::str::from_utf8(buffer.as_bytes()).unwrap(), "aX中");
}

#[test]
fn a_private_use_character_in_the_line_is_rendered_as_itself() {
    // An application may use the private-use area for its own glyphs, so a
    // character there is drawn as itself and reaches the terminal as its
    // own bytes.
    for text in ["\u{e000}", "\u{e0b0}", "\u{e0ff}"] {
        let bytes = text.as_bytes().to_vec();
        let buffer = LineBuffer::from_bytes(bytes.clone());
        let (rendered, point_width) = buffer.render_text(None, RenderOptions::default(), 0);
        assert_eq!(rendered, text, "{text}");
        assert_eq!(rendered_string_to_bytes(&rendered), bytes, "{text}");
        assert_eq!(point_width, 1, "{text}");
    }
}

#[test]
fn a_raw_byte_and_a_private_use_character_keep_their_own_bytes() {
    // A byte that spells out no character is written as itself, and the
    // private-use character beside it is not read as a byte.
    let mut bytes = vec![0xff];
    bytes.extend_from_slice("\u{e0ff}".as_bytes());
    bytes.push(0xfe);
    let buffer = LineBuffer::from_bytes(bytes.clone());
    let (rendered, _) = buffer.render_text(None, RenderOptions::default(), 0);
    assert_eq!(rendered_string_to_bytes(&rendered), bytes);
}

#[test]
fn a_raw_byte_is_rendered_one_cell_wide() {
    // The mark that carries a byte is a zero-width character, so the byte
    // keeps the cell width of the character it stands for.
    let mut bytes = vec![0xff];
    bytes.extend_from_slice(b"x");
    let buffer = LineBuffer::from_bytes(bytes);
    let (rendered, point_width) = buffer.render_text(None, RenderOptions::default(), 0);
    assert_eq!(rendered_string_to_bytes(&rendered), vec![0xff, b'x']);
    assert_eq!(point_width, 2);
}
