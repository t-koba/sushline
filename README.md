# Sushline

Sushline is a pure Rust line-editing and history foundation aiming for
Readline-style behavioral compatibility for interactive command interpreters.
It started as the line editor for
[Sushi shell, a.k.a. Sush](https://github.com/shellgei/rusty_bash), and its
public API is designed for Rust embedders.

The compatibility target is GNU Readline 8.3 patch level 0 and the GNU
History Library behavior in that baseline. PTY oracle probes use GNU Bash 5.3
as a runtime host for Readline/History behavior; these are Bash runtime
observations, not proof of the Readline version reported by other host tooling.
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

## Documentation

- [`COMPATIBILITY.md`](COMPATIBILITY.md): baseline and scope.
- [`DESIGN.md`](DESIGN.md): structure and embedding API.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option.
