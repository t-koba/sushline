mod commands;
mod emacs;
mod parse;
mod vi;
use std::collections::BTreeMap;
use std::fmt;

pub(crate) use commands::{BIND_FUNCTION_NAMES, is_bindable_function_name};
use parse::{parse_named_keyseq, parse_quoted_keyseq};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyMapName {
    Emacs,
    EmacsStandard,
    EmacsMeta,
    EmacsCtlx,
    Vi,
    ViCommand,
    ViInsert,
}

impl KeyMapName {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "emacs" => Some(Self::Emacs),
            "emacs-standard" => Some(Self::EmacsStandard),
            "emacs-meta" => Some(Self::EmacsMeta),
            "emacs-ctlx" => Some(Self::EmacsCtlx),
            "vi" => Some(Self::Vi),
            "vi-move" | "vi-command" => Some(Self::ViCommand),
            "vi-insert" => Some(Self::ViInsert),
            _ => None,
        }
    }

    pub fn canonical(self) -> Self {
        match self {
            Self::Emacs => Self::EmacsStandard,
            Self::Vi => Self::ViCommand,
            other => other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Emacs => "emacs",
            Self::EmacsStandard => "emacs-standard",
            Self::EmacsMeta => "emacs-meta",
            Self::EmacsCtlx => "emacs-ctlx",
            Self::Vi => "vi",
            Self::ViCommand => "vi-command",
            Self::ViInsert => "vi-insert",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeySequence(Vec<u8>);

impl KeySequence {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        Self::parse_with_meta(value, true)
    }

    pub fn parse_with_meta(value: &str, meta_prefix: bool) -> Result<Self, String> {
        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            return parse_quoted_keyseq(&value[1..value.len() - 1], meta_prefix).map(Self);
        }
        parse_named_keyseq(value, meta_prefix).map(Self)
    }

    pub fn display_inputrc(&self) -> String {
        let mut out = String::from("\"");
        for b in &self.0 {
            match *b {
                0x00 => out.push_str("\\C-@"),
                b'\n' => out.push_str("\\n"),
                b'\r' => out.push_str("\\r"),
                b'\t' => out.push_str("\\t"),
                0x1b => out.push_str("\\e"),
                0x1c => out.push_str("\\C-\\\\"),
                0x1d => out.push_str("\\C-]"),
                0x1e => out.push_str("\\C-^"),
                0x1f => out.push_str("\\C-_"),
                0x7f => out.push_str("\\C-?"),
                0x01..=0x1a => {
                    out.push_str("\\C-");
                    out.push((b + b'a' - 1) as char);
                }
                0x80..=0xff => out.push_str(&format!("\\{b:03o}")),
                b'\\' => out.push_str("\\\\"),
                b'"' => out.push_str("\\\""),
                0x20..=0x7e => out.push(*b as char),
            }
        }
        out.push('"');
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EditCommand {
    Abort,
    AcceptLine,
    BackwardChar,
    BackwardDeleteChar,
    BackwardKillLine,
    BackwardKillWord,
    BackwardWord,
    CapitalizeWord,
    BeginningOfLine,
    CallLastKbdMacro,
    ClearScreen,
    CopyRegionAsKill,
    DeleteChar,
    DeleteHorizontalSpace,
    DigitArgument,
    DowncaseWord,
    EndKbdMacro,
    EndOfLine,
    ExchangePointAndMark,
    ForwardChar,
    ForwardWord,
    HistoryBeginning,
    HistoryEnd,
    HistorySearchBackward,
    HistorySearchForward,
    KillLine,
    KillRegion,
    KillWholeLine,
    KillWord,
    UniversalArgument,
    UnixLineDiscard,
    UnixWordRubout,
    ViAppendEol,
    ViAppendMode,
    ViInsertBeg,
    ViInsertionMode,
    ViMovementMode,
    NextHistory,
    PreviousHistory,
    QuotedInsert,
    ReverseSearchHistory,
    RevertLine,
    SelfInsert,
    SetMark,
    PrintLastKbdMacro,
    StartKbdMacro,
    TabComplete,
    TransposeChars,
    TransposeWords,
    Undo,
    UpcaseWord,
    Yank,
    YankPop,
    Eof,
    PrefixMeta,
    Unknown,
}

impl EditCommand {
    pub const ALL: &'static [Self] = &[
        Self::Abort,
        Self::AcceptLine,
        Self::BackwardChar,
        Self::BackwardDeleteChar,
        Self::BackwardKillLine,
        Self::BackwardKillWord,
        Self::BackwardWord,
        Self::CapitalizeWord,
        Self::BeginningOfLine,
        Self::CallLastKbdMacro,
        Self::ClearScreen,
        Self::CopyRegionAsKill,
        Self::DeleteChar,
        Self::DeleteHorizontalSpace,
        Self::DigitArgument,
        Self::DowncaseWord,
        Self::EndKbdMacro,
        Self::EndOfLine,
        Self::ExchangePointAndMark,
        Self::ForwardChar,
        Self::ForwardWord,
        Self::HistoryBeginning,
        Self::HistoryEnd,
        Self::HistorySearchBackward,
        Self::HistorySearchForward,
        Self::KillLine,
        Self::KillRegion,
        Self::KillWholeLine,
        Self::KillWord,
        Self::UniversalArgument,
        Self::UnixLineDiscard,
        Self::UnixWordRubout,
        Self::ViAppendEol,
        Self::ViAppendMode,
        Self::ViInsertBeg,
        Self::ViInsertionMode,
        Self::ViMovementMode,
        Self::NextHistory,
        Self::PreviousHistory,
        Self::QuotedInsert,
        Self::ReverseSearchHistory,
        Self::RevertLine,
        Self::SelfInsert,
        Self::SetMark,
        Self::PrintLastKbdMacro,
        Self::StartKbdMacro,
        Self::TabComplete,
        Self::TransposeChars,
        Self::TransposeWords,
        Self::Undo,
        Self::UpcaseWord,
        Self::Yank,
        Self::YankPop,
        Self::Eof,
        Self::PrefixMeta,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "abort" => Self::Abort,
            "accept-line" => Self::AcceptLine,
            "backward-char" => Self::BackwardChar,
            "backward-delete-char" => Self::BackwardDeleteChar,
            "backward-kill-line" => Self::BackwardKillLine,
            "backward-kill-word" => Self::BackwardKillWord,
            "backward-word" => Self::BackwardWord,
            "capitalize-word" => Self::CapitalizeWord,
            "beginning-of-line" => Self::BeginningOfLine,
            "call-last-kbd-macro" => Self::CallLastKbdMacro,
            "clear-screen" => Self::ClearScreen,
            "copy-region-as-kill" => Self::CopyRegionAsKill,
            "delete-char" => Self::DeleteChar,
            "delete-horizontal-space" => Self::DeleteHorizontalSpace,
            "digit-argument" => Self::DigitArgument,
            "downcase-word" => Self::DowncaseWord,
            "end-kbd-macro" => Self::EndKbdMacro,
            "end-of-file" => Self::Eof,
            "end-of-line" => Self::EndOfLine,
            "exchange-point-and-mark" => Self::ExchangePointAndMark,
            "forward-char" => Self::ForwardChar,
            "forward-word" => Self::ForwardWord,
            "beginning-of-history" => Self::HistoryBeginning,
            "end-of-history" => Self::HistoryEnd,
            "history-search-backward" => Self::HistorySearchBackward,
            "history-search-forward" => Self::HistorySearchForward,
            "kill-line" => Self::KillLine,
            "kill-region" => Self::KillRegion,
            "kill-whole-line" => Self::KillWholeLine,
            "kill-word" => Self::KillWord,
            "universal-argument" => Self::UniversalArgument,
            "unix-line-discard" => Self::UnixLineDiscard,
            "unix-word-rubout" => Self::UnixWordRubout,
            "vi-append-eol" => Self::ViAppendEol,
            "vi-append-mode" => Self::ViAppendMode,
            "vi-insert-beg" => Self::ViInsertBeg,
            "vi-insertion-mode" => Self::ViInsertionMode,
            "vi-movement-mode" => Self::ViMovementMode,
            "next-history" => Self::NextHistory,
            "previous-history" => Self::PreviousHistory,
            "quoted-insert" => Self::QuotedInsert,
            "reverse-search-history" => Self::ReverseSearchHistory,
            "revert-line" => Self::RevertLine,
            "self-insert" => Self::SelfInsert,
            "set-mark" => Self::SetMark,
            "print-last-kbd-macro" => Self::PrintLastKbdMacro,
            "prefix-meta" => Self::PrefixMeta,
            "start-kbd-macro" => Self::StartKbdMacro,
            "complete" => Self::TabComplete,
            "transpose-chars" => Self::TransposeChars,
            "transpose-words" => Self::TransposeWords,
            "undo" => Self::Undo,
            "upcase-word" => Self::UpcaseWord,
            "yank" => Self::Yank,
            "yank-pop" => Self::YankPop,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Abort => "abort",
            Self::AcceptLine => "accept-line",
            Self::BackwardChar => "backward-char",
            Self::BackwardDeleteChar => "backward-delete-char",
            Self::BackwardKillLine => "backward-kill-line",
            Self::BackwardKillWord => "backward-kill-word",
            Self::BackwardWord => "backward-word",
            Self::CapitalizeWord => "capitalize-word",
            Self::BeginningOfLine => "beginning-of-line",
            Self::CallLastKbdMacro => "call-last-kbd-macro",
            Self::ClearScreen => "clear-screen",
            Self::CopyRegionAsKill => "copy-region-as-kill",
            Self::DeleteChar => "delete-char",
            Self::DeleteHorizontalSpace => "delete-horizontal-space",
            Self::DigitArgument => "digit-argument",
            Self::DowncaseWord => "downcase-word",
            Self::EndKbdMacro => "end-kbd-macro",
            Self::EndOfLine => "end-of-line",
            Self::ExchangePointAndMark => "exchange-point-and-mark",
            Self::ForwardChar => "forward-char",
            Self::ForwardWord => "forward-word",
            Self::HistoryBeginning => "beginning-of-history",
            Self::HistoryEnd => "end-of-history",
            Self::HistorySearchBackward => "history-search-backward",
            Self::HistorySearchForward => "history-search-forward",
            Self::KillLine => "kill-line",
            Self::KillRegion => "kill-region",
            Self::KillWholeLine => "kill-whole-line",
            Self::KillWord => "kill-word",
            Self::UniversalArgument => "universal-argument",
            Self::UnixLineDiscard => "unix-line-discard",
            Self::UnixWordRubout => "unix-word-rubout",
            Self::ViAppendEol => "vi-append-eol",
            Self::ViAppendMode => "vi-append-mode",
            Self::ViInsertBeg => "vi-insert-beg",
            Self::ViInsertionMode => "vi-insertion-mode",
            Self::ViMovementMode => "vi-movement-mode",
            Self::NextHistory => "next-history",
            Self::PreviousHistory => "previous-history",
            Self::QuotedInsert => "quoted-insert",
            Self::ReverseSearchHistory => "reverse-search-history",
            Self::RevertLine => "revert-line",
            Self::SelfInsert => "self-insert",
            Self::SetMark => "set-mark",
            Self::PrintLastKbdMacro => "print-last-kbd-macro",
            Self::StartKbdMacro => "start-kbd-macro",
            Self::TabComplete => "complete",
            Self::TransposeChars => "transpose-chars",
            Self::TransposeWords => "transpose-words",
            Self::Undo => "undo",
            Self::UpcaseWord => "upcase-word",
            Self::Yank => "yank",
            Self::YankPop => "yank-pop",
            Self::Eof => "end-of-file",
            Self::PrefixMeta => "prefix-meta",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyBinding {
    Command(EditCommand),
    NamedCommand(String),
    Macro(Vec<u8>),
    ApplicationCommand(String),
}

#[derive(Debug, Clone)]
pub struct KeyMap {
    maps: BTreeMap<KeyMapName, BTreeMap<KeySequence, KeyBinding>>,
    current: KeyMapName,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self {
            maps: BTreeMap::new(),
            current: KeyMapName::EmacsStandard,
        }
    }
}

impl KeyMap {
    pub fn current(&self) -> KeyMapName {
        self.current
    }

    pub fn set_current(&mut self, name: KeyMapName) {
        self.current = name.canonical();
        self.maps.entry(self.current).or_default();
    }

    pub fn bind(&mut self, map: KeyMapName, key: KeySequence, binding: KeyBinding) {
        self.maps
            .entry(map.canonical())
            .or_default()
            .insert(key, binding);
    }

    pub fn lookup(&self, map: KeyMapName, key: &[u8]) -> Option<&KeyBinding> {
        self.maps
            .get(&map.canonical())
            .and_then(|m| m.get(&KeySequence::new(key.to_vec())))
    }

    pub fn has_prefix(&self, map: KeyMapName, prefix: &[u8]) -> bool {
        self.maps.get(&map.canonical()).is_some_and(|m| {
            m.keys()
                .any(|seq| seq.bytes().len() > prefix.len() && seq.bytes().starts_with(prefix))
        })
    }

    pub fn longest_matching_prefix(
        &self,
        map: KeyMapName,
        bytes: &[u8],
    ) -> Option<(usize, &KeyBinding)> {
        self.maps.get(&map.canonical()).and_then(|m| {
            m.iter()
                .filter(|(seq, _)| bytes.starts_with(seq.bytes()))
                .max_by_key(|(seq, _)| seq.bytes().len())
                .map(|(seq, binding)| (seq.bytes().len(), binding))
        })
    }

    pub fn bindings_for_command(&self, command: EditCommand) -> Vec<(KeyMapName, KeySequence)> {
        self.bindings_for_command_name(command.as_str())
    }

    pub fn bindings_for_command_name(&self, command: &str) -> Vec<(KeyMapName, KeySequence)> {
        let mut out = Vec::new();
        for (map, bindings) in &self.maps {
            for (seq, binding) in bindings {
                match binding {
                    KeyBinding::Command(bound) if bound.as_str() == command => {
                        out.push((*map, seq.clone()));
                    }
                    KeyBinding::NamedCommand(bound) if bound == command => {
                        out.push((*map, seq.clone()));
                    }
                    _ => {}
                }
            }
        }
        out
    }

    pub fn unbind_key(&mut self, map: KeyMapName, key: &KeySequence) -> Option<KeyBinding> {
        self.maps
            .get_mut(&map.canonical())
            .and_then(|bindings| bindings.remove(key))
    }

    pub fn unbind_command(&mut self, command: &str) -> usize {
        let mut removed = 0;
        for bindings in self.maps.values_mut() {
            let before = bindings.len();
            bindings.retain(|_, binding| match binding {
                KeyBinding::Command(bound) => bound.as_str() != command,
                KeyBinding::NamedCommand(bound) => bound != command,
                _ => true,
            });
            removed += before - bindings.len();
        }
        removed
    }

    pub fn iter(&self) -> impl Iterator<Item = (KeyMapName, &KeySequence, &KeyBinding)> {
        self.maps.iter().flat_map(|(map, bindings)| {
            bindings
                .iter()
                .map(move |(seq, binding)| (*map, seq, binding))
        })
    }
}

impl fmt::Display for EditCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests;
