# Sushline

Sushline is a pure Rust line-editing and history foundation aiming for
Readline-style behavioral compatibility for interactive command interpreters.
It started as the line editor for
[Sushi shell, a.k.a. Sush](https://github.com/shellgei/rusty_bash), and its
public API is designed for Rust embedders.

The compatibility target is GNU Readline 8.3 patch level 0 and the GNU
History Library behavior in that baseline. PTY oracle probes use GNU Bash 5.3
as a runtime host for Readline/History behavior, and the oracle harness checks
that host version and the Readline release the host carries; these are Bash
runtime observations, not proof of the Readline version reported by other host
tooling.
Bash language, shell runtime, and builtin compatibility are not Sushline goals.
Sushline is an independent Rust implementation and does not include Readline
source code.
It is not a C ABI-compatible replacement for Readline; compatibility is
documented at the Rust API and observable editor/history behavior level.

## Crate Layout

The root crate exposes two public modules:

- `sushline::readline`: line editing, keymaps, inputrc, `bind`, completion,
  rendering, terminal I/O, and application hook interfaces.
- `sushline::history`: history storage, search, stifle/state APIs, file
  persistence, timestamped history records, and history expansion.

```rust
use sushline::readline::{Config, Editor, History, Prompt, Terminal};

let mut editor = Editor::new(
    Config::default(),
    Terminal::new(),
    History::new(),
);

let _ = editor.read_line(Prompt::new("> "), &mut ());
```

On Unix, the built-in terminal restores terminal/signal state and, by default,
re-delivers an actually caught SIGINT to the saved disposition, including fatal
`SIG_DFL`.
Fatal redelivery prevents either `SigintPolicy` read result. For nonfatal handling
of saved `SIG_DFL`, configure the terminal before constructing the editor:

```rust
let mut terminal = sushline::readline::Terminal::new();
terminal.set_reraise_sigint_default(false);
```

This opt-out leaves the read to `SigintPolicy` (`Continue` by default); it does
not change caller handlers, `SIG_IGN`, hook-only reports, or literal Ctrl-C bytes.

## Documentation

- [`COMPATIBILITY.md`](COMPATIBILITY.md): baseline and scope.
- [`DESIGN.md`](DESIGN.md): structure and embedding API.

## Testing

`cargo test --workspace` runs the unit tests and the PTY oracle suite. The
oracle suite compares Sushline against GNU Bash 5.3 observed at runtime, so it
requires GNU Bash 5.3 on `PATH`; on any other host, and on a host that carries
a Readline release the compatibility document does not cover, the suite
reports the mismatch once and stops without producing compatibility
evidence. A host whose Readline release the suite cannot read, a host whose
programs are not ELF images, is named by the suite, which continues there
without a release scope.

The Bash-hosted test targets are behind the `oracle` feature of the `readline`
crate, which is on by default. `cargo test --workspace --no-default-features`
skips them and runs the remaining tests on any host.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option.
