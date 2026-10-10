# Readline and History Compatibility

Sushline is a Rust line-editing and history foundation that aims to match GNU
Readline and History Library observable behavior where that behavior belongs
inside a line editor or history component.

Current compatibility target: GNU Readline 8.3 at patch level 0 and GNU History Library
observable behavior, limited to Sushline's Rust editor and history APIs. The
oracle tests use GNU Bash 5.3 (bundled Readline 8.3, no official patches applied;
sandbox host reports `5.3.0(1)-release`) as a host for GNU Readline/History behavior, but
Bash language, shell runtime, and builtin compatibility are not Sushline goals.
Readline and History Library C ABI compatibility is explicitly out of scope.

## Status Legend

| Status | Meaning |
| --- | --- |
| Compatible | Matched against the baseline for the scoped behavior, with no known user-visible difference. |
| Implemented | Implemented, but not audited enough to claim GNU-compatible behavior for the whole row. |
| Implementation-specific | Implemented through Sushline's Rust model rather than GNU Readline/History internals; observable differences may exist outside tested cases. |
| Terminal-backed | Implemented through `TerminalIo`, terminfo-backed capabilities where available, and fixed ANSI helpers for the built-in terminal; exact bytes may still depend on terminal/backend capabilities. |
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
| Command-word parsing | Readline shell-word commands are typically backed by the embedding shell's lexer state. | Sushline's built-in command-word parser matches covered oracle cases, including quoted history words, command substitutions, process substitutions, and shell operator tokens. Embedders that need exact language lexer state can provide byte spans through `Hooks::shell_word_spans`; shell-word movement, kill, and transpose use spans, while dynamic history completion and yank-argument commands use `Hooks::shell_words`. | Hook-backed |
| Filename quoting and locale edges | Readline behavior depends on quote state, locale, byte/character handling, and shell integration. | Sushline has byte-oriented dequote/requote logic and matching options. Several common quoted cases, including unquoted filenames containing spaces and shell metacharacters, are covered; embedders can provide application quoting through `Hooks::quote_completion`. | Hook-backed |
| Embedder-owned completion categories | Command and variable completion may include aliases, reserved words, functions, builtins, variables, and executables owned by the embedding application. | Sushline provides executables and platform fallbacks; embedder-owned names are supplied through completion hooks such as `Hooks::command_names` and `Hooks::variable_names`. | Hook-backed |
| Application expansion and embedder state commands | Commands such as `shell-expand-line`, `spell-correct-word`, `display-shell-version`, `tty-status`, external editing, and `bind -x` use application-owned state. | Sushline dispatches those responsibilities through context-carrying hooks and applies returned edits/output in the Readline command flow. | Hook-backed |
| Cell width and locale | Readline measures cell width with locale-sensitive `wcwidth`; C/single-byte locales fall back to non-printable handling for multibyte characters. | Sushline measures static UAX#11 widths with no locale input; multibyte characters keep their UTF-8 widths even in C/single-byte locales, and ambiguous-width characters use non-CJK widths. | Implementation-specific |
| Unwrapped ANSI in prompts | GNU counts unwrapped escape sequences as visible width (shell miswraps without `\[` `\]` markers). | Sushline measures unwrapped `ESC [` / `ESC ]` sequences as zero width, matching wrapped-marker prompts; prompt cursor columns and redisplay rows agree. | Implementation-specific |
| Region/display/terminal internals | Readline redisplay and active-region behavior are tied to terminal capabilities. | Sushline implements equivalent visible behavior through its Rust terminal/display model and `TerminalIo`. The built-in terminal backend is Unix-oriented; non-Unix builds compile but return `Unsupported` for live terminal operations unless an embedder supplies another `TerminalIo`. | Terminal-backed |
| Event / input-available hooks | GNU exposes `rl_event_hook` idle callbacks, `rl_input_available_hook` first-shot input polling, and `rl_set_keyboard_input_timeout` idle timeouts. | Sushline performs no implicit idle callback; idle and input-available policy belong to the embedder through a custom `TerminalIo::read_event` timeout/poll plus per-iteration `Hooks::check_signals`. The `keyseq-timeout` value drives the `read_event` timeout. | Terminal-backed |
| Signal install/catch/reraise | GNU gates install on `rl_catch_signals`/`rl_catch_sigwinch`, preserves `SIG_IGN`, chains the saved `SIGWINCH` handler, and reraise path covers every caught signal including `SIGINT` (cleanup→raise→reset). | Sushline installs unconditionally on raw-mode entry; `SIG_IGN` is preserved; the saved `SIGWINCH` handler is chained in-handler; other signals arrive as `TerminalEvent::Signal`/`Hooks::check_signals` with restore→raise→re-enter for non-`INT`; `SIGINT` returns `Interrupted` with `^C` echo and no reraise (known deviation); `rl_catch_signals`/`rl_catch_sigwinch` opt-outs are not implemented. | Implementation-specific |
| Undo insert grouping | GNU `rl_insert_text` concatenates only single-byte inserts into the prior insert entry up to 20 chars; longer runs and multi-byte inserts open new entries. | Sushline matches the single-byte-insert-up-to-20-bytes rule for self-insert, literal, and tab inserts (one ASCII byte with count 1 extends the pending entry while it holds fewer than 20 bytes; anything wider commits first), verified against the patch 0 baseline (Bash 5.3 PTY oracle for 20/21/25-char runs, 20a+multibyte via its own entry, numeric-arg multi-insert, and short-run `aé` plus undo leaving `a`). A complete multibyte char inserts as one unit opening its own entry; torn reads buffer the incomplete lead across events like a keymap prefix, so batched and split input agree. | Compatible |

## High-Level Coverage

| Area | Status | Implemented surface | Boundary notes |
| --- | --- | --- | --- |
| Basic line editing | Compatible | Insert, delete, movement, undo, overwrite, quoted insert, transpose, case conversion, mark/region, keyboard macro replay, kill/yank. | Covered editor behavior matches the GNU oracle; exact shell-word classes can be supplied through hooks, and terminal escape bytes remain backend-dependent. |
| Emacs keymap, bindable names, and `bind` | Compatible | Default keymap, inputrc bindings, macros, numeric arguments, user-facing command names, `bind` output, `bind -x` storage/query/output. | Application command execution and embedder-owned state are represented through hooks. |
| vi mode | Compatible | Insert/command mode, common movement, operators, marks, redo, search, put/yank, vi completion bindings. | Covered oracle cases pass; external editing is hook-backed because policy belongs to the embedder. |
| Init file/inputrc | Compatible | `set`, key bindings, macros, `$if`, `$else`, `$endif`, `$include`, version/term/mode/application conditions, include depth checks. | Parser behavior and bind-visible output are covered; arbitrary shell/application state is intentionally not inferred by Sushline. |
| Completion | Mixed | Default completion, listing, insertion, menu completion, export-completions, display formatting, many filename options. | Embedder-owned categories and application-specific quoting are represented through hooks. |
| History navigation/search | Compatible | Previous/next, beginning/end, prefix search, substring search, incremental and non-incremental search state for covered editor behavior. | No known gap in the scoped Rust behavior. |
| History expansion | Compatible | Event designators including `!#`, word designators, modifiers including `:p` status, quick substitution, policy variables, quote state, inhibit predicates. | Alias expansion and exact application lexer state can be supplied through hooks; `Hooks::expand_history` returns `HistoryExpansion` and preserves print-only status for Sush-owned expansion. |
| History file storage | Compatible | Read, range read, load, write, append, append-new, truncate, timestamp records, timestamp-delimited multiline reads with blank preservation, write/append timestamp control, default `~/.history` helpers. | Timestamp lines delimit entries so physical lines up to the next timestamp read as one entry with embedded blanks; files without timestamps stay one line per entry with blanks dropped. Concurrent-writer merging is embedder policy (last-writer-wins, no locking), matching GNU; GNU C globals are out of scope. Full writes use atomic rename for regular files and in-place truncate for non-regular destinations, so symlink-to-special paths (for example `history -> /dev/null`) discard on write/append with the link preserved. Full writes reset the mode to owner-only `0600` on Unix and all writes are close-only with no fsync, matching GNU; non-regular truncate discards in place where GNU errors (symlink preserved in both). Decided policy (against the Readline 8.3 patch 0 baseline): range FROM/TO and truncate count timestamp-joined Rust entries (logical commands), not physical file lines; timestamp-delimited lines join with embedded blanks preserved, matching the HISTTIMEFORMAT-set host path; files without timestamps stay one line per entry with blanks dropped. |

## User-Facing Readline Commands

The command names below come from the Readline User Manual bindable command
sections.

### Moving

| Command(s) | Status | Notes |
| --- | --- | --- |
| `beginning-of-line`, `end-of-line`, `forward-char`, `backward-char`, `forward-word`, `backward-word` | Compatible | Covered by direct editor and oracle tests. |
| `forward-byte`, `backward-byte` | Compatible | Byte-position movement, including insertion inside UTF-8 byte sequences, is covered by GNU oracle tests. |
| `shell-forward-word`, `shell-backward-word` | Hook-backed | Covered command-word cases match, including metacharacters and process substitution; `Hooks::shell_word_spans` supplies exact shell lexer boundaries when the embedder has them. |
| `previous-screen-line`, `next-screen-line` | Compatible | Covered by GNU oracle tests under a narrow wrapped terminal. |
| `clear-screen`, `clear-display`, `redraw-current-line` | Terminal-backed | Implemented through the terminal/display abstraction; GNU oracle covers line-buffer preservation, and exact escape bytes are terminal/backend dependent. |

### History Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `accept-line`, `previous-history`, `next-history`, `beginning-of-history`, `end-of-history` | Compatible | Implemented in the editor/history integration and covered by tests. |
| `reverse-search-history`, `forward-search-history`, `non-incremental-reverse-search-history`, `non-incremental-forward-search-history`, `non-incremental-forward-search-history-again`, `non-incremental-reverse-search-history-again` | Compatible | Search direction, repeat, case control, abort, accept, execute bells, and search-string quoting are covered (incremental honors any `quoted-insert` binding, non-incremental quotes `^V`/`^Q`). Non-incremental prompts replace the line while querying (`:` for emacs, `/`/`?` for vi `vi-search`); Enter executes and a second Enter accepts. |
| `history-search-backward`, `history-search-forward`, `history-substring-search-backward`, `history-substring-search-forward` | Compatible | Prefix and substring history search are implemented and tested. |
| `history-expand-line`, `magic-space` | Compatible | Core expansion is built in and honors `histchars` and history expansion policy variables. |
| `history-and-alias-expand-line`, `alias-expand-line` | Hook-backed | History expansion is built in; alias expansion uses `Hooks::expand_aliases` because aliases are owned by the embedding application. |
| `yank-nth-arg`, `yank-last-arg`, `insert-last-argument` | Hook-backed | Quoted, numeric, repeated, and shell-construct history arguments are covered by GNU oracle tests; `Hooks::shell_words` can supply exact application lexer words. |
| `fetch-history` | Compatible | Numbered history fetch is covered by GNU oracle tests. |
| `operate-and-get-next` | Compatible | Multi-read prefill behavior, including numeric arguments, is covered by GNU oracle tests. |

### Text Editing

| Command(s) | Status | Notes |
| --- | --- | --- |
| `end-of-file`, `delete-char`, `backward-delete-char`, `forward-backward-delete-char` | Compatible | EOF on empty input and delete behavior are implemented and tested. |
| `quoted-insert`, `tab-insert`, `self-insert`, `bracketed-paste-begin` | Compatible | Literal insertion and bracketed paste are implemented and tested. An embedded TAB expands to spaces up to the next multiple-of-8 stop like GNU `DISPLAY_TABS`, covered by display/oracle tests. |
| `transpose-chars`, `transpose-words` | Compatible | Covered by oracle tests. |
| `shell-transpose-words` | Hook-backed | Quoted and process-substitution command-word transposition is covered by GNU oracle tests; `Hooks::shell_word_spans` can supply exact application lexer boundaries. |
| `upcase-word`, `downcase-word`, `capitalize-word` | Hook-backed | Numeric, negative numeric, and punctuation word-boundary cases are covered by GNU oracle tests; custom word classes are supplied through `Hooks::editing_word_breaks`. |
| `overwrite-mode` | Compatible | Covered by editor tests. |

### Killing And Yanking

| Command(s) | Status | Notes |
| --- | --- | --- |
| `kill-line`, `backward-kill-line`, `unix-line-discard`, `kill-whole-line` | Compatible | Covered by editor/oracle tests, including direction and numeric cases. |
| `kill-word`, `backward-kill-word`, `unix-word-rubout`, `unix-filename-rubout` | Hook-backed | Positive and negative numeric `kill-word` plus representative word/filename rubout cases are covered by GNU oracle tests; custom word classes are supplied through `Hooks::editing_word_breaks`. |
| `shell-kill-word`, `shell-backward-kill-word` | Hook-backed | Shell metacharacter and process-substitution cases are covered by GNU oracle tests; `Hooks::shell_word_spans` can supply exact application lexer boundaries. |
| `delete-horizontal-space` | Compatible | Covered by oracle tests. |
| `kill-region`, `copy-region-as-kill`, `copy-backward-word`, `copy-forward-word` | Compatible | Region and copy-word operations are covered by GNU oracle tests for accepted-line behavior. |
| `yank` | Compatible | Covered by tests. |
| `yank-pop` | Compatible | Multiple-kill cycling behavior is covered by GNU oracle tests. |

### Numeric Arguments And Macros

| Command(s) | Status | Notes |
| --- | --- | --- |
| `digit-argument`, `universal-argument` | Compatible | Implemented and covered by editor/oracle tests. |
| `start-kbd-macro`, `end-kbd-macro`, `call-last-kbd-macro` | Compatible | Consecutive self-insert replay behavior is covered by GNU oracle tests. |
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
| `dabbrev-expand`, `dynamic-complete-history` | Hook-backed | Expands from history words; quoted words and ambiguous common-prefix behavior are covered by GNU oracle tests, and `Hooks::shell_words` can supply exact application lexer words. |
| `glob-complete-word`, `glob-expand-word`, `glob-list-expansions` | Hook-backed | Covered for representative glob cases; application globbing can be supplied through byte-oriented `Hooks::glob_expand`. |
| `vi-complete` | Compatible | Default vi completion bindings `*`, `=`, and backslash are covered by GNU oracle tests, including command-mode cursor-under-character word bounds. |
| `bash-vi-complete` | Hook-backed | Dispatches through command completion; embedder-owned command categories are supplied through `Hooks::command_names`. |
| `export-completions` | Compatible | The Readline export-completions protocol is implemented and tested. |

### Miscellaneous Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `re-read-init-file`, `abort`, `do-lowercase-version`, `prefix-meta`, `undo`, `revert-line`, `set-mark`, `exchange-point-and-mark`, `skip-csi-sequence`, `dump-functions`, `dump-variables`, `dump-macros`, `execute-named-command`, `emacs-editing-mode`, `vi-editing-mode` | Compatible | Covered by direct editor tests and GNU oracle cases for observable line-editing behavior, including explicit inputrc file reload and dump command output. `do-lowercase-version` on a key with no upper/lower difference rings the bell instead of recursing. |
| `arrow-key-prefix` | Compatible | Accepted as a CSI-skip command and tested. |
| `display-shell-version`, `tty-status` | Hook-backed | Version/job/terminal status come from hooks; command output behavior is tested. |
| `shell-expand-line`, `spell-correct-word`, `edit-and-execute-command` | Hook-backed | Application expansion and spelling correction use context-carrying hooks (`expand_line`, `spell_correct`); external editing comes from `Hooks::edit_and_execute`. |
| Application command bindings (`bind -x`) | Compatible | `BindApi` stores, queries, unbinds, and prints bindings in GNU-shaped forms; dispatch uses `Hooks::on_command` because command execution belongs to the embedder. |
| `tilde-expand` | Compatible | Current-word expansion, whitespace preservation, assignment-like words, and `~+`/`~-` behavior are covered by GNU oracle tests. |
| `character-search`, `character-search-backward` | Compatible | Covered by GNU oracle tests. |
| `insert-comment` | Compatible | Inserts/toggles `comment-begin` and accepts the line. |

### Vi Command Names

| Command(s) | Status | Notes |
| --- | --- | --- |
| `vi-append-eol`, `vi-append-mode`, `vi-insert-beg`, `vi-insertion-mode`, `vi-movement-mode`, `vi-editing-mode` | Compatible | Covered by vi/editor tests for the scoped behavior. |
| `vi-arg-digit`, `vi-search`, `vi-search-again`, `vi-char-search` | Compatible | `vi-search` (`/` backward, `?` forward) is non-incremental like GNU: `/query`/`?query` prompts, cursor-exclusive backward execute with a single bell on empty/failed execute, and `n`/`N` repeat. Covered by vi/oracle tests. |
| `vi-bWord`, `vi-backward-bigword`, `vi-back-to-indent`, `vi-first-print`, `vi-backward-word`, `vi-bword`, `vi-prev-word`, `vi-column`, `vi-eWord`, `vi-end-bigword`, `vi-end-word`, `vi-eword`, `vi-fWord`, `vi-forward-bigword`, `vi-forward-word`, `vi-fword`, `vi-next-word`, `vi-match` | Compatible | Covered by GNU oracle cases for punctuation words, bigwords, counts, operator-specific `w`/`W` behavior, first-print, column, and bracket matching. |
| `vi-change-case`, `vi-change-char`, `vi-replace`, `vi-change-to`, `vi-delete`, `vi-delete-to`, `vi-subst`, `vi-yank-to` | Compatible | Operator, change, replacement, and redo cases are covered by vi/oracle tests. |
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
| `$include` | Implementation-specific | CWD-relative includes with tilde expansion; `$VAR`/`${VAR}` are left literal (no env expansion) matching GNU; missing files are silently skipped. Boundary (against the Readline 8.3 patch 0 baseline): `~/` resolves via `HOME`, `~user` is a pure-Rust `/etc/passwd` best-effort lookup with unknown users skipped, and the depth-16 cap is a fail-closed cycle guard. |
| Unsupported `$` directives | Compatible | Unknown directives are ignored. |
| Unknown function names in key bindings | Compatible | Unknown function bindings in inputrc are ignored and later lines continue. |
| Init file load errors during editor construction | Compatible | `Editor::new` retains the initial load error for inspection, `Editor::try_new` returns it, and explicit reload/load APIs report errors. |

### Variables

| Variable(s) | Status | Notes |
| --- | --- | --- |
| `editing-mode`, `keymap` | Compatible | Selects current editing mode or target binding map. |
| `active-region-start-color`, `active-region-end-color`, `enable-active-region` | Terminal-backed | Region display exists, `bind -v` output is GNU-shaped, and rendering is handled through the display backend. |
| `bell-style`, `prefer-visible-bell` | Compatible | Audible/visible/none behavior is implemented through the terminal abstraction. |
| `bind-tty-special-chars` | Implementation-specific | TTY special bindings are applied from terminal metadata exposed by the backend; EOF binding in vi mode is covered by GNU oracle tests. Decided policy (against the Readline 8.3 patch 0 baseline): ERASE/KILL/WERASE plus VEOF/VINTR only, with no VLNEXT binding (use `quoted-insert` explicitly); VEOF/VINTR go through the keymap (`vi-eof-maybe` in vi mode); while the variable is on, tty bytes rebind each read and win over prior user bindings for those bytes (set the variable off to keep custom bindings); disabled or absent bytes leave existing bindings untouched with no stale-byte unset. |
| `blink-matching-paren` | Terminal-backed | Implemented for self-insert through redisplay timing and terminal output. |
| `colored-completion-prefix`, `colored-stats`, `visible-stats` | Terminal-backed | Completion display support exists through the terminal display backend, including `LS_COLORS`-style rules used by Sushline. |
| `comment-begin` | Compatible | Used by `insert-comment`. |
| `completion-display-width`, `completion-prefix-display-length`, `completion-query-items`, `page-completions`, `print-completions-horizontally` | Compatible | Used by completion display and covered by focused tests. |
| `completion-ignore-case`, `completion-map-case`, `expand-tilde`, `mark-directories`, `mark-symlinked-directories`, `match-hidden-files` | Hook-backed | Used by filename completion and covered by GNU oracle cases; application-specific quoting can be supplied through `Hooks::quote_completion`. |
| `disable-completion`, `show-all-if-ambiguous`, `show-all-if-unmodified`, `skip-completed-text`, `menu-complete-display-prefix` | Compatible | Used by completion engine and covered by focused tests. |
| `convert-meta`, `input-meta`, `meta-flag`, `output-meta`, `enable-meta-key`, `force-meta-prefix` | Terminal-backed | Meta input/output behavior is mediated by Sushline's terminal/backend model and covered by variable tests. |
| `echo-control-characters`, `byte-oriented` | Terminal-backed | Affects Sushline display rendering and is covered by variable/display tests. TAB expansion to tab stops is unconditional (also with `echo-control-characters` off), matching the baseline oracle. |
| `enable-bracketed-paste`, `enable-keypad` | Compatible | Applied during terminal preparation/depreparation and tested. Decided policy (against the Readline 8.3 patch 0 baseline, Bash 5.3 PTY oracle): `enable-bracketed-paste` gates terminal DEC `?2004` mode only; injected `ESC[200~...ESC[201~` still pastes literally when off (framed controls stay literal, unterminated begin holds the line awaiting the end marker, pasted text sets the mark), while a lone unframed `ESC[201~` takes the normal unbound path. |
| `emacs-mode-string`, `vi-cmd-mode-string`, `vi-ins-mode-string`, `show-mode-in-prompt` | Compatible | Used by prompt rendering and tested. |
| `history-preserve-point`, `history-size`, `mark-modified-lines`, `revert-all-at-newline`, `search-ignore-case`, `horizontal-scroll-mode`, `isearch-terminators`, `keyseq-timeout` | Compatible | Implemented in editor/history/display/input paths and covered by focused tests. |
| `histchars`, `history-word-delimiters`, `history-search-delimiter-chars`, `history-no-expand-chars`, `history-quotes-inhibit-expansion` | Compatible | Parsed and used to build `HistoryExpansionPolicy` for editor history expansion. |

## History Expansion API

| Feature | Status | Notes |
| --- | --- | --- |
| Event designators `!!`, `!n`, `!-n`, `!string`, `!?string[?]`, `!$`, `!^`, `!:`, `!#` | Compatible | Implemented by `history::expand_history`. |
| Quick substitution `^old^new^` | Compatible | Implemented for the previous history entry. |
| Word designators `0`, `n`, `^`, `$`, `%`, `x-y`, `*`, `x*`, `x-` | Hook-backed | Implemented over command words; quoted words, shell variable-like words, escaped spaces, command substitutions, process substitutions, shell operators, assignment-like array syntax, and common delimiters are covered by GNU oracle tests. Exact shell tokenization and status can be provided through `Hooks::expand_history`. |
| Modifiers `h`, `t`, `r`, `e`, `q`, `x`, `s/old/new/`, `&`, `g`, `a`, `G` | Compatible | Covered by GNU oracle tests for path modifiers, quoting modifiers, and substitution variants. |
| Modifier `p` | Compatible | `HistoryExpansion` preserves the print-only status. |
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
| Files: `read_history`, `write_history`, `append_history`, `history_truncate_file`, default `~/.history` filename | `read_file`, `load_file`, `write_file`, `append_file`, `append_last_to_file`, `append_new_to_file`, `truncate_file`, `default_file_path`, default-file helpers | Compatible | File operations, timestamps, and default path helpers are covered by tests; concurrent-writer merging is embedder policy (last-writer-wins, no locking), matching GNU. Decided policy (against the Readline 8.3 patch 0 baseline): `truncate_file` keeps the last N timestamp-joined Rust entries (logical commands), not physical file lines; timestamps are preserved on truncate. |
| File range: `read_history_range` | `read_file_range`, `load_file_range` | Compatible | Range-reading APIs are implemented and covered by tests. Decided policy (against the Readline 8.3 patch 0 baseline): `read_file_range`/`load_file_range` FROM/TO count timestamp-joined Rust entries (logical commands), not physical file lines. |
| Expansion: `history_expand` | `expand_history`, `expand_history_with_status`, `Hooks::expand_history` | Compatible | Expanded text and `:p` print-only status are available and can be passed through the editor hook boundary. |
| Expansion helpers: `get_history_event`, `history_tokenize`, `history_arg_extract` | `get_history_event`, `history_tokenize`, `history_arg_extract`, `command_words` | Compatible | Rust helper APIs are exposed and covered by tests. |
| Variables: `history_base`, `history_length`, `history_max_entries` | `HistoryState` and methods | Compatible | Represented as Rust-owned state rather than process globals; `HistoryState` offset, length, stifle, and maximum-entry behavior is covered by tests. |
| Variables: `history_expansion_char`, `history_subst_char`, `history_comment_char`, `history_word_delimiters`, `history_search_delimiter_chars`, `history_no_expand_chars`, `history_quotes_inhibit_expansion` | `HistoryChars`, `HistoryExpansionPolicy` | Compatible | Available to expansion APIs and wired into editor history expansion. |
| Variable: `history_write_timestamps` | `write_file_with_timestamps`, `append_file_with_timestamps`, `append_new_to_file_with_timestamps` | Compatible | Timestamp writing can be enabled or suppressed per call. |
| Variable: `history_quoting_state` | `HistoryExpansionPolicy::quote_state` | Compatible | Existing quote state is exposed through the Rust policy object. |
| Variable: `history_inhibit_expansion_function` | `expand_history` inhibit predicate | Compatible | A per-call predicate is available rather than a process-global function pointer. |
