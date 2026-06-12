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

The `readline` crate owns interactive line editing.

Important source areas: `editor.rs`, `state/`, `input.rs`, `keymap/`,
`bind.rs`, `inputrc.rs`, `buffer/`, `command/`, `completion/`, `display.rs`,
`prompt.rs`, `width.rs`, `terminal.rs`, `terminal/`, and `hooks.rs`.

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
expander and returns `HistoryExpansion`, including print-only status. Embedders
that want to suppress expansion return `HistoryExpansion::unchanged(line)`.
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
numeric argument, mark, undo stack, kill ring, search state, completion state,
vi state, keyboard macro state, bracketed paste state, display state, and the
original line. This keeps mutable editing state scoped to the active read while
allowing keymaps, variables, terminal setup, and history to persist across
reads.

## Terminal Boundary

Terminal access is isolated behind `TerminalIo`. The editor consumes
`TerminalEvent` values and writes through terminal methods instead of reading
from stdin or writing escape sequences directly throughout the command code.

On Unix, the concrete `Terminal` handles raw-mode setup and restoration,
terminal size, resize events, byte input, signal events, visible bell,
keypad/meta-key mode, and display clearing. On non-Unix targets it remains
available for compilation but returns `Unsupported` for live terminal
operations until a backend is provided. Tests and embedders can provide their
own `TerminalIo` implementation, including an in-memory terminal for editor
behavior tests without a real TTY.

During an active `read_line`, Sushline may translate terminal and signal events
into editor behavior; process-wide policy and post-read control flow remain the
embedder's responsibility.

Terminal escape bytes that are not terminfo-backed live in
`terminal::escape`. Terminfo remains responsible for existing capabilities such
as clear, flash, meta-key, keypad, and active-region sequences. Fixed ANSI
helpers own cursor movement, clear-to-end strings, bracketed paste toggles,
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
Readline and History behavior against GNU Readline 8.3 / GNU History Library
through a GNU Bash 5.3 host. Embedding-program tests should focus on
application state, completion builtins, prompt expansion, and history policy.
