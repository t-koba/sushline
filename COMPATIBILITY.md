# Readline and History Compatibility

Sushline is a Rust line-editing and history foundation that aims to match GNU
Readline and History Library observable behavior where that behavior belongs
inside a line editor or history component.

Current compatibility target: behavior exposed by GNU Bash 5.3 with GNU
Readline 8.3, limited to Sushline's Rust editor and history APIs.

## Status Legend

| Status | Meaning |
| --- | --- |
| Compatible | Intended to match the baseline observable behavior, with no known user-visible difference. |
| Implemented | Implemented, but this document does not make a strong baseline-compatibility claim for the row. |
| Implementation-specific | Implemented through Sushline's own model rather than Readline internals; known observable differences are called out separately. |
| Mixed | The area contains multiple statuses; use the detailed rows below. |
| Known deviation | A known observable behavior differs from the baseline. |
| Hook-required | Command name or mechanism exists, but baseline shell semantics require embedder-supplied state or hooks. |
| Not implemented | In the compatibility target, but missing or effectively inert. |
| Untested | Implemented or partially implemented, but compatibility has not been verified enough to classify. |

## Explicitly Out Of Scope

Readline and History Library C interfaces are outside this document. Sushline
is not intended for use from C; it exposes Rust crates and Rust APIs.

GNU Bash shell builtins and shell expansion semantics are compatibility targets
only where Readline or the History Library exposes the behavior through line
editing, inputrc, history, or completion behavior listed below.

## Known Deviations and Hook-Required Surfaces

| Area | Baseline behavior | Sushline behavior | Status |
| --- | --- | --- | --- |
| Keyboard macro self-insert replay | Recording `abc` and replaying once with `C-x (` ... `C-x ) C-x e` accepts `abca`. | The same sequence accepts `abcabc`; consecutive self-insert keys are recorded and replayed differently. | Known deviation |
| `yank-pop` after multiple kills | `one C-u two C-u X C-y yank-pop` accepts `Xtwoone`. | The same sequence accepts `Xone`; kill-ring rotation/coalescing differs. | Known deviation |
| Ambiguous completion bell | Ambiguous completion rings the bell in baseline-defined cases. | Multiple-candidate completion inserts common prefixes and displays candidates, but does not ring the bell in all the same cases. | Known deviation |
| Editor history expansion commands | `history-expand-line`, `history-and-alias-expand-line`, and `magic-space` expand history using the shell's current history expansion rules; alias-related commands expand shell aliases. | Commands dispatch to hooks; without hook implementation, the line is unchanged. | Hook-required |

## High-Level Coverage

| Area | Status | Implemented surface | Known gaps |
| --- | --- | --- | --- |
| Basic line editing | Mixed | Insert, delete, movement, undo, overwrite, quoted insert, transpose, case conversion, mark/region basics. | Many core edits are compatible or implemented; keyboard macro replay, yank-pop, word-boundary, mark/region, redisplay, and some numeric-argument details have documented differences. |
| Emacs keymap and bindable command names | Mixed | Default keymap, inputrc bindings, macros, numeric arguments, user-facing command names. | Bindable names are accepted; hook-required commands and commands using Sushline word/display state are classified below. |
| vi mode | Mixed | Insert/command mode, common movement, operators, marks, registers, redo, search, put/yank, vi completion bindings. | Vi commands use Sushline's line buffer, register, undo, motion, and search state; detailed rows distinguish implementation-specific behavior from hook-required commands. |
| Init file/inputrc | Mixed | `set`, key bindings, macros, `$if`, `$else`, `$endif`, `$include`, version/term/mode/variable comparisons, include depth checks. | Variable effectiveness and conditional grammar differences are listed below. |
| Completion | Mixed | Default completion, listing, insertion, menu completion, export-completions, and display formatting. | Ambiguous completion bell behavior, filename quoting/display, case/locale behavior, and hook-required shell categories are listed below. |
| History navigation/search | Mixed | Previous/next, beginning/end, prefix search, substring search, incremental and non-incremental search state. | Region activation and search prompt/display differences are listed below. |
| History expansion | Mixed | Public expansion helper supports event designators, word designators, common modifiers, quick substitution, and configurable expansion/substitution/comment characters. Editor commands dispatch expansion through hooks. | `!#`, `:p` status, quote-state, hook-required editor commands, and policy wiring gaps are listed below. |
| History file storage | Mixed | Read, load, write, append, append-new, truncate, timestamp records, file locking on Unix. | `~/.history` null filename, range reader, and timestamp policy differences are listed below. |

## User-Facing Readline Commands

The command names below come from the Readline User Manual bindable command
sections.

### Moving

| Command(s) | Status | Notes |
| --- | --- | --- |
| `beginning-of-line`, `end-of-line`, `forward-char`, `backward-char`, `forward-word`, `backward-word` | Compatible | Implemented with no known user-visible difference. |
| `forward-byte`, `backward-byte` | Implemented | Implemented as byte-position movement commands. |
| `shell-forward-word`, `shell-backward-word` | Known deviation | Implemented using command-word motion over the current line, not Bash's full lexer. |
| `previous-screen-line`, `next-screen-line` | Implementation-specific | Implemented using Sushline's display-state cursor columns rather than Readline's redisplay internals. |
| `clear-screen` | Known deviation | Clears display without a numeric argument; with a numeric argument Sushline consumes the argument and does not clear the terminal. |
| `clear-display` | Implementation-specific | Implemented through terminal clear-display; scrollback-clearing behavior depends on terminal backend. |
| `redraw-current-line` | Implementation-specific | Refresh behavior is implemented through the current redisplay model, not Readline's redisplay internals. |

### History Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `accept-line`, `previous-history`, `next-history`, `beginning-of-history`, `end-of-history` | Implemented | Implemented in the editor/history integration. |
| `reverse-search-history`, `forward-search-history` | Known deviation | Incremental search exists, including case control; prompt rendering and active-region behavior are Sushline implementations. |
| `non-incremental-reverse-search-history`, `non-incremental-forward-search-history` | Known deviation | Implemented using Sushline's internal search prompt rather than Readline's interaction sequence. |
| `history-search-backward`, `history-search-forward` | Implemented | Prefix history search is implemented. |
| `history-substring-search-backward`, `history-substring-search-forward` | Implemented | Substring history search is implemented. |
| `history-expand-line`, `history-and-alias-expand-line`, `alias-expand-line`, `magic-space` | Hook-required | Baseline history commands expand the current line using active history expansion rules, and alias commands use the shell alias table. Sushline accepts the commands, then calls `Hooks::expand_history` or `Hooks::expand_aliases`; without hook support, the line is unchanged. `magic-space` inserts a space after hook expansion succeeds. |
| `yank-nth-arg`, `yank-last-arg` | Known deviation | Implemented, but word extraction is based on simplified command-word parsing rather than Bash tokenization. |
| `operate-and-get-next` | Known deviation | Accepts the current line and queues the next history line; it does not implement the full Readline numeric-argument behavior. |
| `fetch-history` | Known deviation | Implemented; numeric history addressing is not a full History Library API clone. |
| `non-incremental-forward-search-history-again`, `non-incremental-reverse-search-history-again` | Known deviation | Repeats the last non-incremental search when a previous query exists; otherwise rings the bell. |

### Text Editing

| Command(s) | Status | Notes |
| --- | --- | --- |
| `end-of-file`, `delete-char`, `backward-delete-char`, `forward-backward-delete-char` | Implemented | EOF on empty input is implemented. |
| `quoted-insert`, `tab-insert`, `self-insert` | Implemented | Implemented. |
| `bracketed-paste-begin` | Implemented | Starts Sushline bracketed-paste state; terminal enablement is controlled by `enable-bracketed-paste`. |
| `transpose-chars`, `transpose-words` | Compatible | Implemented with no known user-visible difference. |
| `shell-transpose-words` | Known deviation | Implemented using command-word tokenization. |
| `upcase-word`, `downcase-word`, `capitalize-word` | Known deviation | Implemented using Sushline's word-boundary model rather than Readline's configurable word classes. |
| `overwrite-mode` | Implemented | Implemented. |

### Killing And Yanking

| Command(s) | Status | Notes |
| --- | --- | --- |
| `kill-line`, `backward-kill-line`, `unix-line-discard`, `kill-whole-line` | Compatible | Implemented with no known user-visible difference. |
| `kill-word`, `backward-kill-word`, `unix-word-rubout`, `unix-filename-rubout` | Known deviation | Implemented using Sushline word and filename boundary logic rather than Readline's full word-break configuration. |
| `shell-kill-word`, `shell-backward-kill-word` | Known deviation | Implemented using command-word boundaries rather than a complete Bash lexer. |
| `delete-horizontal-space` | Compatible | Implemented with no known user-visible difference. |
| `kill-region`, `copy-region-as-kill`, `copy-backward-word`, `copy-forward-word` | Known deviation | Region/mark operations exist. Active-region display and mark behavior are implemented by Sushline state, not Readline's mark/region state machine. |
| `yank` | Compatible | Implemented with no known user-visible difference. |
| `yank-pop` | Known deviation | Kill-ring rotation/coalescing differs after multiple kills: baseline accepts `Xtwoone`, while Sushline accepts `Xone` for the documented multiple-kill case. |
| `insert-last-argument` | Known deviation | Alias of last-argument yanking; command-word parsing is simplified. |

### Numeric Arguments And Macros

| Command(s) | Status | Notes |
| --- | --- | --- |
| `digit-argument`, `universal-argument` | Implemented | Implemented for editor commands. |
| `start-kbd-macro`, `end-kbd-macro`, `call-last-kbd-macro` | Known deviation | Macro replay of consecutive self-insert keys differs: baseline accepts `abca`, while Sushline accepts `abcabc`. |
| `print-last-kbd-macro` | Implemented | Inputrc-style macro printing is implemented. |

### Completion Commands And Behavior

| Command/feature(s) | Status | Notes |
| --- | --- | --- |
| `complete`, `possible-completions`, `insert-completions`, `delete-char-or-list` | Known deviation | Multiple candidates insert a common prefix and obey `show-all-if-ambiguous` / `show-all-if-unmodified` / repeated completion display logic, but ambiguous multi-candidate `complete` does not ring the bell in the same places as Readline. Default completion is filename completion unless another completion source is configured. |
| `complete-command`, `possible-command-completions` | Hook-required | Baseline command completion includes shell aliases, reserved words, functions, builtins, and executable files. Sushline completes executable names from `PATH` plus `Hooks::command_names`; shell-owned categories require embedder-supplied state. |
| `complete-filename`, `possible-filename-completions` | Known deviation | Uses Sushline filename completion, quoting, and display behavior. |
| `complete-hostname`, `possible-hostname-completions` | Known deviation | Uses hooks plus `/etc/hosts`, `getent hosts`, and OpenSSH `known_hosts` where available. |
| `complete-username`, `possible-username-completions` | Known deviation | Uses hooks plus `/etc/passwd` and `getent passwd` where available. |
| `complete-variable`, `possible-variable-completions` | Hook-required | Baseline variable completion uses the shell's variable namespace. Sushline uses `Hooks::variable_names`; it has no shell variable table of its own. |
| `menu-complete`, `menu-complete-backward`, `old-menu-complete` | Implementation-specific | Numeric arguments and backward cycling are implemented. Cycling past either end rings the bell and restores the original text. Candidate construction, quoting, and suffix behavior follow Sushline completion state. |
| `complete-into-braces` | Implemented | Implemented for completion candidates returned by the completion engine. |
| `dabbrev-expand`, `dynamic-complete-history` | Known deviation | Expands from history words using Sushline command-word tokenization. |
| `glob-complete-word`, `glob-expand-word`, `glob-list-expansions` | Known deviation | Uses Sushline's glob matching and expansion implementation. |
| `vi-complete`, `bash-vi-complete` | Implementation-specific | Dispatches to command completion in vi-related bindings. |
| `export-completions` | Implemented | Sushline implements the Readline export-completions protocol. |
| Default filename completion quoting | Known deviation | Uses byte-oriented dequote/requote for unquoted, single-quoted, and double-quoted words. Multibyte text, locale behavior, backslash handling, and shell quote-state tracking use Sushline logic. |

### Miscellaneous Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `re-read-init-file`, `abort`, `do-lowercase-version`, `prefix-meta`, `undo`, `revert-line`, `set-mark`, `exchange-point-and-mark`, `skip-csi-sequence`, `dump-functions`, `dump-variables`, `dump-macros`, `execute-named-command`, `emacs-editing-mode`, `vi-editing-mode` | Implementation-specific | Implemented within the Rust editor model. Dump output is generated from Sushline keymaps, variables, and macros. |
| `arrow-key-prefix` | Implemented | Accepted as a CSI-skip command. |
| `display-shell-version`, `tty-status` | Hook-required | Baseline behavior prints shell version or job/terminal status from shell state. Sushline dispatches to `Hooks::version` or `Hooks::tty_status`; it rings the bell when the hook returns no text. |
| `shell-expand-line`, `spell-correct-word`, `edit-and-execute-command` | Hook-required | Baseline `shell-expand-line` performs shell expansion, `spell-correct-word` uses shell spelling correction, and `edit-and-execute-command` edits the line externally and executes it. Sushline dispatches to `Hooks::expand_application_line`, `Hooks::spell_correct`, or `Hooks::edit_and_execute`. |
| Application command bindings (`bind -x`) | Hook-required | Baseline `bind -x` binds a key sequence to a shell command evaluated by the shell. `BindApi` stores application command bindings and dispatches them through `Hooks::on_command`; command execution belongs to the embedder. |
| `tilde-expand` | Known deviation | Supports current word/line tilde expansion using Sushline's user lookup paths; it is not a clone of Bash tilde expansion. |
| `character-search`, `character-search-backward` | Implementation-specific | Implemented through Sushline pending-character search, shared with vi search paths; it does not duplicate all Readline numeric-argument edge behavior. |
| `insert-comment` | Implemented | Inserts/toggles `comment-begin` and accepts the line. |

### Vi Command Names

| Command(s) | Status | Notes |
| --- | --- | --- |
| `vi-append-eol`, `vi-append-mode`, `vi-insert-beg`, `vi-insertion-mode`, `vi-movement-mode`, `vi-editing-mode` | Implementation-specific | Implemented as Sushline vi mode transitions and insert commands. |
| `vi-arg-digit`, `vi-search`, `vi-search-again`, `vi-char-search` | Implementation-specific | Implemented through Sushline vi search and numeric argument state. |
| `vi-bWord`, `vi-backward-bigword`, `vi-back-to-indent`, `vi-first-print`, `vi-backward-word`, `vi-bword`, `vi-prev-word`, `vi-column`, `vi-eWord`, `vi-end-bigword`, `vi-end-word`, `vi-eword`, `vi-fWord`, `vi-forward-bigword`, `vi-forward-word`, `vi-fword`, `vi-next-word`, `vi-match` | Implementation-specific | Implemented as vi movement commands over Sushline's line buffer and word model. |
| `vi-change-case`, `vi-change-char`, `vi-replace`, `vi-change-to`, `vi-delete`, `vi-delete-to`, `vi-subst`, `vi-yank-to` | Implementation-specific | Implemented as Sushline vi operators and edits over Sushline motion state. |
| `vi-overstrike`, `vi-overstrike-delete`, `vi-rubout`, `vi-put`, `vi-redo`, `vi-undo`, `vi-yank-pop` | Implementation-specific | Implemented through vi edit, register, undo, and replay state. |
| `vi-fetch-history`, `vi-eof-maybe`, `vi-goto-mark`, `vi-set-register`, `vi-set-mark`, `vi-tilde-expand`, `vi-unix-word-rubout`, `vi-yank-arg` | Implementation-specific | Implemented through the same history, mark/register, expansion, and yank-argument paths as the non-vi commands. |
| `vi-edit-and-execute-command` | Hook-required | Baseline behavior edits the current line externally and executes it. Sushline dispatches to `Hooks::edit_and_execute`. |

## Readline Init File and Variables

### Init Syntax

| Feature | Status | Notes |
| --- | --- | --- |
| Blank lines and `#` comments | Implemented | Implemented. |
| `set variable value` | Known deviation | Recognized variables are normalized; unknown variables are ignored. |
| Key bindings by key name or quoted key sequence | Implemented | Function bindings and macros are supported. |
| Escape sequences `\C-`, `\M-`, `\e`, `\\`, `\"`, `\'`, `\a`, `\b`, `\d`, `\f`, `\n`, `\r`, `\t`, `\v`, octal, hex | Implemented | Parsed through `KeySequence`/inputrc decoding. |
| `$if`, `$else`, `$endif` | Known deviation | Mode, term, version, application-name, and variable comparisons are implemented. The conditional parser is Sushline's parser, and the `version` condition is evaluated against the fixed Readline version string `8.3`. |
| `$include` | Implemented | Implemented with relative include resolution and include-depth protection. |
| Unsupported `$` directives | Known deviation | Readline ignores some unknown constructs more permissively; Sushline returns an inputrc error. |
| Unknown function names in key bindings | Known deviation | A binding to an unknown function name is an inputrc parse error. Readline reports diagnostics and continues more permissively in some cases. |
| Init file load errors during editor construction | Known deviation | The parser reports `InputrcError`, but `Editor::new` discards the result of the initial inputrc reload. |

### Variables

| Variable(s) | Status | Notes |
| --- | --- | --- |
| `editing-mode`, `keymap` | Implemented | Selects current editing mode or target binding map. |
| `active-region-start-color`, `active-region-end-color`, `enable-active-region` | Implementation-specific | Region display exists and uses Sushline mark/region state across commands. |
| `bell-style`, `prefer-visible-bell` | Implemented | Audible/visible/none behavior is implemented through the terminal abstraction. |
| `bind-tty-special-chars` | Implementation-specific | TTY special bindings are applied from terminal metadata exposed by the terminal backend. |
| `blink-matching-paren` | Known deviation | Implemented for self-insert, with simplified timing/display behavior. |
| `colored-completion-prefix`, `colored-stats`, `visible-stats` | Known deviation | Implemented for completion display. `colored-stats` uses a simplified `LS_COLORS` interpretation and default directory coloring, and `visible-stats` appends simplified type markers. |
| `comment-begin` | Implemented | Used by `insert-comment`. |
| `completion-display-width`, `completion-prefix-display-length`, `completion-query-items`, `page-completions`, `print-completions-horizontally` | Implemented | Used by completion display. |
| `completion-ignore-case`, `completion-map-case`, `expand-tilde`, `mark-directories`, `mark-symlinked-directories`, `match-hidden-files` | Known deviation | Used by filename completion. Case matching is byte-oriented and uses C `tolower` on Unix; `completion-map-case` maps `-` to `_`, so locale and multibyte behavior follow Sushline's byte matching. |
| `disable-completion`, `show-all-if-ambiguous`, `show-all-if-unmodified`, `skip-completed-text`, `menu-complete-display-prefix` | Implemented | Used by completion engine. |
| `convert-meta`, `input-meta`, `meta-flag`, `output-meta`, `enable-meta-key`, `force-meta-prefix` | Implementation-specific | Meta input/output behavior exists, with terminal and locale behavior mediated by Sushline's terminal backend. |
| `echo-control-characters`, `byte-oriented` | Implementation-specific | Affects display rendering through Sushline's redisplay model. |
| `enable-bracketed-paste`, `enable-keypad` | Implemented | Applied during terminal preparation/depreparation. |
| `emacs-mode-string`, `vi-cmd-mode-string`, `vi-ins-mode-string`, `show-mode-in-prompt` | Implemented | Used by prompt rendering. |
| `history-preserve-point`, `history-size`, `mark-modified-lines`, `revert-all-at-newline`, `search-ignore-case`, `horizontal-scroll-mode`, `isearch-terminators`, `keyseq-timeout` | Implemented | Implemented in editor/history/display/input paths. |
| `histchars` | Known deviation | Parsed and used to populate `HistoryExpansionContext` for editor history-expansion hooks. |
| `history-word-delimiters`, `history-search-delimiter-chars`, `history-no-expand-chars`, `history-quotes-inhibit-expansion` | Not implemented | Parsed and stored as variables, but editor history-expansion commands do not build a `HistoryExpansionPolicy` from them. The public history expansion API exposes equivalent policy fields. |

## History Expansion API

| Feature | Status | Notes |
| --- | --- | --- |
| Event designators `!!`, `!n`, `!-n`, `!string`, `!?string[?]`, `!$`, `!^`, `!:` | Implemented | Implemented by `history::expand_history`. |
| Quick substitution `^old^new^` | Implemented | Implemented for the previous history entry. |
| Event designator `!#` | Not implemented | The documented "line typed so far" event is not implemented. |
| Word designators `0`, `n`, `^`, `$`, `%`, `x-y`, `*`, `x*`, `x-` | Implemented | Implemented over `command_words`. |
| Modifiers `h`, `t`, `r`, `e`, `q`, `x`, `s/old/new/`, `&`, `g`, `a`, `G` | Known deviation | Implemented over `command_words`; shell quoting and tokenization use Sushline's byte parser. |
| Modifier `p` | Known deviation | Expansion returns modified text, but `history_expand` return code `2` / "print but do not execute" semantics are not represented by the Rust API. |
| Existing quote state | Not implemented | `history_quoting_state` equivalent is not exposed. |
| Inhibit-expansion callback | Implementation-specific | A per-call inhibit predicate exists; there is no compatible global `history_inhibit_expansion_function`. |

## Editor History Expansion Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `history-expand-line`, `history-and-alias-expand-line`, `magic-space` | Hook-required | Baseline commands expand the current line using the shell's active history expansion behavior; `history-and-alias-expand-line` also applies alias expansion. Sushline dispatches through `Hooks::expand_history`; without an embedder implementation, the line is returned unchanged. |
| `alias-expand-line` | Hook-required | Baseline behavior expands aliases from the shell alias table. Sushline dispatches through `Hooks::expand_aliases`; without an embedder implementation, the line is unchanged. |

## History Library Surface

The Rust `history::History` type covers many History Library operations through
Rust-owned state.

This table maps History Library concepts to Rust-owned Sushline APIs; it is not
a C ABI or C API compatibility promise.

| History area | Rust equivalent | Status | Notes |
| --- | --- | --- | --- |
| State setup: `using_history`, `history_get_history_state`, `history_set_history_state` | `History::new`, `History::state`, `History::set_state` | Implementation-specific | Rust-owned state, no process-global session. |
| List management: `add_history`, `add_history_time`, `remove_history`, `replace_history_entry`, `clear_history`, `stifle_history`, `unstifle_history`, `history_is_stifled` | `push`, `push_bytes`, `add_time`, `remove`, `replace`, `clear`, `stifle`, `unstifle`, `is_stifled` | Implementation-specific | Entries and associated metadata are owned by Rust values. |
| List information: `history_list`, `where_history`, `current_history`, `history_get`, `history_get_time`, `history_total_bytes` | `entries`, `where_history`, `current_history`, `get`, entry `timestamp`, `total_bytes` | Implementation-specific | `history_get_time` does not parse to `time_t`; timestamp is stored as text. |
| Navigation: `history_set_pos`, `previous_history`, `next_history` | `set_pos`, `previous_history`, `next_history` | Implemented | Implemented on `History`. |
| Search: `history_search`, `history_search_prefix`, `history_search_pos` | `history_search_bytes`, `history_search_prefix`, `history_search_pos` | Known deviation | Byte/string variants exist; return types differ from the baseline offset-returning API. |
| Files: `read_history`, `write_history`, `append_history`, `history_truncate_file` | `read_file`, `load_file`, `write_file`, `append_file`, `append_new_to_file`, `truncate_file` | Known deviation | No null filename default to `~/.history`; timestamp write policy differs. |
| File range: `read_history_range` | None | Not implemented | No direct range-reading API. |
| Expansion: `history_expand` | `expand_history` | Known deviation | Rust result type differs; `:p` status is not preserved. |
| Expansion helpers: `get_history_event`, `history_tokenize`, `history_arg_extract` | Internal parser and `command_words` | Implementation-specific | Not exposed as compatible public helpers. |
| Variables: `history_base`, `history_length`, `history_max_entries` | `HistoryState` and methods | Implementation-specific | Represented as Rust state, not globals. |
| Variables: `history_expansion_char`, `history_subst_char`, `history_comment_char`, `history_word_delimiters`, `history_search_delimiter_chars`, `history_no_expand_chars`, `history_quotes_inhibit_expansion` | `HistoryChars`, `HistoryExpansionPolicy` | Implementation-specific | Available to expansion APIs. Editor inputrc wiring currently passes only `histchars` into history-expansion hooks. |
| Variable: `history_write_timestamps` | None | Not implemented | Timestamp records are preserved/read/written when present on entries, but there is no global write-timestamps switch equivalent. |
| Variable: `history_quoting_state` | None | Not implemented | Persistent multi-line quote state is not exposed. |
| Variable: `history_inhibit_expansion_function` | `expand_history` inhibit predicate | Implementation-specific | A per-call predicate exists; there is no compatible global function pointer. |
