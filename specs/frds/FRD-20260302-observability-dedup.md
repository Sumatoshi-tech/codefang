# FRD: Deduplicate Observability Metric Creation (Roadmap F2.1)

**ID**: FRD-20260302-observability-dedup
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F2.1

## Problem

Three duplication patterns remain in `internal/observability/`:

| Location | Duplication | Source |
|----------|-------------|--------|
| `metric_builder.go` | 5 methods (`counter`, `histogram`, `upDownCounter`, `gauge`, `observableCounter`) all follow identical `call → setErr → return` pattern | LIST.md #22 |
| `init.go` | `buildTracerProvider` and `buildMeterProvider` construct OTLP gRPC options identically (~10 lines each) | LIST.md #23 |
| `metrics.go`, `analysis_metrics.go`, `scheduler_metrics.go` | All 3 `New*` factories repeat `newMetricBuilder(mt)` → create → `if b.err != nil` boilerplate | LIST.md #21 |

## Feature

### 1. Generic `createMetric[T]`

Replace 5 builder methods with a single generic function:

```go
func createMetric[T any](b *metricBuilder, name string, fn func() (T, error)) T
```

Each caller passes a closure that invokes the appropriate `metric.Meter` method. The generic function handles error accumulation via `setErr`.

### 2. Generic `buildMetrics[T]` factory helper

Extract the repeated `newMetricBuilder → build → error check` pattern:

```go
func buildMetrics[T any](mt metric.Meter, fn func(*metricBuilder) *T) (*T, error)
```

All 3 `New*` functions delegate to `buildMetrics`. `NewSchedulerMetrics` performs its additional `RegisterCallback` after the `buildMetrics` call.

### 3. Generic `buildOTLPOptions[T]`

Extract the duplicated OTLP option construction:

```go
func buildOTLPOptions[T any](cfg Config, withEndpoint func(string) T, withInsecure func() T, withHeaders func(map[string]string) T) []T
```

Called by `buildTracerProvider` with `otlptracegrpc.*` functions and by `buildMeterProvider` with `otlpmetricgrpc.*` functions.

## Acceptance Criteria

- [x] `metric_builder.go` — 5 methods replaced by single `createMetric[T any]` generic function
- [x] `metric_builder.go` — `buildMetrics[T any]` factory helper added
- [x] `init.go` — `buildOTLPOptions[T any]` extracted; both `buildTracerProvider` and `buildMeterProvider` use it
- [x] `metrics.go` — `NewREDMetrics` uses `buildMetrics` + `createMetric`
- [x] `analysis_metrics.go` — `NewAnalysisMetrics` uses `buildMetrics` + `createMetric`
- [x] `scheduler_metrics.go` — `NewSchedulerMetrics` uses `createMetric` + `buildMetrics`
- [x] All existing observability tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Low.** All changes are behavior-preserving:
- `createMetric[T]` encapsulates the exact same `call → setErr → return` pattern.
- `buildMetrics[T]` encapsulates the exact same `newMetricBuilder → fn → error check` pattern.
- `buildOTLPOptions[T]` produces the same option slices — only import paths change.

## Non-Goals

- Changing metric names, descriptions, or units.
- Modifying `RecordRequest`, `RecordRun`, `TrackInflight`, or `observe` methods.
- Changing the `AnalysisStats` struct or any domain logic.
- Adding new metrics or removing existing ones.

## Implementation

### Files Modified

- `internal/observability/metric_builder.go` — removed 5 methods (`counter`, `histogram`, `upDownCounter`, `gauge`, `observableCounter`); added `createMetric[T any]` generic function and `buildMetrics[T any]` factory helper
- `internal/observability/init.go` — added `buildOTLPOptions[T any]`; simplified `buildTracerProvider` and `buildMeterProvider` to use it
- `internal/observability/metrics.go` — `NewREDMetrics` uses `buildMetrics` + `createMetric` closures
- `internal/observability/analysis_metrics.go` — `NewAnalysisMetrics` uses `buildMetrics` + `createMetric` closures
- `internal/observability/scheduler_metrics.go` — `NewSchedulerMetrics` uses `buildMetrics` + `createMetric` closures
- `internal/observability/metric_builder_test.go` — tests updated for `createMetric`/`buildMetrics` API; added `TestBuildMetrics_Success` and `TestBuildMetrics_PropagatesError`

### Lines Eliminated

~45 lines of duplicated method bodies in `metric_builder.go`, ~10 lines of duplicated OTLP option construction in `init.go`, ~6 lines of boilerplate per `New*` factory.

### Verification

- `go vet ./...` — clean
- `go test ./internal/observability/...` — all pass
- `make lint` — 0 issues, 0 dead code
