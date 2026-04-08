# FRD: Typed Fact Accessors (Roadmap 3.3)

**ID**: FRD-20260302-typed-fact-accessors
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 3.3
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 5, LIST #20

## Problem

Nine analyzer `Configure()` methods perform raw `facts[key].(Type)` type assertions to extract shared pipeline facts. The same fact keys — `FactTickSize`, `FactCommitsByTick`, `FactIdentityDetectorReversedPeopleDict`, and `FactIdentityDetectorPeopleCount` — are asserted with identical patterns across multiple analyzers. This duplicates type knowledge at each call site, risking type mismatches and making the fact contract implicit rather than explicit.

### Current usage matrix

| Fact Key | Type | Used By |
|----------|------|---------|
| `FactTickSize` | `time.Duration` | burndown, devs, imports |
| `FactCommitsByTick` | `map[int][]gitlib.Hash` | devs, anomaly, quality, sentiment |
| `FactIdentityDetectorReversedPeopleDict` | `[]string` | burndown, couples, devs, imports |
| `FactIdentityDetectorPeopleCount` | `int` | burndown, couples |

## Solution

Create four typed accessor functions in `internal/plumbing/fact_accessors.go` that encapsulate the key lookup and type assertion. Each returns `(T, bool)` — the typed value and whether it was present with the correct type.

### Placement

`internal/plumbing/fact_accessors.go` — collocated with the key constants in `keys.go` and types in `types.go`.

### API

```go
// GetTickSize extracts the tick duration from the facts map.
func GetTickSize(facts map[string]any) (time.Duration, bool)

// GetCommitsByTick extracts the commits-by-tick mapping from the facts map.
func GetCommitsByTick(facts map[string]any) (map[int][]gitlib.Hash, bool)

// GetReversedPeopleDict extracts the reversed people dictionary from the facts map.
func GetReversedPeopleDict(facts map[string]any) ([]string, bool)

// GetPeopleCount extracts the unique author count from the facts map.
func GetPeopleCount(facts map[string]any) (int, bool)
```

### Migration (per analyzer)

Before:
```go
if val, exists := facts[pkgplumbing.FactTickSize].(time.Duration); exists {
    b.TickSize = val
}
```

After:
```go
if val, ok := pkgplumbing.GetTickSize(facts); ok {
    b.TickSize = val
}
```

Before:
```go
if val, exists := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string); exists {
    h.reversedPeopleDict = val
}
```

After:
```go
if val, ok := pkgplumbing.GetReversedPeopleDict(facts); ok {
    h.reversedPeopleDict = val
}
```

This removes the direct `identity` import from analyzers that only use the fact key for a type assertion, consolidating the identity-key dependency into `plumbing`.

## Acceptance Criteria

- [x] Four typed accessor functions defined in `internal/plumbing/fact_accessors.go`
- [x] Unit tests in `internal/plumbing/fact_accessors_test.go` covering:
  - Key present with correct type returns `(value, true)`
  - Key absent returns `(zero, false)`
  - Key present with wrong type returns `(zero, false)`
- [x] All analyzers migrated — no raw `facts[key].(Type)` for the four covered keys
- [x] Analyzers that no longer need `identity` import have it removed (devs, imports)
- [x] `go vet` clean
- [x] `go test ./internal/plumbing/... ./internal/analyzers/...` passes (25 packages)
- [x] `make lint` passes — zero issues, zero dead code

## Risk

Low. Each accessor is a trivial two-line function. Each migration is a mechanical call-site replacement. No behavior change — same type assertion, same boolean semantics.

## Implementation

### Files Created

- `internal/plumbing/fact_accessors.go`
- `internal/plumbing/fact_accessors_test.go`

### Files Modified

- `internal/analyzers/burndown/history.go` — use `GetTickSize`, `GetReversedPeopleDict`, `GetPeopleCount`
- `internal/analyzers/couples/history.go` — use `GetPeopleCount`, `GetReversedPeopleDict`
- `internal/analyzers/devs/analyzer.go` — use `GetTickSize`, `GetCommitsByTick`, `GetReversedPeopleDict`
- `internal/analyzers/anomaly/analyzer.go` — use `GetCommitsByTick`
- `internal/analyzers/quality/analyzer.go` — use `GetCommitsByTick`
- `internal/analyzers/sentiment/analyzer.go` — use `GetCommitsByTick`
- `internal/analyzers/imports/history.go` — use `GetTickSize`, `GetReversedPeopleDict`

### Lines Eliminated

~40 lines of duplicated type assertion boilerplate eliminated. More importantly: the type contract is now centralized.

### Verification

- `go vet` — clean
- `go test ./internal/plumbing/... ./internal/analyzers/...` — all pass
- `make lint` — zero issues, zero dead code
