use crate::completion::filename::{
    FilenameOptions, complete_filenames_bytes, expand_tilde, glob_match, glob_match_os,
    is_executable_file, os_name_is_hidden, os_string_to_completion, os_string_to_display,
    split_word_path,
};
use crate::completion::{
    CompletionCandidate, CompletionOptions, CompletionRequest, CompletionResponse,
};
use crate::hooks::Hooks;
use crate::variables::Variables;
use std::collections::BTreeMap;
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
    let mut names = BTreeMap::<Vec<u8>, CompletionCandidate>::new();
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
                let os_name = entry.file_name();
                let name = os_string_completion_bytes(&os_name);
                if name.starts_with(word)
                    && let Some((replacement, replacement_bytes)) = os_string_to_completion(os_name)
                {
                    let replacement = replacement_bytes.unwrap_or_else(|| replacement.into_bytes());
                    names
                        .entry(replacement.clone())
                        .or_insert(CompletionCandidate {
                            replacement,
                            display: None,
                        });
                }
            }
        }
    }
    CompletionResponse {
        candidates: names.into_values().collect(),
        options: Default::default(),
    }
}

#[cfg(unix)]
pub(super) fn os_string_completion_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().to_vec()
}

#[cfg(not(unix))]
pub(super) fn os_string_completion_bytes(value: &std::ffi::OsStr) -> Vec<u8> {
    value.to_string_lossy().as_bytes().to_vec()
}

pub(crate) fn complete_commands_with_hooks_bytes(
    word: &[u8],
    hooks: &impl Hooks,
) -> CompletionResponse {
    let mut response = complete_commands_bytes(word);
    response.candidates.extend(
        hooks
            .command_names()
            .into_iter()
            .filter(|name| name.as_bytes().starts_with(word))
            .map(|name| CompletionCandidate {
                replacement: name.into_bytes(),
                display: None,
            }),
    );
    response
}

pub(super) fn complete_variables(word: &str, hooks: &mut impl Hooks) -> CompletionResponse {
    let has_sigil = word.starts_with('$');
    let prefix = word.strip_prefix('$').unwrap_or(word);
    let candidates = hooks
        .variable_names()
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .map(|name| CompletionCandidate {
            replacement: if has_sigil { format!("${name}") } else { name }.into_bytes(),
            display: None,
        })
        .collect();
    CompletionResponse {
        candidates,
        options: Default::default(),
    }
}

pub(super) fn complete_users(word: &str, hooks: &impl Hooks) -> CompletionResponse {
    let prefix = word.strip_prefix('~').unwrap_or(word);
    let mut names = BTreeMap::<String, ()>::new();
    if let Ok(passwd) = fs::read_to_string("/etc/passwd") {
        for line in passwd.lines() {
            let Some((name, _)) = line.split_once(':') else {
                continue;
            };
            names.insert(name.to_string(), ());
        }
    }
    for name in system_user_names() {
        names.insert(name, ());
    }
    for name in hooks.user_names() {
        names.insert(name, ());
    }
    let candidates = names
        .into_keys()
        .filter(|name| name.starts_with(prefix))
        .map(|name| CompletionCandidate {
            replacement: format!("~{name}/").into_bytes(),
            display: None,
        })
        .collect();
    CompletionResponse {
        candidates,
        options: CompletionOptions {
            filenames: true,
            nospace: true,
            ..Default::default()
        },
    }
}

pub(super) fn complete_hosts(word: &str, hooks: &impl Hooks) -> CompletionResponse {
    let prefix = word.strip_prefix('@').unwrap_or(word);
    let mut hosts = BTreeMap::<String, ()>::new();
    if let Ok(hosts_source) = fs::read_to_string("/etc/hosts") {
        for line in hosts_source
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
        {
            for host in line.split_whitespace().skip(1) {
                hosts.insert(host.to_string(), ());
            }
        }
    }
    for host in system_host_names().into_iter().chain(known_host_names()) {
        hosts.insert(host, ());
    }
    for host in hooks.host_names() {
        hosts.insert(host, ());
    }
    let candidates = hosts
        .into_keys()
        .filter(|host| host.starts_with(prefix))
        .map(|host| CompletionCandidate {
            replacement: host.into_bytes(),
            display: None,
        })
        .collect();
    CompletionResponse {
        candidates,
        options: Default::default(),
    }
}

pub(super) fn system_user_names() -> Vec<String> {
    let Ok(output) = Command::new("getent").arg("passwd").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once(':').map(|(name, _)| name.to_string()))
        .collect()
}

pub(super) fn system_host_names() -> Vec<String> {
    let Ok(output) = Command::new("getent").arg("hosts").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .flat_map(|line| {
            line.split_whitespace()
                .skip(1)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) fn known_host_names() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let path = home.join(".ssh").join("known_hosts");
    let Ok(source) = fs::read_to_string(path) else {
        return Vec::new();
    };
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.starts_with('|'))
        .filter_map(|line| line.split_whitespace().next())
        .flat_map(|hosts| hosts.split(','))
        .filter_map(|host| {
            let host = host.trim();
            if host.is_empty() || host.starts_with('[') {
                None
            } else {
                Some(host.to_string())
            }
        })
        .collect()
}

pub(crate) fn glob_complete(
    word: &str,
    hooks: &impl Hooks,
    variables: &Variables,
) -> CompletionResponse {
    if let Some(matches) = hooks.glob_expand(word) {
        return CompletionResponse {
            candidates: matches
                .into_iter()
                .map(|replacement| CompletionCandidate {
                    replacement: replacement.into_bytes(),
                    display: None,
                })
                .collect(),
            options: CompletionOptions {
                filenames: true,
                ..Default::default()
            },
        };
    }
    if !word.contains(['*', '?', '[']) {
        return complete_filenames_bytes(
            word.as_bytes(),
            &FilenameOptions::from_variables(variables),
        );
    }
    let (dir, pattern, display_dir) = split_word_path(word);
    let mut candidates = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let os_name = entry.file_name();
            let name = os_string_to_display(&os_name);
            if !pattern.starts_with('.') && os_name_is_hidden(&os_name) {
                continue;
            }
            if glob_match_os(pattern, &os_name) || glob_match(pattern, &name) {
                let Some((completion_name, completion_bytes)) = os_string_to_completion(os_name)
                else {
                    continue;
                };
                let mut replacement_bytes = completion_bytes;
                if let Some(bytes) = replacement_bytes.as_mut() {
                    let mut prefixed = display_dir.as_bytes().to_vec();
                    prefixed.extend_from_slice(bytes);
                    *bytes = prefixed;
                }
                candidates.push(CompletionCandidate {
                    replacement: replacement_bytes
                        .unwrap_or_else(|| format!("{display_dir}{completion_name}").into_bytes()),
                    display: None,
                });
            }
        }
    }
    CompletionResponse {
        candidates,
        options: CompletionOptions {
            filenames: true,
            ..Default::default()
        },
    }
}

pub(super) fn glob_complete_bytes(
    word: &[u8],
    hooks: &impl Hooks,
    variables: &Variables,
) -> CompletionResponse {
    if let Ok(word) = std::str::from_utf8(word) {
        return glob_complete(word, hooks, variables);
    }
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        if !word.iter().any(|byte| matches!(byte, b'*' | b'?' | b'[')) {
            return complete_filenames_bytes(word, &FilenameOptions::from_variables(variables));
        }
        let slash = word.iter().rposition(|byte| *byte == b'/');
        let (dir_bytes, pattern, display_dir) = if let Some(pos) = slash {
            (
                word[..pos].to_vec(),
                &word[pos + 1..],
                word[..=pos].to_vec(),
            )
        } else {
            (b".".to_vec(), word, Vec::new())
        };
        let dir = PathBuf::from(OsString::from_vec(dir_bytes));
        let mut candidates = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let os_name = entry.file_name();
                let name_bytes = os_name.as_os_str().as_bytes();
                if pattern.first() != Some(&b'.') && name_bytes.first() == Some(&b'.') {
                    continue;
                }
                if !glob_match_bytes_raw(pattern, name_bytes) {
                    continue;
                }
                let mut replacement = display_dir.clone();
                replacement.extend_from_slice(name_bytes);
                candidates.push(CompletionCandidate {
                    replacement,
                    display: None,
                });
            }
        }
        CompletionResponse {
            candidates,
            options: CompletionOptions {
                filenames: true,
                ..Default::default()
            },
        }
    }
    #[cfg(not(unix))]
    {
        glob_complete(&String::from_utf8_lossy(word), hooks, variables)
    }
}

pub(super) fn glob_match_bytes_raw(pattern: &[u8], name: &[u8]) -> bool {
    fn rec(pattern: &[u8], name: &[u8]) -> bool {
        match pattern.split_first() {
            None => name.is_empty(),
            Some((&b'*', rest)) => {
                rec(rest, name) || (!name.is_empty() && rec(pattern, &name[1..]))
            }
            Some((&b'?', rest)) => !name.is_empty() && rec(rest, &name[1..]),
            Some((&b'[', rest)) => {
                let Some(end) = rest.iter().position(|byte| *byte == b']') else {
                    return !name.is_empty() && name[0] == b'[' && rec(rest, &name[1..]);
                };
                if name.is_empty() {
                    return false;
                }
                let class = &rest[..end];
                let mut matched = false;
                let mut idx = 0;
                while idx < class.len() {
                    if idx + 2 < class.len() && class[idx + 1] == b'-' {
                        matched |= (class[idx]..=class[idx + 2]).contains(&name[0]);
                        idx += 3;
                    } else {
                        matched |= class[idx] == name[0];
                        idx += 1;
                    }
                }
                matched && rec(&rest[end + 1..], &name[1..])
            }
            Some((&ch, rest)) => !name.is_empty() && name[0] == ch && rec(rest, &name[1..]),
        }
    }
    rec(pattern, name)
}
