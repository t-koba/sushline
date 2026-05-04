# sushline Design

`sushline` is a facade crate over separate line-editing and history
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

The `history` crate owns History-compatible data structures and algorithms.
Important source areas: `lib.rs`, `file.rs`, and `expansion.rs`. History save
policy belongs to the embedder.

## Application Integration

`sushline` does not read or mutate application state directly. Embedders provide
that behavior through `sushline::readline::Hooks`.

Main hook types are `CommandContext`, `HistoryExpansionContext`,
`CompletionRequest`, `CompletionResponse`, and `Edit`. Hook methods cover
variables, expansion, completion sources, quoting, word-break policy,
application commands, status text, spelling correction, signals, and version
text.

`bind` and inputrc accept function names from more than one responsibility
area. sushline treats those names as an input contract, not as permission to
implement application policy. Editor-owned functions are handled internally;
application-owned functions are dispatched through hooks.

## Embedding Interface

The embedder supplies:

- Expanding prompts before passing them to `Editor::read_line`.
- Initializing process locale state when locale-aware completion ordering is
  desired. sushline uses the current libc `LC_COLLATE` state for completion
  sorting, but does not call `setlocale`; without embedder initialization, the
  default C locale applies.
- Supplying and persisting history according to the embedding program's policy.
- Programmable completion state and candidate generation.
- Executing application commands for `bind -x`.
- `edit-and-execute-command` policy.
- Translating `ReadlineResult` into the embedding program's input and control
  flow model.
- Handling process, job-control, and signal policy outside an active readline
  session.

## Test Structure

Unit tests cover internal behavior. PTY oracle tests compare observable behavior
against the Bash/Readline baseline. Embedding-program tests should focus on
application state, completion builtins, prompt expansion, and history policy.
