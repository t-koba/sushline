use crate::completion::filename::{
    FilenameOptions, complete_filenames_bytes, expand_tilde, filename_matches_response,
    filenames_response, is_executable_file, join_display_dir, os_string_to_completion,
    split_word_path_bytes,
};
use crate::completion::{
    CompletionCandidate, CompletionOptions, CompletionRequest, CompletionResponse,
};
use crate::hooks::Hooks;
use crate::variables::Variables;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn visible_stats_marker(replacement: &str) -> Option<char> {
    let expanded = expand_tilde(replacement.trim_end_matches('/'));
    let path = Path::new(&expanded);
    let metadata = path.symlink_metadata().ok()?;
    let file_type = metadata.file_type();
    if file_type.is_dir() {
        Some('/')
    } else if file_type.is_symlink() {
        Some('@')
    } else if is_executable_file(path) {
        Some('*')
    } else {
        visible_stats_marker_for_platform(&file_type)
    }
}

#[cfg(unix)]
pub(super) fn visible_stats_marker_for_platform(file_type: &fs::FileType) -> Option<char> {
    use std::os::unix::fs::FileTypeExt;
    if file_type.is_socket() {
        Some('=')
    } else if file_type.is_fifo() {
        Some('|')
    } else {
        None
    }
}

#[cfg(not(unix))]
pub(super) fn visible_stats_marker_for_platform(_file_type: &fs::FileType) -> Option<char> {
    None
}

pub(super) fn default_application_completion(
    request: &CompletionRequest,
    hooks: &mut impl Hooks,
    variables: &Variables,
) -> CompletionResponse {
    if let Some(response) = hooks.default_complete(request) {
        return response;
    }
    complete_filenames_bytes(
        &request.context.word,
        &FilenameOptions::from_variables(variables),
    )
}

pub(super) fn complete_commands_bytes(word: &[u8]) -> CompletionResponse {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !is_executable_file(&path) {
                    continue;
                }
                let Some((replacement, replacement_bytes)) =
                    os_string_to_completion(entry.file_name())
                else {
                    continue;
                };
                let name_bytes = replacement_bytes
                    .as_deref()
                    .unwrap_or(replacement.as_bytes());
                if !name_bytes.starts_with(word) {
                    continue;
                }
                let replacement = replacement_bytes.unwrap_or_else(|| replacement.into_bytes());
                candidates.push(CompletionCandidate::plain(replacement));
            }
        }
    }
    CompletionResponse {
        candidates,
        options: Default::default(),
    }
}

pub(crate) fn complete_commands_with_hooks_bytes(
    word: &[u8],
    hooks: &mut impl Hooks,
) -> CompletionResponse {
    let mut response = complete_commands_bytes(word);
    response.candidates.extend(
        hooks
            .command_names()
            .into_iter()
            .filter(|name| name.starts_with(word))
            .map(CompletionCandidate::plain),
    );
    response
}

pub(super) fn complete_variables(word: &[u8], hooks: &mut impl Hooks) -> CompletionResponse {
    let has_sigil = word.first() == Some(&b'$');
    let prefix = word.strip_prefix(b"$").unwrap_or(word);
    let candidates = hooks
        .variable_names()
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .map(|name| {
            CompletionCandidate::plain(if has_sigil {
                let mut replacement = Vec::with_capacity(name.len() + 1);
                replacement.push(b'$');
                replacement.extend(name);
                replacement
            } else {
                name
            })
        })
        .collect();
    CompletionResponse {
        candidates,
        options: Default::default(),
    }
}

pub(super) fn complete_users(word: &[u8], hooks: &mut impl Hooks) -> CompletionResponse {
    let prefix = word.strip_prefix(b"~").unwrap_or(word);
    let mut names = Vec::new();
    if let Ok(passwd) = fs::read("/etc/passwd") {
        for line in passwd.split(|byte| *byte == b'\n') {
            if let Some(name) = passwd_user_name_bytes(line) {
                names.push(name.to_vec());
            }
        }
    }
    names.extend(system_user_names_bytes());
    names.extend(hooks.user_names());
    let candidates = prefixed_byte_candidates(names, prefix, |name| join_user_completion(&name));
    CompletionResponse {
        candidates,
        options: CompletionOptions {
            filenames: true,
            nospace: true,
            ..Default::default()
        },
    }
}

pub(super) fn complete_hosts(word: &[u8], hooks: &mut impl Hooks) -> CompletionResponse {
    let prefix = word.strip_prefix(b"@").unwrap_or(word);
    let mut hosts = Vec::new();
    if let Ok(hosts_source) = fs::read("/etc/hosts") {
        for line in hosts_source.split(|byte| *byte == b'\n') {
            if trim_ascii(line).first() == Some(&b'#') {
                continue;
            }
            for host in host_names_in_line_bytes(line) {
                hosts.push(host.to_vec());
            }
        }
    }
    hosts.extend(system_host_names_bytes());
    hosts.extend(known_host_names_bytes());
    hosts.extend(hooks.host_names());
    let candidates = prefixed_byte_candidates(hosts, prefix, |name| name);
    CompletionResponse {
        candidates,
        options: Default::default(),
    }
}

fn join_user_completion(name: &[u8]) -> Vec<u8> {
    let mut replacement = Vec::with_capacity(name.len() + 2);
    replacement.push(b'~');
    replacement.extend_from_slice(name);
    replacement.push(b'/');
    replacement
}

fn prefixed_byte_candidates(
    names: Vec<Vec<u8>>,
    prefix: &[u8],
    replacement: impl Fn(Vec<u8>) -> Vec<u8>,
) -> Vec<CompletionCandidate> {
    names
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .map(|name| CompletionCandidate::plain(replacement(name)))
        .collect()
}

fn trim_ascii(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|pos| pos + 1)
        .unwrap_or(0);
    if start >= end { &[] } else { &line[start..end] }
}

fn passwd_user_name_bytes(line: &[u8]) -> Option<&[u8]> {
    let pos = line.iter().position(|byte| *byte == b':')?;
    Some(&line[..pos])
}

fn host_names_in_line_bytes(line: &[u8]) -> impl Iterator<Item = &[u8]> {
    line.split(u8::is_ascii_whitespace)
        .filter(|field| !field.is_empty())
        .skip(1)
}

fn getent_lines_bytes(table: &str) -> Vec<Vec<u8>> {
    let Ok(output) = Command::new("getent").arg(table).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .map(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            line.to_vec()
        })
        .collect()
}

pub(super) fn system_user_names_bytes() -> Vec<Vec<u8>> {
    getent_lines_bytes("passwd")
        .iter()
        .filter_map(|line| passwd_user_name_bytes(line).map(|name| name.to_vec()))
        .collect()
}

pub(super) fn system_host_names_bytes() -> Vec<Vec<u8>> {
    getent_lines_bytes("hosts")
        .iter()
        .flat_map(|line| {
            host_names_in_line_bytes(line)
                .map(|host| host.to_vec())
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) fn known_host_names_bytes() -> Vec<Vec<u8>> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let path = home.join(".ssh").join("known_hosts");
    let Ok(source) = fs::read(path) else {
        return Vec::new();
    };
    let mut hosts = Vec::new();
    for line in source.split(|byte| *byte == b'\n') {
        let line = trim_ascii(line);
        if line.is_empty() || line.starts_with(b"#") || line.starts_with(b"|") {
            continue;
        }
        let Some(first) = line
            .split(u8::is_ascii_whitespace)
            .find(|field| !field.is_empty())
        else {
            continue;
        };
        for host in first.split(|byte| *byte == b',') {
            let host = trim_ascii(host);
            if host.is_empty() || host.starts_with(b"[") {
                continue;
            }
            hosts.push(host.to_vec());
        }
    }
    hosts
}

pub(crate) fn glob_complete_bytes(
    word: &[u8],
    hooks: &mut impl Hooks,
    variables: &Variables,
) -> CompletionResponse {
    use crate::completion::filename::{
        glob_match_bytes, os_str_to_completion_bytes, path_from_bytes,
    };
    if let Some(matches) = hooks.glob_expand(word) {
        return filename_matches_response(matches);
    }
    if !word.iter().any(|byte| matches!(byte, b'*' | b'?' | b'[')) {
        return complete_filenames_bytes(word, &FilenameOptions::from_variables(variables));
    }
    let (dir_bytes, pattern, display_dir) = split_word_path_bytes(word);
    let Some(dir) = path_from_bytes(&dir_bytes) else {
        return filenames_response(Vec::new());
    };
    let mut candidates = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let os_name = entry.file_name();
            let Some(name_bytes) = os_str_to_completion_bytes(&os_name) else {
                continue;
            };
            if pattern.first() != Some(&b'.') && name_bytes.first() == Some(&b'.') {
                continue;
            }
            if !glob_match_bytes(pattern, &name_bytes) {
                continue;
            }
            let replacement = join_display_dir(&display_dir, &name_bytes);
            candidates.push(CompletionCandidate::plain(replacement));
        }
    }
    filenames_response(candidates)
}
