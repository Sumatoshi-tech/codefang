# FRD: Wire pkg/safeconv into Scattered Callers (Roadmap F1.1)

**ID**: FRD-20260302-safeconv-wiring
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F1.1
**Depends on**: [FRD-20260302-safeconv-expansion.md](FRD-20260302-safeconv-expansion.md) (F0.1)

## Problem

Phase F0.1 moved safe type conversion functions into `pkg/safeconv`, but scattered
callers still carry local duplicates of the same type-switch logic:

| Location | Functions | Pattern |
|----------|-----------|---------|
| `common/type_conversion.go` | `ToFloat64`, `ToInt` | Thin wrappers delegating to safeconv — zero callers (dead code) |
| `common/reportutil/reportutil.go` | `GetFloat64`, `GetInt` | Independent type switch (should delegate to safeconv) |
| `complexity/report_section.go` | `getFloat64`, `getInt`, `getIntFromMap`, `getString`, `getFunctions`, `getStringFromMap` | Local duplicates of reportutil functions |
| `complexity/plot.go` | `getIntValue`, `getCyclomaticValue`, `getCognitiveValue`, `getNestingValue` | Local type switch for map int extraction |
| `complexity/aggregator.go` | `extractIntFromReport` | Local type switch (int, int64, float64) returning (int, bool) |
| `quality/analyzer.go` | `extractInt`, `extractFloat` | Local type switch for map extraction |

These 16 local functions duplicate logic that `pkg/safeconv` and `common/reportutil` already provide.

## Feature

1. **Delete** `common/type_conversion.go` — zero callers, pure dead code.
2. **Wire** `reportutil.GetFloat64` and `reportutil.GetInt` to delegate to `safeconv.ToFloat64` / `safeconv.ToInt`.
3. **Replace** all local `getFloat64`/`getInt`/`extractInt`/`extractFloat` variants in `complexity` and `quality` packages with `reportutil` or `safeconv` calls.
4. **Verify** all existing tests pass unchanged — this is a pure refactoring with zero behavior change.

## Acceptance Criteria

- [x] `internal/analyzers/common/type_conversion.go` deleted (dead code, zero callers)
- [x] `reportutil.GetFloat64` delegates to `safeconv.ToFloat64`
- [x] `reportutil.GetInt` delegates to `safeconv.ToInt`
- [x] `complexity/report_section.go` local helpers replaced with `reportutil` calls
- [x] `complexity/plot.go` local helpers replaced with `reportutil.GetInt`
- [x] `complexity/aggregator.go` `extractIntFromReport` replaced with `safeconv.ToInt`
- [x] `quality/analyzer.go` `extractInt`/`extractFloat` replaced with `reportutil` calls
- [x] All existing tests pass unchanged
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Trivial.** All replacements are behavior-preserving:
- `reportutil.GetFloat64(report, key)` handles the same types as the local variants (float64, int).
- `safeconv.ToFloat64/ToInt` handle a superset of types (also int32, int64) — strictly more capable.
- No new code paths are introduced.

## Non-Goals

- `burndown/plot.go:extractInt` has a fallback parameter (different signature) — out of scope.
- `internal/framework/config.go` — already wired in F0.1.
- `cmd/codefang/commands/run.go` — already wired in F0.1.

## Implementation

### Files Deleted

- `internal/analyzers/common/type_conversion.go` — dead code (wrappers over safeconv with zero callers)
- `internal/analyzers/common/type_conversion_test.go` — tests for deleted wrappers (covered by `pkg/safeconv/safeconv_test.go`)

### Files Modified

- `internal/analyzers/common/reportutil/reportutil.go` — `GetFloat64`/`GetInt` now delegate to `safeconv.ToFloat64`/`safeconv.ToInt`
- `internal/analyzers/common/formatter.go` — replaced `ToFloat64` with `safeconv.ToFloat64`
- `internal/analyzers/common/metrics_processor.go` — replaced `ToFloat64`/`ToInt` with `safeconv.ToFloat64`/`safeconv.ToInt`
- `internal/analyzers/common/reporter.go` — replaced `ToFloat64`/`ToInt` with `safeconv.ToFloat64`/`safeconv.ToInt`
- `internal/analyzers/complexity/report_section.go` — replaced 6 local helpers (`getFloat64`, `getInt`, `getIntFromMap`, `getString`, `getFunctions`, `getStringFromMap`) with `reportutil.*` calls
- `internal/analyzers/complexity/plot.go` — replaced `getIntValue` with `reportutil.GetInt` delegation
- `internal/analyzers/complexity/aggregator.go` — replaced `extractIntFromReport` type switch with `safeconv.ToInt`
- `internal/analyzers/quality/analyzer.go` — replaced `extractInt`/`extractFloat` with `reportutil.GetInt`/`reportutil.GetFloat64`
- `internal/analyzers/quality/analyzer_test.go` — updated tests to use `reportutil.GetInt`/`reportutil.GetFloat64`

### Lines Eliminated

~110 lines of duplicate type-switch logic removed across 7 files.

### Verification

- `go vet ./...` — clean
- `go test ./...` — all pass (67 packages)
- `make lint` — 0 issues, 0 dead code
