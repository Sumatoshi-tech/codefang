# FRD: Extract updateHighWatermark helper (Phase 5.2)

**ID**: FRD-20260317-update-high-watermark
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 5.2
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 5 Cross-Analyzer Consolidation

## Problem

StageMetrics RecordBlobBatch, RecordDiffQueue, and RecordCommit each contain identical CAS loops for updating high-watermark counters. The pattern: `for { peak := hwm.Load(); if val <= peak || hwm.CompareAndSwap(peak, val) { break } }`.

## Goal

Extract updateHighWatermark(hwm *atomic.Int64, val int64) and use it in all 4 loop sites.

## In Scope

- Add updateHighWatermark in internal/framework
- RecordBlobBatch (2 loops), RecordDiffQueue (1), RecordCommit (1) use it

## Acceptance Criteria

- [x] updateHighWatermark helper exists
- [x] All 4 CAS loops replaced
- [x] `go test ./internal/framework/...` passes
- [x] `make lint` passes

## Implementation

- Modified: internal/framework/stage_metrics.go (updateHighWatermark; RecordBlobBatch, RecordDiffQueue, RecordCommit use it)
