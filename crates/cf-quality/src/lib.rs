//! `cf-quality` — the composite **quality** history analyzer (`history/quality`).
//!
//! Runs four static component analyzers — complexity, Halstead, comments, and
//! cohesion — on each changed file's UAST per commit, recording **scalars
//! only**, and aggregates them **order-independently** (per-commit results
//! keyed by hash; merging is a no-op).
//!
//! # Module map
//!
//! * [`data`] — per-tick / per-commit data containers.
//! * [`analyzer`] — consume/accumulate glue over the component analyzers.
//! * [`metrics`] — per-tick statistics, time-series and aggregate computation.
//! * [`store`] — store record kinds and plot-section titles.
//! * [`serialize`] — report serialization (charts themselves are non-binding
//!   cosmetic output).
//!
//! # Compatibility
//!
//! Every machine-format report is serialized through [`cf_gojson`] /
//! [`cf_goyaml`] and the CFB1 envelope via [`cf_reportutil`] (see
//! [`serialize`]), never serde defaults; the bytes are pinned against the
//! reference binary by `tests/compat`. Wrapper structs
//! ([`metrics::TickStats`], [`metrics::TimeSeriesEntry`],
//! [`metrics::AggregateData`], [`metrics::ComputedMetrics`]) emit fields in
//! declaration order honoring `omitempty`; the per-commit summary map is
//! map-origin and byte-sorts its keys.
//!
//! # Numeric parity
//!
//! All statistics (`mean`/`median`/`P95`/`max`/`min`/`sum`) come from
//! [`cf_alg_stats`], the operation-for-operation statistics kernel shared with
//! the reference implementation, so the values flowing into reports are
//! bit-faithful.
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
