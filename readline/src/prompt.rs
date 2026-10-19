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

    /// Cell units for horizontal redisplay, retaining explicitly hidden bytes.
    pub(crate) fn horizontal_line(&self) -> crate::display::horizontal::DisplayLine {
        let mut line = crate::display::horizontal::DisplayLine::default();
        let mut span = String::new();
        let mut was_hidden = false;
        let flush = |line: &mut crate::display::horizontal::DisplayLine,
                     span: &mut String,
                     hidden: bool| {
            if hidden {
                line.push_hidden(std::mem::take(span));
            } else {
                line.push_prompt_text(span);
                span.clear();
            }
        };
        parse_prompt(&self.raw, |ch, hidden| {
            if hidden != was_hidden {
                flush(&mut line, &mut span, was_hidden);
                was_hidden = hidden;
            }
            span.push(ch);
        });
        flush(&mut line, &mut span, was_hidden);
        line
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
    let mut width = 0;
    parse_prompt(raw, |ch, hidden| {
        visible.push(ch);
        // Newlines still break lines even inside hidden regions.
        if ch == '\n' {
            width = 0;
        } else if !hidden {
            // Prompt accounting is not rendered-output measurement: only
            // explicit nonprinting spans hide escape bytes. In the Bash 5.3
            // LC_ALL=C CSI fixture, ESC itself counts as one logical column.
            width += if ch == '\x1b' { 1 } else { char_width(ch) };
        }
    });
    (visible, width)
}

fn parse_prompt(raw: &str, mut push_decoded: impl FnMut(char, bool)) {
    let mut hidden_soh = false;
    let mut hidden_bracket = false;
    let mut chars = raw.chars().peekable();

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
                    push_decoded('\x1b', hidden_soh || hidden_bracket);
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
                        push_decoded(decoded, hidden_soh || hidden_bracket);
                    }
                    continue;
                }
                _ => {}
            }
        }

        push_decoded(ch, hidden_soh || hidden_bracket);
    }
}

#[cfg(test)]
mod tests;
