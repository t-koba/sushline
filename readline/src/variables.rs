use crate::config::Config;
use crate::terminal::active_region_default_sequences;
use std::collections::BTreeMap;
use std::ops::Index;

pub struct Variables {
    strings: BTreeMap<String, String>,
    bytes: BTreeMap<String, Vec<u8>>,
}

impl Variables {
    pub fn new() -> Self {
        Self {
            strings: BTreeMap::new(),
            bytes: BTreeMap::new(),
        }
    }

    pub fn default_for_config(config: &Config) -> Self {
        let strings = default_variable_strings(config);
        let mut bytes = strings
            .iter()
            .map(|(key, value)| (key.clone(), value.as_bytes().to_vec()))
            .collect::<BTreeMap<_, _>>();
        let (region_start, region_end) = crate::terminal::active_region_default_sequence_bytes();
        bytes.insert("active-region-start-color".to_string(), region_start);
        bytes.insert("active-region-end-color".to_string(), region_end);
        Self { strings, bytes }
    }

    pub fn get(&self, name: &str) -> Option<&String> {
        self.strings.get(name)
    }

    pub fn get_bytes(&self, name: &str) -> Option<&Vec<u8>> {
        self.bytes.get(name)
    }

    pub fn insert(&mut self, name: String, value: String) -> Option<String> {
        self.bytes.insert(name.clone(), value.as_bytes().to_vec());
        self.strings.insert(name, value)
    }

    pub fn insert_bytes(&mut self, name: String, value: Vec<u8>) {
        self.bytes.insert(name, value);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.strings.contains_key(name)
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.contains(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.strings
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    pub fn strings(&self) -> &BTreeMap<String, String> {
        &self.strings
    }

    pub fn bytes(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.bytes
    }

    pub fn is_on(&self, name: &str) -> bool {
        self.get(name)
            .is_some_and(|value| matches!(value.as_str(), "on" | "1"))
    }
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
