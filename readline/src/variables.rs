use crate::config::Config;
use crate::terminal::active_region_default_sequences;
use std::collections::BTreeMap;
use std::ops::Index;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoolVariable {
    /// BindTtySpecialChars.
    BindTtySpecialChars,
    /// EnableActiveRegion.
    EnableActiveRegion,
    /// EchoControlCharacters.
    EchoControlCharacters,
    /// OutputMeta.
    OutputMeta,
    /// ByteOriented.
    ByteOriented,
    /// HorizontalScrollMode.
    HorizontalScrollMode,
    /// ShowModeInPrompt.
    ShowModeInPrompt,
    /// MarkModifiedLines.
    MarkModifiedLines,
    /// EnableBracketedPaste.
    EnableBracketedPaste,
    /// EnableMetaKey.
    EnableMetaKey,
    /// EnableKeypad.
    EnableKeypad,
    /// RevertAllAtNewline.
    RevertAllAtNewline,
    /// HistoryPreservePoint.
    HistoryPreservePoint,
    /// ShowAllIfAmbiguous.
    ShowAllIfAmbiguous,
    /// ShowAllIfUnmodified.
    ShowAllIfUnmodified,
    /// MenuCompleteDisplayPrefix.
    MenuCompleteDisplayPrefix,
    /// PageCompletions.
    PageCompletions,
    /// ConvertMeta.
    ConvertMeta,
    /// InputMeta.
    InputMeta,
    /// MetaFlag.
    MetaFlag,
    /// SearchIgnoreCase.
    SearchIgnoreCase,
}

impl BoolVariable {
    pub(crate) const COUNT: usize = 21;

    #[allow(dead_code)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::BindTtySpecialChars => "bind-tty-special-chars",
            Self::EnableActiveRegion => "enable-active-region",
            Self::EchoControlCharacters => "echo-control-characters",
            Self::OutputMeta => "output-meta",
            Self::ByteOriented => "byte-oriented",
            Self::HorizontalScrollMode => "horizontal-scroll-mode",
            Self::ShowModeInPrompt => "show-mode-in-prompt",
            Self::MarkModifiedLines => "mark-modified-lines",
            Self::EnableBracketedPaste => "enable-bracketed-paste",
            Self::EnableMetaKey => "enable-meta-key",
            Self::EnableKeypad => "enable-keypad",
            Self::RevertAllAtNewline => "revert-all-at-newline",
            Self::HistoryPreservePoint => "history-preserve-point",
            Self::ShowAllIfAmbiguous => "show-all-if-ambiguous",
            Self::ShowAllIfUnmodified => "show-all-if-unmodified",
            Self::MenuCompleteDisplayPrefix => "menu-complete-display-prefix",
            Self::PageCompletions => "page-completions",
            Self::ConvertMeta => "convert-meta",
            Self::InputMeta => "input-meta",
            Self::MetaFlag => "meta-flag",
            Self::SearchIgnoreCase => "search-ignore-case",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "bind-tty-special-chars" => Self::BindTtySpecialChars,
            "enable-active-region" => Self::EnableActiveRegion,
            "echo-control-characters" => Self::EchoControlCharacters,
            "output-meta" => Self::OutputMeta,
            "byte-oriented" => Self::ByteOriented,
            "horizontal-scroll-mode" => Self::HorizontalScrollMode,
            "show-mode-in-prompt" => Self::ShowModeInPrompt,
            "mark-modified-lines" => Self::MarkModifiedLines,
            "enable-bracketed-paste" => Self::EnableBracketedPaste,
            "enable-meta-key" => Self::EnableMetaKey,
            "enable-keypad" => Self::EnableKeypad,
            "revert-all-at-newline" => Self::RevertAllAtNewline,
            "history-preserve-point" => Self::HistoryPreservePoint,
            "show-all-if-ambiguous" => Self::ShowAllIfAmbiguous,
            "show-all-if-unmodified" => Self::ShowAllIfUnmodified,
            "menu-complete-display-prefix" => Self::MenuCompleteDisplayPrefix,
            "page-completions" => Self::PageCompletions,
            "convert-meta" => Self::ConvertMeta,
            "input-meta" => Self::InputMeta,
            "meta-flag" => Self::MetaFlag,
            "search-ignore-case" => Self::SearchIgnoreCase,
            _ => return None,
        })
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Variables.
pub struct Variables {
    strings: BTreeMap<String, String>,
    bytes: BTreeMap<String, Vec<u8>>,
    flags: [bool; BoolVariable::COUNT],
}

impl Variables {
    /// New.
    pub fn new() -> Self {
        Self::from_maps(BTreeMap::new(), BTreeMap::new())
    }

    /// Default for config.
    pub fn default_for_config(config: &Config) -> Self {
        let strings = default_variable_strings(config);
        let mut bytes = strings
            .iter()
            .map(|(key, value)| (key.clone(), value.as_bytes().to_vec()))
            .collect::<BTreeMap<_, _>>();
        let (region_start, region_end) = crate::terminal::active_region_default_sequence_bytes();
        bytes.insert("active-region-start-color".to_string(), region_start);
        bytes.insert("active-region-end-color".to_string(), region_end);
        Self::from_maps(strings, bytes)
    }

    fn from_maps(strings: BTreeMap<String, String>, bytes: BTreeMap<String, Vec<u8>>) -> Self {
        let mut this = Self {
            strings,
            bytes,
            flags: [false; BoolVariable::COUNT],
        };
        this.rebuild_flags();
        this
    }

    fn rebuild_flags(&mut self) {
        self.flags = [false; BoolVariable::COUNT];
        for (name, value) in &self.strings {
            if let Some(variable) = BoolVariable::from_name(name) {
                self.flags[variable.index()] = bool_value_is_on(value);
            }
        }
    }

    /// Get.
    pub fn get(&self, name: &str) -> Option<&String> {
        self.strings.get(name)
    }

    /// Get bytes.
    pub fn get_bytes(&self, name: &str) -> Option<&Vec<u8>> {
        self.bytes.get(name)
    }

    /// Insert.
    pub fn insert(&mut self, name: String, value: String) -> Option<String> {
        self.bytes.insert(name.clone(), value.as_bytes().to_vec());
        if let Some(variable) = BoolVariable::from_name(&name) {
            self.flags[variable.index()] = bool_value_is_on(&value);
        }
        self.strings.insert(name, value)
    }

    /// Insert bytes.
    pub fn insert_bytes(&mut self, name: String, value: Vec<u8>) {
        self.bytes.insert(name, value);
    }

    /// Contains.
    pub fn contains(&self, name: &str) -> bool {
        self.strings.contains_key(name)
    }

    /// Contains key.
    pub fn contains_key(&self, name: &str) -> bool {
        self.contains(name)
    }

    /// Iter.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.strings
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    /// Strings.
    pub fn strings(&self) -> &BTreeMap<String, String> {
        &self.strings
    }

    /// Bytes.
    pub fn bytes(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.bytes
    }

    /// Is on.
    pub fn is_on(&self, name: &str) -> bool {
        if let Some(variable) = BoolVariable::from_name(name) {
            return self.flag(variable);
        }
        self.get(name).is_some_and(|value| bool_value_is_on(value))
    }

    pub(crate) fn flag(&self, variable: BoolVariable) -> bool {
        self.flags[variable.index()]
    }
}

fn bool_value_is_on(value: &str) -> bool {
    matches!(value, "on" | "1")
}

impl Index<&str> for Variables {
    type Output = String;

    fn index(&self, index: &str) -> &Self::Output {
        &self.strings[index]
    }
}

impl Default for Variables {
    fn default() -> Self {
        Self::new()
    }
}

fn default_variable_strings(config: &Config) -> BTreeMap<String, String> {
    let mut variables = BTreeMap::new();
    let locale_meta = locale_uses_meta();
    let (active_start, active_end) = active_region_default_sequences();
    variables.insert("bind-tty-special-chars".to_string(), "on".to_string());
    variables.insert("active-region-start-color".to_string(), active_start);
    variables.insert("active-region-end-color".to_string(), active_end);
    variables.insert("blink-matching-paren".to_string(), "off".to_string());
    variables.insert("byte-oriented".to_string(), "off".to_string());
    variables.insert("colored-completion-prefix".to_string(), "off".to_string());
    variables.insert("colored-stats".to_string(), "off".to_string());
    variables.insert("completion-ignore-case".to_string(), "off".to_string());
    variables.insert("completion-map-case".to_string(), "off".to_string());
    variables.insert(
        "completion-prefix-display-length".to_string(),
        "0".to_string(),
    );
    variables.insert("disable-completion".to_string(), "off".to_string());
    variables.insert("echo-control-characters".to_string(), "on".to_string());
    variables.insert("enable-active-region".to_string(), "on".to_string());
    variables.insert("enable-bracketed-paste".to_string(), "on".to_string());
    variables.insert("enable-keypad".to_string(), "off".to_string());
    variables.insert("enable-meta-key".to_string(), "on".to_string());
    variables.insert("expand-tilde".to_string(), "off".to_string());
    variables.insert("force-meta-prefix".to_string(), "off".to_string());
    variables.insert("history-preserve-point".to_string(), "off".to_string());
    variables.insert("horizontal-scroll-mode".to_string(), "off".to_string());
    variables.insert(
        "input-meta".to_string(),
        if locale_meta { "on" } else { "off" }.to_string(),
    );
    variables.insert("mark-directories".to_string(), "on".to_string());
    variables.insert("mark-modified-lines".to_string(), "off".to_string());
    variables.insert("mark-symlinked-directories".to_string(), "off".to_string());
    variables.insert("match-hidden-files".to_string(), "on".to_string());
    variables.insert(
        "menu-complete-display-prefix".to_string(),
        "off".to_string(),
    );
    variables.insert(
        "meta-flag".to_string(),
        if locale_meta { "on" } else { "off" }.to_string(),
    );
    variables.insert(
        "output-meta".to_string(),
        if locale_meta { "on" } else { "off" }.to_string(),
    );
    variables.insert(
        "print-completions-horizontally".to_string(),
        "off".to_string(),
    );
    variables.insert("page-completions".to_string(), "on".to_string());
    variables.insert("prefer-visible-bell".to_string(), "on".to_string());
    variables.insert("revert-all-at-newline".to_string(), "off".to_string());
    variables.insert("search-ignore-case".to_string(), "off".to_string());
    variables.insert("show-all-if-ambiguous".to_string(), "off".to_string());
    variables.insert("show-all-if-unmodified".to_string(), "off".to_string());
    variables.insert("show-mode-in-prompt".to_string(), "off".to_string());
    variables.insert("skip-completed-text".to_string(), "off".to_string());
    variables.insert("visible-stats".to_string(), "off".to_string());
    variables.insert("bell-style".to_string(), "audible".to_string());
    variables.insert("comment-begin".to_string(), "#".to_string());
    variables.insert("completion-display-width".to_string(), "-1".to_string());
    variables.insert("completion-query-items".to_string(), "100".to_string());
    variables.insert(
        "convert-meta".to_string(),
        if locale_meta { "off" } else { "on" }.to_string(),
    );
    variables.insert(
        "editing-mode".to_string(),
        match config.editing_mode {
            crate::config::EditingMode::Emacs => "emacs",
            crate::config::EditingMode::Vi => "vi",
        }
        .to_string(),
    );
    variables.insert("emacs-mode-string".to_string(), "@".to_string());
    variables.insert("history-size".to_string(), "-1".to_string());
    variables.insert("isearch-terminators".to_string(), "\x1b\n".to_string());
    variables.insert(
        "keymap".to_string(),
        match config.editing_mode {
            crate::config::EditingMode::Emacs => "emacs",
            crate::config::EditingMode::Vi => "vi",
        }
        .to_string(),
    );
    variables.insert(
        "keyseq-timeout".to_string(),
        config.keyseq_timeout_ms.to_string(),
    );
    variables.insert("vi-cmd-mode-string".to_string(), "(cmd)".to_string());
    variables.insert("vi-ins-mode-string".to_string(), "(ins)".to_string());
    variables
}

fn locale_uses_meta() -> bool {
    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(value) = std::env::var(key)
            && locale_value_uses_meta(&value)
        {
            return true;
        }
    }
    false
}

fn locale_value_uses_meta(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("utf-8") || lower.contains("utf8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    const BOOL_VARIABLES: &[BoolVariable] = &[
        BoolVariable::BindTtySpecialChars,
        BoolVariable::EnableActiveRegion,
        BoolVariable::EchoControlCharacters,
        BoolVariable::OutputMeta,
        BoolVariable::ByteOriented,
        BoolVariable::HorizontalScrollMode,
        BoolVariable::ShowModeInPrompt,
        BoolVariable::MarkModifiedLines,
        BoolVariable::EnableBracketedPaste,
        BoolVariable::EnableMetaKey,
        BoolVariable::EnableKeypad,
        BoolVariable::RevertAllAtNewline,
        BoolVariable::HistoryPreservePoint,
        BoolVariable::ShowAllIfAmbiguous,
        BoolVariable::ShowAllIfUnmodified,
        BoolVariable::MenuCompleteDisplayPrefix,
        BoolVariable::PageCompletions,
        BoolVariable::ConvertMeta,
        BoolVariable::InputMeta,
        BoolVariable::MetaFlag,
        BoolVariable::SearchIgnoreCase,
    ];

    #[test]
    fn bool_flags_match_string_lookup_and_update_on_insert() {
        let mut variables = Variables::default_for_config(&Config::default());
        for variable in BOOL_VARIABLES {
            assert_eq!(
                variables.flag(*variable),
                variables.is_on(variable.name()),
                "{}",
                variable.name()
            );
        }

        for variable in BOOL_VARIABLES {
            variables.insert(variable.name().to_string(), "on".to_string());
            assert!(variables.flag(*variable), "{}", variable.name());
            assert_eq!(variables.flag(*variable), variables.is_on(variable.name()));

            variables.insert(variable.name().to_string(), "off".to_string());
            assert!(!variables.flag(*variable), "{}", variable.name());
            assert_eq!(variables.flag(*variable), variables.is_on(variable.name()));

            variables.insert(variable.name().to_string(), "1".to_string());
            assert!(variables.flag(*variable), "{}", variable.name());
            assert_eq!(variables.flag(*variable), variables.is_on(variable.name()));
        }
    }
}
