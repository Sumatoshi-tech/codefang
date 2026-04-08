# FRD: GetAs[T] generic accessor in reportutil (Roadmap 2.2)

**ID**: FRD-20260306-reportutil-getas
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 2.2
**Date**: 2026-03-06

## Problem

`internal/analyzers/common/reportutil/reportutil.go` has 5 typed accessors over
`map[string]any` that all repeat the same two-step idiom:

1. Check key exists; return zero if not.
2. Type-assert to `T`; return zero if assertion fails.
3. Return value.

Current duplicated pattern (example × 4):

```go
func GetString(report map[string]any, key string) string {
    if v, ok := report[key]; ok {
        if s, isStr := v.(string); isStr {
            return s
        }
    }
    return ""
}
```

The same 6-line structure is repeated in `GetString`, `GetStringSlice`,
`GetStringIntMap`, `GetFunctions`, and `MapString`.

**DoR findings:**

- `GetFloat64` and `GetInt` use `safeconv.ToFloat64` / `safeconv.ToInt` for
  cross-type numeric coercion (`int` stored as `float64` and vice-versa).
  A pure type-assertion `GetAs[float64]` would fail for `int` values, breaking
  callers. These two functions **keep the safeconv path**.
- The other 5 functions (`GetString`, `GetStringSlice`, `GetStringIntMap`,
  `GetFunctions`, `MapString`) always store and retrieve the exact same type —
  pure type assertion is correct and sufficient.

## Decision

Add a generic base accessor:

```go
// GetAs extracts a value of type T from a report map via direct type assertion.
// Returns (zero, false) if the key is absent or the value is not of type T.
// For numeric types requiring cross-type coercion use GetFloat64 or GetInt.
func GetAs[T any](report map[string]any, key string) (T, bool)
```

Refactor the 5 pure-assertion getters to one-liner delegators.

## Contract

- `GetAs` on nil map: `report[key]` on a nil map panics → callers must not pass
  nil. All current callers pass non-nil maps (same pre-condition as before).
- `GetAs[T](report, key)` when key missing → `(zero(T), false)`
- `GetAs[T](report, key)` when value wrong type → `(zero(T), false)`
- `GetAs[T](report, key)` when value is `T` → `(value, true)`
- `GetFloat64` / `GetInt` are **not** changed; they retain safeconv semantics.

## Scope

### Files modified

| File | Change |
|------|--------|
| `internal/analyzers/common/reportutil/reportutil.go` | Add `GetAs[T]`; refactor 5 getters |
| `internal/analyzers/common/reportutil/reportutil_test.go` | Tests for `GetAs[T]` |

### Out of scope

- `GetFloat64` / `GetInt` — numeric coercion; not delegated to `GetAs`
- `FormatInt`, `FormatFloat`, `FormatPercent`, `Pct` — formatting; unrelated

## Acceptance Criteria

- [x] `GetAs[T any]` added to `reportutil.go`
- [x] `GetString`, `GetStringSlice`, `GetStringIntMap`, `GetFunctions`, `MapString`
      delegate to `GetAs[T]`
- [x] `GetFloat64` and `GetInt` unchanged (safeconv path preserved)
- [x] `go test ./internal/analyzers/common/reportutil/...` passes
- [x] All existing tests remain passing (no callers broken)
- [x] `make lint` — 0 issues, no dead code

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `internal/analyzers/common/reportutil/reportutil.go` | Add `GetAs[T]`; delegate 5 getters |
| `internal/analyzers/common/reportutil/reportutil_test.go` | `GetAs[T]` tests |
| `specs/ref/ROADMAP.md` | Mark 2.2 done |
| `AGENTS.md` | Add `GetAs` entry |
