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
    let terminfo = std::env::var("TERMINFO").ok();
    let terminfo_dirs = std::env::var("TERMINFO_DIRS").ok();
    let home = std::env::var("HOME").ok();
    find_terminfo_path_with_env(
        term,
        terminfo.as_deref(),
        terminfo_dirs.as_deref(),
        home.as_deref(),
    )
}

fn find_terminfo_path_with_env(
    term: &str,
    terminfo: Option<&str>,
    terminfo_dirs: Option<&str>,
    home: Option<&str>,
) -> Option<PathBuf> {
    let first = term.as_bytes().first().copied()? as char;
    let mut roots = Vec::new();
    if let Some(root) = terminfo {
        roots.push(PathBuf::from(root));
    }
    // Preserve ncurses-style precedence: explicit root, user database, then
    // configured search directories.
    if let Some(home) = home {
        roots.push(PathBuf::from(home).join(".terminfo"));
    }
    if let Some(dirs) = terminfo_dirs {
        for dir in dirs.split(':') {
            roots.push(if dir.is_empty() {
                PathBuf::from("/usr/share/terminfo")
            } else {
                PathBuf::from(dir)
            });
        }
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
    // Field four counts names plus present string values, not table bytes.
    let str_table_size = read(offset + 8);
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
    // ncurses stores values first, then names. Name offsets are relative to
    // that names subsection, not to the beginning of the whole string table.
    // Absent/cancelled strings have no value bytes (including no NUL).
    let mut names_start = string_table_start;
    for str_idx in 0..str_count {
        let value_offset_pos = string_offsets_start + str_idx * 2;
        let value_offset =
            i16::from_le_bytes([bytes[value_offset_pos], bytes[value_offset_pos + 1]]);
        if value_offset < 0 {
            continue;
        }
        let value = terminfo_table_bytes(
            bytes,
            string_table_start,
            string_table_end,
            value_offset_pos,
        )?;
        names_start = names_start.checked_add(value.len().checked_add(1)?)?;
    }
    if names_start > string_table_end {
        return None;
    }
    for str_idx in 0..str_count {
        let name_offset_pos = name_offsets_start + (bool_count + num_count + str_idx) * 2;
        let value_offset_pos = string_offsets_start + str_idx * 2;
        let name = terminfo_table_bytes(bytes, names_start, string_table_end, name_offset_pos)?;
        if name != b"E3" {
            continue;
        }
        if let Some(value) =
            terminfo_table_bytes(bytes, string_table_start, names_start, value_offset_pos)
        {
            caps.insert("E3", value.to_vec());
        }
    }
    Some(())
}

fn terminfo_table_bytes(
    bytes: &[u8],
    table_start: usize,
    table_end: usize,
    offset_pos: usize,
) -> Option<&[u8]> {
    if offset_pos.checked_add(2)? > bytes.len() {
        return None;
    }
    let string_offset = i16::from_le_bytes([bytes[offset_pos], bytes[offset_pos + 1]]);
    if string_offset < 0 {
        return None;
    }
    let start = table_start.checked_add(string_offset as usize)?;
    if start >= table_end {
        return None;
    }
    let table = bytes.get(start..table_end)?;
    let end = table
        .iter()
        .position(|byte| *byte == 0)
        .map(|end| start + end)?;
    Some(&bytes[start..end])
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
];

#[cfg(test)]
mod tests {
    use super::{
        DefaultCapabilities, default_capabilities_from_terminfo, find_terminfo_path_with_env,
        parse_terminfo,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn caps(entries: &[(&'static str, &[u8])]) -> HashMap<&'static str, Vec<u8>> {
        entries
            .iter()
            .map(|(name, value)| (*name, value.to_vec()))
            .collect()
    }

    fn terminfo_entry(smkx: &[u8], rmkx: &[u8]) -> Vec<u8> {
        let names = b"auditprec\0";
        let mut string_offsets = vec![-1i16; 90];
        let mut string_table = Vec::new();
        string_offsets[88] = 0;
        string_table.extend_from_slice(rmkx);
        string_table.push(0);
        string_offsets[89] = string_table.len() as i16;
        string_table.extend_from_slice(smkx);
        string_table.push(0);

        let mut bytes = Vec::new();
        for field in [
            0x011au16,
            names.len() as u16,
            0,
            0,
            90,
            string_table.len() as u16,
        ] {
            bytes.extend(field.to_le_bytes());
        }
        bytes.extend_from_slice(names);
        for offset in string_offsets {
            bytes.extend(offset.to_le_bytes());
        }
        bytes.extend_from_slice(&string_table);
        bytes
    }

    fn write_entry(root: &Path, term: &str, smkx: &[u8], rmkx: &[u8]) -> PathBuf {
        let entry = root
            .join((term.as_bytes()[0] as char).to_string())
            .join(term);
        fs::create_dir_all(entry.parent().unwrap()).unwrap();
        fs::write(&entry, terminfo_entry(smkx, rmkx)).unwrap();
        entry
    }

    fn selected_capabilities(
        term: &str,
        terminfo: Option<&str>,
        terminfo_dirs: Option<&str>,
        home: Option<&str>,
    ) -> HashMap<&'static str, Vec<u8>> {
        let path = find_terminfo_path_with_env(term, terminfo, terminfo_dirs, home).unwrap();
        parse_terminfo(&fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn user_terminfo_precedes_terminfo_dirs_but_explicit_terminfo_wins() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dirs = temp.path().join("dirs");
        let explicit = temp.path().join("explicit");
        let missing_home = temp.path().join("missing-home");
        let term = "auditprec";
        write_entry(&home.join(".terminfo"), term, b"HOMEKEY", b"HOMEEND");
        write_entry(&dirs, term, b"DIRSKEY", b"DIRSEND");
        write_entry(&explicit, term, b"EXPLICITKEY", b"EXPLICITEND");

        let home_caps = selected_capabilities(
            term,
            None,
            Some(dirs.to_str().unwrap()),
            Some(home.to_str().unwrap()),
        );
        assert_eq!(
            home_caps.get("smkx").map(Vec::as_slice),
            Some(&b"HOMEKEY"[..])
        );
        assert_eq!(
            home_caps.get("rmkx").map(Vec::as_slice),
            Some(&b"HOMEEND"[..])
        );

        let explicit_caps = selected_capabilities(
            term,
            Some(explicit.to_str().unwrap()),
            Some(dirs.to_str().unwrap()),
            Some(home.to_str().unwrap()),
        );
        assert_eq!(
            explicit_caps.get("smkx").map(Vec::as_slice),
            Some(&b"EXPLICITKEY"[..])
        );
        assert_eq!(
            explicit_caps.get("rmkx").map(Vec::as_slice),
            Some(&b"EXPLICITEND"[..])
        );

        let dirs_caps = selected_capabilities(
            term,
            None,
            Some(dirs.to_str().unwrap()),
            Some(missing_home.to_str().unwrap()),
        );
        assert_eq!(
            dirs_caps.get("smkx").map(Vec::as_slice),
            Some(&b"DIRSKEY"[..])
        );
        assert_eq!(
            dirs_caps.get("rmkx").map(Vec::as_slice),
            Some(&b"DIRSEND"[..])
        );
    }

    #[test]
    fn extended_e3_is_not_standard_initialize_pair() {
        // Compiled with tic -x from the adjacent source; no runtime tic dependency.
        let bytes = include_bytes!("../../tests/fixtures/terminfo-e3.bin");
        let capabilities = super::parse_terminfo(bytes).unwrap();
        assert_eq!(
            capabilities.get("E3").map(Vec::as_slice),
            Some(&b"SCROLL"[..])
        );
    }

    #[test]
    fn extended_names_follow_present_values_and_boolean_numeric_names() {
        for num_size in [2, 4] {
            let table = b"SCROLL\0flag\0number\0absent\0cancelled\0E3\0";
            let mut bytes = Vec::new();
            // One boolean, one number, three strings; five names + one value.
            for field in [1u16, 1, 3, 6, table.len() as u16] {
                bytes.extend(field.to_le_bytes());
            }
            bytes.extend([1, 0]); // Boolean and alignment.
            bytes.extend(vec![0; num_size]);
            for offset in [-1i16, -2, 0, 0, 5, 12, 19, 29] {
                bytes.extend(offset.to_le_bytes());
            }
            bytes.extend(table);
            let mut capabilities = HashMap::new();
            super::parse_extended_terminfo(&bytes, 0, num_size, &mut capabilities).unwrap();
            assert_eq!(
                capabilities.get("E3").map(Vec::as_slice),
                Some(&b"SCROLL"[..])
            );
        }
    }

    #[test]
    fn extended_e3_absent_cancelled_and_malformed_values_are_not_exposed() {
        for value_offset in [-1i16, -2, 100] {
            let mut bytes = Vec::new();
            for field in [0u16, 0, 1, 1, 3] {
                bytes.extend(field.to_le_bytes());
            }
            bytes.extend(value_offset.to_le_bytes());
            bytes.extend(0i16.to_le_bytes()); // Name offset.
            bytes.extend(b"E3\0");
            let mut capabilities = HashMap::new();
            super::parse_extended_terminfo(&bytes, 0, 2, &mut capabilities);
            assert!(!capabilities.contains_key("E3"));
        }
    }

    #[test]
    fn truncated_extended_fixture_never_exposes_e3_or_initialize_pair() {
        let bytes = include_bytes!("../../tests/fixtures/terminfo-e3.bin");
        for end in 0..bytes.len() {
            if let Some(capabilities) = super::parse_terminfo(&bytes[..end]) {
                assert!(!capabilities.contains_key("E3"), "prefix length {end}");
            }
        }
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
