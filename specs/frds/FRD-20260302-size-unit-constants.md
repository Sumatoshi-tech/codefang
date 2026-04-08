# FRD: Size Unit Constants (Roadmap 2.4)

**ID**: FRD-20260302-size-unit-constants
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 2.4
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 10

## Problem

`internal/budget/model.go` and `internal/streaming/planner.go` independently defined KiB/MiB/GiB size constants. A third duplicate (`bytesPerMiB = 1024 * 1024`) existed in `cmd/codefang/commands/run.go`. Three sources of truth for the same values.

## Solution

Create `pkg/units` as the single source of truth for binary size multipliers. All consumers import from this package.

### 2.4.a Create `pkg/units` package

Create `pkg/units/units.go` with `KiB`, `MiB`, `GiB` constants.

### 2.4.b Migrate consumers

- `internal/budget/model.go` — import from `pkg/units`, re-export for backward compatibility
- `internal/streaming/planner.go` — import from `pkg/units`, remove local constants
- `internal/streaming/memlog.go` — use `units.KiB`/`units.MiB`
- `cmd/codefang/commands/run.go` — replace `bytesPerMiB` with `units.MiB`

## Acceptance Criteria

- [x] `pkg/units/units.go` defines KiB, MiB, GiB
- [x] `pkg/units/units_test.go` validates constant values
- [x] No duplicate definitions of KiB/MiB/GiB remain outside `pkg/units`
- [x] `budget/model.go` re-exports from `pkg/units` for backward compatibility
- [x] `streaming/planner.go` uses `units.KiB`/`units.MiB` directly
- [x] `bytesPerMiB` removed from `run.go`, replaced with `units.MiB`
- [x] No import cycle introduced
- [x] `go vet` clean
- [x] `go test ./pkg/units/... ./internal/budget/... ./internal/streaming/... ./internal/framework/...` passes
- [x] `make lint` passes

## Risk

Trivial. Mechanical constant extraction. No behavioral change.

## Implementation

### Files Created

- `pkg/units/units.go` — Binary size multiplier constants
- `pkg/units/units_test.go` — Table-driven tests for constant values

### Files Modified

- `internal/budget/model.go` — Imports from `pkg/units`, re-exports constants
- `internal/streaming/planner.go` — Replaced local constants with `units.KiB`/`units.MiB`
- `internal/streaming/memlog.go` — Replaced references with `units.KiB`/`units.MiB`
- `internal/streaming/planner_test.go` — Updated constant references
- `internal/streaming/memlog_test.go` — Updated constant references
- `internal/framework/streaming.go` — Updated constant references
- `cmd/codefang/commands/run.go` — Replaced `bytesPerMiB` with `units.MiB`

### Lines Eliminated

~15 lines of duplicate constant definitions removed.

### Verification

- `go vet` — clean
- `go test` — all pass
- `make lint` — zero issues, zero dead code
