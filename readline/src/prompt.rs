use crate::width::char_width;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Prompt.
pub struct Prompt {
    raw: String,
    visible: String,
    width: usize,
}

impl Prompt {
    /// New.
    pub fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let (visible, width) = strip_readline_markers(&raw);
        Self {
            raw,
            visible,
            width,
        }
    }

    /// Raw.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Visible.
    pub fn visible(&self) -> &str {
        &self.visible
    }

    /// Width.
    pub fn width(&self) -> usize {
        self.width
    }
}

impl From<&str> for Prompt {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<Vec<u8>> for Prompt {
    fn from(value: Vec<u8>) -> Self {
        Self::new(String::from_utf8_lossy(&value).into_owned())
    }
}

impl From<&[u8]> for Prompt {
    fn from(value: &[u8]) -> Self {
        Self::new(String::from_utf8_lossy(value).into_owned())
    }
}

fn strip_readline_markers(raw: &str) -> (String, usize) {
    let mut visible = String::new();
    let mut line_width = 0;
    let mut non_printing = false;
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x01' {
            non_printing = true;
            continue;
        }
        if ch == '\x02' {
            non_printing = false;
            continue;
        }
        if ch == '\\' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    non_printing = true;
                    continue;
                }
                Some(']') => {
                    chars.next();
                    non_printing = false;
                    continue;
                }
                Some('e' | 'E') => {
                    chars.next();
                    push_prompt_char('\x1b', non_printing, &mut visible, &mut line_width);
                    continue;
                }
                Some(c) if c.is_ascii_digit() && c < '8' => {
                    let mut value = 0u32;
                    let mut consumed = 0;
                    while consumed < 3 {
                        let Some(next) = chars.peek().copied() else {
                            break;
                        };
                        if !next.is_ascii_digit() || next >= '8' {
                            break;
                        }
                        chars.next();
                        value = value * 8 + next.to_digit(8).unwrap_or(0);
                        consumed += 1;
                    }
                    if let Some(decoded) = char::from_u32(value) {
                        push_prompt_char(decoded, non_printing, &mut visible, &mut line_width);
                    }
                    continue;
                }
                _ => {}
            }
        }

        push_prompt_char(ch, non_printing, &mut visible, &mut line_width);
    }

    (visible, line_width)
}

fn push_prompt_char(ch: char, non_printing: bool, visible: &mut String, line_width: &mut usize) {
    visible.push(ch);
    if non_printing {
        return;
    }
    if ch == '\n' {
        *line_width = 0;
    } else {
        *line_width += char_width(ch);
    }
}

#[cfg(test)]
mod tests;
