# Readline Compatibility

`sushline` aims to provide familiar Bash/Readline-style interactive behavior
through an independent Rust API. GNU Bash and GNU Readline are the compatibility
baseline for observable editor, completion, and history behavior.

Current baseline: GNU Bash 5.3 with GNU Readline 8.3.

C ABI compatibility with GNU Readline or GNU History is intentionally out of
scope. `sushline` does not include GNU Readline source code.

## Current Status

There are no currently tracked user-visible compatibility gaps in the Rust crate
API scope.

Covered surfaces:

- Emacs and vi editing commands, keymaps, macros, numeric arguments, undo,
  kill/yank, search, and command-word motion.
- History navigation, search, expansion, timestamps, persistence, and state APIs.
- Inputrc, Readline variables, bind query/apply/print behavior, and terminal
  redisplay.
- Filename, command, variable, user, host, glob, menu, and programmable
  completion integration.
- Application-owned behavior through explicit hooks.

## Verification

Compatibility is verified by unit tests and PTY oracle tests against the local
Bash/Readline baseline:

```sh
cd sushline
cargo test
cargo clippy --all-targets -- -D warnings
```

## Compatibility Boundaries

`sushline` owns editing, completion mechanics, terminal redisplay, inputrc,
bind state, and history primitives.

The embedding program owns prompt expansion, command grammar, aliases or other
application expansions, variables, programmable completion state, builtins,
`bind -x`, `edit-and-execute-command`, history save policy, process policy, job
control, and signal policy.

The boundary is the `sushline::readline::Hooks` trait. Application-owned behavior is
provided through typed hook contexts such as `CommandContext`,
`HistoryExpansionContext`, `CompletionRequest`, and `CompletionResponse`, plus
hook methods for variables, expansion, completion, word breaks, signals, and
application commands.
