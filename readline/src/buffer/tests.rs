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
    // Unconditional: also expands with `echo-control-characters` off.
    let off = RenderOptions {
        echo_control: false,
        ..RenderOptions::default()
    };
    b.set_point(3);
    let (rendered, _) = b.render_text(None, off, 0);
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
fn tab_basis_agrees_after_newline_and_in_hscroll_window() {
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
    let (visible, point_width) =
        w.horizontal_window_with_options(80, None, RenderOptions::default());
    assert_eq!(visible, "a       b");
    assert_eq!(point_width, 9);
}
