# cf-metrics

Lightweight metrics primitives.

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
`GoSerialize::to_go_value()`, producing a `cf_gojson::GoValue` tree whose object
fields are in struct **declaration order** with `omitempty` fields dropped when
empty/zero — the exact shape the `cf-gojson` encoder consumes. Per
`specs/rust-rewrite/DESIGN.md` §2, machine-format bytes
(json/yaml/ndjson/timeseries/compact/bin) must be emitted by the shared
`cf-gojson` / `cf-goyaml` crates rather than raw serde defaults.

Compatibility: output bytes are pinned against the reference implementation by
`tests/compat`.

## Dependencies

`cf-gojson` only (no serde).
