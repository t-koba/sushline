# sushline

`sushline` is a pure Rust line-editing and history foundation for interactive
command interpreters. It started as the line editor for
[Sushi shell, a.k.a. Sush](https://github.com/shellgei/rusty_bash), and its
public API is designed for general embedders.

The initial compatibility baseline is Bash 5.3 with Readline 8.3. `sushline`
is an independent Rust implementation and does not include GNU Readline source
code.

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
