//! `cf-sentiment` — commit-comment sentiment analysis (Go id `history/sentiment`).
//!
//! Rust port of `internal/analyzers/sentiment`. Classifies each new or changed
//! comment per commit as carrying positive or negative emotion, aggregating
//! per-tick sentiment over commit history. See `specs/rust-rewrite/DESIGN.md` §1.
//!
//! # Module map
//!
//! * [`analyzer`] — the deterministic comment extraction / merging / filtering
//!   pipeline (pure portion of `analyzer.go`).
//! * [`vader`] — a self-contained port of `github.com/jonreiter/govader`
//!   (VADER, commit `f6505c8d`). See the module docs for why this lives here
//!   rather than depending on `cf-govader` (which is an incomplete,
//!   workspace-excluded scaffold at the time of this port).
//! * [`scorer`] — comment sentiment scoring with multilingual lexicon injection
//!   and SE-domain neutralizers (`scorer.go`).
//! * [`metrics`] — per-tick metric computation (`metrics.go`).
//! * [`model`] — report-bearing types with [`model::ToGoValue`] for byte-identical
//!   serialization through [`cf_gojson`] (`metrics.go` output structs).
//! * [`store`] — store record kinds + store-time-series extraction
//!   (`store_writer.go` / `store_reader.go`).
//!
//! # Byte-identity (DESIGN §2)
//!
//! All report serialization routes through [`cf_gojson`] (and, once it lands,
//! `cf-goyaml`) via [`model::ToGoValue`] — never raw `serde_json`. VADER scores
//! and the multilingual lexicon are reproduced exactly (DESIGN §2.6, rule 7),
//! with the base lexicon vendored byte-for-byte under `data/`.

#![forbid(unsafe_code)]
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

pub mod analyzer;
pub mod metrics;
pub mod model;
pub mod scorer;
pub mod store;
pub mod vader;

// Re-export the most commonly used surface at the crate root.
pub use metrics::{
    aggregate_commits_to_ticks, compute_all_metrics, compute_all_metrics_with_options,
    MetricOptions, ReportData, TickBounds, DIM_SENTIMENT, SENTIMENT_NEGATIVE_THRESHOLD,
    SENTIMENT_POSITIVE_THRESHOLD,
};
pub use model::{
    AggregateData, ComputedMetrics, LowSentimentPeriodData, TimeSeriesData, ToGoValue, TrendData,
};
pub use scorer::{compute_sentiment, compute_sentiment_with_options, ScorerOptions};
pub use store::{KIND_AGGREGATE, KIND_TIME_SERIES, KIND_TREND};
pub use vader::{Sentiment, SentimentIntensityAnalyzer};

pub use analyzer::ANALYZER_ID;

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-sentiment";
