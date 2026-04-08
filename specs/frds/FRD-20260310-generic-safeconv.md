# FRD: Generic safeconv Functions (Roadmap 8.2)

**ID**: FRD-20260310-generic-safeconv
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 8.2
**Date**: 2026-03-10

## Problem

`pkg/safeconv` contains 7 individually-typed functions that repeat the same
overflow-check / clamp / type-switch patterns for specific type pairs:

- `MustUintToInt`, `MustIntToUint`, `MustIntToUint32` — panic on overflow
- `SafeInt64`, `SafeInt` — clamp on overflow
- `ToInt`, `ToFloat64` — extract from `any` via type switch

Each function is a hand-rolled version of a general concept. Adding a new
type pair (e.g., `MustInt64ToInt32`) requires writing another bespoke function.

## Decision

Add three generic functions to `pkg/safeconv`:

```go
// Integer constrains types to built-in integer types.
type Integer interface {
	~int | ~int8 | ~int16 | ~int32 | ~int64 |
		~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 | ~uintptr
}

// MustConvert converts v from From to To, panicking on overflow or sign loss.
func MustConvert[From, To Integer](v From) To

// SafeConvert converts v from From to To, clamping to [minVal, maxVal] of To on overflow.
func SafeConvert[From, To Integer](v From) To

// Extract type-asserts v (type any) to T, returning (zero, false) if it fails.
// If direct type assertion fails, attempts numeric coercion via reflect for
// numeric source and target types.
func Extract[T any](v any) (T, bool)
```

Delegate existing `Must*` and `Safe*` functions to the generic versions.
Mark `ToInt` and `ToFloat64` as deprecated in favour of `Extract[int]` and
`Extract[float64]`; keep them as-is for backward compatibility (Extract has
broader numeric coercion than the original switch statements).

## Contract

### MustConvert
- Converts `From` → `To` via `To(v)`.
- Overflow detection: round-trip check `From(To(v)) != v` OR sign change `(v < 0) != (To(v) < 0)`.
- Panics with `"safeconv: integer conversion overflow"` on overflow.
- No allocation.

### SafeConvert
- Same overflow detection as MustConvert.
- On overflow: clamps to `minVal[To]()` if v < 0, `maxVal[To]()` otherwise.
- `maxVal` / `minVal` computed without `unsafe` using typed `math` constants and round-trip detection.
- No allocation.

### Extract
- Fast path: direct type assertion `v.(T)`.
- Slow path: reflect-based numeric coercion (`reflect.Value.Convert`).
- Only coerces between numeric kinds (int*, uint*, float*).
- Returns `(zero, false)` for nil, non-numeric types, or non-numeric targets.

### Integer constraint
- Defined locally in `pkg/safeconv` (not imported from `pkg/alg/interval`).
- Same set of types as `interval.Integer` plus `~uintptr`.

## Scope

### Files created

| File | Content |
|------|---------|
| `pkg/safeconv/generic.go` | `Integer`, `MustConvert`, `SafeConvert`, `Extract`, helpers |
| `pkg/safeconv/generic_test.go` | Table-driven tests for all three generic functions |

### Files modified

| File | Change |
|------|--------|
| `pkg/safeconv/safeconv.go` | `Must*` delegate to `MustConvert`; `Safe*` delegate to `SafeConvert`; `ToInt`/`ToFloat64` add `Deprecated` comments |
| `pkg/safeconv/safeconv_test.go` | Update panic messages for `Must*` tests |

### Out of scope

- Deleting old functions (deprecated, not removed).
- Centralizing `Integer` constraint across packages.
- Complex number support in `Extract`.
- Changing `ToInt`/`ToFloat64` behavior (they keep their original type switch).

## Acceptance Criteria

- [x] `Integer`, `MustConvert`, `SafeConvert`, `Extract` implemented in `generic.go`
- [x] `Must*` functions delegate to `MustConvert`; `Safe*` delegate to `SafeConvert`
- [x] `ToInt`/`ToFloat64` delegate to `Extract`
- [x] Table-driven tests with ≥90% coverage
- [x] `go test ./pkg/safeconv/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Created

| File | Content |
|------|---------|
| `pkg/safeconv/generic.go` | `Integer` constraint, `MustConvert`, `SafeConvert`, `Extract`, `numericCoerce`, `isNumericKind`, `maxVal`, `minVal`, `signedMax` |
| `pkg/safeconv/generic_test.go` | Table-driven tests for MustConvert (10 tests), SafeConvert (5 test groups), Extract (12 tests) |

### Files Modified

| File | Change |
|------|--------|
| `pkg/safeconv/safeconv.go` | `Must*` → `MustConvert`, `Safe*` → `SafeConvert`, `ToInt`/`ToFloat64` → `Extract` |
| `pkg/safeconv/safeconv_test.go` | Updated panic messages to `panicOverflow`; `uint` tests now expect coercion success |
| `specs/ref/ROADMAP.md` | Marked 8.2 done, added FRD link |
