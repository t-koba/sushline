use super::{EditorState, KillDirection};

pub(crate) fn repeat<F>(state: &mut EditorState, mut op: F)
where
    F: FnMut(&mut EditorState),
{
    let count = repeat_count(state.take_numeric_arg());
    for _ in 0..count {
        op(state);
    }
}

pub(crate) fn repeat_signed<F, R>(state: &mut EditorState, mut forward: F, mut reverse: R)
where
    F: FnMut(&mut EditorState),
    R: FnMut(&mut EditorState),
{
    let arg = state.take_numeric_arg().unwrap_or(1);
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
    let arg = state.take_numeric_arg().unwrap_or(1);
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
    let arg = state.take_numeric_arg().unwrap_or(1);
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

/// The largest numeric argument GNU accepts.
///
/// Measured on the Bash 5.3 host, whose Readline is the 8.3 release: an
/// argument of 1000000 is accepted without
/// a bell, while a larger one rings the bell and resets the argument, so the
/// digits that follow the reset are ordinary text.
pub(crate) const MAX_NUMERIC_ARGUMENT: i32 = 1_000_000;

/// Update the pending numeric argument from a digit/minus key.
///
/// Returns true when the update exceeded `MAX_NUMERIC_ARGUMENT`: the argument
/// is reset so a huge repeat count never executes, and the caller rings the
/// bell. The digit that exceeded the limit is consumed with the argument, and
/// the digits that follow it are ordinary text, as they are on GNU.
pub(crate) fn update_numeric_argument(state: &mut EditorState, key: &[u8]) -> bool {
    let Some(last) = key.last().copied() else {
        return false;
    };
    state.numeric_arg_universal_noop = false;
    match last {
        b'-' => {
            let current = state.numeric_arg.unwrap_or(1);
            state.numeric_arg_negative =
                if state.numeric_arg_sign_only && !state.numeric_arg_universal {
                    !state.numeric_arg_negative
                } else {
                    true
                };
            let magnitude = current.unsigned_abs().min(i32::MAX as u32) as i32;
            state.numeric_arg = Some(if state.numeric_arg_negative {
                -magnitude
            } else {
                magnitude
            });
            state.numeric_arg_sign_only = true;
            false
        }
        b'0'..=b'9' => {
            let digit = (last - b'0') as i32;
            let current = state.numeric_arg.unwrap_or(0);
            let next = if state.numeric_arg_sign_only || state.numeric_arg_universal {
                Some(if state.numeric_arg_negative {
                    -digit
                } else {
                    digit
                })
            } else if state.numeric_arg_negative {
                current
                    .checked_mul(10)
                    .and_then(|scaled| scaled.checked_sub(digit))
            } else {
                current
                    .checked_mul(10)
                    .and_then(|scaled| scaled.checked_add(digit))
            };
            match next {
                Some(value) if value.unsigned_abs() > MAX_NUMERIC_ARGUMENT as u32 => {
                    state.take_numeric_arg();
                    true
                }
                Some(value) => {
                    state.numeric_arg = Some(value);
                    // Preserve a negative sign through leading zero digits.
                    state.numeric_arg_sign_only &= digit == 0;
                    state.numeric_arg_universal = false;
                    false
                }
                None => {
                    state.take_numeric_arg();
                    true
                }
            }
        }
        _ => false,
    }
}
