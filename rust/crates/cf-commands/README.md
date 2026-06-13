# cf-commands

Command wiring and analyzer registration for the `codefang` binary.

## What and why

This crate is the aggregation point for the CLI: it registers every analyzer,
builds the `run` / `render` / `version` command tree (clap builder API, matched
to the reference cobra surface), drives the analysis pipeline, and routes report
serialization through the shared report-format crates (`cf-gojson` /
`cf-goyaml`) — never raw `serde`.

Every machine-report surface this crate emits is pinned byte-for-byte against
the reference binary by the differential gate in `rust/tests/compat`.

The major pieces:

- `formats` — format constants, normalization, validation, and the
  per-phase/`--ndjson` resolution logic. The error string is the exact
  CLI-contract `unsupported format: <fmt>`.
- `version` — the `version` subcommand output.
- `flags` — the full `run`/`render` clap command tree, including the deprecated
  exclusion flags and dynamic per-analyzer flag registration.
- `handlers` / `pipeline` — the run/render execution path.

## Usage

Resolve a user-supplied output format into per-phase formats:

```rust
use cf_commands::{normalize_format, resolve_formats};

assert_eq!(normalize_format(" BIN "), "binary");

// Static-only run: history format resolves empty.
let (static_fmt, history_fmt) = resolve_formats("bin", true, false).unwrap();
assert_eq!(static_fmt, "binary");
assert_eq!(history_fmt, "");
```

This is the crate-level doctest in `src/lib.rs`, run by
`cargo test --doc -p cf-commands`.

## Build

```sh
cargo build -p cf-commands
cargo test -p cf-commands
```

The `runtime` feature pulls in the streaming framework runner, persistence, and
plot crates needed to actually execute analyses; the optional `mcp` feature
mirrors the reference `mcp` subcommand.

## Deeper docs

See the crate rustdoc (`cargo doc -p cf-commands --open`).
