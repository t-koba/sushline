use crate::completion::filename::{
    FilenameOptions, complete_filenames_bytes, complete_filenames_in_context, expand_tilde_bytes,
    filename_matches_response, filenames_response, is_executable_file, join_display_dir,
    os_str_to_completion_bytes, path_from_bytes, split_word_path_bytes,
};
use crate::completion::{
    CompletionCandidate, CompletionContext, CompletionOptions, CompletionRequest,
    CompletionResponse,
};
use crate::hooks::Hooks;
use crate::variables::Variables;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub(crate) fn visible_stats_marker_bytes(replacement: &[u8]) -> Option<char> {
    let trimmed = trim_trailing_slashes(replacement);
    let expanded = expand_tilde_bytes(trimmed);
    let path = path_from_bytes(&expanded)?;
    let metadata = path.symlink_metadata().ok()?;
    let file_type = metadata.file_type();
    if file_type.is_dir() {
        Some('/')
    } else if file_type.is_symlink() {
        Some('@')
    } else if is_executable_file(&path) {
        Some('*')
    } else {
        visible_stats_marker_for_platform(&file_type)
    }
}

fn trim_trailing_slashes(replacement: &[u8]) -> &[u8] {
    let end = replacement
        .iter()
        .rposition(|byte| *byte != b'/')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    &replacement[..end]
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
    filename_context: &CompletionContext,
) -> (CompletionResponse, bool) {
    if let Some(response) = hooks.default_complete(request) {
        return (response, false);
    }
    (
        complete_filenames_in_context(
            filename_context,
            &FilenameOptions::from_variables(variables),
        ),
        true,
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
                let Some(name_bytes) = os_str_to_completion_bytes(&entry.file_name()) else {
                    continue;
                };
                if !name_bytes.starts_with(word) {
                    continue;
                }
                candidates.push(CompletionCandidate::plain(name_bytes));
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
    let end = line
        .iter()
        .position(|byte| *byte == b'#')
        .unwrap_or(line.len());
    let line = &line[..end];
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

fn known_host_names_in_line_bytes(line: &[u8]) -> impl Iterator<Item = &[u8]> {
    let line = trim_ascii(line);
    let host_list: &[u8] = if line.is_empty() || line.starts_with(b"#") || line.starts_with(b"|") {
        &[]
    } else {
        let mut fields = line
            .split(u8::is_ascii_whitespace)
            .filter(|field| !field.is_empty());
        let first = fields.next().unwrap_or(&[]);
        if first == b"@cert-authority" || first == b"@revoked" {
            fields.next().unwrap_or(&[])
        } else {
            first
        }
    };
    host_list
        .split(|byte| *byte == b',')
        .map(trim_ascii)
        .filter(|host| !host.is_empty() && !host.starts_with(b"["))
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
        for host in known_host_names_in_line_bytes(line) {
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
        glob_match_filename_bytes, os_str_to_completion_bytes, path_from_bytes,
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
    let utf8_locale = variables.utf8_locale();
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
            if !glob_match_filename_bytes(pattern, &name_bytes, utf8_locale) {
                continue;
            }
            let replacement = join_display_dir(&display_dir, &name_bytes);
            candidates.push(CompletionCandidate::plain(replacement));
        }
    }
    filenames_response(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts_in_line(line: &[u8]) -> Vec<Vec<u8>> {
        host_names_in_line_bytes(line)
            .map(|host| host.to_vec())
            .collect()
    }

    #[test]
    fn hosts_line_discards_inline_comment_words() {
        assert_eq!(
            hosts_in_line(b"127.0.0.1 localhost myhost alias # comment prose words"),
            vec![b"localhost".to_vec(), b"myhost".to_vec(), b"alias".to_vec(),]
        );
    }

    #[test]
    fn hosts_line_ignores_full_line_comment() {
        assert!(hosts_in_line(b"# localhost myhost comment prose").is_empty());
    }

    fn known_hosts_in_line(line: &[u8]) -> Vec<Vec<u8>> {
        known_host_names_in_line_bytes(line)
            .map(|host| host.to_vec())
            .collect()
    }

    #[test]
    fn known_hosts_line_splits_plain_comma_separated_hosts() {
        assert_eq!(
            known_hosts_in_line(b"alpha,beta ssh-rsa AAAAB3NzaC1"),
            vec![b"alpha".to_vec(), b"beta".to_vec(),]
        );
    }

    #[test]
    fn known_hosts_cert_authority_marker_supplies_host_list() {
        assert_eq!(
            known_hosts_in_line(b"@cert-authority gamma,delta ssh-rsa AAAAB3NzaC1"),
            vec![b"gamma".to_vec(), b"delta".to_vec(),]
        );
    }

    #[test]
    fn known_hosts_revoked_marker_supplies_host_list() {
        assert_eq!(
            known_hosts_in_line(b"@revoked epsilon ssh-rsa AAAAB3NzaC1"),
            vec![b"epsilon".to_vec(),]
        );
    }

    #[test]
    fn known_hosts_marked_line_without_host_field_yields_no_candidates() {
        assert!(known_hosts_in_line(b"@cert-authority").is_empty());
        assert!(known_hosts_in_line(b"@revoked").is_empty());
    }

    #[test]
    fn known_hosts_line_keeps_hashed_and_bracketed_filtering() {
        assert!(known_hosts_in_line(b"|1|salt|hash ssh-rsa AAAAB3NzaC1").is_empty());
        assert_eq!(
            known_hosts_in_line(b"@revoked plain,[10.0.0.1]:2222 ssh-rsa AAAAB3NzaC1"),
            vec![b"plain".to_vec(),]
        );
    }
}
