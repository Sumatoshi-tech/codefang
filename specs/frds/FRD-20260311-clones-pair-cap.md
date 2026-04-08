# FRD: Clones Analyzer — Cap Accumulated Pair Count (Roadmap perf30/4.1)

**ID**: FRD-20260311-clones-pair-cap
**Roadmap**: [specs/perf30/ROADMAP.md](../perf30/ROADMAP.md) — Step 4.1
**Date**: 2026-03-11

## Problem

The clones aggregator (`internal/analyzers/clones/aggregator.go`) accumulates ALL clone pairs
across ALL files via `findClonePairs()` in `visitor.go`. On a large codebase like kubernetes
(~25K files, ~150K functions), the cross-file pair explosion within LSH buckets can be massive
— O(functions^2) worst case. Each `ClonePair` struct + associated `map[string]any` for the
report consumes memory that grows quadratically with similar function count.

The `total_clone_pairs` count and `clone_ratio` are the only metrics displayed in text/compact
output. The full `[]ClonePair` slice is only needed for JSON/YAML/plot export — and even there,
reporting more than 1000 pairs provides diminishing analytical value.

## Decision

Add a `MaxClonePairs` field to `Aggregator` with a default of 1000. The `findClonePairs`
function receives a `pairCap` parameter:

- The dedup `seen` map and similarity computation continue for ALL candidates (accuracy).
- A separate `totalCount` counter tracks every valid pair found.
- Pairs are only appended to the `[]ClonePair` slice while `len(pairs) < pairCap`.
- `GetResult()` uses `totalCount` for `keyTotalClonePairs` and `keyCloneRatio` (exact count).
- The capped `[]ClonePair` slice is used for `keyClonePairs` (report detail, limited).

This preserves summary metric accuracy while bounding memory from pair accumulation.

## Contract

- `Aggregator.MaxClonePairs` defaults to `DefaultMaxClonePairs` (1000) via `NewAggregator()`.
- `total_clone_pairs` in the report is the EXACT count of all detected pairs (not capped).
- `clone_ratio` uses the exact count (not capped).
- `clone_pairs` in the report contains at most `MaxClonePairs` entries (highest similarity first).
- `MaxClonePairs <= 0` means unlimited (backward-compatible, no cap).
- Per-file `Analyzer.Analyze()` also respects a cap for single-file analysis.
- All existing tests pass unchanged.

## Acceptance Criteria

- [x] `DefaultMaxClonePairs` constant added (1000)
- [x] `Aggregator.MaxClonePairs` field added, defaulting via `NewAggregator()`
- [x] `findClonePairs` accepts cap parameter, counts all pairs, limits slice
- [x] `total_clone_pairs` count remains exact (not capped)
- [x] `BenchmarkClonesPairCap` shows heap reduction with cap vs no cap
- [x] `go test ./internal/analyzers/clones/...` passes
- [x] `make lint` passes

## Benchmark Results

```
BenchmarkClonesPairCap/before-no-cap    50,952 heap-delta-KiB  124,750 stored-pairs  124,750 total-pairs  158.6M B/op  766K allocs/op
BenchmarkClonesPairCap/after-capped         24 heap-delta-KiB      100 stored-pairs  124,750 total-pairs   68.0M B/op   18K allocs/op
```

Heap delta: **99.95% reduction** (50,952 KiB → 24 KiB). Total pairs identical (124,750).
