# FRD: Create pkg/alg/stats with Core Statistics (Roadmap F0.2)

**ID**: FRD-20260302-stats-package
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.2
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 2: Statistics & Numerical Algorithms

## Problem

Statistical functions are duplicated across four locations:

1. **`anomaly/zscore.go:60-86`** — `MeanStdDev`, `ComputeZScores` (population stddev)
2. **`quality/metrics.go:316-433`** — `meanFloat`, `meanStdDev`, `percentileFloat`, `medianFloat`, `p95Float`, `maxFloat`, `minFloat`, `sumFloat`
3. **`cohesion/plot.go:448-473`** — `percentile`, `median` (assumes sorted input)
4. **`cohesion/calculations.go:233-236`** — `clamp01`
5. **`streaming/planner.go:200-220`** — `emaGrowthRate` struct with `Update`

~150 lines of duplicated statistical logic.

## Feature

Create `pkg/alg/stats` as the canonical statistics package.

### stats.go — Core Statistics

| Function | Signature | Behavior |
|----------|-----------|----------|
| `Mean` | `Mean(values []float64) float64` | Arithmetic mean; returns 0 for empty |
| `MeanStdDev` | `MeanStdDev(values []float64) (mean, stddev float64)` | Combined mean + population stddev |
| `Median` | `Median(values []float64) float64` | 50th percentile via `Percentile` |
| `Percentile` | `Percentile(values []float64, p float64) float64` | Linear interpolation; sorts a copy |
| `Clamp` | `Clamp[T cmp.Ordered](val, lo, hi T) T` | Generic clamp to [lo, hi] |
| `Min` | `Min[T cmp.Ordered](values []T) T` | Minimum; returns zero-value for empty |
| `Max` | `Max[T cmp.Ordered](values []T) T` | Maximum; returns zero-value for empty |
| `Sum` | `Sum[T cmp.Ordered](values []T) T` | Sum; returns zero-value for empty |

> **Note:** `StdDev` and `ZScores` were designed but not shipped — no production callers existed beyond `anomaly.ComputeZScores` which has different window semantics (exclusive/lookback). YAGNI applied.

### ema.go — Exponential Moving Average

| Type/Method | Signature | Behavior |
|------------|-----------|----------|
| `EMA` | `type EMA struct` | Holds state: value, initialized, alpha |
| `NewEMA` | `NewEMA(alpha float64) *EMA` | Constructor; alpha in (0, 1] |
| `Update` | `(e *EMA) Update(v float64) float64` | EMA step; first call initializes |
| `Value` | `(e *EMA) Value() float64` | Current EMA value |
| `Initialized` | `(e *EMA) Initialized() bool` | Whether Update has been called |

### Constants

| Name | Value | Purpose |
|------|-------|---------|
| `ZScoreMaxSentinel` | `100.0` | Cap for z-score when stddev = 0 |
| `PercentileMedian` | `0.5` | 50th percentile |
| `PercentileP95` | `0.95` | 95th percentile |

## Acceptance Criteria

- [x] `pkg/alg/stats/stats.go` exports all functions above
- [x] `pkg/alg/stats/ema.go` exports EMA type
- [x] `pkg/alg/stats/stats_test.go` covers: empty slices, single element, known statistical values, percentile boundaries
- [x] `pkg/alg/stats/ema_test.go` covers: first update, convergence, alpha=1 tracks exactly, Initialized
- [x] `go vet` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Design Decisions

- **Population stddev** (÷n, not ÷(n−1)): matches all existing implementations.
- **Percentile sorts a copy**: callers don't need to pre-sort. The cohesion/plot.go variant assumed sorted input; the consolidated version handles that internally.
- **EMA alpha at construction**: cleaner API than passing alpha on every Update call.
- **Generic Min/Max/Sum**: use `cmp.Ordered` constraint for broad applicability.

## Risk

Low. New package, no existing callers change. All functions are pure and stateless (except EMA which is trivially stateful).

## Implementation

### Files Created

| File | Description |
|------|-------------|
| `pkg/alg/stats/stats.go` | Core statistics: Mean, MeanStdDev, Percentile, Median, Clamp, Min, Max, Sum |
| `pkg/alg/stats/ema.go` | Exponential Moving Average: EMA struct, NewEMA, Update, Value, Initialized |
| `pkg/alg/stats/stats_test.go` | 11 test functions covering all stats.go exports |
| `pkg/alg/stats/ema_test.go` | 6 test functions covering EMA lifecycle and convergence |

### Files Modified (caller wiring — F1.2 done in same pass)

| File | Change |
|------|--------|
| `internal/analyzers/quality/metrics.go` | Replaced ~145 lines of local stat helpers with `stats.*` calls |
| `internal/analyzers/quality/analyzer.go` | Replaced `medianFloat` with `stats.Median` |
| `internal/analyzers/quality/analyzer_test.go` | Replaced local helper calls with `stats.*` equivalents |
| `internal/analyzers/anomaly/zscore.go` | Delegates to `stats.MeanStdDev`, uses `stats.ZScoreMaxSentinel` |
| `internal/analyzers/anomaly/zscore_test.go` | Uses `stats.ZScoreMaxSentinel` |
| `internal/analyzers/cohesion/calculations.go` | Replaced `clamp01` with `stats.Clamp` |
| `internal/analyzers/cohesion/plot.go` | Replaced local `percentile`/`median` with `stats.Percentile`/`stats.Median` |
| `internal/analyzers/cohesion/plot_test.go` | Uses `stats.Percentile` |
| `internal/streaming/planner.go` | Replaced `emaGrowthRate` struct with `*stats.EMA` |
| `internal/streaming/planner_test.go` | Uses `stats.NewEMA` |
