<!-- Template: MADR 4.0 (https://adr.github.io/madr/). -->

# 0003 — Git access through libgit2 (Rust `git2` crate, vendored)

- Status: accepted
- Date: libgit2-via-cgo
- Deciders: @dmytrogajewski

## Context and problem statement

History analysis is one of Codefang's two modes: it opens a Git repository,
walks the commit history, computes per-commit tree and file diffs, and reads
blob content for many commits across potentially planet-scale repositories. This
is the hot path for burndown, couples, devs, and every other history analyzer.
The project needs a Git implementation that is fast at bulk commit walking, tree
diffing, and blob lookup, supports both normal and bare repositories, and — now
that the codebase is a Rust workspace — produces diff, blob, and hash results
that are byte-for-byte identical to the frozen Go reference binary so the parity
gate (`rust/tests/compat/`) keeps passing. How should Codefang access Git object
and history data in the Rust port?

The original decision (Go era) chose libgit2 through the `git2go/v34` **cgo**
bindings, vendored and built statically. That binding-level choice is specific
to Go and is superseded here; the underlying engine choice — libgit2 — is
re-confirmed.

## Decision drivers

- History analysis must walk large commit histories and diff trees at high
  throughput.
- The implementation must support both normal and bare repositories.
- The project targets planet-scale repositories, so memory and CPU efficiency of
  Git operations are first-order concerns.
- Output must match the frozen Go oracle byte-for-byte: the same libgit2 diff,
  blob, and hash semantics the Go binary produced must be preserved, or the
  differential parity gate fails.
- A stable, well-maintained Git core with broad object-model coverage reduces
  the surface the project must reimplement.

## Considered options

- libgit2 through the Rust [`git2`](https://crates.io/crates/git2) crate with the
  `vendored-libgit2` feature, building the vendored libgit2 under
  `third_party/libgit2`.
- A pure-Rust Git implementation (`gix` / `gitoxide`).
- Shelling out to the `git` command-line binary.

## Decision outcome

Chosen option: "libgit2 via the Rust `git2` crate, vendored". libgit2 is the
same mature C implementation of the Git object model the Go binary used, so its
diff, blob, and hash semantics are preserved exactly — which is what the parity
gate requires. The `git2` crate gives direct, in-process access without
per-operation subprocess overhead, and its `vendored-libgit2` feature compiles
the pinned `third_party/libgit2` sources at build time, so no system libgit2 is
needed and the version is reproducible.

`cf-gitlib` wraps `git2` for repository open, commit walking, tree diff,
changes, blob reads, and its batch blob/diff worker, exposing per-thread
`!Send`/`!Sync` repository handles with RAII cleanup. The plumbing analyzers
(tree-diff, blob-cache, file-diff) sit directly on top of this layer. The
workspace pins the crate as
`git2 = { version = "0.19", default-features = false, features = ["vendored-libgit2"] }`.

### Consequences

- Good: High-throughput tree diffing and blob lookup in-process, matching the
  planet-scale target.
- Good: Identical libgit2 semantics to the frozen Go oracle, so the differential
  parity gate stays green without reimplementing the diff/hash logic.
- Good: First-class support for both normal and bare repositories through
  libgit2's object model.
- Good: `vendored-libgit2` pins a reproducible libgit2 version and removes the
  manual cgo build flags (`CGO_CFLAGS`, `CGO_LDFLAGS`, `CGO_ENABLED=1`) the Go
  build needed — `cargo build` drives the C build through the crate's build
  script.
- Neutral: Git access is isolated behind `cf-gitlib`, so the rest of the
  workspace does not depend on the binding directly.
- Bad: The build still requires a C toolchain and CMake to compile vendored
  libgit2, which complicates fully-static cross-compilation.
- Bad: `git2` exposes raw libgit2 objects with explicit lifetimes; `cf-gitlib`
  must manage repository and object handles carefully (RAII helps, but the
  `!Send` handles constrain the threading model).

## Pros and cons of the options

### libgit2 via the Rust `git2` crate, vendored

- Good: Mature C Git core; strong throughput on the diff and blob hot paths.
- Good: Same engine as the Go oracle, so output bytes match the parity gate.
- Good: `vendored-libgit2` pins the version and needs no system library or
  hand-set build flags.
- Bad: Still needs a C toolchain and CMake at build time.

### Pure-Rust gitoxide (`gix`)

- Good: No C toolchain; trivial cross-compilation and pure-Rust static binaries.
- Bad: Different diff/blob/hash implementation than libgit2 — output bytes would
  diverge from the frozen Go oracle, breaking the parity gate, which is the
  contract the rewrite must hold.
- Bad: Some bulk-history operations are less battle-tested than libgit2 on
  very large repositories.

### Shell out to the git CLI

- Good: No build-time dependency beyond a `git` on PATH.
- Bad: Per-operation subprocess overhead is prohibitive when diffing and reading
  blobs across an entire history.
- Bad: Parsing CLI output is brittle compared to a typed object-model API, and
  would not reproduce libgit2's exact bytes.

## Links

- Supersedes: the Go-era decision to use the `git2go/v34` **cgo** bindings — the
  binding is replaced by the Rust `git2` crate; the libgit2 engine choice is
  re-confirmed.
- Superseded by: none
- Related: [0001 — Record architecture decisions](0001-record-architecture-decisions.md)
