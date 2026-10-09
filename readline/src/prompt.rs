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

    /// Width after a mode/operator prefix on the final line.
    ///
    /// A multiline prompt starts its last line after the prefix, so only the
    /// prompt last line counts; otherwise the prefix last-line width applies.
    pub(crate) fn width_after_prefix(&self, prefix_width: usize) -> usize {
        if self.visible.contains('\n') {
            self.width
        } else {
            prefix_width + self.width
        }
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
    let mut measurable = String::new();
    let mut hidden_soh = false;
    let mut hidden_bracket = false;
    let mut chars = raw.chars().peekable();

    let push_decoded = |ch: char,
                        visible: &mut String,
                        measurable: &mut String,
                        hidden_soh: bool,
                        hidden_bracket: bool| {
        visible.push(ch);
        // Newlines still break lines even inside hidden regions.
        if ch == '\n' || (!hidden_soh && !hidden_bracket) {
            measurable.push(ch);
        }
    };

    while let Some(ch) = chars.next() {
        if ch == '\x01' {
            hidden_soh = true;
            continue;
        }
        if ch == '\x02' {
            hidden_soh = false;
            continue;
        }
        if ch == '\\' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    hidden_bracket = true;
                    continue;
                }
                Some(']') => {
                    chars.next();
                    hidden_bracket = false;
                    continue;
                }
                Some('e' | 'E') => {
                    chars.next();
                    push_decoded(
                        '\x1b',
                        &mut visible,
                        &mut measurable,
                        hidden_soh,
                        hidden_bracket,
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
                        push_decoded(
                            decoded,
                            &mut visible,
                            &mut measurable,
                            hidden_soh,
                            hidden_bracket,
                        );
                    }
                    continue;
                }
                _ => {}
            }
        }

        push_decoded(
            ch,
            &mut visible,
            &mut measurable,
            hidden_soh,
            hidden_bracket,
        );
    }

    let width = last_line_width(&measurable);
    (visible, width)
}

#[cfg(test)]
mod tests;
