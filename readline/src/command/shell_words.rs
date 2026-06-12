use crate::buffer::LineBuffer;
use crate::hooks::Hooks;

fn hook_token_spans(line: &[u8], hooks: &impl Hooks) -> Option<Vec<(usize, usize)>> {
    let spans = hooks.tokenize_with_spans(line)?;
    let mut previous_end = 0;
    for &(start, end) in &spans {
        if start >= end || end > line.len() || start < previous_end {
            return None;
        }
        previous_end = end;
    }
    Some(spans)
}

pub(super) fn move_shell_forward_word(buffer: &mut LineBuffer, hooks: &impl Hooks) -> bool {
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

pub(super) fn move_shell_backward_word(buffer: &mut LineBuffer, hooks: &impl Hooks) -> bool {
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

pub(super) fn kill_shell_forward_word(buffer: &mut LineBuffer, hooks: &impl Hooks) -> Vec<u8> {
    let start = buffer.point();
    if move_shell_forward_word(buffer, hooks) {
        buffer.delete_range_bytes(start, buffer.point())
    } else {
        Vec::new()
    }
}

pub(super) fn kill_shell_backward_word(buffer: &mut LineBuffer, hooks: &impl Hooks) -> Vec<u8> {
    let end = buffer.point();
    if move_shell_backward_word(buffer, hooks) {
        buffer.delete_range_bytes(buffer.point(), end)
    } else {
        Vec::new()
    }
}

pub(super) fn transpose_shell_words(buffer: &mut LineBuffer, hooks: &impl Hooks) -> bool {
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

    let line = buffer.as_bytes();
    let left = line[left_start..left_end].to_vec();
    let middle = line[left_end..right_start].to_vec();
    let right = line[right_start..right_end].to_vec();
    let mut replacement = Vec::with_capacity(right.len() + middle.len() + left.len());
    replacement.extend_from_slice(&right);
    replacement.extend_from_slice(&middle);
    replacement.extend_from_slice(&left);
    buffer.replace_range_bytes(left_start, right_end, &replacement);
    true
}
