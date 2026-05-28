# Readline and History Compatibility

Sushline is a Rust line-editing and history foundation that aims to match GNU
Readline and History Library observable behavior where that behavior belongs
inside a line editor or history component.

Current compatibility target: GNU Readline 8.3 and GNU History Library
observable behavior, limited to Sushline's Rust editor and history APIs. The
oracle tests use GNU Bash 5.3 as a host for GNU Readline/History behavior, but
Bash language, shell runtime, and builtin compatibility are not Sushline goals.
Readline and History Library C ABI compatibility is explicitly out of scope.

## Status Legend

| Status | Meaning |
| --- | --- |
| Compatible | Matched against the baseline for the scoped behavior, with no known user-visible difference. |
| Implemented | Implemented, but not audited enough to claim GNU-compatible behavior for the whole row. |
| Implementation-specific | Implemented through Sushline's Rust model rather than GNU Readline/History internals; observable differences may exist outside tested cases. |
| Terminal-backed | Implemented through `TerminalIo` and terminfo-backed terminal behavior; exact escape bytes are terminal/backend dependent. |
| Mixed | The area contains multiple statuses; use the detailed rows below. |
| Known deviation | A known observable behavior differs from the baseline. |
| Hook-backed | The Readline command/mechanism is implemented; GNU-equivalent embedder-owned state or behavior is supplied through `Hooks`. |
| Not implemented | In the compatibility target, but missing or effectively inert. |
| Untested | Implemented or partially implemented, but not verified enough to classify. |

## Explicitly Out Of Scope

Readline and History Library C interfaces are outside this document. Sushline
is not intended for use from C; it exposes Rust crates and Rust APIs.

Shell language, shell builtins, and shell expansion semantics are compatibility
targets only where GNU Readline or the GNU History Library exposes the behavior
through line editing, inputrc, history, or completion behavior listed below.
Embedder-owned state, such as aliases, variables, jobs, shell expansion,
external editing, and `bind -x` command execution, must be supplied through
`Hooks`.

## Compatibility Boundaries

| Area | Baseline behavior | Sushline behavior | Status |
| --- | --- | --- | --- |
| Command-word parsing | Readline shell-word commands are typically backed by the embedding shell's lexer state. | Sushline's built-in command-word parser matches covered oracle cases, including quoted history words, command substitutions, process substitutions, and shell operator tokens. Embedders that need exact language lexer state can provide byte spans through `Hooks::tokenize_with_spans`; shell-word movement, kill, transpose, dynamic history completion, and yank-argument commands use those hook boundaries. | Hook-backed |
| Filename quoting and locale edges | Readline behavior depends on quote state, locale, byte/character handling, and shell integration. | Sushline has byte-oriented dequote/requote logic and matching options. Several common quoted cases, including unquoted filenames containing spaces and shell metacharacters, are covered; embedders can provide application quoting through `Hooks::quote_completion`, with `Hooks::quote` retained for legacy unquoted filename quoting. | Hook-backed |
| Embedder-owned completion categories | Command and variable completion may include aliases, reserved words, functions, builtins, variables, and executables owned by the embedding application. | Sushline provides executables and platform fallbacks; embedder-owned names are supplied through completion hooks such as `Hooks::command_names` and `Hooks::variable_names`. | Hook-backed |
| Application expansion and embedder state commands | Commands such as `shell-expand-line`, `spell-correct-word`, `display-shell-version`, `tty-status`, external editing, and `bind -x` use application-owned state. | Sushline dispatches those responsibilities through context-carrying hooks and applies returned edits/output in the Readline command flow. | Hook-backed |
| Region/display/terminal internals | Readline redisplay and active-region behavior are tied to terminal capabilities. | Sushline implements equivalent visible behavior through its Rust terminal/display model and `TerminalIo`. | Terminal-backed |

## High-Level Coverage

| Area | Status | Implemented surface | Boundary notes |
| --- | --- | --- | --- |
| Basic line editing | Compatible | Insert, delete, movement, undo, overwrite, quoted insert, transpose, case conversion, mark/region, keyboard macro replay, kill/yank. | Covered editor behavior matches the GNU oracle; exact shell-word classes can be supplied through hooks, and terminal escape bytes remain backend-dependent. |
| Emacs keymap, bindable names, and `bind` | Compatible | Default keymap, inputrc bindings, macros, numeric arguments, user-facing command names, `bind` output, `bind -x` storage/query/output. | Application command execution and embedder-owned state are represented through hooks. |
| vi mode | Compatible | Insert/command mode, common movement, operators, marks, redo, search, put/yank, vi completion bindings. | Covered oracle cases pass; external editing is hook-backed because policy belongs to the embedder. |
| Init file/inputrc | Compatible | `set`, key bindings, macros, `$if`, `$else`, `$endif`, `$include`, version/term/mode/application conditions, include depth checks. | Parser behavior and bind-visible output are covered; arbitrary shell/application state is intentionally not inferred by Sushline. |
| Completion | Mixed | Default completion, listing, insertion, menu completion, export-completions, display formatting, many filename options. | Embedder-owned categories and application-specific quoting are represented through hooks. |
| History navigation/search | Compatible | Previous/next, beginning/end, prefix search, substring search, incremental and non-incremental search state for covered editor behavior. | No known gap in the scoped Rust behavior. |
| History expansion | Compatible | Event designators including `!#`, word designators, modifiers including `:p` status, quick substitution, policy variables, quote state, inhibit predicates. | Alias expansion and exact application lexer state can be supplied through hooks; `Hooks::expand_history_with_status` preserves print-only status for Sush-owned expansion. |
| History file storage | Compatible | Read, range read, load, write, append, append-new, truncate, timestamp records, write/append timestamp control, default `~/.history` helpers, file locking on Unix. | No known gap in the scoped Rust file behavior; GNU C globals are out of scope. |

## User-Facing Readline Commands

The command names below come from the Readline User Manual bindable command
sections.

### Moving

| Command(s) | Status | Notes |
| --- | --- | --- |
| `beginning-of-line`, `end-of-line`, `forward-char`, `backward-char`, `forward-word`, `backward-word` | Compatible | Covered by direct editor and oracle tests. |
| `forward-byte`, `backward-byte` | Compatible | Byte-position movement, including insertion inside UTF-8 byte sequences, is covered by GNU oracle tests. |
| `shell-forward-word`, `shell-backward-word` | Hook-backed | Covered command-word cases match, including metacharacters and process substitution; `Hooks::tokenize_with_spans` supplies exact shell lexer boundaries when the embedder has them. |
| `previous-screen-line`, `next-screen-line` | Compatible | Covered by GNU oracle tests under a narrow wrapped terminal. |
| `clear-screen`, `clear-display`, `redraw-current-line` | Terminal-backed | Implemented through the terminal/display abstraction; GNU oracle covers line-buffer preservation, and exact escape bytes are terminal/backend dependent. |

### History Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `accept-line`, `previous-history`, `next-history`, `beginning-of-history`, `end-of-history` | Compatible | Implemented in the editor/history integration and covered by tests. |
| `reverse-search-history`, `forward-search-history`, `non-incremental-reverse-search-history`, `non-incremental-forward-search-history`, `non-incremental-forward-search-history-again`, `non-incremental-reverse-search-history-again` | Compatible | Search direction, repeat, case control, abort, and accept behavior are covered. |
| `history-search-backward`, `history-search-forward`, `history-substring-search-backward`, `history-substring-search-forward` | Compatible | Prefix and substring history search are implemented and tested. |
| `history-expand-line`, `magic-space` | Compatible | Core expansion is built in and honors `histchars` and history expansion policy variables. |
| `history-and-alias-expand-line`, `alias-expand-line` | Hook-backed | History expansion is built in; alias expansion uses `Hooks::expand_aliases` because aliases are owned by the embedding application. |
| `yank-nth-arg`, `yank-last-arg`, `insert-last-argument` | Hook-backed | Quoted, numeric, repeated, and shell-construct history arguments are covered by GNU oracle tests; `Hooks::tokenize` or `Hooks::tokenize_with_spans` can supply exact application lexer words. |
| `fetch-history` | Compatible | Numbered history fetch is covered by GNU oracle tests. |
| `operate-and-get-next` | Compatible | Multi-read prefill behavior, including numeric arguments, is covered by GNU oracle tests. |

### Text Editing

| Command(s) | Status | Notes |
| --- | --- | --- |
| `end-of-file`, `delete-char`, `backward-delete-char`, `forward-backward-delete-char` | Compatible | EOF on empty input and delete behavior are implemented and tested. |
| `quoted-insert`, `tab-insert`, `self-insert`, `bracketed-paste-begin` | Compatible | Literal insertion and bracketed paste are implemented and tested. |
| `transpose-chars`, `transpose-words` | Compatible | Covered by oracle tests. |
| `shell-transpose-words` | Hook-backed | Quoted and process-substitution command-word transposition is covered by GNU oracle tests; `Hooks::tokenize_with_spans` can supply exact application lexer boundaries. |
| `upcase-word`, `downcase-word`, `capitalize-word` | Hook-backed | Numeric, negative numeric, and punctuation word-boundary cases are covered by GNU oracle tests; custom word classes are supplied through `Hooks::editing_word_breaks`. |
| `overwrite-mode` | Compatible | Covered by editor tests. |

### Killing And Yanking

| Command(s) | Status | Notes |
| --- | --- | --- |
| `kill-line`, `backward-kill-line`, `unix-line-discard`, `kill-whole-line` | Compatible | Covered by editor/oracle tests, including direction and numeric cases. |
| `kill-word`, `backward-kill-word`, `unix-word-rubout`, `unix-filename-rubout` | Hook-backed | Positive and negative numeric `kill-word` plus representative word/filename rubout cases are covered by GNU oracle tests; custom word classes are supplied through `Hooks::editing_word_breaks`. |
| `shell-kill-word`, `shell-backward-kill-word` | Hook-backed | Shell metacharacter and process-substitution cases are covered by GNU oracle tests; `Hooks::tokenize_with_spans` can supply exact application lexer boundaries. |
| `delete-horizontal-space` | Compatible | Covered by oracle tests. |
| `kill-region`, `copy-region-as-kill`, `copy-backward-word`, `copy-forward-word` | Compatible | Region and copy-word operations are covered by GNU oracle tests for accepted-line behavior. |
| `yank` | Compatible | Covered by tests. |
| `yank-pop` | Compatible | The previously documented multiple-kill case now matches the GNU oracle. |

### Numeric Arguments And Macros

| Command(s) | Status | Notes |
| --- | --- | --- |
| `digit-argument`, `universal-argument` | Compatible | Implemented and covered by editor/oracle tests. |
| `start-kbd-macro`, `end-kbd-macro`, `call-last-kbd-macro` | Compatible | The previously documented consecutive self-insert replay difference now matches the GNU oracle. |
| `print-last-kbd-macro` | Compatible | Output for recorded macro bodies is covered by GNU oracle tests. |

### Completion Commands And Behavior

| Command/feature(s) | Status | Notes |
| --- | --- | --- |
| `complete`, `possible-completions`, `insert-completions`, `delete-char-or-list` | Compatible | Common-prefix insertion, display, insertion, repeated completion, ambiguous bells, and delete/list switching are covered by GNU oracle and focused tests. |
| `complete-command`, `possible-command-completions` | Hook-backed | Executables are completed locally; application-owned aliases/functions/builtins/reserved words are supplied through `Hooks::command_names`. |
| `complete-filename`, `possible-filename-completions` | Hook-backed | Common cases pass, including escaped and unescaped spaces, shell metacharacter quoting, hidden files, and ambiguous-candidate bell behavior; application-specific quoting can be supplied through `Hooks::quote_completion`, including quoted completion contexts. |
| `complete-hostname`, `possible-hostname-completions` | Hook-backed | Uses hooks plus platform sources where available. |
| `complete-username`, `possible-username-completions` | Hook-backed | Uses hooks plus platform sources where available. |
| `complete-variable`, `possible-variable-completions` | Hook-backed | Application-owned variables are supplied through `Hooks::variable_names`. |
| `menu-complete`, `menu-complete-backward`, `old-menu-complete` | Compatible | Menu behavior is covered by focused tests, including numeric arguments, backward cycling, wrapping, single-match behavior, and display-prefix handling. |
| `complete-into-braces` | Compatible | GNU brace layout, shared prefix handling, quoting, and append-space behavior are covered by oracle tests. |
| `dabbrev-expand`, `dynamic-complete-history` | Hook-backed | Expands from history words; quoted words and ambiguous common-prefix behavior are covered by GNU oracle tests, and `Hooks::tokenize` / `Hooks::tokenize_with_spans` can supply exact application lexer words. |
| `glob-complete-word`, `glob-expand-word`, `glob-list-expansions` | Hook-backed | Covered for representative glob cases; application globbing can be supplied through `Hooks::glob_expand_bytes` or the legacy UTF-8 `Hooks::glob_expand`. |
| `vi-complete` | Compatible | Default vi completion bindings `*`, `=`, and backslash are covered by GNU oracle tests, including command-mode cursor-under-character word bounds. |
| `bash-vi-complete` | Hook-backed | Dispatches through command completion; embedder-owned command categories are supplied through `Hooks::command_names`. |
| `export-completions` | Compatible | The Readline export-completions protocol is implemented and tested. |

### Miscellaneous Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `re-read-init-file`, `abort`, `do-lowercase-version`, `prefix-meta`, `undo`, `revert-line`, `set-mark`, `exchange-point-and-mark`, `skip-csi-sequence`, `dump-functions`, `dump-variables`, `dump-macros`, `execute-named-command`, `emacs-editing-mode`, `vi-editing-mode` | Compatible | Covered by direct editor tests and GNU oracle cases for observable line-editing behavior, including explicit inputrc file reload and dump command output. |
| `arrow-key-prefix` | Compatible | Accepted as a CSI-skip command and tested. |
| `display-shell-version`, `tty-status` | Hook-backed | Version/job/terminal status come from hooks; command output behavior is tested. |
| `shell-expand-line`, `spell-correct-word`, `edit-and-execute-command` | Hook-backed | Application expansion and spelling correction can use context-carrying hooks (`expand_application_line_with_context`, `spell_correct_with_context`); external editing comes from `Hooks::edit_and_execute`. |
| Application command bindings (`bind -x`) | Compatible | `BindApi` stores, queries, unbinds, and prints bindings in GNU-shaped forms; dispatch uses `Hooks::on_command` because command execution belongs to the embedder. |
| `tilde-expand` | Compatible | Current-word expansion, whitespace preservation, assignment-like words, and `~+`/`~-` behavior are covered by GNU oracle tests. |
| `character-search`, `character-search-backward` | Compatible | Covered by GNU oracle tests. |
| `insert-comment` | Compatible | Inserts/toggles `comment-begin` and accepts the line. |

### Vi Command Names

| Command(s) | Status | Notes |
| --- | --- | --- |
| `vi-append-eol`, `vi-append-mode`, `vi-insert-beg`, `vi-insertion-mode`, `vi-movement-mode`, `vi-editing-mode` | Compatible | Covered by vi/editor tests for the scoped behavior. |
| `vi-arg-digit`, `vi-search`, `vi-search-again`, `vi-char-search` | Compatible | Covered by vi/oracle tests for numeric and search behavior. |
| `vi-bWord`, `vi-backward-bigword`, `vi-back-to-indent`, `vi-first-print`, `vi-backward-word`, `vi-bword`, `vi-prev-word`, `vi-column`, `vi-eWord`, `vi-end-bigword`, `vi-end-word`, `vi-eword`, `vi-fWord`, `vi-forward-bigword`, `vi-forward-word`, `vi-fword`, `vi-next-word`, `vi-match` | Compatible | Covered by GNU oracle cases for punctuation words, bigwords, counts, operator-specific `w`/`W` behavior, first-print, column, and bracket matching. |
| `vi-change-case`, `vi-change-char`, `vi-replace`, `vi-change-to`, `vi-delete`, `vi-delete-to`, `vi-subst`, `vi-yank-to` | Compatible | Operator, change, replacement, and redo cases are covered, including the previously documented `r` redo difference. |
| `vi-overstrike`, `vi-overstrike-delete`, `vi-rubout`, `vi-put`, `vi-redo`, `vi-undo`, `vi-yank-pop` | Compatible | Covered by vi/editor tests for the scoped behavior. |
| `vi-fetch-history`, `vi-eof-maybe`, `vi-goto-mark`, `vi-set-mark`, `vi-tilde-expand`, `vi-unix-word-rubout`, `vi-yank-arg` | Compatible | History fetch, EOF behavior, tilde expansion, vi mark movement, default-unbound register key behavior, vi word rubout, and vi yank-arg numeric behavior are covered by GNU oracle tests. |
| `vi-edit-and-execute-command` | Hook-backed | External edit-and-execute behavior comes from `Hooks::edit_and_execute`; hook dispatch and acceptance are tested. |

## Readline Init File and Variables

### Init Syntax

| Feature | Status | Notes |
| --- | --- | --- |
| Blank lines and `#` comments | Compatible | Implemented. |
| `set variable value` | Compatible | Recognized variables are normalized; unknown variables are ignored. |
| Key bindings by key name or quoted key sequence | Compatible | Function bindings and macros are supported. |
| Escape sequences `\C-`, `\M-`, `\e`, `\\`, `\"`, `\'`, `\a`, `\b`, `\d`, `\f`, `\n`, `\r`, `\t`, `\v`, octal, hex | Compatible | Parsed through `KeySequence` and inputrc decoding. |
| `$if`, `$else`, `$endif` | Compatible | Mode, term, version, and application-name conditions are implemented; arbitrary variable comparisons are intentionally inactive to match GNU oracle behavior. |
| `$include` | Compatible | Implemented with relative include resolution and include-depth protection. |
| Unsupported `$` directives | Compatible | Unknown directives are ignored. |
| Unknown function names in key bindings | Compatible | Unknown function bindings in inputrc are ignored and later lines continue. |
| Init file load errors during editor construction | Compatible | `Editor::new` retains the initial load error for inspection, `Editor::try_new` returns it, and explicit reload/load APIs report errors. |

### Variables

| Variable(s) | Status | Notes |
| --- | --- | --- |
| `editing-mode`, `keymap` | Compatible | Selects current editing mode or target binding map. |
| `active-region-start-color`, `active-region-end-color`, `enable-active-region` | Terminal-backed | Region display exists, `bind -v` output is GNU-shaped, and rendering is handled through the display backend. |
| `bell-style`, `prefer-visible-bell` | Compatible | Audible/visible/none behavior is implemented through the terminal abstraction. |
| `bind-tty-special-chars` | Compatible | TTY special bindings are applied from terminal metadata exposed by the backend; EOF binding in vi mode is covered by GNU oracle tests. |
| `blink-matching-paren` | Terminal-backed | Implemented for self-insert through redisplay timing and terminal output. |
| `colored-completion-prefix`, `colored-stats`, `visible-stats` | Terminal-backed | Completion display support exists through the terminal display backend, including `LS_COLORS`-style rules used by Sushline. |
| `comment-begin` | Compatible | Used by `insert-comment`. |
| `completion-display-width`, `completion-prefix-display-length`, `completion-query-items`, `page-completions`, `print-completions-horizontally` | Compatible | Used by completion display and covered by focused tests. |
| `completion-ignore-case`, `completion-map-case`, `expand-tilde`, `mark-directories`, `mark-symlinked-directories`, `match-hidden-files` | Hook-backed | Used by filename completion and covered by GNU oracle cases; application-specific quoting can be supplied through `Hooks::quote_completion`. |
| `disable-completion`, `show-all-if-ambiguous`, `show-all-if-unmodified`, `skip-completed-text`, `menu-complete-display-prefix` | Compatible | Used by completion engine and covered by focused tests. |
| `convert-meta`, `input-meta`, `meta-flag`, `output-meta`, `enable-meta-key`, `force-meta-prefix` | Terminal-backed | Meta input/output behavior is mediated by Sushline's terminal/backend model and covered by variable tests. |
| `echo-control-characters`, `byte-oriented` | Terminal-backed | Affects Sushline display rendering and is covered by variable/display tests. |
| `enable-bracketed-paste`, `enable-keypad` | Compatible | Applied during terminal preparation/depreparation and tested. |
| `emacs-mode-string`, `vi-cmd-mode-string`, `vi-ins-mode-string`, `show-mode-in-prompt` | Compatible | Used by prompt rendering and tested. |
| `history-preserve-point`, `history-size`, `mark-modified-lines`, `revert-all-at-newline`, `search-ignore-case`, `horizontal-scroll-mode`, `isearch-terminators`, `keyseq-timeout` | Compatible | Implemented in editor/history/display/input paths and covered by focused tests. |
| `histchars`, `history-word-delimiters`, `history-search-delimiter-chars`, `history-no-expand-chars`, `history-quotes-inhibit-expansion` | Compatible | Parsed and used to build `HistoryExpansionPolicy` for editor history expansion. |

## History Expansion API

| Feature | Status | Notes |
| --- | --- | --- |
| Event designators `!!`, `!n`, `!-n`, `!string`, `!?string[?]`, `!$`, `!^`, `!:`, `!#` | Compatible | Implemented by `history::expand_history`; `!#` was added in this audit. |
| Quick substitution `^old^new^` | Compatible | Implemented for the previous history entry. |
| Word designators `0`, `n`, `^`, `$`, `%`, `x-y`, `*`, `x*`, `x-` | Hook-backed | Implemented over command words; quoted words, shell variable-like words, escaped spaces, command substitutions, process substitutions, shell operators, assignment-like array syntax, and common delimiters are covered by GNU oracle tests. Exact shell tokenization and status can be provided through `Hooks::expand_history_with_status`. |
| Modifiers `h`, `t`, `r`, `e`, `q`, `x`, `s/old/new/`, `&`, `g`, `a`, `G` | Compatible | Covered by GNU oracle tests for path modifiers, quoting modifiers, and substitution variants. |
| Modifier `p` | Compatible | `expand_history_with_status` preserves the print-only status. |
| Existing quote state | Compatible | `HistoryExpansionPolicy::quote_state` exposes quote state to the Rust API. |
| Inhibit-expansion callback | Compatible | A per-call inhibit predicate is available. |

## Editor History Expansion Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `history-expand-line`, `magic-space` | Compatible | Uses built-in history expansion and policy variables. |
| `history-and-alias-expand-line` | Hook-backed | History expansion is built in; alias expansion uses `Hooks::expand_aliases`. |
| `alias-expand-line` | Hook-backed | Aliases are embedder-owned and use `Hooks::expand_aliases`. |

## History Library Surface

The Rust `history::History` type covers many History Library operations through
Rust-owned state. This table maps History Library concepts to Rust-owned
Sushline APIs; it is not a C ABI or C API compatibility promise.

| History area | Rust equivalent | Status | Notes |
| --- | --- | --- | --- |
| State setup: `using_history`, `history_get_history_state`, `history_set_history_state` | `History::new`, `History::state`, `History::set_state` | Compatible | Rust-owned state is covered by tests; process-global C session state is out of scope. |
| List management: `add_history`, `add_history_time`, `remove_history`, `replace_history_entry`, `clear_history`, `stifle_history`, `unstifle_history`, `history_is_stifled` | `push`, `push_bytes`, `add_time`, `remove`, `replace`, `clear`, `stifle`, `unstifle`, `is_stifled` | Compatible | Rust-owned entry and metadata operations are covered by tests. |
| List information: `history_list`, `where_history`, `current_history`, `history_get`, `history_get_time`, `history_total_bytes` | `entries`, `where_history`, `current_history`, `get`, entry `timestamp`, `total_bytes` | Compatible | Rust-owned list, position, timestamp, and byte-count behavior is covered by tests. |
| Navigation: `history_set_pos`, `previous_history`, `next_history` | `set_pos`, `previous_history`, `next_history` | Compatible | Implemented on `History` and tested. |
| Search: `history_search`, `history_search_prefix`, `history_search_pos` | `history_search_bytes`, `history_search_prefix`, `history_search_pos` | Compatible | Byte/string search behavior is covered by tests; Rust return types are the in-scope API. |
| Files: `read_history`, `write_history`, `append_history`, `history_truncate_file`, default `~/.history` filename | `read_file`, `load_file`, `write_file`, `append_file`, `append_last_to_file`, `append_new_to_file`, `truncate_file`, `default_file_path`, default-file helpers | Compatible | File operations, timestamps, default path helpers, and Unix locking are covered by tests. |
| File range: `read_history_range` | `read_file_range`, `load_file_range` | Compatible | Range-reading APIs were added in this audit. |
| Expansion: `history_expand` | `expand_history`, `expand_history_with_status`, `Hooks::expand_history_with_status` | Compatible | Expanded text and `:p` print-only status are available and can be passed through the editor hook boundary. |
| Expansion helpers: `get_history_event`, `history_tokenize`, `history_arg_extract` | `get_history_event`, `history_tokenize`, `history_arg_extract`, `command_words` | Compatible | Rust helper APIs are exposed and covered by tests. |
| Variables: `history_base`, `history_length`, `history_max_entries` | `HistoryState` and methods | Compatible | Represented as Rust-owned state rather than process globals; `HistoryState` offset, length, stifle, and maximum-entry behavior is covered by tests. |
| Variables: `history_expansion_char`, `history_subst_char`, `history_comment_char`, `history_word_delimiters`, `history_search_delimiter_chars`, `history_no_expand_chars`, `history_quotes_inhibit_expansion` | `HistoryChars`, `HistoryExpansionPolicy` | Compatible | Available to expansion APIs and wired into editor history expansion. |
| Variable: `history_write_timestamps` | `write_file_with_timestamps`, `append_file_with_timestamps`, `append_new_to_file_with_timestamps` | Compatible | Timestamp writing can be enabled or suppressed per call. |
| Variable: `history_quoting_state` | `HistoryExpansionPolicy::quote_state` | Compatible | Existing quote state is exposed through the Rust policy object. |
| Variable: `history_inhibit_expansion_function` | `expand_history` inhibit predicate | Compatible | A per-call predicate is available rather than a process-global function pointer. |
