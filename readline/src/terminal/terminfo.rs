#![cfg_attr(not(unix), allow(dead_code))]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(unix)]
pub(super) fn terminfo_keypad_sequence(enabled: bool) -> Option<Vec<u8>> {
    let cap = if enabled { "smkx" } else { "rmkx" };
    terminfo_sequence(cap)
}

#[cfg(unix)]
pub(super) fn terminfo_sequence(cap: &str) -> Option<Vec<u8>> {
    terminfo_capabilities()
        .and_then(|caps| caps.get(cap).cloned())
        .or_else(|| hardcoded_terminfo_fallback(cap))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DefaultCapabilities {
    pub(crate) bracketed_paste: bool,
    pub(crate) active_region_colors: Option<(Vec<u8>, Vec<u8>)>,
}

pub(crate) fn default_capabilities() -> DefaultCapabilities {
    #[cfg(unix)]
    {
        default_capabilities_from_terminfo(terminfo_capabilities())
    }
    #[cfg(not(unix))]
    {
        DefaultCapabilities::default()
    }
}

fn default_capabilities_from_terminfo(
    capabilities: Option<&HashMap<&'static str, Vec<u8>>>,
) -> DefaultCapabilities {
    let Some(capabilities) = capabilities else {
        return DefaultCapabilities::default();
    };
    let capability = |name: &str| {
        capabilities
            .get(name)
            .map(Vec::as_slice)
            .filter(|sequence| !sequence.is_empty())
    };
    // Both defaults require clear and a complete standout pair. Clear alone
    // is insufficient: Bash also leaves bracketed paste off for vt52/adm3a.
    // This is Sushline's capability policy, not GNU's source-level mechanism.
    let active_region_colors = capability("clear").and_then(|_| {
        capability("smso")
            .zip(capability("rmso").or_else(|| capability("sgr0")))
            .map(|(start, end)| (start.to_vec(), end.to_vec()))
    });
    DefaultCapabilities {
        bracketed_paste: active_region_colors.is_some(),
        active_region_colors,
    }
}

fn hardcoded_terminfo_fallback(cap: &str) -> Option<Vec<u8>> {
    match cap {
        "clear" => Some(b"\x1b[H\x1b[2J".to_vec()),
        "flash" => Some(b"\x1b[?5h\x1b[?5l".to_vec()),
        _ => None,
    }
}

static TERMINFO_CAPABILITIES: OnceLock<Option<HashMap<&'static str, Vec<u8>>>> = OnceLock::new();

fn terminfo_capabilities() -> Option<&'static HashMap<&'static str, Vec<u8>>> {
    TERMINFO_CAPABILITIES
        .get_or_init(load_terminfo_capabilities)
        .as_ref()
}

fn load_terminfo_capabilities() -> Option<HashMap<&'static str, Vec<u8>>> {
    let term = std::env::var("TERM").ok()?;
    let bytes = std::fs::read(find_terminfo_path(&term)?).ok()?;
    parse_terminfo(&bytes)
}

fn find_terminfo_path(term: &str) -> Option<PathBuf> {
    let first = term.as_bytes().first().copied()? as char;
    let mut roots = Vec::new();
    if let Ok(root) = std::env::var("TERMINFO") {
        roots.push(PathBuf::from(root));
    }
    if let Ok(dirs) = std::env::var("TERMINFO_DIRS") {
        for dir in dirs.split(':') {
            roots.push(if dir.is_empty() {
                PathBuf::from("/usr/share/terminfo")
            } else {
                PathBuf::from(dir)
            });
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join(".terminfo"));
    }
    roots.extend([
        PathBuf::from("/usr/share/terminfo"),
        PathBuf::from("/usr/lib/terminfo"),
        PathBuf::from("/lib/terminfo"),
        PathBuf::from("/etc/terminfo"),
    ]);
    for root in roots {
        let hex = format!("{:x}", first as u32);
        for dir in [first.to_string(), hex] {
            let path = root.join(dir).join(term);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn parse_terminfo(bytes: &[u8]) -> Option<HashMap<&'static str, Vec<u8>>> {
    let header = parse_terminfo_header(bytes)?;
    let mut offset = 12usize
        .checked_add(header.names_size)?
        .checked_add(header.bool_count)?;
    if !offset.is_multiple_of(2) {
        offset += 1;
    }
    offset = offset.checked_add(header.num_count.checked_mul(header.num_size)?)?;
    let string_offsets_start = offset;
    let string_table_start = string_offsets_start.checked_add(header.str_count.checked_mul(2)?)?;
    let string_table_end = string_table_start.checked_add(header.str_table_size)?;
    if string_table_end > bytes.len() {
        return None;
    }
    let mut caps = HashMap::new();
    for (name, idx) in STANDARD_STRING_CAPS {
        if *idx >= header.str_count {
            continue;
        }
        let pos = string_offsets_start + idx * 2;
        let string_offset = i16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
        if string_offset < 0 {
            continue;
        }
        let start = string_table_start + string_offset as usize;
        if start >= string_table_end {
            continue;
        }
        let Some(end) = bytes[start..string_table_end]
            .iter()
            .position(|byte| *byte == 0)
            .map(|end| start + end)
        else {
            continue;
        };
        caps.insert(*name, bytes[start..end].to_vec());
    }
    parse_extended_terminfo(bytes, string_table_end, header.num_size, &mut caps);
    Some(caps)
}

fn parse_extended_terminfo(
    bytes: &[u8],
    mut offset: usize,
    num_size: usize,
    caps: &mut HashMap<&'static str, Vec<u8>>,
) -> Option<()> {
    if !offset.is_multiple_of(2) {
        offset += 1;
    }
    if offset + 10 > bytes.len() {
        return None;
    }
    let read = |idx: usize| u16::from_le_bytes([bytes[idx], bytes[idx + 1]]) as usize;
    let bool_count = read(offset);
    let num_count = read(offset + 2);
    let str_count = read(offset + 4);
    let str_table_size = read(offset + 6);
    offset += 10;
    offset = offset.checked_add(bool_count)?;
    if !offset.is_multiple_of(2) {
        offset += 1;
    }
    offset = offset.checked_add(num_count.checked_mul(num_size)?)?;
    let string_offsets_start = offset;
    offset = offset.checked_add(str_count.checked_mul(2)?)?;
    let name_offsets_start = offset;
    let name_count = bool_count.checked_add(num_count)?.checked_add(str_count)?;
    offset = offset.checked_add(name_count.checked_mul(2)?)?;
    let string_table_start = offset;
    let string_table_end = string_table_start.checked_add(str_table_size)?;
    if string_table_end > bytes.len() {
        return None;
    }
    for str_idx in 0..str_count {
        let name_offset_pos = name_offsets_start + (bool_count + num_count + str_idx) * 2;
        let value_offset_pos = string_offsets_start + str_idx * 2;
        let name =
            terminfo_table_bytes(bytes, string_table_start, string_table_end, name_offset_pos)?;
        if name != b"E3" {
            continue;
        }
        let value = terminfo_table_bytes(
            bytes,
            string_table_start,
            string_table_end,
            value_offset_pos,
        )?;
        caps.insert("E3", value);
    }
    Some(())
}

fn terminfo_table_bytes(
    bytes: &[u8],
    table_start: usize,
    table_end: usize,
    offset_pos: usize,
) -> Option<Vec<u8>> {
    if offset_pos + 2 > bytes.len() {
        return None;
    }
    let string_offset = i16::from_le_bytes([bytes[offset_pos], bytes[offset_pos + 1]]);
    if string_offset < 0 {
        return None;
    }
    let start = table_start + string_offset as usize;
    if start >= table_end {
        return None;
    }
    let end = bytes[start..table_end]
        .iter()
        .position(|byte| *byte == 0)
        .map(|end| start + end)?;
    Some(bytes[start..end].to_vec())
}

struct TerminfoHeader {
    names_size: usize,
    bool_count: usize,
    num_count: usize,
    str_count: usize,
    str_table_size: usize,
    num_size: usize,
}

fn parse_terminfo_header(bytes: &[u8]) -> Option<TerminfoHeader> {
    if bytes.len() < 12 {
        return None;
    }
    let read = |idx: usize| u16::from_le_bytes([bytes[idx], bytes[idx + 1]]) as usize;
    let magic = read(0);
    let num_size = match magic {
        0x011a => 2,
        0x021e => 4,
        _ => return None,
    };
    Some(TerminfoHeader {
        names_size: read(2),
        bool_count: read(4),
        num_count: read(6),
        str_count: read(8),
        str_table_size: read(10),
        num_size,
    })
}

const STANDARD_STRING_CAPS: &[(&str, usize)] = &[
    ("clear", 5),
    ("smso", 35),
    ("sgr0", 39),
    ("rmso", 43),
    ("flash", 45),
    ("rmkx", 88),
    ("smkx", 89),
    ("E3", 300),
];

#[cfg(test)]
mod tests {
    use super::{DefaultCapabilities, default_capabilities_from_terminfo};
    use std::collections::HashMap;

    fn caps(entries: &[(&'static str, &[u8])]) -> HashMap<&'static str, Vec<u8>> {
        entries
            .iter()
            .map(|(name, value)| (*name, value.to_vec()))
            .collect()
    }

    #[test]
    fn keypad_capabilities_have_no_hardcoded_fallback() {
        assert_eq!(super::hardcoded_terminfo_fallback("smkx"), None);
        assert_eq!(super::hardcoded_terminfo_fallback("rmkx"), None);
    }

    #[test]
    fn terminal_defaults_fail_closed_without_usable_capabilities() {
        assert_eq!(
            default_capabilities_from_terminfo(None),
            DefaultCapabilities::default()
        );
        let empty = caps(&[]);
        let standout_only = caps(&[("smso", b"standout"), ("rmso", b"reset")]);
        let empty_clear = caps(&[("clear", b""), ("smso", b"standout"), ("rmso", b"reset")]);
        for capabilities in [&empty, &standout_only, &empty_clear] {
            let defaults = default_capabilities_from_terminfo(Some(capabilities));
            assert!(!defaults.bracketed_paste);
            assert!(defaults.active_region_colors.is_none());
        }
    }

    #[test]
    fn clear_without_standout_disables_both_terminal_defaults() {
        let capabilities = caps(&[("clear", b"clear")]);
        assert_eq!(
            default_capabilities_from_terminfo(Some(&capabilities)),
            DefaultCapabilities::default()
        );
    }

    #[test]
    fn incomplete_standout_capabilities_disable_both_terminal_defaults() {
        for capabilities in [
            caps(&[("clear", b"clear"), ("smso", b"standout")]),
            caps(&[("clear", b"clear"), ("rmso", b"reset")]),
            caps(&[("clear", b"clear"), ("smso", b""), ("rmso", b"reset")]),
            caps(&[
                ("clear", b"clear"),
                ("smso", b"standout"),
                ("rmso", b""),
                ("sgr0", b""),
            ]),
        ] {
            assert_eq!(
                default_capabilities_from_terminfo(Some(&capabilities)),
                DefaultCapabilities::default()
            );
        }
    }

    #[test]
    fn terminal_defaults_use_clear_and_standout_capabilities() {
        let capabilities = caps(&[
            ("clear", b"clear"),
            ("smso", b"standout"),
            ("rmso", b"standout-reset"),
        ]);
        assert_eq!(
            default_capabilities_from_terminfo(Some(&capabilities)),
            DefaultCapabilities {
                bracketed_paste: true,
                active_region_colors: Some((b"standout".to_vec(), b"standout-reset".to_vec())),
            }
        );
    }

    #[test]
    fn full_reset_capability_can_end_active_region_when_rmso_is_missing() {
        let capabilities = caps(&[
            ("clear", b"clear"),
            ("smso", b"standout"),
            ("sgr0", b"reset-all"),
        ]);
        assert_eq!(
            default_capabilities_from_terminfo(Some(&capabilities)),
            DefaultCapabilities {
                bracketed_paste: true,
                active_region_colors: Some((b"standout".to_vec(), b"reset-all".to_vec())),
            }
        );
    }
}

#[cfg(not(unix))]
pub(super) fn terminfo_keypad_sequence(_enabled: bool) -> Option<Vec<u8>> {
    None
}

#[cfg(not(unix))]
pub(super) fn terminfo_sequence(_cap: &str) -> Option<Vec<u8>> {
    None
}
