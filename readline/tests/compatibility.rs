//! Consistency checks for COMPATIBILITY.md.
//!
//! These checks read the document and the library's own bind listings. They
//! need no oracle host and no `oracle` feature, so they run in the CI job
//! that does not build the pinned Bash and Readline host.

use readline::{BindQuery, Config, Editor, History, Terminal};

fn editor() -> Editor<Terminal> {
    Editor::new(Config::default(), Terminal::default(), History::new())
}

#[test]
fn compatibility_document_covers_bind_commands_and_variables() {
    let compatibility =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../COMPATIBILITY.md"))
            .expect("read compatibility document");

    let mut list_line = editor();
    for command in list_line
        .bind_api()
        .print(BindQuery::ListFunctionNames)
        .lines()
    {
        assert!(
            compatibility.contains(&format!("`{command}`")),
            "COMPATIBILITY.md missing bind command {command}"
        );
    }

    let mut variable_line = editor();
    for line in variable_line
        .bind_api()
        .print(BindQuery::PrintVariablesReusable)
        .lines()
    {
        let Some(name) = line
            .strip_prefix("set ")
            .and_then(|rest| rest.split(' ').next())
        else {
            continue;
        };
        assert!(
            compatibility.contains(&format!("`{name}`")),
            "COMPATIBILITY.md missing bind variable {name}"
        );
    }
}

// COMPATIBILITY.md uses single-backtick spans. Pipes inside them or escaped
// by an odd backslash run are not table separators.
fn markdown_table_cell_count(line: &str) -> Option<usize> {
    let line = line.trim();
    if !line.starts_with('|') {
        return None;
    }

    let (mut cells, mut in_code, mut slashes, mut offset) = (1, false, 0, 0);
    for ch in line.chars() {
        if ch == '\\' {
            slashes += 1;
            offset += ch.len_utf8();
            continue;
        }
        let escaped = slashes % 2 == 1;
        slashes = 0;
        if ch == '`' && !escaped {
            in_code = !in_code;
        } else if ch == '|'
            && !in_code
            && !escaped
            && offset != 0
            && offset + ch.len_utf8() != line.len()
        {
            cells += 1;
        }
        offset += ch.len_utf8();
    }
    assert!(!in_code, "unclosed inline-code span in table row: {line}");
    Some(cells)
}

fn is_markdown_table_separator(line: &str) -> bool {
    line.trim().trim_matches('|').split('|').all(|cell| {
        let dashes = cell.trim().trim_start_matches(':').trim_end_matches(':');
        dashes.len() >= 3 && dashes.bytes().all(|byte| byte == b'-')
    })
}

fn assert_markdown_table_rows_match_headers(markdown: &str) {
    let lines = markdown.lines().collect::<Vec<_>>();
    let mut index = 0;
    while index + 1 < lines.len() {
        let Some(expected) = markdown_table_cell_count(lines[index]) else {
            index += 1;
            continue;
        };
        if !is_markdown_table_separator(lines[index + 1]) {
            index += 1;
            continue;
        }
        assert_eq!(markdown_table_cell_count(lines[index + 1]), Some(expected));
        index += 2;
        while index < lines.len() {
            let Some(actual) = markdown_table_cell_count(lines[index]) else {
                break;
            };
            assert_eq!(
                actual,
                expected,
                "wrong table cell count on line {}",
                index + 1
            );
            index += 1;
        }
    }
}

#[test]
fn markdown_table_scanner_ignores_code_and_escaped_pipes() {
    let table = r#"| Feature | Notes |
| --- | --- |
| extglob | `@(a|b)` |
| escaped | raw\|pipe |"#;
    assert_markdown_table_rows_match_headers(table);
}

#[test]
fn compatibility_document_tables_have_consistent_cell_counts() {
    let compatibility =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../COMPATIBILITY.md"))
            .expect("read compatibility document");
    assert_markdown_table_rows_match_headers(&compatibility);
}

#[test]
fn compatibility_document_classifies_editor_bind_and_history_coverage() {
    let compatibility =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../COMPATIBILITY.md"))
            .expect("read compatibility document");

    let documented_rows = compatibility
        .split("## Compatibility Boundaries")
        .nth(1)
        .expect("compatibility boundary section");
    let known_deviation_rows = documented_rows
        .lines()
        .filter(|line| line.starts_with('|') && line.contains("| Known deviation |"))
        .collect::<Vec<_>>();
    for deviation in [
        "| Signal install/catch/reraise |",
        "| `~user` and recursive `$include` |",
        "| `horizontal-scroll-mode` |",
    ] {
        assert!(
            known_deviation_rows
                .iter()
                .any(|row| row.starts_with(deviation)),
            "COMPATIBILITY.md must retain the {deviation} deviation"
        );
    }
    assert!(documented_rows.lines().any(|line| {
        line.starts_with("| Cell width and locale |")
            && line.contains("| Compatible |")
            && line.contains("Bash 5.3")
            && line.contains("PTY runtime")
    }));
    assert!(documented_rows.lines().any(|line| {
        line.starts_with("| Unwrapped ANSI in prompts |")
            && line.contains("| Implemented |")
            && line.contains("Bash 5.3")
            && line.contains("PTY runtime")
    }));
    assert!(documented_rows.lines().any(|line| {
        line.starts_with("| Completion redraw of invalid filename bytes |")
            && line.contains("| Compatible |")
    }));
    assert!(documented_rows.lines().any(|line| line.starts_with(
        "| `menu-complete`, `menu-complete-backward`, `old-menu-complete` | Compatible |"
    )));
    for forbidden in ["| Not implemented |", "| Untested |"] {
        let rows = documented_rows
            .lines()
            .filter(|line| line.starts_with('|') && line.contains(forbidden))
            .collect::<Vec<_>>();
        assert!(rows.is_empty(), "{forbidden} rows remain: {rows:?}");
    }

    // Rows whose first column names a long list of methods are locked by
    // their status alone, so the lock does not depend on the rest of the row.
    for (row_start, status) in [
        ("| List information: `history_list`", "Mixed"),
        ("| Search: `history_search`", "Mixed"),
        ("| Files: `read_history`", "Mixed"),
        ("| File range: `read_history_range`", "Mixed"),
    ] {
        let row = compatibility
            .lines()
            .find(|line| line.starts_with(row_start))
            .unwrap_or_else(|| panic!("COMPATIBILITY.md must keep the {row_start} row"));
        assert!(
            row.contains(&format!("| {status} |")),
            "the {row_start} row must be {status}: {row}"
        );
    }

    for area in [
        "| Basic line editing | Mixed |",
        "| Completion | Mixed |",
        "| Emacs keymap, bindable names, and `bind` | Mixed |",
        "| vi mode | Mixed |",
        "| Init file/inputrc | Mixed |",
        "| History navigation/search | Mixed |",
        "| History expansion | Mixed |",
        "| History file storage | Mixed |",
        "| `character-search`, `character-search-backward` | Compatible |",
        "| `vi-arg-digit`, `vi-search`, `vi-search-again`, `vi-char-search` | Compatible |",
    ] {
        assert!(
            compatibility.contains(area),
            "COMPATIBILITY.md must classify {area}"
        );
    }
}
