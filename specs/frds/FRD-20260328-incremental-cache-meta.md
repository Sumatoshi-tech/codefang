# FRD-20260328: Incremental Cache Metadata

**Date:** 2026-03-28
**Author:** Agent
**Status:** Complete
**Roadmap:** specs/filestats/ROADMAP.md — Step 2.1
**Spec:** specs/filestats/SPEC.md — Feature 2

## Problem

The incremental history cache (Feature 2) needs a persistence layer for cache metadata — tracking which HEAD SHA was last cached, which branch, how many commits were processed, and which analyzers were included. This metadata enables the runner to detect valid/stale caches and decide whether to replay all commits or only new ones.

## Solution

Add `incremental.go` to the existing `internal/cache/` package with:

1. **`IncrementalMeta`** struct — JSON-serializable cache metadata.
2. **`CacheKey(rootSHA, branch)`** — deterministic directory name from root SHA + branch.
3. **`WriteMeta(dir, meta)`** — atomic JSON write using `storage.WriteAtomic`.
4. **`ReadMeta(dir)`** — read and unmarshal, returning structured errors for missing/corrupt files.

### Type Definition

```go
type IncrementalMeta struct {
    Version     int       `json:"version"`
    HeadSHA     string    `json:"head_sha"`
    Branch      string    `json:"branch"`
    RootSHA     string    `json:"root_sha"`
    CommitCount int       `json:"commit_count"`
    AnalyzerIDs []string  `json:"analyzer_ids"`
    Timestamp   time.Time `json:"timestamp"`
}
```

### Cache Key

`CacheKey(rootSHA, branch)` produces a SHA-256 hash of `rootSHA + ":" + branch`, hex-encoded. This is the subdirectory name under `--cache-dir`.

### Staleness Detection

`IsStale(meta, currentRootSHA)` returns true when `meta.RootSHA != currentRootSHA` — indicating a force-push or history rewrite.

### Error Handling

- Missing file → `ErrCacheNotFound`
- Corrupt/unparseable JSON → `ErrCacheCorrupt`

## Test Plan

- Write/read round-trip: write meta, read back, verify fields match.
- Missing file: ReadMeta on empty dir returns ErrCacheNotFound.
- Corrupt file: ReadMeta on garbage data returns ErrCacheCorrupt.
- CacheKey: same inputs produce same output; different inputs produce different output.
- IsStale: matching root SHA → false; mismatching → true.
- `go test -race` clean.

## Implementation

**Status:** Complete

**Files created:**
- `internal/cache/incremental.go` — `IncrementalMeta`, `Key()`, `IsStale()`, `WriteMeta()`, `ReadMeta()`, sentinel errors
- `internal/cache/incremental_test.go` — 8 test cases, 90-100% coverage

**Design notes:**
- `Key()` uses SHA-256 of `rootSHA:branch` — deterministic, filesystem-safe.
- `WriteMeta()` uses `storage.WriteAtomic` for crash safety.
- `ReadMeta()` uses sentinel errors (`ErrCacheNotFound`, `ErrCacheCorrupt`) for clean error handling.
- Named `Key` not `CacheKey` to avoid stutter (`cache.Key` vs `cache.CacheKey`).

**Lint:** Clean. **Race:** Clean.
