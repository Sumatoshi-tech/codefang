<!-- Template: MADR 4.0 (https://adr.github.io/madr/). -->

# 0002 — Two-binary split: UAST and Codefang

- Status: accepted
- Date: two-binary-split-uast-codefang
- Deciders: @dmytrogajewski

## Context and problem statement

The original `hercules` was a monolith. Codefang has two distinct jobs: turning source code into a standardized tree structure, and running static and history analyzers over code and Git repositories. Parsing is a self-contained transformation from source text to a Universal Abstract Syntax Tree (UAST); analysis consumes UASTs or Git repositories and produces metrics. Should both jobs ship as one binary, or be split so each does one thing?

## Decision drivers

- The project commits to the Unix philosophy: small tools joined by pipes.
- Parsing and analysis have different inputs, outputs, and likely consumers (an editor or agent may want only the parser).
- Composability: a UAST produced by one tool should pipe into the other.
- A clear seam keeps each tool's command surface small and its responsibility single.

## Considered options

- Two binaries: `uast` (parser) and `codefang` (analyzer), composed via pipes.
- A single monolithic binary with subcommands for both parsing and analysis.
- A library-only distribution with no standalone parser binary.

## Decision outcome

Chosen option: "Two binaries — `uast` and `codefang`", because parsing and analysis are separable responsibilities with different consumers, and a pipe-friendly split lets each tool stay small while still composing into a pipeline.

`uast` (`cmd/uast/`) turns source code into UAST and exposes `parse`, `query`, `diff`, `explore`, and `server`. `codefang` (`cmd/codefang/`) consumes UASTs for static analysis or Git repositories for history analysis via `run`, `mcp`, and related commands. Both CLIs are built with Cobra and share the same `pkg/` libraries, so the split is at the command surface, not a duplication of core logic. The canonical composition is `uast parse main.go | codefang run -a static/* --format json`.

### Consequences

- Good: Each binary has one responsibility and a small command surface.
- Good: `uast` is usable on its own as a universal parser, independent of any analysis.
- Good: The tools compose through stdin/stdout, fitting CI pipelines and AI-agent tool wiring.
- Neutral: Two binaries must be installed (`go install .../cmd/codefang` and `.../cmd/uast`), and both are versioned together from one module.
- Bad: A workflow that always wants both pays an extra process boundary and a serialization step between them.

## Pros and cons of the options

### Two binaries (UAST + Codefang)

- Good: Single-responsibility tools; either is useful alone.
- Good: Pipe composition matches the Unix philosophy the project commits to.
- Neutral: Shared `pkg/` code means the split costs no logic duplication.

### Single monolithic binary

- Good: One install, no inter-process serialization.
- Bad: One large command surface mixing parsing and analysis concerns.
- Bad: Cannot offer the parser alone to consumers that do not want the analyzer.

### Library-only distribution

- Good: Maximum flexibility for embedders.
- Bad: No paste-ready CLI for the common case; raises the barrier for first use and for pipe-based and agent workflows.

## Links

- Supersedes: none
- Superseded by: none
- Related: [0001 — Record architecture decisions](0001-record-architecture-decisions.md)
