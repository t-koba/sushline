use super::{EditorState, KillDirection};

pub(crate) fn repeat<F>(state: &mut EditorState, mut op: F)
where
    F: FnMut(&mut EditorState),
{
    let count = repeat_count(state.numeric_arg.take());
    for _ in 0..count {
        op(state);
    }
}

pub(crate) fn repeat_signed<F, R>(state: &mut EditorState, mut forward: F, mut reverse: R)
where
    F: FnMut(&mut EditorState),
    R: FnMut(&mut EditorState),
{
    let arg = state.numeric_arg.take().unwrap_or(1);
    let count = arg.unsigned_abs();
    let op: &mut dyn FnMut(&mut EditorState) = if arg < 0 { &mut reverse } else { &mut forward };
    for _ in 0..count {
        op(state);
    }
}

pub(crate) fn repeat_case_word<F, R>(state: &mut EditorState, mut forward: F, mut reverse: R)
where
    F: FnMut(&mut EditorState) -> bool,
    R: FnMut(&mut EditorState) -> bool,
{
    let arg = state.numeric_arg.take().unwrap_or(1);
    let count = arg.unsigned_abs();
    let op: &mut dyn FnMut(&mut EditorState) -> bool =
        if arg < 0 { &mut reverse } else { &mut forward };
    for _ in 0..count {
        if !op(state) {
            break;
        }
    }
}

pub(crate) fn repeat_signed_collect_bytes<F, R, AF, AR>(
    state: &mut EditorState,
    mut forward: F,
    mut reverse: R,
    mut append_forward: AF,
    mut append_reverse: AR,
    directions: (KillDirection, KillDirection),
    out: &mut Vec<u8>,
) -> KillDirection
where
    F: FnMut(&mut EditorState) -> Vec<u8>,
    R: FnMut(&mut EditorState) -> Vec<u8>,
    AF: FnMut(Vec<u8>, &mut Vec<u8>),
    AR: FnMut(Vec<u8>, &mut Vec<u8>),
{
    let arg = state.numeric_arg.take().unwrap_or(1);
    let count = arg.unsigned_abs();
    if arg < 0 {
        for _ in 0..count {
            append_reverse(reverse(state), out);
        }
        directions.1
    } else {
        for _ in 0..count {
            append_forward(forward(state), out);
        }
        directions.0
    }
}

pub(crate) fn repeat_count(arg: Option<i32>) -> u32 {
    arg.unwrap_or(1).unsigned_abs()
}

/// Update the pending numeric argument from a digit/minus key.
///
/// Returns true when the update overflowed `i32`: the argument is reset so a
/// huge repeat count never executes, and the caller rings the bell.
/// A lone `-` on `i32::MIN` keeps `MIN` without bell: it is already the most
/// negative representable argument.
pub(crate) fn update_numeric_argument(state: &mut EditorState, key: &[u8]) -> bool {
    let Some(last) = key.last().copied() else {
        return false;
    };
    match last {
        b'-' => {
            let current = state.numeric_arg.unwrap_or(1);
            state.numeric_arg = Some(if current == i32::MIN {
                i32::MIN
            } else {
                -current.abs()
            });
            state.numeric_arg_sign_only = true;
            false
        }
        b'0'..=b'9' => {
            let digit = (last - b'0') as i32;
            let current = state.numeric_arg.unwrap_or(0);
            let next = if current < 0 {
                if state.numeric_arg_sign_only {
                    Some(-digit)
                } else {
                    current
                        .checked_mul(10)
                        .and_then(|scaled| scaled.checked_sub(digit))
                }
            } else {
                current
                    .checked_mul(10)
                    .and_then(|scaled| scaled.checked_add(digit))
            };
            match next {
                Some(value) => {
                    state.numeric_arg = Some(value);
                    state.numeric_arg_sign_only = false;
                    false
                }
                None => {
                    state.numeric_arg = None;
                    state.numeric_arg_sign_only = false;
                    true
                }
            }
        }
        _ => false,
    }
}
