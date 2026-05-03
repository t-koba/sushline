use crate::completion::CompletionResponse;
use crate::variables::Variables;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct FilenameOptions {
    pub(crate) ignore_case: bool,
    pub(crate) map_case: bool,
    pub(crate) mark_directories: bool,
    pub(crate) mark_symlinked_directories: bool,
    pub(crate) match_hidden_files: bool,
    pub(crate) expand_tilde: bool,
    pub(crate) colored_stats: bool,
}

impl FilenameOptions {
    pub(crate) fn from_variables(variables: &Variables) -> Self {
        Self {
            ignore_case: variables.is_on("completion-ignore-case"),
            map_case: variables.is_on("completion-map-case"),
            mark_directories: variables.is_on("mark-directories"),
            mark_symlinked_directories: variables.is_on("mark-symlinked-directories"),
            match_hidden_files: variables.is_on("match-hidden-files"),
            expand_tilde: variables.is_on("expand-tilde"),
            colored_stats: variables.is_on("colored-stats"),
        }
    }
}

pub(crate) fn complete_filenames_bytes(word: &[u8], opts: &FilenameOptions) -> CompletionResponse {
    if let Ok(word) = std::str::from_utf8(word) {
        return complete_filenames_utf8(word, opts);
    }
    complete_filenames_raw_bytes(word, opts)
}

fn complete_filenames_utf8(word: &str, opts: &FilenameOptions) -> CompletionResponse {
    let expanded = expand_tilde(word);
    let path = Path::new(&expanded);
    let (dir, prefix, _) = split_word_path(&expanded);
    let (_, _, display_dir) = split_word_path(word);

    let mut candidates = Vec::new();
    let mut directory_candidate_count = 0;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let os_name = entry.file_name();
            let name = os_string_to_display(&os_name);
            let byte_prefix_match = os_name_starts_with(&os_name, prefix);
            if !opts.match_hidden_files && os_name_is_hidden(&os_name) && !prefix.starts_with('.') {
                continue;
            }
            if byte_prefix_match
                || completion_eq_prefix(&name, prefix, opts.ignore_case, opts.map_case)
            {
                let Some((completion_name, completion_bytes)) = os_string_to_completion(os_name)
                else {
                    continue;
                };
                let mut replacement_bytes = completion_bytes.clone();
                let mut replacement = if opts.expand_tilde {
                    format!(
                        "{}{completion_name}",
                        path.parent()
                            .filter(|parent| !parent.as_os_str().is_empty())
                            .map(|parent| format!("{}/", parent.display()))
                            .unwrap_or_default()
                    )
                } else {
                    format!("{display_dir}{completion_name}")
                };
                if let Some(bytes) = replacement_bytes.as_mut() {
                    let mut prefix_bytes = if opts.expand_tilde {
                        path.parent()
                            .filter(|parent| !parent.as_os_str().is_empty())
                            .map(|parent| format!("{}/", parent.display()).into_bytes())
                            .unwrap_or_default()
                    } else {
                        display_dir.as_bytes().to_vec()
                    };
                    prefix_bytes.extend_from_slice(bytes);
                    *bytes = prefix_bytes;
                }
                let mut display = (!display_dir.is_empty()).then(|| name.to_string());
                if let Ok(file_type) = entry.file_type()
                    && file_type.is_dir()
                {
                    directory_candidate_count += 1;
                    if opts.mark_directories
                        || (opts.mark_symlinked_directories
                            && is_symlinked_directory(&entry.path()))
                    {
                        replacement.push('/');
                        if let Some(bytes) = replacement_bytes.as_mut() {
                            bytes.push(b'/');
                        }
                    }
                    display = Some(if opts.colored_stats {
                        format!(
                            "\x1b[{}m{name}/\x1b[0m",
                            ls_color_code_for_candidate(&name, &entry.path(), "di")
                                .unwrap_or_else(|| "34".to_string())
                        )
                    } else if opts.mark_directories {
                        format!("{name}/")
                    } else {
                        name.to_string()
                    });
                } else if opts.colored_stats && is_executable_file(&entry.path()) {
                    display = Some(format!(
                        "\x1b[{}m{name}\x1b[0m",
                        ls_color_code_for_candidate(&name, &entry.path(), "ex")
                            .unwrap_or_else(|| "32".to_string())
                    ));
                } else if opts.colored_stats
                    && let Some(code) = ls_color_code_for_candidate(&name, &entry.path(), "fi")
                {
                    display = Some(format!("\x1b[{code}m{name}\x1b[0m"));
                }
                let replacement =
                    replacement_bytes.unwrap_or_else(|| replacement.as_bytes().to_vec());
                candidates.push(crate::completion::CompletionCandidate {
                    replacement,
                    display,
                });
            }
        }
    }

    let single_directory = candidates.len() == 1 && directory_candidate_count == 1;
    CompletionResponse {
        candidates,
        options: crate::completion::CompletionOptions {
            filenames: true,
            nospace: single_directory,
            ..Default::default()
        },
    }
}

#[cfg(unix)]
fn complete_filenames_raw_bytes(word: &[u8], opts: &FilenameOptions) -> CompletionResponse {
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let (dir_bytes, prefix, display_dir) = split_word_path_bytes(word);
    let dir = PathBuf::from(OsString::from_vec(dir_bytes.clone()));
    let mut candidates = Vec::new();
    let mut directory_candidate_count = 0;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let os_name = entry.file_name();
            let name_bytes = os_name.as_os_str().as_bytes();
            if !opts.match_hidden_files
                && name_bytes.first() == Some(&b'.')
                && prefix.first() != Some(&b'.')
            {
                continue;
            }
            if !name_bytes.starts_with(prefix) {
                continue;
            }
            let name = String::from_utf8_lossy(name_bytes).into_owned();
            let mut replacement_bytes = display_dir.clone();
            replacement_bytes.extend_from_slice(name_bytes);
            let mut display = (!display_dir.is_empty()).then(|| name.clone());
            if let Ok(file_type) = entry.file_type()
                && file_type.is_dir()
            {
                directory_candidate_count += 1;
                if opts.mark_directories
                    || (opts.mark_symlinked_directories && is_symlinked_directory(&entry.path()))
                {
                    replacement_bytes.push(b'/');
                }
                display = Some(if opts.colored_stats {
                    format!(
                        "\x1b[{}m{name}/\x1b[0m",
                        ls_color_code_for_candidate(&name, &entry.path(), "di")
                            .unwrap_or_else(|| "34".to_string())
                    )
                } else if opts.mark_directories {
                    format!("{name}/")
                } else {
                    name
                });
            }
            candidates.push(crate::completion::CompletionCandidate {
                replacement: replacement_bytes,
                display,
            });
        }
    }
    let single_unmarked_directory =
        candidates.len() == 1 && directory_candidate_count == 1 && !opts.mark_directories;
    CompletionResponse {
        candidates,
        options: crate::completion::CompletionOptions {
            filenames: true,
            nospace: single_unmarked_directory,
            ..Default::default()
        },
    }
}

#[cfg(not(unix))]
fn complete_filenames_raw_bytes(word: &[u8], opts: &FilenameOptions) -> CompletionResponse {
    complete_filenames_utf8(&String::from_utf8_lossy(word), opts)
}

pub(super) fn split_word_path(word: &str) -> (PathBuf, &str, String) {
    if word.ends_with('/') {
        return (PathBuf::from(word), "", word.to_string());
    }
    let path = Path::new(word);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let prefix = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let display_dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| format!("{}/", p.display()))
        .unwrap_or_default();
    (parent, prefix, display_dir)
}

#[cfg(unix)]
fn split_word_path_bytes(word: &[u8]) -> (Vec<u8>, &[u8], Vec<u8>) {
    if word.ends_with(b"/") {
        return (word.to_vec(), &b""[..], word.to_vec());
    }
    if let Some(pos) = word.iter().rposition(|byte| *byte == b'/') {
        return (
            word[..pos].to_vec(),
            &word[pos + 1..],
            word[..=pos].to_vec(),
        );
    }
    (b".".to_vec(), word, Vec::new())
}

pub(crate) fn complete_directories_bytes(
    word: &[u8],
    opts: &FilenameOptions,
) -> CompletionResponse {
    let mut response = complete_filenames_bytes(word, opts);
    response
        .candidates
        .retain(|candidate| is_directory_completion_bytes(word, candidate));
    response
}

fn is_directory_completion_bytes(
    word: &[u8],
    candidate: &crate::completion::CompletionCandidate,
) -> bool {
    if candidate.replacement_bytes().ends_with(b"/") {
        return true;
    }
    if let Ok(word) = std::str::from_utf8(word) {
        return is_directory_completion(word, &candidate.replacement_string());
    }
    false
}

fn is_directory_completion(word: &str, replacement: &str) -> bool {
    if replacement.ends_with('/') {
        return true;
    }
    let expanded = expand_tilde(replacement);
    if Path::new(&expanded).is_dir() {
        return true;
    }
    if !replacement.contains('/')
        && let Some((dir, _)) = word.rsplit_once('/')
    {
        return Path::new(&expand_tilde(dir)).join(replacement).is_dir();
    }
    false
}

pub(crate) fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

pub(crate) fn os_string_to_completion(
    value: std::ffi::OsString,
) -> Option<(String, Option<Vec<u8>>)> {
    match value.into_string() {
        Ok(value) => Some((value, None)),
        Err(value) => {
            let bytes = os_string_to_bytes(&value)?;
            Some((String::from_utf8_lossy(&bytes).into_owned(), Some(bytes)))
        }
    }
}

pub(crate) fn os_string_to_display(value: &std::ffi::OsStr) -> String {
    value
        .to_os_string()
        .into_string()
        .unwrap_or_else(os_string_lossy)
}

#[cfg(unix)]
fn os_string_lossy(value: std::ffi::OsString) -> String {
    use std::os::unix::ffi::OsStringExt;
    String::from_utf8_lossy(&value.into_vec()).into_owned()
}

#[cfg(unix)]
fn os_string_to_bytes(value: &std::ffi::OsString) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    Some(value.as_os_str().as_bytes().to_vec())
}

#[cfg(not(unix))]
fn os_string_lossy(value: std::ffi::OsString) -> String {
    value.to_string_lossy().into_owned()
}

#[cfg(not(unix))]
fn os_string_to_bytes(_value: &std::ffi::OsString) -> Option<Vec<u8>> {
    None
}

fn ls_color_code_for_candidate(name: &str, path: &Path, fallback_kind: &str) -> Option<String> {
    let colors = std::env::var("LS_COLORS").ok()?;
    let kind = ls_color_kind(path).unwrap_or(fallback_kind);
    ls_color_code_from_spec(name, path, kind, &colors)
}

pub(crate) fn ls_color_code_from_spec(
    name: &str,
    path: &Path,
    fallback_kind: &str,
    colors: &str,
) -> Option<String> {
    let mut fallback = None;
    for part in colors.split(':') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if let Some(pattern) = key.strip_prefix('*')
            && (name.ends_with(pattern) || ls_colors_glob_match(pattern, name))
        {
            return Some(value.to_string());
        }
        if key == "or"
            && path.symlink_metadata().is_ok_and(|metadata| {
                metadata.file_type().is_symlink() && fs::metadata(path).is_err()
            })
        {
            return Some(value.to_string());
        }
        if key == fallback_kind {
            fallback = Some(value.to_string());
        }
    }
    fallback
}

fn ls_color_kind(path: &Path) -> Option<&'static str> {
    let metadata = path.symlink_metadata().ok()?;
    let file_type = metadata.file_type();
    if file_type.is_dir() {
        Some("di")
    } else if file_type.is_symlink() {
        Some("ln")
    } else if is_executable_file(path) {
        Some("ex")
    } else {
        ls_color_kind_for_platform(&file_type).or(Some("fi"))
    }
}

#[cfg(unix)]
fn ls_color_kind_for_platform(file_type: &fs::FileType) -> Option<&'static str> {
    use std::os::unix::fs::FileTypeExt;
    if file_type.is_socket() {
        Some("so")
    } else if file_type.is_fifo() {
        Some("pi")
    } else if file_type.is_block_device() {
        Some("bd")
    } else if file_type.is_char_device() {
        Some("cd")
    } else {
        None
    }
}

#[cfg(not(unix))]
fn ls_color_kind_for_platform(_file_type: &fs::FileType) -> Option<&'static str> {
    None
}

fn is_symlinked_directory(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
        && path.is_dir()
}

fn ls_colors_glob_match(pattern: &str, value: &str) -> bool {
    glob_match(pattern, value)
}

fn completion_eq_prefix(candidate: &str, prefix: &str, ignore_case: bool, map_case: bool) -> bool {
    let normalize = |value: &str| {
        let mapped = if ignore_case && map_case {
            value.replace('-', "_")
        } else {
            value.to_string()
        };
        if ignore_case {
            mapped.to_lowercase()
        } else {
            mapped
        }
    };
    normalize(candidate).starts_with(&normalize(prefix))
}

#[cfg(unix)]
fn os_name_starts_with(name: &std::ffi::OsStr, prefix: &str) -> bool {
    use std::os::unix::ffi::OsStrExt;
    name.as_bytes().starts_with(prefix.as_bytes())
}

#[cfg(unix)]
pub(crate) fn os_name_is_hidden(name: &std::ffi::OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;
    name.as_bytes().first().is_some_and(|byte| *byte == b'.')
}

#[cfg(not(unix))]
fn os_name_starts_with(name: &std::ffi::OsStr, prefix: &str) -> bool {
    name.to_string_lossy().starts_with(prefix)
}

#[cfg(not(unix))]
pub(crate) fn os_name_is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

#[cfg(unix)]
pub(crate) fn glob_match_os(pattern: &str, name: &std::ffi::OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;
    glob_match_bytes(pattern.as_bytes(), name.as_bytes())
}

#[cfg(not(unix))]
pub(crate) fn glob_match_os(pattern: &str, name: &std::ffi::OsStr) -> bool {
    glob_match(pattern, &name.to_string_lossy())
}

pub(crate) fn glob_match(pattern: &str, value: &str) -> bool {
    fn inner(pattern: &[char], value: &[char]) -> bool {
        match (pattern.split_first(), value.split_first()) {
            (None, None) => true,
            (None, Some(_)) => false,
            (Some((&'\\', rest)), Some((&v, value_rest))) => {
                if let Some((&escaped, pattern_rest)) = rest.split_first() {
                    escaped == v && inner(pattern_rest, value_rest)
                } else {
                    v == '\\' && inner(rest, value_rest)
                }
            }
            (Some((&'*', rest)), _) => {
                inner(rest, value) || (!value.is_empty() && inner(pattern, &value[1..]))
            }
            (Some((&'?', rest)), Some((_, value_rest))) => inner(rest, value_rest),
            (Some((&'[', rest)), Some((&v, value_rest))) => {
                if let Some((matched, after_class)) = match_bracket_class(rest, v) {
                    matched && inner(after_class, value_rest)
                } else {
                    v == '[' && inner(rest, value_rest)
                }
            }
            (Some((&p, rest)), Some((&v, value_rest))) if p == v => inner(rest, value_rest),
            _ => false,
        }
    }
    inner(
        &pattern.chars().collect::<Vec<_>>(),
        &value.chars().collect::<Vec<_>>(),
    )
}

fn glob_match_bytes(pattern: &[u8], value: &[u8]) -> bool {
    match (pattern.split_first(), value.split_first()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some((&b'\\', rest)), Some((&v, value_rest))) => {
            if let Some((&escaped, pattern_rest)) = rest.split_first() {
                escaped == v && glob_match_bytes(pattern_rest, value_rest)
            } else {
                v == b'\\' && glob_match_bytes(rest, value_rest)
            }
        }
        (Some((&b'*', rest)), _) => {
            glob_match_bytes(rest, value)
                || (!value.is_empty() && glob_match_bytes(pattern, &value[1..]))
        }
        (Some((&b'?', rest)), Some((_, value_rest))) => glob_match_bytes(rest, value_rest),
        (Some((&b'[', rest)), Some((&v, value_rest))) => {
            if let Some((matched, after_class)) = match_bracket_class_bytes(rest, v) {
                matched && glob_match_bytes(after_class, value_rest)
            } else {
                v == b'[' && glob_match_bytes(rest, value_rest)
            }
        }
        (Some((&p, rest)), Some((&v, value_rest))) if p == v => glob_match_bytes(rest, value_rest),
        _ => false,
    }
}

fn match_bracket_class_bytes(pattern_after_open: &[u8], value: u8) -> Option<(bool, &[u8])> {
    let mut idx = 0;
    let mut negated = false;
    if matches!(pattern_after_open.first(), Some(b'!' | b'^')) {
        negated = true;
        idx += 1;
    }
    let mut matched = false;
    let mut saw_member = false;
    while idx < pattern_after_open.len() {
        let ch = pattern_after_open[idx];
        if ch == b']' && saw_member {
            return Some((matched != negated, &pattern_after_open[idx + 1..]));
        }
        saw_member = true;
        if idx + 2 < pattern_after_open.len()
            && pattern_after_open[idx + 1] == b'-'
            && pattern_after_open[idx + 2] != b']'
        {
            let end = pattern_after_open[idx + 2];
            matched |= ch <= value && value <= end;
            idx += 3;
        } else {
            matched |= ch == value;
            idx += 1;
        }
    }
    None
}

fn match_bracket_class(pattern_after_open: &[char], value: char) -> Option<(bool, &[char])> {
    let mut idx = 0;
    let mut negated = false;
    if matches!(pattern_after_open.first(), Some('!' | '^')) {
        negated = true;
        idx += 1;
    }
    let mut matched = false;
    let mut saw_member = false;
    while idx < pattern_after_open.len() {
        let ch = pattern_after_open[idx];
        if ch == ']' && saw_member {
            return Some((matched != negated, &pattern_after_open[idx + 1..]));
        }
        if ch == '['
            && pattern_after_open.get(idx + 1) == Some(&':')
            && let Some((class_matched, next_idx)) =
                match_posix_character_class(pattern_after_open, idx, value)
        {
            saw_member = true;
            matched |= class_matched;
            idx = next_idx;
            continue;
        }
        saw_member = true;
        if idx + 2 < pattern_after_open.len()
            && pattern_after_open[idx + 1] == '-'
            && pattern_after_open[idx + 2] != ']'
        {
            let end = pattern_after_open[idx + 2];
            if ch <= value && value <= end {
                matched = true;
            }
            idx += 3;
        } else {
            if ch == value {
                matched = true;
            }
            idx += 1;
        }
    }
    None
}

fn match_posix_character_class(chars: &[char], idx: usize, value: char) -> Option<(bool, usize)> {
    let mut end = idx + 2;
    while end + 1 < chars.len() {
        if chars[end] == ':' && chars[end + 1] == ']' {
            let name = chars[idx + 2..end].iter().collect::<String>();
            let matched = match name.as_str() {
                "alnum" => value.is_alphanumeric(),
                "alpha" => value.is_alphabetic(),
                "ascii" => value.is_ascii(),
                "blank" => matches!(value, ' ' | '\t'),
                "cntrl" => value.is_control(),
                "digit" => value.is_ascii_digit(),
                "graph" => !value.is_whitespace() && !value.is_control(),
                "lower" => value.is_lowercase(),
                "print" => !value.is_control(),
                "punct" => value.is_ascii_punctuation(),
                "space" => value.is_whitespace(),
                "upper" => value.is_uppercase(),
                "xdigit" => value.is_ascii_hexdigit(),
                _ => return None,
            };
            return Some((matched, end + 2));
        }
        end += 1;
    }
    None
}

pub(crate) fn expand_tilde(line: &str) -> String {
    let Some(home) = std::env::var_os("HOME") else {
        return line.to_string();
    };
    let home = home.to_string_lossy();
    if line == "~" {
        return home.into_owned();
    }
    if let Some(rest) = line.strip_prefix("~/") {
        return format!("{home}/{rest}");
    }
    if let Some(rest) = line.strip_prefix('~') {
        let (user, suffix) = rest.split_once('/').unwrap_or((rest, ""));
        if !user.is_empty()
            && let Some(user_home) = user_home_dir(user)
        {
            return if suffix.is_empty() {
                user_home
            } else {
                format!("{user_home}/{suffix}")
            };
        }
    }
    line.split_whitespace()
        .map(|word| {
            if word == "~" {
                home.to_string()
            } else if let Some(rest) = word.strip_prefix("~/") {
                format!("{home}/{rest}")
            } else if let Some(rest) = word.strip_prefix('~') {
                let (user, suffix) = rest.split_once('/').unwrap_or((rest, ""));
                if !user.is_empty()
                    && let Some(user_home) = user_home_dir(user)
                {
                    if suffix.is_empty() {
                        user_home
                    } else {
                        format!("{user_home}/{suffix}")
                    }
                } else {
                    word.to_string()
                }
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn user_home_dir(user: &str) -> Option<String> {
    if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let mut fields = line.split(':');
            if fields.next() == Some(user) {
                return fields.nth(4).map(str::to_string);
            }
        }
    }
    let output = Command::new("getent")
        .arg("passwd")
        .arg(user)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .and_then(|line| line.split(':').nth(5).map(str::to_string))
}
