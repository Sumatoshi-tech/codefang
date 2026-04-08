# Reusable Code Dedup & Extraction Roadmap

**Spec**: `specs/ref/SPEC.md`
**Created**: 2026-03-20
**Status**: Active

---

## Overview

This roadmap decomposes the reusable code specification into progressive, testable steps. Each phase delivers standalone value. The focus is on deduplication first (eliminating redundant code), then extraction (promoting internal patterns to shared packages).

---

## Phase 1: High-ROI Dedup — Drop-in Replacements

Low-risk changes where a shared package already exists and callers just need rewiring.

### 1.1 Replace inline bloom functions in UAST loader with `pkg/alg/bloom`

- **Status**: `[ ]` Pending
- **Description**: `pkg/uast/loader.go` has hand-rolled `bloomAdd`, `bloomMayContain`, `bloomHashes` (~30 lines). Replace with the production-grade `pkg/alg/bloom.Filter`.
- **DoR**: Read `pkg/uast/loader.go` lines 178-210 and `pkg/alg/bloom/bloom.go` API.
- **DoD**:
  - `bloomAdd`, `bloomMayContain`, `bloomHashes` removed from `loader.go`
  - Loader uses `bloom.New(...)` with equivalent false-positive rate
  - Existing `pkg/uast` tests pass (`go test ./pkg/uast/...`)
  - No new allocations in hot path (benchmark parity)
- **Files**: `pkg/uast/loader.go`, `pkg/alg/bloom/bloom.go`

### 1.2 Replace hardcoded 1024 constants with `pkg/units`

- **Status**: `[ ]` Pending
- **Description**: Several test files use raw `1024*1024` or `32*1024*1024*1024` instead of `units.MiB`, `units.GiB`. Migrate remaining occurrences.
- **DoR**: Grep for `1024` patterns in non-third-party Go files.
- **DoD**:
  - All non-third-party Go files use `units.KiB/MiB/GiB` instead of raw multipliers
  - `go build ./...` passes
  - Values unchanged (mechanical replacement)
- **Files**: `internal/framework/runner_test.go`, `internal/framework/config_test.go`, `internal/analyzers/couples/memory_leak_test.go`, `pkg/alg/bloom/bloom_test.go`, `cmd/codefang/commands/universal_memory_test.go`

### 1.3 Replace inline chunk loops with `pkg/alg/chunk.Chunk`

- **Status**: `[ ]` Pending
- **Description**: Three files have hand-rolled chunking loops that duplicate `alg.Chunk()`.
- **DoR**: Read `pkg/alg/chunk.go` API and the three inline loop sites.
- **DoD**:
  - `scripts/bench-hibernation/main.go:~291` uses `alg.Chunk()`
  - `internal/framework/commit_streamer.go:~49` uses `alg.Chunk()`
  - `internal/analyzers/common/renderer/renderer.go:~148` uses `alg.Chunk()`
  - All tests pass for affected packages
- **Files**: `pkg/alg/chunk.go`, `scripts/bench-hibernation/main.go`, `internal/framework/commit_streamer.go`, `internal/analyzers/common/renderer/renderer.go`

### 1.4 Migrate `os.ReadFile` callers to `pkg/iosafety.ReadFile`

- **Status**: `[ ]` Pending
- **Description**: 12 production `os.ReadFile()` calls bypass path validation. Migrate to `iosafety.ReadFile()` where the path originates from user input or external config.
- **DoR**: Grep `os.ReadFile` in non-test, non-third-party Go files. Classify each call as user-facing vs internal.
- **DoD**:
  - All user-facing `os.ReadFile` calls replaced with `iosafety.ReadFile`
  - Internal reads (embedded data, known-safe paths) left as-is with a comment explaining why
  - Tests pass for all modified packages
- **Files**: Various — audit required

---

## Phase 2: Consolidate Redundant Abstractions

Merge overlapping types where two implementations serve the same purpose.

### 2.1 Consolidate `ThresholdLabeler` into `Classifier[float64]`

- **Status**: `[ ]` Pending
- **Description**: `threshold_labeler.go` is a non-generic type alias `[]Threshold[float64]`. `classify.go` provides a generic `Classifier[T]` with a default label and sorting. Migrate all `ThresholdLabeler` callers to `Classifier[float64]` and delete the file.
- **DoR**: Identify all callers of `ThresholdLabeler` via grep. Read both files.
- **DoD**:
  - `threshold_labeler.go` deleted
  - All callers use `Classifier[float64]` (or appropriate `T`)
  - Tests pass for all affected analyzers
  - No behavioral change in classification output
- **Files**: `internal/analyzers/common/threshold_labeler.go`, `internal/analyzers/common/classify.go`

### 2.2 Unify `VisitPreOrder` and `TraverseTree`

- **Status**: `[ ]` Pending
- **Description**: `pkg/uast/pkg/node/node.go` has `VisitPreOrder` (28+ usages) and `pkg/alg/tree.go` has `TraverseTree` (1 usage). They do the same thing. Evaluate whether `VisitPreOrder` can delegate to `TraverseTree` internally, keeping the Node-specific API stable.
- **DoR**: Read both implementations. Map all 28+ `VisitPreOrder` call sites. Assess if `TraverseTree` signature covers all use cases.
- **DoD**:
  - `VisitPreOrder` internally delegates to `TraverseTree` OR `TraverseTree` is enhanced and `VisitPreOrder` becomes a thin wrapper
  - All 28+ call sites unchanged (API-compatible)
  - `go test ./pkg/uast/... ./pkg/alg/... ./internal/analyzers/...` passes
  - Benchmark parity (no performance regression)
- **Files**: `pkg/uast/pkg/node/node.go`, `pkg/alg/tree.go`

---

## Phase 3: Extract Internal Patterns to `pkg/`

Promote battle-tested internal utilities to shared packages for broader reuse.

### 3.1 Extract `ContextStack[T]` to `pkg/alg/stack`

- **Status**: `[ ]` Pending
- **Description**: `ContextStack[T]` in `internal/analyzers/common/context_stack.go` is a clean, generic LIFO stack. Extract to `pkg/alg/stack/stack.go` for reuse outside analyzers.
- **DoR**: Read `context_stack.go`. Identify all importers.
- **DoD**:
  - `pkg/alg/stack/stack.go` contains `Stack[T]` (renamed from `ContextStack`)
  - `internal/analyzers/common/context_stack.go` becomes a type alias or is deleted with imports updated
  - All existing tests pass
  - New `pkg/alg/stack/stack_test.go` with unit tests
- **Files**: `internal/analyzers/common/context_stack.go` → `pkg/alg/stack/stack.go`

### 3.2 Extract `Classifier[T]` to `pkg/alg/classify`

- **Status**: `[ ]` Pending
- **Description**: After Phase 2.1, `Classifier[T]` will be the sole classification utility. Extract from `internal/analyzers/common/` to `pkg/alg/classify/` for reuse.
- **DoR**: Phase 2.1 completed. Read `classify.go` and all importers.
- **DoD**:
  - `pkg/alg/classify/classify.go` with `Classifier[T]`
  - Old file deleted or re-exports from new location
  - All tests pass
  - New `pkg/alg/classify/classify_test.go`
- **Files**: `internal/analyzers/common/classify.go` → `pkg/alg/classify/classify.go`
- **Depends on**: 2.1

### 3.3 Extract `SpillableDataCollector` to `pkg/spill`

- **Status**: `[ ]` Pending
- **Description**: The spillable data collector pattern (transparent spill-to-disk when buffer exceeds threshold) is a high-value reusable pattern. Extract from `internal/analyzers/common/spillable_data_collector.go`.
- **DoR**: Read the collector, its tests, and all importers. Understand the gob encoding and composite key dependencies.
- **DoD**:
  - `pkg/spill/collector.go` with the generic collector
  - All existing tests pass
  - New package-level tests in `pkg/spill/collector_test.go`
  - No dependency on `internal/` packages from `pkg/spill`
- **Files**: `internal/analyzers/common/spillable_data_collector.go` → `pkg/spill/collector.go`

---

## Phase 4: Ad-hoc Type Assertion Cleanup

### 4.1 Audit and replace remaining `val.(type)` assertions with `pkg/safeconv`

- **Status**: `[ ]` Pending
- **Description**: While `pkg/safeconv` is widely adopted (20+ call sites), audit the codebase for remaining bare type assertions on `any`/`interface{}` values in production code. Replace with `safeconv.Extract[T]` or appropriate wrapper.
- **DoR**: Grep for `\.\(int\)`, `\.\(float64\)`, `\.\(string\)` in non-test Go files. Filter to cases operating on `any`/`interface{}`.
- **DoD**:
  - All `any`-typed value assertions in production code use `safeconv`
  - Internal strongly-typed assertions (known concrete types) left as-is
  - Tests pass
- **Files**: Various — audit required

---

## Phase 5: Verify & Harden

### 5.1 Cross-package integration test

- **Status**: `[ ]` Pending
- **Description**: Run full test suite and verify no regressions from Phases 1–4.
- **DoR**: All prior phases completed.
- **DoD**:
  - `go test ./...` passes
  - `go vet ./...` clean
  - `go build ./cmd/...` succeeds
  - No new lint warnings
- **Depends on**: 1.1–4.1

### 5.2 Benchmark comparison

- **Status**: `[ ]` Pending
- **Description**: Run existing benchmarks before and after the full dedup to confirm no performance regressions.
- **DoR**: Identify existing benchmark tests. Capture baseline.
- **DoD**:
  - Benchmark results documented (before/after)
  - No >5% regression on any benchmark
  - Memory allocation counts unchanged or improved
- **Depends on**: 5.1

---

## Items Already Done (No Action Needed)

These were identified in the spec but are already well-designed and in active use:

| Package | Status | Evidence |
|---------|--------|----------|
| `pkg/alg/*` (bloom, cms, hll, lsh, etc.) | Complete | Active usage across analyzers |
| `pkg/pipeline/*` (WorkerPool, RunPC, Phase, Batcher) | Complete | Used by framework |
| `pkg/metrics/metrics.go` | Complete | Metric definition pattern in use |
| `pkg/sigutil/sigutil.go` | Complete | Signal handling in use |
| `pkg/pathfilter/pathfilter.go` | Complete | File filtering in use |
| `pkg/persist/persist.go` | Complete | Persistence layer in use |
| `pkg/safeconv/` | Mostly Complete | 20+ call sites (Phase 4.1 for stragglers) |
| `pkg/units/` | Mostly Complete | Production code migrated (Phase 1.2 for tests) |
| `pkg/textutil/` | Complete | Text utilities in use |
| `pkg/timeutil/` | Complete | Newly added, in use |

---

## Dependency Graph

```
Phase 1 (all independent, can parallelize)
  1.1  1.2  1.3  1.4

Phase 2 (independent of Phase 1)
  2.1  2.2

Phase 3 (3.2 depends on 2.1, rest independent)
  3.1  3.2 → 2.1
  3.3

Phase 4 (independent)
  4.1

Phase 5 (depends on all above)
  5.1 → all
  5.2 → 5.1
```

---

## Changelog

- **2026-03-20**: Initial roadmap created from `SPEC.md` analysis and codebase audit.
