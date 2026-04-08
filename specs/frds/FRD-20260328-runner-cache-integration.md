# FRD-20260328: Runner Incremental Cache Integration

**Date:** 2026-03-28
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 2.2
**Spec:** specs/filestats/SPEC.md — Feature 2

## Problem

The runner processes all commits from scratch every invocation. For large repos (500K+ commits), this is slow. With the cache metadata layer (step 2.1), the runner can now skip already-processed commits by loading cached analyzer/aggregator state.

## Solution

Add two new phases to `Runner.Run()`:

1. **cacheProbePhase** — after init, loads cached state and trims commits.
2. **cacheWritePhase** — after finalize, saves state for future runs.

### Runner Changes

- `CacheDir string` field on `Runner`.
- Phase chain becomes: init → initAgg → **cacheProbe** → process → finalize → **cacheWrite**.

### cacheProbePhase

1. If `CacheDir` is empty, skip (no-op).
2. Read `IncrementalMeta` from `CacheDir/<cacheKey>/`.
3. If not found, proceed with full run.
4. If stale (root SHA mismatch), log warning, proceed with full run.
5. If valid: load checkpoints on analyzers that support `Checkpointable`, restore aggregator spill state, trim `s.commits` to `commits[meta.CommitCount:]`.

### cacheWritePhase

1. If `CacheDir` is empty, skip.
2. Save checkpoints on all `Checkpointable` analyzers.
3. Write `IncrementalMeta` with updated `HeadSHA`, `CommitCount`, `Timestamp`.

### Commit Trimming

The `runState.commits` slice is modified in-place (sliced) by the cache probe. `processCommitsPhase` then processes only the remaining commits. The `indexOffset` parameter in `processCommits` handles correct index numbering.

## Test Plan

This step modifies deep framework code with libgit2 dependencies. Unit tests will use a simplified approach:
- Test `cacheProbePhase` and `cacheWritePhase` as standalone functions with mock state.
- Integration testing deferred to e2e tests via CLI (step 2.3).

## Implementation

**Status:** Complete

**Files modified:**
- `internal/framework/runner.go`:
  - `CacheDir string` field on `Runner`
  - `runState` extended with `totalCommitCount`, `cacheSubDir`
  - `Run()` phase chain: init → initAgg → **cacheProbe** → process → finalize → **cacheWrite**
  - `cacheProbePhase()`: reads meta, validates, loads checkpoints, trims commits
  - `cacheWritePhase()`: saves checkpoints, spills aggregators, writes meta
  - `probeCache()`, `writeCache()`, `loadCachedCheckpoints()`, `restoreCachedAggSpills()`
  - `ErrCacheStale`, `ErrCacheInvalid` sentinel errors
  - `cacheProbeResult` type, `cacheDirPerm` constant

**Design decisions:**
- Cache probe is a non-fatal phase: failures log and proceed with full run (no data loss).
- Commit trimming via slice `commits[meta.CommitCount:]` — simpler than iterator skip.
- `indexOffset` in `processCommitsPhase` preserves correct commit numbering after trimming.
- Reuses existing `Checkpointable` and `SpillState` infrastructure — no new serialization format.

**Lint:** Clean (new code only). **Race:** Clean. All existing framework tests pass.
