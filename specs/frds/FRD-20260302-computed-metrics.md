# FRD: Consolidate ComputeAllMetrics Pattern (Roadmap F3.1)

**ID**: FRD-20260302-computed-metrics
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F3.1

## Problem

All 14 analyzers implement an identical `ComputeAllMetrics` orchestration pattern:
parse report → create metric instances → compute → collect results. Additionally, each
analyzer defines a `ComputedMetrics` struct with three identical interface methods
(`AnalyzerName`, `ToJSON`, `ToYAML`). This is ~450 lines of duplicated orchestration and
~140 lines of duplicated interface boilerplate. See LIST.md #36.

## Feature

Create a generic `MetricResult` type and `MetricSet` wrapper. `MetricSet` implements the
`metricsSerializer` interface (`ToJSON`, `ToYAML`) and `AnalyzerName()`, eliminating the
per-analyzer boilerplate. `ComputeAllMetrics` is a simple orchestrator that evaluates a
list of computer functions and returns a `*MetricSet`.

Migrate the typos analyzer as proof that the pattern works end-to-end with the existing
`BaseHistoryAnalyzer[M]` serialization chain.

## Acceptance Criteria

- [x] `internal/analyzers/common/computed_metrics.go` exports `MetricResult`, `MetricSet`, `ComputeAllMetrics`
- [x] `internal/analyzers/common/computed_metrics_test.go` has ≥90% coverage
- [x] typos analyzer migrated to use `common.MetricSet` and `common.ComputeAllMetrics`
- [x] All existing tests pass (typos tests updated for new API)
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Low.** The orchestrator is a thin wrapper. The typos migration changes the type parameter
from `*typos.ComputedMetrics` to `*common.MetricSet`, but the JSON/YAML serialization output
is backward-compatible because `MetricSet.ToJSON()` returns a `map[string]any` keyed by
metric name — the same keys that previously came from JSON struct tags.

## Non-Goals

- Migrating all 14 analyzers in this FRD (only typos as proof).
- Changing individual metric computation logic.
- Modifying `BaseHistoryAnalyzer[M]` or `SafeMetricComputer[M]`.
- Adding metric metadata to serialized output (only values are serialized).

## Implementation

### Files Created

- `internal/analyzers/common/computed_metrics.go` — `MetricResult` struct, `MetricSet` struct with `AnalyzerName()`, `ToJSON()`, `ToYAML()`, `Metrics()` accessor, and `ComputeAllMetrics` orchestrator
- `internal/analyzers/common/computed_metrics_test.go` — tests for empty report, single metric, multiple metrics, AnalyzerName, ToJSON/ToYAML serialization, Metrics accessor

### Files Modified

- `internal/analyzers/typos/analyzer.go` — `BaseHistoryAnalyzer[*ComputedMetrics]` → `BaseHistoryAnalyzer[*common.MetricSet]`; `ComputeMetricsFn` uses `common.ComputeAllMetrics` via closure
- `internal/analyzers/typos/metrics.go` — `ComputedMetrics` struct removed along with `AnalyzerName`, `ToJSON`, `ToYAML` methods; `ComputeAllMetrics` rewritten to use `common.ComputeAllMetrics`
- `internal/analyzers/typos/metrics_test.go` — tests updated for new return type (`*common.MetricSet` accessed via `Metrics()`)
- `internal/analyzers/typos/analyzer_test.go` — `ComputedMetrics` replaced with local `serializedMetrics` struct for JSON/YAML deserialization tests
- `internal/analyzers/typos/store_writer_test.go` — `refMetrics.FileTypos`/`refMetrics.Aggregate` replaced with `MetricSet.ToJSON()` extraction
- `internal/analyzers/common/renderer/pipeline_test.go` — `typos.ComputedMetrics` replaced with `common.MetricSet`; added `mustEmptyTyposMetrics()` helper
- `tools/schemagen/schemagen.go` — `typos.ComputedMetrics` replaced with local `typosSchemaType` for reflection-based schema generation

### Verification

- `go vet ./...` — clean
- `go test ./internal/analyzers/common/... ./internal/analyzers/typos/... ./internal/analyzers/common/renderer/...` — all pass
- `make lint` — 0 issues, 0 dead code
