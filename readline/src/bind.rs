use crate::config::Config;
use crate::inputrc::{InputrcError, InputrcParser, apply_variable, parse_binding_line_in_map};
use crate::keymap::{
    BIND_FUNCTION_NAMES, EditCommand, KeyBinding, KeyMap, KeyMapName, KeySequence,
    is_bindable_function_name,
};
use crate::variables::Variables;
use std::fs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindQuery {
    ListFunctionNames,
    PrintReusable,
    PrintFunctions,
    PrintVariablesReusable,
    PrintVariables,
    PrintMacrosReusable,
    PrintMacros,
    PrintApplicationCommandsReusable,
    PrintApplicationCommands,
    QueryFunction(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindError {
    pub message: String,
}

impl From<String> for BindError {
    fn from(message: String) -> Self {
        Self { message }
    }
}

impl From<InputrcError> for BindError {
    fn from(value: InputrcError) -> Self {
        Self {
            message: format!("line {}: {}", value.line, value.message),
        }
    }
}

pub struct BindApi<'a> {
    keymap: &'a mut KeyMap,
    variables: &'a mut Variables,
    config: Config,
    target_map: KeyMapName,
}

impl<'a> BindApi<'a> {
    pub(crate) fn with_config(
        keymap: &'a mut KeyMap,
        variables: &'a mut Variables,
        config: &Config,
    ) -> Self {
        let target_map = keymap.current();
        Self {
            keymap,
            variables,
            config: config.clone(),
            target_map,
        }
    }

    pub fn apply_line(&mut self, line: &str) -> Result<(), BindError> {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("set ") {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or("").trim();
            apply_variable(self.keymap, self.variables, name, value);
            if name.eq_ignore_ascii_case("keymap")
                && let Some(map) = KeyMapName::parse(value)
            {
                self.target_map = map.canonical();
            } else if name.eq_ignore_ascii_case("editing-mode") {
                self.target_map = self.keymap.current();
            }
            return Ok(());
        }
        parse_binding_line_in_map(line, self.keymap, self.target_map, self.variables)
            .map_err(BindError::from)
    }

    pub fn bind_application_command(
        &mut self,
        key: &str,
        command: impl Into<String>,
    ) -> Result<(), BindError> {
        let seq = KeySequence::parse(key).map_err(BindError::from)?;
        self.keymap.bind(
            self.target_map,
            seq,
            KeyBinding::ApplicationCommand(command.into()),
        );
        Ok(())
    }

    pub fn bind_application_command_spec(&mut self, spec: &str) -> Result<(), BindError> {
        let (key, command) = split_bind_spec(spec).map_err(BindError::from)?;
        self.bind_application_command(key.trim(), unquote_value(command.trim())?)
    }

    pub fn unbind_application_command(&mut self, key: &str) -> Result<bool, BindError> {
        let seq = KeySequence::parse(key).map_err(BindError::from)?;
        let removed = matches!(
            self.keymap.unbind_key(self.target_map, &seq),
            Some(KeyBinding::ApplicationCommand(_))
        );
        Ok(removed)
    }

    pub fn apply_builtin_args(&mut self, args: &[&str]) -> Result<String, BindError> {
        let mut output = String::new();
        let mut idx = 0;
        while idx < args.len() {
            match args[idx] {
                "-l" => output.push_str(&self.print(BindQuery::ListFunctionNames)),
                "-p" => output.push_str(&self.print(BindQuery::PrintReusable)),
                "-P" => output.push_str(&self.print(BindQuery::PrintFunctions)),
                "-v" => output.push_str(&self.print(BindQuery::PrintVariablesReusable)),
                "-V" => output.push_str(&self.print(BindQuery::PrintVariables)),
                "-s" => output.push_str(&self.print(BindQuery::PrintMacrosReusable)),
                "-S" => output.push_str(&self.print(BindQuery::PrintMacros)),
                "-X" => output.push_str(&self.print(BindQuery::PrintApplicationCommandsReusable)),
                "-f" => {
                    idx += 1;
                    let path = args.get(idx).ok_or_else(|| BindError {
                        message: "-f: option requires an argument".to_string(),
                    })?;
                    fs::metadata(path).map_err(|err| {
                        let reason = if err.kind() == std::io::ErrorKind::NotFound {
                            "No such file or directory".to_string()
                        } else {
                            err.to_string()
                        };
                        BindError {
                            message: format!("{path}: cannot read: {reason}"),
                        }
                    })?;
                    InputrcParser::new()
                        .parse_file(
                            std::path::Path::new(path),
                            &self.config,
                            self.keymap,
                            self.variables,
                        )
                        .map_err(BindError::from)?;
                }
                "-m" => {
                    idx += 1;
                    let map = args.get(idx).ok_or_else(|| BindError {
                        message: "-m: option requires an argument".to_string(),
                    })?;
                    let map = KeyMapName::parse(map).ok_or_else(|| BindError {
                        message: format!("`{map}': invalid keymap name"),
                    })?;
                    self.target_map = map;
                }
                "-q" => {
                    idx += 1;
                    let name = args.get(idx).ok_or_else(|| BindError {
                        message: "-q: option requires an argument".to_string(),
                    })?;
                    output.push_str(&self.print(BindQuery::QueryFunction((*name).to_string())));
                }
                "-u" => {
                    idx += 1;
                    let name = args.get(idx).ok_or_else(|| BindError {
                        message: "-u: option requires an argument".to_string(),
                    })?;
                    self.unbind_command(name)?;
                }
                "-r" => {
                    idx += 1;
                    let key = args.get(idx).ok_or_else(|| BindError {
                        message: "-r: option requires an argument".to_string(),
                    })?;
                    self.unbind_key(key)?;
                }
                "-x" => {
                    idx += 1;
                    let spec = args.get(idx).ok_or_else(|| BindError {
                        message: "-x: option requires an argument".to_string(),
                    })?;
                    self.bind_application_command_spec(spec)?;
                }
                option if option.starts_with('-') && option.len() > 2 => {
                    for flag in option[1..].chars() {
                        match flag {
                            'l' => output.push_str(&self.print(BindQuery::ListFunctionNames)),
                            'p' => output.push_str(&self.print(BindQuery::PrintReusable)),
                            'P' => output.push_str(&self.print(BindQuery::PrintFunctions)),
                            'v' => output.push_str(&self.print(BindQuery::PrintVariablesReusable)),
                            'V' => output.push_str(&self.print(BindQuery::PrintVariables)),
                            's' => output.push_str(&self.print(BindQuery::PrintMacrosReusable)),
                            'S' => output.push_str(&self.print(BindQuery::PrintMacros)),
                            'X' => output
                                .push_str(&self.print(BindQuery::PrintApplicationCommandsReusable)),
                            'f' | 'm' | 'q' | 'u' | 'r' | 'x' => {
                                return Err(BindError {
                                    message: format!("-{flag}: option requires an argument"),
                                });
                            }
                            _ => {
                                return Err(BindError {
                                    message: format!("-{flag}: invalid option"),
                                });
                            }
                        }
                    }
                }
                option if option.starts_with('-') => {
                    return Err(BindError {
                        message: format!("{option}: invalid option"),
                    });
                }
                line => self.apply_line(line)?,
            }
            idx += 1;
        }
        Ok(output)
    }

    pub fn print(&self, query: BindQuery) -> String {
        match query {
            BindQuery::ListFunctionNames => self.list_function_names(),
            BindQuery::PrintReusable => self.print_reusable_bindings(None),
            BindQuery::PrintFunctions => self.print_functions(),
            BindQuery::PrintVariablesReusable => self.print_variables(true),
            BindQuery::PrintVariables => self.print_variables(false),
            BindQuery::PrintMacrosReusable => self.print_macros(true),
            BindQuery::PrintMacros => self.print_macros(false),
            BindQuery::PrintApplicationCommandsReusable => self.print_application_commands(true),
            BindQuery::PrintApplicationCommands => self.print_application_commands(false),
            BindQuery::QueryFunction(name) => self.print_query_function(&name),
        }
    }

    pub fn unbind_key(&mut self, key: &str) -> Result<bool, BindError> {
        let seq = KeySequence::parse(key).map_err(BindError::from)?;
        Ok(self.keymap.unbind_key(self.target_map, &seq).is_some())
    }

    pub fn unbind_command(&mut self, command: &str) -> Result<usize, BindError> {
        if !is_bindable_function_name(command) {
            return Err(BindError {
                message: format!("`{command}': unknown function name"),
            });
        }
        Ok(self.keymap.unbind_command(command))
    }

    fn list_function_names(&self) -> String {
        BIND_FUNCTION_NAMES.join("\n") + "\n"
    }

    fn print_reusable_bindings(&self, filter: Option<EditCommand>) -> String {
        if filter.is_none() {
            return self.print_reusable_keymap();
        }
        let mut lines = Vec::new();
        for (_, seq, binding) in self.keymap.iter() {
            match binding {
                KeyBinding::Command(command) if filter.is_none_or(|f| f == *command) => {
                    lines.push(format!("{}: {}", seq.display_inputrc(), command.as_str()));
                }
                KeyBinding::NamedCommand(command) if filter.is_none() => {
                    lines.push(format!("{}: {command}", seq.display_inputrc()));
                }
                KeyBinding::ApplicationCommand(command) if filter.is_none() => {
                    lines.push(format!(
                        "{}: \"{}\"",
                        seq.display_inputrc(),
                        escape(command)
                    ));
                }
                _ => {}
            }
        }
        lines.sort();
        lines.join("\n") + optional_newline(!lines.is_empty())
    }

    fn print_reusable_keymap(&self) -> String {
        let mut lines = Vec::new();
        for command in BIND_FUNCTION_NAMES {
            let mut bindings = self
                .keymap
                .bindings_for_command_name(command)
                .into_iter()
                .map(|(_, seq)| seq.display_inputrc())
                .collect::<Vec<_>>();
            bindings.sort();
            if bindings.is_empty() {
                lines.push(format!("# {command} (not bound)"));
            } else {
                for seq in bindings {
                    lines.push(format!("{seq}: {command}"));
                }
            }
        }
        for (_, seq, binding) in self.keymap.iter() {
            match binding {
                KeyBinding::NamedCommand(command)
                    if !BIND_FUNCTION_NAMES.contains(&command.as_str()) =>
                {
                    lines.push(format!("{}: {command}", seq.display_inputrc()));
                }
                KeyBinding::Macro(value) => {
                    lines.push(format!(
                        "{}: \"{}\"",
                        seq.display_inputrc(),
                        escape_bytes(value)
                    ));
                }
                KeyBinding::ApplicationCommand(command) => {
                    lines.push(format!(
                        "{}: \"{}\"",
                        seq.display_inputrc(),
                        escape(command)
                    ));
                }
                _ => {}
            }
        }
        lines.join("\n") + "\n"
    }

    fn print_functions(&self) -> String {
        let mut lines = Vec::new();
        for command in BIND_FUNCTION_NAMES {
            lines.push(self.function_binding_line(command, FunctionLineKind::Print));
        }
        lines.join("\n") + "\n"
    }

    fn print_query_function(&self, name: &str) -> String {
        if !is_bindable_function_name(name) {
            return format!("{name} is not a function\n");
        }
        self.function_binding_line(name, FunctionLineKind::Query) + "\n"
    }

    fn function_binding_line(&self, command: &str, kind: FunctionLineKind) -> String {
        let mut bindings = self
            .keymap
            .bindings_for_command_name(command)
            .into_iter()
            .map(|(_, seq)| seq.display_inputrc())
            .collect::<Vec<_>>();
        bindings.sort();
        bindings.dedup();
        if bindings.is_empty() {
            format!("{command} is not bound to any keys")
        } else {
            let phrase = match kind {
                FunctionLineKind::Print => "can be found on",
                FunctionLineKind::Query => "can be invoked via",
            };
            format!("{command} {phrase} {}.", bindings.join(", "))
        }
    }

    fn print_variables(&self, reusable: bool) -> String {
        let mut lines = Vec::new();
        for name in READLINE_VARIABLE_NAMES {
            if let Some(value) = self.variables.get(name) {
                lines.push(format_variable(name, value, reusable));
            }
        }
        for (name, value) in self.variables.iter().filter(|(name, _)| {
            !READLINE_VARIABLE_NAMES.contains(name)
                && !HIDDEN_READLINE_VARIABLE_NAMES.contains(name)
        }) {
            lines.push(format_variable(name, value, reusable));
        }
        lines.join("\n") + optional_newline(!lines.is_empty())
    }

    fn print_macros(&self, reusable: bool) -> String {
        let mut lines = Vec::new();
        for (_, seq, binding) in self.keymap.iter() {
            if let KeyBinding::Macro(value) = binding {
                if reusable {
                    lines.push(format!(
                        "{}: \"{}\"",
                        seq.display_inputrc(),
                        escape_bytes(value)
                    ));
                } else {
                    lines.push(format!(
                        "{} outputs {}",
                        seq.display_inputrc(),
                        escape_bytes(value)
                    ));
                }
            }
        }
        lines.sort();
        lines.join("\n") + optional_newline(!lines.is_empty())
    }

    fn print_application_commands(&self, reusable: bool) -> String {
        let mut lines = Vec::new();
        for (_, seq, binding) in self.keymap.iter() {
            if let KeyBinding::ApplicationCommand(command) = binding {
                if reusable {
                    lines.push(format!(
                        "{}: \"{}\"",
                        seq.display_inputrc(),
                        escape(command)
                    ));
                } else {
                    lines.push(format!(
                        "{} executes `{}`",
                        seq.display_inputrc(),
                        escape(command)
                    ));
                }
            }
        }
        lines.sort();
        lines.join("\n") + optional_newline(!lines.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FunctionLineKind {
    Print,
    Query,
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_bytes(value: &[u8]) -> String {
    let mut out = String::new();
    for byte in value {
        match *byte {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x1b => out.push_str("\\e"),
            0x7f => out.push_str("\\C-?"),
            0x00..=0x1f => {
                out.push_str("\\C-");
                out.push((*byte + 0x40) as char);
            }
            0x20..=0x7e => out.push(*byte as char),
            _ => out.push_str(&format!("\\x{byte:02x}")),
        }
    }
    out
}

fn split_bind_spec(spec: &str) -> Result<(&str, &str), String> {
    let mut in_quote = None;
    let mut escaped = false;
    for (idx, ch) in spec.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(quote) = in_quote {
            if ch == quote {
                in_quote = None;
            }
            continue;
        }
        if matches!(ch, '\'' | '"') {
            in_quote = Some(ch);
            continue;
        }
        if ch == ':' {
            let key = spec[..idx].trim();
            if key.is_empty() {
                return Err("missing key sequence in application command binding".to_string());
            }
            return Ok((key, &spec[idx + 1..]));
        }
    }
    if !spec.trim_start().starts_with('"') {
        Err(format!(
            "{}: first non-whitespace character is not `\"'",
            spec.trim()
        ))
    } else {
        Err("missing ':' in application command binding".to_string())
    }
}

fn unquote_value(value: &str) -> Result<String, BindError> {
    if !(value.starts_with('"') && value.ends_with('"') && value.len() >= 2) {
        return Ok(value.to_string());
    }
    let mut out = String::new();
    let mut escaped = false;
    for ch in value[1..value.len() - 1].chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            out.push(ch);
        }
    }
    if escaped {
        return Err(BindError {
            message: "trailing escape in quoted application command".to_string(),
        });
    }
    Ok(out)
}

fn optional_newline(non_empty: bool) -> &'static str {
    if non_empty { "\n" } else { "" }
}

fn format_variable(name: &str, value: &str, reusable: bool) -> String {
    if reusable {
        format!("set {name} {value}")
    } else {
        format!("{name} is set to `{value}'")
    }
}

const READLINE_VARIABLE_NAMES: &[&str] = &[
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
    "visible-stats",
    "bell-style",
    "comment-begin",
    "completion-display-width",
    "completion-prefix-display-length",
    "completion-query-items",
    "editing-mode",
    "emacs-mode-string",
    "history-size",
    "keymap",
    "keyseq-timeout",
    "vi-cmd-mode-string",
    "vi-ins-mode-string",
];

const HIDDEN_READLINE_VARIABLE_NAMES: &[&str] = &[
    "active-region-end-color",
    "active-region-start-color",
    "isearch-terminators",
];

#[cfg(test)]
mod tests;
