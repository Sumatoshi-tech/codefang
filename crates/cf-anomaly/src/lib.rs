//! `cf-anomaly` — temporal anomaly detection over commit history.
//!
//! Implements the `history/anomaly` analyzer. It detects sudden quality
//! degradation in commit history using Z-score analysis over a trailing
//! sliding window of per-tick metrics (files changed, lines added/removed,
//! net churn, language diversity, author count). It is also used by the
//! `quality` and `sentiment` analyzers via the cross-analyzer enrichment path
//! ([`enrich`]).
//!
//! # Serialization
//!
//! Every report-bearing type implements [`model::ToGoValue`], producing ONE
//! [`cf_gojson::GoValue`] tree that the machine formats (`json`, `yaml`,
//! `bin`) encode through `cf-gojson` / `cf-goyaml`. Struct types serialize
//! fields in declaration order with omit-when-empty; the dynamic `languages`
//! map byte-sorts its keys; every list is present (`[]` when empty).
//!
//! # Algorithm
//!
//! 1. Per-commit metrics are collected ([`model::CommitAnomalyData`]).
//! 2. Commits are aggregated into ticks
//!    ([`aggregate::aggregate_commits_to_ticks`]): lines add up, files /
//!    languages / authors merge as sets, so `files_changed` is the number
//!    of DISTINCT files the tick touched.
//! 3. Trailing-window Z-scores are computed per metric
//!    ([`zscore::compute_z_scores`]) over the previous `window_size` ticks
//!    (fewer at the start of a history), with at least two observations.
//! 4. A tick is flagged when any metric's `|Z|` strictly exceeds the
//!    threshold ([`detect::detect_anomalies_from_ticks`]); the list is sorted
//!    by descending severity, then tick.
//! 5. Aggregate / time-series outputs are produced
//!    ([`metrics::compute_all_metrics`]).
//!
//! When a trailing window of two or more observations has zero variance and
//! the current value differs, the signed sentinel Z-score `±100.0`
//! ([`zscore::Z_SCORE_SENTINEL`]) is assigned. The threshold and the window
//! come from the validated [`config::Config`].

pub mod aggregate;
pub mod config;
pub mod detect;
pub mod enrich;
pub mod metrics;
pub mod model;
pub mod zscore;

pub use enrich::ANALYZER_NAME_ANOMALY;

/// Analyzer id.
pub const ANALYZER_ID: &str = "history/anomaly";

/// Human analyzer name.
pub const ANALYZER_NAME: &str = "TemporalAnomaly";

/// Analyzer description.
pub const ANALYZER_DESCRIPTION: &str =
    "Detects sudden quality degradation in commit history using Z-score anomaly detection.";

/// Estimated bytes of transfer payload per commit (memory budgeting hint).
pub const ANOMALY_AVG_TC_SIZE: i64 = 200;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_constants() {
        assert_eq!(ANALYZER_ID, "history/anomaly");
        assert_eq!(ANALYZER_NAME, "TemporalAnomaly");
        assert_eq!(ANALYZER_NAME_ANOMALY, "anomaly");
        assert_eq!(ANOMALY_AVG_TC_SIZE, 200);
    }
}
