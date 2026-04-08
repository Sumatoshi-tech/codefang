# FRD: Move SharedResponse[T] to pkg/pipeline (Roadmap F5.1)

**ID**: FRD-20260303-shared-response-move
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F5.1
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 4: Pipeline & Concurrency Primitives

## Problem

`SharedResponse[T]` is a fully generic sync.Once memoization primitive defined in
`internal/framework/shared_response.go`. It has zero framework-specific dependencies
(only `context` and `sync`), yet it lives in an internal package that prevents reuse
outside the framework. The `pkg/pipeline` package already contains composable pipeline
primitives (`RunPC`, `Phase`, `Batcher`, `Fetcher`) — `SharedResponse[T]` belongs there.

## Feature

Move `SharedResponse[T]` and `NewSharedResponse[T]` from `internal/framework/` to
`pkg/pipeline/`. Update the 2 framework callers (`diff_pipeline.go`, `blob_pipeline.go`)
to reference `pipeline.SharedResponse` and `pipeline.NewSharedResponse`.

### Design Decisions

- **Move, not copy**: Delete the original file after creating the new one. No backward
  compatibility alias needed because all callers are within `internal/framework/`.
- **Tests move too**: The existing 6 tests migrate from `framework_test` to `pipeline_test`
  with updated imports.
- **Same API**: Zero signature changes. The type, constructor, and method remain identical.

### Migration Scope

| File | Action |
|------|--------|
| `pkg/pipeline/shared_response.go` | Create (moved from framework) |
| `pkg/pipeline/shared_response_test.go` | Create (moved from framework) |
| `internal/framework/shared_response.go` | Delete |
| `internal/framework/shared_response_test.go` | Delete |
| `internal/framework/diff_pipeline.go` | Update 3 references to `pipeline.*` |
| `internal/framework/blob_pipeline.go` | Update 4 references to `pipeline.*` |

## Acceptance Criteria

- [x] `pipeline.SharedResponse[T]` exists in `pkg/pipeline/shared_response.go`
- [x] `pipeline.NewSharedResponse[T]` exists in `pkg/pipeline/shared_response.go`
- [x] 6 tests in `pkg/pipeline/shared_response_test.go`
- [x] `internal/framework/shared_response.go` deleted
- [x] `internal/framework/shared_response_test.go` deleted
- [x] `diff_pipeline.go` uses `pipeline.SharedResponse` and `pipeline.NewSharedResponse`
- [x] `blob_pipeline.go` uses `pipeline.SharedResponse` and `pipeline.NewSharedResponse`
- [x] All existing tests pass
- [x] `go vet` clean, `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created:**
- `pkg/pipeline/shared_response.go` — `SharedResponse[T]` struct, `NewSharedResponse[T]` constructor, `Get` method
- `pkg/pipeline/shared_response_test.go` — 6 tests: ReturnsComputedValue, ReturnsError, EvaluatesOnce, CancelledContext, CachesResultAcrossCalls, CachesErrorAcrossCalls

**Files deleted:**
- `internal/framework/shared_response.go`
- `internal/framework/shared_response_test.go`

**Files modified:**
- `internal/framework/diff_pipeline.go` — 3 references updated to `pipeline.SharedResponse` / `pipeline.NewSharedResponse`
- `internal/framework/blob_pipeline.go` — 4 references updated to `pipeline.SharedResponse` / `pipeline.NewSharedResponse`
