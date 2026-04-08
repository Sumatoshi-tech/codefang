# FRD: Migrate plumbing parallel patterns to WorkerPool (Phase 7.1)

**ID**: FRD-20260317-workerpool-plumbing
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 7.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 7 Pipeline/Worker Consolidation

## Problem

Plumbing analyzers (file_diff, uast) use ad-hoc goroutine+channel patterns for parallelism. analyzeFilesParallel and runParallel already use pkg/pipeline.WorkerPool. Consolidating plumbing to WorkerPool reduces duplication and standardizes concurrency.

## Current State (Documented)

| Location | Pattern | Uses WorkerPool |
|----------|---------|-----------------|
| static.go analyzeFilesParallel | WorkerPool.RunChan | Yes |
| analyzer.go runParallel | WorkerPool.Run | Yes |
| file_diff.go processChangesParallel | manual goroutines + jobs/results channels | No |
| uast.go changesParallel | manual goroutines + jobs/results channels | No |
| blob_cache.go consumeParallel | manual goroutines + batch splitting, per-worker repos | No |

## Goal

Migrate processChangesParallel and changesParallel to WorkerPool. Leave consumeParallel as-is (batch-based, per-worker repo allocation — different model).

## In Scope

- processChangesParallel → WorkerPool
- changesParallel → WorkerPool

## Out of Scope

- consumeParallel (batch + b.repos[idx] model; would require WorkerPool extension or separate design)
- cmd/uast parallel (if any)

## Acceptance Criteria

- [x] processChangesParallel uses pipeline.WorkerPool
- [x] changesParallel uses pipeline.WorkerPool
- [x] `go test ./internal/analyzers/plumbing/...` passes
- [x] `make lint` passes
- [x] No performance regression (semantic equivalence)

## Implementation

- Modified: internal/analyzers/plumbing/file_diff.go — processChangesParallel now uses WorkerPool, returns (map, error), Consume propagates error
- Modified: internal/analyzers/plumbing/uast.go — changesParallel now uses WorkerPool, returns ([]uast.Change, error), parseErr stored for inspection; removed sync import
