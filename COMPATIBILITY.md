# Readline and History Compatibility

Sushline is a Rust line-editing and history foundation that aims to match GNU
Readline and History Library observable behavior where that behavior belongs
inside a line editor or history component.

The compatibility target is GNU Readline 8.3 patch level 0 and GNU History
Library behavior, limited to Sushline's Rust editor and history APIs. The CI
oracle host is GNU Bash 5.3 patch level 0, built from the pinned
release tarball without later standalone Bash patches. The local oracle-suite
gate checks only GNU Bash major.minor 5.3, so a later patch level on `PATH` can
pass that gate; observations from such a local host are not patchlevel-0
evidence. The suite rejects other major.minor versions. The Readline
the host carries is part of that baseline: the suite reads the release
number, the version label and the command list the host's Readline carries
([`oracle/host_readline.rs`](readline/tests/oracle/host_readline.rs)), names
the release it measured ([`oracle/host.rs`](readline/tests/oracle/host.rs)),
and stops on a release the claims below do not cover, so the oracle evidence
is scoped to a Readline release and not to Bash alone. The number is read
from the host's own image when the host links Readline statically, in either
the symbols it exports or the symbols it was built with, and from the
library the host loads otherwise. A host whose release cannot be read, a
host whose programs are not ELF images, is named by the suite and the suite
continues there; the evidence of such a host is not scoped to a release. The
host the claims were measured on links Readline statically and exports the
release number 0x0803, the 8.3 release this document targets. Its library's
own version label reads 8.1, so the image states the release two ways and
the two disagree; the release identity rests on the release number, because
the host's command list holds the two names the 8.3 release adds to the
library's function table (`execute-named-command` and `export-completions`):
the shell registers neither of them, and no earlier release holds them, the
installed 8.2 release's library listing neither in its own function dump, so
the code is the 8.3 release and the label is stale. The suite reads the
number and the label from the image that carries them and asks the host for
its command list, and it names all three in its note, so the evidence
carries the release it was measured on. The CI job builds the Readline
release this document names from the upstream tarball and links the pinned
Bash host against it, so the same suite runs against that release there as
well. The build is configured with the options the job names in its log and
in the cache keys, and the job stops the build when the release's configure
does not accept one of them; the host's Readline selection is named in the
cache key as well, so neither a library nor a host built another way is
reused as the pinned one. The job checks the release number the built
library exports, that the built library links a terminal-capability library,
and that the pinned host's command list holds the two names the 8.3 release
adds, so a build of another release cannot pass as the pinned one; the suite
reads the same signals from that library, so the job log names them as well.
The job also runs the output-stream probe against that pinned release, so
the claim the output-stream row makes is checked on the release this
document targets and not only observed on the 8.2 release the runner image
ships. That job has run on a tree that carries these steps and passed, so
the pinned build of the release this document names has been exercised in
CI: the job built it from the pinned digest, linked the pinned host
against it, checked the output-stream claim on it, and ran the suite
against that host, which reported the release it carries. The 8.3
baseline of the claims below therefore rests on that run as well as on
the observations made against the reference host. The job has run again
on a tree that carries the history range measurement with the probe linked
to the pinned history library and with the loader guard that names the
library, and it passed: the loader's report named the pinned prefix for
every readline and history library the probe loaded, so the pinned 8.3
release's own range call is what answered the measurement. It returned the
entries the file-range row records for the fixture the job measures, and
it confirmed the two cases where the two read a file differently.
The job carries the history timestamp measurement too, with the probe
linked to the pinned history library and with the loader guard that names
the library, so the rule the list-information row records is measured
against the release this document names rather than only against the
release the runner image ships.
The job also calls `readline(prompt)` directly in a program linked to
the pinned library and checks the output bytes for literal shell-looking
sequences and Readline marker delimiters, while checking the library
release number and loader path.
The tarball's digest is pinned in the job, so the fetch is a checked one.
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
| Cell width and locale | In UTF-8 mode, width and cursor motion depend on character and locale handling. | Static UTF-8 `char_width` uses UAX #11; character motion follows bytewise or UTF-8 motion per `byte-oriented`/locale, and interior rendering preserves bytes. The Compatible status remains based on Bash 5.3 PTY runtime evidence. A separate direct pinned Readline 8.3 probe covers exactly four C.UTF-8 fixtures: e + U+0301, U+00B7, U+754C, and U+2708 + U+FE0F; two trials in each of the primary and fresh-runner reports agreed byte-for-byte. Each accepted the fixture bytes and recorded `rl_point == rl_end` at UTF-8 byte lengths 3, 2, 3, and 6 respectively; the raw post-movement bytes were `08 1b 5b 43` for e + U+0301, U+00B7 and U+2708 + U+FE0F, and `08 08 1b 5b 43 1b 5b 43` for U+754C. This PTY probe has no terminal emulator, so it establishes no physical terminal columns or general grapheme-width rule. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`sushline_unicode_width_probe_baseline_fixtures`](readline/tests/oracle/horizontal_scroll.rs), [`readline-unicode-width.sh`](.github/scripts/readline-unicode-width.sh), [`lifecycle.rs`](readline/src/editor/tests/lifecycle.rs), [`buffer/tests.rs`](readline/src/buffer/tests.rs). | Compatible |
| Unwrapped ANSI in prompts | Readline draws the prompt it is given: it reads the invisible markers a shell hands it and leaves every other byte, so a shell expands its own prompt escapes before the editor sees them. Unmarked and marked ANSI prompts differ in Readline redisplay. | The prompt is drawn as it is given: `Prompt::new` reads only Readline's invisible markers, so a prompt the caller meant literally stays literal, and the shell's spelling is an explicit choice (`PromptEscapes::Shell`): the backslash sequences a shell reads in a prompt are read the way the shell reads them (`\\`, `\a`, `\e`, `\n`, `\r`, `\[` and `\]` for a non-printing span, and `\NNN` with exactly three octal digits for the byte the shell writes, except that a value above 0x7f is carried as the character with that code point, because a UTF-8 prompt string cannot hold a lone byte above 0x7f, then the quoting its expansion removes: `\\`, `\"`, `\$` and `` \` ``), every other backslash sequence is left as it was written, a run of backslashes before a quoting character is read the way the shell reads it, the parameters and commands a prompt holds are the shell's own expansion and are left as they were written, and the shell's own prompt sequences (`\u`, `\w`, `\t`, ...) are not read because they name the shell's variables. Sushline preserves the unmarked/marked column distinction and final cursor columns while repainting rather than matching incremental output bytes. Evidence is Bash 5.3 PTY runtime plus a direct C probe linked to the pinned GNU Readline 8.3 library. The probe preserved literal `A\eB> `, `A\101B> ` and `A\[\e[31m\]B> ` prompt bytes; it emitted the ANSI sequence in a `\001`/`\002`-marked prompt without emitting those markers. CI records the release number and confirms the loaded `libreadline` path is under the pinned prefix. The marked/unmarked redisplay comparison is the Bash 5.3 PTY runtime observation, not an inference from the probe. An additional Bash 5.3/Sushline PTY fixture compares raw `A\\B> ` to `A\B> `, `A\"B> ` to `A"B> `, `A\$B> ` to `A$B> `, raw ``A\`B> `` to ``A`B> ``, `A\\eB> ` to `A\eB> `, `A\\\eB> ` to `A\` + ESC + `B> `, and `A\\\\eB> ` to `A\eB> `; both PTYs accepted each fixture line, and escaped dollar and backtick remained literal. These are only the measured prompt strings. Horizontal scrolling keeps its separate deviation below. Tests: [`oracle/display.rs`](readline/tests/oracle/display.rs), [`prompt/tests.rs`](readline/src/prompt/tests.rs). | Implemented |
| Region/display/terminal internals | Redisplay and active-region behavior depend on terminal capabilities. | Rust display model uses `TerminalIo`; the built-in terminal is Unix-oriented and live operations return `Unsupported` on non-Unix unless another backend is supplied. The built-in terminal reads the input the terminal holds as one batch, and the editor dispatches the batch character by character and redraws it once, like GNU Readline; a key that changes the display without running a command, like the query of an incremental search, redraws as well, so a batch that carries the query with the search key draws the same query as the split reads, which the Bash 5.3 PTY host shows by producing the same output for its batched write and its split write of the same keys; the bytes that follow an accepted line stay in the input stream, so the next read continues with them. The built-in terminal writes the inputrc diagnostics and the readline error messages to standard error like GNU Readline, and writes the history expansion failure message there as well: the Bash host's `history-expand-line` command, which is the interaction the editor mirrors, leaves the line unchanged and reports the failure on standard error, the same sink, and the shell's own expansion reports it there too. A custom backend chooses the destination of its diagnostics through `TerminalIo::write_diagnostic`, which defaults to the terminal output stream. A diagnostic that does not reach the terminal's own screen leaves the display untouched, so `TerminalIo::diagnostics_use_terminal` reports whether the sink is that screen, and the built-in terminal compares standard error with standard output. Raw-mode setup preserves `IXON`/`IXOFF` and normalizes input flags; for `CS7` when `input-meta` is enabled, it clears `ISTRIP`/`INPCK`, matching the pinned Readline 8.3 synthetic-termios measurement of `rl_prep_terminal(meta_flag)`, while CS7 without input-meta preserves them. The pinned 8.3 synthetic termios fixture asserts the requested flags for CS8 and for CS7 with input-meta both disabled and enabled; it does not establish live CS7 PTY behavior. Exact bytes depend on backend. Tests: [`oracle/display.rs`](readline/tests/oracle/display.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs), [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`oracle/diagnostics.rs`](readline/tests/oracle/diagnostics.rs), [`variables.rs`](readline/tests/variables.rs), [`lifecycle.rs`](readline/src/editor/tests/lifecycle.rs), [`terminal.rs`](readline/src/terminal.rs). Bash redirects the line editor's output to standard error, while the built-in terminal writes the display to standard output, so a redirected standard output swallows the prompt for Sushline but not for the host (Bash 5.3 PTY observation). GNU Readline 8.2, linked directly into a program whose own standard output and standard error were redirected to separate files, wrote its prompt to standard output, so the library's default output stream is standard output; the built-in terminal follows that default and differs from the host shell that redirects it. The claim rests on that 8.2 observation: the Readline the host the claims were measured on links is static, so no program of its own can be linked against it, and the shell redirects the editor's output itself. The oracle CI job observes the runner image's Readline, prints the release it saw, and checks the claim when the image ships the 8.2 it was observed on, so a change in the image's library shows up in the job log. The same job runs the probe against the pinned 8.3 release and checks the claim there, so the claim is checked on the release this document targets as well. | Terminal-backed |
| Event / input-available hooks | GNU provides idle event/input hooks and keyboard-input timeouts. | No implicit idle callback: embedder policy uses `TerminalIo::read_event` timeout/poll and `Hooks::check_signals`; `keyseq-timeout` drives the read timeout. Tests: [`variables.rs`](readline/tests/variables.rs). | Terminal-backed |
| Signal install/catch/reraise | GNU independently gates signal catching, preserves ignored dispositions, chains saved `SIGWINCH`, and cleans up then reraises caught signals including `SIGINT`. Two independent pinned Readline 8.3 direct-library PTY measurements observed a live non-application `SIGWINCH` wrapper during `readline()`, one saved application-handler callback, return of the pre-seeded `abcX`, and restoration of the application handler; the fixture does not measure terminal resizing/redisplay, Bash, or public C ABI behavior (artifact: `pinned-readline-8.3-measurements/readline_sigwinch.out`). A separate pinned-8.3 direct-library PTY fixture sent `SIGQUIT` after hook readiness in three isolated children: the caller-handler case returned `abc`, invoked the application handler once, and restored it; `SIG_DFL` terminated the child by `SIGQUIT`; and `SIG_IGN` returned `abc`, invoked no handler, and remained ignored. All three cases completed without timeout or cleanup kill; the pinned CI fixture asserts these outcomes and retains `pinned-readline-8.3-measurements/readline_sigquit.out`. These are direct-library fixture observations, not Bash or public C ABI claims. | Built-in `Terminal` selects eight signals and `SIGWINCH` independently (both on by default); restores changed dispositions, preserves `SIG_IGN`, chains `SIGWINCH`, and permits only one raw-mode terminal at a time (`WouldBlock` otherwise). Interrupted reads recheck hooks; non-INT signals restore/raise/re-enter. Built-in editors default to `SigintPolicy::Continue`: when the caller installed a non-default SIGINT handler, a caught signal is re-delivered through that restored handler, then a returning handler is followed by `^C` echo and continuation with the partial line and queued bytes. With `SIG_DFL`, the configured `Continue` or `Interrupted` policy handles the read without terminating the process. Hook-only reports and unquoted Ctrl-C input bytes follow the configured read policy but are not re-delivered. This matches the pinned GNU Readline 8.3 direct-library fixture under a simple caller handler and controlling PTY: input `abc`, Ctrl-C, `X` returned `abcX` (hex `61626358`) with exactly one caller-handler invocation. The same pinned-library controlling-PTY probe with `SIG_DFL` terminated by SIGINT before returning from `readline()`: the isolated child reported return code -2 (signal 2), no observation timeout, and captured `sigint> abc^C` with no trailing `X` or return record. With `SIGINT=SIG_IGN`, the probe returned `abcX` (hex `61626358`), reported zero application-handler calls, and observed SIGINT ignored before and after `readline()`. The CI oracle asserts all three outputs and retains them in `pinned-readline-8.3-measurements/readline_sigint.out`; they describe only these direct-library fixtures, including process termination in the default-disposition case. The claim is limited to the tested caller-handler, default, and ignored-disposition fixtures, not Bash behavior or C ABI compatibility. `Editor::set_sigint_policy(SigintPolicy::Interrupted)` explicitly retains the Interrupted result, discarding the partial line but queuing suffix bytes for the next read. Custom `TerminalIo` backends default to Interrupted unless they opt in via `TerminalIo::sigint_policy` or the editor setter; OS signal dispositions remain caller-owned when catching is disabled. `SIGQUIT` disposition follows host signal policy rather than key-binding precedence. Tests: [`signals.rs`](readline/tests/signals.rs), [`editor/tests/signals.rs`](readline/src/editor/tests/signals.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). The Bash-hosted interrupt oracle cases explicitly select Interrupted and remain separate from the direct-library fixture. | Known deviation |
| Completion redraw of invalid filename bytes | `output-meta` controls whether invalid filename bytes redraw as octal escapes or raw, with locale-dependent defaults. | Sushline follows the same locale-dependent defaults; explicit settings override them and accepted bytes remain raw. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Compatible |
| Undo insert grouping | GNU joins single-byte inserts into a prior insert entry up to 20 characters; longer and multibyte inserts begin entries. | Self-insert, literal, and tab inserts follow the 20-byte single-byte rule; multibyte scalars begin their own entries and incomplete input is buffered across events. The Bash PTY oracle compares one undo after 20 and 21 single-byte inserts and `aé`, yielding an empty line, 20 `a`s, and `a`, respectively; after `ab` plus `M-5 a`, one undo leaves `ab` and a second empties the line. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). | Compatible |

## High-Level Coverage

| Area | Status | Implemented surface | Boundary notes |
| --- | --- | --- | --- |
| Basic line editing | Mixed | Insert/delete/movement, undo/overwrite, marks/regions, macros, kill/yank, transpose, case conversion. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs). | Shell-word classes are hookable; terminal bytes depend on backend; character motion follows the boundary policy above. |
| Emacs keymap, bindable names, and `bind` | Mixed | Keymaps, inputrc bindings/macros, numeric arguments, command names and bind output/storage/query. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). | `bind -x` execution and application state are hook-backed. |
| vi mode | Mixed | Insert/command mode, movement/operators, marks, redo/search/put/yank and completion bindings. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). | External editing is hook-backed. |
| Init file/inputrc | Mixed | `set`, bindings/macros, conditionals and includes, with numeric-argument and abort handling in macros. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`inputrc.rs`](readline/src/inputrc/tests.rs). | Arbitrary shell/application state is not inferred. |
| Completion | Mixed | Default, filename, menu, listing, insertion, export, display, and options. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs). | Application names/quoting are hook-backed. Invalid-byte redraw follows the boundary row above. |
| History navigation/search | Mixed | Previous/next, prefix/substring and incremental/non-incremental search. For unstifled raw History appends, the pinned GNU History 8.3 `history_position_append.out` fixtures show sequential fresh additions retain position 0/current `alpha` with no previous entry, and additions at positions 1 and 3 retain those numeric positions (current `beta` and newly appended `delta`, respectively); Sushline matches these fixtures for both append APIs. The same artifact asserts stifled fixtures: limit 0 retains no entries; limit 1 keeps position 0 through eviction; and limit 2 preserves positions 0, 1, and 2 while evicting `beta` from `beta`/`gamma` and appending `delta`. Sushline matches these fixtures for both append APIs. Additional pinned fixtures seed three entries then stifle at limits 0, 1, 2 and 4, preserving selected offsets 1 or 3 through stifling, state round-trip and append, including offsets beyond the resulting length. Sushline preserves these offsets; cursor-relative navigation and searches beyond the end return `None` without moving the offset, while `beginning_bytes`, `end_bytes`, or a valid `set_pos` can recover. The pinned GNU probe avoids navigating from beyond-end offsets after direct calls showed `current_history`/`previous_history` can segfault there; no such GNU behavior is claimed. This is fixture-scoped evidence, not a general stifling or file-import claim. Automatic editor insertion separately resets navigation to the unpositioned end. Readline-editor `search-ignore-case` uses one-to-one lowercase scalars from an owned `LC_CTYPE` locale in supported UTF-8 locales; unavailable locale support or invalid UTF-8 falls back to ASCII byte folding. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`state/search.rs`](readline/src/state/search.rs). | Vi `/` and `?` stay case-sensitive with `search-ignore-case` on. The public History byte-search API remains ASCII-only. With `revert-all-at-newline` off, edited history text survived the tested navigation, prefix-search, and incremental/non-incremental search-overlay traversals in the Bash 5.3/Readline 8.3 PTY fixtures; this is fixture-scoped runtime evidence, not exact Bash patchlevel-0 or all-path evidence. A Sushline unit test covers restoring the edit's undo state. |
| History expansion | Mixed | Event/word designators, modifiers, quick substitution, policy, quote state and inhibition, including quote/backslash and tokenization handling. The direct History API matches the installed GNU History 8.2 measurements for event and word designators, known modifiers, substitutions, relative events, and quick substitution. CI retains the pinned GNU History 8.3 direct-library artifact and asserts exact outputs/statuses for the selected matching event/word, modifier, substitution, relative-event, quick-substitution, and error fixtures. It also asserts the pinned token, argument-extraction, and `!!:1` outputs for bounded extended-glob-shaped, nested, quoted/escaped, command/process-substitution, and arithmetic word fixtures; these are History word-handling observations, not shell parsing or glob expansion. The pinned artifact records status 0 for unchanged `echo ^a^b`, status 1 for an expanded event, and status 2 for print-only `!!:p`; `HistoryExpansionStatus` represents those three Rust outcomes while failures remain `Err`. The same artifact records the seeded `echo a b` event expanding the exact open-single-quote input `echo '!!` with status 1 and output `echo 'echo a b`; the pure History default matches it. Its separate `!!:u` fixture reports `u: unrecognized history modifier`, which Sushline preserves as a distinct expansion error message. The Readline editor keeps a separate quote-inhibition policy for the Bash 5.3 prompt fixture, which leaves `'!!'` unchanged. These direct-library and Bash-host observations are distinct and fixture-scoped. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`history.rs`](history/tests/history.rs), [`editor/tests/history.rs`](readline/src/editor/tests/history.rs). A separate Bash 5.3 `history -p` runtime fixture verifies that `:x` retains SP/TAB/LF separators and quotes empty fields while leaving CR/FF within fields; this is fixture-scoped runtime evidence, not standalone source proof. The print-only expansion result takes the same sink, because the Bash host's `history-expand-line` command writes it to standard error as well (Bash 5.3 observation), while the shell's `history -p` builtin writes it to standard output. The failure message is error text, so it goes to the diagnostic sink; the built-in terminal writes it to standard error. | Alias expansion and exact application lexer state are hook-backed; stateful substitution/search memory belongs to `HistoryExpansionState`. |
| History file storage | Mixed | Read/write/append/range/truncate, timestamps, multiline entries, and default-path helpers. By default each physical line is an entry of its own: the first line after a timestamp record carries that timestamp, the lines after it carry none, blank lines carry no entry, and trailing unterminated lines are ignored. The grouping is explicit configuration, with per-line and joined modes corresponding to GNU's `history_multiline_entries`; installed GNU History 8.2 informed exploratory comparisons, while pinned GNU History 8.3 CI asserts sixteen specific `read_history_range` fixtures in `pinned-readline-8.3-measurements`: `history_range_per_line.out`, `history_range_joined.out`, `history_range_plain_joined.out`, `history_range_late_record.out`, `history_range_blank_lines.out`, `history_range_blank_lines_joined.out`, `history_range_plain_blank_lines.out`, `history_range_plain_blank_lines_joined.out`, `history_range_blank_line_edges_timestamped.out`, `history_range_blank_line_edges_plain.out`, `history_range_blank_line_edges_joined_timestamped.out`, `history_range_blank_line_edges_joined_plain.out`, `history_range_edge_timestamped.out`, `history_range_edge_plain.out`, `history_range_oob_timestamped.out`, and `history_range_oob_plain.out`; the two out-of-range fixtures record GNU 8.3 returning no entries when FROM is past EOF and the remaining lines when TO is past EOF, matching Rust in the corresponding timestamped and plain test fixtures; Rust tests assert Sushline's grouping; the pinned artifact records fixture-scoped GNU 8.3 outputs, not broad equivalence. The separate pinned full-file fixture `A0\n#1700000000\nB0\n` returns three entries in both GNU 8.3 and Sushline, retaining the late marker-looking line as command text; this is fixture-scoped evidence. For the blank-line edge fixtures, both timestamped and plain artifacts report `multiline=0 from=1 to=1: (none)`, `multiline=0 from=2 to=1: {A1}`, `multiline=0 from=1 to=0: (none)`, and `multiline=0 from=1 to=2: (none)`. These are only the measured equal, reversed, and forward bounds touching the blank physical command line; they do not establish general range equivalence. Sushline retains its TO<FROM-to-EOF rule because changing it to GNU's tested results would discard remaining entries. The joined-mode edge artifacts report one empty-string entry (`{}`) for `(1,1)`, `(1,0)`, and `(1,2)`, and `{A1}` for `(2,1)` in both timestamped and plain fixtures. For the timestamped fixture, Sushline returns an untimestamped empty entry for the blank-only `(1,1)` and `(1,2)` ranges, matching GNU's entry output, and retains remaining entries for TO<FROM. For the plain fixture, it returns an empty entry for `(1,1)` and `(1,2)`, matching GNU, and retains `A1\nB0\nB1` for `(2,1)` and `\nA1\nB0\nB1` for `(1,0)`. These are fixture-scoped differences, not general GNU equivalence. In the timestamped blank-line fixture, GNU 8.3 returns A0/A1 in per-line mode and `A0\n\nA1` in joined mode; the blank consumes a range position without becoming an entry, and Sushline matches both outputs. The no-timestamp blank-line fixtures report the same range boundary: GNU returns A0/A1 in per-line mode and `A0\n\nA1` as one joined entry; Sushline matches both outputs. In the no-timestamp `history_range_plain_joined.out` fixture, GNU mode 1 and Sushline's `Joined` policy both return one `C0\nC1\nC2` entry ([`history_multiline_joined_joins_plain_range_file`](history/tests/history.rs)). For the tested range edges, pinned GNU 8.3 returns no entries for timestamped equal bounds (0..0 and 2..2) or reversed bounds (2..1), unlike Sushline's existing product rule; the tested plain-file bounds return C0, C2, and C2 respectively, matching Rust. The product rule is retained as a value-preserving distinction; these results are limited to the named fixtures; GNU blank-position counting is established only for the tested FROM<TO cases. Four pinned GNU History 8.3 full-file blank-line artifacts are `history_full_blank_timestamped_mode0.out`, `history_full_blank_timestamped_mode1.out`, `history_full_blank_plain_mode0.out`, and `history_full_blank_plain_mode1.out`. For these fixtures, per-line mode returns A/B for timestamped input and A/B/C for plain input; joined mode returns A and `B\n` for timestamped input, but one `\nA\n\n\nB\n\n\nC\n` entry for plain input. Sushline matches both per-line and both joined outputs for these fixtures. Direct pinned GNU History 8.3 full-file fixtures returned `read_history_status=ok` and truncated each tested command at its first NUL: `one\0hidden\nnext\0tail\nfinal\n` yielded `one`, `next`, `final`; a leading-NUL physical line was skipped while `next` remained readable; and the same result held in the timestamped fixture. The pinned CI artifact `pinned-readline-8.3-measurements` contains `history_full_nul_plain.out`, `history_full_nul_leading.out`, and `history_full_nul_timestamped.out` for these fixtures. The pinned fixture `#1700000000\0suffix\nalpha\nbeta\n` is recorded in `pinned-readline-8.3-measurements/history_timestamp_nul.out`: GNU returns `alpha` and `beta`, truncates the first stored timestamp to `#1700000000`, and gives the second entry a run-dependent `#`-numeric timestamp; with default `history_comment_char=0`, both `history_get_time` results are zero. Sushline retains the decoded `#1700000000\0suffix` metadata on `alpha`; its `gnu_timestamp_text()` view returns `#1700000000`, matching this fixture, and `beta` has no imported timestamp. Separately, Rust preserves NUL bytes in command-line payloads. Concurrent writers are last-writer-wins, range bounds count blank physical command lines but not timestamp records, `History::truncate_file` selects raw bytes by physical LF count, independent of entry grouping: if the count is at most `max_len`, it preserves the exact input bytes without rewriting (but still sets owner-only mode `0600` on Unix); otherwise, limit 0 empties the file, and a nonzero limit retains the exact suffix after skipping `LF_count - max_len` LF bytes, including any unterminated tail. Timestamp records and blank lines count. Pinned GNU History 8.3 (`history_multiline_entries=0`) showed this rule for the tested bytes only: the primary and independent replica reports agreed byte-for-byte across 27 fixture/limit pairs, with two successful trials each: `pinned-readline-8.3-measurements/history_truncate_primary.out` and `pinned-readline-8.3-history-truncate-replica/history_truncate_replica.out` (identical report digest `a5c9a58b3cf3158a453c465e954819794484043fb048ae811f8e8f1cb9bf3529`). The exact fixtures and limits are in [the probe](.github/scripts/readline-history-truncate.sh): plain blank (10, 1), plain unterminated (10, 1, 0), plain forced unterminated (1, 2), timestamped two-block (10, 1, 2, 3, 4), single-block (1, 2), blank (1, 2), dangling marker (0, 1, 2), consecutive markers (1, 2), unterminated command (1, 2), unterminated marker (1, 2), range-bounds (1), and post-append (2). For the two-block input `#1700000000\nA0\nA1\n#1700000001\nB0\nB1\n`, limit 10 preserves the input and limits 1, 2, 3, and 4 yield respectively `B1\n`, `B0\nB1\n`, `#1700000001\nB0\nB1\n`, and `A1\n#1700000001\nB0\nB1\n`. For plain inputs, `A\n\nB\n` is unchanged at 10 and yields `B\n` at 1; `A\nB` is unchanged at 10 and 1 and empty at 0; `A\nB\nC` yields `B\nC` at 1 and is unchanged at 2. For timestamped EOF tails, `#1700000000\nA\n#1700000001\nB` yields `#1700000001\nB` at 1 and `A\n#1700000001\nB` at 2; `#1700000000\nA\n#1700000001` yields `A\n#1700000001` at 1 and is unchanged at 2. The existing range-bounds fixture `#1700000000\nline one\n\nline two\n#1700000001\nnext\n` yields `next\n` at 1; the post-append fixture `#1700000000\necho one\n# not timestamp\n#1700000001\nprintf two\n#1700000001\nprintf two\n#1700000002\nprintf three\n` yields `#1700000002\nprintf three\n` at 2. Sushline's [`truncate_file_preserves_noop_bytes_and_truncates_when_needed`](history/tests/history.rs) and [`truncates_timestamped_history_with_measured_gnu_bytes`](history/tests/history.rs) tests assert the corresponding plain and timestamped bytes; the other two fixtures are exercised by their history tests. These results do not establish general GNU equivalence beyond the tested inputs and limits. Regular-file rewrites set `0600` on Unix and publish by atomic rename; non-regular writes truncate in place. On Unix and Windows, successful output files are explicitly closed and close errors are returned: atomic replacement skips rename, while append/in-place destinations may already have changed. No fsync or crash durability is promised; other Rust targets retain drop-only close behavior, and reads reject non-regular destinations. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). | Known deviations: In the pinned fixtures GNU History 8.3 truncates tested command lines and timestamp text at NUL. Rust preserves command-line NUL bytes and retains the decoded timestamp suffix (with invalid UTF-8 replaced); `gnu_timestamp_text()` matches the pinned timestamp prefix, while `beta` has no imported timestamp. |

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
| `history-search-backward`, `history-search-forward`, `history-substring-search-backward`, `history-substring-search-forward` | Compatible | Direction, repeat, case, abort/accept, bell, and cursor behavior. Editor searches use locale-aware lowercase matching with `search-ignore-case` on and stay case-sensitive otherwise. Numeric counts repeat, reverse, or no-op with excessive counts stopping at the nearest match; prefix search skips the selected entry and failed forward search preserves the line; failed navigation rings once. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `magic-space` | Compatible | Built-in expansion uses GNU-default policy; custom characters/policy are direct-API only. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `history-expand-line` | Compatible | Built-in expansion uses GNU-default policy; custom characters/policy are direct-API only. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs). |
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
| `digit-argument`, `universal-argument` | Compatible | Multi-digit chaining, sign handling, and universal-argument composition. A numeric argument may not exceed 1000000, the universal argument included: a larger one rings the bell and resets the argument, and the digits that follow the reset are ordinary text (Bash 5.3 PTY observations on the host's Readline 8.3). A pending argument is drawn in place of the whole prompt, as `(arg: N) `, and the value grows with the argument, as in `(arg: 34) `; a sign-only prefix shows its sign, as in `(arg: -1) `, a leading zero shows `(arg: 0) `, and the universal argument shows `(arg: 4) `. The mode indicator belongs to the prompt and is replaced with it. The prompt returns when the argument is consumed, and the line's text follows the indicator (Bash 5.3 PTY observations on the host's Readline 8.3, compared screen by screen). That comparison needs a host that carries the 8.3 release, because the 8.2 release leaves the prompt on the screen; the cases report nothing on a host with another release. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`editor/tests/display.rs`](readline/src/editor/tests/display.rs). |
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
| `export-completions` | Compatible | In the `export_completions_bash_fixture_reports_utf8_byte_range_and_accepts_line` PTY fixture, Bash and Sushline report count `3`, prefix `al`, byte range `9:11`, and candidates `alp`, `alpha`, `alpine`; Bash accepts callback args `é al` and Sushline leaves `probe é al` unchanged. Tests: [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`completion.rs`](readline/tests/completion.rs). |

### Miscellaneous Commands

| Command(s) | Status | Notes |
| --- | --- | --- |
| `re-read-init-file`, `abort`, `do-lowercase-version`, `undo`, `revert-line`, `set-mark`, `exchange-point-and-mark`, `skip-csi-sequence`, `dump-functions`, `dump-variables`, `dump-macros`, `execute-named-command`, `emacs-editing-mode`, `vi-editing-mode` | Compatible | Observable editing, reload and dump behavior; `do-lowercase-version` rings when the key has no case variant, empty or exhausted `undo` rings once, `revert-line` with no changes rings once, `abort` rings while preserving the line, and `exchange-point-and-mark` with no mark stays silent. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| Explicit `emacs-meta` / `emacs-ctlx` bindings | Compatible for measured Bash-hosted fixtures | Unconditional simple and nested meta macros/functions and ctlx macros/functions dispatch through ESC/C-x from the standard map. Root and submap rebinding address the same binding, defaults remain visible, and bindings on ESC/C-x or a nested prefix do not disconnect descendants; unbound continuations use the prefix macro fallback. The finite Bash-hosted paired fixtures also check relative reusable macro listing (including empty-sequence fallback last), nonreusable `-S` rendering for printable, control, quoted, and backslash keys with macro-payload escaping, function query versus listing punctuation, and cross-view removal. `-S` prints unquoted key spellings ordered by sequence bytes; reusable `-s` retains quoted inputrc syntax. Local tests cover fallback-last ordering in both macro formats. Local tests cover shared user-override identity, command removal, and application-command listing/removal. Named submaps are views of prefixed standard-map storage, not independently mirrored maps. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs), [`keymap/tests.rs`](readline/src/keymap/tests.rs), [`bind/tests.rs`](readline/src/bind/tests.rs). This is not a standalone pinned-library measurement or a claim about arbitrary keymap trees or switching. |
| `prefix-meta` | Sushline extension | Sushline-only command; no default key binding, so it affects behavior only when explicitly bound in inputrc. Physical ESC input is separate. Tests: [`keymap/tests.rs`](readline/src/keymap/tests.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
| `arrow-key-prefix` | Compatible | Accepted CSI-skip command. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `display-shell-version`, `tty-status` | Hook-backed | Status supplied by hooks. Tests: [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `shell-expand-line`, `spell-correct-word`, `edit-and-execute-command` | Hook-backed | Application services use hooks. Tests: [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| Application command bindings (`bind -x`) | Hook-backed | BindApi stores, queries, unbinds and prints; dispatch uses `Hooks::on_command`. Tests: [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
| `tilde-expand` | Compatible | Current-word expansion including whitespace, assignment-like words, and `~+`/`~-`; named-user trailing-slash handling is included. Tests: [`editing.rs`](readline/tests/editing.rs), [`oracle/completion.rs`](readline/tests/oracle/completion.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs). |
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
| `vi-fetch-history`, `vi-eof-maybe`, `vi-goto-mark`, `vi-set-mark`, `vi-tilde-expand`, `vi-unix-word-rubout`, `vi-yank-arg` | Compatible | Fetch, EOF, tilde, mark, default register-command, rubout and numeric yank-arg behavior; `vi-goto-mark` with no mark rings once, a vi mark is named by one lowercase ASCII letter and any other name rings once, with `vi-tilde-expand` trailing-slash handling as in the `tilde-expand` row above. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`vi_mode.rs`](readline/tests/vi_mode.rs). |
| `vi-edit-and-execute-command` | Hook-backed | External edit behavior uses `Hooks::edit_and_execute`. Tests: [`bind.rs`](readline/tests/bind.rs), [`bind_golden.rs`](readline/tests/bind_golden.rs). |
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
| `$if`, `$else`, `$endif` | Compatible | Mode, term, version and application conditions, plus library-owned variable string tests with `=`, `==`, and `!=` (see fixture scope below). Unknown names are false even with `!=`; arbitrary shell/application state is not inferred. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs). |
| `$include` path rules | Compatible | CWD-relative includes, `~/` expansion via `HOME`, literal `$HOME`, and skipping missing includes. A direct pinned GNU Readline 8.3 library fixture measured each in two fresh trials: CWD-relative paths resolve from the process CWD, `~/` uses controlled `HOME`, a literal `$HOME` filename is read without expansion, and parsing continues after a missing include. Every trial returned `read_status=0`, emitted no stderr, reported no timeout, exited 0, and cleaned up successfully; paired trials agreed. These results are limited to the test-owned fixtures. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs); pinned artifact: `pinned-readline-8.3-measurements/readline_include_paths.out`. |
| `$include ~user/<relative path>` for a known account | Compatible | A test-owned inputrc file under the current account's home loads `completion-ignore-case on` in Sushline and the scoped GNU Readline 8.3 direct-library fixture. Sushline CI requires the test to run; two independent pinned reports match on release, read status, loaded variable, completion, and cleanup (the later report repeats two fresh trials). This is fixture-scoped and does not establish behavior for arbitrary users. Tests: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs); pinned artifact: `pinned-readline-8.3-measurements/readline_named_include.out`. |
| Unknown or unavailable `~user` account data | Implemented | Sushline's best-effort pure-Rust lookup silently skips an unknown user or unavailable account data. For a controlled nonexistent-account include, pinned GNU Readline 8.3 reported `read_status=0`, no stderr, and applied a later `editing-mode vi` directive in both trials (artifact: `pinned-readline-8.3-measurements/readline_named_include.out`); this is fixture-scoped evidence. Behavior for other unavailable account data remains unmeasured against 8.3. The unknown-user case is tested. Tests: [`inputrc/tests.rs`](readline/src/inputrc/tests.rs). |
| Recursive `$include` cycles | Known deviation | The two-file `A → B → A` direct-library fixture on pinned GNU Readline 8.3 ended by signal 11 (`-11`) in both fresh bounded trials (artifact: `pinned-readline-8.3-measurements/readline_include_cycle.out`); the interactive-shell self-cycle fixture exits with wait status 139 before the first prompt. Sushline reports `inputrc include depth exceeded`, which the editor records and continues from, rather than reproducing GNU's crash. Test-owned acyclic chains of 18 and 32 nested includes were accepted by both Sushline and pinned GNU Readline 8.3, with the leaf variable loaded. For depth 32, both independent pinned reports agree on two fresh direct-library trials each: release 8.3, successful read, loaded sentinel, and cleanup (`pinned-readline-8.3-measurements/readline_include_depth_32.out` and `pinned-readline-8.3-include-depth-replica/readline_include_depth_32.out`); the depth-18 report is `pinned-readline-8.3-measurements/readline_include_depth.out`. Sushline's finite cap is 32; the 33-deep test retains a recursion guard, while behavior beyond 32 is unmeasured. A variable-conditioned recursive include also terminates after the variable changes, so filename-only cycle rejection would reject valid input. Findings are fixture-scoped. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`inputrc/tests.rs`](readline/src/inputrc/tests.rs). |
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
| Bracketed-paste CR normalization and incomplete streams | Implemented | Completed pastes normalize embedded carriage returns to line feeds; split and truncated terminators buffer partial payloads in the editor. A separate bounded direct-library PTY probe linked to pinned GNU Readline 8.3 returned `A\nB` for a paste containing `A\rB` and `split` for a split terminator in two independent reports, aligning with Sushline's tested normalization and buffering behavior. This observation is limited to the direct-library fixture and makes no Bash or public C ABI claim. Tests: [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`editor/tests/lifecycle.rs`](readline/src/editor/tests/lifecycle.rs), [pinned direct-library probe](.github/workflows/ci.yml). |
| `enable-keypad` | Compatible | Defaults off; explicit on emits available terminfo sequences around editing with explicit off emitting none. Tests: [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs), [`terminfo.rs`](readline/src/terminal/terminfo.rs). |
| `emacs-mode-string`, `vi-cmd-mode-string`, `vi-ins-mode-string`, `show-mode-in-prompt` | Compatible | Prompt rendering. Tests: [`variables.rs`](readline/tests/variables.rs). |
| `horizontal-scroll-mode` | Known deviation | In the tested Bash 5.3 PTY fixtures, the visible window, cursor, hysteresis and restoration match Sushline for ASCII in bytewise and batched input. The Bash-hosted observations count scroll offsets in bytes: at 80 columns, batched input shows the last 51 line bytes with the cursor one column past that window, while bytewise ASCII keeps the cursor at the right edge (76 visible columns, cursor-after-cells 77); a narrow multibyte tail leaves 50 columns (cursor-after-cells 51). Sushline also measures offsets in bytes and matches the tested cell/cursor outcomes for ASCII, narrow-multibyte, combining-mark, ambiguous-width and batched wide-character tails. For the wide edge fixture, two independent direct-library Readline 8.3 reports (primary and fresh-runner replica, two trials each; artifacts `pinned-readline-8.3-measurements/readline_horizontal_render.out` and `pinned-readline-8.3-horizontal-render-replica/readline_horizontal_render.out`) record a frame beginning `\r<\xb8\xad`: the glyph's lead byte is absent and the frame is invalid UTF-8, while Sushline clips the cell and keeps its output valid UTF-8. The bytewise wide-tail fixture shows 46 leading digits for Bash and 49 for Sushline (cursor columns 49 and 52). For the multiline `one\nP>` fixture at 76 ASCII bytes, the Bash PTY under a UTF-8 locale overwrites only the first cell with `<` and leaves prior prompt text, while Sushline repaints the window; the rows agree under the C locale. The pinned direct-library reports instead record `one\r\nP>` followed by 76 one-byte `a` updates and no cursor-motion or scroll controls, yielding row `P>` plus 76 `a` bytes and zero-based cursor column 78. Thus the multiline difference is a Bash-hosted observation, not established direct-library behavior. Unmarked ANSI prompt escapes count as logical columns; findings are fixture-scoped. Tests: [`oracle/horizontal_scroll.rs`](readline/tests/oracle/horizontal_scroll.rs), [`display.rs`](readline/tests/display.rs), [`horizontal_scroll.rs`](readline/src/display/horizontal.rs). |
| `history-preserve-point`, `history-size`, `mark-modified-lines`, `revert-all-at-newline`, `isearch-terminators`, `keyseq-timeout` | Compatible | Editor/history/display/input behavior. Tests: [`variables.rs`](readline/tests/variables.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| `search-ignore-case` (editor searches) | Compatible | Readline editor searches use owned-locale lowercase matching for valid UTF-8 in supported UTF-8 locales with ASCII byte folding otherwise. Vi `/` and `?` remain case-sensitive. Tests: [`oracle/history.rs`](readline/tests/oracle/history.rs), [`oracle/editing.rs`](readline/tests/oracle/editing.rs), [`state/search.rs`](readline/src/state/search.rs). |
| `search-ignore-case` (public History byte API) | Sushline extension | The explicit public `History` byte-search API folds ASCII only; it is separate from the editor inputrc variable, which uses locale-aware matching. Tests: [`history.rs`](history/tests/history.rs). |
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

Numeric inputrc storage is covered by finite Bash 5.3-hosted Readline 8.3
fixtures, not a standalone pinned 8.3 library comparison. The earlier
four-spelling fixtures for `completion-display-width`,
`completion-prefix-display-length`, `completion-query-items`, `history-size`,
and `keyseq-timeout` observe canonical stored strings through equality
conditions. In particular, RHS `0100` and `+100` do not match after setting
`0100`: storage is normalized, not the comparison operands.

The paired [`oracle/inputrc.rs`](readline/tests/oracle/inputrc.rs)
`bash_bind_v_and_sushline_store_measured_numeric_matrix` fixture covers the
following eighteen spellings for all five settings. Each spelling runs in a
bounded fresh-HOME Bash PTY; a startup inputrc file seeds each setting with
`100` and then applies the tested value. The test asserts a file-only
`comment-begin` sentinel to check startup loading and inspects exact `bind -v`
`set <name> <stored>` records, without setting values through `bind`.
Sushline's inputrc loader and local
[`inputrc/tests.rs`](readline/src/inputrc/tests.rs) parser tests assert the
same stored strings. The final column applies separately to
`completion-prefix-display-length`, `completion-query-items`, and
`keyseq-timeout`.

| Input spelling | `completion-display-width` | `history-size` | Each of the other three settings |
| --- | --- | --- | --- |
| `100`, `0100`, `+100` | `100` | `100` | `100` |
| `-1` | `-1` | `-1` | `0` |
| `many` | `0` | `0` | `0` |
| `100junk`, `+100junk` | `100` | `100` | `100` |
| `-100junk` | `-100` | `-1` | `0` |
| `-2` | `-2` | `-1` | `0` |
| `2147483647` | `2147483647` | `2147483647` | `2147483647` |
| `2147483648` | `-2147483648` | `-1` | `0` |
| `4294967295` | `-1` | `-1` | `0` |
| `4294967296` | `0` | `0` | `0` |
| `-2147483648` | `-2147483648` | `-1` | `0` |
| `-2147483649` | `2147483647` | `2147483647` | `2147483647` |
| `-4294967296` | `0` | `0` | `0` |
| `9999999999999999999999999999999999999999` (40 digits) | `-1` | `-1` | `0` |
| `-9999999999999999999999999999999999999999` (minus and 40 digits) | `0` | `0` | `0` |

The implementation's signed decimal prefix, saturating 64-bit parse, 32-bit
conversion, and setting-specific negative clamps match these fixtures only.
They do not establish a general C/`atoi` grammar, unmeasured range behavior,
or cross-architecture compatibility. Nonnumeric string settings remain
unchanged.

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

The pinned GNU History 8.3 CI measurement records bounded `history_tokenize`, `history_arg_extract(1, 1, input)`, and seeded-entry `!!:1` expansion output (hex, stable labels) for five extended-glob-shaped prefixes plus nested, quoted, escaped, command-substitution, process-substitution, and arithmetic controls. For the measured unquoted forms, all five prefixes and the nested form remain one token; quoted and escaped forms, substitutions, and arithmetic retain their separately measured boundaries. Rust tests compare tokenization, extraction, and word selection with these fixture-scoped results; they do not claim glob expansion or Bash parsing. The measurement was recorded before a later, separate SIGINT probe compilation error failed that oracle job. Existing Bash 5.3-hosted and installed-8.2-scoped rows above remain separate.

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
| List management: `add_history`, `add_history_time`, `remove_history`, `replace_history_entry`, `clear_history`, `stifle_history`, `unstifle_history`, `history_is_stifled` | `push`, `push_bytes`, `add_time`, `remove`, `replace`, `clear`, `stifle`, `unstifle`, `is_stifled` | Compatible | Empty entries, timestamp targeting, stifle limits and replacement/undo/timestamp behavior; a timestamp set through `add_time` is the record's text, the `#`-plus-digits form; replace keeps the cursor, removing an invalid index is a cursor-preserving no-op, and auto-add skips empty lines. Tests: [`history.rs`](history/tests/history.rs). |
| List information: `history_list`, `where_history`, `current_history`, `history_get`, `history_get_time`, `history_total_bytes` | `entries`, `where_history`, `current_history`, `get`, entry `timestamp`, `timestamp_seconds`, `total_bytes` | Mixed | The entries are numbered from zero here and from one in the C library: the measured library answers nothing for `history_get(0)` and the first entry for `history_get(1)`, so `get` and `history_get` name the same entry at different indexes, and the oracle job measures the same against the pinned 8.3 release's own history library. `timestamp_seconds` parses the stored `#` record the way the C library's number conversion reads it and returns `Option<i64>`, preserving zero separately from missing or out-of-range values: the conversion's whitespace (space, tab, newline, vertical tab, form feed, carriage return) is skipped, a sign is accepted, a decimal prefix is read, and a value that does not fit `i64` has no value. GNU's `history_get_time` checks the mutable `history_comment_char` and returns zero when it does not match or conversion fails; the installed GNU History 8.2 library was measured with a zero default, so a `#` record returned zero until the character was set to `#`. Readline 8.3 source checks that character and uses `strtol`, and the oracle job measures that rule against the pinned 8.3 release's own history library. Tests: [`history.rs`](history/tests/history.rs). |
| Navigation: `history_set_pos`, `previous_history`, `next_history` | `set_pos`, `previous_history`, `next_history` | Compatible | Rust-owned position navigation. Tests: [`history.rs`](history/tests/history.rs). |
| Search: `history_search`, `history_search_prefix`, `history_search_pos` | `history_search_bytes`, `history_search_prefix`, `history_search_pos` | Mixed | The two integers of `history_search_pos` are read in the opposite order to the one the C library's header documents: the measured library takes the direction first and the position second, and this library's `history_search_pos` takes them in that order, so a caller following the header would pass them the other way round. The position is an absolute index counted from zero, the search includes the position it starts at, and the direction is negative to search backwards, while this library takes the direction as an enumeration and answers with the entry and the offset in it. A failed search preserves the cursor and an out-of-range `history_search_pos` falls back to the current cursor. Measured against the installed GNU History 8.2 library, and the oracle job measures the same against the pinned 8.3 release's own history library. Tests: [`history.rs`](history/tests/history.rs). |
| Case-insensitive search helper | `history_search_bytes_with_case` | Sushline extension | Explicit ASCII-only case folding; GNU History search has no equivalent option. Tests: [`history.rs`](history/tests/history.rs). |
| Files: `read_history`, `write_history`, `append_history`, `history_truncate_file`, default `~/.history` filename | `read_file`, `read_file_with_multiline`, `load_file`, `load_file_with_multiline`, `write_file`, append variants, `truncate_file`, default path/helpers | Mixed | Regular/special files, timestamps and `~/.history` helpers; concurrent writers are last-writer-wins, timestamp delimiters use the raw `#`-plus-digits prefix with lossy U+FFFD metadata handling; On file input, decoded timestamp metadata retains the NUL suffix; `gnu_timestamp_text()` exposes the prefix matching the pinned GNU History 8.3 fixture `pinned-readline-8.3-measurements/history_timestamp_nul.out`, where Sushline leaves `beta` without an imported timestamp. Command-line NUL preservation is separate. The writer emits an entry's stored timestamp as the record's text, so a timestamp set through `add_time` is written as the record the reader recognizes, and `truncate_file` instead selects raw bytes by physical LF count, independently of entry grouping: if the count is at most `max_len`, it preserves the exact input bytes; otherwise, limit 0 empties the file and a nonzero limit keeps the suffix after skipping the excess LF bytes, including any unterminated tail. Timestamp records and blank lines count as physical lines; the GNU match is limited to the tested fixtures and limits described above. The grouping of a file's physical lines into entries is configuration (`HistoryMultiline`): the default is one entry per physical line and `Joined` groups the selected physical lines of each block into one entry; for a file in GNU's timestamp format, whose first line is a timestamp record, the two settings correspond to the two modes of GNU's `history_multiline_entries`, measured against the installed GNU 8.2 history library, apart from a blank line inside a block, which the joined setting keeps and GNU's mode 1 drops. Files that do not begin with a timestamp record are treated as command files: both this library and GNU read a later `#`-plus-digits line as command text. The pinned GNU 8.3 full-file and range fixtures confirm this for `A0`, `#1700000000`, `B0`. GNU mode 1 joins a plain command file into one entry. Sushline's `HistoryMultiline::Joined` matches the pinned GNU History 8.3 bytes for the plain range fixture `history_range_plain_joined.out`, the plain blank-range fixture `history_range_plain_blank_lines_joined.out`, and the blank-containing full-file fixture `history_full_blank_plain_mode1.out`; this is fixture-scoped evidence, not a general equivalence claim. The writer emits a timestamp record before an entry that carries one, so if its first entry has no timestamp and a later entry does, the output is a command file by the first-line rule; both GNU mode 0 and this reader return the later record as command text rather than metadata. Tests: [`history.rs`](history/tests/history.rs). |
| File-import cursor fixtures | `read_file`, `read_file_range`, `load_file`, `load_file_range` | Compatible | For the three-line plain-file fixtures in [`history-import-cursor.expected`](.github/scripts/history-import-cursor.expected), pinned GNU History 8.3 and Sushline preserve the starting numeric offset: full import into empty history starts at 0, and full import into old-a/b/c retains offsets 0, 2 and 3; range import retains offset 1. With stifle limit 2, full imports retain offsets 1 and 3 and the range import retains offset 3 as the list becomes file-beta/file-gamma; 3 is beyond the new end. These claims cover only the tested files, bounds and positions. The CI probe checks the pinned libraries and avoids navigation or current-entry queries at invalid offsets. Tests: [`history.rs`](history/tests/history.rs), [`history-import-cursor.sh`](.github/scripts/history-import-cursor.sh). |
| File range: `read_history_range` | `read_file_range`, `read_file_range_with_multiline`, `load_file_range`, `load_file_range_with_multiline` | Mixed | Range and non-regular input behavior. FROM/TO bounds count physical command lines, including blanks; timestamp records are metadata and do not count. For FROM below TO, TO is exclusive; the selected physical-line span is then grouped by policy, so a range can end with a partial block. Blank lines count toward the bounds; they yield no entry in per-line mode but are preserved in joined spans and can form an empty entry when selected alone. Pinned GNU History 8.3 fixtures `history_range_blank_lines.out`, `history_range_blank_lines_joined.out`, `history_range_plain_blank_lines.out`, and `history_range_plain_blank_lines_joined.out` show blank lines consume a range position but do not themselves yield an entry in per-line mode; Sushline matches those range boundaries, while joined outputs may retain an interior blank separator. For the pinned plain-file fixtures `history_range_plain_joined.out` and `history_range_plain_blank_lines_joined.out`, GNU mode 1 and Sushline's `Joined` both return one joined entry; the tested timestamped range fixtures also match for per-line and joined grouping. These are fixture-scoped observations. For the pinned 8.3 timestamped edge fixture, equal/reversed bounds return no entries, unlike Sushline's retained-lines rule; the no-blank plain edge fixture returns C0, C2, and C2 and matches Rust. In the blank-line joined edge fixtures, GNU records empty entries for blank-only spans; Sushline matches the tested plain equal/forward cases, while timestamped blank-only ranges and the TO<FROM retained-through-EOF rule remain fixture-scoped differences. The specific out-of-range fixtures `history_range_oob_timestamped.out` and `history_range_oob_plain.out` record FROM past EOF returning none and TO past EOF returning remaining lines, matching Rust for those timestamped and plain fixtures. Sushline retains its equal/reversed-bound rule as a value-preserving distinction for the tested timestamped cases (artifacts: `history_range_edge_timestamped.out` and `history_range_edge_plain.out`). Tests: [`history.rs`](history/tests/history.rs). |
| Expansion: `history_expand` | `expand_history`, status/state variants, `HistoryExpansionState`, `Hooks::expand_history` | Compatible | Text and print-only status; Editor context forwards session state to hooks. Tests: [`history.rs`](history/tests/history.rs), [`oracle/history.rs`](readline/tests/oracle/history.rs). |
| Expansion helpers: `get_history_event`, `history_tokenize`, `history_arg_extract` | `get_history_event`, `history_tokenize`, `history_arg_extract`, `command_words` | Compatible | Rust helpers exposed. Tests: [`history.rs`](history/tests/history.rs). |
| Variables: `history_base`, `history_length`, `history_max_entries` | `HistoryState` and methods | Compatible | Rust-owned offset, length, stifle and maximum-entry behavior. Tests: [`history.rs`](history/tests/history.rs). |
| Variables: `history_expansion_char`, `history_subst_char`, `history_comment_char`, `history_word_delimiters`, `history_no_expand_chars`, `history_quotes_inhibit_expansion` | `HistoryChars`, `HistoryExpansionPolicy` | Compatible | Available to expansion API and editor expansion. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_write_timestamps` | Timestamp write/append methods | Compatible | Enabled or suppressed per call. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_quoting_state` | `HistoryExpansionPolicy::quote_state` | Compatible | Exposed through Rust policy object. Tests: [`history.rs`](history/tests/history.rs). |
| Variable: `history_inhibit_expansion_function` | `expand_history` inhibit predicate | Compatible | Per-call predicate rather than process-global pointer. Tests: [`history.rs`](history/tests/history.rs). |
