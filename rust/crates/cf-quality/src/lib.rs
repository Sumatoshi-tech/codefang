//! `cf-quality` — Rust port of the Go `internal/analyzers/quality` package.
//!
//! The composite **quality** history analyzer (ID `history/quality`) runs four
//! static component analyzers — complexity, Halstead, comments, and cohesion —
//! on each changed file's UAST per commit, recording **scalars only**, and
//! aggregates them **order-independently** (per-commit results keyed by hash;
//! `Merge` is a no-op). It is ported after its components, per
//! `specs/rust-rewrite/DESIGN.md` §1.1.
//!
//! # Module map (Go file → Rust module)
//!
//! | Go file | Rust module |
//! | --- | --- |
//! | `analyzer.go` (data types) | [`data`] |
//! | `analyzer.go` (Consume/aggregator) | [`analyzer`] |
//! | `metrics.go` | [`metrics`] |
//! | `store_writer.go` / `store_reader.go` | [`store`] |
//! | (serialization) | [`serialize`] |
//! | `plot.go` | non-binding cosmetic (titles in [`store`]); charts not ported |
//!
//! # Byte-identity
//!
//! Every machine-format report is serialized through [`cf_gojson`] / [`cf_goyaml`]
//! and the CFB1 envelope via [`cf_reportutil`] (see [`serialize`]), never serde
//! defaults, per DESIGN §2. Wrapper structs ([`metrics::TickStats`],
//! [`metrics::TimeSeriesEntry`], [`metrics::AggregateData`],
//! [`metrics::ComputedMetrics`]) emit fields in Go declaration order honoring
//! `omitempty`; the per-commit summary map is map-origin and byte-sorts its keys.
//!
//! # Numeric parity
//!
//! All statistics (`mean`/`median`/`P95`/`max`/`min`/`sum`) come from
//! [`cf_alg_stats`], the operation-for-operation port of `pkg/alg/stats`, so the
//! values flowing into reports are bit-faithful to Go.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod analyzer;
pub mod data;
pub mod metrics;
pub mod serialize;
pub mod store;

pub use analyzer::{
    accumulate_file, consume_commit, fold_commits, ComponentSet, ScalarReport, DESCRIPTION,
    ESTIMATED_TC_SIZE, ID,
};
pub use data::{TickData, TickQuality};
pub use metrics::{
    aggregate_commits_to_ticks, commit_summary, compute_all_metrics, compute_tick_stats,
    extract_commit_time_series, parse_report_data, AggregateData, CommitSummary, ComputedMetrics,
    ReportData, TickBounds, TickStats, TimeSeriesEntry,
};
pub use store::{KIND_AGGREGATE, KIND_TIME_SERIES};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-quality";
