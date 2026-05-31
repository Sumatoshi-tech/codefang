//! Anomaly detection over commit metrics.
//!
//! Port of `DetectAnomalies` in `internal/analyzers/anomaly/detect.go`. Computes
//! rolling Z-scores of per-commit churn and flags commits whose absolute
//! Z-score is at or above the configured threshold.

use crate::model::{Anomaly, AnomalyConfig, AnomalyReport, CommitMetric};
use crate::zscore::rolling_zscores;

/// Default trailing-window length when the config leaves it unset (`<= 0`).
///
/// Mirrors Go's `defaultWindow` (analyzer.go) of 30 commits.
const DEFAULT_WINDOW: usize = 30;

/// Detects anomalous commits by trailing-window rolling Z-score of churn.
///
/// Mirrors Go's `DetectAnomalies`:
/// 1. empty input → empty report;
/// 2. sort a copy of `metrics` by ascending timestamp;
/// 3. compute rolling Z-scores of the churn values over `cfg.window`
///    (falling back to [`DEFAULT_WINDOW`] when unset);
/// 4. flag every commit whose `|z| >= cfg.threshold`, in timestamp order.
#[must_use]
pub fn detect_anomalies(metrics: &[CommitMetric], cfg: &AnomalyConfig) -> AnomalyReport {
    if metrics.is_empty() {
        return AnomalyReport {
            anomalies: Vec::new(),
        };
    }

    let mut sorted: Vec<CommitMetric> = metrics.to_vec();
    // Go uses sort.Slice on the timestamp; for unique timestamps the ordering is
    // identical to this stable sort. (Ties are rare in the bounded golden runs;
    // see ROADMAP risk notes for the sort.Slice-vs-stable parity caveat.)
    sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    let values: Vec<f64> = sorted.iter().map(|m| m.churn as f64).collect();

    let window = if cfg.window == 0 {
        DEFAULT_WINDOW
    } else {
        cfg.window
    };
    let zscores = rolling_zscores(&values, window);

    let mut anomalies: Vec<Anomaly> = Vec::new();
    for (i, &z) in zscores.iter().enumerate() {
        if z.abs() >= cfg.threshold {
            anomalies.push(Anomaly {
                commit_hash: sorted[i].commit_hash.clone(),
                churn: sorted[i].churn,
                z_score: z,
                timestamp: sorted[i].timestamp,
            });
        }
    }

    AnomalyReport { anomalies }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric(hash: &str, churn: i64, ts: i64) -> CommitMetric {
        CommitMetric {
            commit_hash: hash.to_string(),
            churn,
            timestamp: ts,
        }
    }

    #[test]
    fn empty_input_empty_report() {
        let cfg = AnomalyConfig {
            window: 30,
            threshold: 2.0,
        };
        assert!(detect_anomalies(&[], &cfg).anomalies.is_empty());
    }

    #[test]
    fn flags_high_threshold_spike() {
        let metrics = vec![
            metric("a", 1, 1),
            metric("b", 1, 2),
            metric("c", 1, 3),
            metric("d", 100, 4),
        ];
        let cfg = AnomalyConfig {
            window: 4,
            threshold: 1.0,
        };
        let report = detect_anomalies(&metrics, &cfg);
        assert_eq!(report.anomalies.len(), 1);
        assert_eq!(report.anomalies[0].commit_hash, "d");
        assert_eq!(report.anomalies[0].churn, 100);
    }

    #[test]
    fn sorts_by_timestamp_before_scoring() {
        // Provide out-of-order timestamps; the spike must still be detected.
        let metrics = vec![
            metric("d", 100, 4),
            metric("a", 1, 1),
            metric("c", 1, 3),
            metric("b", 1, 2),
        ];
        let cfg = AnomalyConfig {
            window: 4,
            threshold: 1.0,
        };
        let report = detect_anomalies(&metrics, &cfg);
        assert_eq!(report.anomalies.len(), 1);
        assert_eq!(report.anomalies[0].commit_hash, "d");
    }
}
