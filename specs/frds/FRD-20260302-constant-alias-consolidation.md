# FRD: Trivial Constant/Alias Consolidation (Roadmap F1.11)

**ID**: FRD-20260302-constant-alias-consolidation
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F1.11

## Problem

Three minor duplications remain after Phase 0 + Phase 1 wiring:

| Location | Duplication | Source |
|----------|-------------|--------|
| `internal/budget/model.go` | Re-exports `KiB`/`MiB`/`GiB` from `pkg/units` | LIST.md #1 |
| `internal/cache/lru.go` | `LRUStats` struct duplicates `pkg/alg/lru.Stats` (minus `MaxEntries`) | LIST.md #5 |
| `common/formatter.go` + `common/reporter.go` | `extractMetrics` / `extractKeyMetrics` share identical core logic | LIST.md #24 |

## Feature

### 1. Budget constants

Remove `KiB`/`MiB`/`GiB` re-exports from `internal/budget/model.go`. Replace all references
in `budget/` with `units.KiB`/`units.MiB`/`units.GiB`. Update the one external caller
(`cmd/codefang/commands/run.go`) to import `pkg/units` directly.

### 2. LRUStats type alias

Replace `cache.LRUStats` struct + `HitRate()` method with `type LRUStats = lru.Stats`.
Simplify `LRUBlobCache.Stats()` to return `lru.Stats` directly (no field-by-field copy).
The `MaxEntries` field from `lru.Stats` is now visible but will be zero (blob cache uses
size-based limits, not count-based) — harmless addition.

### 3. extractMetrics consolidation

Create a package-level function `extractAllNumericMetrics(report analyze.Report) map[string]float64`
in `internal/analyzers/common/`. Have `Formatter.extractMetrics` delegate to it.
Have `Reporter.extractKeyMetrics` delegate to it for the no-filter path.

## Acceptance Criteria

- [x] `internal/budget/model.go` — `KiB`/`MiB`/`GiB` re-exports removed; all budget references use `units.*`
- [x] `cmd/codefang/commands/run.go` — `budget.MiB` replaced with `units.MiB`
- [x] `internal/cache/lru.go` — `LRUStats` struct + `HitRate()` deleted; replaced with `type LRUStats = lru.Stats`
- [x] `internal/cache/lru.go` — `Stats()` method simplified (no manual field copy)
- [x] `internal/analyzers/common/` — shared `extractAllNumericMetrics` function created
- [x] `Formatter.extractMetrics` delegates to `extractAllNumericMetrics`
- [x] `Reporter.extractKeyMetrics` delegates to `extractAllNumericMetrics` for unfiltered path
- [x] All existing tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Trivial.** All changes are behavior-preserving:
- Budget constants are already backed by `pkg/units`; removing re-exports only changes import paths.
- `lru.Stats` is a superset of `LRUStats` (adds `MaxEntries`). Existing field accesses remain valid.
- `extractAllNumericMetrics` is a pure extraction of the shared loop — no logic change.

## Non-Goals

- Moving `DefaultLRUCacheSize` or other cache constants — different concern.
- Changing `DiffCacheStats` — uses its own `lru.Stats` already.
- Refactoring the filtered path of `extractKeyMetrics` — only the unfiltered path is duplicated.

## Implementation

### Files Modified

- `internal/budget/model.go` — removed `KiB`/`MiB`/`GiB` re-exports; all constants now use `units.KiB`/`units.MiB`/`units.GiB` directly
- `internal/budget/solver.go` — added `pkg/units` import; replaced `MiB`/`GiB` with `units.MiB`/`units.GiB`
- `internal/budget/model_test.go` — added `pkg/units` import; replaced `MiB` with `units.MiB`
- `internal/budget/solver_test.go` — added `pkg/units` import; replaced `KiB`/`MiB`/`GiB` with `units.KiB`/`units.MiB`/`units.GiB`
- `cmd/codefang/commands/run.go` — replaced `budget.MiB` with `units.MiB` (already imported `pkg/units`)
- `internal/cache/lru.go` — `LRUStats` struct + `HitRate()` (~18 lines) replaced with `type LRUStats = lru.Stats`; `Stats()` method simplified to `return c.cache.Stats()`
- `internal/analyzers/common/formatter.go` — `extractMetrics` now delegates to new `extractAllNumericMetrics`
- `internal/analyzers/common/reporter.go` — `extractKeyMetrics` unfiltered path delegates to `extractAllNumericMetrics`

### Lines Eliminated

~30 lines of duplicate code removed across 3 areas.

### Verification

- `go vet ./...` — clean
- `go test ./internal/budget/... ./internal/cache/... ./internal/analyzers/common/...` — all pass
- `make lint` — 0 issues, 0 dead code
