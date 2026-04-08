# FRD: Remove MapFloat64 Duplicate (Roadmap 1.2)

**ID**: FRD-20260302-mapfloat64-dedup
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 1.2
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 1

## Problem

`MapFloat64` in `internal/analyzers/common/reportutil/reportutil.go:98` is character-for-character identical to `GetFloat64` at line 15. Both accept `map[string]any` and a key, return `float64`, and handle `int` → `float64` conversion identically. The duplication adds dead weight and confusion about which function to use.

## Feature

### 1.2 Remove MapFloat64 Duplicate

- Delete the `MapFloat64` function from `reportutil.go`
- Replace all 7 call sites with `GetFloat64`:
  - 5 in `internal/analyzers/halstead/report_section.go`
  - 2 in `internal/analyzers/cohesion/report_section.go`
- Delete 3 redundant `TestMapFloat64_*` test functions from `reportutil_test.go` (identical coverage exists via `TestGetFloat64_*`)

## Acceptance Criteria

- [x] `MapFloat64` function deleted from `reportutil.go`
- [x] All 7 call sites updated to use `GetFloat64`
- [x] 3 redundant `TestMapFloat64_*` tests deleted
- [x] `go vet ./internal/analyzers/common/reportutil/... ./internal/analyzers/halstead/... ./internal/analyzers/cohesion/...` clean
- [x] `go test ./internal/analyzers/common/reportutil/... ./internal/analyzers/halstead/... ./internal/analyzers/cohesion/...` passes
- [x] `make lint` passes (zero issues, zero dead code)

## Risk

Trivial. `MapFloat64` and `GetFloat64` are identical functions operating on the same type (`map[string]any`). The replacement is a mechanical rename with no behavioral change.

## Implementation

### Files Modified

- `internal/analyzers/common/reportutil/reportutil.go` — Deleted `MapFloat64` function (~13 lines)
- `internal/analyzers/common/reportutil/reportutil_test.go` — Deleted 3 `TestMapFloat64_*` test functions (~27 lines)
- `internal/analyzers/halstead/report_section.go` — Replaced 5 `reportutil.MapFloat64` → `reportutil.GetFloat64`
- `internal/analyzers/cohesion/report_section.go` — Replaced 2 `reportutil.MapFloat64` → `reportutil.GetFloat64`

### Lines Eliminated

~40 lines of duplicate code and tests removed.

### Verification

- `go vet` — clean
- `go test` — all pass
- `make lint` — zero issues, zero dead code
