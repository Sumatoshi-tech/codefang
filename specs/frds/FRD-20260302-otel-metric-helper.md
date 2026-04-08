# FRD: OTel Metric Creation Helper (Roadmap 5.1)

**ID**: FRD-20260302-otel-metric-helper
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 5.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 9, LIST #27

## Problem

Three metric constructor functions in `internal/observability/` repeat identical boilerplate for every OTel instrument:

```go
counter, err := mt.Int64Counter(metricName,
    metric.WithDescription("..."),
    metric.WithUnit("{unit}"),
)
if err != nil {
    return nil, fmt.Errorf("create %s: %w", metricName, err)
}
```

This 7-line pattern appears **13 times** across 3 files:

| File | Function | Instruments | Lines |
|------|----------|-------------|-------|
| `metrics.go` | `NewREDMetrics` | 4 (2 counters, 1 histogram, 1 up-down counter) | ~28 |
| `analysis_metrics.go` | `NewAnalysisMetrics` | 5 (4 counters, 1 histogram) | ~35 |
| `scheduler_metrics.go` | `NewSchedulerMetrics` | 3 (2 observable gauges, 1 observable counter) | ~21 |

Total: 13 instruments × 7 lines = ~84 lines of boilerplate.

## Solution

Introduce a `metricBuilder` that accumulates the first error, enabling declarative struct initialization with a single error check at the end.

### Builder API

```go
type metricBuilder struct {
    meter metric.Meter
    err   error
}

func newMetricBuilder(mt metric.Meter) *metricBuilder
func (b *metricBuilder) counter(name, desc, unit string) metric.Int64Counter
func (b *metricBuilder) histogram(name, desc, unit string, bounds ...float64) metric.Float64Histogram
func (b *metricBuilder) upDownCounter(name, desc, unit string) metric.Int64UpDownCounter
func (b *metricBuilder) gauge(name, desc, unit string) metric.Int64ObservableGauge
func (b *metricBuilder) observableCounter(name, desc, unit string) metric.Int64ObservableCounter
```

### Rewritten constructors

```go
// Before: 34 lines
func NewREDMetrics(mt metric.Meter) (*REDMetrics, error) {
    reqTotal, err := mt.Int64Counter(metricRequestsTotal, ...)
    if err != nil { return nil, fmt.Errorf("create %s: %w", ...) }
    // ... 3 more blocks ...
    return &REDMetrics{...}, nil
}

// After: 12 lines
func NewREDMetrics(mt metric.Meter) (*REDMetrics, error) {
    b := newMetricBuilder(mt)
    rm := &REDMetrics{
        requestsTotal:    b.counter(metricRequestsTotal, "Total number of requests", "{request}"),
        requestDuration:  b.histogram(metricRequestDuration, "Request duration in seconds", "s", durationBucketBoundaries...),
        errorsTotal:      b.counter(metricErrorsTotal, "Total number of errors", "{error}"),
        inflightRequests: b.upDownCounter(metricInflightRequests, "Number of in-flight requests", "{request}"),
    }
    if b.err != nil {
        return nil, b.err
    }
    return rm, nil
}
```

### Key design decisions

1. **First-error accumulation**: `metricBuilder` stores only the first error. This matches the OTel API guarantee that instruments are safe to use even on creation failure (the SDK returns valid no-op instruments). Subsequent builder calls are skipped when `b.err` is non-nil to avoid masking the root cause.

2. **Unexported builder**: `metricBuilder` is package-private — it's an implementation detail of the `observability` package, not a public API.

3. **Variadic bounds for histograms**: The `histogram` method accepts optional `bounds ...float64`, defaulting to no explicit boundaries. This cleanly handles both histogram configurations (`durationBucketBoundaries` and default SDK bounds).

4. **No `must` pattern**: The builder preserves error propagation (no panics). Callers continue to get `error` returns, maintaining compatibility with the existing public API.

## Acceptance Criteria

- [x] `metricBuilder` type with 5 instrument methods in `internal/observability/metric_builder.go`
- [x] `NewREDMetrics` rewritten using builder (~22 lines reduced)
- [x] `NewAnalysisMetrics` rewritten using builder (~22 lines reduced)
- [x] `NewSchedulerMetrics` rewritten using builder (~10 lines reduced)
- [x] All existing tests pass unchanged (`go test ./internal/observability/...`)
- [x] New unit tests for `metricBuilder` covering error accumulation and all instrument types
- [x] `go vet` clean
- [x] `make lint` passes — zero issues, zero dead code

## Risk

Low. The refactoring is mechanical — each instrument creation maps 1:1 to a builder method call. The OTel API guarantees valid no-op instruments on error, so the first-error-wins pattern is safe. All existing tests exercise the full constructor paths and verify metric recording behavior.

## Implementation

| Action | File |
|--------|------|
| Created | `internal/observability/metric_builder.go` — `metricBuilder` with 5 instrument methods and first-error accumulation |
| Created | `internal/observability/metric_builder_test.go` — 10 white-box tests covering all instrument types and error behavior |
| Modified | `internal/observability/metrics.go` — `NewREDMetrics` rewritten with builder, removed `fmt` import |
| Modified | `internal/observability/analysis_metrics.go` — `NewAnalysisMetrics` rewritten with builder, removed `fmt` import |
| Modified | `internal/observability/scheduler_metrics.go` — `NewSchedulerMetrics` rewritten with builder (kept `fmt` for `RegisterCallback` error) |
