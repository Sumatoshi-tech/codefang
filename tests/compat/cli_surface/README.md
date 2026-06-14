# CLI Surface Conformance (Go oracle vs Rust)

Part of the Go-Rust compatibility test system
(`specs/go-compat-testing/SPEC.md`, Scope #1 / Roadmap #2).

## What it does

Recursively invokes `--help` on the **live Go binary** (root + every subcommand)
for both `codefang` and `uast`, parses the help into a normalized, format-agnostic
**surface model**, does the same for the **Rust binary**, and asserts the two are
identical. It also checks **error-path parity** (bad flag, unknown command, missing
required arg, unknown analyzer) -> exit code + stderr category.

- The **oracle is the live Go binary** (`build/bin/{codefang,uast}`). The expected
  surface is never re-derived in code; it is read out of Go by running `--help`.
- Comparison is against the Rust binary at `target/release/{codefang,uast}`.
- Run env is pinned: `set -f; TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800`.

## Why a structured surface model, not a help-text byte diff

Go uses **cobra** and Rust uses **clap**. The two render help prose differently by
construction (section headers, wrapping, the literal `Print help` line, `<value>`
vs `string` type markers). A byte diff of help text would fail on cosmetic
rendering and prove nothing about the actual surface. So the contract is the
**structured surface** -- but it is *not* weakened: every flag long-name,
short-letter, value-arity, and default; every subcommand; every positional
shape (count + variadic); and every `--help` exit code / output stream are
compared. A missing/extra/changed surface element is a hard FAIL.

The one place a byte diff is genuinely impossible across frameworks -- error-path
stderr wording -- is compared by **exit code** (a real, enforceable contract) plus
a tolerant **error category** (bad-flag / unknown-command / missing-arg / runtime),
and for the runtime probe Rust must also surface the specific diagnostic concept
(it cannot pass with a generic stub message).

## Files

- `extract_surface.py` -- runs `--help` recursively on a binary, parses **both**
  cobra and clap help into the surface model. `extract_surface.py {go|rust} {codefang|uast}`.
- `cli_surface.py` -- extracts both sides, diffs them, runs error-path probes,
  prints per-row PASS/FAIL + tally. `--json` for machine output; `--only {surface,errorpath}`.
- `selftest/self_test.py` -- the **self-proof** (SPEC rule #6): plants 8 known
  surface mutations + 1 anti-noop/tamper case and asserts each is detected, asserts
  the identical-surface baseline yields zero divergences, and asserts the live
  comparator reports the real divergences with nonzero exit.
- `run.sh` -- entry point: self-proof first, then live conformance.

## Running

```sh
tests/compat/cli_surface/run.sh
```

## Current measured divergences (Rust != Go)

These are REAL, not parser artifacts (each verified by hand against both binaries):

**Surface (codefang):**
- `completion` subcommand (+ `bash/zsh/fish/powershell`) is **missing in Rust**.
- ~30 `run` flags present in Go are **missing in Rust**: `--anomaly-window`,
  `--anonymize`, `--burndown-*` (debug/files/goroutines/hibernation-*/people),
  `--blob-cache-goroutines`, `--diff-goroutines`, `--diff-timeout`,
  `--empty-commits`, `--exact-signatures`, `--fail-on-missing-submodules`,
  `--granularity`, `--min-comment-len`, `--no-diff-cleanup`, `--no-diff-whitespace`,
  `--people-dict`, `--sampling`, `--shotness-dsl-name`, `--shotness-dsl-struct`,
  `--tick-size`, `--typos-max-distance`, `--uast-changes-goroutines`, `--whitelist`.
- `--checkpoint` / `--resume` differ in value-arity: Go bool flag vs Rust
  optional-value (`[<checkpoint>]`).

**Surface (uast):**
- `uast mapping` has a variadic positional in Rust that Go does not declare.

**Error paths (both):**
- Exit code on argv errors: Go (cobra) = **1**, Rust (clap) = **2**
  (bad-flag, unknown-command, missing-arg).
- Unknown-analyzer: Go emits `unknown analyzer id: ...`; Rust emits a stub
  (`command dispatch is blocked on cf-commands (tier 8)`).

The gate is therefore **RED** as it stands -- correctly, because the Rust surface
is not yet equivalent to Go. The self-proof is **GREEN**, proving the comparator
catches defects and does not produce false divergences on identical input.
