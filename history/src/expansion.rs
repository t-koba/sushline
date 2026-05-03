use super::History;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryChars {
    pub expansion: char,
    pub quick_substitution: char,
    pub comment: Option<char>,
}

impl HistoryChars {
    pub fn parse(value: &str) -> Self {
        let mut chars = value.chars();
        Self {
            expansion: chars.next().unwrap_or('!'),
            quick_substitution: chars.next().unwrap_or('^'),
            comment: chars.next(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryExpansionPolicy {
    pub word_delimiters: Vec<u8>,
    pub search_delimiters: Vec<u8>,
    pub no_expand_chars: Vec<u8>,
    pub quotes_inhibit_expansion: bool,
}

type HistoryEvent = (Vec<u8>, usize, Option<Vec<u8>>);
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryExpansionError {
    EventNotFound(String),
    BadWordSpecifier(String),
}

impl HistoryExpansionError {
    pub fn message(&self) -> String {
        match self {
            Self::EventNotFound(token) => format!("{token}: event not found"),
            Self::BadWordSpecifier(spec) => format!("{spec}: bad word specifier"),
        }
    }
}

pub fn expand_history(
    line: &[u8],
    history: &History,
    histchars: HistoryChars,
    policy: &HistoryExpansionPolicy,
    inhibit: impl Fn(usize) -> bool,
) -> Result<Vec<u8>, HistoryExpansionError> {
    let expansion = histchars.expansion as u8;
    let quick = histchars.quick_substitution as u8;
    if line.first() == Some(&quick)
        && let Some(old_end) = line[1..]
            .iter()
            .position(|byte| *byte == quick)
            .map(|idx| idx + 1)
        && let Some(last) = history.entries().last()
    {
        let new_end = line[old_end + 1..]
            .iter()
            .position(|byte| *byte == quick)
            .map(|idx| old_end + 1 + idx)
            .unwrap_or(line.len());
        return Ok(replace_once(
            &last.line_bytes,
            &line[1..old_end],
            &line[old_end + 1..new_end],
        ));
    }

    let mut out = Vec::with_capacity(line.len());
    let mut idx = 0;
    let mut quote = None;
    let mut at_word_start = true;
    let mut last_substitution: Option<(Vec<u8>, Vec<u8>)> = None;
    let mut last_event_search: Option<Vec<u8>> = None;
    while idx < line.len() {
        let byte = line[idx];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
        } else if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        }
        if histchars.comment == Some(byte as char) && at_word_start {
            out.extend_from_slice(&line[idx..]);
            break;
        }
        if byte != expansion
            || inhibit(idx)
            || line
                .get(idx + 1)
                .is_some_and(|next| policy.no_expand_chars.contains(next))
            || (policy.quotes_inhibit_expansion && quote == Some(b'\''))
        {
            out.push(byte);
            at_word_start = byte.is_ascii_whitespace() || policy.word_delimiters.contains(&byte);
            idx += 1;
            continue;
        }
        if histchars.comment == line.get(idx + 1).map(|byte| *byte as char) {
            idx += 2;
            let prefix = out.clone();
            out.extend_from_slice(&prefix);
            at_word_start = prefix.last().is_none_or(|byte| {
                byte.is_ascii_whitespace() || policy.word_delimiters.contains(byte)
            });
            continue;
        }
        let Some((mut event, mut next, matched_word)) = parse_history_event(
            line,
            idx + 1,
            history,
            histchars,
            policy,
            &mut last_event_search,
        )?
        else {
            out.push(byte);
            idx += 1;
            continue;
        };
        if line.get(next) == Some(&b':') {
            next += 1;
            let designator_start = next;
            let (selected, next_after_designator) =
                apply_history_word_designator(&event, matched_word.as_deref(), line, next, policy)?;
            event = selected;
            next = next_after_designator;
            if next == designator_start {
                let (modified, next_after_modifier) = apply_history_modifier(
                    &event,
                    line,
                    next,
                    &mut last_substitution,
                    last_event_search.as_deref(),
                    policy,
                )?;
                event = modified;
                next = next_after_modifier;
            }
        } else if line
            .get(next)
            .is_some_and(|byte| matches!(byte, b'^' | b'$' | b'*' | b'%' | b'-' | b'0'..=b'9'))
        {
            let (designator, next_after_designator) = read_history_designator(line, next);
            next = next_after_designator;
            event = if designator == b"%" {
                matched_word.unwrap_or_default()
            } else {
                select_history_words(&event, &designator, policy)?
            };
        }
        while line.get(next) == Some(&b':') {
            next += 1;
            let (modified, next_after_modifier) = apply_history_modifier(
                &event,
                line,
                next,
                &mut last_substitution,
                last_event_search.as_deref(),
                policy,
            )?;
            event = modified;
            next = next_after_modifier;
        }
        at_word_start = event
            .last()
            .is_none_or(|byte| byte.is_ascii_whitespace() || policy.word_delimiters.contains(byte));
        out.extend_from_slice(&event);
        idx = next;
    }
    Ok(out)
}

fn parse_history_event(
    line: &[u8],
    mut idx: usize,
    history: &History,
    histchars: HistoryChars,
    policy: &HistoryExpansionPolicy,
    last_event_search: &mut Option<Vec<u8>>,
) -> Result<Option<HistoryEvent>, HistoryExpansionError> {
    match line.get(idx).copied() {
        Some(byte) if byte == histchars.expansion as u8 => {
            idx += 1;
            history
                .entries()
                .last()
                .map(|entry| Some((entry.line_bytes.clone(), idx, None)))
                .ok_or_else(|| HistoryExpansionError::EventNotFound("!!".to_string()))
        }
        Some(b'$') => {
            idx += 1;
            let last_arg = history
                .entries()
                .last()
                .and_then(|entry| command_words(&entry.line_bytes, policy).last().cloned());
            last_arg
                .map(|event| Some((event, idx, None)))
                .ok_or_else(|| HistoryExpansionError::EventNotFound("!$".to_string()))
        }
        Some(b'^') => {
            idx += 1;
            let first_arg = history
                .entries()
                .last()
                .and_then(|entry| command_words(&entry.line_bytes, policy).get(1).cloned());
            first_arg
                .map(|event| Some((event, idx, None)))
                .ok_or_else(|| HistoryExpansionError::EventNotFound("!^".to_string()))
        }
        Some(b'-') | Some(b'0'..=b'9') => {
            let start = idx;
            if line.get(idx) == Some(&b'-') {
                idx += 1;
            }
            while line.get(idx).is_some_and(u8::is_ascii_digit) {
                idx += 1;
            }
            let spec = &line[start..idx];
            let spec_text = String::from_utf8_lossy(spec);
            let event = spec_text.parse::<isize>().ok().and_then(|event| {
                if event < 0 {
                    let pos = history.entries().len() as isize + event;
                    (pos >= 0)
                        .then(|| history.entries().get(pos as usize))
                        .flatten()
                        .map(|entry| entry.line_bytes.clone())
                } else {
                    history
                        .get_1_based_entry(event as usize)
                        .map(|entry| entry.line_bytes.clone())
                }
            });
            event
                .map(|event| Some((event, idx, None)))
                .ok_or_else(|| HistoryExpansionError::EventNotFound(format!("!{spec_text}")))
        }
        Some(b'?') => {
            idx += 1;
            let start = idx;
            while line
                .get(idx)
                .is_some_and(|byte| !is_history_search_delimiter(*byte, policy))
            {
                idx += 1;
            }
            let mut needle = line[start..idx].to_vec();
            if line.get(idx) == Some(&b'?') {
                idx += 1;
            }
            if needle.is_empty()
                && let Some(previous) = last_event_search.clone()
            {
                needle = previous;
            }
            if !needle.is_empty() {
                *last_event_search = Some(needle.clone());
            }
            history
                .entries()
                .iter()
                .rev()
                .find(|entry| find_bytes_local(&entry.line_bytes, &needle).is_some())
                .map(|entry| {
                    let matched_word = command_words(&entry.line_bytes, policy)
                        .into_iter()
                        .rev()
                        .find(|word| find_bytes_local(word, &needle).is_some());
                    Some((entry.line_bytes.clone(), idx, matched_word))
                })
                .ok_or_else(|| {
                    HistoryExpansionError::EventNotFound(format!(
                        "!?{}?",
                        String::from_utf8_lossy(&needle)
                    ))
                })
        }
        Some(c) if c.is_ascii_alphanumeric() || c == b'_' => {
            let start = idx;
            while line
                .get(idx)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            {
                idx += 1;
            }
            let prefix = &line[start..idx];
            history
                .entries()
                .iter()
                .rev()
                .find(|entry| entry.line_bytes.starts_with(prefix))
                .map(|entry| Some((entry.line_bytes.clone(), idx, None)))
                .ok_or_else(|| {
                    HistoryExpansionError::EventNotFound(format!(
                        "!{}",
                        String::from_utf8_lossy(prefix)
                    ))
                })
        }
        Some(b':') => history
            .entries()
            .last()
            .map(|entry| Some((entry.line_bytes.clone(), idx, None)))
            .ok_or_else(|| HistoryExpansionError::EventNotFound("!!".to_string())),
        _ => Ok(None),
    }
}

fn is_history_search_delimiter(byte: u8, policy: &HistoryExpansionPolicy) -> bool {
    byte == b'?' || policy.search_delimiters.contains(&byte)
}

fn apply_history_word_designator(
    line: &[u8],
    matched_word: Option<&[u8]>,
    input: &[u8],
    mut idx: usize,
    policy: &HistoryExpansionPolicy,
) -> Result<(Vec<u8>, usize), HistoryExpansionError> {
    if input.get(idx) == Some(&b'%') {
        return Ok((matched_word.unwrap_or_default().to_vec(), idx + 1));
    }
    let start = idx;
    while let Some(byte) = input.get(idx).copied() {
        if byte == b':' || byte.is_ascii_whitespace() {
            break;
        }
        if matches!(
            byte,
            b'h' | b't' | b'r' | b'e' | b'p' | b'q' | b'x' | b's' | b'g' | b'G'
        ) {
            break;
        }
        idx += 1;
    }
    if idx == start {
        Ok((line.to_vec(), idx))
    } else {
        Ok((select_history_words(line, &input[start..idx], policy)?, idx))
    }
}

fn select_history_words(
    line: &[u8],
    spec: &[u8],
    policy: &HistoryExpansionPolicy,
) -> Result<Vec<u8>, HistoryExpansionError> {
    let words = command_words(line, policy);
    if words.is_empty() {
        return Err(HistoryExpansionError::BadWordSpecifier(
            String::from_utf8_lossy(spec).into_owned(),
        ));
    }
    let last = words.len().saturating_sub(1);
    let idx_for = |token: &[u8]| -> Result<usize, HistoryExpansionError> {
        match token {
            b"^" => Ok(1.min(last)),
            b"$" => Ok(last),
            b"" => Ok(0),
            _ => {
                let text = String::from_utf8_lossy(token);
                let idx = text.parse::<usize>().map_err(|_| {
                    HistoryExpansionError::BadWordSpecifier(
                        String::from_utf8_lossy(spec).into_owned(),
                    )
                })?;
                if idx <= last {
                    Ok(idx)
                } else {
                    Err(HistoryExpansionError::BadWordSpecifier(
                        String::from_utf8_lossy(spec).into_owned(),
                    ))
                }
            }
        }
    };
    if spec == b"*" {
        return Ok(join_words(&words[1.min(words.len())..]));
    }
    if let Some(start) = spec.strip_suffix(b"*") {
        return Ok(join_words(&words[idx_for(start)?..]));
    }
    if let Some(dash) = spec.iter().position(|byte| *byte == b'-') {
        let start = idx_for(&spec[..dash])?;
        let end = if dash + 1 == spec.len() {
            last.saturating_sub(1)
        } else {
            idx_for(&spec[dash + 1..])?
        };
        if start > end {
            return Err(HistoryExpansionError::BadWordSpecifier(
                String::from_utf8_lossy(spec).into_owned(),
            ));
        }
        return Ok(join_words(&words[start..=end]));
    }
    Ok(words[idx_for(spec)?].clone())
}

fn read_history_designator(input: &[u8], mut idx: usize) -> (Vec<u8>, usize) {
    let start = idx;
    match input.get(idx).copied() {
        Some(b'^' | b'$' | b'*' | b'%') => (vec![input[idx]], idx + 1),
        Some(b'-') => {
            idx += 1;
            while input.get(idx).is_some_and(u8::is_ascii_digit) {
                idx += 1;
            }
            (input[start..idx].to_vec(), idx)
        }
        Some(b'0'..=b'9') => {
            while input.get(idx).is_some_and(u8::is_ascii_digit) {
                idx += 1;
            }
            (input[start..idx].to_vec(), idx)
        }
        _ => (Vec::new(), idx),
    }
}

fn apply_history_modifier(
    line: &[u8],
    input: &[u8],
    idx: usize,
    last_substitution: &mut Option<(Vec<u8>, Vec<u8>)>,
    last_event_search: Option<&[u8]>,
    policy: &HistoryExpansionPolicy,
) -> Result<(Vec<u8>, usize), HistoryExpansionError> {
    Ok(match input.get(idx).copied() {
        Some(b'h') => (history_head(line), idx + 1),
        Some(b't') => (history_tail(line), idx + 1),
        Some(b'r') => (history_root(line), idx + 1),
        Some(b'e') => (history_extension(line), idx + 1),
        Some(b'q') => (quote_history_word(line), idx + 1),
        Some(b'x') => (
            join_words(
                &command_words(line, policy)
                    .into_iter()
                    .map(|word| quote_history_word(&word))
                    .collect::<Vec<_>>(),
            ),
            idx + 1,
        ),
        Some(b'p') => (line.to_vec(), idx + 1),
        Some(b's') => {
            let (line, idx, substitution) = apply_substitution_modifier(
                line,
                input,
                idx + 1,
                false,
                last_substitution.as_ref(),
                last_event_search,
            );
            *last_substitution = Some(substitution);
            (line, idx)
        }
        Some(b'g') if input.get(idx + 1) == Some(&b's') => {
            let (line, idx, substitution) = apply_substitution_modifier(
                line,
                input,
                idx + 2,
                true,
                last_substitution.as_ref(),
                last_event_search,
            );
            *last_substitution = Some(substitution);
            (line, idx)
        }
        Some(b'G') if input.get(idx + 1) == Some(&b's') => {
            let (line, idx, substitution) = apply_substitution_modifier_each_word(
                line,
                input,
                idx + 2,
                last_substitution.as_ref(),
                last_event_search,
                policy,
            );
            *last_substitution = Some(substitution);
            (line, idx)
        }
        Some(b'a') if input.get(idx + 1) == Some(&b's') => {
            let (line, idx, substitution) = apply_substitution_modifier(
                line,
                input,
                idx + 2,
                true,
                last_substitution.as_ref(),
                last_event_search,
            );
            *last_substitution = Some(substitution);
            (line, idx)
        }
        Some(b'&') => {
            if let Some((old, new)) = last_substitution.as_ref() {
                (replace_once(line, old, new), idx + 1)
            } else {
                (line.to_vec(), idx)
            }
        }
        Some(b'g') if input.get(idx + 1) == Some(&b'&') => {
            if let Some((old, new)) = last_substitution.as_ref() {
                (replace_all(line, old, new), idx + 2)
            } else {
                (line.to_vec(), idx)
            }
        }
        Some(b'G') if input.get(idx + 1) == Some(&b'&') => {
            if let Some((old, new)) = last_substitution.as_ref() {
                (replace_each_word_once(line, old, new, policy), idx + 2)
            } else {
                (line.to_vec(), idx)
            }
        }
        Some(b'a') if input.get(idx + 1) == Some(&b'&') => {
            if let Some((old, new)) = last_substitution.as_ref() {
                (replace_all(line, old, new), idx + 2)
            } else {
                (line.to_vec(), idx)
            }
        }
        Some(ch) => {
            return Err(HistoryExpansionError::BadWordSpecifier(
                String::from_utf8_lossy(&[ch]).into_owned(),
            ));
        }
        None => (line.to_vec(), idx),
    })
}

fn apply_substitution_modifier(
    line: &[u8],
    input: &[u8],
    mut idx: usize,
    global: bool,
    last_substitution: Option<&(Vec<u8>, Vec<u8>)>,
    last_event_search: Option<&[u8]>,
) -> (Vec<u8>, usize, (Vec<u8>, Vec<u8>)) {
    let delimiter = input.get(idx).copied().unwrap_or(b'/');
    if input.get(idx).is_some() {
        idx += 1;
    }
    let (mut old, next_idx) = read_history_substitution_part(input, idx, delimiter, None);
    idx = next_idx;
    if old.is_empty()
        && let Some((previous_old, _)) = last_substitution
    {
        old = previous_old.clone();
    }
    if old.is_empty()
        && let Some(search) = last_event_search
    {
        old = search.to_vec();
    }
    let (new, next_idx) = read_history_substitution_part(input, idx, delimiter, Some(&old));
    idx = next_idx;
    let replaced = if global {
        replace_all(line, &old, &new)
    } else {
        replace_once(line, &old, &new)
    };
    (replaced, idx, (old, new))
}

fn apply_substitution_modifier_each_word(
    line: &[u8],
    input: &[u8],
    mut idx: usize,
    last_substitution: Option<&(Vec<u8>, Vec<u8>)>,
    last_event_search: Option<&[u8]>,
    policy: &HistoryExpansionPolicy,
) -> (Vec<u8>, usize, (Vec<u8>, Vec<u8>)) {
    let delimiter = input.get(idx).copied().unwrap_or(b'/');
    if input.get(idx).is_some() {
        idx += 1;
    }
    let (mut old, next_idx) = read_history_substitution_part(input, idx, delimiter, None);
    idx = next_idx;
    if old.is_empty()
        && let Some((previous_old, _)) = last_substitution
    {
        old = previous_old.clone();
    }
    if old.is_empty()
        && let Some(search) = last_event_search
    {
        old = search.to_vec();
    }
    let (new, next_idx) = read_history_substitution_part(input, idx, delimiter, Some(&old));
    idx = next_idx;
    (
        replace_each_word_once(line, &old, &new, policy),
        idx,
        (old, new),
    )
}

fn read_history_substitution_part(
    input: &[u8],
    mut idx: usize,
    delimiter: u8,
    amp_replacement: Option<&[u8]>,
) -> (Vec<u8>, usize) {
    let mut part = Vec::new();
    let mut escaped = false;
    while let Some(byte) = input.get(idx).copied() {
        idx += 1;
        if escaped {
            part.push(byte);
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        if byte == delimiter {
            break;
        }
        if byte == b'&'
            && let Some(replacement) = amp_replacement
        {
            part.extend_from_slice(replacement);
        } else {
            part.push(byte);
        }
    }
    if escaped {
        part.push(b'\\');
    }
    (part, idx)
}

pub fn command_words(line: &[u8], policy: &HistoryExpansionPolicy) -> Vec<Vec<u8>> {
    command_word_spans(line, policy)
        .into_iter()
        .map(|(start, end)| dequote_history_word(&line[start..end]))
        .collect()
}

fn command_word_spans(line: &[u8], policy: &HistoryExpansionPolicy) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut quote = None;
    let mut escape = false;
    let mut word_start = None;
    let mut idx = 0;
    while let Some(byte) = line.get(idx).copied() {
        if escape {
            word_start.get_or_insert(idx);
            escape = false;
            idx += 1;
            continue;
        }
        if byte == b'\\' && quote != Some(b'\'') {
            word_start.get_or_insert(idx);
            escape = true;
            idx += 1;
            continue;
        }
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            word_start.get_or_insert(idx);
            idx += 1;
            continue;
        }
        if byte == b'$' && matches!(line.get(idx + 1), Some(b'\'' | b'"')) {
            word_start.get_or_insert(idx);
            quote = line.get(idx + 1).copied();
            idx += 2;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            word_start.get_or_insert(idx);
            quote = Some(byte);
            idx += 1;
            continue;
        }
        if byte == b'#' && word_start.is_none() {
            break;
        }
        if byte.is_ascii_whitespace() || policy.word_delimiters.contains(&byte) {
            if let Some(start) = word_start.take() {
                spans.push((start, idx));
            }
        } else {
            word_start.get_or_insert(idx);
        }
        idx += 1;
    }
    if let Some(start) = word_start {
        spans.push((start, line.len()));
    }
    spans
}

fn dequote_history_word(word: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(word.len());
    let mut idx = 0;
    let mut quote = None;
    let mut escape = false;
    while let Some(byte) = word.get(idx).copied() {
        idx += 1;
        if escape {
            if byte != b'\n' {
                out.push(byte);
            }
            escape = false;
            continue;
        }
        if byte == b'\\' && quote != Some(b'\'') {
            escape = true;
            continue;
        }
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            } else {
                out.push(byte);
            }
            continue;
        }
        if byte == b'$' && matches!(word.get(idx), Some(b'\'' | b'"')) {
            quote = word.get(idx).copied();
            idx += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else {
            out.push(byte);
        }
    }
    if escape {
        out.push(b'\\');
    }
    out
}

fn join_words(words: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for (idx, word) in words.iter().enumerate() {
        if idx > 0 {
            out.push(b' ');
        }
        out.extend_from_slice(word);
    }
    out
}

fn replace_once(line: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    if old.is_empty() {
        return line.to_vec();
    }
    let Some(pos) = find_bytes_local(line, old) else {
        return line.to_vec();
    };
    let mut out = Vec::with_capacity(line.len() - old.len() + new.len());
    out.extend_from_slice(&line[..pos]);
    out.extend_from_slice(new);
    out.extend_from_slice(&line[pos + old.len()..]);
    out
}

fn find_bytes_local(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn replace_all(line: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    if old.is_empty() {
        return line.to_vec();
    }
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(pos) = find_bytes_local(rest, old) {
        out.extend_from_slice(&rest[..pos]);
        out.extend_from_slice(new);
        rest = &rest[pos + old.len()..];
    }
    out.extend_from_slice(rest);
    out
}

fn replace_each_word_once(
    line: &[u8],
    old: &[u8],
    new: &[u8],
    policy: &HistoryExpansionPolicy,
) -> Vec<u8> {
    if old.is_empty() {
        return line.to_vec();
    }
    let spans = command_word_spans(line, policy);
    if spans.is_empty() {
        return line.to_vec();
    }
    let mut out = Vec::with_capacity(line.len());
    let mut cursor = 0;
    for (start, end) in spans {
        out.extend_from_slice(&line[cursor..start]);
        out.extend_from_slice(&replace_once(&line[start..end], old, new));
        cursor = end;
    }
    out.extend_from_slice(&line[cursor..]);
    out
}

fn history_head(value: &[u8]) -> Vec<u8> {
    let trimmed = value.strip_suffix(b"/").unwrap_or(value);
    trimmed
        .iter()
        .rposition(|byte| *byte == b'/')
        .map(|idx| {
            if idx == 0 {
                b"/".to_vec()
            } else {
                trimmed[..idx].to_vec()
            }
        })
        .unwrap_or_default()
}

fn history_tail(value: &[u8]) -> Vec<u8> {
    let trimmed = value.strip_suffix(b"/").unwrap_or(value);
    trimmed
        .iter()
        .rposition(|byte| *byte == b'/')
        .map(|idx| trimmed[idx + 1..].to_vec())
        .unwrap_or_else(|| trimmed.to_vec())
}

fn history_root(value: &[u8]) -> Vec<u8> {
    let tail_start = value
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |idx| idx + 1);
    let Some(dot) = value[tail_start..]
        .iter()
        .rposition(|byte| *byte == b'.')
        .map(|idx| tail_start + idx)
    else {
        return value.to_vec();
    };
    value[..dot].to_vec()
}

fn history_extension(value: &[u8]) -> Vec<u8> {
    let tail_start = value
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |idx| idx + 1);
    value[tail_start..]
        .iter()
        .rposition(|byte| *byte == b'.')
        .map(|idx| value[tail_start + idx..].to_vec())
        .unwrap_or_default()
}

fn quote_history_word(value: &[u8]) -> Vec<u8> {
    if value.is_empty() {
        return b"''".to_vec();
    }
    if value.iter().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'/' | b'.' | b':')
    }) {
        return value.to_vec();
    }
    let mut out = Vec::from(&b"'"[..]);
    for byte in value {
        if *byte == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(*byte);
        }
    }
    out.push(b'\'');
    out
}

impl Default for HistoryExpansionPolicy {
    fn default() -> Self {
        Self {
            word_delimiters: b" \t\n\"\\'`@$><=;|&{(".to_vec(),
            search_delimiters: b" \t\n:;".to_vec(),
            no_expand_chars: b" \t\n\r=".to_vec(),
            quotes_inhibit_expansion: false,
        }
    }
}
