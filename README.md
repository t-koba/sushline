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
oracle suite compares Sushline against GNU Bash 5.3 observed at runtime and
requires GNU Bash 5.3 on `PATH`. If Bash is missing, is not GNU Bash, or has
another version, the suite reports the mismatch once and stops. On GNU Bash
5.3, a readable Readline release outside the scope documented in
[`COMPATIBILITY.md`](COMPATIBILITY.md) also causes the suite to stop. If the
Readline release cannot be read (for example, because the host executable is
not an ELF image), the suite reports that limitation and continues without a
Readline release scope.

The Bash-hosted test targets are behind the `oracle` feature of the `readline`
crate, which is on by default. `cargo test --workspace --no-default-features`
skips them and runs the remaining tests on any host.

CI runs the following checks by event:

| Event | Checks and evidence |
| --- | --- |
| Push to a non-`mizu` branch or tag, or a pull request | `ci / check`, `ci / oracle`, and `ci / windows-terminal`; required full-file late-timestamp comparison artifact. Upstream-only measurements and the replica job are skipped. |
| Manual dispatch with `refresh_upstream_measurements: false` (default) | The same three jobs and artifact as a push/PR. |
| Manual dispatch with `refresh_upstream_measurements: true` | `ci / check`, `ci / oracle` (including upstream primary measurements), `ci / windows-terminal`, and `ci / oracle-measurements-replica`; required comparison, primary measurement, and replica report artifacts, plus primary/replica agreement checks. |

For every workflow event, `ci / check` runs formatting, Clippy, and the
non-oracle workspace tests. `ci / windows-terminal` compiles the readline
library on Windows and runs targeted native-console, filename, quoting,
editor-completion, locale-default, and C-locale history tests. These Windows
tests cover those named fixtures only; they are not a full workspace run or a
GNU compatibility claim. `ci / oracle` builds or restores and validates the
same digest-pinned Readline 8.3 and Bash 5.3, runs `Run the oracle suite`, and
runs the mixed full-file late-timestamp Rust/GNU comparison. Its required
`pinned-readline-8.3-full-file-comparison` artifact contains
`history_full_late_record.out`, `history_rust_full_late_record.out`, and
`history_full_file_comparison.out`.

The dependency-free workflow guard in `ci / check` validates action pins and
these event routes, with positive and mutation-based negative self-tests. Run
`python3 .github/scripts/check-action-pins.py --self-test` locally; it reads the
workflow without running oracle probes. It supports the workflow's plain block
mapping layout, counts measurement steps without a parallel measurement
inventory, and checks each required replica-agreement identity and its
comparison command.

The regular oracle check does not run all upstream measurements. Request the
manual refresh to run the 55 upstream-only primary measurement/report steps
and the raw full-file GNU probes, then the 35 fresh-runner replica measurement
steps and existing agreement checks. The refresh requires every listed primary
report in `pinned-readline-8.3-measurements` (excluding the three mixed comparison
files) and the replica report artifacts. Missing required reports or failed
assertions fail the refresh. Both runners check out `github.sha`; the replica
needs the primary oracle job, restores only its exact digest/configuration-pinned
Readline cache (a miss fails), and downloads the primary artifact from the same
workflow run. A skipped replica check is not evidence of a measurement refresh.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option.
