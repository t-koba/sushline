use crate::completion::{CompletionCandidate, CompletionContext, CompletionResponse};
use crate::locale::LocaleLowercase;
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
    utf8_locale: bool,
    lc_ctype: Option<String>,
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
            utf8_locale: variables.utf8_locale(),
            lc_ctype: variables.lc_ctype().map(str::to_owned),
        }
    }
}

pub(crate) fn complete_filenames_bytes(word: &[u8], opts: &FilenameOptions) -> CompletionResponse {
    complete_filenames_with_tilde_context(word, opts, LeadingTildeContext::Unquoted)
}

pub(crate) fn complete_filenames_in_context(
    context: &CompletionContext,
    opts: &FilenameOptions,
) -> CompletionResponse {
    let tilde_context = leading_tilde_context(context);
    if context.word.starts_with(b"~/") && tilde_context == LeadingTildeContext::Escaped {
        return filename_response(Vec::new(), 0);
    }
    complete_filenames_with_tilde_context(&context.word, opts, tilde_context)
}

fn complete_filenames_with_tilde_context(
    word: &[u8],
    opts: &FilenameOptions,
    tilde_context: LeadingTildeContext,
) -> CompletionResponse {
    // In the measured Bash 5.3 cases, an unquoted ~/ resolves against HOME
    // for lookup regardless of expand-tilde, while the option controls whether
    // that expanded path is inserted. A quoted ~/ also resolves and inserts
    // the HOME path with the option off. Escaped leading tilde paths are not
    // expanded by the context-aware entry point.
    let expands_leading_tilde =
        word.starts_with(b"~/") && !matches!(tilde_context, LeadingTildeContext::Escaped);
    // Bash leaves slashless tilde words (bare `~` and the measured named-user
    // form) alone during filename completion, regardless of expand-tilde. The
    // measured named-user slash-path lookup resolves `~user/...`; generic
    // expansion omits an empty suffix, so preserve the final slash for the
    // filename lookup of `~user/` without changing generic expansion.
    let is_slashless_tilde = word.starts_with(b"~") && !word.contains(&b'/');
    let is_named_user_path = is_named_user_tilde_path(word);
    let mut lookup_word = if tilde_context != LeadingTildeContext::Escaped
        && ((opts.expand_tilde && !is_slashless_tilde)
            || expands_leading_tilde
            || is_named_user_path)
    {
        expand_tilde_bytes(word)
    } else {
        word.to_vec()
    };
    if is_named_user_path && word.ends_with(b"/") && !lookup_word.ends_with(b"/") {
        lookup_word.push(b'/');
    }
    let display_word = if opts.expand_tilde || tilde_context == LeadingTildeContext::Quoted {
        lookup_word.as_slice()
    } else {
        word
    };
    let (dir_bytes, prefix, _) = split_word_path_bytes(&lookup_word);
    let (_, _, display_dir) = split_word_path_bytes(display_word);
    let Some(dir) = path_from_bytes(&dir_bytes) else {
        return filename_response(Vec::new(), 0);
    };
    let locale_lowercase = if opts.ignore_case && opts.utf8_locale {
        opts.lc_ctype.as_deref().and_then(LocaleLowercase::new)
    } else {
        None
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
            if !completion_prefix_matches_bytes(
                &name_bytes,
                prefix,
                opts,
                locale_lowercase.as_ref(),
            ) {
                continue;
            }
            push_filename_candidate(
                &mut candidates,
                &mut directory_candidate_count,
                &display_dir,
                &name_bytes,
                &entry.path(),
                opts,
            );
        }

        // Bash includes the current and parent directories when the typed
        // prefix explicitly starts with a dot. Do not add them for an empty
        // prefix, and only synthesize them once the requested directory opened.
        if !prefix.is_empty() && prefix.first() == Some(&b'.') {
            for (name, name_bytes) in [(".", b".".as_slice()), ("..", b"..".as_slice())] {
                if !completion_prefix_matches_bytes(
                    name_bytes,
                    prefix,
                    opts,
                    locale_lowercase.as_ref(),
                ) {
                    continue;
                }
                push_filename_candidate(
                    &mut candidates,
                    &mut directory_candidate_count,
                    &display_dir,
                    name_bytes,
                    &dir.join(name),
                    opts,
                );
            }
        }
    }
    filename_response(candidates, directory_candidate_count)
}

fn push_filename_candidate(
    candidates: &mut Vec<CompletionCandidate>,
    directory_candidate_count: &mut usize,
    display_dir: &[u8],
    name_bytes: &[u8],
    path: &Path,
    opts: &FilenameOptions,
) {
    let name = filename_display_name(name_bytes);
    let replacement_bytes = join_display_dir(display_dir, name_bytes);
    let directory = directory_completion(path, opts);
    if directory.is_some() {
        *directory_candidate_count += 1;
    }
    let display = match directory.as_ref() {
        Some(directory) => Some(directory_display(&name, directory)),
        None => (!display_dir.is_empty()).then_some(name),
    };
    candidates.push(crate::completion::CompletionCandidate {
        replacement: replacement_bytes,
        display,
    });
}

pub(crate) fn filenames_response(candidates: Vec<CompletionCandidate>) -> CompletionResponse {
    filename_response(candidates, 0)
}

pub(crate) fn filename_matches_response(matches: Vec<Vec<u8>>) -> CompletionResponse {
    filenames_response(
        matches
            .into_iter()
            .map(CompletionCandidate::plain)
            .collect(),
    )
}

fn filename_response(
    candidates: Vec<CompletionCandidate>,
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

pub(crate) fn split_word_path_bytes(word: &[u8]) -> (Vec<u8>, &[u8], Vec<u8>) {
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

pub(crate) fn join_display_dir(display_dir: &[u8], name: &[u8]) -> Vec<u8> {
    let mut replacement = Vec::with_capacity(display_dir.len() + name.len());
    replacement.extend_from_slice(display_dir);
    replacement.extend_from_slice(name);
    replacement
}

#[cfg(test)]
pub(crate) fn complete_directories_bytes(
    word: &[u8],
    opts: &FilenameOptions,
) -> CompletionResponse {
    filter_directory_candidates(complete_filenames_bytes(word, opts), word, opts)
}

pub(crate) fn complete_directories_in_context(
    context: &CompletionContext,
    opts: &FilenameOptions,
) -> CompletionResponse {
    filter_directory_candidates(
        complete_filenames_in_context(context, opts),
        &context.word,
        opts,
    )
}

fn filter_directory_candidates(
    mut response: CompletionResponse,
    word: &[u8],
    opts: &FilenameOptions,
) -> CompletionResponse {
    response.candidates.retain(|candidate| {
        candidate.replacement_bytes().ends_with(b"/")
            || filename_directory_completion(word, candidate.replacement_bytes(), opts).is_some()
    });
    response.options.nospace = response.candidates.len() == 1;
    response
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LeadingTildeContext {
    Unquoted,
    Quoted,
    Escaped,
}

fn leading_tilde_context(context: &CompletionContext) -> LeadingTildeContext {
    let Some(raw_word) = context.line.get(context.word_start..context.word_end) else {
        return LeadingTildeContext::Unquoted;
    };
    let escaped_word = raw_word.strip_prefix(b"\"").unwrap_or(raw_word);
    if escaped_word
        .strip_prefix(b"\\~")
        .is_some_and(|rest| rest.contains(&b'/'))
    {
        return LeadingTildeContext::Escaped;
    }
    let quoted_word = raw_word
        .strip_prefix(b"$'")
        .or_else(|| raw_word.strip_prefix(b"'"))
        .or_else(|| raw_word.strip_prefix(b"\""));
    if quoted_word.is_some_and(|word| word.starts_with(b"~/") || is_named_user_tilde_path(word)) {
        return LeadingTildeContext::Quoted;
    }
    LeadingTildeContext::Unquoted
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

#[cfg(unix)]
pub(crate) fn os_str_to_completion_bytes(value: &std::ffi::OsStr) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    Some(value.as_bytes().to_vec())
}

#[cfg(not(unix))]
pub(crate) fn os_str_to_completion_bytes(value: &std::ffi::OsStr) -> Option<Vec<u8>> {
    value.to_str().map(|value| value.as_bytes().to_vec())
}

pub(crate) fn filename_ls_color_code(word: &[u8], replacement: &[u8]) -> Option<String> {
    let path = completion_replacement_path(word, replacement)?;
    let metadata = path.symlink_metadata().ok()?;
    let file_type = metadata.file_type();
    let kind = if file_type.is_dir() {
        "di"
    } else if file_type.is_symlink() {
        "ln"
    } else if file_type.is_file() && filename_is_executable(&path) {
        "ex"
    } else if file_type.is_file() {
        "fi"
    } else {
        return None;
    };
    let colors = std::env::var("LS_COLORS").unwrap_or_default();
    if kind == "di"
        && let Some(color) =
            ls_color_code_from_spec(&filename_display_name(replacement), &path, kind, &colors)
    {
        // Preserve the existing directory-name pattern behavior. This is not
        // extended to other candidate kinds by the measured type-code change.
        return Some(color);
    }
    if kind == "ln" && ls_color_type_code(&colors, "ln").as_deref() == Some("target") {
        // The measured ln=target case follows a regular, non-executable file
        // target and uses its fi style. Unknown target kinds fail closed rather
        // than treating the token `target` as an SGR parameter.
        let target = fs::metadata(&path).ok()?;
        if target.is_file() && !filename_is_executable(&path) {
            return ls_color_type_code(&colors, "fi").filter(|color| color != "target");
        }
        return None;
    }
    ls_color_type_code(&colors, kind).or_else(|| match kind {
        "di" => Some("01;34".to_string()),
        "ex" => Some("01;32".to_string()),
        "ln" => Some("01;36".to_string()),
        // Bash's tested default for regular files is no visible SGR style.
        "fi" => None,
        _ => None,
    })
}

#[cfg(unix)]
fn filename_is_executable(path: &Path) -> bool {
    is_executable_file(path)
}

#[cfg(not(unix))]
fn filename_is_executable(_path: &Path) -> bool {
    false
}

fn ls_color_type_code(colors: &str, kind: &str) -> Option<String> {
    colors.split(':').rev().find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == kind).then(|| value.to_string())
    })
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
    // GNU `fnprint` lists control bytes as caret notation (`^[` for ESC,
    // `^?` for DEL) so the listing stays inert text. Render C0 and DEL the
    // same way before any display-only wrapping; replacement bytes stay raw
    // and non-ASCII bytes keep the lossless round-trip below.
    let mut out = String::new();
    for byte in name {
        if *byte < 0x20 {
            out.push('^');
            out.push((*byte + 0x40) as char);
        } else if *byte == 0x7f {
            out.push_str("^?");
        } else {
            crate::buffer::append_bytes_lossless(&mut out, &[*byte]);
        }
    }
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
pub(crate) fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(not(unix))]
pub(crate) fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    std::str::from_utf8(bytes).ok().map(PathBuf::from)
}

fn directory_display(name: &str, directory: &DirectoryCompletion) -> String {
    let suffix = if directory.display_slash { "/" } else { "" };
    format!("{name}{suffix}")
}

fn ls_colors_glob_match(pattern: &str, value: &str) -> bool {
    glob_match(pattern, value)
}

pub(super) fn filename_candidate_prefix_len(
    candidate: &[u8],
    prefix: &[u8],
    opts: &FilenameOptions,
) -> Option<usize> {
    let locale_lowercase = if opts.ignore_case && opts.utf8_locale {
        opts.lc_ctype.as_deref().and_then(LocaleLowercase::new)
    } else {
        None
    };
    completion_prefix_len_bytes(candidate, prefix, opts, locale_lowercase.as_ref())
}

fn completion_prefix_matches_bytes(
    candidate: &[u8],
    prefix: &[u8],
    opts: &FilenameOptions,
    locale_lowercase: Option<&LocaleLowercase>,
) -> bool {
    completion_prefix_len_bytes(candidate, prefix, opts, locale_lowercase).is_some()
}

fn completion_prefix_len_bytes(
    candidate: &[u8],
    prefix: &[u8],
    opts: &FilenameOptions,
    locale_lowercase: Option<&LocaleLowercase>,
) -> Option<usize> {
    if candidate.starts_with(prefix) {
        return Some(prefix.len());
    }
    if !opts.ignore_case || candidate.len() < prefix.len() {
        return None;
    }
    if let Some(locale_lowercase) = locale_lowercase {
        return completion_prefix_len_utf8(candidate, prefix, opts.map_case, |character| {
            locale_lowercase.lowercase(character)
        });
    }
    candidate
        .iter()
        .zip(prefix.iter())
        .all(|(candidate, prefix)| completion_byte_eq(*candidate, *prefix, opts.map_case))
        .then_some(prefix.len())
}

#[cfg(test)]
fn completion_prefix_matches_utf8(
    candidate: &[u8],
    prefix: &[u8],
    map_case: bool,
    lowercase: impl FnMut(char) -> char,
) -> bool {
    completion_prefix_len_utf8(candidate, prefix, map_case, lowercase).is_some()
}

fn completion_prefix_len_utf8(
    candidate: &[u8],
    prefix: &[u8],
    map_case: bool,
    mut lowercase: impl FnMut(char) -> char,
) -> Option<usize> {
    let mut offset = 0;
    while offset < prefix.len() {
        match (
            decode_utf8_scalar(candidate, offset),
            decode_utf8_scalar(prefix, offset),
        ) {
            (Some((candidate_char, candidate_width)), Some((prefix_char, _))) => {
                let candidate_char = map_completion_case_char(candidate_char, map_case);
                let prefix_char = map_completion_case_char(prefix_char, map_case);
                if lowercase(candidate_char) != lowercase(prefix_char) {
                    return None;
                }
                offset += candidate_width;
            }
            _ => {
                if !completion_byte_eq(candidate[offset], prefix[offset], map_case) {
                    return None;
                }
                offset += 1;
            }
        }
    }
    (offset == prefix.len()).then_some(offset)
}

fn decode_utf8_scalar(bytes: &[u8], offset: usize) -> Option<(char, usize)> {
    let first = *bytes.get(offset)?;
    let width = match first {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return None,
    };
    let end = offset.checked_add(width)?;
    let scalar = std::str::from_utf8(bytes.get(offset..end)?)
        .ok()?
        .chars()
        .next()?;
    Some((scalar, width))
}

fn map_completion_case_char(character: char, map_case: bool) -> char {
    if map_case && character == '-' {
        '_'
    } else {
        character
    }
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

#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn glob_match_bytes(pattern: &[u8], value: &[u8]) -> bool {
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

pub(crate) fn glob_match_filename_bytes(pattern: &[u8], value: &[u8], utf8_locale: bool) -> bool {
    // Use character matching for valid UTF-8 patterns and names in UTF-8 locales.
    if utf8_locale
        && let (Ok(pattern), Ok(value)) = (std::str::from_utf8(pattern), std::str::from_utf8(value))
    {
        return glob_match(pattern, value);
    }
    glob_match_bytes(pattern, value)
}

#[cfg_attr(not(unix), allow(dead_code))]
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
        if ch == b'\\'
            && idx + 1 < pattern_after_open.len()
            && matches!(pattern_after_open[idx + 1], b'[' | b']')
        {
            saw_member = true;
            matched |= pattern_after_open[idx + 1] == value;
            idx += 2;
            continue;
        }
        if ch == b']' && saw_member {
            return Some((matched != negated, &pattern_after_open[idx + 1..]));
        }
        if ch == b'['
            && pattern_after_open.get(idx + 1) == Some(&b':')
            && let Some((class_matched, next_idx)) =
                match_posix_character_class_bytes(pattern_after_open, idx, value)
        {
            saw_member = true;
            matched |= class_matched;
            idx = next_idx;
            continue;
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

fn match_posix_character_class_bytes(
    pattern: &[u8],
    idx: usize,
    value: u8,
) -> Option<(bool, usize)> {
    let mut end = idx + 2;
    while end + 1 < pattern.len() {
        if pattern[end] == b':' && pattern[end + 1] == b']' {
            let name = &pattern[idx + 2..end];
            let matched = match name {
                b"alnum" => value.is_ascii_alphanumeric(),
                b"alpha" => value.is_ascii_alphabetic(),
                b"ascii" => value.is_ascii(),
                b"blank" => matches!(value, b' ' | b'\t'),
                b"cntrl" => value.is_ascii_control(),
                b"digit" => value.is_ascii_digit(),
                b"graph" => value.is_ascii_graphic(),
                b"lower" => value.is_ascii_lowercase(),
                b"print" => value.is_ascii_graphic() || value == b' ',
                b"punct" => value.is_ascii_punctuation(),
                b"space" => matches!(value, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r'),
                b"upper" => value.is_ascii_uppercase(),
                b"xdigit" => value.is_ascii_hexdigit(),
                _ => return None,
            };
            return Some((matched, end + 2));
        }
        end += 1;
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
        if ch == '\\'
            && idx + 1 < pattern_after_open.len()
            && matches!(pattern_after_open[idx + 1], '[' | ']')
        {
            saw_member = true;
            matched |= pattern_after_open[idx + 1] == value;
            idx += 2;
            continue;
        }
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

pub(crate) fn is_named_user_tilde_path(word: &[u8]) -> bool {
    word.strip_prefix(b"~")
        .and_then(|rest| rest.iter().position(|byte| *byte == b'/'))
        .is_some_and(|slash| slash > 0)
}

pub(crate) fn expand_tilde_bytes(line: &[u8]) -> Vec<u8> {
    expand_tilde_word_bytes(line).unwrap_or_else(|| line.to_vec())
}

fn current_shell_directory_bytes() -> Option<Vec<u8>> {
    let current_directory = std::env::current_dir().ok()?;
    let pwd = std::env::var_os("PWD");
    current_directory_bytes_for_pwd(pwd.as_deref(), &current_directory)
}

fn current_directory_bytes_for_pwd(
    pwd: Option<&std::ffi::OsStr>,
    current_directory: &Path,
) -> Option<Vec<u8>> {
    let current_bytes = os_str_to_completion_bytes(current_directory.as_os_str())?;
    if let Some(pwd) = pwd {
        let pwd_path = Path::new(pwd);
        if pwd_path.is_absolute()
            && let Ok(canonical_pwd) = pwd_path.canonicalize()
            && current_directory
                .canonicalize()
                .is_ok_and(|canonical_current| canonical_current == canonical_pwd)
            && let Some(pwd_bytes) = os_str_to_completion_bytes(pwd)
        {
            return Some(pwd_bytes);
        }
    }
    Some(current_bytes)
}

fn expand_tilde_word_bytes(word: &[u8]) -> Option<Vec<u8>> {
    // `~+` resolves from a valid shell PWD, preserving its logical spelling.
    if let Some(suffix) = word.strip_prefix(b"~+/") {
        let mut directory = current_shell_directory_bytes()?;
        if !directory.ends_with(b"/") {
            directory.push(b'/');
        }
        directory.extend_from_slice(suffix);
        return Some(directory);
    }
    // `~-` uses OLDPWD without validating it against the current directory.
    if let Some(suffix) = word.strip_prefix(b"~-/") {
        let mut directory = std::env::var_os("OLDPWD")
            .and_then(|directory| os_str_to_completion_bytes(&directory))?;
        if !directory.ends_with(b"/") {
            directory.push(b'/');
        }
        directory.extend_from_slice(suffix);
        return Some(directory);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_options() -> FilenameOptions {
        FilenameOptions {
            ignore_case: false,
            map_case: false,
            mark_directories: true,
            mark_symlinked_directories: true,
            match_hidden_files: true,
            expand_tilde: false,
            utf8_locale: false,
            lc_ctype: None,
        }
    }

    #[test]
    fn utf8_prefix_comparison_advances_by_candidate_byte_width() {
        let lowercase = |character: char| character.to_lowercase().next().unwrap_or(character);
        assert!(completion_prefix_matches_utf8(
            "Kelvin".as_bytes(),
            b"kzz",
            false,
            lowercase,
        ));
        assert!(completion_prefix_matches_utf8(
            "Kelvin".as_bytes(),
            b"kzze",
            false,
            lowercase,
        ));
        assert!(!completion_prefix_matches_utf8(
            "Kelvin".as_bytes(),
            b"kzzz",
            false,
            lowercase,
        ));
    }

    #[test]
    fn locale_lowercase_uses_the_owned_utf8_ctype_locale() {
        let Some(locale) = LocaleLowercase::new("C.UTF-8") else {
            return;
        };
        assert_eq!(locale.lowercase('Ä'), 'ä');
        assert_eq!(locale.lowercase('K'), 'k');
        assert_eq!(locale.lowercase('Σ'), 'σ');
        assert_eq!(locale.lowercase('ς'), 'ς');
        assert!(!completion_prefix_matches_utf8(
            "İndex".as_bytes(),
            b"i",
            false,
            |character| locale.lowercase(character),
        ));
        assert!(completion_prefix_matches_utf8(
            "İndex".as_bytes(),
            b"in",
            false,
            |character| locale.lowercase(character),
        ));
        assert!(!completion_prefix_matches_utf8(
            "Σigma".as_bytes(),
            "ς".as_bytes(),
            false,
            |character| locale.lowercase(character),
        ));
        assert!(completion_prefix_matches_utf8(
            "ÄLPHA".as_bytes(),
            "älp".as_bytes(),
            false,
            |character| locale.lowercase(character),
        ));
        assert!(completion_prefix_matches_utf8(
            "Kelvin".as_bytes(),
            b"kzz",
            false,
            |character| locale.lowercase(character),
        ));

        let Some(c_locale) = LocaleLowercase::new("C") else {
            return;
        };
        assert_eq!(c_locale.lowercase('Ä'), 'Ä');
        assert!(!completion_prefix_matches_utf8(
            "ÄLPHA".as_bytes(),
            "älp".as_bytes(),
            false,
            |character| c_locale.lowercase(character),
        ));
    }

    #[test]
    fn utf8_prefix_comparison_falls_back_to_bytewise_matching_for_invalid_units() {
        let lowercase = |character: char| character.to_lowercase().next().unwrap_or(character);
        let candidate = [0xff, b' ', b'A', b'l', b'p', b'h', b'a'];
        assert!(completion_prefix_matches_utf8(
            &candidate,
            &[0xff, b' ', b'a', b'l', b'p'],
            false,
            lowercase,
        ));
        assert!(!completion_prefix_matches_utf8(
            &candidate,
            &[0xfe, b' ', b'a', b'l', b'p'],
            false,
            lowercase,
        ));
    }

    #[test]
    fn glob_bracket_classes_match_escaped_open_and_close_delimiters() {
        assert!(glob_match(r"[\]]", "]"));
        assert!(glob_match(r"[\[]", "["));
        assert!(!glob_match(r"[\]]", "["));
        assert!(!glob_match(r"[\[]", "]"));

        assert!(glob_match_filename_bytes(b"[\\]]", b"]", false));
        assert!(glob_match_filename_bytes(b"[\\[]", b"[", false));
        assert!(glob_match_filename_bytes(b"[\\]]", b"]", true));
        assert!(glob_match_filename_bytes(b"[\\[]", b"[", true));
    }

    #[test]
    fn glob_matching_uses_utf8_characters_only_in_utf8_locales() {
        assert!(super::glob_match_filename_bytes(
            b"?clair",
            "éclair".as_bytes(),
            true,
        ));
        assert!(!super::glob_match_filename_bytes(
            b"?clair",
            "éclair".as_bytes(),
            false,
        ));
        assert!(super::glob_match_filename_bytes(
            b"pre\xff*",
            b"pre\xff-match",
            true,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn invalid_utf8_filename_bytes_survive_matching_and_replacement() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        if LocaleLowercase::new("C.UTF-8").is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let filename = [0xff, b' ', b'A', b'l', b'p', b'h', b'a'];
        fs::write(
            dir.path()
                .join(std::ffi::OsString::from_vec(filename.to_vec())),
            b"",
        )
        .unwrap();
        let mut word = dir.path().as_os_str().as_bytes().to_vec();
        word.extend_from_slice(b"/\xff alp");
        let mut opts = test_options();
        opts.ignore_case = true;
        opts.utf8_locale = true;
        opts.lc_ctype = Some("C.UTF-8".to_string());

        let response = complete_filenames_bytes(&word, &opts);
        assert_eq!(response.candidates.len(), 1);
        assert!(
            response.candidates[0]
                .replacement_bytes()
                .ends_with(&filename)
        );
    }

    #[cfg(unix)]
    #[test]
    fn current_directory_validation_rejects_stale_pwd_and_preserves_logical_pwd() {
        use std::os::unix::fs::symlink;

        let current = tempfile::tempdir().expect("current directory fixture");
        let stale = tempfile::tempdir().expect("stale PWD fixture");
        let logical_root = tempfile::tempdir().expect("logical PWD fixture");
        let logical = logical_root.path().join("logical-cwd");
        symlink(current.path(), &logical).expect("create logical PWD symlink");
        let current_bytes = os_str_to_completion_bytes(current.path().as_os_str())
            .expect("current directory bytes");
        let logical_bytes =
            os_str_to_completion_bytes(logical.as_os_str()).expect("logical PWD bytes");

        assert!(
            current_directory_bytes_for_pwd(Some(stale.path().as_os_str()), current.path())
                .as_deref()
                == Some(current_bytes.as_slice()),
            "stale PWD did not fall back to the current directory"
        );
        assert!(
            current_directory_bytes_for_pwd(None, current.path()).as_deref()
                == Some(current_bytes.as_slice()),
            "unset PWD did not fall back to the current directory"
        );
        assert!(
            current_directory_bytes_for_pwd(Some(std::ffi::OsStr::new("relative")), current.path())
                .as_deref()
                == Some(current_bytes.as_slice()),
            "relative PWD did not fall back to the current directory"
        );
        assert!(
            current_directory_bytes_for_pwd(Some(logical.as_os_str()), current.path()).as_deref()
                == Some(logical_bytes.as_slice()),
            "valid logical PWD spelling was not preserved"
        );
    }

    #[test]
    fn control_bytes_render_as_caret_notation() {
        assert_eq!(filename_display_name(b"probe\x1b[2Jend"), "probe^[[2Jend");
        assert_eq!(filename_display_name(b"a\x07b"), "a^Gb");
        assert_eq!(filename_display_name(b"a\x08b"), "a^Hb");
        assert_eq!(filename_display_name(b"a\tb"), "a^Ib");
        assert_eq!(filename_display_name(b"a\nb"), "a^Jb");
        assert_eq!(filename_display_name(b"a\x0bb"), "a^Kb");
        assert_eq!(filename_display_name(b"a\x0cb"), "a^Lb");
        assert_eq!(filename_display_name(b"a\rb"), "a^Mb");
        assert_eq!(filename_display_name(b"a\x7fb"), "a^?b");
        assert_eq!(filename_display_name(b"plain"), "plain");
    }

    #[test]
    fn basename_and_lossless_bytes_are_preserved() {
        assert_eq!(
            filename_display_name(b"/tmp/dir/probe\x1b[2Jend"),
            "probe^[[2Jend"
        );
        assert_eq!(filename_display_name(b"dir/"), "dir");
        let display = filename_display_name(&[0xff, b'a']);
        assert_eq!(
            crate::buffer::rendered_string_to_bytes(&display),
            vec![0xff, b'a']
        );
    }

    #[test]
    fn discovered_control_filename_lists_caret_but_replaces_raw() {
        let dir = tempfile::tempdir().unwrap();
        let raw: Vec<u8> = b"probe\x1b[2Jend".to_vec();
        let path = super::path_from_bytes(&raw).unwrap();
        std::fs::write(dir.path().join(&path), b"x").unwrap();
        let word = dir
            .path()
            .join("probe")
            .to_string_lossy()
            .into_owned()
            .into_bytes();
        let response = complete_filenames_bytes(&word, &test_options());
        assert_eq!(response.candidates.len(), 1);
        let candidate = &response.candidates[0];
        assert!(candidate.replacement_bytes().ends_with(&raw));
        let listed = candidate.display.as_deref().expect("file display");
        assert_eq!(listed, "probe^[[2Jend");
        assert!(!listed.contains('\u{1b}'));
        // The shared basename helper used by the listing fallback agrees.
        assert_eq!(
            filename_display_name(candidate.replacement_bytes()),
            "probe^[[2Jend"
        );
    }
}
