//! `cf-anomaly` — temporal anomaly detection over commit history.
//!
//! Implements the `history/anomaly` analyzer. It detects sudden quality
//! degradation in commit history using Z-score analysis over a trailing
//! sliding window of per-tick metrics (files changed, lines added/removed,
//! net churn, language diversity, author count). It is also used by the
//! `quality` and `sentiment` analyzers via the cross-analyzer enrichment path
//! ([`enrich`]).
//!
//! # Byte-identity
//!
//! Every report-bearing type implements [`model::ToGoValue`], producing a
//! [`cf_gojson::GoValue`] tree that is serialized through
//! [`cf_gojson::Encoder`] so the machine formats (`json`, `yaml`, `ndjson`,
//! `timeseries`, `compact`, `bin`) follow the report-format contract (pinned
//! against the reference implementation by `rust/tests/compat`). Struct types
//! serialize fields in declaration order with omit-when-empty; the dynamic
//! `languages` / `commit_metrics` maps byte-sort their keys.
//!
//! # Algorithm
//!
//! 1. Per-commit metrics are collected ([`model::CommitAnomalyData`]).
//! 2. Commits are aggregated into ticks ([`aggregate::aggregate_commits_to_ticks`]).
//! 3. Trailing-window Z-scores are computed per metric ([`zscore::compute_z_scores`]).
//! 4. A tick is flagged when any metric's `|Z|` exceeds the threshold
//!    ([`detect::detect_anomalies_from_ticks`]); ties resolve by descending
//!    severity.
//! 5. Aggregate / time-series outputs are produced ([`metrics::compute_all_metrics`]).
//!
//! When a trailing window has zero variance and the current value differs, a
//! sentinel Z-score of `100.0` ([`cf_alg_stats::ZSCORE_MAX_SENTINEL`]) is
//! assigned (reference-implementation behavior).

pub mod aggregate;
pub mod config;
pub mod detect;
pub mod enrich;
pub mod metrics;
pub mod model;
pub mod store;
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
