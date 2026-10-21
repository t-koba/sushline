# Sushline Design

Sushline is a facade crate over separate line-editing and history
implementations.

## Crate Layout

```text
sushline/
  Cargo.toml
  src/lib.rs
  readline/
  history/
```

Public modules:

- `sushline::readline`: editor, keymaps, inputrc, bind, completion, terminal
  I/O, prompt handling, hooks, and editor-owned history types.
- `sushline::history`: history entries, navigation, search, state/stifle APIs,
  file persistence, timestamps, and history expansion. This is a thin alias to
  the history crate; `sushline::readline::History` is the canonical path when
  embedding an `Editor`.

Embedders should depend on the root crate.

## Internal Crates

The `sushline-readline` package owns interactive line editing.

Important source areas: `editor.rs`, `state/`, `input.rs`, `keymap/`,
`bind.rs`, `inputrc.rs`, `buffer/`, `command/`, `completion/`, `display.rs`,
`prompt.rs`, `width.rs`, `terminal.rs`, `terminal/`, and `hooks.rs`.

The `sushline-history` package owns history data structures and algorithms.
Important source areas: `lib.rs`, `file.rs`, and `expansion.rs`. History save
policy belongs to the embedder.

## Text and Byte Model

Editable input is stored as bytes. `ReadlineResult::Line`, `LineBuffer`,
history entries, completion replacements, key sequences, keyboard macros, and
most hook payloads use `Vec<u8>` or `&[u8]`.

Rationale:

- Interactive command lines are not guaranteed to be valid UTF-8. History files,
  shell words, terminal input, and Unix pathnames can contain arbitrary bytes.
- Readline exposes several byte-oriented behaviors, including byte offsets,
  key-sequence matching, macros, quoted insert, and byte movement commands.
- Embedders often need to pass the accepted line to a parser or process layer
  without lossy transcoding.

## Application Integration

Sushline accesses embedding-program behavior through
`sushline::readline::Hooks`.

Main hook types are `CommandContext`, `HistoryExpansionContext`,
`LineExpansionContext`, `SpellCorrectionContext`, `QuoteContext`,
`CompletionRequest`, `CompletionResponse`, and `Edit`. Hook methods cover
line expansion, history expansion, completion sources, completion quoting,
glob expansion, command/user/host/variable name sources, shell word policy,
application commands, status text, spelling correction, signals, and version
text.

`Hooks` is intentionally a single trait with default methods. `Editor` does
not store hooks; each `read_line` call receives `&mut impl Hooks`, which lets an
embedder borrow application state for exactly one read. Hook methods use
`&mut self` uniformly because real embedders commonly update shell state,
completion caches, or signal flags while answering a request.

The default `expand_history` implementation runs sushline's built-in history
expander with the `HistoryExpansionContext::expansion_state` session and returns
`HistoryExpansion`, including print-only status. Embedders that want to suppress
expansion return `HistoryExpansion::unchanged(line)`.
Completion uses two distinct hooks: `complete` is programmable completion, and
`default_complete` is the application-owned default path used when no compspec
exists or when an empty programmable result requests `bashdefault`.

Most hook payloads are byte-oriented. Command, user, host, and variable name
sources return `Vec<Vec<u8>>`; `glob_expand` accepts bytes; shell word hooks are
split into `shell_word_spans` for editing by word ranges and `shell_words` for
history-word commands. The default `shell_words` derives words from valid
spans, but a words-only hook does not affect shell-word movement.

The current hook surface consists of signal/status hooks, command interception,
line expansion, alias expansion, external editing, spelling correction, history
expansion, programmable and default completion, completion quoting, glob
expansion, command/user/host/variable name sources, completion/editing word
breaks, and shell word spans/words.

`bind` and inputrc accept function names from multiple functional areas.
Editor-owned functions are handled internally; application-owned functions are
dispatched through hooks.

## Editor Runtime State

`Editor` owns configuration, terminal I/O, keymaps, variables, and the shared
history object. A single `read_line` call creates an `EditorState` for the
active line.

`EditorState` holds per-line runtime state: the line buffer, pending key bytes,
numeric argument, mark, undo stack, search state, completion state,
vi state, keyboard macro replay flags, bracketed paste state,
display state, and the original line, plus per-line kill/yank flags and yank
ranges. This keeps mutable
editing state scoped to the active read while allowing keymaps, variables,
terminal setup, and history to persist across reads. History-expansion
substitution memory (`&` and empty-old reuse) plus the most-recent substring
search and its matched word (`!??` reuse, `%`, empty-old search fallback) is
`Editor`-owned `HistoryExpansionState` rather than per-line state, so
successive `history-expand-line` / `magic-space` expansions share GNU
History-library memory across commands and reads; direct
`history::expansion` callers keep one `HistoryExpansionState` per expansion
session and use the `*_with_state` APIs. Fresh states fail closed for both
unseeded `&` and unseeded `!??`; failed searches keep the remembered needle.
Killed text itself is `Editor`-owned: the kill-ring contents persist across
reads while each new read resets yank ranges and last-command flags, so kills
do not coalesce across prompts and `yank-pop` needs a fresh `yank`. The last
recorded keyboard macro and an unfinished keyboard-macro recording are
likewise `Editor`-owned, so `call-last-kbd-macro` replays across reads and a
recording started at one prompt continues at the next, like GNU Readline.
Replay flags stay per-line, and a macro replay that accepts a line keeps its
remaining bytes for the next prompt. Completed incremental and
non-incremental history-search memories (each query and direction, plus the
non-incremental match position used by `-again`) are likewise `Editor`-owned
and persist independently across reads; emacs and vi non-incremental search
share the latter, so vi `n`/`N` continues that query. Active searches, pending
operators, counts, marks, and insert recording stay per-line. The vi character-search target and last change
are also `Editor`-owned, so search-again, character-search repeat, and dot
repeat work on the next prompt.

## Terminal Boundary

Terminal access is isolated behind `TerminalIo`. The editor consumes
`TerminalEvent` values and writes through terminal methods instead of reading
from stdin or writing escape sequences directly throughout the command code.

On Unix, the concrete `Terminal` handles raw-mode setup and restoration,
terminal size, resize events, byte input, signal events, visible bell, keypad
mode, and display clearing. `enable-meta-key` defaults on, but the built-in
backend emits no meta-mode sequences. Bash 5.3 `bind -v` and PTY probes
observed the same default and no meta-mode output; these are runtime
observations, not source-level proof of GNU's implementation. The editor still
notifies
`TerminalIo::set_meta_key_enabled` during setup, cleanup, and signal recovery;
custom `TerminalIo` implementations own whether and how to act on that
notification. On non-Unix targets the built-in backend remains available for
compilation but returns `Unsupported` for live terminal operations until a
backend is provided. Tests and embedders can provide their own `TerminalIo`
implementation, including an in-memory terminal for editor behavior tests
without a real TTY.

The built-in terminal reads the input the terminal holds as one batch, like
GNU Readline, which processes the input that is available character by
character and redisplays once. The editor dispatches a batch character by
character, draws the pending text before a key that interrupts it, and keeps
the bytes that follow an accepted line in the input stream, so the next read
continues with them. A key that changes the display without running a command,
like the query of an incremental search, redraws at once, like GNU Readline's
search display, which is redrawn for every query key. A custom `TerminalIo` implementation owns its own read
granularity; the editor draws the text of an event once, when the event ends.

Every error text the editor writes goes through `TerminalIo::write_diagnostic`:
the inputrc diagnostics, the readline error messages, and the history
expansion failure message. The print-only history expansion result joins them
on that sink, because a GNU Bash 5.3 probe observed that result on standard
error. The built-in terminal writes them to standard
error, so an embedder's standard output stays free of
them. The inputrc diagnostics and the readline error messages are written
there like GNU Readline writes its own error text; the history expansion
failure message is the built-in terminal's own choice, because that
destination has not been observed on the GNU side with separated streams. A
custom backend keeps the default of writing the diagnostics to its own
output stream.

A diagnostic that reaches the terminal's own screen moves the terminal
cursor, so the display model follows it. `TerminalIo::diagnostics_use_terminal`
reports whether the diagnostic sink is that screen; the built-in terminal
compares standard error with standard output. A diagnostic written to another
stream leaves the display untouched, so the next redisplay starts at the
rendered line again instead of erasing what the terminal already showed.

The built-in `Terminal` has independent pre-read signal policies:
`set_catch_signals(bool)` selects the eight non-`SIGWINCH` signals and
`set_catch_sigwinch(bool)` selects resize catching; both default to `true`.
They apply on raw-mode entry, preserve inherited `SIG_IGN`, and restore only
changed dispositions on exit. Because termios and signal dispositions are
process-wide, the built-in backend allows only one active raw-mode session; a
second entry returns `WouldBlock`. Custom `TerminalIo` implementations own
their signal policy. An interrupted built-in poll/read without a caught event yields
`TerminalEvent::Timeout` so the editor can recheck `Hooks::check_signals`.

During an active `read_line`, Sushline may translate terminal and signal events
into editor behavior; process-wide policy and post-read control flow remain the
embedder's responsibility.

Idle and input-available policy (GNU `rl_event_hook` / `rl_input_available_hook` /
keyboard-input timeout) belongs to the embedder's `TerminalIo::read_event`
timeout/poll plus per-iteration `Hooks::check_signals`; Sushline performs no
implicit idle callback, and `keyseq-timeout` drives the `read_event` timeout.

Terminal escape bytes that are not terminfo-backed live in
`terminal::escape`. Terminfo remains responsible for existing capabilities such
as clear, flash, keypad, and active-region sequences; the built-in terminal
does not use terminfo `smm`/`rmm` or fallback sequences for meta-key mode. Fixed
ANSI helpers own cursor movement, clear-to-end strings, bracketed paste toggles,
cursor save/restore, and fallback visible bell bytes.

## Display Ownership

Redisplay orchestration lives in `display.rs`. It combines prompts, buffer
rendering, completion display state, cursor placement, and terminal writes for
the active line. It does not own cell-width rules or escape string literals.

`buffer/render.rs` is pure buffer rendering: it converts editable bytes into a
rendered string plus point widths, including control-character and meta-byte
display semantics. `prompt.rs` owns prompt marker parsing, including hidden
regions. `completion/display.rs` owns completion candidate layout and
pagination.

`width.rs` centralizes shared cell-width primitives and ANSI-aware output
measurement. It intentionally keeps prompt marker parsing, completion-visible
width, and buffer control-character rendering as distinct semantics rather
than folding them into one generic rule.

## Keymaps, Variables, and Inputrc

Key bindings and Readline variables live in editor-owned `KeyMap` and
`Variables` structures. The inputrc parser mutates those structures by applying
`set` commands, function bindings, macros, conditionals, and includes.

`Config` selects the application name, initial editing mode, inputrc discovery
policy, key-sequence timeout, and automatic history insertion policy. Runtime
inputrc reloads update the same keymap and variable state used by the read
loop.

The inputrc parser separates fatal problems from the non-fatal diagnostics
that GNU Readline prints and keeps reading past. A `set` directive that names
an unknown variable, or a value that GNU Readline rejects, is reported as an
`InputrcDiagnostic` and ignored, so the rest of the file still applies. The
editor prints those diagnostics before the next prompt, like GNU Readline
prints them while it starts up, and keeps them available through
`Editor::inputrc_diagnostics`.

## Completion Boundary

Completion uses structured request and response values. `CompletionRequest`
describes the current line, point, word range, triggering key, and completion
type. `CompletionResponse` returns byte replacements, optional display text,
and completion options such as filename quoting, suffix/prefix insertion,
sorting, and append behavior.

The editor owns the mechanics of applying completions, displaying candidates,
menu cycling, and repeated completion state. Completion sources may be built in
or supplied by hooks.

Internally, completion dispatch stays in `completion/engine.rs`; candidate
insertion and requoting live in `completion/insert.rs`; menu-completion state
cycling lives in `completion/menu.rs`; display layout, filename discovery, and
quoting helpers stay in their own sibling modules.

## History Storage and Files

The `history` crate stores entries, timestamps, undo metadata, cursor state,
stifling state, and the loaded-file boundary used by append-new writes. Search,
navigation, expansion helpers, and byte-preserving file reads operate on that
state.

History file writes use last-writer-wins with no locking. For regular
destinations, full writes and truncation write through a unique sibling
temporary file created with exclusive creation, set owner-only (`0600`) on
Unix, and renamed into place; append writes only the selected entry range.
For non-regular destinations (resolved through symlinks, for example
`history -> /dev/null`), full writes truncate and write in place with the link
preserved, and `truncate_file` rejects non-regular destinations with an error
like GNU. Reads (`read_file`/`load_file` and range variants) check the opened
descriptor and reject non-regular inputs with an error like GNU, while symlinks
resolve to their target descriptor. Full writes to regular destinations reset the mode to `0600` on Unix
even when the destination pre-existed with a wider mode, and all writes are
close-only with no fsync, matching GNU. Concurrent-writer merging is embedder policy.
The editor can add accepted lines automatically when configured, but long-term
save timing remains controlled by the embedder.

## Embedding Interface

The embedder supplies:

- Expanding prompts before passing them to `Editor::read_line`.
- Initializing process locale state when locale-sensitive completion behavior
  is desired. Completion sorting delegates ordering to libc `strcoll`; on Unix,
  `completion-ignore-case` maps each byte with locale-aware `tolower` on Unix
  and uses ASCII folding elsewhere; this is byte-wise, not multibyte case folding.
  The editor's history search honours `search-ignore-case` with the locale it
  owns from the environment, not the process locale: it takes the first non-empty of
  `LC_ALL`, `LC_CTYPE` and `LANG`, and
  falls back to ASCII byte folding when that locale is unavailable or the
  bytes are not valid UTF-8. The public History byte-search API stays
  ASCII-only and consults no locale.
- Supplying and persisting history according to the embedding program's policy.
- Programmable completion state and candidate generation.
- Shell-specific completion quoting, glob expansion, history expansion status,
  spelling correction, shell word ranges, and shell expansion through `Hooks`.
- Executing application commands for `bind -x`.
- `edit-and-execute-command` policy.
- Translating `ReadlineResult` into the embedding program's input and control
  flow model.
- Handling process, job-control, and signal policy outside an active
  line-editing session.

## Test Structure

Unit tests cover internal behavior. PTY oracle tests compare observable
Readline and History behavior against a GNU Bash 5.3 host; the Readline the
host carries is read from the release number it exports, so the oracle
evidence names the Readline release it was measured on and stops on a release
the compatibility document does not cover. The oracle suite requires that host
on `PATH` and stops without evidence on any other host. Its test
targets are behind the `oracle` feature of the `readline` crate, so a host
without that Bash can run the rest of the suite with
`cargo test --workspace --no-default-features`. Embedding-program tests should
focus on application state, completion builtins, prompt expansion, and history
policy.
