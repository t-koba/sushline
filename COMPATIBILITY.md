# Readline and History Compatibility

Sushline is a Rust line-editing and history foundation that aims to match GNU
Readline and History Library observable behavior where that behavior belongs
inside a line editor or history component.

The compatibility target is GNU Readline 8.3 patch level 0 and GNU History
Library behavior, limited to Sushline's Rust editor and history APIs. Oracle
comparisons use GNU Bash 5.3 with bundled Readline 8.3 as a host. Bash language,
shell runtime, and builtin compatibility are not goals. The C ABI is out of
scope. Claims below are limited to the cited oracle tests and their asserted
fixtures; test modules are pointers to that evidence, not claims of complete
feature coverage.

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

**Oracle qualifier:** Bash PTY comparisons are runtime observations against the
tested Bash 5.3 host, not GNU source proof, and establish only the tested fixture
scope.

## Explicitly Out Of Scope

Readline and History Library C interfaces are outside this document. Sushline
exposes Rust crates and Rust APIs. Shell language, shell builtins, and shell
expansion are targets only where GNU Readline or History exposes behavior
through the editor, inputrc, history, or completion. Aliases, variables, jobs,
shell expansion, external editing, and `bind -x` execution are embedder-owned
and supplied through `Hooks` where applicable.

## Compatibility Boundaries

| Area | Baseline behavior | Sushline behavior | Status |
| --- | --- | --- | --- |
| Command-word parsing | Shell-word operations typically use the embedding shell's lexer state. | Built-in parser covers tested quoted history words, substitutions, and operators; `Hooks::shell_word_spans` supplies exact spans, while dynamic history completion and yank-argument commands use `Hooks::shell_words`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). | Hook-backed |
| Filename quoting and locale edges | Quoting depends on quote state, locale, byte handling, and shell integration. | Byte-oriented dequote/requote supports tested plain, quoted, ANSI-C, metacharacter, control-byte, tilde, and cursor/suffix cases; `Hooks::quote_completion` supplies application policy. Open ANSI-C insertion preserves a closing delimiter from the hook result. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Hook-backed |
| Embedder-owned completion categories | Command/variable completion may include application-owned aliases, reserved words, functions, builtins, and variables. | Executables/platform fallbacks are local; application names use `Hooks::command_names` and `Hooks::variable_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Hook-backed |
| Application expansion and embedder state commands | Expansion, spelling, shell version, TTY status, external editing, and `bind -x` depend on application state. | Context-carrying hooks provide these responsibilities and return edits/output to Readline command flow. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). | Hook-backed |
| Cell width and locale | In UTF-8 mode, width and cursor motion depend on character and locale handling. | Static UTF-8 `char_width` uses UAX #11 (wide `中` is two cells; ambiguous characters use non-CJK widths). In the tested `LC_ALL=C` PTY case, Bash emits two backspaces for two Ctrl-Bs over `中x`; Sushline renders the UTF-8 bytes and moves by scalar, emitting cursor columns `19` then `16` with the tested prompt. This is a known cursor-width difference, not a physical-terminal glyph-width claim. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). | Known deviation |
| Unwrapped ANSI in prompts | In the tested Bash 5.3 10-column PTY, an ANSI color prompt produces different Readline redisplay when its escapes are not marked with `\[`/`\]` as nonprinting. | Sushline treats the tested unwrapped `ESC [` sequences as zero-width; bare and `SOH`/`STX`-marked forms produce identical redraw for this prompt and command. Tests: [`oracle/display.rs`](readline/tests/oracle/display.rs), especially `unwrapped_ansi_prompt_redisplay_differs_from_bash_markers_only_in_bash`, and [`prompt/tests.rs`](readline/src/prompt/tests.rs). | Known deviation |
| Region/display/terminal internals | Redisplay and active-region behavior depend on terminal capabilities. | Rust display model uses `TerminalIo`; the built-in terminal is Unix-oriented and live operations return `Unsupported` on non-Unix unless another backend is supplied. Raw-mode setup clears `IXON`/`IXOFF`, `ICRNL`/`INLCR`, and `ISTRIP`/`INPCK` for `CS8`, but leaves `CS7` unchanged, narrower than GNU's `CS7` plus meta-flag case. Exact bytes depend on backend. Tests: [`oracle/display.rs`](readline/tests/oracle/display.rs), [`variables.rs`](readline/tests/variables.rs). | Terminal-backed |
| Event / input-available hooks | GNU provides idle event/input hooks and keyboard-input timeouts. | No implicit idle callback: embedder policy uses `TerminalIo::read_event` timeout/poll and `Hooks::check_signals`; `keyseq-timeout` drives the read timeout. Tests: [`variables.rs`](readline/tests/variables.rs). | Terminal-backed |
| Signal install/catch/reraise | GNU independently gates signal catching, preserves ignored dispositions, chains saved `SIGWINCH`, and cleans up then reraises caught signals including `SIGINT`. | Built-in `Terminal` selects eight signals and `SIGWINCH` independently (both on by default); restores changed dispositions, preserves `SIG_IGN`, chains `SIGWINCH`, and permits only one raw-mode terminal at a time (`WouldBlock` otherwise). Interrupted reads recheck hooks; non-INT signals restore/raise/re-enter. `SIGINT` returns `Interrupted` with `^C` echo and is not reraised (known deviation). Custom backends own signal policy. Tests: [`signals.rs`](readline/tests/signals.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). | Known deviation |
| Completion redraw of invalid filename bytes | In the tested Bash 5.3 PTY fixtures, `output-meta` off redraws byte `0xFF` as ASCII `\\377`; `output-meta` on redraws it raw. The locale defaults are off under `LC_ALL=C` and on under `C.UTF-8`. | With the startup `INPUTRC` isolated, Sushline follows the same defaults for the tested `complete-filename` fixture; explicit inputrc settings still override them. Accepted filename bytes remain raw. This is fixture-scoped to byte `0xFF`, the tested prefix, command, and locales. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), especially `bash_readline_and_sushline_list_raw_invalid_byte_filename_under_tested_locales`, `complete_filename_accepts_exact_raw_invalid_byte_filename_under_tested_locales`, and `filename_completion_redraw_respects_locale_output_meta_defaults_for_invalid_byte`. | Compatible |
| Undo insert grouping | GNU joins single-byte inserts into a prior insert entry up to 20 characters; longer and multibyte inserts begin entries. | Self-insert, literal, and tab inserts follow the 20-byte single-byte rule; multibyte scalars begin their own entries and incomplete input is buffered across events. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), including `bash_readline_and_sushline_accept_same_short_multibyte_undo_split`. | Compatible |

## High-Level Coverage

| Area | Status | Implemented surface | Boundary notes |
| --- | --- | --- | --- |
| Basic line editing | Mixed | Insert/delete/movement, undo/overwrite, marks/regions, macros, kill/yank, transpose, case conversion. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). | Shell-word classes are hookable; terminal bytes depend on backend; C-locale multibyte cursor motion differs as scoped above. |
| Emacs keymap, bindable names, and `bind` | Compatible | Keymaps, inputrc bindings/macros, numeric arguments, command names and bind output/storage/query. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). | `bind -x` execution and application state are hook-backed. |
| vi mode | Compatible | Insert/command mode, movement/operators, marks, redo/search/put/yank and completion bindings. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). | External editing is hook-backed. |
| Init file/inputrc | Compatible | `set`, bindings/macros, conditionals and includes. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc.rs`](readline/src/inputrc/tests.rs). | Arbitrary shell/application state is not inferred. |
| Completion | Mixed | Default, filename, menu, listing, insertion, export, display, and tested options. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Application names/quoting are hook-backed. Invalid-byte redraw differs under `LC_ALL=C` as described above. |
| History navigation/search | Mixed | Previous/next, prefix/substring and incremental/non-incremental search. Readline-editor `search-ignore-case` uses one-to-one lowercase scalars from an owned `LC_CTYPE` locale in supported UTF-8 locales; unavailable locale support or invalid UTF-8 falls back to ASCII byte folding. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`state/search.rs`](readline/src/state/search.rs). | Vi `/` and `?` stay case-sensitive with `search-ignore-case` on. The public History byte-search API remains ASCII-only. |
| History expansion | Compatible | Event/word designators, modifiers, quick substitution, policy, quote state and inhibition. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`history.rs`](history/tests/history.rs). | Alias expansion and exact application lexer state are hook-backed; stateful substitution/search memory belongs to `HistoryExpansionState`. |
| History file storage | Mixed | Read/write/append/range/truncate, timestamps, multiline entries, and default-path helpers. Timestamp lines delimit logical entries with embedded blanks; untimestamped files remain one entry per physical line and blank lines are dropped. An unterminated trailing line is ignored. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). | NUL-free inputs agree; Rust preserves embedded NUL bytes while GNU C strings truncate. Concurrent-writer policy is last-writer-wins without locking. Range/truncate count timestamp-joined logical entries. Writes use atomic rename for regular files, in-place truncate for special destinations (including symlink-to-special, link preserved); regular-file mode is `0600`, close-only/no fsync. Reads/range/truncate reject non-regular destinations. |

## User-Facing Readline Commands

GNU command names below come from the Readline User Manual bindable-command
sections; Sushline-only extensions are marked `Implementation-specific`.

### Moving

| Command(s) | Status | Notes |
| --- | --- | --- |
| `beginning-of-line`, `end-of-line`, `forward-char`, `backward-char`, `forward-word`, `backward-word` | Compatible | Covered editing and word-motion cases. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `forward-byte`, `backward-byte` | Compatible | Includes movement inside UTF-8 byte sequences. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `shell-forward-word`, `shell-backward-word` | Hook-backed | Built-in parser covers tested cases; exact spans can come from `Hooks::shell_word_spans`. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `previous-screen-line`, `next-screen-line` | Compatible | Narrow-terminal cases. Tests: [`oracle/display.rs`](readline/tests/oracle/display.rs). |
| `clear-screen`, `clear-display`, `redraw-current-line` | Terminal-backed | Line buffer is preserved; escape bytes depend on terminal/backend. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### History Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `accept-line`, `previous-history`, `next-history`, `beginning-of-history`, `end-of-history` | Compatible | Editor/history integration. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `reverse-search-history`, `forward-search-history`, `non-incremental-reverse-search-history`, `non-incremental-forward-search-history`, `non-incremental-forward-search-history-again`, `non-incremental-reverse-search-history-again` | Compatible | Direction, repeat, case, abort/accept, bells, quoting, prompt and query memory are covered. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-search-backward`, `history-search-forward`, `history-substring-search-backward`, `history-substring-search-forward` | Compatible | With `search-ignore-case` enabled, the editor uses one-to-one `towlower_l` matching in a supported UTF-8 locale; unavailable locale support or invalid UTF-8 uses the existing ASCII byte fold. With it off (default), the tested ASCII and non-ASCII backward and forward prefix/substring fixtures stay case-sensitive. A numeric argument repeats navigation for the tested count-2 fixtures, a negative count searches the opposite direction for the tested fixtures, a zero count is a silent no-op for the tested fixtures, and an excessive count stops silently at the oldest/newest match for the tested fixtures; navigation uses text before point for the tested backward and forward mid-line fixtures; prefix search skips the selected entry; a failed forward prefix search past the newest entry restores the saved edit, while from an older entry it preserves the current line (tested same-case past-newest vs cross-case fixtures); a failed navigation rings once and a successful one stays silent for the tested fixtures; an empty substring matches every entry for the tested fixtures (newest/newer, otherwise bell); forward substring search at history end does not wrap. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-expand-line`, `magic-space` | Compatible | Built-in expansion uses GNU-default policy; custom characters/policy are direct-API only. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-and-alias-expand-line`, `alias-expand-line` | Hook-backed | Alias expansion uses `Hooks::expand_aliases`. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `yank-nth-arg`, `yank-last-arg`, `insert-last-argument` | Hook-backed | Tested shell words; exact application lexer words may come from `Hooks::shell_words`; yanking with nothing to yank rings once. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `fetch-history` | Compatible | Numbered fetch; an out-of-range number (including 0) silently fetches the first entry, and an empty history stays put. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `operate-and-get-next` | Compatible | Multi-read prefill including numeric arguments. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |

### Text Editing

| Command(s) | Status | Notes |
| --- | --- | --- |
| `end-of-file`, `delete-char`, `backward-delete-char`, `forward-backward-delete-char` | Compatible | Empty-input EOF and delete behavior; backward or forward delete with nothing to delete rings once. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `quoted-insert`, `tab-insert`, `self-insert`, `bracketed-paste-begin` | Compatible | Literal insertion/paste; embedded TAB expands to next multiple-of-eight stop; a negative self-insert count inserts nothing. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/display.rs`](readline/tests/oracle/display.rs). |
| `transpose-chars`, `transpose-words` | Compatible | Tested editor cases; transpose with nothing to swap rings once, while an explicit zero count stays silent. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `shell-transpose-words` | Hook-backed | Tested shell constructs; exact spans may come from hooks. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `upcase-word`, `downcase-word`, `capitalize-word` | Hook-backed | Numeric and tested word boundaries; custom classes use `Hooks::editing_word_breaks`. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `overwrite-mode` | Compatible | Editor tests. Tests: [`editing.rs`](readline/tests/editing.rs). |

### Killing And Yanking

| Command(s) | Status | Notes |
| --- | --- | --- |
| `kill-line`, `backward-kill-line`, `unix-line-discard`, `kill-whole-line` | Compatible | Direction and numeric cases; `unix-line-discard` or `backward-kill-line` with nothing to kill rings once. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `kill-word`, `backward-kill-word`, `unix-word-rubout`, `unix-filename-rubout` | Hook-backed | Tested numeric/rubout cases; a rubout with nothing to kill rings once; custom classes use `Hooks::editing_word_breaks`. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `shell-kill-word`, `shell-backward-kill-word` | Hook-backed | Tested shell constructs; exact spans may come from hooks. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `delete-horizontal-space` | Compatible | Tested editor cases. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `kill-region`, `copy-region-as-kill`, `copy-backward-word`, `copy-forward-word` | Compatible | Accepted-line behavior. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `yank` | Compatible | Includes Editor-owned kill-ring persistence across prompts. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `yank-pop` | Compatible | Kill cycling; yanking with an empty ring rings once. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### Numeric Arguments And Macros

| Command(s) | Status | Notes |
| --- | --- | --- |
| `digit-argument`, `universal-argument` | Compatible | Editor/oracle cases, including multi-digit chaining (tested `M-1 M-0` self-insert repeat). Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `start-kbd-macro`, `end-kbd-macro`, `call-last-kbd-macro` | Compatible | Replay and Editor-owned macro persistence across prompts. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `print-last-kbd-macro` | Compatible | Recorded macro output. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### Completion Commands And Behavior

| Command/feature(s) | Status | Notes |
| --- | --- | --- |
| `complete`, `possible-completions`, `insert-completions`, `delete-char-or-list` | Compatible | Tested common-prefix, display/insertion, repeated completion, ambiguity, quoting, and `show-all-if-unmodified` cases. A closed ANSI-C `$'\x61'` word completes to `a`; noquote/fullquote option cases and default separator behavior are covered. Control-byte listing displays caret notation; raw names sort before directory markers in the tested `a!dir` case. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `complete-command`, `possible-command-completions` | Hook-backed | Executables local; application names use `Hooks::command_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `complete-filename`, `possible-filename-completions` | Hook-backed | Tested filename quoting, locale, hidden names, directory marks, control bytes (`ESC`, `TAB`, `LF`, `DEL`, `BEL`, backspace, vertical tab, form feed, `CR`), invalid bytes, reopened quote segments, and cursor/suffix boundaries; application policy uses `Hooks::quote_completion`. For the tested `0xFF` candidate with startup `INPUTRC` isolated, redraw follows the locale `output-meta` default on both implementations; accepted filename bytes remain raw. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`editor/tests/completion.rs`](readline/src/editor/tests/completion.rs). |
| `complete-hostname`, `possible-hostname-completions` | Hook-backed | Hooks plus available platform sources. Tests: [`completion.rs`](readline/tests/completion.rs). |
| `complete-username`, `possible-username-completions` | Hook-backed | Hooks plus available platform sources. Tests: [`completion.rs`](readline/tests/completion.rs). |
| `complete-variable`, `possible-variable-completions` | Hook-backed | Application variables use `Hooks::variable_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `menu-complete`, `menu-complete-backward`, `old-menu-complete` | Compatible | Numeric args, cycling/wrapping, single match and display prefix; quoted filename closers, separators, directory slashes, suffixes, case settings and cursor placement are covered for fixtures. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `complete-into-braces` | Compatible | Brace layout, shared prefix, quoting and separator. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `dabbrev-expand`, `dynamic-complete-history` | Hook-backed | Tested history words; exact words may come from `Hooks::shell_words`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `glob-complete-word`, `glob-expand-word`, `glob-list-expansions` | Hook-backed | Representative glob cases; `Hooks::glob_expand` supplies application globbing. `?` matches one UTF-8 character in the tested locale and falls back to bytes when an invalid locale is unavailable. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `vi-complete` | Compatible | Default vi `*`, `=`, and backslash bindings, including command-mode bounds. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `bash-vi-complete` | Hook-backed | Command completion; application names use `Hooks::command_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `export-completions` | Compatible | Export-completions protocol. Tests: [`completion.rs`](readline/tests/completion.rs). |

### Miscellaneous Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `re-read-init-file`, `abort`, `do-lowercase-version`, `undo`, `revert-line`, `set-mark`, `exchange-point-and-mark`, `skip-csi-sequence`, `dump-functions`, `dump-variables`, `dump-macros`, `execute-named-command`, `emacs-editing-mode`, `vi-editing-mode` | Compatible | Observable editing, reload and dump behavior; `do-lowercase-version` rings when the key has no case variant, empty `undo` rings once, `revert-line` with no changes rings once, and `abort` rings while preserving the line. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `prefix-meta` | Implementation-specific | Sushline inputrc extension, not a GNU binding or Bash oracle target; physical ESC input is separate. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `arrow-key-prefix` | Compatible | Accepted CSI-skip command. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `display-shell-version`, `tty-status` | Hook-backed | Status supplied by hooks. Tests: [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `shell-expand-line`, `spell-correct-word`, `edit-and-execute-command` | Hook-backed | Application services use hooks. Tests: [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| Application command bindings (`bind -x`) | Hook-backed | BindApi stores, queries, unbinds and prints; dispatch uses `Hooks::on_command`. Tests: [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `tilde-expand` | Compatible | Current-word expansion, whitespace, assignment-like words and `~+`/`~-`. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `character-search`, `character-search-backward` | Compatible | Tested editor cases; a search with no match rings once. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `insert-comment` | Compatible | Inserts/toggles `comment-begin` and accepts. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### Vi Command Names

| Command(s) | Status | Notes |
| --- | --- | --- |
| `vi-append-eol`, `vi-append-mode`, `vi-insert-beg`, `vi-insertion-mode`, `vi-movement-mode`, `vi-editing-mode` | Compatible | Tested mode behavior. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-arg-digit`, `vi-search`, `vi-search-again`, `vi-char-search` | Compatible | Non-incremental `/` and `?`, cursor-exclusive backward execute, repeat and persistent `f/F/t/T` target; vi search remains case-sensitive with `search-ignore-case` on. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `vi-bWord`, `vi-backward-bigword`, `vi-back-to-indent`, `vi-first-print`, `vi-backward-word`, `vi-bword`, `vi-prev-word`, `vi-column`, `vi-eWord`, `vi-end-bigword`, `vi-end-word`, `vi-eword`, `vi-fWord`, `vi-forward-bigword`, `vi-forward-word`, `vi-fword`, `vi-next-word`, `vi-match` | Compatible | Tested punctuation/bigword, counts, operators, column and bracket cases. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `vi-change-case`, `vi-change-char`, `vi-replace`, `vi-change-to`, `vi-delete`, `vi-delete-to`, `vi-subst`, `vi-yank-to` | Compatible | Operator, change, replacement and redo. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-overstrike`, `vi-overstrike-delete`, `vi-rubout`, `vi-put`, `vi-redo`, `vi-undo`, `vi-yank-pop` | Compatible | Tested editor cases; last change persists across reads; empty `vi-undo` rings once. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-fetch-history`, `vi-eof-maybe`, `vi-goto-mark`, `vi-set-mark`, `vi-tilde-expand`, `vi-unix-word-rubout`, `vi-yank-arg` | Compatible | Tested fetch, EOF, tilde, mark, default register-command behavior, rubout and numeric yank-arg behavior. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `vi-edit-and-execute-command` | Hook-backed | External edit behavior uses `Hooks::edit_and_execute`. Tests: [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `vi-set-register` | Implementation-specific | Named vi registers are a Sushline extension, disabled by default; enable by setting `sushline-vi-registers` to `on` in inputrc. |

## Readline Init File and Variables

### Init Syntax

| Feature | Status | Notes |
| --- | --- | --- |
| Blank lines and `#` comments | Compatible | Byte-detected, including non-UTF-8 comment bytes. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Inputrc file encoding | Implementation-specific | Files/includes are read as bytes; valid lines survive invalid bytes elsewhere, non-UTF-8 non-comment lines are skipped; use ASCII `\\nnn`/`\\xHH` escapes for eight-bit keys. GNU parses raw C bytes without UTF-8 validation. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| `set variable value` | Compatible | Recognized variables normalized; unknown names ignored. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Key bindings by key name or quoted key sequence | Compatible | Functions and macros. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Escape sequences `\C-`, `\M-`, `\e`, `\\`, `\"`, `\'`, `\a`, `\b`, `\d`, `\f`, `\n`, `\r`, `\t`, `\v`, octal, hex | Compatible | Decoded through `KeySequence`. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| `$if`, `$else`, `$endif` | Compatible | Mode, term, version and application conditions; arbitrary variable comparisons are inactive. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| `$include` | Implementation-specific | CWD-relative, tilde expansion; `$VAR` stays literal; missing files skipped. `~user` uses best-effort pure-Rust passwd lookup; depth-16 cap prevents cycles. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Unsupported `$` directives | Compatible | Unknown directives ignored. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Unknown function names in key bindings | Compatible | Ignored; later lines continue. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Init file load errors during editor construction | Compatible | `Editor::new` retains, `try_new` returns, explicit load/reload reports errors. Tests: [`inputrc.rs`](readline/src/inputrc/tests.rs). |

### Variables

| Variable(s) | Status | Notes |
| --- | --- | --- |
| `editing-mode`, `keymap` | Compatible | Select mode or target binding map. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `active-region-start-color`, `active-region-end-color`, `enable-active-region` | Terminal-backed | Region display uses backend; default on only when terminfo has non-empty `clear`, `smso`, and `rmso` or `sgr0`, otherwise off and color variables absent. Bash PTYs observed on for xterm/screen/tmux/linux/vt100/ansi and off for vt52/adm3a/dumb/empty/unset/unknown TERM; explicit inputrc overrides. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `bell-style`, `prefer-visible-bell` | Compatible | Audible/visible/none via terminal abstraction. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `bind-tty-special-chars` | Implementation-specific | Applies ERASE/KILL/WERASE and VEOF/VINTR, not VLNEXT; VEOF/VINTR pass through keymap. While on, each read rebinds available tty bytes over user bindings; off/absent bytes do not unset existing bindings. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `blink-matching-paren` | Terminal-backed | Tested `()` blink matches transient cursor position after 500 ms but uses save/absolute/restore rather than Bash backspace/right motion. Sushline handles wrapped/scrolled visible openers; offscreen skip is only Sushline-tested, not a GNU claim. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `colored-completion-prefix` | Terminal-backed | Tested shared prefix boundary; fallback prefix style `01;35` matches tested Bash output; suffix retains file style. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `colored-stats` | Terminal-backed | Tested directory/file/executable/symlink LS_COLORS styles; fallback styles are directory `01;34`, executable `01;32`, symlink `01;36`, and no regular-file style. Duplicate `fi` uses the last entry; `ln=target` uses target style. Directory slash is outside SGR; tested executable `*` follows reset. Other types/tokens are unestablished. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `visible-stats` | Terminal-backed | Tested `/`, `*`, `@` markers for directory, executable and symlink; regular file unmarked. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `comment-begin` | Compatible | Used by `insert-comment`. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `completion-display-width` | Compatible | With 12 `alpha-NN` files and 100 columns, widths 20/21 place `[alpha-01]` and `[alpha-01, alpha-02]` in the first row. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `print-completions-horizontally` | Compatible | In the same fixture, width 20 yields only `[alpha-01]`; at 21, horizontal prints `[alpha-01, alpha-02]`, vertical `[alpha-01, alpha-07]`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `page-completions` | Compatible | Tested ten-item, one-column pager on 8x40 terminal; `q` stops listing. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `completion-query-items` | Compatible | Tested one candidate at threshold 1 and two at threshold 2, with paging default/on. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `completion-prefix-display-length` | Compatible | Tested threshold 4 for normal/hidden shared prefixes. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `completion-ignore-case` | Implementation-specific | POSIX UTF-8 path keeps a byte-prefix fast path, then compares decoded scalars with locale lowercase at shared byte offsets and advances by the candidate scalar width, preserving raw names; invalid units, non-UTF-8/unavailable locale and unsupported locale APIs fall back to bytes. Bash matched tested non-ASCII pairs under `C.UTF-8`, not under `C`; tested positives/negatives do not establish general Unicode case folding. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `completion-map-case` | Compatible | Separator mapping requires both this and `completion-ignore-case`; tested both directions, case pairs and original candidate spelling. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `expand-tilde` | Compatible | For tested unquoted `~/`, off preserves spelling while on inserts the resolved path; quoted `~/` expands with either setting. Tested `~/sub`, `~+`, `~-`, named-user and escaped/literal forms are fixture-scoped. Invalid/stale `PWD` falls back to cwd; valid logical symlink PWD is retained. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `mark-directories` | Compatible | Tested listing/insertion slash for regular and symlinked directories. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `mark-symlinked-directories` | Compatible | Symlink insertion slash requires both this and `mark-directories`; defaults are off/on respectively. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `match-hidden-files` | Compatible | With only `.alpha`, empty prefix completes when on and stays unchanged when off; explicit `.a` completes with either setting, while non-dot `a` does not match. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `show-all-if-ambiguous` | Compatible | Tested listing with two matches when on, not on first completion when off. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `show-all-if-unmodified` | Compatible | Tested listing for an already-common prefix; for quoted input with `fullquote` off it stays unchanged/unlisted, while `fullquote` on unquotes and lists. Tested ANSI-C spelling stays unchanged/unlisted with either. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `disable-completion` | Compatible | Tested setting off inserts common prefix/rings once; on leaves typed prefix/rings no bell. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `skip-completed-text` | Compatible | On consumes matching candidate suffixes (and tested closers); partial or unmatched suffixes remain. Case-insensitive matches do not consume a byte-different suffix in tested cases. Cursor boundaries, regular/ANSI-C spellings, directory slashes and menu paths are covered only by the stated matrices. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `menu-complete-display-prefix` | Compatible | Tested first forward/backward actions: display-prefix off inserts an item; on inserts shared prefix and rings once. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `convert-meta`, `input-meta`, `meta-flag`, `output-meta` | Terminal-backed | Bash defaults are convert-meta on/others off under `C`, reversed under `C.UTF-8`; conversion is chunk-invariant. Raw high-bit input survives with conversion off, including inherited `ISTRIP`. Invalid `0xFF` renders as `\377` with output-meta off and raw with it on; valid UTF-8 renders octally only under `C`/off. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `force-meta-prefix` | Implementation-specific | With `convert-meta`, controls Meta binding representation; tested `\\M-a` and raw high-bit versus ESC input under `C`/`C.UTF-8`. Physical ESC+printable unbound input rings without inserting suffix; CSI/SS3 fallback is separate. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `enable-meta-key` | Terminal-backed | Defaults on; notifies backend on setup/re-entry/cleanup. Built-in terminal emits no meta-mode sequences; custom backends own behavior. Tested Bash PTYs emitted none across listed TERM families and recovery. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `echo-control-characters`, `byte-oriented` | Terminal-backed | With byte-oriented on, UTF-8 `é` is octal under `C.UTF-8`/output-meta off; quoted SOH and DEL render `^A`/`^?` with echo-control-characters either way under `C` and `C.UTF-8`. TAB expansion is unconditional. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `enable-bracketed-paste` | Implementation-specific | Gates DEC `?2004` terminal mode only; injected framed paste is parsed literally when off, unterminated begin waits, pasted text sets mark, lone end follows unbound path. Default requires `clear`, standout-on and standout-reset terminfo capabilities (tested on for xterm/screen/tmux/linux/vt100/ansi, off for vt52/adm3a/dumb/empty/unknown); explicit inputrc overrides. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `enable-keypad` | Implementation-specific | Defaults off; on requests available terminfo `smkx`/`rmkx`, no hardcoded fallback. Bash defaults off across tested TERM values; explicit on emits capabilities for xterm-256color/vt100/vt52, none for dumb/unknown/empty/unset. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `emacs-mode-string`, `vi-cmd-mode-string`, `vi-ins-mode-string`, `show-mode-in-prompt` | Compatible | Prompt rendering. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `horizontal-scroll-mode` | Implementation-specific | Tested: at 12 columns `P>` shows eight `a`s plus `>`; at 80 columns, 90 bytewise `a`s with `SUSHLINE_READY>` end at `<` plus 52 `a`s, point column 53. Activation/restoration checkpoints support `columns-2`/`columns-4` hysteresis and one-third restoration. Bash's 100-digit end point varies by bytewise/10-byte/batched input (76/72/52); built-in Sushline reads one byte/event. Prompt ANSI, hidden markers and Unicode clipping have Sushline tests only; other paths remain unverified. Tests: [`oracle/horizontal_scroll.rs`](readline/tests/oracle/horizontal_scroll.rs), [`horizontal_scroll.rs`](readline/src/display/horizontal.rs). |
| `history-preserve-point`, `history-size`, `mark-modified-lines`, `revert-all-at-newline`, `isearch-terminators`, `keyseq-timeout` | Compatible | Editor/history/display/input behavior. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `search-ignore-case` (editor searches) | Compatible | Readline editor searches use owned-locale one-to-one lowercase matching for valid UTF-8 in supported UTF-8 locales; unsupported/unavailable locale handling and invalid UTF-8 retain ASCII byte folding. The measured Bash 5.3 C.UTF-8 pairs Ä/ä, Σ/σ, K/k, İ/i, and ẞ/ß match, while Σ/ς and ß/ss do not. Vi `/` and `?` remain case-sensitive. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`state/search.rs`](readline/src/state/search.rs). |
| `search-ignore-case` (public History byte API) | Implementation-specific | The public `History` byte-search API folds ASCII only by design and is unchanged; editor locale matching is separate. Tests: [`history.rs`](history/tests/history.rs). |
| `histchars`, `history-word-delimiters`, `history-no-expand-chars`, `history-quotes-inhibit-expansion` | Implementation-specific | Not GNU Readline variables; inputrc ignores them. Rust `HistoryExpansionPolicy` offers per-call configuration with GNU defaults. Tests: [`history.rs`](history/tests/history.rs). |

## History Expansion API

| Feature | Status | Notes |
| --- | --- | --- |
| Event designators `!!`, `!n`, `!-n`, `!string`, `!?string[?]`, `!$`, `!^`, `!:`, `!#` | Compatible | Dots, slashes and most punctuation remain in `!string` prefixes; whitespace, `:`, introducers and shell metacharacters terminate them. Omitted `?` needles stop at newline/end without splitting modifiers; newline is not consumed. `search_delimiters` adds `!string` terminators only and is not inputrc-configurable. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Quick substitution `^old^new^` | Compatible | Previous history entry. Tests: [`history.rs`](history/tests/history.rs). |
| Word designators `0`, `n`, `^`, `$`, `%`, `x-y`, `*`, `x*`, `x-` | Hook-backed | Tested quoted/escaped words, shell constructs and `%` match-position selection. Configured non-operator delimiters group contiguous configured bytes; whitespace is skipped unless configured, while operators/redirections at field entry remain fields. Exact tokenization/status can come from `Hooks::expand_history`. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Modifiers `h`, `t`, `r`, `e`, `q`, `x`, `s/old/new/`, `&`, `g`, `a`, `G` | Compatible | Tested path/quote/substitution variants; `&` and empty-old reuse remembered substitution. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Substitution and search memory across calls | Compatible | `HistoryExpansionState` retains substitution, latest substring needle and matched word; failed searches retain the needle, unseeded `!??` fails closed. Tests: [`history.rs`](history/tests/history.rs). |
| Modifier `p` | Compatible | `HistoryExpansion` preserves print-only status. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Existing quote state | Compatible | `HistoryExpansionPolicy::quote_state`. Tests: [`history.rs`](history/tests/history.rs). |
| Inhibit-expansion callback | Compatible | Per-call predicate. Tests: [`history.rs`](history/tests/history.rs). |

## Editor History Expansion Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `history-expand-line`, `magic-space` | Compatible | GNU-default policy; Editor-owned expansion state persists across reads; custom policy is direct-API only. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-and-alias-expand-line` | Hook-backed | History built in; aliases use `Hooks::expand_aliases`. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `alias-expand-line` | Hook-backed | Aliases are embedder-owned. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |

## History Library Surface

The Rust `history::History` type covers History Library operations through
Rust-owned state; this is not a C ABI or C API promise.

| History area | Rust equivalent | Status | Notes |
| --- | --- | --- | --- |
| State setup: `using_history`, `history_get_history_state`, `history_set_history_state` | `History::new`, `History::state`, `History::set_state` | Compatible | Rust-owned state; process-global C state is out of scope. Tests: [`history.rs`](history/tests/history.rs). |
| List management: `add_history`, `add_history_time`, `remove_history`, `replace_history_entry`, `clear_history`, `stifle_history`, `unstifle_history`, `history_is_stifled` | `push`, `push_bytes`, `add_time`, `remove`, `replace`, `clear`, `stifle`, `unstifle`, `is_stifled` | Compatible | Empty explicit entries, timestamp targeting, stifle limits and replacement/undo/timestamp behavior. Editor auto-add separately skips empty accepted lines. Tests: [`history.rs`](history/tests/history.rs). |
| List information: `history_list`, `where_history`, `current_history`, `history_get`, `history_get_time`, `history_total_bytes` | `entries`, `where_history`, `current_history`, `get`, entry `timestamp`, `total_bytes` | Compatible | Rust list, position, timestamp and byte count. Tests: [`history.rs`](history/tests/history.rs). |
| Navigation: `history_set_pos`, `previous_history`, `next_history` | `set_pos`, `previous_history`, `next_history` | Compatible | Implemented and tested. Tests: [`history.rs`](history/tests/history.rs). |
| Search: `history_search`, `history_search_prefix`, `history_search_pos` | `history_search_bytes`, `history_search_bytes_with_case`, `history_search_prefix`, `history_search_pos` | Implementation-specific | Valid positions are inclusive with directional occurrence selection; failed searches preserve cursor. Ignore-case folds ASCII only. Beyond-end positions fail closed, unlike GNU's rejected-position fallback. Tests: [`history.rs`](history/tests/history.rs). |
| Files: `read_history`, `write_history`, `append_history`, `history_truncate_file`, default `~/.history` filename | `read_file`, `load_file`, `write_file`, append variants, `truncate_file`, default path/helpers | Mixed | Regular/special files, timestamps and `~/.history` helpers tested; concurrent writers are last-writer-wins without locking. Truncate counts timestamp-joined logical entries; embedded NUL differs as described above. Tests: [`history.rs`](history/tests/history.rs). |
| File range: `read_history_range` | `read_file_range`, `load_file_range` | Compatible | Range and non-regular input behavior tested; FROM/TO count timestamp-joined logical entries. Tests: [`history.rs`](history/tests/history.rs). |
| Expansion: `history_expand` | `expand_history`, status/state variants, `HistoryExpansionState`, `Hooks::expand_history` | Compatible | Text and print-only status; Editor context forwards session state to hooks. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Expansion helpers: `get_history_event`, `history_tokenize`, `history_arg_extract` | `get_history_event`, `history_tokenize`, `history_arg_extract`, `command_words` | Compatible | Rust helpers exposed and tested. Tests: [`history.rs`](history/tests/history.rs). |
| Variables: `history_base`, `history_length`, `history_max_entries` | `HistoryState` and methods | Compatible | Rust-owned offset, length, stifle and maximum-entry behavior. Tests: [`history.rs`](history/tests/history.rs). |
| Variables: `history_expansion_char`, `history_subst_char`, `history_comment_char`, `history_word_delimiters`, `history_no_expand_chars`, `history_quotes_inhibit_expansion` | `HistoryChars`, `HistoryExpansionPolicy` | Compatible | Available to expansion API and editor expansion. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_write_timestamps` | Timestamp write/append methods | Compatible | Enabled or suppressed per call. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_quoting_state` | `HistoryExpansionPolicy::quote_state` | Compatible | Exposed through Rust policy object. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_inhibit_expansion_function` | `expand_history` inhibit predicate | Compatible | Per-call predicate rather than process-global pointer. Tests: [`history.rs`](history/tests/history.rs). |
