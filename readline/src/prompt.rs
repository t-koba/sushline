use crate::width::last_line_width;

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
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x01' || ch == '\x02' {
            continue;
        }
        if ch == '\\' {
            match chars.peek().copied() {
                Some('[') | Some(']') => {
                    chars.next();
                    continue;
                }
                Some('e' | 'E') => {
                    chars.next();
                    visible.push('\x1b');
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
                        visible.push(decoded);
                    }
                    continue;
                }
                _ => {}
            }
        }

        visible.push(ch);
    }

    let width = last_line_width(&visible);
    (visible, width)
}

#[cfg(test)]
mod tests;
