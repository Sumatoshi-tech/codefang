//! `cf-quality` — the composite **quality** history analyzer (`history/quality`).
//!
//! Runs the four static component analyzers — complexity, Halstead, comments,
//! and cohesion — on each analyzed file's UAST per commit, recording one
//! scalar sample per file ([`analyzer::analyze_file`]), and aggregates the
//! samples per tick ([`metrics::compute_all_metrics`]).
//!
//! # Module map
//!
//! * [`analyzer`] — the per-file sample over the four component analyzers.
//! * [`data`] — the per-tick sample container.
//! * [`metrics`] — per-tick statistics, time-series and aggregate computation.
//! * [`serialize`] — report serialization through [`cf_gojson`] /
//!   [`cf_reportutil`] (charts themselves are non-binding cosmetic output).
//!
//! # Which files count
//!
//! A file contributes a sample when its UAST parse succeeded; a failed parse
//! contributes nothing. A parsed file without functions contributes its
//! complexity / Halstead / comment scalars (zeros for the function-based
//! ones) but NO cohesion sample — the static cohesion analyzer treats such a
//! file as "no data", and so does quality. A tick in which no file was
//! analyzed is absent from the time series.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod analyzer;
pub mod data;
pub mod metrics;
pub mod serialize;

pub use analyzer::{analyze_file, FileSample, FIND_MAX_DEPTH};
pub use data::TickQuality;
pub use metrics::{
    commit_summary, compute_all_metrics, compute_tick_stats, AggregateData, CommitSummary,
    ComputedMetrics, ReportData, TickBounds, TickStats, TimeSeriesEntry,
};
