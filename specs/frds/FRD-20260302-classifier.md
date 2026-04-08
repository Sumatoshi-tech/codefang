# FRD: Create threshold Classifier[T] utility (Roadmap F0.6)

**ID**: FRD-20260302-classifier
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.6
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster: Classification Utilities

## Problem

6+ analyzer functions implement the same "check value against descending thresholds, return first matching label" pattern. Each is a hand-rolled if/switch chain with hard-coded thresholds:

- `clones/report.go:classifyCloneType(float64)` — 3 thresholds
- `shotness/metrics.go:classifyChangeRisk(int)` — 2 thresholds
- `cohesion/metrics.go:classifyCohesionQuality(float64)` — 4 thresholds
- `halstead/metrics.go:classifyVolumeLevel(float64)` — 4 thresholds
- `couples/report_section.go:categorizeStrength` — 4 thresholds (inner loop)
- `cohesion/report_section.go:severityForCohesion(float64)` — 3 thresholds
- `complexity/report_section.go:severityForComplexity(int)` — 3 thresholds
- `halstead/report_section.go:severityForFunction(float64)` — 3 thresholds
- `couples/report_section.go:severityForStrength(float64)` — 3 thresholds

## Feature

Create a generic `Classifier[T cmp.Ordered]` in `internal/analyzers/common/classify.go`.

### classify.go — Threshold Classifier

| Export | Signature | Behavior |
|--------|-----------|----------|
| `Threshold[T]` | `struct { Limit T; Label string }` | A single threshold boundary |
| `Classifier[T]` | `struct` (unexported fields) | Immutable threshold classifier |
| `NewClassifier[T]` | `func(thresholds []Threshold[T], defaultLabel string) Classifier[T]` | Creates classifier; sorts thresholds descending by Limit |
| `(c Classifier[T]) Classify` | `func(value T) string` | Returns label for first threshold where `value >= Limit`, or default |

### Algorithm

`NewClassifier` copies and sorts thresholds in **descending** order by `Limit`. `Classify` iterates sorted thresholds and returns the label of the first threshold where `value >= threshold.Limit`. If no threshold matches, returns the default label.

### Design Decisions

- **Descending >= semantics**: Matches all existing classification patterns (highest threshold first).
- **Constructor sorts**: Caller can provide thresholds in any order; the constructor normalizes.
- **Immutable after construction**: Safe for concurrent use without synchronization.
- **No new dependencies**: Pure stdlib (`cmp`, `slices`).

## Acceptance Criteria

- [x] `internal/analyzers/common/classify.go` exports: `Threshold[T]`, `Classifier[T]`, `NewClassifier[T]`
- [x] `internal/analyzers/common/classify_test.go` covers: empty thresholds, boundary values, below-all, above-all, exact match, unsorted input, int and float64 types
- [x] All tests pass, ≥95% statement coverage
- [x] `go vet` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created:**
- `internal/analyzers/common/classify.go` — `Threshold[T]`, `Classifier[T]`, `NewClassifier[T]`
- `internal/analyzers/common/classify_test.go` — 12 tests, 100% coverage

**Files modified (F1.6 wiring):**
- `internal/analyzers/clones/report.go` — `classifyCloneType` → `cloneTypeClassifier.Classify`
- `internal/analyzers/shotness/metrics.go` — `classifyChangeRisk` → `changeRiskClassifier.Classify`
- `internal/analyzers/cohesion/metrics.go` — `classifyCohesionQuality` → `cohesionQualityClassifier.Classify`
- `internal/analyzers/halstead/metrics.go` — `classifyVolumeLevel` → `volumeLevelClassifier.Classify`
