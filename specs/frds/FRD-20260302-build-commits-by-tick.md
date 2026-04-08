# FRD: Shared buildCommitsByTickFromTicks (Roadmap 3.2)

**ID**: FRD-20260302-build-commits-by-tick
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 3.2
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 3d

## Problem

Three history analyzers (`anomaly`, `quality`, `sentiment`) each define an identical local `buildCommitsByTickFromTicks` function that converts `[]analyze.TICK` into `map[int][]gitlib.Hash`. The structure is always the same:

1. Type-assert `tick.Data` to the analyzer's local `TickData` type.
2. Check the commit-keyed map field is non-nil.
3. Iterate the map keys, converting each string to `gitlib.Hash` via `gitlib.NewHash`.
4. Append hashes to `ct[tick.Tick]`.

The only difference is which `TickData` type and which map field is used:

| Analyzer | Map field | Map value type |
|----------|-----------|----------------|
| anomaly | `CommitMetrics` | `*CommitAnomalyData` |
| quality | `CommitQuality` | `*TickQuality` |
| sentiment | `CommentsByCommit` | `[]string` |

Three sources of truth for the same algorithm.

## Solution

Create a generic function `BuildCommitsByTick[V any]` in the `analyze` package. The caller provides a callback that type-asserts `tick.Data` and returns the commit-keyed map (or `nil, false` if invalid). The shared function handles hash building and tick aggregation.

### Placement

`internal/analyzers/analyze/commits_by_tick.go` — alongside `TICK` and other tick-related types.

### API

```go
// BuildCommitsByTick converts ticks into a map from tick index to commit hashes.
// The extract callback type-asserts tick.Data and returns the commit-keyed map.
func BuildCommitsByTick[V any](ticks []TICK, extract func(any) (map[string]V, bool)) map[int][]gitlib.Hash
```

### Migration (per analyzer)

Before:
```go
func buildCommitsByTickFromTicks(ticks []analyze.TICK) map[int][]gitlib.Hash {
    ct := make(map[int][]gitlib.Hash)
    for _, tick := range ticks {
        td, ok := tick.Data.(*TickData)
        if !ok || td == nil || td.CommitMetrics == nil {
            continue
        }
        hashes := make([]gitlib.Hash, 0, len(td.CommitMetrics))
        for h := range td.CommitMetrics {
            hashes = append(hashes, gitlib.NewHash(h))
        }
        ct[tick.Tick] = append(ct[tick.Tick], hashes...)
    }
    return ct
}
```

After:
```go
ct = analyze.BuildCommitsByTick(ticks, func(data any) (map[string]*CommitAnomalyData, bool) {
    td, ok := data.(*TickData)
    if !ok || td == nil {
        return nil, false
    }
    return td.CommitMetrics, td.CommitMetrics != nil
})
```

Then delete the local `buildCommitsByTickFromTicks` function.

## Acceptance Criteria

- [x] `BuildCommitsByTick[V any]` defined in `internal/analyzers/analyze/commits_by_tick.go`
- [x] Unit test in `internal/analyzers/analyze/commits_by_tick_test.go` covering:
  - Empty ticks slice returns empty map
  - Ticks with nil data are skipped
  - Ticks with valid data produce correct hash mapping
  - Multiple ticks with same tick index are merged
  - Extract returning false is skipped
- [x] All 3 local `buildCommitsByTickFromTicks` functions removed
- [x] `go vet` clean
- [x] `go test ./internal/analyzers/analyze/... ./internal/analyzers/anomaly/... ./internal/analyzers/quality/... ./internal/analyzers/sentiment/...` passes
- [x] `make lint` passes — zero issues, zero dead code

## Risk

Low. The function is a generic wrapper around a mechanical loop. Each migration is a replacement of a local function with a callback-based invocation.

## Implementation

### Files Created

- `internal/analyzers/analyze/commits_by_tick.go` — Generic `BuildCommitsByTick[V any]` function
- `internal/analyzers/analyze/commits_by_tick_test.go` — 7 table-driven tests

### Files Modified

- `internal/analyzers/anomaly/analyzer.go` — remove `buildCommitsByTickFromTicks`, use `BuildCommitsByTick`
- `internal/analyzers/quality/analyzer.go` — remove `buildCommitsByTickFromTicks`, use `BuildCommitsByTick`
- `internal/analyzers/sentiment/analyzer.go` — remove `buildCommitsByTickFromTicks`, use `BuildCommitsByTick`

### Lines Eliminated

~45 lines of duplicate `buildCommitsByTickFromTicks` functions removed across 3 packages.

### Verification

- `go vet` — clean
- `go test ./internal/analyzers/...` — all pass
- `make lint` — zero issues, zero dead code
