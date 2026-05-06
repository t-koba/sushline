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
  file persistence, timestamps, and history expansion.

Embedders should depend on the root crate.

## Internal Crates

The `readline` crate owns interactive line editing.

Important source areas: `editor.rs`, `input.rs`, `keymap/`, `bind.rs`,
`inputrc.rs`, `buffer/`, `command/`, `completion/`, `display.rs`, `prompt.rs`,
`terminal.rs`, and `hooks.rs`.

The `history` crate owns history data structures and algorithms.
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
`CompletionRequest`, `CompletionResponse`, and `Edit`. Hook methods cover
variables, expansion, completion sources, quoting, word-break policy,
application commands, status text, spelling correction, signals, and version
text.

`bind` and inputrc accept function names from multiple functional areas.
Editor-owned functions are handled internally; application-owned functions are
dispatched through hooks.

## Editor Runtime State

`Editor` owns configuration, terminal I/O, keymaps, variables, and the shared
history object. A single `read_line` call creates an `EditorState` for the
active line.

`EditorState` holds per-line runtime state: the line buffer, pending key bytes,
numeric argument, mark, undo stack, kill ring, search state, completion state,
vi state, keyboard macro state, bracketed paste state, display state, and the
original line. This keeps mutable editing state scoped to the active read while
allowing keymaps, variables, terminal setup, and history to persist across
reads.

## Terminal Boundary

Terminal access is isolated behind `TerminalIo`. The editor consumes
`TerminalEvent` values and writes through terminal methods instead of reading
from stdin or writing escape sequences directly throughout the command code.

The concrete `Terminal` handles raw-mode setup and restoration, terminal size,
resize events, byte input, signal events, visible bell, keypad/meta-key mode,
and display clearing. Tests can provide an in-memory `TerminalIo`
implementation, which keeps editor behavior testable without a real TTY.

During an active `read_line`, Sushline may translate terminal and signal events
into editor behavior; process-wide policy and post-read control flow remain the
embedder's responsibility.

## Keymaps, Variables, and Inputrc

Key bindings and Readline variables live in editor-owned `KeyMap` and
`Variables` structures. The inputrc parser mutates those structures by applying
`set` commands, function bindings, macros, conditionals, and includes.

`Config` selects the application name, initial editing mode, inputrc discovery
policy, key-sequence timeout, and automatic history insertion policy. Runtime
inputrc reloads update the same keymap and variable state used by the read
loop.

## Completion Boundary

Completion uses structured request and response values. `CompletionRequest`
describes the current line, point, word range, triggering key, and completion
type. `CompletionResponse` returns byte replacements, optional display text,
and completion options such as filename quoting, suffix/prefix insertion,
sorting, and append behavior.

The editor owns the mechanics of applying completions, displaying candidates,
menu cycling, and repeated completion state. Completion sources may be built in
or supplied by hooks.

## History Storage and Files

The `history` crate stores entries, timestamps, undo metadata, cursor state,
stifling state, and the loaded-file boundary used by append-new writes. Search,
navigation, expansion helpers, and byte-preserving file reads operate on that
state.

History file writes are serialized through a side lock file on Unix. Full writes
and truncation write to a temporary path and rename it into place; append writes
only the selected entry range. The editor can add accepted lines automatically
when configured, but long-term save timing remains controlled by the embedder.

## Embedding Interface

The embedder supplies:

- Expanding prompts before passing them to `Editor::read_line`.
- Initializing process locale state when locale-aware completion ordering is
  desired. Completion sorting delegates ordering to libc `strcoll`.
- Supplying and persisting history according to the embedding program's policy.
- Programmable completion state and candidate generation.
- Executing application commands for `bind -x`.
- `edit-and-execute-command` policy.
- Translating `ReadlineResult` into the embedding program's input and control
  flow model.
- Handling process, job-control, and signal policy outside an active
  line-editing session.

## Test Structure

Unit tests cover internal behavior. PTY oracle tests compare observable behavior
against the GNU Bash 5.3 and GNU Readline 8.3 baseline. Embedding-program tests
should focus on application state, completion builtins, prompt expansion, and
history policy.
