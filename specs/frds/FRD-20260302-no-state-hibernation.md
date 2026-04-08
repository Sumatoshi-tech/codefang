# FRD: Create NoStateHibernation mixin (Roadmap F0.7)

**ID**: FRD-20260302-no-state-hibernation
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.7
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster: Analyzer Lifecycle

## Problem

5 history analyzers implement identical no-op hibernation patterns — each in its own `hibernation.go` file with the same 4 methods:

| Analyzer | `Hibernate()` | `Boot()` | `WorkingStateSize()` | `AvgTCSize()` |
|----------|---------------|----------|---------------------|---------------|
| anomaly  | `return nil`  | `return nil` | `return 0` | `return 200`  |
| imports  | `return nil`  | `return nil` | `return 0` | `return 1024` |
| quality  | `return nil`  | `return nil` | `return 0` | `return 2048` |
| sentiment| `return nil`  | `return nil` | `return 0` | `return 500`  |
| typos    | `return nil`  | `return nil` | `return 0` | `return 200`  |

`Hibernate()` and `Boot()` are always `return nil`. `WorkingStateSize()` is always 0. Only `AvgTCSize()` varies per analyzer.

## Feature

Create an embeddable `NoStateHibernation` struct in `internal/analyzers/common/no_state_hibernation.go` that satisfies the `streaming.Hibernatable` interface with no-op implementations.

### no_state_hibernation.go — NoStateHibernation mixin

| Export | Signature | Behavior |
|--------|-----------|----------|
| `NoStateHibernation` | `struct{}` | Embeddable zero-size mixin |
| `(NoStateHibernation) Hibernate` | `func() error` | Returns nil (no-op) |
| `(NoStateHibernation) Boot` | `func() error` | Returns nil (no-op) |

### Design Decisions

- **Only `Hibernate()` and `Boot()`**: The `WorkingStateSize()` and `AvgTCSize()` methods belong to the `HistoryAnalyzer` interface (in `analyze/history.go`) and are already provided by `BaseHistoryAnalyzer[M]` via its `EstimatedStateSize`/`EstimatedTCSize` fields. Including them in the mixin would cause Go method promotion ambiguity when embedded alongside `BaseHistoryAnalyzer`. Instead, callers should set `EstimatedTCSize` in the constructor.
- **Value receiver**: Uses value receiver on a zero-size struct — no allocation, safe for concurrent use.
- **Satisfies `streaming.Hibernatable`**: Compile-time assertion via `var _ streaming.Hibernatable`.
- **No new dependencies**: Only imports `streaming` for the interface assertion.

### F1.7 Wiring Plan

When embedding `NoStateHibernation` in the 5 analyzers:
1. Embed `common.NoStateHibernation` in analyzer struct
2. Set `EstimatedTCSize` in constructor (moves `avgTCSize` from hibernation.go constant)
3. Remove the entire `hibernation.go` file (all 4 methods + constants + import + assertion become redundant)

## Acceptance Criteria

- [x] `internal/analyzers/common/no_state_hibernation.go` exports: `type NoStateHibernation struct{}` with `Hibernate() error` (returns nil), `Boot() error` (returns nil)
- [x] Compile-time assertion: `var _ streaming.Hibernatable = NoStateHibernation{}`
- [x] `internal/analyzers/common/no_state_hibernation_test.go` covers: Hibernate returns nil, Boot returns nil, interface satisfaction, zero-size struct (4 tests)
- [x] All tests pass
- [x] `go vet` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created:**
- `internal/analyzers/common/no_state_hibernation.go` — `NoStateHibernation` struct with `Hibernate()`, `Boot()`
- `internal/analyzers/common/no_state_hibernation_test.go` — 4 tests, 98% coverage

**Files deleted (F1.7 wiring):**
- `internal/analyzers/anomaly/hibernation.go`
- `internal/analyzers/imports/hibernation.go`
- `internal/analyzers/quality/hibernation.go`
- `internal/analyzers/sentiment/hibernation.go`
- `internal/analyzers/typos/hibernation.go`

**Files modified (F1.7 wiring):**
- `internal/analyzers/anomaly/analyzer.go` — embedded `NoStateHibernation`, set `EstimatedTCSize: 200`
- `internal/analyzers/imports/history.go` — embedded `NoStateHibernation`, set `EstimatedTCSize: 1024`
- `internal/analyzers/quality/analyzer.go` — embedded `NoStateHibernation`, set `EstimatedTCSize: 2048`
- `internal/analyzers/sentiment/analyzer.go` — embedded `NoStateHibernation`, set `EstimatedTCSize: 500`
- `internal/analyzers/typos/analyzer.go` — embedded `NoStateHibernation`, set `EstimatedTCSize: 200`
