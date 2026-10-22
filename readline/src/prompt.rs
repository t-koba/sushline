use crate::width::char_width;

/// The backslash sequences a prompt's text is read as.
///
/// GNU Readline draws the prompt it is given: it reads the invisible markers
/// the shell hands it and leaves every other byte as it is. A shell expands
/// its own prompt escapes before the editor sees them, so an editor that
/// decodes them changes a prompt the caller meant literally. The shell's
/// spelling is therefore an explicit choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptEscapes {
    /// The prompt text as given; only Readline's own invisible markers are
    /// read.
    #[default]
    None,
    /// The shell's prompt spelling: the backslash sequences a shell reads in
    /// a prompt.
    ///
    /// A shell reads the prompt escapes in two passes: the first reads the
    /// escapes, the second removes the quoting its expansion removes. The
    /// escapes the first pass reads are `\\`, `\a`, `\e`, `\n` and `\r`, `\[`
    /// and `\]` mark a non-printing span, and `\NNN` with exactly three
    /// octal digits is one byte. The second pass reads `\\`, `\"`, `\$` and
    /// `` \` ``. Every other backslash sequence is left as it was written.
    /// The shell's own prompt sequences (`\u`, `\w`, `\t`, ...) name the
    /// shell's variables and are not read here.
    Shell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Prompt.
pub struct Prompt {
    raw: String,
    visible: String,
    width: usize,
    escapes: PromptEscapes,
}

impl Prompt {
    /// New.
    ///
    /// The text is taken as given: only Readline's own invisible markers
    /// (`\x01` ... `\x02`) are read, so a prompt the caller meant literally
    /// stays literal. A shell expands its own prompt escapes before it hands
    /// the prompt to the editor, and GNU Readline draws the prompt it is
    /// given. [`Prompt::with_escapes`] reads the shell's spelling instead.
    pub fn new(raw: impl Into<String>) -> Self {
        Self::with_escapes(raw, PromptEscapes::None)
    }

    /// New prompt whose text is read as `escapes` says.
    pub fn with_escapes(raw: impl Into<String>, escapes: PromptEscapes) -> Self {
        let raw = raw.into();
        let (visible, width) = strip_readline_markers(&raw, escapes);
        Self {
            raw,
            visible,
            width,
            escapes,
        }
    }

    /// The escapes the prompt's text is read as.
    pub fn escapes(&self) -> PromptEscapes {
        self.escapes
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
        parse_prompt(&self.raw, self.escapes, |ch, hidden| {
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

fn strip_readline_markers(raw: &str, escapes: PromptEscapes) -> (String, usize) {
    let mut visible = String::new();
    let mut width = 0;
    parse_prompt(raw, escapes, |ch, hidden| {
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

/// The characters the shell's first pass reads.
///
/// The first pass reads the prompt escapes: the control characters and the
/// backslash itself. Every other backslash sequence is left as it was
/// written.
fn prompt_escape(c: char) -> Option<char> {
    match c {
        '\\' => Some('\\'),
        'a' => Some('\x07'),
        'e' => Some('\x1b'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        _ => None,
    }
}

/// The characters the shell's second pass reads.
///
/// The second pass removes the quoting the shell's expansion removes, so a
/// quoted character loses its backslash.
fn quoted_escape(c: char) -> Option<char> {
    match c {
        '\\' => Some('\\'),
        '"' => Some('"'),
        '$' => Some('$'),
        '`' => Some('`'),
        _ => None,
    }
}

/// The prompt as a shell spells it, with the spans the shell marks as
/// non-printing.
fn decode_shell_prompt(raw: &str) -> Vec<(char, bool)> {
    let chars: Vec<char> = raw.chars().collect();
    let mut decoded: Vec<(char, bool)> = Vec::with_capacity(chars.len());
    let mut hidden = false;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\x01' {
            hidden = true;
            i += 1;
            continue;
        }
        if ch == '\x02' {
            hidden = false;
            i += 1;
            continue;
        }
        if ch != '\\' {
            decoded.push((ch, hidden));
            i += 1;
            continue;
        }
        let Some(&next) = chars.get(i + 1) else {
            decoded.push((ch, hidden));
            i += 1;
            continue;
        };
        match next {
            '[' => {
                hidden = true;
                i += 2;
            }
            ']' => {
                hidden = false;
                i += 2;
            }
            '0'..='7' => {
                // The shell reads an octal escape only when three octal
                // digits follow, and the value it produces is one byte.
                let octal = chars.get(i + 1..i + 4).filter(|digits| {
                    digits
                        .iter()
                        .all(|digit| digit.is_ascii_digit() && *digit < '8')
                });
                if let Some(digits) = octal {
                    let value = digits
                        .iter()
                        .fold(0u32, |acc, digit| acc * 8 + u32::from(*digit as u8 - b'0'));
                    match (value & 0xff) as u8 {
                        // A NUL byte cannot be in the shell's prompt.
                        0 => {}
                        // The invisible-region markers.
                        0x01 => hidden = true,
                        0x02 => hidden = false,
                        byte => decoded.push((char::from(byte), hidden)),
                    }
                    i += 4;
                } else {
                    decoded.push((ch, hidden));
                    i += 1;
                }
            }
            other => match prompt_escape(other) {
                Some(decoded_char) => {
                    decoded.push((decoded_char, hidden));
                    i += 2;
                }
                None => {
                    decoded.push((ch, hidden));
                    decoded.push((other, hidden));
                    i += 2;
                }
            },
        }
    }

    // The second pass removes the quoting the shell's expansion removes.
    let mut quoted: Vec<(char, bool)> = Vec::with_capacity(decoded.len());
    let mut i = 0;
    while i < decoded.len() {
        let (ch, hidden) = decoded[i];
        let next = decoded.get(i + 1).map(|(next, _)| *next);
        match (ch, next) {
            ('\\', Some(next)) => match quoted_escape(next) {
                Some(quoted_char) => {
                    quoted.push((quoted_char, hidden));
                    i += 2;
                }
                None => {
                    if next == '\n' {
                        i += 2;
                    } else {
                        quoted.push((ch, hidden));
                        i += 1;
                    }
                }
            },
            _ => {
                quoted.push((ch, hidden));
                i += 1;
            }
        }
    }
    quoted
}

fn parse_prompt(raw: &str, escapes: PromptEscapes, mut push_decoded: impl FnMut(char, bool)) {
    match escapes {
        PromptEscapes::None => {
            let mut hidden = false;
            for ch in raw.chars() {
                if ch == '\x01' {
                    hidden = true;
                    continue;
                }
                if ch == '\x02' {
                    hidden = false;
                    continue;
                }
                push_decoded(ch, hidden);
            }
        }
        PromptEscapes::Shell => {
            for (ch, hidden) in decode_shell_prompt(raw) {
                push_decoded(ch, hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests;
