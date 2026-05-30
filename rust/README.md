# codefang — Rust rewrite (scaffold)

Byte-identity-first port of codefang from Go to Rust, keeping libgit2. This is
the initial workspace scaffold per `specs/rust-rewrite/DESIGN.md`.

## Status

- Cargo virtual workspace with crate-per-module layout (DESIGN §1). Edition
  2021; toolchain pinned in `rust-toolchain.toml`.
- `codefang` and `uast` CLI surfaces reproduced with clap's builder API,
  mirroring cobra command/flag/short/default/help and the SilenceErrors
  asymmetry (DESIGN §4). `--help` / `--version` work end to end.
- Subcommand bodies that are not yet ported print an explicit "not yet
  implemented" marker and exit non-zero so the golden harness can SKIP them.
- Golden-diff harness under `tests/golden-harness` driven by
  `tests/golden/MANIFEST.json` (DESIGN §6). It byte-compares stdout against
  goldens, reports the first differing byte offset, and SKIPs records whose
  body is still stubbed.

## Layout

- `crates/cf-*` — one library crate per Go package. Tier-0 keystones
  `cf-gojson` / `cf-goyaml` are the only Go-byte-compatible encoders.
- `bins/codefang`, `bins/uast` — the two binaries.
- `tests/golden-harness` — the byte-diff integration test;
  `tests/golden` — captured goldens + MANIFEST.json.

## Build & test

```sh
cd rust
cargo build
cargo test --no-run
cargo test -p golden-harness --test golden_diff   # byte-diff harness
```

The first `cargo build` compiles a vendored libgit2 (git2 `vendored-libgit2`),
which requires `cmake` and a C/C++ toolchain.

## Known follow-ups (see DESIGN)

- Pin vendored libgit2 to the `third_party/libgit2` submodule revision matching
  git2go v34 for bit-exact diff/blob/hash.
- Override clap's help template/headings per binary to byte-match cobra (Layer-D
  CLI golden).
- Implement `cf-gojson::go_float` (Grisu/Ryū shortest digits + Go rendering)
  under the Layer-A differential fuzz.
