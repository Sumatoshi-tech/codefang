# FRD-20260328: CLI Flags --cache-dir and --no-cache

**Date:** 2026-03-28
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 2.3

## Problem

The runner cache integration (step 2.2) added `Runner.CacheDir` but there is no CLI entry point. Users need `--cache-dir` and `--no-cache` flags.

## Solution

Add flags to `codefang run`, wire through `HistoryRunOptions` to `Runner.CacheDir`.

## Implementation

**Status:** Complete

**Files modified:**
- `cmd/codefang/commands/run.go` — `CacheDir`/`NoCache` fields, `registerPersistenceFlags()`, `resolveCacheDir()`, `runner.CacheDir` wiring
- `cmd/codefang/commands/run_test.go` — 2 new tests (CacheDir, NoCache propagation)

**Lint:** Clean. **Tests:** All 40+ CLI tests pass.
