# FRD: Generic Pipeline Base Type Spike (Roadmap F4.1)

**ID**: FRD-20260302-generic-pipeline-spike
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F4.1

## Problem

`BlobPipeline` and `DiffPipeline` share structural patterns (producer-consumer goroutines,
`SharedResponse[T]` deduplication, worker pool dispatch, cache awareness). The question is
whether a generic `BatchProcessor[In, Out, Job]` can eliminate this duplication.

## Feature

Architecture spike to evaluate feasibility of a generic pipeline base type.

## Spike Result

**NOT FEASIBLE.** See [specs/ref/SPIKE-generic-pipeline.md](../ref/SPIKE-generic-pipeline.md)
for full analysis.

Key findings:
- Pipelines share ~30% of structure (goroutine topology, SharedResponse, worker dispatch)
- They diverge on ~70% (batch semantics, phase count, worker request types, result
  extraction, cache strategies, memory optimization)
- A generic type would need 8+ type parameters and 6+ hooks — more complex than the
  two concrete implementations combined
- The most impactful shared pattern (`SharedResponse[T]`) is already extracted (F2.3)

## Acceptance Criteria

- [x] Spike document in `specs/ref/` analyzing feasibility
- [x] Documented rationale for keeping pipelines separate

## Risk

**None.** Documentation-only deliverable; no code changes.

## Non-Goals

- Implementing a generic pipeline (determined infeasible).
- Modifying `BlobPipeline` or `DiffPipeline`.
- Changing `SharedResponse[T]`.

## Implementation

### Files Created

- `specs/ref/SPIKE-generic-pipeline.md` — architecture spike analyzing BlobPipeline vs
  DiffPipeline for generic unification feasibility
