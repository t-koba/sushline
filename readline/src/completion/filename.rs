use crate::completion::CompletionResponse;
use crate::variables::Variables;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) struct DirectoryCompletion {
    pub(super) append_slash: bool,
    pub(super) display_slash: bool,
}

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
    let completion_word = if opts.expand_tilde {
        expand_tilde_bytes(word)
    } else {
        word.to_vec()
    };
    let (dir_bytes, prefix, completion_display_dir) = split_word_path_bytes(&completion_word);
    let display_dir = if opts.expand_tilde {
        completion_display_dir
    } else {
        split_word_path_bytes(word).2
    };
    let Some(dir) = path_from_bytes(&dir_bytes) else {
        return filename_response(Vec::new(), 0);
    };
    let mut candidates = Vec::new();
    let mut directory_candidate_count = 0;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let os_name = entry.file_name();
            let Some(name_bytes) = os_str_to_completion_bytes(&os_name) else {
                continue;
            };
            if !opts.match_hidden_files
                && name_bytes.first() == Some(&b'.')
                && prefix.first() != Some(&b'.')
            {
                continue;
            }
            if !completion_prefix_matches_bytes(&name_bytes, prefix, opts) {
                continue;
            }
            let name = filename_display_name(&name_bytes);
            let mut replacement_bytes = display_dir.clone();
            replacement_bytes.extend_from_slice(&name_bytes);
            let mut display = (!display_dir.is_empty()).then(|| name.clone());
            if let Some(directory) = directory_completion(&entry.path(), opts) {
                directory_candidate_count += 1;
                display = Some(directory_display(&name, &entry.path(), opts, &directory));
            }
            candidates.push(crate::completion::CompletionCandidate {
                replacement: replacement_bytes,
                display,
            });
        }
    }
    filename_response(candidates, directory_candidate_count)
}

fn filename_response(
    candidates: Vec<crate::completion::CompletionCandidate>,
    directory_candidate_count: usize,
) -> CompletionResponse {
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

fn split_word_path_bytes(word: &[u8]) -> (Vec<u8>, &[u8], Vec<u8>) {
    if word.ends_with(b"/") {
        return (word.to_vec(), &b""[..], word.to_vec());
    }
    if let Some(pos) = word.iter().rposition(|byte| *byte == b'/') {
        if pos == 0 {
            return (b"/".to_vec(), &word[1..], b"/".to_vec());
        }
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
    response.candidates.retain(|candidate| {
        candidate.replacement_bytes().ends_with(b"/")
            || filename_directory_completion(word, candidate.replacement_bytes(), opts).is_some()
    });
    response.options.nospace = response.candidates.len() == 1;
    response
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
            let bytes = os_str_to_completion_bytes(&value)?;
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
fn os_str_to_completion_bytes(value: &std::ffi::OsStr) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    Some(value.as_bytes().to_vec())
}

#[cfg(not(unix))]
fn os_str_to_completion_bytes(value: &std::ffi::OsStr) -> Option<Vec<u8>> {
    value.to_str().map(|value| value.as_bytes().to_vec())
}

#[cfg(not(unix))]
fn os_string_lossy(value: std::ffi::OsString) -> String {
    value.to_string_lossy().into_owned()
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

pub(super) fn directory_completion(
    path: &Path,
    opts: &FilenameOptions,
) -> Option<DirectoryCompletion> {
    let metadata = path.symlink_metadata().ok()?;
    let file_type = metadata.file_type();
    let is_symlinked_directory = file_type.is_symlink() && path.is_dir();
    if !file_type.is_dir() && !is_symlinked_directory {
        return None;
    }
    Some(DirectoryCompletion {
        append_slash: opts.mark_directories
            && (!is_symlinked_directory || opts.mark_symlinked_directories),
        display_slash: opts.mark_directories,
    })
}

pub(super) fn filename_directory_completion(
    word: &[u8],
    replacement: &[u8],
    opts: &FilenameOptions,
) -> Option<DirectoryCompletion> {
    let path = completion_replacement_path(word, replacement)?;
    directory_completion(&path, opts)
}

pub(super) fn filename_display_name(replacement: &[u8]) -> String {
    let without_trailing_slashes = replacement
        .iter()
        .rposition(|byte| *byte != b'/')
        .map(|idx| &replacement[..=idx])
        .unwrap_or(replacement);
    let name = without_trailing_slashes
        .iter()
        .rposition(|byte| *byte == b'/')
        .map(|idx| &without_trailing_slashes[idx + 1..])
        .unwrap_or(without_trailing_slashes);
    let mut out = String::new();
    crate::buffer::append_bytes_lossless(&mut out, name);
    out
}

fn completion_replacement_path(word: &[u8], replacement: &[u8]) -> Option<PathBuf> {
    let replacement = expand_tilde_bytes(replacement);
    let path = path_from_bytes(&replacement)?;
    if path.is_dir() || replacement.contains(&b'/') {
        return Some(path);
    }
    if let Some(pos) = word.iter().rposition(|byte| *byte == b'/') {
        let mut joined = expand_tilde_bytes(&word[..=pos]);
        joined.extend_from_slice(&replacement);
        return path_from_bytes(&joined);
    }
    Some(path)
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    std::str::from_utf8(bytes).ok().map(PathBuf::from)
}

fn directory_display(
    name: &str,
    path: &Path,
    opts: &FilenameOptions,
    directory: &DirectoryCompletion,
) -> String {
    let suffix = if directory.display_slash { "/" } else { "" };
    if opts.colored_stats {
        format!(
            "\x1b[{}m{name}{suffix}\x1b[0m",
            ls_color_code_for_candidate(name, path, "di").unwrap_or_else(|| "34".to_string())
        )
    } else {
        format!("{name}{suffix}")
    }
}

fn ls_colors_glob_match(pattern: &str, value: &str) -> bool {
    glob_match(pattern, value)
}

fn completion_prefix_matches_bytes(
    candidate: &[u8],
    prefix: &[u8],
    opts: &FilenameOptions,
) -> bool {
    if candidate.starts_with(prefix) {
        return true;
    }
    if !opts.ignore_case || candidate.len() < prefix.len() {
        return false;
    }
    candidate
        .iter()
        .zip(prefix.iter())
        .all(|(candidate, prefix)| completion_byte_eq(*candidate, *prefix, opts.map_case))
}

fn completion_byte_eq(candidate: u8, prefix: u8, map_case: bool) -> bool {
    let candidate = map_completion_case_byte(candidate, map_case);
    let prefix = map_completion_case_byte(prefix, map_case);
    completion_tolower_byte(candidate) == completion_tolower_byte(prefix)
}

fn map_completion_case_byte(byte: u8, map_case: bool) -> u8 {
    if map_case && byte == b'-' { b'_' } else { byte }
}

#[cfg(unix)]
fn completion_tolower_byte(byte: u8) -> u8 {
    let lowered = unsafe { libc::tolower(byte as libc::c_uchar as libc::c_int) };
    if (0..=u8::MAX as libc::c_int).contains(&lowered) {
        lowered as u8
    } else {
        byte
    }
}

#[cfg(not(unix))]
fn completion_tolower_byte(byte: u8) -> u8 {
    byte.to_ascii_lowercase()
}

#[cfg(unix)]
pub(crate) fn os_name_is_hidden(name: &std::ffi::OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;
    name.as_bytes().first().is_some_and(|byte| *byte == b'.')
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

fn expand_tilde_bytes(line: &[u8]) -> Vec<u8> {
    expand_tilde_word_bytes(line).unwrap_or_else(|| line.to_vec())
}

fn expand_tilde_word_bytes(word: &[u8]) -> Option<Vec<u8>> {
    if word == b"~" {
        return std::env::var_os("HOME").and_then(|home| os_str_to_completion_bytes(&home));
    }
    if let Some(rest) = word.strip_prefix(b"~/") {
        let mut home =
            std::env::var_os("HOME").and_then(|home| os_str_to_completion_bytes(&home))?;
        home.push(b'/');
        home.extend_from_slice(rest);
        return Some(home);
    }
    let rest = word.strip_prefix(b"~")?;
    let (user, suffix) = if let Some(pos) = rest.iter().position(|byte| *byte == b'/') {
        (&rest[..pos], &rest[pos + 1..])
    } else {
        (rest, &b""[..])
    };
    if user.is_empty() {
        return None;
    }
    let mut home = user_home_dir_bytes(user)?;
    if !suffix.is_empty() {
        home.push(b'/');
        home.extend_from_slice(suffix);
    }
    Some(home)
}

#[cfg(unix)]
fn user_home_dir_bytes(user: &[u8]) -> Option<Vec<u8>> {
    if let Ok(passwd) = fs::read("/etc/passwd") {
        for line in passwd.split(|byte| *byte == b'\n') {
            let mut fields = line.split(|byte| *byte == b':');
            if fields.next() == Some(user) {
                return fields.nth(4).map(|field| field.to_vec());
            }
        }
    }

    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let output = Command::new("getent")
        .arg("passwd")
        .arg(OsString::from_vec(user.to_vec()))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .next()
        .and_then(|line| line.split(|byte| *byte == b':').nth(5))
        .map(|field| field.to_vec())
}

#[cfg(not(unix))]
fn user_home_dir_bytes(_user: &[u8]) -> Option<Vec<u8>> {
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
