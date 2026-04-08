# FRD: Config Loader Fact Application (Roadmap 4.2)

**ID**: FRD-20260302-config-loader-facts
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 4.2
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 5, LIST #26

## Problem

`internal/config/loader.go` contains 7 `applyXxxFacts` functions that follow nearly identical patterns to transfer config values into the analyzer facts map:

| Function | Lines | Fields | Pattern |
|----------|-------|--------|---------|
| `applyBurndownFacts` | 131-158 | 9 | `if int > 0`, `bool always`, `if string != ""` |
| `applyDevsFacts` | 160-163 | 2 | `bool always` |
| `applyImportsFacts` | 165-173 | 2 | `if int > 0` |
| `applySentimentFacts` | 175-183 | 2 | `if int > 0`, `if float64 > 0` (cast to float32) |
| `applyShotnessFacts` | 185-193 | 2 | `if string != ""` |
| `applyTyposFacts` | 195-199 | 1 | `if int > 0` |
| `applyAnomalyFacts` | 201-209 | 2 | `if float64 > 0` (cast to float32), `if int > 0` |

### Three repeated patterns

1. **Positive numeric**: `if value > 0 { facts[key] = value }` — 12 fields
2. **Non-empty string**: `if value != "" { facts[key] = value }` — 3 fields
3. **Always-apply bool**: `facts[key] = value` — 6 fields

Plus 2 fields with `float64 → float32` conversion (`Sentiment.Gap`, `Anomaly.Threshold`).

Total: ~90 lines (118-209) encoding 3 distinct patterns across 7 functions.

## Solution

Replace all 7 functions with 3 generic helper functions and a single `ApplyToFacts` method that reads as a declarative mapping table.

### Approach: Type-safe generic helpers (builder-style, no reflection)

```go
// positive constrains types eligible for > 0 skip-on-zero semantics.
type positive interface {
    ~int | ~float32
}

// applyPositive sets facts[key] = value if value > 0.
func applyPositive[T positive](facts map[string]any, key string, value T)

// applyNonEmpty sets facts[key] = value if value is non-empty.
func applyNonEmpty(facts map[string]any, key string, value string)

// applyBool sets facts[key] = value unconditionally.
func applyBool(facts map[string]any, key string, value bool)
```

### Rewritten ApplyToFacts

```go
func (c *Config) ApplyToFacts(facts map[string]any) {
    bd := c.History.Burndown
    applyPositive(facts, "Burndown.Granularity", bd.Granularity)
    applyPositive(facts, "Burndown.Sampling", bd.Sampling)
    applyBool(facts, "Burndown.TrackFiles", bd.TrackFiles)
    applyBool(facts, "Burndown.TrackPeople", bd.TrackPeople)
    applyPositive(facts, "Burndown.HibernationThreshold", bd.HibernationThreshold)
    applyBool(facts, "Burndown.HibernationOnDisk", bd.HibernationToDisk)
    applyNonEmpty(facts, "Burndown.HibernationDirectory", bd.HibernationDirectory)
    applyBool(facts, "Burndown.Debug", bd.Debug)
    applyPositive(facts, "Burndown.Goroutines", bd.Goroutines)

    // ... remaining analyzers as one-liner calls ...
}
```

### Key design decisions

1. **Generics over reflection**: The `positive` type constraint with `~int | ~float32` provides compile-time type safety. No `reflect` import needed.

2. **float64 → float32 at call site**: The two fields that need conversion (`Sentiment.Gap`, `Anomaly.Threshold`) pass `float32(value)` to `applyPositive`, so the stored fact is `float32` — matching original behavior exactly.

3. **No constant extraction for fact keys**: Each fact key string appears exactly once in production code. Test constants in `apply_test.go` verify correctness. Adding production constants would create dual sources of truth.

4. **New file `facts.go`**: Separates fact application logic from config loading logic, improving cohesion.

5. **Test gap fix**: Add missing `TestApplyToFacts_Anomaly` test.

## Acceptance Criteria

- [x] 3 helper functions: `applyPositive[T]`, `applyNonEmpty`, `applyBool`
- [x] Single `ApplyToFacts` replaces all 7 `applyXxxFacts` functions
- [x] All existing `apply_test.go` tests pass unchanged
- [x] New `TestApplyToFacts_Anomaly` test added
- [x] `go test ./internal/config/...` passes
- [x] `go vet` clean
- [x] `make lint` passes — zero issues, zero dead code
- [x] Fact application behavior identical to original (verified by existing tests)

## Risk

Low. The 3 patterns are mechanical extractions with no behavioral change. The existing test suite covers all 7 analyzer sections (after adding anomaly). The float64→float32 conversion is preserved at the call site. No external API changes — `ApplyToFacts` signature is unchanged.

## Implementation

### Files created

| File | Purpose |
|------|---------|
| `internal/config/facts.go` | Generic helpers (`applyPositive[T]`, `applyNonEmpty`, `applyBool`) and rewritten `ApplyToFacts` method |

### Files modified

| File | Change |
|------|--------|
| `internal/config/loader.go` | Removed `ApplyToFacts` and all 7 `applyXxxFacts` functions (lines 118-209) |
| `internal/config/apply_test.go` | Added `TestApplyToFacts_Anomaly` test and anomaly fact key constants |
