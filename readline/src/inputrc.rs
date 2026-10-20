use crate::config::Config;
use crate::keymap::{
    EditCommand, KeyBinding, KeyMap, KeyMapName, KeySequence,
    is_bindable_function_name_with_registers,
};
use crate::variables::Variables;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
/// InputrcError.
pub struct InputrcError {
    /// Line.
    pub line: usize,
    /// Message.
    pub message: String,
}

/// A non-fatal inputrc problem that GNU Readline reports while parsing.
///
/// GNU Readline prints these diagnostics and keeps reading the file, so a
/// typo in one `set` directive does not discard the rest of the inputrc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputrcDiagnostic {
    /// File the directive came from, when the inputrc was read from disk.
    pub path: Option<PathBuf>,
    /// Line number in that file.
    pub line: usize,
    /// Variable name as written in the directive.
    pub variable: String,
    /// Message text, like GNU Readline's diagnostic.
    pub message: String,
}

struct ParseContext<'a> {
    config: &'a Config,
    keymap: &'a mut KeyMap,
    variables: &'a mut Variables,
    binding_map: &'a mut KeyMapName,
}

impl InputrcError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

#[derive(Debug)]
/// InputrcParser.
pub struct InputrcParser {
    max_include_depth: usize,
    readline_version: String,
}

impl Default for InputrcParser {
    fn default() -> Self {
        Self::new()
    }
}

impl InputrcParser {
    /// New.
    pub fn new() -> Self {
        Self {
            max_include_depth: 17,
            readline_version: "8.3".to_string(),
        }
    }

    /// Parse str.
    pub fn parse_str(
        &self,
        source: &str,
        config: &Config,
        keymap: &mut KeyMap,
        variables: &mut Variables,
    ) -> Result<Vec<InputrcDiagnostic>, InputrcError> {
        let mut binding_map = keymap.current();
        let mut ctx = ParseContext {
            config,
            keymap,
            variables,
            binding_map: &mut binding_map,
        };
        let mut diagnostics = Vec::new();
        self.parse_bytes_inner(source.as_bytes(), &mut ctx, 0, None, &mut diagnostics)?;
        Ok(diagnostics)
    }

    /// Parse bytes.
    ///
    /// Byte-tolerant entry for inputrc content that may contain non-UTF-8
    /// bytes. Blank lines and `#` comments are detected before UTF-8 decoding
    /// so valid lines survive invalid bytes elsewhere. `\\nnn`/`\\xHH`
    /// escapes remain the portable form; the measured raw-byte path accepts a
    /// bare `0xFF` inside a double-quoted binding key. Complete UTF-8
    /// binding keys and command/macro values survive invalid trailing bytes;
    /// invalid bytes in set directives, command names, quoted
    /// macro/value bodies, and other line types are skipped.
    pub fn parse_bytes(
        &self,
        source: &[u8],
        config: &Config,
        keymap: &mut KeyMap,
        variables: &mut Variables,
    ) -> Result<Vec<InputrcDiagnostic>, InputrcError> {
        let mut binding_map = keymap.current();
        let mut ctx = ParseContext {
            config,
            keymap,
            variables,
            binding_map: &mut binding_map,
        };
        let mut diagnostics = Vec::new();
        self.parse_bytes_inner(source, &mut ctx, 0, None, &mut diagnostics)?;
        Ok(diagnostics)
    }

    /// Parse file.
    pub fn parse_file(
        &self,
        path: &Path,
        config: &Config,
        keymap: &mut KeyMap,
        variables: &mut Variables,
    ) -> Result<Vec<InputrcDiagnostic>, InputrcError> {
        let bytes =
            fs::read(path).map_err(|e| InputrcError::new(0, format!("{}: {e}", path.display())))?;
        let mut binding_map = keymap.current();
        let mut ctx = ParseContext {
            config,
            keymap,
            variables,
            binding_map: &mut binding_map,
        };
        let mut diagnostics = Vec::new();
        self.parse_bytes_inner(&bytes, &mut ctx, 0, Some(path), &mut diagnostics)?;
        Ok(diagnostics)
    }

    fn parse_bytes_inner(
        &self,
        source: &[u8],
        ctx: &mut ParseContext<'_>,
        include_depth: usize,
        path: Option<&Path>,
        diagnostics: &mut Vec<InputrcDiagnostic>,
    ) -> Result<(), InputrcError> {
        if include_depth > self.max_include_depth {
            return Err(InputrcError::new(0, "inputrc include depth exceeded"));
        }

        let mut active_stack = vec![true];
        let mut condition_stack = Vec::new();
        let mut line_count = 0;

        for (idx, raw_bytes) in split_inputrc_lines(source).enumerate() {
            let line_no = idx + 1;
            line_count = line_no;
            // Byte-level fast path: blank lines and `#` comments survive
            // even when the rest of the line is not valid UTF-8.
            let trimmed_bytes = trim_ascii(raw_bytes);
            if trimmed_bytes.is_empty() || trimmed_bytes.starts_with(b"#") {
                continue;
            }
            let decoded = match std::str::from_utf8(trimmed_bytes) {
                Ok(decoded) => decoded,
                Err(_) => {
                    // This fast path runs before directive dispatch, so it
                    // must respect the active conditional itself. A raw byte
                    // inside a double-quoted key binds as that byte (measured
                    // for `0xFF`); other invalid bytes stay on the rejection
                    // path below.
                    if *active_stack.last().unwrap_or(&true)
                        && try_bind_raw_quoted_key(trimmed_bytes, ctx)
                    {
                        continue;
                    }
                    // GNU accepts a complete binding command or quoted
                    // macro and ignores trailing text. Keep invalid bytes
                    // out of key/value syntax; do not apply this rule to set
                    // directives or other inputrc lines.
                    let Some((_, prefix_end)) = binding_value_prefix_bounds(trimmed_bytes) else {
                        continue;
                    };
                    let Ok(decoded) = std::str::from_utf8(&trimmed_bytes[..prefix_end]) else {
                        continue;
                    };
                    decoded
                }
            };
            let line = decoded.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(rest) = line.strip_prefix('$') {
                let mut parts = rest.split_whitespace();
                match parts.next() {
                    Some("if") => {
                        let cond = parts.collect::<Vec<_>>().join(" ");
                        let matched = eval_condition(
                            &cond,
                            ctx.config,
                            ctx.keymap,
                            ctx.variables,
                            *ctx.binding_map,
                            self.readline_version.as_str(),
                        );
                        let parent = *active_stack.last().unwrap_or(&true);
                        condition_stack.push(matched);
                        active_stack.push(parent && matched);
                    }
                    Some("else") => {
                        let parent = active_stack
                            .get(active_stack.len().saturating_sub(2))
                            .copied()
                            .unwrap_or(true);
                        let prev = condition_stack
                            .last_mut()
                            .ok_or_else(|| InputrcError::new(line_no, "$else without $if"))?;
                        *prev = !*prev;
                        if let Some(active) = active_stack.last_mut() {
                            *active = parent && *prev;
                        }
                    }
                    Some("endif") => {
                        condition_stack
                            .pop()
                            .ok_or_else(|| InputrcError::new(line_no, "$endif without $if"))?;
                        active_stack.pop();
                    }
                    Some("include") if *active_stack.last().unwrap_or(&true) => {
                        let path = parts.collect::<Vec<_>>().join(" ");
                        let path = expand_include_path(&path);
                        let Ok(included) = fs::read(&path) else {
                            continue;
                        };
                        self.parse_bytes_inner(
                            &included,
                            ctx,
                            include_depth + 1,
                            Some(&path),
                            diagnostics,
                        )?;
                    }
                    Some("include") => {}
                    Some(_) => {}
                    None => {}
                }
                continue;
            }

            if !*active_stack.last().unwrap_or(&true) {
                continue;
            }

            if let Some(after) = line
                .get(..3)
                .filter(|prefix| prefix.eq_ignore_ascii_case("set"))
                .and_then(|_| line.get(3..))
                .filter(|after| after.is_empty() || after.starts_with(char::is_whitespace))
            {
                let rest = after.trim_start_matches(char::is_whitespace);
                let mut parts = rest.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap_or("");
                let value = parts.next().unwrap_or("").trim();
                if let Some(message) = apply_variable(ctx.keymap, ctx.variables, name, value) {
                    diagnostics.push(InputrcDiagnostic {
                        path: path.map(Path::to_path_buf),
                        line: line_no,
                        variable: name.to_string(),
                        message,
                    });
                }
                if name.eq_ignore_ascii_case("keymap")
                    && let Some(map) = KeyMapName::parse(value)
                {
                    *ctx.binding_map = map.canonical();
                } else if name.eq_ignore_ascii_case("editing-mode") {
                    *ctx.binding_map = ctx.keymap.current();
                }
                continue;
            }

            if let Err(message) =
                parse_binding_line_in_map(line, ctx.keymap, *ctx.binding_map, ctx.variables)
                && !message.starts_with("unknown readline command:")
            {
                return Err(InputrcError::new(line_no, message));
            }
        }

        if active_stack.len() != 1 {
            return Err(InputrcError::new(line_count, "unterminated $if"));
        }

        Ok(())
    }
}

fn split_inputrc_lines(source: &[u8]) -> impl Iterator<Item = &[u8]> {
    // Mirror `str::lines` semantics: no trailing empty after a final `\n`,
    // interior blank lines preserved, empty input yields zero lines.
    let mut parts: Vec<&[u8]> = source
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r".as_slice()).unwrap_or(line))
        .collect();
    if !source.is_empty()
        && source.ends_with(b"\n")
        && parts.last().is_some_and(|last| last.is_empty())
    {
        parts.pop();
    }
    if source.is_empty() {
        parts.clear();
    }
    parts.into_iter()
}

/// Bind a double-quoted key containing raw non-UTF-8 bytes.
///
/// Normalize non-UTF-8 bytes in a bare double-quoted key to octal escapes and
/// reuse the regular binding parser for command and macro semantics. Keys
/// containing backslash syntax stay on the old skip path.
fn try_bind_raw_quoted_key(line: &[u8], ctx: &mut ParseContext<'_>) -> bool {
    let Some((colon, prefix_end)) = binding_value_prefix_bounds(line) else {
        return false;
    };
    let key = trim_ascii(&line[..colon]);
    if key.len() < 2 || !key.starts_with(b"\"") || !key.ends_with(b"\"") {
        return false;
    }
    if quoted_value_end(key) != Some(key.len()) {
        return false;
    }
    let inner = &key[1..key.len() - 1];
    if std::str::from_utf8(inner).is_ok() || inner.contains(&b'\\') {
        return false;
    }
    let Ok(value) = std::str::from_utf8(&line[colon..prefix_end]) else {
        return false;
    };

    let mut normalized = String::with_capacity(key.len() + value.len());
    normalized.push('"');
    for byte in inner {
        if byte.is_ascii() {
            normalized.push(*byte as char);
        } else {
            normalized.push('\\');
            normalized.push((b'0' + (byte >> 6)) as char);
            normalized.push((b'0' + ((byte >> 3) & 0x07)) as char);
            normalized.push((b'0' + (byte & 0x07)) as char);
        }
    }
    normalized.push('"');
    normalized.push_str(value);
    parse_binding_line_in_map(&normalized, ctx.keymap, *ctx.binding_map, ctx.variables).is_ok()
}

fn binding_value_prefix_bounds(line: &[u8]) -> Option<(usize, usize)> {
    let trimmed = trim_ascii(line);
    if trimmed.starts_with(b"$")
        || (trimmed.len() > 3
            && trimmed[..3].eq_ignore_ascii_case(b"set")
            && trimmed[3].is_ascii_whitespace())
    {
        return None;
    }

    let mut quote = None;
    let mut escaped = false;
    let mut colon = None;
    for (index, byte) in line.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        match byte {
            b'"' | b'\'' if quote == Some(byte) => quote = None,
            b'"' | b'\'' if quote.is_none() => quote = Some(byte),
            b'#' if quote.is_none() && (index == 0 || line[index - 1].is_ascii_whitespace()) => {
                return None;
            }
            b':' if quote.is_none() => colon = Some(index),
            _ => {}
        }
        if colon.is_some() {
            break;
        }
    }
    let colon = colon?;
    let after_colon = &line[colon + 1..];
    let leading = after_colon
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    let command_start = colon + 1 + leading;
    let value = &line[command_start..];
    if matches!(value.first(), Some(b'"' | b'\'')) {
        return Some((colon, command_start + quoted_value_end(value)?));
    }
    let command_len = value
        .iter()
        .position(|byte| byte.is_ascii_whitespace())
        .unwrap_or(value.len());
    if command_len == 0 {
        return None;
    }
    std::str::from_utf8(&value[..command_len]).ok()?;
    Some((colon, command_start + command_len))
}

fn quoted_value_end(value: &[u8]) -> Option<usize> {
    let quote = value
        .first()
        .copied()
        .filter(|quote| matches!(quote, b'"' | b'\''))?;
    let mut escaped = false;
    for (offset, byte) in value.iter().copied().enumerate().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        if byte == quote {
            return Some(offset + 1);
        }
    }
    None
}

fn trim_ascii(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    let end = line
        .iter()
        .rev()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    if start + end >= line.len() {
        return b"";
    }
    &line[start..line.len() - end]
}

/// Parse binding line in map.
pub fn parse_binding_line_in_map(
    line: &str,
    keymap: &mut KeyMap,
    map: KeyMapName,
    variables: &Variables,
) -> Result<(), String> {
    let Some(pos) = find_unquoted_colon(line) else {
        return Err("missing ':' in binding".to_string());
    };
    let key = line[..pos].trim();
    let value = line[pos + 1..].trim();
    let meta_prefix =
        variable_is_on(variables, "force-meta-prefix") || variable_is_on(variables, "convert-meta");
    let seq = KeySequence::parse_with_meta(key, meta_prefix)?;
    // GNU consumes one complete quoted macro value and ignores any suffix.
    let binding = if let Some(end) = quoted_value_end(value.as_bytes()) {
        KeyBinding::Macro(decode_inputrc_bytes(&value[..end], meta_prefix)?)
    } else if let Some(command) = value.split_whitespace().next().and_then(EditCommand::parse) {
        KeyBinding::Command(command)
    } else if let Some(command) = value.split_whitespace().next().filter(|command| {
        is_bindable_function_name_with_registers(command, variables.is_on("sushline-vi-registers"))
    }) {
        KeyBinding::NamedCommand(command.to_string())
    } else {
        return Err(format!("unknown readline command: {value}"));
    };
    keymap.bind_user(map, seq, binding);
    Ok(())
}

fn variable_is_on(variables: &Variables, name: &str) -> bool {
    variables.is_on(name)
}

/// Apply variable.
/// Apply a `set` directive.
///
/// Returns GNU Readline's diagnostic when the directive names an unknown
/// variable or an invalid value for a variable that GNU validates; the
/// directive is then ignored like GNU ignores it.
pub fn apply_variable(
    keymap: &mut KeyMap,
    variables: &mut Variables,
    name: &str,
    value: &str,
) -> Option<String> {
    let canonical_name = name.to_ascii_lowercase();
    if !is_known_set_variable(&canonical_name) {
        return Some("unknown variable name".to_string());
    }
    if let Some(message) = invalid_set_value(&canonical_name, value) {
        return Some(message);
    }
    let normalized = normalize_variable_value(&canonical_name, value)?;
    let byte_value = normalize_variable_value_bytes(&canonical_name, value);
    if canonical_name == "editing-mode" {
        keymap.set_current(match normalized.as_str() {
            "vi" => KeyMapName::ViInsert,
            _ => KeyMapName::EmacsStandard,
        });
    } else if canonical_name == "keymap" && KeyMapName::parse(&normalized).is_none() {
        return None;
    }
    variables.insert(canonical_name.clone(), normalized);
    if let Some(bytes) = byte_value {
        variables.insert_bytes(canonical_name, bytes);
    }
    None
}

/// True when GNU Readline 8.3 accepts the variable in a `set` directive.
fn is_known_set_variable(name: &str) -> bool {
    READLINE_BOOLEAN_VARIABLES.contains(&name) || SET_VARIABLE_NAMES.contains(&name)
}

/// GNU Readline's diagnostic for a value that its variable rejects.
///
/// GNU validates the value of `bell-style`, `comment-begin`, `editing-mode`,
/// and `keymap`; the other variables accept any value.
fn invalid_set_value(name: &str, value: &str) -> Option<String> {
    let normalized = value.to_ascii_lowercase();
    let invalid = match name {
        "bell-style" => !matches!(
            normalized.as_str(),
            "" | "none" | "visible" | "audible" | "on" | "off"
        ),
        "comment-begin" => value.is_empty(),
        "editing-mode" => canonical_editing_mode(value).is_none(),
        "keymap" => KeyMapName::parse(value).is_none(),
        _ => false,
    };
    invalid.then(|| format!("could not set value to `{value}'"))
}

/// The `editing-mode` value GNU Readline derives from an inputrc value.
///
/// GNU accepts any value that starts with `vi` or `emacs`, selects the mode
/// that prefix names, and reports the mode it selected.
fn canonical_editing_mode(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();
    if normalized.starts_with("vi") {
        Some("vi")
    } else if normalized.starts_with("emacs") {
        Some("emacs")
    } else {
        None
    }
}

fn eval_condition(
    cond: &str,
    config: &Config,
    keymap: &KeyMap,
    variables: &Variables,
    binding_map: KeyMapName,
    readline_version: &str,
) -> bool {
    let cond = cond.trim();
    if cond.eq_ignore_ascii_case(&config.application_name) {
        return true;
    }
    if let Some((op, lhs, rhs)) = parse_condition_comparison(cond) {
        let name = lhs.to_ascii_lowercase();
        let expected = rhs.trim();
        if name == "version" {
            let ordering = compare_versions(readline_version, expected);
            return comparison_matches(ordering, op);
        }
        if name == "term" {
            if !matches!(op, "=" | "==" | "!=") {
                return false;
            }
            return std::env::var("TERM")
                .map(|value| {
                    let matched = value == expected || value.starts_with(&format!("{expected}-"));
                    if op == "!=" { !matched } else { matched }
                })
                .unwrap_or(false);
        }
        if name == "mode" {
            if !matches!(op, "=" | "==" | "!=") {
                return false;
            }
            let matched = match expected {
                "vi" => matches!(
                    keymap.current(),
                    KeyMapName::ViCommand | KeyMapName::ViInsert
                ),
                "emacs" => matches!(keymap.current(), KeyMapName::EmacsStandard),
                _ => false,
            };
            return if op == "!=" { !matched } else { matched };
        }
        let _ = (variables, binding_map);
    }
    false
}

fn parse_condition_comparison(cond: &str) -> Option<(&str, &str, &str)> {
    for op in ["<=", ">=", "==", "!=", "<", ">", "="] {
        if let Some((lhs, rhs)) = cond.split_once(op) {
            return Some((op, lhs.trim(), rhs.trim()));
        }
    }
    let mut parts = cond.split_whitespace();
    if let (Some(lhs), Some(op), Some(rhs), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
        && matches!(op, "<=" | ">=" | "==" | "!=" | "<" | ">" | "=")
    {
        return Some((op, lhs, rhs));
    }
    None
}

fn comparison_matches(ordering: std::cmp::Ordering, op: &str) -> bool {
    match op {
        "<" => ordering.is_lt(),
        "<=" => !ordering.is_gt(),
        ">" => ordering.is_gt(),
        ">=" => !ordering.is_lt(),
        "=" | "==" => ordering.is_eq(),
        "!=" => !ordering.is_eq(),
        _ => false,
    }
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let parse = |value: &str| {
        value
            .split('.')
            .map(|part| part.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    let left = parse(left);
    let right = parse(right);
    let len = left.len().max(right.len());
    for idx in 0..len {
        match left
            .get(idx)
            .unwrap_or(&0)
            .cmp(right.get(idx).unwrap_or(&0))
        {
            std::cmp::Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    std::cmp::Ordering::Equal
}

fn expand_include_path(path: &str) -> PathBuf {
    let mut path = path.trim().to_string();
    if let Ok(decoded) = decode_inputrc_string(&path) {
        path = decoded;
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    if let Some(rest) = path.strip_prefix('~') {
        let (user, suffix) = rest.split_once('/').unwrap_or((rest, ""));
        if !user.is_empty()
            && let Some(home) = user_home_dir(user)
        {
            return if suffix.is_empty() {
                home
            } else {
                home.join(suffix)
            };
        }
    }
    PathBuf::from(path)
}

fn user_home_dir(user: &str) -> Option<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next() == Some(user))
            .then(|| fields.nth(4).map(PathBuf::from))
            .flatten()
    })
}

fn find_unquoted_colon(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' | '\'' if quote == Some(ch) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(ch),
            ':' if quote.is_none() => return Some(idx),
            _ => {}
        }
    }
    None
}

fn is_quoted(value: &str) -> bool {
    quoted_value_end(value.as_bytes()) == Some(value.len())
}

fn normalize_variable_value(name: &str, value: &str) -> Option<String> {
    let value = value.trim();
    if name == "history-size" && value.parse::<isize>().is_err() {
        return Some("0".to_string());
    }
    if matches!(
        name,
        "completion-display-width"
            | "completion-prefix-display-length"
            | "completion-query-items"
            | "history-size"
            | "keyseq-timeout"
    ) {
        if value.parse::<isize>().is_ok() {
            return Some(value.to_string());
        }
        return Some("0".to_string());
    }
    if name == "editing-mode" {
        return canonical_editing_mode(value).map(str::to_string);
    }
    if name == "keymap" {
        return Some(value.to_ascii_lowercase());
    }
    if matches!(
        name,
        "active-region-start-color" | "active-region-end-color" | "isearch-terminators"
    ) {
        return Some(if is_quoted(value) {
            decode_inputrc_string(value).unwrap_or_else(|_| value.to_string())
        } else {
            value.to_string()
        });
    }
    if matches!(
        name,
        "emacs-mode-string" | "vi-cmd-mode-string" | "vi-ins-mode-string"
    ) {
        return Some(if is_quoted(value) {
            decode_inputrc_string(value).unwrap_or_else(|_| value.to_string())
        } else {
            value.to_string()
        });
    }
    if name == "comment-begin" {
        return Some(if is_quoted(value) {
            value[1..value.len() - 1].to_string()
        } else {
            value.to_string()
        });
    }
    if name == "bell-style" {
        // GNU keeps the current bell style when the value is empty and
        // canonicalizes `on` and `off` to `audible` and `none`.
        return match value.to_ascii_lowercase().as_str() {
            "" => None,
            "on" => Some("audible".to_string()),
            "off" => Some("none".to_string()),
            other => Some(other.to_string()),
        };
    }
    if !READLINE_BOOLEAN_VARIABLES.contains(&name) {
        return None;
    }
    if value.is_empty() || value.eq_ignore_ascii_case("on") || value == "1" {
        Some("on".to_string())
    } else {
        Some("off".to_string())
    }
}

fn normalize_variable_value_bytes(name: &str, value: &str) -> Option<Vec<u8>> {
    if matches!(
        name,
        "active-region-start-color"
            | "active-region-end-color"
            | "isearch-terminators"
            | "emacs-mode-string"
            | "vi-cmd-mode-string"
            | "vi-ins-mode-string"
    ) {
        return Some(if is_quoted(value) {
            decode_inputrc_bytes(value, true).ok()?
        } else {
            value.as_bytes().to_vec()
        });
    }
    normalize_variable_value(name, value).map(String::into_bytes)
}

fn decode_inputrc_string(value: &str) -> Result<String, String> {
    let bytes = decode_inputrc_bytes(value, true)?;
    String::from_utf8(bytes).map_err(|_| "decoded string is not valid utf-8".to_string())
}

fn decode_inputrc_bytes(value: &str, meta_prefix: bool) -> Result<Vec<u8>, String> {
    let quote = value
        .as_bytes()
        .first()
        .copied()
        .filter(|quote| matches!(quote, b'"' | b'\''))
        .ok_or_else(|| "expected quoted string".to_string())? as char;
    let inner = value
        .strip_prefix(quote)
        .and_then(|value| value.strip_suffix(quote))
        .ok_or_else(|| "expected quoted string".to_string())?;
    let keyseq = KeySequence::parse_with_meta(&format!("\"{inner}\""), meta_prefix)?;
    Ok(keyseq.bytes().to_vec())
}

/// Non-boolean variables GNU Readline 8.3 accepts in a `set` directive,
/// plus Sushline's own string variables.
const SET_VARIABLE_NAMES: &[&str] = &[
    "active-region-end-color",
    "active-region-start-color",
    "bell-style",
    "comment-begin",
    "completion-display-width",
    "completion-prefix-display-length",
    "completion-query-items",
    "editing-mode",
    "emacs-mode-string",
    "history-size",
    "isearch-terminators",
    "keymap",
    "keyseq-timeout",
    "vi-cmd-mode-string",
    "vi-ins-mode-string",
];

const READLINE_BOOLEAN_VARIABLES: &[&str] = &[
    "bind-tty-special-chars",
    "blink-matching-paren",
    "byte-oriented",
    "colored-completion-prefix",
    "colored-stats",
    "completion-ignore-case",
    "completion-map-case",
    "convert-meta",
    "disable-completion",
    "echo-control-characters",
    "enable-active-region",
    "enable-bracketed-paste",
    "enable-keypad",
    "enable-meta-key",
    "expand-tilde",
    "force-meta-prefix",
    "history-preserve-point",
    "horizontal-scroll-mode",
    "input-meta",
    "mark-directories",
    "mark-modified-lines",
    "mark-symlinked-directories",
    "match-hidden-files",
    "menu-complete-display-prefix",
    "meta-flag",
    "output-meta",
    "page-completions",
    "prefer-visible-bell",
    "print-completions-horizontally",
    "revert-all-at-newline",
    "search-ignore-case",
    "show-all-if-ambiguous",
    "show-all-if-unmodified",
    "show-mode-in-prompt",
    "skip-completed-text",
    "sushline-vi-registers",
    "visible-stats",
];

#[cfg(test)]
mod tests;

pub(crate) fn discover_inputrc_path() -> PathBuf {
    if let Ok(path) = std::env::var("INPUTRC")
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }
    let user_inputrc = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".inputrc");
    if user_inputrc.is_file() {
        user_inputrc
    } else {
        PathBuf::from("/etc/inputrc")
    }
}
