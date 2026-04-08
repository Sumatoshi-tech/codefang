# FRD: Halstead — Deduplicate Function Name Keys (Roadmap perf30/4.2)

**ID**: FRD-20260311-halstead-dedup
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 4.2
**Date**: 2026-03-11

## Problem

The `SpillableDataCollector` in `common/aggregator.go` uses a single `identifierKey` (typically
`"name"`) for last-write-wins deduplication. When multiple files contain functions with the same
name (`init`, `main`, `New`, `Close`, `anonymous`), only the last occurrence survives.

This affects all analyzers using `common.NewAggregator` with `identifierKey="name"`:
- `halstead` — function metrics lost
- `complexity` — function metrics lost
- `cohesion` — function metrics lost

The `_source_file` field is already stamped on every item by `StampSourceFile` in `static.go`.
Using it as part of the dedup key solves the collision.

## Decision

Add composite identifier key support to `SpillableDataCollector`:

1. Add `identifierKeys []string` field alongside existing `identifierKey string`.
2. When `identifierKeys` is non-empty, build the dedup key by joining values of all
   keys with `:` separator.
3. Add `NewSpillableDataCollectorComposite(collectionKey string, identifierKeys []string, threshold int)`
   constructor.
4. Existing `NewSpillableDataCollector` continues to work (single key, backward-compatible).
5. Change `common.NewAggregator` to accept `identifierKeys ...string` (variadic).
   Single key behaves as before; multiple keys enable composite dedup.
6. Halstead, complexity, and cohesion aggregators pass `["_source_file", "name"]` as identifier keys.

The composite key `"/repo/pkg/foo.go:init"` is unique across files while preserving
sort-by-name behavior in the output.

## Contract

- `SpillableDataCollector` composite keys join with `:` separator.
- The last key in `identifierKeys` is required; earlier keys are optional (graceful fallback).
- When an optional key (e.g., `_source_file`) is missing, it is omitted from the composite.
- The `identifierKey` field (single) is used when `identifierKeys` is empty (backward-compatible).
- Output items are unchanged — no new fields added, no existing fields modified.
- Sort order uses the last key (primary identifier) for deterministic output.

## Acceptance Criteria

- [x] `SpillableDataCollector` supports composite identifier keys
- [x] `common.NewAggregator` accepts `[]string` identifier keys
- [x] Halstead aggregator uses `["_source_file", "name"]` composite keys
- [x] Complexity and cohesion aggregators use `["_source_file", "name"]` composite keys
- [x] `BenchmarkHalsteadDedup` shows correct item preservation (4000 items)
- [x] `go test ./internal/analyzers/halstead/...` passes
- [x] `go test ./internal/analyzers/common/...` passes
- [x] `make lint` passes

## Benchmark Results

```
BenchmarkHalsteadDedup-24    3    2.97ms/op    4000 items-collected    1.15M B/op    5040 allocs/op
```

1000 files × 4 functions each → all 4000 items preserved (zero data loss).
