# FRD: Type Conversion Utilities (Roadmap 2.2)

**ID**: FRD-20260302-type-conversion-utilities
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 2.2
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 4

## Problem

Five identical type conversion methods exist across three types in `internal/analyzers/common/`:

- `Formatter.toFloat` — float64 conversion (formatter.go)
- `Reporter.toFloat` — float64 conversion (reporter.go)
- `MetricsProcessor.extractFloat` — float64 conversion (metrics_processor.go)
- `Reporter.toInt` — int conversion (reporter.go)
- `MetricsProcessor.extractInt` — int conversion (metrics_processor.go)

All are private receiver methods with identical type switch bodies.

## Feature

### 2.2 Extract Standalone Type Conversion Functions

- Create `ToFloat64(value any) (float64, bool)` in `common/type_conversion.go`
- Create `ToInt(value any) (int, bool)` in `common/type_conversion.go`
- Replace all 5 method definitions and their call sites
- Delete redundant test functions (coverage now centralized)

## Acceptance Criteria

- [x] `ToFloat64` and `ToInt` standalone functions created
- [x] All 5 method definitions deleted
- [x] All call sites updated (14 occurrences across 3 files)
- [x] Table-driven tests with 10 cases each
- [x] Redundant method-level tests deleted (3 test functions)
- [x] `go vet` clean
- [x] `go test ./internal/analyzers/common/...` passes
- [x] `make lint` passes (zero issues, zero dead code)

## Risk

Trivial. All 5 methods have identical bodies. The standalone functions are in the same package, so no import changes needed at call sites.

## Implementation

### Files Created

- `internal/analyzers/common/type_conversion.go` — `ToFloat64` and `ToInt` functions
- `internal/analyzers/common/type_conversion_test.go` — Table-driven tests

### Files Modified

- `internal/analyzers/common/formatter.go` — Deleted `toFloat` method, replaced 2 call sites with `ToFloat64`
- `internal/analyzers/common/reporter.go` — Deleted `toFloat` and `toInt` methods, replaced 6 call sites
- `internal/analyzers/common/metrics_processor.go` — Deleted `extractFloat` and `extractInt` methods, replaced 2 call sites
- `internal/analyzers/common/formatter_test.go` — Deleted `TestFormatter_toFloat`
- `internal/analyzers/common/reporter_test.go` — Deleted `TestReporter_ToFloat` and `TestReporter_ToInt`
- `internal/analyzers/common/metrics_processor_test.go` — Deleted `TestMetricsProcessor_extractFloat` and `TestMetricsProcessor_extractInt`

### Lines Eliminated

~80 lines of duplicate method definitions + ~65 lines of redundant tests removed.

### Verification

- `go vet` — clean
- `go test` — all pass
- `make lint` — zero issues, zero dead code
