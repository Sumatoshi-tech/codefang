# cf-gitlib

Git repository/commit/diff/blob access layer over libgit2.

Provides the workspace's view of a git repository: opening a repo, walking
history, reading commits and trees, computing tree-level changes, and reading
blob content, plus a batch blob/diff worker. It is built on the `git2` crate
with **vendored libgit2** so object, diff, and hash semantics match the
reference binary byte-for-byte.

## Why

The history analyzers (burndown, couples, devs, file_history, ...) need a single
git access layer whose output is reproducible and parity-stable. Pinning libgit2
keeps diff line counts, tree-change ordering, and hash rendering identical to the
reference implementation; those surface into machine reports whose bytes are
pinned by `tests/compat`.

## Model

- `Repository` handles are **per-thread** (`!Send` / `!Sync`) and free libgit2
  objects via RAII `Drop`.
- `Hash` is a 20-byte SHA-1 with lossless `git2::Oid` conversion and a frozen,
  lowercase 40-char hex rendering.
- `GitError` Display strings are a frozen CLI error contract.

## Usage

`Hash` is the pure, dependency-free entry point (parsing, rendering, zero check):

```rust
use cf_gitlib::Hash;

let h = Hash::new("0123456789ABCDEF0123456789abcdef01234567");
// Rendering is always 40 lowercase hex chars, regardless of input case.
assert_eq!(h.to_string(), "0123456789abcdef0123456789abcdef01234567");
assert!(!h.is_zero());

// Lossless round-trip through a libgit2 OID.
assert_eq!(Hash::from_oid(&h.to_oid()), h);
```

Opening a real repository and walking history goes through `Repository`; see the
module docs (`repository`, `revwalk`, `commit`, `changes`) and the crate's
integration tests for end-to-end examples.

## Build

```sh
cargo build -p cf-gitlib
cargo test -p cf-gitlib
```

The vendored libgit2 is pinned via the `third_party/libgit2` submodule; the
`git2` dependency uses the `vendored-libgit2` feature so builds do not depend on
a system libgit2.

## Deeper docs

- Rustdoc: `cargo doc -p cf-gitlib --open`
- Rewrite design rationale: `specs/rust-rewrite/DESIGN.md` §3.
