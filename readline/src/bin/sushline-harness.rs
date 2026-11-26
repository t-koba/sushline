use readline::{
    Config, Editor, History, HistoryExpansion, HistoryExpansionContext, Hooks, Prompt,
    PromptEscapes, ReadlineResult, SigintPolicy, Terminal, expand_history_with_status_with_state,
};
use std::ffi::OsString;
use std::process::ExitCode;
use std::{fs, path::Path};

struct HarnessApplication;

impl Hooks for HarnessApplication {
    fn command_names(&mut self) -> Vec<Vec<u8>> {
        let mut names = vec![b"echo".to_vec()];
        for dir in std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if is_executable_file(&path)
                    && let Some(name) = command_name_bytes(&entry.file_name())
                {
                    names.push(name);
                }
            }
        }
        names.sort();
        names.dedup();
        names
    }

    fn variable_names(&mut self) -> Vec<Vec<u8>> {
        environment_variable_names(std::env::vars_os())
    }

    fn expand_history(
        &mut self,
        context: HistoryExpansionContext<'_>,
    ) -> Result<HistoryExpansion, String> {
        let line = context.line;
        expand_history_with_status_with_state(
            line,
            context.history,
            context.histchars,
            context.policy,
            context.expansion_state,
            |idx| inhibits_history_expansion(line, idx),
        )
        .map_err(|err| err.message())
    }
}

// The harness embeds Sushline in a shell-like prompt, so it installs the
// narrow history inhibitions Bash 5.3 uses. Bash's candidate scan suppresses
// `[!...]`, `${!...}` and `$!`. With another ordinary event on the line its
// expansion pass re-checks a bracket or indirect `!` against the output built
// so far plus one byte, so the bang stays literal there only when that byte is
// its closing delimiter. Sushline's single pass approximates those accepted
// lines here. `$!` stays inhibited at its own index.
#[derive(Clone, Copy)]
enum ShellBang {
    Plain,
    Dollar,
    Bracket(u8),
    BareColon,
}

fn inhibits_history_expansion(line: &[u8], idx: usize) -> bool {
    let bangs = classify_shell_bangs(line);
    // GNU history_expand fails the whole line when any unescaped, unquoted,
    // non-comment event ends in a bare colon with no word designator, so the
    // paired fixture pins that coexistence line as fully literal.
    if bangs.iter().enumerate().any(|(at, bang)| {
        matches!(bang, Some(ShellBang::BareColon))
            || matches!(bang, Some(ShellBang::Plain) if bang_has_bare_colon(line, at))
    }) {
        return true;
    }
    // Bash's quote-aware pre-scan consumes a bang after any backslash run.
    // Its expansion pass uses backslash parity, so an even run becomes a
    // live candidate again when another ordinary event triggers that pass.
    if line.get(idx) == Some(&b'!') && idx > 0 && line[idx - 1] == b'\\' {
        let mut cursor = idx;
        while cursor > 0 && line[cursor - 1] == b'\\' {
            cursor -= 1;
        }
        let other_event = bangs
            .iter()
            .enumerate()
            .any(|(at, bang)| at.abs_diff(idx) > 1 && matches!(bang, Some(ShellBang::Plain)));
        if (idx - cursor) % 2 == 1 || !other_event {
            return true;
        }
    }
    match bangs.get(idx).copied().flatten() {
        Some(ShellBang::Dollar) | Some(ShellBang::BareColon) => true,
        Some(ShellBang::Bracket(close)) => {
            if bangs
                .iter()
                .any(|bang| matches!(bang, Some(ShellBang::Plain)))
            {
                line.get(idx + 1) == Some(&close)
            } else {
                true
            }
        }
        Some(ShellBang::Plain) => bang_has_bare_colon(line, idx),
        None => false,
    }
}

/// A colon right after the event designator with no word designator leaves
/// the leading bang literal in the measured shell prompt. The scan stops at
/// whitespace or a quote so it does not reach into later words.
fn bang_has_bare_colon(line: &[u8], idx: usize) -> bool {
    let mut cursor = idx + 1;
    while let Some(&byte) = line.get(cursor) {
        if byte.is_ascii_whitespace() || byte == b'\'' || byte == b'"' {
            return false;
        }
        if byte == b':' {
            return line
                .get(cursor + 1)
                .is_none_or(|next| next.is_ascii_whitespace());
        }
        cursor += 1;
    }
    false
}

fn classify_shell_bangs(line: &[u8]) -> Vec<Option<ShellBang>> {
    const NO_EXPAND: &[u8] = b" \t\n\r=;&|()<>";
    let mut bangs = vec![None; line.len()];
    let mut quote = None;
    const WORD_DELIMITERS: &[u8] = b" \t\n;&()|<>";
    let mut at_word_start = true;
    for (i, byte) in line.iter().enumerate() {
        let escaped = is_escaped(line, i);
        if quote == Some(b'\'') {
            if *byte == b'\'' {
                quote = None;
            }
            at_word_start = false;
            continue;
        }
        if quote.is_none() && *byte == b'#' && at_word_start {
            break;
        }
        match quote {
            Some(b'"') => {
                if *byte == b'"' && !escaped {
                    quote = None;
                }
            }
            None => {
                if (*byte == b'\'' || *byte == b'"') && !escaped {
                    quote = Some(*byte);
                }
            }
            _ => unreachable!(),
        }
        if *byte == b'!'
            && i + 1 < line.len()
            && !(i > 0 && line[i - 1] == b'\\')
            && !line.get(i + 1).is_some_and(|next| NO_EXPAND.contains(next))
            && !(quote == Some(b'"') && line.get(i + 1) == Some(&b'"'))
        {
            bangs[i] = Some(if idx_is_dollar_bang(line, i) {
                ShellBang::Dollar
            } else if let Some(close) = bracket_bang_delimiter(line, i) {
                ShellBang::Bracket(close)
            } else if line.get(i + 1) == Some(&b':')
                && line.get(i + 2).is_none_or(|next| b" \t\n\r".contains(next))
            {
                ShellBang::BareColon
            } else {
                ShellBang::Plain
            });
        }
        at_word_start = WORD_DELIMITERS.contains(byte);
    }
    bangs
}

fn idx_is_dollar_bang(line: &[u8], idx: usize) -> bool {
    idx > 1 && line[idx - 1] == b'$' && line[idx] == b'!'
}

fn bracket_bang_delimiter(line: &[u8], idx: usize) -> Option<u8> {
    if idx > 0 && line[idx - 1] == b'[' && line[idx + 1..].contains(&b']') {
        Some(b']')
    } else if idx > 1
        && line[idx - 1] == b'{'
        && line[idx - 2] == b'$'
        && line[idx + 1..].contains(&b'}')
    {
        Some(b'}')
    } else {
        None
    }
}

fn is_escaped(line: &[u8], idx: usize) -> bool {
    let mut start = idx;
    while start > 0 && line[start - 1] == b'\\' {
        start -= 1;
    }
    (idx - start) % 2 == 1
}

fn command_name_bytes(name: &std::ffi::OsStr) -> Option<Vec<u8>> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(name.as_bytes().to_vec())
    }
    #[cfg(not(unix))]
    {
        name.to_str().map(|name| name.as_bytes().to_vec())
    }
}

fn environment_variable_names(
    variables: impl Iterator<Item = (OsString, OsString)>,
) -> Vec<Vec<u8>> {
    let mut names = variables
        .filter_map(|(name, _)| name.into_string().ok().map(String::into_bytes))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn is_executable_file(path: &Path) -> bool {
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

fn main() -> ExitCode {
    let prompt = std::env::var("SUSHLINE_PROMPT").unwrap_or_default();
    let mut history = History::new();
    if let Ok(lines) = std::env::var("SUSHLINE_HISTORY") {
        for entry in lines.split('\n').filter(|entry| !entry.is_empty()) {
            history.push(entry);
        }
    }
    // The harness models a shell prompt after history initialization; raw
    // pushes preserve their measured cursor, so position this fixture at end.
    history.reset_cursor();

    let mut hooks = HarnessApplication;
    let mut terminal = Terminal::new();
    if std::env::var("SUSHLINE_RERAISE_SIGINT_DEFAULT").as_deref() == Ok("false") {
        terminal.set_reraise_sigint_default(false);
    }
    let mut line = Editor::new(Config::default(), terminal, history);
    if std::env::var("SUSHLINE_SIGINT_POLICY").as_deref() == Ok("interrupted") {
        line.set_sigint_policy(SigintPolicy::Interrupted);
    }
    if let Ok(inputrc) = std::env::var("SUSHLINE_INPUTRC")
        && let Err(err) = line.load_inputrc_str(&inputrc)
    {
        eprintln!("SUSHLINE_INPUTRC_ERROR:{err:?}");
        return ExitCode::from(1);
    }
    if let Some(path) = std::env::var_os("SUSHLINE_INPUTRC_FILE")
        && let Err(err) = line.load_inputrc_file(Path::new(&path))
    {
        eprintln!("SUSHLINE_INPUTRC_FILE_ERROR:{err:?}");
        return ExitCode::from(1);
    }
    // Oracle fixtures apply bind API lines after history is seeded, matching
    // `bind 'set ...'` run by the shell before its readline prompt.
    for setting in std::env::var("SUSHLINE_BIND_LINES")
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty())
    {
        if let Err(err) = line.bind_api().apply_line(setting) {
            eprintln!("SUSHLINE_BIND_ERROR:{err:?}");
            return ExitCode::from(1);
        }
    }

    let reads = std::env::var("SUSHLINE_READS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let continue_on_interrupt = std::env::var_os("SUSHLINE_CONTINUE_ON_INTERRUPT").is_some();
    // Opt in to a distinct readiness marker for each read. A redisplay repeats
    // the current prompt, so a shared prompt cannot frame batched PTY input.
    let numbered_prompts = std::env::var_os("SUSHLINE_NUMBERED_PROMPTS").is_some();
    let shell_escapes = std::env::var_os("SUSHLINE_PROMPT_SHELL_ESCAPES").is_some();
    if let Ok(prelude) = std::env::var("SUSHLINE_PRELUDE") {
        print!("{prelude}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
    for read_idx in 1..=reads {
        let read_prompt = if numbered_prompts {
            format!("{prompt}{read_idx}:")
        } else {
            prompt.clone()
        };
        // The prompt is drawn as it is given; a caller that wants the
        // shell's spelling of it asks for it.
        let read_prompt = if shell_escapes {
            Prompt::with_escapes(read_prompt, PromptEscapes::Shell)
        } else {
            Prompt::new(read_prompt)
        };
        match line.read_line(read_prompt, &mut hooks) {
            Ok(ReadlineResult::Line(bytes)) => {
                // Write before the marker so PTY callers can read a complete
                // history projection as soon as acceptance is observed.
                if let Some(path) = std::env::var_os("SUSHLINE_HISTORY_OUT")
                    && let Err(err) = line.history().write_file(Path::new(&path))
                {
                    eprintln!("SUSHLINE_HISTORY_OUT_ERROR:{err:?}");
                    return ExitCode::from(1);
                }
                if reads == 1 {
                    println!("SUSHLINE_ACCEPTED:{}", String::from_utf8_lossy(&bytes));
                } else {
                    println!(
                        "SUSHLINE_ACCEPTED_{read_idx}:{}",
                        String::from_utf8_lossy(&bytes)
                    );
                }
                if std::env::var_os("SUSHLINE_ACCEPTED_HEX").is_some() {
                    const HEX: &[u8; 16] = b"0123456789abcdef";
                    let mut hex = String::with_capacity(bytes.len() * 2);
                    for byte in bytes {
                        hex.push(HEX[(byte >> 4) as usize] as char);
                        hex.push(HEX[(byte & 0x0f) as usize] as char);
                    }
                    println!("SUSHLINE_ACCEPTED_HEX:{hex}");
                }
            }
            Ok(ReadlineResult::Interrupted) => {
                println!("SUSHLINE_INTERRUPTED");
                if !continue_on_interrupt {
                    return ExitCode::from(130);
                }
            }
            Ok(ReadlineResult::Eof) => {
                println!("SUSHLINE_EOF");
                return ExitCode::SUCCESS;
            }
            Err(err) => {
                eprintln!("SUSHLINE_ERROR:{err:?}");
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::environment_variable_names;
    use std::ffi::OsString;

    #[cfg(unix)]
    #[test]
    fn command_names_preserve_non_utf8_bytes() {
        use std::os::unix::ffi::OsStringExt;

        let name = OsString::from_vec(b"cmd-\xff".to_vec());
        assert_eq!(super::command_name_bytes(&name), Some(b"cmd-\xff".to_vec()));
    }

    #[cfg(unix)]
    #[test]
    fn invalid_environment_values_do_not_hide_names() {
        use std::os::unix::ffi::OsStringExt;

        let variables = [
            (
                OsString::from("SUSHLINE_VALID_NAME"),
                OsString::from_vec(vec![0xff]),
            ),
            (
                OsString::from_vec(vec![0xfe]),
                OsString::from("valid value"),
            ),
        ]
        .into_iter();
        assert_eq!(
            environment_variable_names(variables),
            vec![b"SUSHLINE_VALID_NAME".to_vec()]
        );
    }
}
