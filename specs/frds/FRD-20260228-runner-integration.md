# FRD: Runner Integration (Phase 2)

**ID**: FRD-20260228-runner-integration
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Steps 2.1, 2.2, 2.3, 2.4
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md) — Step 2
**Depends on**: [FRD-20260228-report-store](FRD-20260228-report-store.md) (Phase 1)

## Problem

`FinalizeWithAggregators` materializes ALL analyzer reports simultaneously in memory.
On kubernetes (56K commits, 50K files), this causes OOM (10-20 GB peak).
Phase 1 introduced `ReportStore` for chunked I/O. Phase 2 wires it into the Runner
so analyzers can write one-at-a-time to the store, releasing each aggregator before
the next analyzer starts.

## Feature

1. **StoreWriter interface** — optional interface for analyzers that can stream
   chunked records directly to a `ReportWriter`, bypassing the monolithic `Report` map.
2. **FinalizeToStore** — new Runner method that processes one analyzer at a time,
   using `StoreWriter` for implementing analyzers and a legacy gob fallback for others.
3. **StreamingConfig wiring** — `ReportStore` field on `StreamingConfig` so the
   streaming pipeline branches to `FinalizeToStore` when a store is configured.

## Interfaces

### StoreWriter

```go
// StoreWriter is optionally implemented by HistoryAnalyzers that can write
// chunked records directly to a ReportWriter, bypassing monolithic Report maps.
type StoreWriter interface {
    WriteToStore(ctx context.Context, ticks []TICK, w ReportWriter) error
}
```

## Behavior

### FinalizeToStore

```
for each leaf analyzer (i >= CoreCount):
    agg := runner.aggregators[i]
    if agg == nil:
        write empty meta to store, continue

    agg.Collect()
    ticks := agg.FlushAllTicks()

    w := store.Begin(analyzer.Flag(), meta)

    if analyzer implements StoreWriter:
        analyzer.WriteToStore(ctx, ticks, w)
    else:
        report := analyzer.ReportFromTICKs(ctx, ticks)
        w.Write("report", report)

    w.Close()

    // Release memory before next analyzer.
    runner.aggregators[i] = nil
    agg.Close()
```

### StreamingConfig Wiring

- New field: `ReportStore analyze.ReportStore` on `StreamingConfig`.
- At finalize points: when `ReportStore != nil`, call `runner.FinalizeToStore(ctx, store)`
  instead of `runner.FinalizeWithAggregators(ctx)`.
- Legacy path unchanged when `ReportStore` is nil.

## Acceptance Criteria

1. `StoreWriter` interface compiles in `internal/analyzers/analyze/`.
2. `FinalizeToStore` processes each analyzer sequentially, nil-ing aggregators.
3. Legacy gob fallback: non-`StoreWriter` analyzers produce readable store entries.
4. Equivalence test: legacy fallback via store produces same data as `FinalizeWithAggregators`.
5. `StreamingConfig.ReportStore` field compiles and branches correctly.
6. `go build ./internal/...` compiles.
7. `go test ./internal/framework/...` passes.
8. `make lint` clean.

## Non-Goals

- Concrete `StoreWriter` implementations for specific analyzers (Phase 6-7).
- `runtime.GC()` calls — structural nil-ing is sufficient.
- Commit metadata injection into store (deferred until render command needs it).

## Implementation

Files created/modified:
- `internal/analyzers/analyze/history.go` — `StoreWriter` interface
- `internal/framework/runner.go` — `FinalizeToStore` method
- `internal/framework/runner_test.go` — equivalence test
- `internal/framework/export_test.go` — test helper exports
- `internal/framework/streaming.go` — `ReportStore` field on `StreamingConfig`
