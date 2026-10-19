use crate::buffer::LineBuffer;
use crate::hooks::{Hooks, validate_token_spans};

fn hook_token_spans(line: &[u8], hooks: &mut impl Hooks) -> Option<Vec<(usize, usize)>> {
    let spans = hooks.shell_word_spans(line)?;
    validate_token_spans(line, &spans).then_some(spans)
}

pub(super) fn move_shell_forward_word(buffer: &mut LineBuffer, hooks: &mut impl Hooks) -> bool {
    let Some(spans) = hook_token_spans(buffer.as_bytes(), hooks) else {
        return buffer.forward_command_word();
    };
    let point = buffer.point();
    let Some((_, end)) = spans.into_iter().find(|(_, end)| point < *end) else {
        return false;
    };
    buffer.set_point(end);
    buffer.point() != point
}

pub(super) fn move_shell_backward_word(buffer: &mut LineBuffer, hooks: &mut impl Hooks) -> bool {
    let Some(spans) = hook_token_spans(buffer.as_bytes(), hooks) else {
        return buffer.backward_command_word();
    };
    let point = buffer.point();
    let Some((start, _)) = spans.into_iter().rev().find(|(start, _)| *start < point) else {
        return false;
    };
    buffer.set_point(start);
    buffer.point() != point
}

pub(super) fn kill_shell_forward_word(buffer: &mut LineBuffer, hooks: &mut impl Hooks) -> Vec<u8> {
    let start = buffer.point();
    if move_shell_forward_word(buffer, hooks) {
        buffer.delete_range_bytes(start, buffer.point())
    } else {
        Vec::new()
    }
}

pub(super) fn kill_shell_backward_word(buffer: &mut LineBuffer, hooks: &mut impl Hooks) -> Vec<u8> {
    let end = buffer.point();
    if move_shell_backward_word(buffer, hooks) {
        buffer.delete_range_bytes(buffer.point(), end)
    } else {
        Vec::new()
    }
}

pub(super) fn transpose_shell_words(buffer: &mut LineBuffer, hooks: &mut impl Hooks) -> bool {
    let Some(spans) = hook_token_spans(buffer.as_bytes(), hooks) else {
        return buffer.transpose_command_words();
    };
    let Some((left_index, &(left_start, left_end))) = spans
        .iter()
        .enumerate()
        .rev()
        .find(|(_, (start, _))| *start < buffer.point())
    else {
        return false;
    };
    let (left_start, left_end, right_start, right_end) = if let Some(&(right_start, right_end)) =
        spans.iter().find(|(start, _)| *start >= left_end)
    {
        (left_start, left_end, right_start, right_end)
    } else if let Some(&(previous_start, previous_end)) =
        left_index.checked_sub(1).and_then(|index| spans.get(index))
    {
        (previous_start, previous_end, left_start, left_end)
    } else {
        return false;
    };

    buffer.transpose_word_ranges(left_start, left_end, right_start, right_end)
}

pub(super) fn transpose_shell_words_by_count(
    buffer: &mut LineBuffer,
    hooks: &mut impl Hooks,
    count: i32,
) -> bool {
    if count == 0 {
        return false;
    }
    if count == 1 {
        return transpose_shell_words(buffer, hooks);
    }
    let spans =
        hook_token_spans(buffer.as_bytes(), hooks).unwrap_or_else(|| buffer.command_word_spans());
    transpose_word_spans_by_count(buffer, &spans, count)
}

fn transpose_word_spans_by_count(
    buffer: &mut LineBuffer,
    spans: &[(usize, usize)],
    count: i32,
) -> bool {
    let mut point = buffer.point();
    move_word_spans_by_count(&mut point, spans, count, true);
    let right_end = point;
    move_word_spans_by_count(&mut point, spans, 1, false);
    let right_start = point;
    move_word_spans_by_count(&mut point, spans, count, false);
    let left_start = point;
    move_word_spans_by_count(&mut point, spans, 1, true);
    let left_end = point;

    if left_start == right_start || right_start < left_end {
        return false;
    }
    buffer.transpose_word_ranges(left_start, left_end, right_start, right_end)
}

fn move_word_spans_by_count(
    point: &mut usize,
    spans: &[(usize, usize)],
    count: i32,
    forward: bool,
) {
    let move_forward = forward != count.is_negative();
    for _ in 0..count.unsigned_abs() {
        let next_point = if move_forward {
            spans
                .iter()
                .find(|(_, end)| *point < *end)
                .map(|(_, end)| *end)
        } else {
            spans
                .iter()
                .rev()
                .find(|(start, _)| *start < *point)
                .map(|(start, _)| *start)
        };
        let Some(next_point) = next_point else {
            break;
        };
        if next_point == *point {
            break;
        }
        *point = next_point;
    }
}
