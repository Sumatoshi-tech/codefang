<!-- Template: MADR 4.0 (https://adr.github.io/madr/). -->

# 0003 — Git access through libgit2 via cgo

- Status: accepted
- Date: libgit2-via-cgo
- Deciders: @dmytrogajewski

## Context and problem statement

History analysis is one of Codefang's two modes: it opens a Git repository, walks the commit history, computes per-commit tree and file diffs, and reads blob content for many commits across potentially planet-scale repositories. This is the hot path for burndown, couples, devs, and every other history analyzer. The project needs a Git implementation that is fast at bulk commit walking, tree diffing, and blob lookup, and that supports both normal and bare repositories. How should Codefang access Git object and history data?

## Decision drivers

- History analysis must walk large commit histories and diff trees at high throughput.
- The implementation must support both normal and bare repositories.
- The project targets planet-scale repositories, so memory and CPU efficiency of Git operations are first-order concerns.
- A stable, well-maintained Git core with broad object-model coverage reduces the surface the project must reimplement.

## Considered options

- libgit2 through the `git2go/v34` cgo bindings, with libgit2 vendored and built statically (`third_party/libgit2`).
- A pure-Go Git implementation (`go-git`).
- Shelling out to the `git` command-line binary.

## Decision outcome

Chosen option: "libgit2 via the `git2go/v34` cgo bindings", because libgit2 is a mature C implementation of the Git object model that delivers the throughput history analysis needs for tree diffing and blob lookup, and the cgo bindings give direct, in-process access without per-operation subprocess overhead.

`pkg/gitlib/` wraps `git2go` for repository open, commit walking, tree diff, changes, blob reads, and its worker pool / batch processing. libgit2 is vendored under `third_party/libgit2` and built statically; `make` drives the build and sets `CGO_CFLAGS`, `CGO_LDFLAGS`, and `CGO_ENABLED=1` so the toolchain links the vendored library. The plumbing analyzers (`TreeDiffAnalyzer`, `BlobCacheAnalyzer`, `FileDiffAnalyzer`) sit directly on top of this layer.

### Consequences

- Good: High-throughput tree diffing and blob lookup in-process, matching the planet-scale target.
- Good: First-class support for both normal and bare repositories through libgit2's object model.
- Good: Vendoring and a static build pin a reproducible libgit2 version rather than depending on whatever the host provides.
- Neutral: Git access is isolated behind `pkg/gitlib/`, so the rest of the codebase does not depend on the binding directly.
- Bad: The build requires cgo and a C toolchain; `CGO_ENABLED=1` and the libgit2 include/lib paths must be set, which complicates cross-compilation and pure-Go static builds.
- Bad: cgo introduces a Go/C boundary with manual resource lifetimes; the `pkg/gitlib/` layer must free libgit2 objects carefully to avoid leaks.

## Pros and cons of the options

### libgit2 via git2go (cgo), vendored and static

- Good: Mature C Git core; strong throughput on the diff and blob hot paths.
- Good: Vendored static build pins the version and avoids host-library drift.
- Bad: Requires cgo, a C toolchain, and explicit CGO build flags; harder to cross-compile.
- Bad: Manual memory management across the cgo boundary.

### Pure-Go go-git

- Good: No cgo; trivial cross-compilation and pure-Go static binaries.
- Bad: Lower throughput on bulk history walking and tree diffing for very large repositories, which is exactly the planet-scale hot path.

### Shell out to the git CLI

- Good: No build-time dependency beyond a `git` on PATH.
- Bad: Per-operation subprocess overhead is prohibitive when diffing and reading blobs across an entire history.
- Bad: Parsing CLI output is brittle compared to a typed object-model API.

## Links

- Supersedes: none
- Superseded by: none
- Related: [0001 — Record architecture decisions](0001-record-architecture-decisions.md)
