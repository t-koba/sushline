# Readline and History Compatibility

Sushline is a Rust line-editing and history foundation that aims to match GNU
Readline and History Library observable behavior where that behavior belongs
inside a line editor or history component.

The compatibility target is GNU Readline 8.3 patch level 0 and GNU History
Library behavior, limited to Sushline's Rust editor and history APIs. Oracle
comparisons use GNU Bash 5.3 as a host, and the oracle suite requires that
version on `PATH` and stops without evidence on any other host; the Readline
library that host links is not asserted, so the oracle evidence is Bash runtime
observation rather than proof of a Readline version.
Bash language, shell runtime, and builtin compatibility are not goals. The C
ABI is out of scope. Claims below are limited to the cited oracle tests and
their asserted fixtures; test modules are pointers to that evidence, not claims of complete
feature coverage.

## Status Legend

| Status | Meaning |
| --- | --- |
| Compatible | Matched against the baseline for the scoped behavior, with no known user-visible difference. |
| Implemented | Implemented, but not audited enough to claim GNU-compatible behavior for the whole row. |
| Implementation-specific | Implemented through Sushline's Rust model rather than GNU Readline/History internals; observable differences may exist outside the cited evidence. |
| Sushline extension | Deliberate Sushline-only behavior or API, used only by explicit invocation/configuration; it does not alter GNU-default Readline behavior. |
| Terminal-backed | Implemented through `TerminalIo`, terminfo-backed capabilities where available, and fixed ANSI helpers for the built-in terminal; exact bytes may still depend on terminal/backend capabilities. |
| Mixed | The area contains multiple statuses; use the detailed rows below. |
| Known deviation | A known observable behavior differs from the baseline. |
| Hook-backed | The Readline command/mechanism is implemented; GNU-equivalent embedder-owned state or behavior is supplied through `Hooks`. |
| Not implemented | In the compatibility target, but missing or effectively inert. |
| Untested | Implemented or partially implemented, but not verified enough to classify. |

**Oracle qualifier:** Bash PTY comparisons are runtime observations against the
tested Bash 5.3 host, not GNU source proof, and establish only the tested fixture
scope.

Redisplay checkpoints for ordinary complete Emacs keys use a causal Ctrl-G
abort/BEL barrier in both PTYs, capturing output strictly before BEL rather
than waiting for silence. Steps must not themselves ring or leave pending
input state; the fixtures use the default Emacs abort binding and explicitly enable audible bells.
Sushline's built-in terminal currently reads one byte per event, redisplaying
before it dispatches the next key. Changing that granularity requires
revalidating the barrier. An unfinished paste cannot use this barrier because
it consumes Ctrl-G as payload. Paste PTY tests therefore checkpoint only after
the terminator and wait for an explicit completion marker after Enter.
Synchronous Sushline editor tests cover partial-payload buffering and split or
truncated terminators; absence of intermediate Bash output is no longer
claimed from timing-based checkpoints. Deadlines only bound hangs.

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
| Command-word parsing | Shell-word operations typically use the embedding shell's lexer state. | Built-in parser covers common cases; `Hooks::shell_word_spans` supplies exact spans, while dynamic history completion and yank-argument commands use `Hooks::shell_words`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). | Hook-backed |
| Filename quoting and locale edges | Quoting depends on quote state, locale, byte handling, and shell integration. | Byte-oriented dequote/requote covers common cases; `Hooks::quote_completion` supplies application policy, including the closing delimiter for open ANSI-C insertion. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Hook-backed |
| Embedder-owned completion categories | Command/variable completion may include application-owned aliases, reserved words, functions, builtins, and variables. | Executables/platform fallbacks are local; application names use `Hooks::command_names` and `Hooks::variable_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Hook-backed |
| Application expansion and embedder state commands | Expansion, spelling, shell version, TTY status, external editing, and `bind -x` depend on application state. | Context-carrying hooks provide these responsibilities and return edits/output to Readline command flow. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). | Hook-backed |
| Cell width and locale | In UTF-8 mode, width and cursor motion depend on character and locale handling. | Static UTF-8 `char_width` uses UAX #11; character motion follows bytewise or UTF-8 motion per `byte-oriented`/locale, and interior rendering preserves bytes. Evidence is Bash 5.3 PTY runtime only. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`lifecycle.rs`](readline/src/editor/tests/lifecycle.rs), [`buffer/tests.rs`](readline/src/buffer/tests.rs). | Compatible |
| Unwrapped ANSI in prompts | Unmarked and `\[`/`\]`-marked ANSI prompts differ in Readline redisplay. | Sushline preserves the unmarked/marked column distinction and final cursor columns while repainting rather than matching incremental output bytes. Evidence is Bash 5.3 PTY runtime only; horizontal scrolling keeps its separate deviation below. Tests: [`oracle/display.rs`](readline/tests/oracle/display.rs), [`prompt/tests.rs`](readline/src/prompt/tests.rs). | Implemented |
| Region/display/terminal internals | Redisplay and active-region behavior depend on terminal capabilities. | Rust display model uses `TerminalIo`; the built-in terminal is Unix-oriented and live operations return `Unsupported` on non-Unix unless another backend is supplied. The built-in terminal writes the inputrc diagnostics and the readline error messages to standard error like GNU Readline, and writes the history expansion failure message there as well, which is the built-in terminal's own choice: the failure destination has not been observed on the GNU side with separated streams. A custom backend chooses the destination of its diagnostics through `TerminalIo::write_diagnostic`, which defaults to the terminal output stream. A diagnostic that does not reach the terminal's own screen leaves the display untouched, so `TerminalIo::diagnostics_use_terminal` reports whether the sink is that screen, and the built-in terminal compares standard error with standard output. Raw-mode setup preserves `IXON`/`IXOFF` and normalizes input flags; for `CS7`, Sushline leaves `ISTRIP`/`INPCK` unchanged, unlike GNU's `CS7+meta_flag` path (unit-level coverage only). Exact bytes depend on backend. Tests: [`oracle/display.rs`](readline/tests/oracle/display.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`oracle/diagnostics.rs`](readline/tests/oracle/diagnostics.rs), [`variables.rs`](readline/tests/variables.rs), [`lifecycle.rs`](readline/src/editor/tests/lifecycle.rs), [`terminal.rs`](readline/src/terminal.rs). Bash redirects the line editor's output to standard error, while the built-in terminal writes the display to standard output, so a redirected standard output swallows the prompt for Sushline but not for the host (Bash 5.3 PTY observation). GNU Readline 8.2, linked directly into a program whose own standard output and standard error were redirected to separate files, wrote its prompt to standard output, so the library's default output stream is standard output; the built-in terminal follows that default and differs from the host shell that redirects it. The claim rests on that 8.2 observation; the declared 8.3 baseline has not been measured. The oracle CI job observes the runner image's Readline and prints the version it saw, so a change in the image's library shows up in the job log. | Terminal-backed |
| Event / input-available hooks | GNU provides idle event/input hooks and keyboard-input timeouts. | No implicit idle callback: embedder policy uses `TerminalIo::read_event` timeout/poll and `Hooks::check_signals`; `keyseq-timeout` drives the read timeout. Tests: [`variables.rs`](readline/tests/variables.rs). | Terminal-backed |
| Signal install/catch/reraise | GNU independently gates signal catching, preserves ignored dispositions, chains saved `SIGWINCH`, and cleans up then reraises caught signals including `SIGINT`. | Built-in `Terminal` selects eight signals and `SIGWINCH` independently (both on by default); restores changed dispositions, preserves `SIG_IGN`, chains `SIGWINCH`, and permits only one raw-mode terminal at a time (`WouldBlock` otherwise). Interrupted reads recheck hooks; non-INT signals restore/raise/re-enter. `SIGINT` returns `Interrupted` with `^C` echo and is not reraised (known deviation). `SIGQUIT` disposition follows host signal policy rather than key-binding precedence. Custom backends own signal policy. Tests: [`signals.rs`](readline/tests/signals.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). The oracle case retries the GNU run, because GNU re-raises the interrupt and that re-raise can leave the host waiting instead of showing the next prompt. | Known deviation |
| Completion redraw of invalid filename bytes | `output-meta` controls whether invalid filename bytes redraw as octal escapes or raw, with locale-dependent defaults. | Sushline follows the same locale-dependent defaults; explicit settings override them and accepted bytes remain raw. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Compatible |
| Undo insert grouping | GNU joins single-byte inserts into a prior insert entry up to 20 characters; longer and multibyte inserts begin entries. | Self-insert, literal, and tab inserts follow the 20-byte single-byte rule; multibyte scalars begin their own entries and incomplete input is buffered across events. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). | Compatible |

## High-Level Coverage

| Area | Status | Implemented surface | Boundary notes |
| --- | --- | --- | --- |
| Basic line editing | Mixed | Insert/delete/movement, undo/overwrite, marks/regions, macros, kill/yank, transpose, case conversion. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). | Shell-word classes are hookable; terminal bytes depend on backend; character motion follows the boundary policy above. |
| Emacs keymap, bindable names, and `bind` | Compatible | Keymaps, inputrc bindings/macros, numeric arguments, command names and bind output/storage/query. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). | `bind -x` execution and application state are hook-backed. |
| vi mode | Compatible | Insert/command mode, movement/operators, marks, redo/search/put/yank and completion bindings. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). | External editing is hook-backed. |
| Init file/inputrc | Compatible | `set`, bindings/macros, conditionals and includes, with numeric-argument and abort handling in macros. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`inputrc.rs`](readline/src/inputrc/tests.rs). | Arbitrary shell/application state is not inferred. |
| Completion | Mixed | Default, filename, menu, listing, insertion, export, display, and options. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Application names/quoting are hook-backed. Invalid-byte redraw follows the boundary row above. |
| History navigation/search | Mixed | Previous/next, prefix/substring and incremental/non-incremental search. Readline-editor `search-ignore-case` uses one-to-one lowercase scalars from an owned `LC_CTYPE` locale in supported UTF-8 locales; unavailable locale support or invalid UTF-8 falls back to ASCII byte folding. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`state/search.rs`](readline/src/state/search.rs). | Vi `/` and `?` stay case-sensitive with `search-ignore-case` on. The public History byte-search API remains ASCII-only. |
| History expansion | Compatible | Event/word designators, modifiers, quick substitution, policy, quote state and inhibition, including quote/backslash and tokenization handling. Libhistory cases are History API evidence only. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`history.rs`](history/tests/history.rs), [`editor/tests/history.rs`](readline/src/editor/tests/history.rs). The print-only expansion result takes the same sink, because GNU writes it to standard error as well (Bash 5.3 PTY observation). The failure message is error text, so it goes to the diagnostic sink; the built-in terminal writes it to standard error. | Alias expansion and exact application lexer state are hook-backed; stateful substitution/search memory belongs to `HistoryExpansionState`. |
| History file storage | Mixed | Read/write/append/range/truncate, timestamps, multiline entries, and default-path helpers. Timestamp lines delimit logical entries; untimestamped files keep one entry per line with blanks dropped and trailing unterminated lines ignored. NUL bytes are preserved, concurrent writers are last-writer-wins, range/truncate count logical entries, writes use atomic rename or in-place truncate with `0600` and no fsync, and reads reject non-regular destinations. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). | Embedded NUL is a known deviation from GNU C-string truncation. |

## User-Facing Readline Commands

GNU command names below come from the Readline User Manual bindable-command
sections; deliberate Sushline-only extensions use the `Sushline extension` status.

### Moving

| Command(s) | Status | Notes |
| --- | --- | --- |
| `beginning-of-line`, `end-of-line`, `forward-char`, `backward-char`, `forward-word`, `backward-word` | Compatible | A character move that cannot move (including an excessive count) rings once, while a zero or fully satisfied count stays silent. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `forward-byte`, `backward-byte` | Compatible | Includes movement inside UTF-8 byte sequences. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `shell-forward-word`, `shell-backward-word` | Hook-backed | Built-in parser covers common cases; exact spans can come from `Hooks::shell_word_spans`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `previous-screen-line`, `next-screen-line` | Compatible | Narrow-terminal movement. Tests: [`oracle/display.rs`](readline/tests/oracle/display.rs). |
| `clear-screen`, `clear-display`, `redraw-current-line` | Terminal-backed | Line buffer is preserved; escape bytes depend on terminal/backend. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### History Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `accept-line`, `previous-history`, `next-history`, `beginning-of-history`, `end-of-history` | Compatible | Editor/history integration. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `reverse-search-history`, `forward-search-history`, `non-incremental-reverse-search-history`, `non-incremental-forward-search-history`, `non-incremental-forward-search-history-again`, `non-incremental-reverse-search-history-again` | Compatible | Direction, repeat, case, abort/accept, bells, quoting, prompt and query memory. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-search-backward`, `history-search-forward`, `history-substring-search-backward`, `history-substring-search-forward` | Compatible | Direction, repeat, case, abort/accept, bell, and cursor behavior. Editor searches use locale-aware lowercase matching with `search-ignore-case` on and stay case-sensitive otherwise. Numeric counts repeat, reverse, or no-op with excessive counts stopping at the nearest match; prefix search skips the selected entry and failed forward search preserves the line; failed navigation rings once. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-expand-line`, `magic-space` | Compatible | Built-in expansion uses GNU-default policy; custom characters/policy are direct-API only. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-and-alias-expand-line`, `alias-expand-line` | Hook-backed | Alias expansion uses `Hooks::expand_aliases`. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `yank-nth-arg`, `yank-last-arg`, `insert-last-argument` | Hook-backed | Shell words; exact application lexer words may come from `Hooks::shell_words`. Yanking with nothing to yank rings once; an out-of-range or explicitly negative word number rings instead of clamping, and 0 yanks the command word. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `fetch-history` | Compatible | Numbered fetch; an out-of-range number (including 0) silently fetches the first entry, and an empty history stays put. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `operate-and-get-next` | Compatible | Multi-read prefill including numeric arguments. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |

### Text Editing

| Command(s) | Status | Notes |
| --- | --- | --- |
| `end-of-file`, `delete-char`, `backward-delete-char`, `forward-backward-delete-char` | Compatible | Empty-input EOF and delete behavior; `delete-char` and `backward-delete-char` with nothing to delete ring once. Counted and overwrite-mode behavior matches, with counted kills yankable. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`editing.rs`](readline/tests/editing.rs). |
| `quoted-insert`, `tab-insert`, `self-insert`, `bracketed-paste-begin` | Compatible | `tab-insert` counts match accepted bytes rather than display expansion; `quoted-insert` TAB expands to the next display stop and negative `self-insert` inserts nothing. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/display.rs`](readline/tests/oracle/display.rs). |
| `transpose-chars` | Compatible | Ordinary, zero, numeric, and near-EOL behavior; bell behavior is described separately. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `transpose-words` | Compatible | Numeric behavior. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `shell-transpose-words` | Hook-backed | Quoted-word and numeric behavior with accepted-line cursor placement; exact spans may come from hooks. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `upcase-word`, `downcase-word`, `capitalize-word` | Hook-backed | Numeric behavior with hookable word classes via `Hooks::editing_word_breaks`. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `overwrite-mode` | Compatible | Editor tests. Tests: [`editing.rs`](readline/tests/editing.rs). |

### Killing And Yanking

| Command(s) | Status | Notes |
| --- | --- | --- |
| `kill-line`, `backward-kill-line`, `unix-line-discard`, `kill-whole-line` | Compatible | Direction and numeric behavior; `unix-line-discard` or `backward-kill-line` with nothing to kill rings once, while `kill-line` and `kill-whole-line` stay silent with nothing to kill. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `kill-word`, `backward-kill-word`, `unix-word-rubout`, `unix-filename-rubout` | Hook-backed | Word kills and rubouts honor numeric counts; empty `unix-word-rubout` rings once while empty word kills stay silent; custom classes use `Hooks::editing_word_breaks`. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`editing.rs`](readline/tests/editing.rs). |
| `shell-kill-word`, `shell-backward-kill-word` | Hook-backed | Shell constructs; exact spans may come from hooks. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `delete-horizontal-space` | Compatible | Editor behavior. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `kill-region`, `copy-region-as-kill`, `copy-backward-word`, `copy-forward-word` | Compatible | Unmarked region starts at line start; copy-word behavior handles whitespace boundaries and signed counts. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `yank` | Compatible | Includes Editor-owned kill-ring persistence across prompts. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `yank-pop` | Compatible | Kill cycling; yanking with an empty ring rings once. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### Numeric Arguments And Macros

| Command(s) | Status | Notes |
| --- | --- | --- |
| `digit-argument`, `universal-argument` | Compatible | Multi-digit chaining, sign handling, and universal-argument composition. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `start-kbd-macro`, `end-kbd-macro` | Compatible | A recording without an argument starts a fresh definition, an unfinished recording continues across prompts, and a replay that accepts a line continues in the next prompt. Explicit start and end counts follow the append/replay, no-op, invalidation, abort, and nesting behavior with matching bells and diagnostics. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `call-last-kbd-macro` | Compatible | Default and counted calls, with no-definition and nesting-exhaustion bells. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `print-last-kbd-macro` | Compatible | Recorded macro output. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### Completion Commands And Behavior

| Command/feature(s) | Status | Notes |
| --- | --- | --- |
| `complete`, `possible-completions`, `insert-completions`, `delete-char-or-list` | Compatible | Common-prefix, display/insertion, ambiguity, quoting, and separator behavior with counted `delete-char-or-list` handling. The fallback `-G` matcher treats escaped brackets literally; escaped-hyphen handling is unspecified. Other listing behavior is in the variable rows below. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `complete-command`, `possible-command-completions` | Hook-backed | Executables local; application names use `Hooks::command_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `complete-filename`, `possible-filename-completions` | Hook-backed | Filename quoting, hidden names, directory marks, control/invalid bytes, and listing behavior; application policy uses `Hooks::quote_completion`. Redraw follows locale `output-meta` defaults with accepted bytes raw. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `complete-hostname`, `possible-hostname-completions` | Hook-backed | Hooks plus available platform sources. Tests: [`completion.rs`](readline/tests/completion.rs). |
| `complete-username`, `possible-username-completions` | Hook-backed | Hooks plus available platform sources. Tests: [`completion.rs`](readline/tests/completion.rs). |
| `complete-variable`, `possible-variable-completions` | Hook-backed | Application variables use `Hooks::variable_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `menu-complete`, `menu-complete-backward`, `old-menu-complete` | Compatible | Exhaustion, wrap, display-prefix, quoting, and cursor behavior. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`completion.rs`](readline/tests/completion.rs), [`editor/tests/completion.rs`](readline/src/editor/tests/completion.rs). |
| `complete-into-braces` | Compatible | Brace layout, shared prefix, quoting and separator. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `dabbrev-expand`, `dynamic-complete-history` | Hook-backed | History-word behavior; exact words may come from `Hooks::shell_words`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `glob-complete-word`, `glob-expand-word`, `glob-list-expansions` | Hook-backed | Glob behavior; `Hooks::glob_expand` supplies application globbing with locale-aware matching and byte fallback. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `vi-complete` | Compatible | Default vi `*`, `=`, and backslash bindings, including command-mode bounds. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `bash-vi-complete` | Hook-backed | Command completion; application names use `Hooks::command_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `export-completions` | Compatible | Export-completions protocol. Tests: [`completion.rs`](readline/tests/completion.rs). |

### Miscellaneous Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `re-read-init-file`, `abort`, `do-lowercase-version`, `undo`, `revert-line`, `set-mark`, `exchange-point-and-mark`, `skip-csi-sequence`, `dump-functions`, `dump-variables`, `dump-macros`, `execute-named-command`, `emacs-editing-mode`, `vi-editing-mode` | Compatible | Observable editing, reload and dump behavior; `do-lowercase-version` rings when the key has no case variant, empty or exhausted `undo` rings once, `revert-line` with no changes rings once, `abort` rings while preserving the line, and `exchange-point-and-mark` with no mark stays silent. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `prefix-meta` | Sushline extension | Sushline-only command; no default key binding, so it affects behavior only when explicitly bound in inputrc. Physical ESC input is separate. Tests: [`keymap/tests.rs`](readline/src/keymap/tests.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `arrow-key-prefix` | Compatible | Accepted CSI-skip command. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `display-shell-version`, `tty-status` | Hook-backed | Status supplied by hooks. Tests: [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `shell-expand-line`, `spell-correct-word`, `edit-and-execute-command` | Hook-backed | Application services use hooks. Tests: [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| Application command bindings (`bind -x`) | Hook-backed | BindApi stores, queries, unbinds and prints; dispatch uses `Hooks::on_command`. Tests: [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `tilde-expand` | Compatible | Current-word expansion including whitespace, assignment-like words, and `~+`/`~-`; named-user trailing-slash handling is included. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `character-search`, `character-search-backward` | Compatible | Editor search behavior; no match rings once. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `insert-comment` | Compatible | Inserts/toggles `comment-begin` and accepts. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### Vi Command Names

| Command(s) | Status | Notes |
| --- | --- | --- |
| `vi-append-eol`, `vi-append-mode`, `vi-insert-beg`, `vi-insertion-mode`, `vi-movement-mode`, `vi-editing-mode` | Compatible | Mode behavior. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-arg-digit`, `vi-search`, `vi-search-again`, `vi-char-search` | Compatible | Non-incremental `/` and `?`, cursor-exclusive backward execute, repeat and persistent `f/F/t/T` target; vi search remains case-sensitive with `search-ignore-case` on, and vi character search with no match rings once. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `vi-bWord`, `vi-backward-bigword`, `vi-back-to-indent`, `vi-first-print`, `vi-backward-word`, `vi-bword`, `vi-prev-word`, `vi-column`, `vi-eWord`, `vi-end-bigword`, `vi-end-word`, `vi-eword`, `vi-fWord`, `vi-forward-bigword`, `vi-forward-word`, `vi-fword`, `vi-next-word`, `vi-match` | Compatible | Punctuation/bigword, count, operator, column and bracket behavior. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `vi-change-case`, `vi-change-char`, `vi-replace`, `vi-change-to`, `vi-delete`, `vi-delete-to`, `vi-subst`, `vi-yank-to` | Compatible | Operator, change, replacement and redo. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-overstrike`, `vi-overstrike-delete`, `vi-rubout`, `vi-put`, `vi-redo`, `vi-undo`, `vi-yank-pop` | Compatible | Editor behavior; last change persists across reads; empty `vi-undo` or `vi-put` rings once, while empty `vi-redo` stays silent. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-fetch-history`, `vi-eof-maybe`, `vi-goto-mark`, `vi-set-mark`, `vi-tilde-expand`, `vi-unix-word-rubout`, `vi-yank-arg` | Compatible | Fetch, EOF, tilde, mark, default register-command, rubout and numeric yank-arg behavior; `vi-goto-mark` with no mark rings once, with `vi-tilde-expand` trailing-slash handling as in the `tilde-expand` row above. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-edit-and-execute-command` | Hook-backed | External edit behavior uses `Hooks::edit_and_execute`. Tests: [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `vi-set-register` | Sushline extension | Named vi registers are unavailable by default; enable by setting `sushline-vi-registers` to `on` in inputrc. Tests: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`bind/tests.rs`](readline/src/bind/tests.rs). |

## Readline Init File and Variables

### Init Syntax

| Feature | Status | Notes |
| --- | --- | --- |
| Blank lines and `#` comments | Compatible | Byte-detected, including non-UTF-8 comment bytes. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Inputrc file encoding | Compatible | Files and includes are read as bytes; raw and escaped high-bit key bytes bind alike. Trailing text after complete commands or quoted macro values is ignored, `set` values are not truncated, and invalid bytes in command names or quoted bodies are skipped. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs). |
| `set variable value` | Compatible | Recognized variables normalized. Unknown variable names, and values that GNU Readline rejects for `bell-style`, `comment-begin`, `editing-mode`, and `keymap`, are reported with GNU's diagnostic text before the next prompt, and the directive is ignored; the built-in terminal writes the diagnostics to standard error, like GNU Readline, and a custom backend keeps them in its own output stream. GNU reads the value without its surrounding double quotes, so `""` is the empty value and a rejected value is reported unquoted. `bell-style` accepts `none`, `visible`, `audible`, `on`, and `off` in any case, `editing-mode` accepts any value that starts with `vi` or `emacs`, and GNU's canonical values are stored (`on` to `audible`, `off` to `none`, `vi-command` to `vi` with the derived `keymap` variable `vi-insert`). Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| Key bindings by key name or quoted key sequence | Compatible | Functions and macros. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Recursive inputrc macros | Compatible | Direct and indirect macro cycles discard the recursive suffix with one nesting diagnostic and bell, preserve following input, and accept a later macro. The nesting diagnostic goes to the terminal's diagnostic sink, which the built-in terminal points at standard error like GNU Readline, while a custom backend keeps it in its own output stream. A diagnostic that does not reach the terminal's screen leaves the display untouched, so the redisplay after it starts at the rendered line again. Keyboard-macro replay/count semantics are unchanged. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Escape sequences `\C-`, `\M-`, `\e`, `\\`, `\"`, `\'`, `\a`, `\b`, `\d`, `\f`, `\n`, `\r`, `\t`, `\v`, octal, hex | Compatible | Decoded through `KeySequence`. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| `$if`, `$else`, `$endif` | Compatible | Mode, term, version and application conditions; arbitrary variable comparisons are inactive. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| `$include` path rules | Compatible | CWD-relative includes, `~/` expansion via `HOME`, unexpanded literal `$HOME`, and skipping missing includes. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| `~user` and recursive `$include` | Known deviation | Acyclic include chains apply. In an interactive terminal a recursive cycle kills GNU's shell with a segmentation fault (wait status 139) before the first prompt, while Sushline fails the inputrc load with the depth-limit error (`inputrc include depth exceeded`), which the editor records and continues from. `~user` handling uses best-effort pure-Rust lookup and skips without account data when unavailable. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs). |
| Unsupported `$` directives | Compatible | Unknown directives ignored. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Unknown function names in key bindings | Compatible | Ignored; later lines continue. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Init file load errors during editor construction | Compatible | `Editor::new` retains, `try_new` returns, explicit load/reload reports errors. Tests: [`inputrc.rs`](readline/src/inputrc/tests.rs). |

### Variables

| Variable(s) | Status | Notes |
| --- | --- | --- |
| `editing-mode`, `keymap` | Compatible | Select mode or target binding map. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `active-region-start-color`, `active-region-end-color`, `enable-active-region` | Terminal-backed | Region display uses backend; default on requires non-empty `clear`, `smso`, and `rmso` or `sgr0`, otherwise off; explicit inputrc overrides. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `bell-style`, `prefer-visible-bell` | Compatible | Audible/visible/none via terminal abstraction. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `bind-tty-special-chars` | Compatible | Explicit inputrc/`bind` overrides win for terminal-special keys; with the variable off, the macro wins. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `blink-matching-paren` | Terminal-backed | `()` blink matches the cursor position after the timeout using save/absolute/restore rather than incremental motion, including wrapped/scrolled openers. Offscreen behavior is Sushline-only. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `colored-completion-prefix` | Terminal-backed | Shared-prefix highlighting; fallback prefix style matches Bash output with file-style suffix. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `colored-stats` | Terminal-backed | Directory/file/executable/symlink LS_COLORS styles with directory, executable, and symlink fallbacks and no regular-file style; duplicate `fi` uses the last entry and `ln=target` uses target style, with directory slash outside SGR and executable `*` after reset. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `visible-stats` | Terminal-backed | `/`, `*`, `@` markers for directory, executable and symlink; regular file unmarked. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `comment-begin` | Compatible | Used by `insert-comment`. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `completion-display-width` | Compatible | Width behavior matches first-row placement. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `print-completions-horizontally` | Compatible | Horizontal and vertical first-row placement behavior. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `page-completions` | Compatible | Controls `--More--` pagination independently of the `completion-query-items` confirmation; `q` stops listing while preserving trailing input. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`editor/tests/completion.rs`](readline/src/editor/tests/completion.rs), [`completion.rs`](readline/tests/completion.rs). |
| `completion-query-items` | Compatible | Threshold confirmation behavior; acceptance and rejection preserve trailing input. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`editor/tests/completion.rs`](readline/src/editor/tests/completion.rs), [`completion.rs`](readline/tests/completion.rs). |
| `completion-prefix-display-length` | Compatible | Prefix abbreviation behavior. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `completion-ignore-case` | Compatible | ASCII, accented-pair, and special-case filename matching; without general Unicode case-fold equivalence. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `completion-map-case` | Compatible | Separator mapping requires both this and `completion-ignore-case`, and successful completion preserves candidate spelling. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `expand-tilde` | Compatible | Unquoted `~/` preserves spelling when off and inserts the resolved path when on, while quoted `~/` expands with either setting; invalid/stale `PWD` falls back to cwd and valid logical symlink `PWD` is retained. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `mark-directories` | Compatible | Insertion and listing-display marking for regular and symlinked candidates. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `mark-symlinked-directories` | Compatible | Symlink insertion requires both this and `mark-directories`; defaults are off/on respectively. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `match-hidden-files` | Compatible | Hidden and visible candidate handling including `.`/`..` segments. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `show-all-if-ambiguous` | Compatible | Prefix behavior with bells and displayed candidates. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `show-all-if-unmodified` | Compatible | Common-prefix behavior. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `disable-completion` | Compatible | Setting off inserts the common prefix with a bell; on leaves the typed prefix silent. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `skip-completed-text` | Compatible | On consumes matching suffixes and closers while partial or unmatched suffixes remain; case-insensitive matches do not consume byte-different suffixes. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`editor/tests/completion.rs`](readline/src/editor/tests/completion.rs), [`completion.rs`](readline/tests/completion.rs). |
| `menu-complete-display-prefix` | Compatible | Display-prefix off inserts the selected item; on leaves the shared prefix with one bell on first motion. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `convert-meta`, `input-meta`, `meta-flag`, `output-meta` | Terminal-backed | Defaults follow the locale with chunk-invariant conversion; raw high-bit input survives with conversion off and rendering follows output-meta. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `force-meta-prefix` | Compatible | With `convert-meta`, controls Meta binding representation; unbound ESC+printable rings once and processing continues, while redundant bare ESCs in vi command mode are silent; CSI/SS3 fallback is out of scope. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `enable-meta-key` | Terminal-backed | Defaults on with backend notification on setup/re-entry/cleanup; the built-in terminal emits no meta-mode sequences and custom backends own behavior. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `echo-control-characters`, `byte-oriented` | Terminal-backed | Byte-oriented and echo-control-characters rendering with unconditional TAB expansion. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `enable-bracketed-paste` terminal mode | Compatible | DEC mode sequences with terminfo-based defaults and explicit on/off overrides. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`variables.rs`](readline/tests/variables.rs). |
| Complete bracketed-paste frames | Compatible | Injected frames are accepted without a bell and set the mark; a lone unbound end marker rings once and redispatches its final byte. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`variables.rs`](readline/tests/variables.rs). |
| Bracketed-paste CR normalization and incomplete streams | Implemented | Completed pastes normalize embedded carriage returns to line feeds; split and truncated terminators buffer partial payloads in the editor. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `enable-keypad` | Compatible | Defaults off; explicit on emits available terminfo sequences around editing with explicit off emitting none. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`terminfo.rs`](readline/src/terminal/terminfo.rs). |
| `emacs-mode-string`, `vi-cmd-mode-string`, `vi-ins-mode-string`, `show-mode-in-prompt` | Compatible | Prompt rendering. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `horizontal-scroll-mode` | Known deviation | The window, the cursor, the hysteresis, and the restoration match GNU for ASCII when the input arrives one byte at a time. GNU Readline counts the scroll offset in bytes in every path: at 80 columns a batched input shows the last 51 bytes of the line with the cursor one column past the window's column width, and the bytewise path scrolls to keep the cursor at the window's right edge, so an ASCII tail fills the screen (76 columns, the cursor at column 77) while a narrow multibyte tail leaves only the last 51 bytes visible (50 columns, the cursor at column 51). Sushline's display layer counts the scroll offset in bytes as well, so once a window exists its window and cursor match GNU's for the ASCII and the narrow-multibyte tails. Three differences remain. The window starts when the cursor's byte position in the line reaches the edge, the prompt included, so a multibyte prompt or tail scrolls a window where a cell count would not: 76 text bytes behind the two-byte prompt `P>`, and 74 behind the four-byte prompt `é> `. End to end the built-in terminal reads one byte per event, so pasted or piped input scrolls like bytewise input and lands on a different window than GNU's batch: in the combining-mark and ambiguous-width fixtures GNU's cursor reaches column 50 or 51 and Sushline's column 65. In the wide-character-tail fixture, GNU's window holds the last 51 bytes with the cursor at column 52, while Sushline's bytewise path keeps 64 `b`s visible with the cursor at column 65, which is the input-granularity difference above. In the bytewise wide-tail fixture the two editors show different amounts of the line, GNU 46 leading digits against Sushline's 49, and their cursor columns, 49 and 52, follow from that difference. Where the window's edge falls inside a glyph, GNU's frame is invalid UTF-8 because the edge splits the character, while Sushline clips the cell and keeps the output valid UTF-8. Unmarked ANSI prompt escapes count as logical columns. Tests: [`oracle/horizontal_scroll.rs`](readline/tests/oracle/horizontal_scroll.rs), [`display.rs`](readline/tests/display.rs), [`horizontal_scroll.rs`](readline/src/display/horizontal.rs). |
| `history-preserve-point`, `history-size`, `mark-modified-lines`, `revert-all-at-newline`, `isearch-terminators`, `keyseq-timeout` | Compatible | Editor/history/display/input behavior. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `search-ignore-case` (editor searches) | Compatible | Readline editor searches use owned-locale lowercase matching for valid UTF-8 in supported UTF-8 locales with ASCII byte folding otherwise. Vi `/` and `?` remain case-sensitive. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`state/search.rs`](readline/src/state/search.rs). |
| `search-ignore-case` (public History byte API) | Sushline extension | The explicit public `History` byte-search API folds ASCII only; it is separate from the editor inputrc variable, which uses locale-aware matching. Tests: [`history.rs`](history/tests/history.rs). |
| `histchars`, `history-word-delimiters`, `history-no-expand-chars`, `history-quotes-inhibit-expansion` | Sushline extension | These are not GNU Readline variables and inputrc ignores them; `HistoryChars` and `HistoryExpansionPolicy` are explicit Rust API configuration, with GNU defaults. Tests: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`history.rs`](history/tests/history.rs). |

## History Expansion API

| Feature | Status | Notes |
| --- | --- | --- |
| Event designators `!!`, `!n`, `!-n`, `!string`, `!?string[?]`, `!$`, `!^`, `!:`, `!#` | Compatible | Common `!string` prefix termination and omitted-`?` needle handling; `search_delimiters` adds `!string` terminators only and is not inputrc-configurable. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Quick substitution `^old^new^` | Compatible | Previous history entry. Tests: [`history.rs`](history/tests/history.rs). |
| Word designators `0`, `n`, `^`, `$`, `%`, `x-y`, `*`, `x*`, `x-` | Hook-backed | Quoted/escaped word, shell-construct, and `%` position behavior; exact tokenization and status can come from `Hooks::expand_history`. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Modifiers `h`, `t`, `r`, `e`, `q`, `x`, `s/old/new/`, `&`, `g`, `a`, `G` | Compatible | Path and substitution modifiers operate on event text; the last `:q` or `:x` mode applies once after the modifier chain, and parsed substitutions update remembered state. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Substitution and search memory across calls | Compatible | `HistoryExpansionState` retains substitution and search memory with closed failure handling. Tests: [`history.rs`](history/tests/history.rs). |
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
| List management: `add_history`, `add_history_time`, `remove_history`, `replace_history_entry`, `clear_history`, `stifle_history`, `unstifle_history`, `history_is_stifled` | `push`, `push_bytes`, `add_time`, `remove`, `replace`, `clear`, `stifle`, `unstifle`, `is_stifled` | Compatible | Empty entries, timestamp targeting, stifle limits and replacement/undo/timestamp behavior; replace keeps the cursor, removing an invalid index is a cursor-preserving no-op, and auto-add skips empty lines. Tests: [`history.rs`](history/tests/history.rs). |
| List information: `history_list`, `where_history`, `current_history`, `history_get`, `history_get_time`, `history_total_bytes` | `entries`, `where_history`, `current_history`, `get`, entry `timestamp`, `total_bytes` | Compatible | Rust list, position, timestamp and byte count. Tests: [`history.rs`](history/tests/history.rs). |
| Navigation: `history_set_pos`, `previous_history`, `next_history` | `set_pos`, `previous_history`, `next_history` | Compatible | Rust-owned position navigation. Tests: [`history.rs`](history/tests/history.rs). |
| Search: `history_search`, `history_search_prefix`, `history_search_pos` | `history_search_bytes`, `history_search_prefix`, `history_search_pos` | Compatible | Valid positions are inclusive with directional occurrence selection; failed searches preserve cursor and out-of-range `history_search_pos` falls back to the current cursor. History API evidence only. Tests: [`history.rs`](history/tests/history.rs). |
| Case-insensitive search helper | `history_search_bytes_with_case` | Sushline extension | Explicit ASCII-only case folding; GNU History search has no equivalent option. Tests: [`history.rs`](history/tests/history.rs). |
| Files: `read_history`, `write_history`, `append_history`, `history_truncate_file`, default `~/.history` filename | `read_file`, `load_file`, `write_file`, append variants, `truncate_file`, default path/helpers | Mixed | Regular/special files, timestamps and `~/.history` helpers; concurrent writers are last-writer-wins, timestamp delimiters use the raw `#`-plus-digits prefix with lossy U+FFFD metadata handling, and truncate counts logical entries. Tests: [`history.rs`](history/tests/history.rs). |
| File range: `read_history_range` | `read_file_range`, `load_file_range` | Compatible | Range and non-regular input behavior; FROM/TO count timestamp-joined logical entries. Tests: [`history.rs`](history/tests/history.rs). |
| Expansion: `history_expand` | `expand_history`, status/state variants, `HistoryExpansionState`, `Hooks::expand_history` | Compatible | Text and print-only status; Editor context forwards session state to hooks. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Expansion helpers: `get_history_event`, `history_tokenize`, `history_arg_extract` | `get_history_event`, `history_tokenize`, `history_arg_extract`, `command_words` | Compatible | Rust helpers exposed. Tests: [`history.rs`](history/tests/history.rs). |
| Variables: `history_base`, `history_length`, `history_max_entries` | `HistoryState` and methods | Compatible | Rust-owned offset, length, stifle and maximum-entry behavior. Tests: [`history.rs`](history/tests/history.rs). |
| Variables: `history_expansion_char`, `history_subst_char`, `history_comment_char`, `history_word_delimiters`, `history_no_expand_chars`, `history_quotes_inhibit_expansion` | `HistoryChars`, `HistoryExpansionPolicy` | Compatible | Available to expansion API and editor expansion. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_write_timestamps` | Timestamp write/append methods | Compatible | Enabled or suppressed per call. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_quoting_state` | `HistoryExpansionPolicy::quote_state` | Compatible | Exposed through Rust policy object. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_inhibit_expansion_function` | `expand_history` inhibit predicate | Compatible | Per-call predicate rather than process-global pointer. Tests: [`history.rs`](history/tests/history.rs). |
