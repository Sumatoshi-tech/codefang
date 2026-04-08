# FRD: Expand pkg/safeconv with Clamp + Extraction Variants (Roadmap F0.1)

**ID**: FRD-20260302-safeconv-expansion
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 1: Type Conversions & Safe Arithmetic

## Problem

Safe type conversion functions are duplicated in three locations:

1. **`pkg/safeconv/safeconv.go`** — `MustUintToInt`, `MustIntToUint`, `MustIntToUint32` (canonical)
2. **`internal/framework/config.go:204-219`** — `SafeInt64(uint64) int64`, `SafeInt(uint64) int` (clamping conversions)
3. **`internal/analyzers/common/type_conversion.go`** — `ToFloat64(any) (float64, bool)`, `ToInt(any) (int, bool)` (type-switch extraction)

`pkg/safeconv` is the canonical location for safe conversions, but it only has the `Must*` (panic-on-overflow) variants. The clamping and extraction variants live elsewhere.

## Feature

Add four functions to `pkg/safeconv`:

| Function | Signature | Behavior |
|----------|-----------|----------|
| `SafeInt64` | `SafeInt64(v uint64) int64` | Clamp to `math.MaxInt64` on overflow |
| `SafeInt` | `SafeInt(v uint64) int` | Clamp to `MaxInt` on overflow |
| `ToFloat64` | `ToFloat64(value any) (float64, bool)` | Type-switch extraction: float64, int, int32, int64 |
| `ToInt` | `ToInt(value any) (int, bool)` | Type-switch extraction: int, int32, int64, float64 |

## Acceptance Criteria

- [x] `pkg/safeconv/safeconv.go` exports existing `Must*` + new `SafeInt64`, `SafeInt`, `ToFloat64`, `ToInt`
- [x] `MaxInt64` constant exported for use by callers
- [x] `pkg/safeconv/safeconv_test.go` covers all cases (35 subtests, 100% coverage)
- [x] `go vet ./pkg/safeconv/...` clean
- [x] `go test ./pkg/safeconv/...` passes
- [x] `go test -race ./pkg/safeconv/...` passes
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

Trivial. Behavioral identity preserved — callers now delegate to `pkg/safeconv`.

## Implementation

### Files Created

- `pkg/safeconv/safeconv_test.go` — 7 test functions, 35 subtests, 100% coverage

### Files Modified

- `pkg/safeconv/safeconv.go` — Added `MaxInt64`, `SafeInt64`, `SafeInt`, `ToInt`, `ToFloat64`
- `internal/framework/config.go` — Removed local `maxInt`/`maxInt64` constants and `SafeInt64`/`SafeInt` functions; now imports `pkg/safeconv`
- `internal/framework/coordinator.go` — Replaced `SafeInt64()` calls with `safeconv.SafeInt64()`
- `cmd/codefang/commands/run.go` — Replaced `framework.SafeInt64()` with `safeconv.SafeInt64()`
- `internal/analyzers/common/type_conversion.go` — Now delegates to `safeconv.ToFloat64`/`safeconv.ToInt`

### Lines Eliminated

~25 lines of duplicate function definitions removed from `internal/framework/config.go`.

### Verification

- `go vet` — clean
- `go test` — all pass (100% coverage)
- `go test -race` — clean
- `make lint` — 0 issues, 0 dead code
