use crate::config::Config;
use crate::keymap::{
    EditCommand, KeyBinding, KeyMap, KeyMapName, KeySequence, is_bindable_function_name,
};
use crate::variables::Variables;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputrcError {
    pub line: usize,
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

#[derive(Debug, Default)]
pub struct InputrcParser {
    max_include_depth: usize,
    readline_version: String,
}

impl InputrcParser {
    pub fn new() -> Self {
        Self {
            max_include_depth: 16,
            readline_version: "8.3".to_string(),
        }
    }

    pub fn parse_str(
        &self,
        source: &str,
        config: &Config,
        keymap: &mut KeyMap,
        variables: &mut Variables,
    ) -> Result<(), InputrcError> {
        let mut binding_map = keymap.current();
        let mut ctx = ParseContext {
            config,
            keymap,
            variables,
            binding_map: &mut binding_map,
        };
        self.parse_str_inner(source, &mut ctx, None, 0)
    }

    pub fn parse_file(
        &self,
        path: &Path,
        config: &Config,
        keymap: &mut KeyMap,
        variables: &mut Variables,
    ) -> Result<(), InputrcError> {
        let source = fs::read_to_string(path)
            .map_err(|e| InputrcError::new(0, format!("{}: {e}", path.display())))?;
        let mut binding_map = keymap.current();
        let mut ctx = ParseContext {
            config,
            keymap,
            variables,
            binding_map: &mut binding_map,
        };
        self.parse_str_inner(&source, &mut ctx, path.parent(), 0)
    }

    fn parse_str_inner(
        &self,
        source: &str,
        ctx: &mut ParseContext<'_>,
        base_dir: Option<&Path>,
        include_depth: usize,
    ) -> Result<(), InputrcError> {
        if include_depth > self.max_include_depth {
            return Err(InputrcError::new(0, "inputrc include depth exceeded"));
        }

        let mut active_stack = vec![true];
        let mut condition_stack = Vec::new();

        for (idx, raw_line) in source.lines().enumerate() {
            let line_no = idx + 1;
            let line = raw_line.trim();
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
                        let path = expand_include_path(&path, base_dir);
                        let included = fs::read_to_string(&path).map_err(|e| {
                            InputrcError::new(
                                line_no,
                                format!("{}: cannot include: {e}", path.display()),
                            )
                        })?;
                        self.parse_str_inner(&included, ctx, path.parent(), include_depth + 1)?;
                    }
                    Some("include") => {}
                    Some(other) => {
                        return Err(InputrcError::new(
                            line_no,
                            format!("unsupported inputrc directive: ${other}"),
                        ));
                    }
                    None => {}
                }
                continue;
            }

            if !*active_stack.last().unwrap_or(&true) {
                continue;
            }

            if let Some(rest) = line.strip_prefix("set ") {
                let mut parts = rest.splitn(2, char::is_whitespace);
                let Some(name) = parts.next() else {
                    continue;
                };
                let value = parts.next().unwrap_or("").trim();
                apply_variable(ctx.keymap, ctx.variables, name, value);
                if name.eq_ignore_ascii_case("keymap")
                    && let Some(map) = KeyMapName::parse(value)
                {
                    *ctx.binding_map = map.canonical();
                } else if name.eq_ignore_ascii_case("editing-mode") {
                    *ctx.binding_map = ctx.keymap.current();
                }
                continue;
            }

            parse_binding_line_in_map(line, ctx.keymap, *ctx.binding_map, ctx.variables)
                .map_err(|message| InputrcError::new(line_no, message))?;
        }

        if active_stack.len() != 1 {
            return Err(InputrcError::new(
                source.lines().count(),
                "unterminated $if",
            ));
        }

        Ok(())
    }
}

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
    let binding = if is_quoted(value) {
        KeyBinding::Macro(decode_inputrc_bytes(value, meta_prefix)?)
    } else if let Some(command) = value.split_whitespace().next().and_then(EditCommand::parse) {
        KeyBinding::Command(command)
    } else if let Some(command) = value
        .split_whitespace()
        .next()
        .filter(|command| is_bindable_function_name(command))
    {
        KeyBinding::NamedCommand(command.to_string())
    } else {
        return Err(format!("unknown readline command: {value}"));
    };
    keymap.bind(map, seq, binding);
    Ok(())
}

fn variable_is_on(variables: &Variables, name: &str) -> bool {
    variables
        .get(name)
        .map(|value| matches!(value.as_str(), "on" | "1" | ""))
        .unwrap_or(false)
}

pub fn apply_variable(keymap: &mut KeyMap, variables: &mut Variables, name: &str, value: &str) {
    let canonical_name = name.to_ascii_lowercase();
    let Some(normalized) = normalize_variable_value(&canonical_name, value) else {
        return;
    };
    let byte_value = normalize_variable_value_bytes(&canonical_name, value);
    if canonical_name == "editing-mode" {
        match normalized.as_str() {
            "vi" => keymap.set_current(KeyMapName::ViInsert),
            "emacs" => keymap.set_current(KeyMapName::EmacsStandard),
            _ => return,
        }
    } else if canonical_name == "keymap" && KeyMapName::parse(&normalized).is_none() {
        return;
    }
    variables.insert(canonical_name.clone(), normalized);
    if let Some(bytes) = byte_value {
        variables.insert_bytes(canonical_name, bytes);
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
        if !matches!(op, "=" | "==" | "!=") {
            return false;
        }
        let actual = variables
            .get(name.as_str())
            .map(String::as_str)
            .or_else(|| {
                if name == "editing-mode" {
                    Some(match keymap.current() {
                        KeyMapName::ViCommand | KeyMapName::ViInsert => "vi",
                        _ => "emacs",
                    })
                } else if name == "keymap" {
                    Some(binding_map.as_str())
                } else {
                    None
                }
            });
        return match op {
            "=" | "==" => actual.map(|value| value == expected).unwrap_or(false),
            "!=" => actual.map(|value| value != expected).unwrap_or(true),
            _ => false,
        };
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

fn expand_include_path(path: &str, base_dir: Option<&Path>) -> PathBuf {
    let mut path = path.trim().to_string();
    if let Ok(decoded) = decode_inputrc_string(&path) {
        path = decoded;
    }
    path = expand_env_vars(&path);
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
    let path = PathBuf::from(path);
    if path.is_relative()
        && let Some(base_dir) = base_dir
    {
        return base_dir.join(path);
    }
    path
}

fn expand_env_vars(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            out.push(ch);
            continue;
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            let mut name = String::new();
            while let Some(next) = chars.peek().copied() {
                chars.next();
                if next == '}' {
                    break;
                }
                name.push(next);
            }
            out.push_str(&std::env::var(name).unwrap_or_default());
            continue;
        }
        let mut name = String::new();
        while let Some(next) = chars.peek().copied() {
            if next == '_' || next.is_ascii_alphanumeric() {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }
        if name.is_empty() {
            out.push('$');
        } else {
            out.push_str(&std::env::var(name).unwrap_or_default());
        }
    }
    out
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
    value.len() >= 2 && value.starts_with('"') && value.ends_with('"')
}

fn normalize_variable_value(name: &str, value: &str) -> Option<String> {
    let value = value.trim();
    if name == "history-size" && value.parse::<isize>().is_err() {
        return Some("500".to_string());
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
        return None;
    }
    if matches!(name, "editing-mode" | "keymap") {
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
    if matches!(
        name,
        "bell-style"
            | "comment-begin"
            | "histchars"
            | "history-word-delimiters"
            | "history-search-delimiter-chars"
            | "history-no-expand-chars"
    ) {
        return Some(value.to_string());
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
    let inner = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| "expected quoted string".to_string())?;
    let keyseq = KeySequence::parse_with_meta(&format!("\"{inner}\""), meta_prefix)?;
    Ok(keyseq.bytes().to_vec())
}

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
    "history-quotes-inhibit-expansion",
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
