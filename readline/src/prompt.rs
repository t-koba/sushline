use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    raw: String,
    visible: String,
    width: usize,
}

impl Prompt {
    pub fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let (visible, width) = strip_readline_markers(&raw);
        Self {
            raw,
            visible,
            width,
        }
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn visible(&self) -> &str {
        &self.visible
    }

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
    let mut width = 0;
    let mut current_line_width = 0;
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
                    push_prompt_char(
                        '\x1b',
                        non_printing,
                        &mut visible,
                        &mut current_line_width,
                        &mut width,
                    );
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
                        push_prompt_char(
                            decoded,
                            non_printing,
                            &mut visible,
                            &mut current_line_width,
                            &mut width,
                        );
                    }
                    continue;
                }
                _ => {}
            }
        }

        push_prompt_char(
            ch,
            non_printing,
            &mut visible,
            &mut current_line_width,
            &mut width,
        );
    }

    (visible, width)
}

fn push_prompt_char(
    ch: char,
    non_printing: bool,
    visible: &mut String,
    current_line_width: &mut usize,
    width: &mut usize,
) {
    visible.push(ch);
    if non_printing {
        return;
    }
    if ch == '\n' {
        *current_line_width = 0;
        *width = 0;
    } else {
        *current_line_width += UnicodeWidthChar::width(ch).unwrap_or(0);
        *width = *current_line_width;
    }
}

#[cfg(test)]
mod tests;
