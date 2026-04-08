# FRD: ThresholdLabeler in internal/analyzers/common (Roadmap 2.1)

**ID**: FRD-20260306-threshold-labeler
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 2.1
**Date**: 2026-03-06

## Problem

Multiple analyzers duplicate a `float64 → string` message builder using identical
`if/else` or `switch` chains with 2–4 threshold comparisons. The structures are:

| Analyzer | Function | Pattern | Thresholds |
|----------|----------|---------|------------|
| `cohesion/aggregator.go` | `getCohesionMessage` | `>=` desc | 0.7, 0.4, 0.3 |
| `cohesion/cohesion.go` | `getCohesionMessage` (method) | `>=` desc | 0.7, 0.4, 0.3 (duplicate!) |
| `comments/aggregator.go` | `buildMessage` | `>=` desc | 0.8, 0.6, 0.4 |
| `comments/comments.go` | `getCommentMessage` (method) | `>=` desc | 0.8, 0.6, 0.4 (duplicate!) |
| `halstead/aggregator.go` | `buildHalsteadMessage` | `>=` desc | 5000, 1000, 100 |

**DoR findings:**

- `complexity/aggregator.go::buildComplexityMessage` uses `<= ascending` pattern
  (lower complexity = better). This is the inverse of `ThresholdLabeler`'s `>=`
  semantics and is **out of scope** — migrating it would require negating the input
  value, which reduces clarity.
- `sentiment` has no aggregator `buildMessage` function (no `aggregator.go`).
  The `sentimentLabel`/`classifySentiment` functions use a 3-zone `>=`/`<=` pattern
  (positive/neutral/negative) which does not map to a single-direction threshold chain.
  Both are **out of scope**.

**Note on `common.Classifier[T]`:** A generic `Classifier[T cmp.Ordered]` already
exists in this package with `>=` semantics and auto-sorting. `ThresholdLabeler`
is a thin slice alias over the existing `Threshold[float64]` type (field `Limit`),
calls `Label()` (vs `Classify()`), and requires no constructor. The slice literal
syntax is more natural for static configurations embedded in aggregator constructors.

## Decision

Create `ThresholdLabeler` as a slice type alias over the existing `Threshold[float64]`:

```go
// ThresholdLabeler maps a float64 score to a string label using an ordered list
// of Threshold[float64] values. Thresholds must be sorted descending by Limit
// (highest first) — the first threshold where score >= Limit wins.
// A catch-all fallback: {Limit: 0, Label: "..."} matches any score >= 0.
type ThresholdLabeler []Threshold[float64]

func (l ThresholdLabeler) Label(score float64) string
```

Migrate the 5 functions (3 unique + 2 duplicates) to `ThresholdLabeler`.

## `ThresholdLabeler` contract

- `ThresholdLabeler(nil).Label(x)` → `""`
- Thresholds checked in order; first match wins
- Caller is responsible for descending sort by `Limit`
- `{Limit: 0, Label: "..."}` acts as catch-all fallback for scores in [0, ∞)

## Scope

### Files modified

| File | Change |
|------|--------|
| `internal/analyzers/common/threshold_labeler.go` | New: `Threshold`, `ThresholdLabeler` |
| `internal/analyzers/common/threshold_labeler_test.go` | New: unit tests |
| `internal/analyzers/cohesion/aggregator.go` | Replace `getCohesionMessage` body |
| `internal/analyzers/cohesion/cohesion.go` | Replace `getCohesionMessage` method body |
| `internal/analyzers/comments/aggregator.go` | Replace `buildMessage` body |
| `internal/analyzers/comments/comments.go` | Replace `getCommentMessage` method body |
| `internal/analyzers/halstead/aggregator.go` | Replace `buildHalsteadMessage` body |
| `specs/ref/ROADMAP.md` | Mark 2.1 done |

### Out of scope

- `complexity/aggregator.go::buildComplexityMessage` — `<=` ascending; excluded
- `complexity/complexity.go::getComplexityMessage` — same
- `sentiment` — no aggregator buildMessage; `sentimentLabel`/`classifySentiment` use 3-zone pattern

## Acceptance Criteria

- [x] `ThresholdLabeler` and `Threshold` types in `common` package
- [x] `Label(score)` returns first matching label (descending order); `""` for no match
- [x] nil/empty slice → `""` (no panic)
- [x] 5 `buildMessage`/`getXxxMessage` function bodies replaced
- [x] `sort` not imported (no new dependencies)
- [x] `make test ./internal/analyzers/common/...` passes ≥90% coverage
- [x] `make test ./internal/analyzers/{cohesion,comments,halstead}/...` passes
- [x] `make lint` passes — 0 issues, no dead code

## Risk

Low.
- `ThresholdLabeler.Label()` is pure data-driven; each `if/else` chain is preserved
  verbatim as threshold values — no logic change
- The 5 replaced functions all keep the same label strings; only the dispatch
  mechanism changes
- `nil` slice returns `""` — callers never use the return value for nil input
  (score > 0 always produces non-empty output from the threshold lists used)

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `internal/analyzers/common/threshold_labeler.go` | New |
| `internal/analyzers/common/threshold_labeler_test.go` | New |
| `internal/analyzers/cohesion/aggregator.go` | Replace getCohesionMessage |
| `internal/analyzers/cohesion/cohesion.go` | Replace getCohesionMessage method |
| `internal/analyzers/comments/aggregator.go` | Replace buildMessage |
| `internal/analyzers/comments/comments.go` | Replace getCommentMessage method |
| `internal/analyzers/halstead/aggregator.go` | Replace buildHalsteadMessage |
| `specs/ref/ROADMAP.md` | Marked 2.1 done |
