fn push_meta_prefix(out: &mut Vec<u8>, meta_prefix: bool, byte: u8) {
    if meta_prefix {
        out.push(0x1b);
        out.push(byte);
    } else {
        out.push(byte | 0x80);
    }
}

pub(super) fn parse_named_keyseq(value: &str, meta_prefix: bool) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut pending_meta = false;
    for part in value.split('-') {
        match part.to_ascii_lowercase().as_str() {
            "control" | "c" => continue,
            "meta" | "m" => {
                pending_meta = true;
                continue;
            }
            "del" | "rubout" => {
                if pending_meta {
                    push_meta_prefix(&mut out, meta_prefix, 0x7f);
                } else {
                    out.push(0x7f);
                }
                pending_meta = false;
            }
            "esc" | "escape" => {
                if pending_meta {
                    push_meta_prefix(&mut out, meta_prefix, 0x1b);
                } else {
                    out.push(0x1b);
                }
                pending_meta = false;
            }
            "lfd" | "newline" => {
                if pending_meta {
                    push_meta_prefix(&mut out, meta_prefix, b'\n');
                } else {
                    out.push(b'\n');
                }
                pending_meta = false;
            }
            "ret" | "return" => {
                if pending_meta {
                    push_meta_prefix(&mut out, meta_prefix, b'\r');
                } else {
                    out.push(b'\r');
                }
                pending_meta = false;
            }
            "space" | "spc" => {
                if pending_meta {
                    push_meta_prefix(&mut out, meta_prefix, b' ');
                } else {
                    out.push(b' ');
                }
                pending_meta = false;
            }
            "tab" => {
                if pending_meta {
                    push_meta_prefix(&mut out, meta_prefix, b'\t');
                } else {
                    out.push(b'\t');
                }
                pending_meta = false;
            }
            s if s.len() == 1 => {
                let b = s.as_bytes()[0];
                if value.to_ascii_lowercase().contains("control-")
                    || value.to_ascii_lowercase().contains("c-")
                {
                    let byte = control_byte(b)?;
                    if pending_meta {
                        push_meta_prefix(&mut out, meta_prefix, byte);
                    } else {
                        out.push(byte);
                    }
                } else {
                    if pending_meta {
                        push_meta_prefix(&mut out, meta_prefix, b);
                    } else {
                        out.push(b);
                    }
                }
                pending_meta = false;
            }
            _ => return Err(format!("unknown key name: {value}")),
        }
    }
    Ok(out)
}

pub(super) fn parse_quoted_keyseq(value: &str, meta_prefix: bool) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.extend(ch.to_string().as_bytes());
            continue;
        }

        match chars.next() {
            Some('C') | Some('c') if chars.next() == Some('-') => {
                let c = chars
                    .next()
                    .ok_or_else(|| "missing control character".to_string())?;
                out.push(control_byte(c as u8)?);
            }
            Some('M') | Some('m') if chars.next() == Some('-') => {
                parse_meta_character(&mut chars, &mut out, meta_prefix)?;
            }
            Some('a') => out.push(0x07),
            Some('b') => out.push(0x08),
            Some('d') => out.push(0x7f),
            Some('e') => out.push(0x1b),
            Some('f') => out.push(0x0c),
            Some('n') => out.push(b'\n'),
            Some('r') => out.push(b'\r'),
            Some('t') => out.push(b'\t'),
            Some('v') => out.push(0x0b),
            Some('x') => out.push(parse_hex_escape(&mut chars)?),
            Some(first @ '0'..='7') => out.push(parse_octal_escape(first, &mut chars)?),
            Some('\\') => out.push(b'\\'),
            Some('"') => out.push(b'"'),
            Some(other) => out.extend(other.to_string().as_bytes()),
            None => return Err("trailing escape in key sequence".to_string()),
        }
    }
    Ok(out)
}

fn parse_meta_character<I>(
    chars: &mut std::iter::Peekable<I>,
    out: &mut Vec<u8>,
    meta_prefix: bool,
) -> Result<(), String>
where
    I: Iterator<Item = char>,
{
    match chars.next() {
        Some('\\') => match chars.next() {
            Some('C') | Some('c') if chars.next() == Some('-') => {
                let c = chars
                    .next()
                    .ok_or_else(|| "missing control character".to_string())?;
                push_meta_prefix(out, meta_prefix, control_byte(c as u8)?);
            }
            Some('M') | Some('m') if chars.next() == Some('-') => {
                let mut nested = Vec::new();
                parse_meta_character(chars, &mut nested, meta_prefix)?;
                for byte in nested {
                    push_meta_prefix(out, meta_prefix, byte & 0x7f);
                }
            }
            Some('e') => push_meta_prefix(out, meta_prefix, 0x1b),
            Some('x') => push_meta_prefix(out, meta_prefix, parse_hex_escape(chars)?),
            Some(first @ '0'..='7') => {
                push_meta_prefix(out, meta_prefix, parse_octal_escape(first, chars)?);
            }
            Some(other) => {
                for byte in other.to_string().as_bytes() {
                    push_meta_prefix(out, meta_prefix, *byte);
                }
            }
            None => return Err("missing meta character".to_string()),
        },
        Some(c) => {
            for byte in c.to_string().as_bytes() {
                push_meta_prefix(out, meta_prefix, *byte);
            }
        }
        None => return Err("missing meta character".to_string()),
    }
    Ok(())
}

fn parse_hex_escape<I>(chars: &mut std::iter::Peekable<I>) -> Result<u8, String>
where
    I: Iterator<Item = char>,
{
    let mut value = 0_u8;
    let mut digits = 0;
    while digits < 2 {
        let Some(ch) = chars.peek().copied() else {
            break;
        };
        let Some(digit) = ch.to_digit(16) else {
            break;
        };
        chars.next();
        value = value
            .checked_mul(16)
            .and_then(|current| current.checked_add(digit as u8))
            .ok_or_else(|| "hex escape out of range".to_string())?;
        digits += 1;
    }
    if digits == 0 {
        return Err("missing hex digits in key sequence".to_string());
    }
    Ok(value)
}

fn parse_octal_escape<I>(first: char, chars: &mut std::iter::Peekable<I>) -> Result<u8, String>
where
    I: Iterator<Item = char>,
{
    let mut value = first.to_digit(8).expect("caller passes an octal digit") as u8;
    let mut digits = 1;
    while digits < 3 {
        let Some(ch) = chars.peek().copied() else {
            break;
        };
        let Some(digit) = ch.to_digit(8) else {
            break;
        };
        chars.next();
        value = value
            .checked_mul(8)
            .and_then(|current| current.checked_add(digit as u8))
            .ok_or_else(|| "octal escape out of range".to_string())?;
        digits += 1;
    }
    Ok(value)
}

fn control_byte(b: u8) -> Result<u8, String> {
    match b {
        b'?' => Ok(0x7f),
        b'a'..=b'z' => Ok(b - b'a' + 1),
        b'A'..=b'Z' => Ok(b - b'A' + 1),
        b'@'..=b'_' => Ok(b - b'@'),
        _ => Err(format!("invalid control character: {}", b as char)),
    }
}
