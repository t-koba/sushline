use super::*;

#[test]
fn edits_unicode_buffer_by_graphemes() {
    let mut b = LineBuffer::from("a界b");
    assert_eq!(b.len_chars(), "a界b".len());
    assert_eq!(b.display_width(), 4);
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
