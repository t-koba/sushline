# Readline and History Compatibility

Sushline targets GNU Readline 8.3 patch level 0 and GNU History Library
observable behavior within its Rust editor and history APIs. Bash language,
shell runtime, builtin compatibility, and the C ABI are out of scope. Claims
below are limited to the cited oracle tests and their asserted fixtures;
test modules point to evidence, not complete feature coverage.

### Oracle identity

The CI oracle host is GNU Bash 5.3 patch level 0, built from the pinned
release tarball without later standalone Bash patches and linked against
GNU Readline 8.3 built from its upstream release tarball. The local
oracle-suite gate accepts only Bash major.minor 5.3; a later patch level on
`PATH` can pass, but its observations are not patchlevel-0 evidence.

Readline identity is checked separately from Bash. The suite reads the
release number, version label, and host command list
([`oracle/host_readline.rs`](readline/tests/oracle/host_readline.rs)), names
the measured release ([`oracle/host.rs`](readline/tests/oracle/host.rs)), and
stops on a release outside the claims below. For a statically linked host,
it reads the number and label from the host image's exported or build-time
symbols; otherwise it reads the loaded library. If the release cannot be
read, for example from a non-ELF host executable, the suite reports that
limitation and continues, but the resulting evidence has no release scope.
Its note records the number, label, and command list so observations carry
their measured identity.

The reference host used for the claims links Readline statically and exports
release number `0x0803` (8.3), but its library version label says 8.1. Release
identity rests on the number, corroborated by the host command list's
`execute-named-command` and `export-completions`, the two names added to the
8.3 library function table. Neither is registered by the shell or present in
the installed 8.2 library's function dump: the host carries 8.3 code with a
stale 8.1 label.

### CI reproducibility and evidence scope

CI pins the tarball digests and checks fetched content. Configure options
are recorded in the job log and cache keys; the build fails if the release's
configure rejects an option. Cache keys also identify the host's Readline
selection, preventing reuse of a library or host built another way as the
pinned baseline. CI checks the built library's exported release number,
its linkage to a terminal-capability library, and the pinned host's command
list for both 8.3 additions. The suite reads the same identity signals from
the pinned library and reports them in the job log, so another release
cannot pass as the pinned one.

The 8.3 baseline is supported by successful pinned CI measurements as well
as reference-host observations. CI evidence includes:

- The oracle suite against the pinned Bash/Readline host and the
  output-stream probe against the pinned Readline release, checking the
  output-stream row on 8.3 rather than only the runner's 8.2 library.
- The history range measurement linked to the pinned history library. Its
  loader guard reported the pinned prefix for every loaded Readline and
  History library, establishing that the pinned 8.3 range call produced the
  file-range row's fixture entries, including the two cases where GNU and
  Sushline read the file differently.
- The history timestamp measurement linked to the pinned history library,
  with the same library-naming loader guard, measuring the list-information
  row's rule against the target release rather than only the runner's
  library.
- A direct `readline(prompt)` call linked to the pinned library, checking
  output bytes for literal shell-looking sequences and Readline marker
  delimiters, with release-number and loader-path checks.

These measurements establish only their asserted fixture scope; they do
not extend the compatibility claims beyond the evidence cited below.

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
| Hook-backed | The command or mechanism is implemented; the GNU-equivalent embedder-owned state or behavior is supplied through `Hooks`. |
| Not implemented | In the compatibility target, but missing or effectively inert. |
| Untested | Implemented or partially implemented, but not verified enough to classify. |

**Oracle qualifier:** Bash PTY comparisons are runtime observations against the
tested Bash 5.3 host and the Readline release it carries, not GNU source proof,
and establish only the tested fixture scope.

Redisplay checkpoints for ordinary complete Emacs keys use a causal Ctrl-G
abort/BEL barrier in both PTYs, capturing output strictly before BEL rather
than waiting for silence. Steps must not themselves ring or leave pending
input state; the fixtures use the default Emacs abort binding and explicitly enable audible bells.
Sushline's built-in terminal reads the input the terminal holds as one batch,
and the editor dispatches the batch character by character and redraws it
once, like GNU Readline, which processes the available input character by
character and redisplays once. A key that changes the display without running
a command, like the query of an incremental search, redraws as well: GNU
Readline redraws the search display for every query key, and a Bash 5.3 PTY
observation shows that the host's batched write and its split write of a
search key with its query produce the same output. The editor draws the
pending text before a key that interrupts it, so the redisplay still precedes
the abort bell and the barrier holds. An unfinished paste cannot use this
barrier because it consumes Ctrl-G as payload. Paste PTY tests therefore checkpoint only after
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
| Application expansion and embedder state commands | Expansion, spelling, shell version, TTY status, external editing, and `bind -x` depend on application state. | Context-carrying hooks provide these responsibilities and return edits/output to Readline command flow. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). | Hook-backed |
| Legacy `term=` inputrc conditions | With `TERM=xterm-256-color`, pinned GNU Readline 8.3 matches `term=xterm` but not `term=xterm-256`. | Sushline matches these two exact cases; no general terminal-name matching rule is claimed. Evidence: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`readline-inputrc-term.expected`](.github/scripts/readline-inputrc-term.expected), [`readline-inputrc-variable-rhs.sh`](.github/scripts/readline-inputrc-variable-rhs.sh). | Compatible |
| Cell width and locale | UTF-8 width and cursor motion depend on character and locale handling. | Static `char_width` uses UAX #11; motion follows `byte-oriented`/locale, preserving interior bytes. Compatible status rests on Bash 5.3 PTY runtime evidence. The separate pinned 8.3 C.UTF-8 probe covers only e + U+0301, U+00B7, U+754C, and U+2708 + U+FE0F input acceptance, byte positions and raw movement output. Without a terminal emulator it establishes neither physical columns nor a general grapheme-width rule. Evidence: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/horizontal_scroll.rs`](readline/tests/oracle/horizontal_scroll.rs), [`readline-unicode-width.sh`](.github/scripts/readline-unicode-width.sh), [`editor/tests/lifecycle.rs`](readline/src/editor/tests/lifecycle.rs), [`buffer/tests.rs`](readline/src/buffer/tests.rs). | Compatible |
| Unwrapped ANSI in prompts | Readline interprets invisible markers, not shell prompt escapes; marked and unmarked ANSI prompts redisplay differently. | `Prompt::new` preserves literal bytes except Readline markers. Opt-in `PromptEscapes::Shell` handles `\\`, `\a`, `\e`, `\n`, `\r`, nonprinting `\[`/`\]`, exactly three octal digits after a backslash, and shell post-expansion backslash/quote/dollar/backtick quoting (including runs of backslashes); unknown escapes, parameter/command expansions and shell-state escapes such as `\u`, `\w`, `\t` remain literal. Octal values above 0x7f become Unicode code points, not lone bytes, because prompts are UTF-8 strings. Sushline preserves the measured marked/unmarked column distinction and final cursor columns by repainting, not by matching incremental bytes. The pinned 8.3 direct-library probe establishes literal shell-looking prompt bytes and removal of invisible markers only; redisplay and backslash-run/quoting comparisons are separate Bash 5.3 PTY runtime fixtures, limited to their measured strings. Escaped dollar and backtick remain literal. Horizontal scrolling has its own deviation below. Evidence: [`oracle/display.rs`](readline/tests/oracle/display.rs), [`prompt/tests.rs`](readline/src/prompt/tests.rs), [`ci.yml`](.github/workflows/ci.yml). | Implemented |
| Region/display/terminal internals | Redisplay and active regions depend on terminal capabilities; the library defaults to stdout, while Bash redirects editor output to stderr. | `TerminalIo` owns display; the built-in backend supports Unix terminals and Windows consoles with VT output (requiring console stdin/stdout); other non-Unix targets return `Unsupported` for live operations unless replaced. It dispatches available input character by character and redraws once per batch, including incremental-search query changes; accepted-line suffix bytes remain queued. Diagnostics, expansion failures and print-only `history-expand-line` output use stderr in the built-in backend, matching the Bash-hosted command (not the shell `history -p` stdout sink). Custom backends choose `write_diagnostic` (default: terminal output); `diagnostics_use_terminal` identifies whether that sink is the terminal screen; off-screen diagnostics leave the display untouched. On Unix, raw mode preserves `IXON`/`IXOFF` and normalizes input flags. The pinned 8.3 synthetic `rl_prep_terminal(meta_flag)` fixture covers CS8 and CS7 with input-meta off/on: CS7 clears `ISTRIP`/`INPCK` only with input-meta on. It establishes no live CS7 PTY behavior. The built-in display uses stdout, matching direct-library probes on 8.2 and pinned 8.3 but differing from Bash under stdout redirection. Exact bytes depend on backend. Evidence: [`oracle/display.rs`](readline/tests/oracle/display.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs), [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`oracle/diagnostics.rs`](readline/tests/oracle/diagnostics.rs), [`variables.rs`](readline/tests/variables.rs), [`editor/tests/lifecycle.rs`](readline/src/editor/tests/lifecycle.rs), [`terminal.rs`](readline/src/terminal.rs), [`ci.yml`](.github/workflows/ci.yml). | Terminal-backed |
| Live PTY resize redisplay | Bash 5.3 carrying pinned Readline 8.3 redraws the tested inputs in four Linux PTY shrinks (three ASCII, one single-width C.UTF-8), four ASCII growths, and one two-step ASCII 80→12→24 fixture. Claims cover only these fixtures. | The paired cases in [`oracle/display.rs`](readline/tests/oracle/display.rs) compare Bash and Sushline final cleared rows, cursor, and accepted bytes. Three 80-to-12 ASCII shrinks use the 16-byte line with point at end, after byte 13, and after byte 10 (the initial wrap boundary); all finish with rows `P>abcdefghij` / `klmnop` and cursors `(1, 6)`, `(1, 3)`, and `(1, 0)`. The C.UTF-8 shrink uses `abcdefghiéjklmno`, with point after nine ASCII characters and `é`; rows are `P>abcdefghié` / `jklmno`, cursor `(1, 0)`, and the required locale must be present. Four 12-to-80 ASCII growths cover a 10-byte row-filling line, a wrapped 16-byte line at end, and points after byte 10 (the initial wrap boundary) and byte 13; cursors are `(0, 12)`, `(0, 18)`, `(0, 12)`, and `(0, 15)`. The boundary growth ends with `P>abcdefghijklmnop`. The 80→12→24 fixture starts with the 16-byte ASCII line and point after byte 10; after the second resize its final row is `P>abcdefghijklmnop`, cursor `(0, 12)`, and accepted bytes match the original input. A causal Ctrl-G/BEL barrier follows the first automatic redisplay; only the final resize uses Ctrl-L/BEL for the cleared-screen checkpoint. These results establish no resize sequence beyond the listed fixtures, general redraw-byte parity, UTF-8 width beyond the single tested character, or terminal-emulator reflow. Evidence: [`oracle/display.rs`](readline/tests/oracle/display.rs), [`ci.yml`](.github/workflows/ci.yml). | Implemented |
| Event / input-available hooks | GNU provides idle event/input hooks and keyboard-input timeouts. The pinned GNU Readline 8.3 direct-library `rl_event_hook` probe in the “Measure an idle event hook against the pinned release” step of [`ci.yml`](.github/workflows/ci.yml) invokes a self-unregistering hook exactly once before the parent supplies input, then `readline` returns `probe` with exit status 0, stdout `idle> probe\n` and stderr `readline_release=8.3\nhook=1\nreturned=probe\nhooks=1\n`. That idle-event fixture establishes only its invocation and returned line; it does not measure `rl_input_available_hook`. The separate pinned GNU Readline 8.3 direct-library probe in the “Measure one input-available hook against the pinned release” step establishes actual PTY input readiness when the parent supplies input at the pre-input handshake; `rl_input_available_hook` is invoked exactly once, reports `available=1` and returns 1. The C probe’s `readline` call returns `probe`; the process exits 0 with stdout `available> probe\n`. These fixture-scoped measurements establish neither periodic cadence nor general event-loop equivalence. | No implicit idle callback: embedder policy uses `TerminalIo::read_event` timeout/poll and `Hooks::check_signals`; `keyseq-timeout` drives the read timeout. Tests: [`variables.rs`](readline/tests/variables.rs). | Terminal-backed |
| Signal install/catch/reraise | Pinned 8.3 direct-library PTY fixtures cover saved `SIGWINCH` chaining/restoration and caller-handler, default and ignored dispositions for `SIGINT`/`SIGQUIT`. | Built-in `Terminal` independently enables eight signals and `SIGWINCH` (both on by default), preserves `SIG_IGN`, restores dispositions, chains `SIGWINCH`, and allows only one raw-mode terminal (`WouldBlock` otherwise). Interrupted reads recheck hooks; non-INT signals restore/raise/re-enter. After terminal/signal cleanup, a genuinely caught SIGINT is re-delivered once to the saved caller handler or, by default, `SIG_DFL`, matching GNU’s tested default-disposition termination. Fatal redelivery prevents either `SigintPolicy` result. Explicit `set_reraise_sigint_default(false)` opts out for `SIG_DFL` and leaves the read to Continue/Interrupted. Default `SigintPolicy::Continue` echoes `^C` and continues with partial/queued input if redelivery returns or is opted out. Ignored SIGINT stays ignored. Hook-only reports and unquoted Ctrl-C bytes follow read policy without redelivery. Explicit Interrupted discards the partial line but queues suffix bytes if redelivery returns; custom backends default to Interrupted unless configured otherwise. With catching disabled, OS dispositions remain caller-owned. `SIGQUIT` follows host signal policy, not key-binding precedence. The direct-library fixtures establish one saved-handler callback and restoration, default-disposition termination and ignored-disposition continuation only; the `SIGWINCH` direct-library fixture establishes no resizing/redisplay. No general Bash or C ABI signal equivalence is claimed. Bash interrupt oracle tests separately select Interrupted. Evidence: [`signals.rs`](readline/tests/signals.rs), [`editor/tests/signals.rs`](readline/src/editor/tests/signals.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`ci.yml`](.github/workflows/ci.yml). | Implemented |
| Completion redraw of invalid filename bytes | For one invalid filename byte (`0xFF`), default redraw is `\377` under `LC_ALL=C` and raw `0xFF` under `LC_ALL=C.UTF-8` when that locale is available; explicit `output-meta on` lists the raw byte in both tested locales. | Sushline matches these exact PTY cases, and accepted-line bytes remain raw in the same locales. No broader locale rule is established. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Compatible |
| Undo insert grouping | GNU joins single-byte inserts into a prior insert entry up to 20 characters; longer and multibyte inserts begin entries. | Self-insert, literal, and tab inserts follow the 20-byte single-byte rule; multibyte scalars begin their own entries and incomplete input is buffered across events. The Bash PTY oracle compares one undo after 20 and 21 single-byte inserts and `aé`, yielding an empty line, 20 `a`s, and `a`, respectively; after `ab` plus `M-5 a`, one undo leaves `ab` and a second empties the line. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). | Compatible |

## High-Level Coverage

| Area | Status | Implemented surface | Boundary notes |
| --- | --- | --- | --- |
| Basic line editing | Mixed | Insert/delete/movement, undo/overwrite, marks/regions, macros, kill/yank, transpose, case conversion. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). | Shell-word classes are hookable; terminal bytes depend on backend; character motion follows the boundary policy above. |
| Emacs keymap, bindable names, and `bind` | Mixed | Keymaps, inputrc bindings/macros, numeric arguments, command names and bind output/storage/query. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). | `bind -x` execution and application state are hook-backed. |
| vi mode | Mixed | Insert/command mode, movement/operators, marks, redo/search/put/yank and completion bindings. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). | External editing is hook-backed. |
| Init file/inputrc | Mixed | `set`, bindings/macros, conditionals and includes, including numeric arguments and abort in macros. Variable-RHS comparison is limited to the cited `comment-begin` fixture, not general inputrc or shell grammar: in its five pinned `==` cases, stored `a` matches RHS `a` and `a b` but not `"a"`, while stored `a b` matches neither `a b` nor `"a b"`. | Arbitrary shell/application state is not inferred. Evidence: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`readline-inputrc-variable-rhs.expected`](.github/scripts/readline-inputrc-variable-rhs.expected). |
| Completion | Mixed | Default, filename, menu, listing, insertion, export, display, and options. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Application names/quoting are hook-backed. Invalid-byte redraw follows the boundary row above. |
| History navigation/search | Mixed | Previous/next, prefix/substring and incremental/non-incremental search. Bounded raw History offset/position fixtures and Readline locale/input-handling fixtures are detailed under [History Library Surface](#history-library-surface) and [editor search evidence](#editor-search-evidence). Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`history/tests/history.rs`](history/tests/history.rs). | Vi `/` and `?` remain case-sensitive; the public History byte API is ASCII-only for case-insensitive search. With `revert-all-at-newline` off, edited text survives the tested navigation, prefix and search-overlay traversals; this is Bash 5.3/Readline 8.3 runtime evidence, not exact Bash patchlevel-0 or all-path coverage. A Sushline unit test also covers restoration of edit undo state. |
| History expansion | Mixed | Event/word designators, modifiers, quick substitution, policy, quote state and inhibition; bounded direct-library and Bash-hosted evidence is detailed under [History Expansion API](#history-expansion-api). Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`history/tests/history.rs`](history/tests/history.rs), [`editor/tests/history.rs`](readline/src/editor/tests/history.rs). | Alias expansion and exact application lexer state are hook-backed; substitution/search memory belongs to `HistoryExpansionState`. Editor quote inhibition and diagnostic/output sinks are separate from pure History defaults. |
| History file storage | Mixed | Read/write/append/range/truncate, timestamps, multiline entries and default paths. See [History Library Surface](#history-library-surface) for scoped file evidence and deviations. Tests: [`history/tests/history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). | Defaults preserve command NUL bytes and decoded timestamp suffixes; opt-in `TruncateAtNul` matches only the cited truncation fixtures, not GNU’s synthesized runtime timestamps. Writers remain lossless for stored bytes/text. |

The bounded custom-word-delimiter fixture compares default delimiters with
an explicit comma-containing list for `!!:2` on `echo alpha,beta gamma`.
It preserves punctuation fields rather than treating the list as shell grammar.
The pinned GNU History 8.3 assertions are in
[`history_expand_probe`](.github/workflows/ci.yml); the paired
[`custom_history_word_delimiters_preserve_punctuation_fields`](history/tests/history.rs)
regression asserts selected bytes and Rust status. No other delimiter list
or shell parsing compatibility is claimed.

### Search-delimiter expansion fixture

The pinned GNU History 8.3
[`history_expand_probe`](.github/workflows/ci.yml) and
[`history_expansion_consults_search_delimiters_for_string_prefixes`](history/tests/history.rs)
cover four default/custom-dot delimiter cases after positioning at the newest
of the four seeded entries. The custom dot terminates the `!foo.bar` prefix
search but leaves the `!?bar` substring control unaffected. Exact statuses
and outputs live in those assertions, not a general delimiter-set claim.
`HistoryExpansionPolicy::search_delimiters` is explicit Rust API policy,
not inputrc configuration.

### Current-line event expansion fixture

The pinned GNU History 8.3
[`history_expand_probe`](.github/workflows/ci.yml) and
[`history_expansion_current_line_event_supports_shared_suffixes`](history/tests/history.rs)
cover exactly `printf !#`, `echo a b !#:q`, `echo a b !#:0`,
`echo a b !#:s/a/X/`, and `echo a b !#:$` with their shared three-entry seed.
These validate current-line event suffix handling only for those five inputs;
exact output bytes (including trailing spaces) and statuses are asserted there,
not a general current-line-event/modifier claim.

## User-Facing Readline Commands

GNU command names below come from the Readline User Manual bindable-command
sections. The names the Bash shell registers with Readline itself are the
shell's own rather than GNU Readline commands, and the Readline library
provides none of them: the history expansion names (`magic-space`,
`history-expand-line`, `history-and-alias-expand-line`, `alias-expand-line`),
the shell word names (`shell-forward-word`, `shell-backward-word`,
`shell-kill-word`, `shell-backward-kill-word`, `shell-transpose-words`), the
completion names (`complete-command`, `complete-filename`,
`complete-hostname`, `complete-username`, `complete-variable`,
`complete-into-braces`, `possible-command-completions`,
`possible-filename-completions`, `possible-hostname-completions`,
`possible-username-completions`, `possible-variable-completions`,
`glob-complete-word`, `glob-expand-word`, `glob-list-expansions`,
`dabbrev-expand`, `dynamic-complete-history`, `bash-vi-complete`) and the
application service names (`shell-expand-line`, `spell-correct-word`,
`edit-and-execute-command`, `vi-edit-and-execute-command`,
`display-shell-version`, `insert-last-argument`). The baseline for those
names is the Bash 5.3 host's own command, and the status column gives
Sushline's behavior for every name.

The classification was measured on the development host, whose Bash links
Readline statically and carries the 8.3 release; the scope above states the
signals that release identity rests on. Its `bind -l` output lists the
library's command names and the names the shell adds, and the difference is
the shell's own.
The Readline 8.3 function table is the 8.2 table plus the two commands the
8.3 release notes add (`execute-named-command` and `export-completions`), and
neither table holds a name above, so those two names are commands of the
release this document targets and their rows keep the Readline baseline. The
CI oracle job builds its host against the Readline release this document
names, and the suite names the release its host carries. Deliberate
Sushline-only extensions use the `Sushline extension` status.

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
| `history-search-backward`, `history-search-forward`, `history-substring-search-backward`, `history-substring-search-forward` | Compatible | Direction, repeat, case, abort/accept, bell, and cursor behavior. Case/locale behavior is limited to the pinned fixtures in [Editor search evidence](#editor-search-evidence); those cases do not establish general locale or Unicode-folding behavior. Numeric counts repeat, reverse, or no-op with excessive counts stopping at the nearest match; prefix search skips the selected entry and failed forward search preserves the line; failed navigation rings once. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `magic-space` | Compatible | Built-in expansion uses GNU-default policy; custom characters/policy are direct-API only. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-expand-line` | Compatible | Built-in expansion uses GNU-default policy; custom characters/policy are direct-API only. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-and-alias-expand-line`, `alias-expand-line` | Hook-backed | Alias expansion uses `Hooks::expand_aliases`. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `yank-nth-arg`, `yank-last-arg`, `insert-last-argument` | Hook-backed | Shell words; exact application lexer words may come from `Hooks::shell_words`. Yanking with nothing to yank rings once; an out-of-range or explicitly negative word number rings instead of clamping, and 0 yanks the command word. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `fetch-history` | Compatible | Numbered fetch; an out-of-range number (including 0) silently fetches the first entry, and an empty history stays put. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `operate-and-get-next` | Compatible | Multi-read prefill including numeric arguments. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |

### Text Editing

| Command(s) | Status | Notes |
| --- | --- | --- |
| `end-of-file`, `delete-char`, `backward-delete-char`, `forward-backward-delete-char` | Compatible | Empty-input EOF and delete behavior; empty deletes ring once. Counted/overwrite deletes match, with counted kills yankable. The pinned 8.3 finite-stream fixtures cover empty, unterminated `abc`, and LF-terminated `abc`. Sushline accepts nonempty buffered bytes on backend `UnexpectedEof`, preserving empty EOF; the Rust finite-input regression checks both LF and the terminal Enter event CR. This is stream EOF, not Ctrl-D command dispatch. Evidence: [`readline-stream-eof.expected`](.github/scripts/readline-stream-eof.expected), [`stream_eof.rs`](readline/tests/stream_eof.rs), [`readline-stream-eof.sh`](.github/scripts/readline-stream-eof.sh), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`editing.rs`](readline/tests/editing.rs). |
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
| `digit-argument`, `universal-argument` | Compatible | Multi-digit/sign/universal composition; arguments above 1000000 ring and reset, leaving subsequent digits as ordinary text. The pending `(arg: N) ` indicator replaces the whole prompt, including its mode indicator, until consumed; sign-only, leading-zero and universal-argument displays are covered. Screen comparisons require host Readline 8.3 (8.2 retains the prompt) and report nothing on other releases. These are Bash 5.3 PTY observations. Evidence: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`editor/tests/display.rs`](readline/src/editor/tests/display.rs). |
| `start-kbd-macro`, `end-kbd-macro` | Compatible | A recording without an argument starts a fresh definition, an unfinished recording continues across prompts, and a replay that accepts a line continues in the next prompt. Explicit start and end counts follow the append/replay, no-op, invalidation, abort, and nesting behavior with matching bells and diagnostics. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `call-last-kbd-macro` | Compatible | Default and counted calls, with no-definition and nesting-exhaustion bells. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `print-last-kbd-macro` | Compatible | Recorded macro output. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### Completion Commands And Behavior

| Command/feature(s) | Status | Notes |
| --- | --- | --- |
| `complete`, `possible-completions`, `insert-completions`, `delete-char-or-list` | Compatible | Common-prefix, display/insertion, ambiguity, quoting, and separator behavior with counted `delete-char-or-list` handling. The fallback `-G` matcher treats escaped brackets literally; escaped-hyphen handling is unspecified. Other listing behavior is in the variable rows below. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `complete-command`, `possible-command-completions` | Hook-backed | Executables local; application names use `Hooks::command_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `complete-filename`, `possible-filename-completions` | Hook-backed | Filename quoting, hidden names, directory marks, control/invalid bytes, and listing behavior; application policy uses `Hooks::quote_completion`. Redraw behavior for one invalid filename byte (`0xFF`) is limited to `LC_ALL=C` and `LC_ALL=C.UTF-8` when available; see [Compatibility Boundaries](#compatibility-boundaries) for default and explicit `output-meta` forms. Accepted-line bytes remain raw in those fixtures; no general locale rule is claimed. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `complete-hostname`, `possible-hostname-completions` | Hook-backed | Hooks plus available platform sources. Tests: [`completion.rs`](readline/tests/completion.rs). |
| `complete-username`, `possible-username-completions` | Hook-backed | Hooks plus available platform sources. Tests: [`completion.rs`](readline/tests/completion.rs). |
| `complete-variable`, `possible-variable-completions` | Hook-backed | Application variables use `Hooks::variable_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `menu-complete`, `menu-complete-backward`, `old-menu-complete` | Compatible | Exhaustion, wrap, display-prefix, quoting, and cursor behavior. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`completion.rs`](readline/tests/completion.rs), [`editor/tests/completion.rs`](readline/src/editor/tests/completion.rs). |
| `complete-into-braces` | Compatible | Brace layout, shared prefix, quoting and separator. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `dabbrev-expand`, `dynamic-complete-history` | Hook-backed | History-word behavior; exact words may come from `Hooks::shell_words`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `glob-complete-word`, `glob-expand-word`, `glob-list-expansions` | Hook-backed | Glob behavior; `Hooks::glob_expand` supplies application globbing with locale-aware matching and byte fallback. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `vi-complete` | Compatible | Default vi `*`, `=`, and backslash bindings, including command-mode bounds. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `bash-vi-complete` | Hook-backed | Command completion; application names use `Hooks::command_names`. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). |
| `export-completions` | Compatible | In the `export_completions_bash_fixture_reports_utf8_byte_range_and_accepts_line` PTY fixture, Bash and Sushline report count `3`, prefix `al`, byte range `9:11`, and candidates `alp`, `alpha`, `alpine`; Bash accepts callback args `é al` and Sushline leaves `probe é al` unchanged. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`completion.rs`](readline/tests/completion.rs). |

### Miscellaneous Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `re-read-init-file`, `abort`, `do-lowercase-version`, `undo`, `revert-line`, `set-mark`, `exchange-point-and-mark`, `skip-csi-sequence`, `dump-functions`, `dump-variables`, `dump-macros`, `execute-named-command`, `emacs-editing-mode`, `vi-editing-mode` | Compatible | Observable editing, reload and dump behavior; `do-lowercase-version` rings when the key has no case variant, empty or exhausted `undo` rings once, `revert-line` with no changes rings once, `abort` rings while preserving the line, and `exchange-point-and-mark` with no mark stays silent. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| Explicit `emacs-meta` / `emacs-ctlx` bindings | Compatible for measured Bash-hosted fixtures | Simple/nested meta and ctlx functions/macros dispatch through ESC/C-x. Root and submap rebinding share storage; prefix bindings preserve descendants and unbound continuations use macro fallback. Finite paired fixtures cover reusable relative listings (empty fallback last), nonreusable `-S` escaping/order, function query/list punctuation and cross-view removal. `-S` uses unquoted sequence-byte order; `-s` uses quoted inputrc syntax. Local tests additionally cover override identity, removal and application-command views. These are prefixed standard-map views, not independent maps; no standalone pinned-library or arbitrary-tree/switching claim is made. Evidence: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs), [`keymap/tests.rs`](readline/src/keymap/tests.rs), [`bind/tests.rs`](readline/src/bind/tests.rs). |
| `prefix-meta` | Sushline extension | Sushline-only command; no default key binding, so it affects behavior only when explicitly bound in inputrc. Physical ESC input is separate. Tests: [`keymap/tests.rs`](readline/src/keymap/tests.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `arrow-key-prefix` | Compatible | Accepted CSI-skip command. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `display-shell-version`, `tty-status` | Hook-backed | Status supplied by hooks. Tests: [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `shell-expand-line`, `spell-correct-word`, `edit-and-execute-command` | Hook-backed | Application services use hooks. Tests: [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| Application command bindings (`bind -x`) | Hook-backed | BindApi stores, queries, unbinds and prints; dispatch uses `Hooks::on_command`. Tests: [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `tilde-expand` | Compatible | Current-word expansion including whitespace, assignment-like words, `~+`/`~-`, and named-user trailing-slash handling. See [Tilde Expansion Evidence](#tilde-expansion-evidence) for shared Unix lookup behavior, tests, and fixture-specific results. |
| `character-search`, `character-search-backward` | Compatible | Editor search behavior; no match rings once. The search takes a whole character, so a multibyte target is one target, it is matched like GNU and the rest of the character never reaches the line. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `insert-comment` | Compatible | Inserts/toggles `comment-begin` and accepts. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |

### Vi Command Names

| Command(s) | Status | Notes |
| --- | --- | --- |
| `vi-append-eol`, `vi-append-mode`, `vi-insert-beg`, `vi-insertion-mode`, `vi-movement-mode`, `vi-editing-mode` | Compatible | Mode behavior. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-arg-digit`, `vi-search`, `vi-search-again`, `vi-char-search` | Compatible | Non-incremental `/` and `?`, cursor-exclusive backward execute, repeat and persistent `f/F/t/T` target; vi search remains case-sensitive with `search-ignore-case` on, and vi character search with no match rings once. The vi character search takes a whole character, like GNU, so a multibyte target is one target. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `vi-bWord`, `vi-backward-bigword`, `vi-back-to-indent`, `vi-first-print`, `vi-backward-word`, `vi-bword`, `vi-prev-word`, `vi-column`, `vi-eWord`, `vi-end-bigword`, `vi-end-word`, `vi-eword`, `vi-fWord`, `vi-forward-bigword`, `vi-forward-word`, `vi-fword`, `vi-next-word`, `vi-match` | Compatible | Punctuation/bigword, count, operator, column and bracket behavior. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `vi-change-case`, `vi-change-char`, `vi-replace`, `vi-change-to`, `vi-delete`, `vi-delete-to`, `vi-subst`, `vi-yank-to` | Compatible | Operator, change, replacement and redo. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-overstrike`, `vi-overstrike-delete`, `vi-rubout`, `vi-put`, `vi-redo`, `vi-undo`, `vi-yank-pop` | Compatible | Editor behavior; last change persists across reads; empty `vi-undo` or `vi-put` rings once, while empty `vi-redo` stays silent. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-fetch-history`, `vi-eof-maybe`, `vi-goto-mark`, `vi-set-mark`, `vi-unix-word-rubout`, `vi-yank-arg` | Compatible | Fetch, EOF, mark, default register-command, rubout and numeric yank-arg behavior; `vi-goto-mark` with no mark rings once, a vi mark is named by one lowercase ASCII letter and any other name rings once. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-tilde-expand` | Compatible | Tilde expansion with trailing-slash handling as in `tilde-expand`. See [Tilde Expansion Evidence](#tilde-expansion-evidence) for the current-user fixtures with `HOME` unset, empty, and nonempty, plus the separate named-user current-word vi fixture and tests. |
| `vi-edit-and-execute-command` | Hook-backed | External edit behavior uses `Hooks::edit_and_execute`. Tests: [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `vi-set-register` | Sushline extension | Named vi registers are unavailable by default; enable by setting `sushline-vi-registers` to `on` in inputrc. Tests: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`bind/tests.rs`](readline/src/bind/tests.rs). |

### Tilde Expansion Evidence

On Unix, `tilde-expand`, `vi-tilde-expand`, filename-tilde expansion/completion
and `$include` share NSS-backed home lookup: `getpwuid_r` for the current UID
when `HOME` is absent, and `getpwnam_r` for names. A present `HOME`, including
empty, retains precedence for the current user. Synthetic unit tests cover the
shared selection and HOME-unset command/completion helpers; these are distinct
from the direct measurements below.

Each row is a two-trial direct comparison of Sushline against the pinned GNU
Readline 8.3 library. Current-word commands and filename completion are separate
measurements; the vi command has its own fixture.

| Fixture scope | Result in both trials for each implementation | Controlled lookups and checks | Report artifact |
| --- | --- | --- | --- |
| Current-user current-word `tilde-expand`, `HOME` unset | Matched the synthetic current-UID home. | One passwd lookup per process per trial; guards passed. | `pinned-readline-8.3-home-unset-tilde-expand/readline_tilde_home_unset.out` |
| Current-user current-word `tilde-expand`, `HOME` present but empty | Returned an empty word. | Zero passwd lookups; guards passed. | `pinned-readline-8.3-home-empty-tilde-expand/readline_tilde_home_empty.out` |
| Current-user current-word `tilde-expand`, `HOME` present and nonempty | Matched the test-owned HOME. | Zero passwd-home lookups; guards passed. | `pinned-readline-8.3-home-nonempty-tilde-expand/readline_tilde_home_nonempty.out` |
| Current-user current-word `vi-tilde-expand`, `HOME` unset | Matched the synthetic current-UID home in both trials. | One controlled current-UID passwd-home lookup per implementation per trial; pinned GNU 8.3 identity, guards, and cleanup passed. | `pinned-readline-8.3-home-unset-vi-tilde-expand/readline_tilde_home_unset_vi.out` |
| Current-user current-word `vi-tilde-expand`, `HOME` present but empty | Returned an empty word in both trials. | Zero current-UID passwd-home lookups per implementation per trial; pinned GNU 8.3 identity, stable comparison, guards, and cleanup passed. | `pinned-readline-8.3-home-empty-vi-tilde-expand/readline_tilde_home_empty_vi.out` |
| Current-user current-word `vi-tilde-expand`, `HOME` present and nonempty | Matched the test-owned HOME without a trailing slash in both trials. | Zero current-UID passwd-home lookups per implementation per trial; pinned GNU 8.3 identity, stable matching classifications, fixture-state guards, and cleanup passed. | `pinned-readline-8.3-home-nonempty-vi-tilde-expand/readline_tilde_home_nonempty_vi.out` |
| Direct synthetic named-account current-word `tilde-expand` | Matched the test-owned account home and preserved its trailing slash. | One named-account lookup per run; guards passed. | `pinned-readline-8.3-named-user-tilde-expand/readline_tilde_named_user.out` |
| Direct synthetic named-user current-word `vi-tilde-expand` | Matched the synthetic home and preserved its trailing slash. | One named-account lookup per run; guards and cleanup passed. | `pinned-readline-8.3-named-user-vi-tilde-expand/readline_tilde_named_user_vi.out` |
| Direct `complete-filename`, `expand-tilde on`, one fixed synthetic named-user prefix and one unique test-owned candidate | Resolved the synthetic home and candidate. | One named-home lookup per implementation per trial; GNU's pinned 8.3 library check, all guards, and cleanup checks passed. | `pinned-readline-8.3-named-user-filename-completion/readline_tilde_named_user_filename.out` |
| Direct `complete-filename`, `expand-tilde off`, one fixed synthetic named-user prefix and one controlled candidate | Both implementations resolved the synthetic named-user home and candidate in both trials. | Two named-home lookup hits per implementation per trial; pinned GNU 8.3 identity, stable matching classifications, guards, fixture-state checks, and cleanup passed. | `pinned-readline-8.3-named-user-tilde-off-filename-completion-5384cfce415be711ac392fa5ae7cb616194de366/readline_tilde_named_user_filename_off.out` |
| Synthetic current-UID `~`, child `HOME` absent; `complete-filename` with `expand-tilde on` and one test-owned candidate | Resolved the synthetic current-UID home and candidate. | One controlled passwd-home lookup per implementation per trial; GNU's pinned 8.3 identity, guards, and cleanup passed. | `pinned-readline-8.3-home-unset-filename-completion/readline_tilde_home_unset_filename.out` |
| Current-user `~/`, child `HOME` present and nonempty, and separate synthetic current-UID passwd home; `complete-filename` with `expand-tilde on` and one test-owned candidate | Both implementations resolved the test-owned HOME and candidate in both trials. | Zero current-UID passwd-home lookups per implementation per trial; pinned GNU 8.3 identity, stable matching classifications, fixture-state guards, and cleanup passed. | `pinned-readline-8.3-home-nonempty-filename-completion/readline_tilde_home_nonempty_filename.out` |
| Current-user `~/`, child `HOME` present and nonempty, separate synthetic current-UID passwd home and literal-prefix CWD candidate; `complete-filename` with `expand-tilde off` | Both implementations resolved the test-owned HOME and its controlled candidate in both trials, not the literal relative candidate. | Zero current-UID passwd-home lookups per implementation per trial; pinned GNU 8.3 identity, stable matching classifications, guards, fixture-state checks, and cleanup passed. | `pinned-readline-8.3-home-nonempty-tilde-off-filename-completion-5384cfce415be711ac392fa5ae7cb616194de366/readline_tilde_home_nonempty_filename_off.out` |
| Current-user bare `~` (no slash), child `HOME` present and nonempty; `complete-filename` with `expand-tilde on` | Both implementations retained the literal prefix in both trials; neither resolved HOME, passwd home, or the controlled candidate. | Zero current-UID passwd-home lookups per implementation per trial; GNU emitted exactly one BEL byte on stderr per trial while Sushline stderr was empty; pinned GNU 8.3 identity, stable matching result classifications, guards, fixture-state checks, and cleanup passed. | `pinned-readline-8.3-home-nonempty-bare-tilde-filename-completion-5384cfce415be711ac392fa5ae7cb616194de366/readline_tilde_home_nonempty_filename_bare.out` |
| Current-user bare `~` (no slash), child `HOME` present but empty; `complete-filename` with `expand-tilde on` | Both implementations retained the literal prefix in both trials; neither resolved HOME, passwd home, or the controlled candidate. | Zero current-UID passwd-home lookups per implementation per trial; GNU emitted exactly one BEL byte on stderr per trial while Sushline stderr was empty; pinned GNU 8.3 identity, stable matching result classifications, guards, fixture-state checks, and cleanup passed. | `pinned-readline-8.3-home-empty-bare-tilde-filename-completion-1dc85c2089fcde6dc5768543969828fda1ab6f53/readline_tilde_home_empty_filename_bare.out` |
| Current-user bare `~` (no slash), child `HOME` present and nonempty; `complete-filename` with `expand-tilde off` | Both implementations retained the unescaped literal prefix with trailing slash in both trials; neither selected the test-owned HOME, passwd-home, or literal-prefix CWD candidate. | Zero current-UID passwd-home lookups per implementation per trial; stderr was empty in both; pinned GNU 8.3 identity, stable matching classifications, guards, fixture-state checks, and cleanup passed. | `pinned-readline-8.3-home-nonempty-bare-tilde-off-filename-completion-f27f78a5a033dd30d55c2822e8f6e903b743aa1b/readline_tilde_home_nonempty_filename_bare_off.out` |
| Current-user bare `~` (no slash), child `HOME` unset; `complete-filename` with `expand-tilde off` | Both implementations retained the unescaped literal prefix with trailing slash in both trials; neither selected a candidate. | One controlled current-UID passwd-home lookup per implementation per trial; stderr was empty; pinned GNU 8.3 identity, stable matching classifications, guards, fixture-state checks, and cleanup passed. | `pinned-readline-8.3-home-unset-bare-tilde-off-filename-completion-4a413df0b00dd8f2fee3e2022a04aa004a0c3888/readline_tilde_home_unset_filename_bare_off.out` |
| Current-user bare `~` (no slash), child `HOME` present but empty; `complete-filename` with `expand-tilde off` | Both implementations retained the unescaped literal prefix without trailing slash in both trials; neither selected a candidate. | Zero current-UID passwd-home lookups per implementation per trial; stderr was empty; pinned GNU 8.3 identity, stable matching classifications, guards, fixture-state checks, and cleanup passed. | `pinned-readline-8.3-home-empty-bare-tilde-off-filename-completion-4a413df0b00dd8f2fee3e2022a04aa004a0c3888/readline_tilde_home_empty_filename_bare_off.out` |

Each filename-completion result is limited to its exact fixture. The named-user
filename result does not establish behavior for other completion options, arbitrary
account names, or broader NSS-provider behavior; the current-user rows do not
generalize beyond their stated HOME states, prefixes, candidates, or completion
options. The bare-`~` rows establish only their no-slash prefixes and stated
HOME/options. In the two `expand-tilde on` rows, result classifications match, but
GNU emits one BEL byte on stderr while Sushline stderr is empty; the separate
HOME-nonempty `expand-tilde off` row matches both result classifications and stderr.
The vi rows are likewise limited to their stated command, user class, and HOME
state; they do not establish other combinations, prefixes, or NSS-provider behavior.

Tests and sources for `tilde-expand` and the shared lookup/completion behavior:
[`editing.rs`](readline/tests/editing.rs),
[`oracle/completion.rs`](readline/tests/oracle/completion.rs),
[`oracle/editing.rs`](readline/tests/oracle/editing.rs),
[`readline-include-paths.sh`](.github/scripts/readline-include-paths.sh),
[`ci.yml`](.github/workflows/ci.yml).
The `vi-tilde-expand` tests and sources are
[`editing.rs`](readline/tests/editing.rs),
[`oracle/editing.rs`](readline/tests/oracle/editing.rs),
[`vi_mode.rs`](readline/tests/vi_mode.rs),
[`readline-include-paths.sh`](.github/scripts/readline-include-paths.sh),
[`ci.yml`](.github/workflows/ci.yml).

## Readline Init File and Variables

### Init Syntax

| Feature | Status | Notes |
| --- | --- | --- |
| Blank lines and `#` comments | Compatible | Byte-detected, including non-UTF-8 comment bytes. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Inputrc file encoding | Compatible | Files and includes are read as bytes; raw and escaped high-bit key bytes bind alike. Trailing text after complete commands or quoted macro values is ignored, and `set` values are not truncated. Pinned Bash 5.3 / Readline 8.3 fixtures bind `beginning-\x80of-line`, `beginning-\xfeof-line` and `beginning-\xffof-line` (raw `0x80`, `0xfe` and `0xff`, respectively) as `beginning-of-line` and capture accepted line `he�lloXabc` for the double-quoted macro bodies `he\x80llo`, `he\xfello` and `he\xffllo` (lossy UTF-8 PTY text). These establish no rule for other invalid bytes or parser forms. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs). |
| `set variable value` | Compatible | Recognized variables normalized. Unknown variable names, and values that GNU Readline rejects for `bell-style`, `comment-begin`, `editing-mode`, and `keymap`, are reported with GNU's diagnostic text before the next prompt, and the directive is ignored; the built-in terminal writes the diagnostics to standard error, like GNU Readline, and a custom backend keeps them in its own output stream. GNU reads the value without its surrounding double quotes, so `""` is the empty value and a rejected value is reported unquoted. `bell-style` accepts `none`, `visible`, `audible`, `on`, and `off` in any case, `editing-mode` accepts any value that starts with `vi` or `emacs`, and GNU's canonical values are stored (`on` to `audible`, `off` to `none`, `vi-command` to `vi` with the derived `keymap` variable `vi-insert`). In the exact fixture `set completion-ignore-case on\r\n`, followed by LF `$if completion-ignore-case = on` and a Ctrl-O macro, both implementations leave the macro unselected; the LF `set` control selects it in both. This is only that variable/value/condition fixture, not a general CRLF `set` rule. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). Sanitized pinned-8.3 report: `pinned-readline-8.3-crlf-set-value-80d4988173ff4e673a4d0c85a4d5904fdc54195a/readline_crlf_set_value.json` (measurement SHA `80d4988173ff4e673a4d0c85a4d5904fdc54195a`; `ci / check` and `ci / oracle` passed). |
| Key bindings by key name or quoted key sequence | Compatible | Functions and macros. For the exact inputrc binding `"\C-o": "TRUE"` terminated by CRLF (macro `TRUE"` + CR), pinned Bash 5.3 / Readline 8.3 and Sushline accept `abcTRUE"` after `abc`, Ctrl-O alone; the LF control (macro `TRUE`) accepts `abcTRUE` after explicit Enter. For the same quoted macro with a bare CR as its final byte and no LF, both accept `abcTRUE"` after `abc`, Ctrl-O, Enter; this is only that exact fixture. For `"\C-o": backward-char` terminated by CRLF, both accept `abcX` after `abc`, Ctrl-O, `X`, Enter; the LF control accepts `abXc`. For the same binding with a bare CR as its final byte and no LF, both accept `abcX`; the no-CR unterminated control accepts `abXc`. These fixtures establish no other command-binding or line-ending forms. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs). |
| Recursive inputrc macros | Compatible | Direct and indirect macro cycles discard the recursive suffix with one nesting diagnostic and bell, preserve following input, and accept a later macro. The nesting diagnostic goes to the terminal's diagnostic sink, which the built-in terminal points at standard error like GNU Readline, while a custom backend keeps it in its own output stream. A diagnostic that does not reach the terminal's screen leaves the display untouched, so the redisplay after it starts at the rendered line again. Keyboard-macro replay/count semantics are unchanged. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| Escape sequences `\C-`, `\M-`, `\e`, `\\`, `\"`, `\'`, `\a`, `\b`, `\d`, `\f`, `\n`, `\r`, `\t`, `\v`, octal, hex | Compatible | Decoded through `KeySequence`. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| `$if`, `$else`, `$endif` | Known deviation | Directive names (`if`, `else`, `endif`, and `include`) are ASCII case-insensitive. Conditional parsing for version, editing mode, application-name and Readline-variable cases matches the pinned fixtures. The two pinned application-name case-mismatch controls both select `IF_TRUE` (uppercase condition/lowercase application name and the reverse); this establishes only those exact cases, not general case folding. In the exact mixed-ending fixture `$if version >= 8.0\r\n`, LF-terminated branch bindings, `$else\r\n`, `$endif\r\n`, GNU reports trailing garbage for the `$if` CR and unknown parser directives for `$else\r` and `$endif\r`, then accepts `abc` after `abc`, Ctrl-O, `X`, Enter; Sushline accepts the CRLF directives, selects the version branch and accepts `Xabc`. This is only that exact fixture, not a general condition or line-ending rule. The linked GNU 8.3 cases are fixture-scoped and establish no general grammar, argument case/prefix matching, locale, Unicode, or shell-state rule. Evidence: [`readline-inputrc-conditions.expected`](.github/scripts/readline-inputrc-conditions.expected), [`readline-inputrc-variable-rhs.sh`](.github/scripts/readline-inputrc-variable-rhs.sh), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`readline-inputrc-version-inequality.expected`](.github/scripts/readline-inputrc-version-inequality.expected), [`readline-inputrc-version-equality.expected`](.github/scripts/readline-inputrc-version-equality.expected), [`readline-inputrc-application-case.expected`](.github/scripts/readline-inputrc-application-case.expected), [`readline-inputrc-application-case-reverse.expected`](.github/scripts/readline-inputrc-application-case-reverse.expected), [`readline-inputrc-application-prefix.expected`](.github/scripts/readline-inputrc-application-prefix.expected), [`readline-inputrc-application-name-prefix.expected`](.github/scripts/readline-inputrc-application-name-prefix.expected), [`readline-inputrc-version-double-equality.expected`](.github/scripts/readline-inputrc-version-double-equality.expected), [`readline-inputrc-mode-vi-true.expected`](.github/scripts/readline-inputrc-mode-vi-true.expected), [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs). |
| `$include` path rules | Compatible | Measured 8.3 rule: skip leading whitespace and the whitespace separating `$include` from its filename, then take the rest of the line verbatim. Internal spaces (including repeated spaces) and tabs are preserved; quotes and trailing whitespace are literal filename bytes, not syntax, so they load only if that exact name exists. A CRLF line ending leaves its CR as the filename’s final byte: `$include NAME\r\n` loads only a file named `NAME\r`, while the LF control loads `NAME`; both directions are pinned PTY fixtures. A separate two-trial pinned 8.3 direct-library fixture, matched by Sushline's `bare_cr_include_path_keeps_terminal_cr` regression, covers one bare-CR case: LF selects the plain child (`completion-ignore-case on`, vi mode), while bare CR selects the CR-suffixed child (`off`, vi); GNU `read_status=0` in all four reads. This establishes only that fixture; other `$include` line-ending forms remain unmeasured. The `$if`, `$else`, and `$endif` row records a separate, exact CRLF conditional fixture. `$VAR` is never expanded, `~/` and `~user` expand, and a missing include is skipped while later lines still apply. In two pinned GNU Readline 8.3 direct-library trials, a test-owned mode-000 child was confirmed unreadable; `read_status=0`, the outer `completion-ignore-case off` remained in force, and the later `editing-mode vi` applied. Sushline's Unix `ignores_active_include_permission_denied` parser test likewise keeps the outer variable off and applies a later Ctrl-O binding when its open guard denies access; it skips if the process can still open the file. This establishes only the exact mode-000 fixture, not other permission failures or privileged access. On Unix, the pinned nonempty-HOME control and the two-trial HOME-empty fixture establish only their exact `$include` cases: the control loads `~/control.inputrc` from its test-owned HOME; with `HOME=""`, `$include ~/tmp/<unique>/marker.inputrc` loads the test-owned `/tmp/<unique>/marker.inputrc`, with `home_present=1`, `home_empty=1`, zero passwd-home lookups, `read_status=0`, `completion-ignore-case on`, and non-vi mode in both trials. The empty-HOME result distinguishes this absolute target from the CWD-relative and synthetic-passwd candidates; Sushline's resolver now matches the measured path. Two separate pinned HOME-unset trials each found a synthetic current-UID home with one lookup, read the child successfully, and let its `completion-ignore-case on` override the outer `off`. Named homes use `getpwnam_r`; the NSS lookup is shared with `tilde-expand`/`vi-tilde-expand` and filename-tilde expansion/completion. Coverage also includes CWD-relative, `./`, literal `$HOME`, quoted and trailing-whitespace spellings, and missing-include continuation. These measurements are fixture-scoped; they do not establish other HOME-empty or HOME-unset account-data cases. Evidence: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`readline-include-paths.sh`](.github/scripts/readline-include-paths.sh), [`ci.yml`](.github/workflows/ci.yml); pinned reports: `pinned-readline-8.3-home-unset-include/readline_include_home_unset.out`, `pinned-readline-8.3-home-empty-include/readline_include_home_empty.out`, `pinned-readline-8.3-unreadable-include/readline_include_unreadable.out`, `pinned-readline-8.3-bare-cr-include/readline_include_bare_cr.out`. |
| `$include ~user/<relative path>` for a known account | Compatible | A test-owned file under the current account’s home loads the child variable and later vi directive in Sushline and the pinned 8.3 fixture; CI requires the Sushline test to run. Sushline resolves named homes on Unix through `getpwnam_r` (NSS), independently of `HOME`, rather than parsing `/etc/passwd`; synthetic lookup tests cover this selection. The pinned fixture does not establish arbitrary-user or arbitrary-NSS-provider behavior. Evidence: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`ci.yml`](.github/workflows/ci.yml); pinned artifact: `pinned-readline-8.3-measurements/readline_named_include.out`. |
| Unknown or unavailable `~user` account data | Implemented | Bounded pinned 8.3 fixtures continue past a nonexistent-account include without a target, and load a test-owned literal CWD-relative `~sushline-no-such-user/marker.inputrc` when present. A controlled `getpwnam`/`getpwnam_r` EIO failure likewise leaves the unresolved spelling relative and loads its marker; Sushline matches. The separate synthetic empty-home-account fixture does not load the literal marker, but still applies later vi mode; Sushline matches that case too. Other account-data behavior is unmeasured. Evidence: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`readline-include-paths.sh`](.github/scripts/readline-include-paths.sh), [`ci.yml`](.github/workflows/ci.yml). |
| Recursive `$include` cycles | Known deviation | The pinned 8.3 two-file A → B → A fixture crashes with SIGSEGV; the shell self-cycle fixture exits 139 before the first prompt. Sushline instead records `inputrc include depth exceeded` and continues. Both accept the test-owned acyclic depths 18 and 32 and load the leaf; Sushline’s safety cap defaults to 32 and is configurable via `Config::inputrc_max_include_depth`, with a default depth-33 regression guard and no GNU claim beyond 32. A variable-conditioned recursion can terminate after mutation, so filename-only cycle rejection would reject valid input. Evidence: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`readline-include-depth.sh`](.github/scripts/readline-include-depth.sh), [`ci.yml`](.github/workflows/ci.yml). |
| Unsupported `$` directives | Compatible | An unknown directive emits GNU's `unknown parser directive` diagnostic, is otherwise ignored, and does not prevent later bindings; the pinned fixture covers `$unknown-directive`. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs). |
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
| `convert-meta`, `input-meta`, `meta-flag`, `output-meta` | Terminal-backed | Sushline defaults enable `input-meta`/`meta-flag` and `output-meta`, and disable `convert-meta`, only for an available UTF-8-named effective locale; other or unavailable locales use the reverse defaults. Unit tests cover `LC_ALL=C` over UTF-8 `LANG`, empty `LC_ALL` falling through to UTF-8 `LANG`, and an unavailable UTF-8-named locale. The conversion unit test shows byte `0xE1` is invariant across one-chunk and split-event delivery with `convert-meta on`, and is preserved raw with `convert-meta`, `input-meta`, and `meta-flag` off. GNU PTY comparisons explicitly set the variables and compare rendering under `LC_ALL=C` and available `C.UTF-8`; they establish no general GNU default equivalence. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `force-meta-prefix` | Compatible | With `convert-meta`, controls Meta binding representation; unbound ESC+printable rings once and processing continues, while redundant bare ESCs in vi command mode are silent; CSI/SS3 fallback is out of scope. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `enable-meta-key` | Terminal-backed | Defaults on with backend notification on setup/re-entry/cleanup; the built-in terminal emits no meta-mode sequences and custom backends own behavior. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `echo-control-characters`, `byte-oriented` | Terminal-backed | Byte-oriented and echo-control-characters rendering with unconditional TAB expansion. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `enable-bracketed-paste` terminal mode | Compatible | DEC mode sequences with terminfo-based defaults and explicit on/off overrides. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`variables.rs`](readline/tests/variables.rs). |
| Complete bracketed-paste frames | Compatible | Injected frames are accepted without a bell and set the mark; a lone unbound end marker rings once and redispatches its final byte. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`variables.rs`](readline/tests/variables.rs). |
| Bracketed-paste CR normalization and incomplete streams | Implemented | Completed pastes normalize CR to LF; split/truncated terminators buffer payload. Pinned 8.3 direct-library PTY fixtures cover CR normalization, a split end marker, and canonical VEOF with the master held open: missing/truncated end markers return the partial payload (including the truncated marker) only after follow-up LF, with redisplay while the child remains alive. Sushline’s corresponding true-EOF PTY tests assert those returned lines. These are the cited fixtures only, not broader paste, Bash or C ABI equivalence. Evidence: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`editor/tests/lifecycle.rs`](readline/src/editor/tests/lifecycle.rs), [`ci.yml`](.github/workflows/ci.yml). |
| `enable-keypad` | Compatible | Defaults off; explicit on emits available terminfo sequences around editing with explicit off emitting none. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`terminfo.rs`](readline/src/terminal/terminfo.rs). |
| `emacs-mode-string`, `vi-cmd-mode-string`, `vi-ins-mode-string`, `show-mode-in-prompt` | Compatible | Prompt rendering. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `horizontal-scroll-mode` | Known deviation | Tested Bash 5.3 PTY ASCII windows, cursors, hysteresis and restoration match for bytewise/batched input; offsets are byte-based. Sushline also matches the tested narrow-multibyte, combining, ambiguous and batched-wide cell/cursor outcomes. For the pinned 8.3 wide-edge fixture (50 zeros, `中`, 49 ones; 80 columns, `P>`, C.UTF-8), both emit the `b8 ad` suffix without the `e4` lead byte; the Bash 5.3/Sushline PTY insertion rows replay as `<`, two adjacent replacement cells and 49 ones, with cursor column 52. Ctrl-A restores complete UTF-8 and both restored rows show 23 trailing ones. These wide-tail observations remain fixture-scoped, not general Unicode or byte-for-byte redisplay claims. For bytewise typing with a final `中` in the 80-column `P>` fixture, both show 46 leading digits at cursor column 49; GNU’s replay row retains two replacement cells after the glyph while Sushline preserves the valid glyph. For the separate bytewise four-byte `🙂` tail under the 80-column `P>` C.UTF-8 fixture, GNU’s replay contains 42 digits at cursor column 43 (including `4` and `6` after the glyph with replacement cells), while Sushline shows 49 digits at column 52 and ends at the intact glyph; this is one observed fixture, not a general emoji or Unicode claim. For multiline `one\nP>` at 76 ASCII bytes under UTF-8, Bash leaves stale prompt cells while Sushline repaints; C-locale rows agree. The direct-library multiline fixture instead emits prompt plus 76 single-byte updates without scroll/motion controls (zero-based cursor 78), so that multiline difference is Bash-hosted only. Unmarked ANSI escapes count as logical columns. All conclusions are fixture-scoped. Evidence: [`oracle/horizontal_scroll.rs`](readline/tests/oracle/horizontal_scroll.rs), [`display.rs`](readline/tests/display.rs), [`display/horizontal.rs`](readline/src/display/horizontal.rs), [`readline-horizontal-render.sh`](.github/scripts/readline-horizontal-render.sh). |
| `history-preserve-point`, `history-size`, `mark-modified-lines`, `revert-all-at-newline`, `isearch-terminators`, `keyseq-timeout` | Compatible | Editor/history/display/input behavior. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `search-ignore-case` (editor searches) | Compatible | The [editor search evidence](#editor-search-evidence) records exact C.UTF-8 lowercase/no-expansion cases, invalid-byte controls and C-locale meta-input handling; these do not establish general locale or Unicode-folding behavior. The tested vi `/` and `?` paths remain case-sensitive. Evidence: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`state/search.rs`](readline/src/state/search.rs), [`editor/tests/history.rs`](readline/src/editor/tests/history.rs). |
| `search-ignore-case` (public History byte API) | Sushline extension | The explicit public `History` byte-search API folds ASCII only; it is separate from the editor inputrc variable, whose pinned cases are described in [Editor search evidence](#editor-search-evidence). Tests: [`history.rs`](history/tests/history.rs). |
| `histchars`, `history-word-delimiters`, `history-no-expand-chars`, `history-quotes-inhibit-expansion` | Sushline extension | These are not GNU Readline variables and inputrc ignores them; `HistoryChars` and `HistoryExpansionPolicy` are explicit Rust API configuration, with GNU defaults. Tests: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), [`history.rs`](history/tests/history.rs). |

Inputrc variable-test evidence is limited to the finite Bash 5.3-hosted
Readline PTY fixtures in [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs),
not a standalone pinned GNU Readline 8.3 library comparison. They observe
ASCII case-insensitive names and string comparisons with only the first
ASCII-whitespace-delimited RHS token (without unquoting): `editing-mode == EMACS`
and `COMPLETION-IGNORE-CASE == OFF` match, while quoted `"emacs"`/`"off"`,
`0`, `false`, and `nonsense` do not match their unquoted mode/`off` values.
The fixtures cover `=`, `==`, `!=`, unknown-name inequality returning false,
`100` versus `0100` without numeric coercion, required whitespace before the
variable operator (`editing-mode==emacs` is false), and unsupported `>`.
They retain the compact `mode=emacs` and `version>=8.3` controls, exercise
single-token `comment-begin` values containing operators, and observe earlier
`set` changes, nested `$else`, editing-mode changes, and an `emacs-meta`
binding target. Variable tests use current mode/binding-target state rather
than stale defaults. With stored `comment-begin` equal to `a`, RHS `a b`,
`a  b`, `a<TAB>b`, `a # comment`, and `A B` match like `a`; `ab`, `a==b`,
`a!=b`, and quoted `"a"` do not. With stored value `a b` set using quotes,
neither RHS `a b` nor `"a b"` matches. These are finite tokenization and
stored-multiword controls, not a general shell-state compatibility claim.

`input-meta` and `meta-flag` share one stored value and boolean flag. A finite
four-case Bash 5.3-hosted Readline PTY fixture writes `on` then `off`, or
`off` then `on`, in both name orders; each equality test through the first
name sees the last write. A consolidated `bind -v` comparison checks that
both output names report that last value for the same four ordered writes.
Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs),
[`bind_golden.rs`](readline/tests/bind_golden.rs). This is not a standalone
pinned-library measurement or a broader input-processing claim. Comparison
operands are not coerced. Local tests cover both setters, byte preservation
and lossy UTF-8 string rendering, defaults, flags, and all public read views.
`Variables::strings()` and `Variables::bytes()` return owned snapshots with
both names; mutating a snapshot does not change the variables. Use `insert`
or `insert_bytes` to update the shared value.

Legacy inputrc `mode=` and `term=` tests have a separate, finite twelve-case
Bash 5.3-hosted PTY comparison, not a standalone pinned-library measurement.
With editing mode `emacs` and `TERM=xterm-256-color`, `term=xterm`,
`term=xterm-256-color`, `TERM=xterm`, `mode=emacs`, and `mode=EMACS` match;
`term=xterm-256`, `term==xterm`, `term!=vt100`, `term = xterm`, `mode==emacs`,
`mode!=vi`, and `mode = emacs` do not. These legacy tests use compact single
`=` syntax rather than general variable-comparison operators. Terminal tests
match the full name or the base before its first hyphen, not an arbitrary
hyphen-separated prefix. Additional bounded Bash 5.3 `bind -f` observations
show `term=XTERM-256-COLOR`, `term=xterm trailing`, and `mode=emacs trailing`
matching, but `term= xterm` and `mode= emacs` not matching: comparison folds
ASCII case and stops at the first RHS whitespace without skipping leading
whitespace. Local tests also check that `mode=` follows earlier inputrc
editing-mode mutations instead of configuration defaults. Tests:
[`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs),
[`inputrc/tests.rs`](readline/src/inputrc/tests.rs). General variable and
version comparisons keep their separate syntax and semantics described above.

Numeric storage evidence is the finite Bash 5.3-hosted Readline 8.3 matrix in
[`bash_bind_v_and_sushline_store_measured_numeric_matrix`](readline/tests/oracle/inputrc.rs)
and [`inputrc/tests.rs`](readline/src/inputrc/tests.rs), not a standalone pinned-library
comparison. It covers eighteen spellings for `completion-display-width`,
`completion-prefix-display-length`, `completion-query-items`, `history-size`, and
`keyseq-timeout`, with startup-file loading checked before inspecting `bind -v`.
Storage is normalized, comparison operands are not: setting `0100` stores `100`.
The signed decimal prefix, saturating 64-bit parse, 32-bit conversion and
setting-specific negative clamps match only that matrix, including junk suffixes,
overflow and 40-digit controls. This establishes no general C/`atoi` grammar,
unmeasured range or cross-architecture behavior. Nonnumeric strings are unchanged.

### Editor search evidence

The following are bounded pinned Readline 8.3 direct-library observations,
paired with [`oracle/history.rs`](readline/tests/oracle/history.rs) Bash/Sushline
PTY regressions. Expected projections define the seeds, keys and accepted bytes;
none implies general Unicode folding, locale or search equivalence.

- **C.UTF-8 lowercase:** the single-entry prefix and reverse-incremental
  `älpha`/`ÄLPHA old` cases, and the forward-incremental case with `prior`
  plus `ÄLPHA old`, match. Projections:
  [prefix](.github/scripts/readline-unicode-history-prefix-search.expected),
  [reverse](.github/scripts/readline-unicode-history-incremental-search.expected),
  [forward](.github/scripts/readline-unicode-history-forward-incremental-search.expected).
- **No multi-character expansion in these cases:** reverse/forward incremental
  `ss` against `ß entry`, and query-side `ß` against `SS`, do not expand the
  lowercase mapping; the forward controls include `prior`. Projections:
  [reverse entry-side](.github/scripts/readline-unicode-history-expansion-incremental-search.expected),
  [forward entry-side](.github/scripts/readline-unicode-history-forward-expansion-incremental-search.expected),
  [reverse query-side](.github/scripts/readline-unicode-history-query-expansion-incremental-search.expected),
  [forward query-side](.github/scripts/readline-unicode-history-forward-query-expansion-incremental-search.expected).
- **C-locale input handling, not Unicode folding:** with `search-ignore-case=on`,
  UTF-8 `älpha` input and history `ÄLPHA old`, default meta conversion keeps
  the high bytes out of the query. With `convert-meta=off`/`input-meta=on`,
  the first unquoted high byte exits search and is processed as ordinary input.
  The separate ASCII-`a` prefix case first matches, then inserts the high-byte
  suffix at that match start. Projections:
  [default meta](.github/scripts/readline-c-locale-history-incremental-search.expected),
  [input-meta](.github/scripts/readline-c-locale-input-meta-history-incremental-search.expected),
  [ASCII prefix then high bytes](.github/scripts/readline-c-locale-input-meta-history-ascii-prefix-search.expected).
- **Invalid history bytes:** C.UTF-8 ASCII `alpha` queries accept unchanged
  `pre\xffALPHA` in reverse incremental search and `ALPHA\xfftail` in the
  independent prefix fixture. Projections:
  [incremental](.github/scripts/readline-invalid-history-search.expected),
  [prefix](.github/scripts/readline-invalid-history-prefix-search.expected);
  [probe](.github/scripts/readline-invalid-history-search.sh). These establish
  only the two invalid-byte cases, not general search equivalence; matching
  Rust byte-seeded editor regressions are in
  [`editor/tests/history.rs`](readline/src/editor/tests/history.rs).

## History Expansion API

| Feature | Status | Notes |
| --- | --- | --- |
| Event designators `!!`, `!n`, `!-n`, `!string`, `!?string[?]`, `!$`, `!^`, `!:`, `!#` | Compatible | Scoped event/prefix termination and omitted-`?` behavior; `search_delimiters` is API policy, not inputrc. The [search-delimiter](#search-delimiter-expansion-fixture) and [current-line](#current-line-event-expansion-fixture) notes delimit the exact direct-library cases. In the stifled append fixture absolute event numbers follow the advancing base, while relative numbers use resident entries; clear/reseed resets the base. Ordinal `get_1_based_entry` access is unchanged. Evidence: [`history/tests/history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs), [`history-expand-reuse.expected`](.github/scripts/history-expand-reuse.expected), [`ci.yml`](.github/workflows/ci.yml). |
| Quick substitution `^old^new^` | Compatible | Uses the previous entry. The pinned quick-substitution fixture distinguishes expanded versus escaped replacement ampersands and remembered `!!:&` reuse. Evidence: [`history/tests/history.rs`](history/tests/history.rs), [`history-expand-reuse.expected`](.github/scripts/history-expand-reuse.expected), [`ci.yml`](.github/workflows/ci.yml). |
| Word designators `0`, `n`, `^`, `$`, `%`, `x-y`, `*`, `x*`, `x-` | Hook-backed | Quoted/escaped word, shell-construct, and `%` position behavior; exact tokenization and status can come from `Hooks::expand_history`. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Modifiers `h`, `t`, `r`, `e`, `q`, `x`, `s/old/new/`, `&`, `g`, `a`, `G` | Compatible | Path/substitution operations use event text; the last `:q`/`:x` mode applies to concatenated output. The Bash 5.3 `history -p` fixture covers `:x` SP/TAB/LF separators and empty fields while CR/FF remain within fields; this is runtime evidence, not standalone source proof. The pinned unsupported-`:u` fixture retains its distinct unrecognized-modifier error. Evidence: [`history/tests/history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs), [`ci.yml`](.github/workflows/ci.yml). |
| Substitution and search memory across calls | Compatible | `HistoryExpansionState` owns session memory. In the pinned two-entry sequences, successful search/matched-word state survives failed search and clear/reseed, enabling `!??`, `!:%` and empty-old substitution reuse. A first failed search does not seed substitution memory even after append (`no previous substitution`); cold-start/first-failure `%` is empty. These are only the projected seeds/call orders, not a general reset/failure-state rule. Rust tests assert typed errors, not GNU’s exact error text/status mapping. Evidence: [`history/tests/history.rs`](history/tests/history.rs), [`history-expand-reuse.expected`](.github/scripts/history-expand-reuse.expected), [`ci.yml`](.github/workflows/ci.yml). |
| Modifier `p` | Compatible | `HistoryExpansionStatus` distinguishes unchanged, expanded and print-only results; failures remain `Err`. The editor’s print-only output follows the diagnostic sink (built-in stderr), matching Bash `history-expand-line`, not the shell `history -p` stdout sink. Evidence: [`history/tests/history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs), [`ci.yml`](.github/workflows/ci.yml). |
| Existing quote state | Compatible | Explicit `HistoryExpansionPolicy::quote_state`. Pure History defaults expand the pinned open-single-quote `echo '!!` fixture; the editor separately inhibits the Bash 5.3 prompt fixture `'!!'`. These direct-library and Bash-hosted policies are distinct and fixture-scoped. Evidence: [`history/tests/history.rs`](history/tests/history.rs), [`ci.yml`](.github/workflows/ci.yml), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Inhibit-expansion callback | Compatible | The per-call `inhibit(index)` predicate matches the pinned index-zero-only callback fixture (`!!` inhibited; `x!!` expanded at index 1). No general callback equivalence is claimed. Evidence: [`history/tests/history.rs`](history/tests/history.rs), [`ci.yml`](.github/workflows/ci.yml). |

The pinned GNU History 8.3 [CI assertions](.github/workflows/ci.yml) and
[`history_extended_glob_word_selection_matches_pinned_history_83`](history/tests/history.rs)
cover tokenization, argument extraction and seeded `!!:1` selection for five
extended-glob-shaped prefixes, nested forms, and quoted/escaped,
command/process-substitution and arithmetic controls. The measured unquoted
prefixes and nested form remain one word; the controls retain their separately
asserted boundaries. This is History word handling, not glob expansion or Bash
parsing. The historical measurement completed before a separate SIGINT probe
compilation failure, so that observation alone did not establish a passing job.
Bash-hosted and exploratory installed-8.2 observations remain distinct.

## Editor History Expansion Commands

The names below are the editor's history expansion commands. `magic-space`,
`history-expand-line`, `history-and-alias-expand-line` and `alias-expand-line`
are the shell-registered names of the User-Facing Readline Commands section
above, so their baseline is the Bash 5.3 host's own command rather than a GNU
Readline command. The status column gives the editor's behavior for each name.

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
| List management: `add_history`, `add_history_time`, `remove_history`, `replace_history_entry`, `clear_history`, `stifle_history`, `unstifle_history`, `history_is_stifled` | `push`, `push_bytes`, `add_time`, `remove`, `replace`, `clear`, `stifle`, `unstifle`, `is_stifled` | Compatible | Empty entries, replacement/undo/timestamps and stifle limits. `add_time` stores record text; auto-add skips empty lines. Invalid-index removal is a cursor-preserving no-op. Pinned A/B/C/D fixtures cover replacement/removal (including invalid indexes), stifled C/D lists, unstifling caps 0/2 and clearing at the projected offsets: replacement/removal/unstifle preserve the numeric offset, even beyond the new end; clear resets offset/length but retains stifling. Current-entry calls are guarded below length, and no post-clear navigation is measured. Raw append/state-round-trip fixtures preserve selected numeric offsets, including after stifling to 0/1/2/4 and appending. Sushline safely returns `None` for cursor-relative operations beyond the end without moving the offset; beginning/end or valid `set_pos` recover. The GNU probe avoids such navigation because direct calls could segfault. Editor auto-insertion separately resets to the unpositioned end. No general stifling/import claim is made. Evidence: [`history/tests/history.rs`](history/tests/history.rs), [`history-position-replace.expected`](.github/scripts/history-position-replace.expected), [`history-position-remove.expected`](.github/scripts/history-position-remove.expected), [`history-position-unstifle.expected`](.github/scripts/history-position-unstifle.expected), [`history-position-clear.expected`](.github/scripts/history-position-clear.expected), [`ci.yml`](.github/workflows/ci.yml). |
| List information: `history_list`, `where_history`, `current_history`, `history_get`, `history_get_time`, `history_total_bytes` | `entries`, `where_history`, `current_history`, `get`, entry `timestamp`, `timestamp_seconds`, `gnu_timestamp_seconds`, `total_bytes` | Mixed | Rust indexes start at zero, GNU `history_get` at one. `timestamp_seconds` parses stored timestamp text to `Option<i64>` with C-style whitespace, sign and decimal-prefix handling; absent/overflow values remain distinct from zero. GNU `history_get_time` instead gates on mutable `history_comment_char` and returns zero for mismatch/failure (the measured default is zero). `gnu_timestamp_seconds(comment_char)` preserves the Rust view while providing that GNU-style zero behavior. Matching/different markers, vertical-tab and overflow are bounded pinned `history_time.out` fixtures, not general conversion equivalence. `total_bytes` matches resident line/timestamp byte lengths for the projected empty, ASCII, UTF-8 and mutation/stifling fixtures, not allocation accounting. Evidence: [`history-total-bytes.expected`](.github/scripts/history-total-bytes.expected), [`history/tests/history.rs`](history/tests/history.rs), [`ci.yml`](.github/workflows/ci.yml). |
| Navigation: `history_set_pos`, `previous_history`, `next_history` | `set_pos`, `previous_history`, `next_history` | Compatible | Pinned A/B/C fixtures cover positions 0–3, end-position success, beyond-end target failure without movement, and cleared-empty position 0/1 controls. Paired previous/next calls cover those same starting offsets. Current-entry queries stay below length; navigation never starts beyond end. This establishes no negative-position or beyond-end navigation behavior. Evidence: [`history/tests/history.rs`](history/tests/history.rs), [`history-position-set-pos.expected`](.github/scripts/history-position-set-pos.expected), [`history-position-navigation.expected`](.github/scripts/history-position-navigation.expected). |
| Search: `history_search`, `history_search_prefix`, `history_search_pos` | `history_search_bytes`, `history_search_prefix`, `history_search_pos` | Mixed | The measured GNU library takes `history_search_pos` direction before position, opposite the header’s parameter names; Rust follows measured order with an enum direction and entry/offset result. Search includes its zero-based starting position, failure preserves the cursor, and an invalid start falls back to the current cursor (projected starts 0/3/99, cursor 1). Raw byte-search fixtures cover ASCII/UTF-8 single-entry offsets, multi-entry traversal/current match/failure, invalid bytes and empty queries; the pinned single-entry uppercase `ALPHA` control does not match lowercase `alpha` in either direction. Offsets are byte-based, not character counts. For the projected `alpha\0suffix` query GNU’s C string stops at NUL, while Rust searches the full slice and finds no match, preserving state. This is a deliberate byte-slice capability, not general NUL/search equivalence. Evidence: [`history-position-search-pos.expected`](.github/scripts/history-position-search-pos.expected), [`history/tests/history.rs`](history/tests/history.rs), [`history-search-offset.expected`](.github/scripts/history-search-offset.expected), [`history-search-nul-query.expected`](.github/scripts/history-search-nul-query.expected). |
| Case-insensitive search helper | `history_search_bytes_with_case` | Sushline extension | Explicit ASCII-only case folding; GNU History search has no equivalent option. Tests: [`history.rs`](history/tests/history.rs). |
| Files: `read_history`, `write_history`, `append_history`, `history_truncate_file`, default `~/.history` filename | `read_file`, `read_file_with_multiline`, `load_file`, `load_file_with_multiline`, `write_file`, append variants, `truncate_file`, default path/helpers | Mixed | Read/write/append/truncate and default paths have distinct policies and fixture limits; see [History file behavior](#history-file-behavior). |
| File-import cursor fixtures | `read_file`, `read_file_range`, `load_file`, `load_file_range` | Compatible | The three-line plain-file projection covers empty/nonempty full imports, range import, and stifled imports at its selected positions/bounds. Numeric offsets survive, including offset 3 beyond a resulting two-entry list. The probe avoids current-entry/navigation queries at invalid offsets. No other files, bounds or positions are claimed. Evidence: [`history-import-cursor.expected`](.github/scripts/history-import-cursor.expected), [`history/tests/history.rs`](history/tests/history.rs), [`history-import-cursor.sh`](.github/scripts/history-import-cursor.sh). |
| File range: `read_history_range` | `read_file_range`, `read_file_range_with_multiline`, `read_file_range_with_options`, `load_file_range`, `load_file_range_with_multiline`, `load_file_range_with_options` | Mixed by policy | `HistoryReadOptions::range_policy` defaults to `HistoryRangePolicy::Sushline`. Opt-in `GnuReadline83` matches the pinned GNU Readline 8.3 range matrix asserted in CI. In the no-blank timestamped fixture, (0,0), (2,2) and (2,1) yield no entries; in the plain fixture they yield {C0}, {C2} and {C2}. Forward ranges and explicit `None` suffixes are unchanged by the option. See [History file ranges](#history-file-ranges) for exact bounds, fixtures and limits. |
| Expansion: `history_expand` | `expand_history`, status/state variants, `HistoryExpansionState`, `Hooks::expand_history` | Compatible | Text and print-only status; Editor context forwards session state to hooks. Marker matching is byte-oriented and ASCII-only; non-ASCII `HistoryChars` values are inert. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Expansion helpers: `get_history_event`, `history_tokenize`, `history_arg_extract` | `get_history_event`, `history_tokenize`, `history_arg_extract`, `command_words` | Compatible | Rust helpers exposed. Tests: [`history.rs`](history/tests/history.rs). |
| Variables: `history_base`, `history_length`, `history_max_entries` | `HistoryState` and methods | Compatible | Rust-owned offset, length, stifle and maximum-entry behavior. Tests: [`history.rs`](history/tests/history.rs). |
| Variables: `history_expansion_char`, `history_subst_char`, `history_comment_char`, `history_word_delimiters`, `history_no_expand_chars`, `history_quotes_inhibit_expansion` | `HistoryChars`, `HistoryExpansionPolicy` | Compatible | Available to expansion API and editor expansion. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_write_timestamps` | Timestamp write/append methods | Compatible | Enabled or suppressed per call. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_quoting_state` | `HistoryExpansionPolicy::quote_state` | Compatible | The pinned GNU History 8.3 [`history_expand_probe`](.github/workflows/ci.yml) checks only `!!` after seeding `echo previous`, `echo a b`, `echo a b`, with `history_quotes_inhibit_expansion=1`: inherited single-quote state returned status 0/literal `!!`, and state 0 returned status 1/`echo a b`. Both globals reset to 0 after each run; exact rows are retained in `history_expand.out`. [`history_expansion_inherited_quote_state_matches_pinned_fixture`](history/tests/history.rs) checks the corresponding Rust policy outputs and `Unchanged`/`Expanded` statuses. The exact rows passed in pinned GNU History 8.3 CI; local GNU 8.2 observations are directional only. This is not a general quote-state compatibility claim. |
| Variable: `history_inhibit_expansion_function` | `expand_history` inhibit predicate | Compatible | Per-call predicate rather than process-global pointer. Tests: [`history.rs`](history/tests/history.rs). |

### History file behavior

These are fixture-scoped only. Close errors propagate; there is no
fsync/crash-durability guarantee; concurrent writers are last-writer-wins.

Evidence: [`history/tests/history.rs`](history/tests/history.rs),
[`ci.yml`](.github/workflows/ci.yml).

#### Full writes and file types

On Unix, regular rewrites are atomic and mode 0600. This includes writes through a
symlink to an existing regular file: Sushline preserves the link but defaults to
replacing its target with mode 0600. Opting into `HistoryWriteOptions { preserve_symlink_target_mode: true, ..Default::default() }` preserves an existing
regular symlink target's Unix permission mode on full writes and truncation; direct
paths and missing targets still use 0600. Only the pinned GNU Readline 8.3
`write_history` symlink fixture (mode-0644 target preserved) is claimed equivalent.

Non-regular reads are rejected and non-regular writes truncate in place.

Evidence: [write](.github/scripts/history-write-mode.expected).

#### Full-file imports

- Sushline preserves NUL bytes by default; opt-in `TruncateAtNul` matches only cited
  truncation fixtures and does not synthesize GNU run-dependent timestamps. Evidence:
  [NUL per-line](.github/scripts/history-full-nul-eof.expected), [NUL joined](.github/scripts/history-full-nul-eof-joined.expected).
- Per-line imports skip blanks and apply timestamp records to following commands;
  `Joined` retains interior blanks.
- The pinned plain joined-EOF pair produces one `A\nB` entry without the final LF and
  one `A\nB\nunterminated` entry with it; no broader unterminated-line rule is claimed.
  Evidence: [joined EOF](.github/scripts/history-joined-eof.expected).
- The pinned GNU History 8.3 multiline-mode-1, limit-1 fixture yields one entry
  containing `B0\nB1` from the final timestamp block; no broader joined/stifled limit
  behavior is claimed. Evidence: [multiline cap](.github/scripts/history-multiline-limit.expected).
- CRLF is stripped on import but preserved on truncation. Evidence:
  [CRLF](.github/scripts/history-full-crlf.expected).

#### Appends

- On Unix, the pinned GNU History 8.3 append fixture shows mode 0644 retained for an
  existing direct file and the relative symlink target; both implementations preserve
  the link and append to its target.
- For a missing direct destination, Sushline defaults to creating the file even when no
  entries are selected. Options-taking append variants with `HistoryAppendOptions { create_missing: false }` return NotFound without creating it, matching only the pinned
  GNU History 8.3 `append_history` ENOENT/no-file count-0 and count-1 cases.

Appends remain non-atomic. Evidence: [append](.github/scripts/history-append.expected).

#### Truncation

On Unix, a no-op truncation preserves the existing mode, matching the tested GNU mode
fixture; actual rewrites default to mode 0600, with the same symlink-target opt-in
described above.

Evidence: [truncate](.github/scripts/readline-history-truncate.sh),
[write](.github/scripts/history-write-mode.expected),
[CRLF](.github/scripts/history-full-crlf.expected).

#### Default history-file path

The pinned GNU History 8.3 `read_history(NULL)` probe with `HOME` absent returned status
0 and loaded none of the test-owned CWD `.history`, synthetic passwd-home `.history`, or
literal CWD `~/.history` sentinels in two fresh trials; the probe's passwd-lookup
counter remained 0. This result is limited to that call and those fixtures: it does not
identify a fallback path or establish equivalence to this helper.

The Rust default-path helper uses `$HOME/.history` for nonempty `HOME`; a present
but empty `HOME` produces relative `.history`, while an unset `HOME` produces
relative `./.history`. Both relative paths target the current directory. On Unix,
non-UTF-8 bytes in `HOME` are preserved in the returned path. These are
Sushline-specific path-selection behaviors; no GNU path-equivalence claim is made
for empty or unset `HOME`.

### History file ranges

Bounds count physical command lines, including blanks, but not timestamp records. For
FROM<TO, TO is exclusive; grouping happens after selection and may end at a partial
block. Per-line mode skips blank entries; joined spans keep blanks and may yield an
empty entry. `HistoryReadOptions::range_policy` defaults to
`HistoryRangePolicy::Sushline`: equal bounds select one physical position and reversed
bounds select through EOF. Convenience methods retain that default.

Opt-in `GnuReadline83` matches the pinned GNU Readline 8.3 range matrix asserted in CI:
timestamped/plain forward ranges, late markers, blank-line edges in both grouping modes,
no-blank equal/reversed per-line edges, and per-line out-of-range bounds.

- In the no-blank timestamped fixture, (0,0), (2,2) and (2,1) yield no entries; in the
  plain fixture they yield {C0}, {C2} and {C2}.
- In both blank-line fixtures, (1,1), (1,0) and (1,2) yield none per-line or one empty
  joined entry; (2,1) yields {A1} in both modes.
- FROM past EOF yields none and TO past EOF yields the remaining lines in the tested
  cases.

Forward ranges and explicit `None` suffixes are unchanged by the option, which affects
only range reads/loads, not full-file imports or mutations. These are fixture-scoped
results, not general GNU range equivalence. Evidence:
[`history/tests/history.rs`](history/tests/history.rs),
[`ci.yml`](.github/workflows/ci.yml).
