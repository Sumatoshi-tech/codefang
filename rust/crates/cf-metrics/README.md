# cf-metrics

Lightweight metrics primitives. Rust port of the Go package `pkg/metrics`.

Provides the building blocks reused across analyzers:

- `Metric` trait — a self-contained computation with metadata (`name`,
  `display_name`, `description`, `metric_type`, `compute`).
- `MetricMeta` — embeddable metadata helper.
- `TimeSeriesPoint` — `{ tick, value }` data point.
- `RiskLevel` + `RISK_CRITICAL`/`RISK_HIGH`/`RISK_MEDIUM`/`RISK_LOW` constants and
  `risk_priority` (sortable priority: `CRITICAL < HIGH < MEDIUM < LOW/unknown`).
- `RiskResult` — `{ value, risk_level, threshold?, message? }` risk metric output.
- `Registry` — name-keyed collection of type-erased metrics.

## Serialization

Report-bearing types (`TimeSeriesPoint`, `RiskResult`) expose
`GoSerialize::to_go_value()`, producing a `GoValue` tree whose object fields are in
Go struct **declaration order** with `omitempty` fields dropped when empty/zero —
the exact shape the `cf-gojson` encoder consumes. Per `specs/rust-rewrite/DESIGN.md`
§2, machine-format bytes (json/yaml/ndjson/timeseries/compact/bin) must be emitted
by the shared `cf-gojson` / `cf-goyaml` crates rather than raw serde defaults.
Because `cf-gojson` is not yet ported, this crate carries **no external
dependencies**: the local `GoValue` enum is a placeholder mirroring the `cf-gojson`
surface and will be replaced by `cf_gojson::GoValue` once that crate is integrated.

## Dependencies

None (pure `std`). When `cf-gojson` lands, add it as a dependency and re-target the
`GoSerialize` impls at `cf_gojson::GoValue`.
