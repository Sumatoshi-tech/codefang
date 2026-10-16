//! `cf-sentiment` — commit-comment sentiment analysis (analyzer id
//! `history/sentiment`).
//!
//! Classifies each new or changed comment per commit as carrying positive or
//! negative emotion, aggregating per-tick sentiment over commit history.
//!
//! # Module map
//!
//! * [`analyzer`] — the deterministic comment extraction / merging / filtering
//!   pipeline.
//! * [`vader`] — the self-contained VADER scoring engine.
//! * [`scorer`] — comment sentiment scoring with multilingual lexicon injection
//!   and SE-domain neutralizers.
//! * [`metrics`] — per-tick metric computation (only ticks with new or
//!   changed comments carry a score).
//! * [`model`] — report-bearing types with [`model::ToGoValue`] for byte-stable
//!   serialization through [`cf_gojson`].
//! * [`store`] — the per-tick series the cross-analyzer anomaly path reads.
//!
//! All report serialization routes through [`cf_gojson`] (and `cf-goyaml` for
//! YAML) via [`model::ToGoValue`] — never raw `serde_json`. The VADER base
//! lexicon and emoji table are vendored under `data/`.

#![forbid(unsafe_code)]
// Deliberate parity casts (usize -> i64 lengths, f64 -> f32 report fields) are
// part of the report contract; the lossy-cast lints would only add noise.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

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
pub use store::extract_store_time_series;
pub use vader::{Sentiment, SentimentIntensityAnalyzer};

pub use analyzer::{new_or_changed, ANALYZER_ID, DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-sentiment";
