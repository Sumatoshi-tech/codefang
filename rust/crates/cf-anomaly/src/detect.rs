//! Anomaly detection over per-tick and external time-series data.
//!
//! Ports `buildRecords` / `detectAnomaliesFromTicks` from
//! `internal/analyzers/anomaly/analyzer.go` and `detectExternalAnomalies` from
//! `internal/analyzers/anomaly/enrich.go`.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use cf_alg_stats::mean_std_dev;

use crate::model::{ExternalAnomaly, ExternalSummary, RawMetrics, Record, TickMetrics, ZScoreSet};
use crate::zscore::compute_z_scores;

/// Detects anomalies across all six per-tick metrics.
///
/// Mirrors Go `detectAnomaliesFromTicks`: it computes independent trailing-window
/// Z-scores for net churn, files changed, lines added, lines removed, language
/// diversity, and author count, flags any tick whose maximum absolute Z-score
/// strictly exceeds `threshold`, and sorts the result by descending severity.
///
/// `tick_metrics` keys are consumed in ascending order (Go `mapx.SortedKeys`).
/// The `threshold` is a Go `float32` widened to `f64` for the comparison, exactly
/// as Go does (`thresholdF := float64(threshold)`).
#[must_use]
pub fn detect_anomalies_from_ticks(
    tick_metrics: &BTreeMap<i64, TickMetrics>,
    threshold: f32,
    window: usize,
) -> Vec<Record> {
    let ticks: Vec<i64> = tick_metrics.keys().copied().collect();
    if ticks.is_empty() {
        return Vec::new();
    }

    let n = ticks.len();
    let mut churn = vec![0.0_f64; n];
    let mut files = vec![0.0_f64; n];
    let mut added = vec![0.0_f64; n];
    let mut removed = vec![0.0_f64; n];
    let mut lang_div = vec![0.0_f64; n];
    let mut authors = vec![0.0_f64; n];

    for (i, tick) in ticks.iter().enumerate() {
        let tm = &tick_metrics[tick];
        churn[i] = tm.net_churn as f64;
        files[i] = tm.files_changed as f64;
        added[i] = tm.lines_added as f64;
        removed[i] = tm.lines_removed as f64;
        lang_div[i] = tm.languages.len() as f64;
        authors[i] = tm.author_ids.len() as f64;
    }

    let churn_scores = compute_z_scores(&churn, window);
    let files_scores = compute_z_scores(&files, window);
    let added_scores = compute_z_scores(&added, window);
    let removed_scores = compute_z_scores(&removed, window);
    let lang_scores = compute_z_scores(&lang_div, window);
    let author_scores = compute_z_scores(&authors, window);

    let threshold_f = f64::from(threshold);
    let mut anomalies = build_records(
        &ticks,
        tick_metrics,
        &churn_scores,
        &files_scores,
        &added_scores,
        &removed_scores,
        &lang_scores,
        &author_scores,
        threshold_f,
    );

    // Sort by max absolute Z-score descending. Go uses sort.Slice, which is not
    // stable; ties keep an unspecified relative order. We use a stable sort with
    // a strict comparator so behavior is deterministic without altering the
    // documented "most extreme first" semantics.
    anomalies.sort_by(|a, b| {
        b.max_abs_z_score
            .partial_cmp(&a.max_abs_z_score)
            .unwrap_or(Ordering::Equal)
    });

    anomalies
}

/// Builds anomaly records for ticks whose max-abs Z-score exceeds `threshold`.
///
/// Mirrors Go `buildRecords`. A tick is flagged only when `max_abs > threshold`
/// (strict), matching `if maxAbs <= threshold { continue }`.
#[allow(clippy::too_many_arguments)]
fn build_records(
    ticks: &[i64],
    tick_metrics: &BTreeMap<i64, TickMetrics>,
    churn_scores: &[f64],
    files_scores: &[f64],
    added_scores: &[f64],
    removed_scores: &[f64],
    lang_scores: &[f64],
    author_scores: &[f64],
    threshold: f64,
) -> Vec<Record> {
    let mut anomalies = Vec::new();

    for (i, tick) in ticks.iter().enumerate() {
        let scores = ZScoreSet {
            net_churn: churn_scores[i],
            files_changed: files_scores[i],
            lines_added: added_scores[i],
            lines_removed: removed_scores[i],
            language_diversity: lang_scores[i],
            author_count: author_scores[i],
        };

        let max_abs = scores.max_abs();
        if max_abs <= threshold {
            continue;
        }

        let tm = &tick_metrics[tick];

        anomalies.push(Record {
            tick: *tick,
            z_scores: scores,
            max_abs_z_score: max_abs,
            metrics: RawMetrics {
                files_changed: tm.files_changed,
                lines_added: tm.lines_added,
                lines_removed: tm.lines_removed,
                net_churn: tm.net_churn,
                language_diversity: tm.languages.len() as i64,
                author_count: tm.author_ids.len() as i64,
            },
            files: tm.files.clone(),
        });
    }

    anomalies
}

/// Detects anomalies on an external analyzer's per-dimension time series.
///
/// Mirrors Go `detectExternalAnomalies` (enrich.go). Dimensions are processed in
/// sorted-name order for deterministic output. A dimension is skipped when its
/// value count does not match `ticks.len()`. A value is flagged when its
/// absolute Z-score strictly exceeds `threshold`.
///
/// Returns `(anomalies, summaries)`, where summaries are emitted one per
/// processed dimension (in sorted order) and anomalies in dimension-then-tick
/// order (the caller re-sorts the global anomaly list by |Z| descending).
#[must_use]
pub fn detect_external_anomalies(
    source: &str,
    ticks: &[i64],
    dimensions: &BTreeMap<String, Vec<f64>>,
    window_size: usize,
    threshold: f64,
) -> (Vec<ExternalAnomaly>, Vec<ExternalSummary>) {
    let mut anomalies = Vec::new();
    let mut summaries = Vec::new();

    // BTreeMap already yields keys in sorted order (Go does sort.Strings).
    for (dim_name, values) in dimensions {
        if values.len() != ticks.len() {
            continue;
        }

        let scores = compute_z_scores(values, window_size);
        let (mean, stddev) = mean_std_dev(values);

        let mut highest_z = 0.0_f64;
        let mut anomaly_count = 0_i64;

        for (i, &score) in scores.iter().enumerate() {
            let abs_score = score.abs();

            if abs_score > threshold {
                anomalies.push(ExternalAnomaly {
                    source: source.to_string(),
                    dimension: dim_name.clone(),
                    tick: ticks[i],
                    z_score: score,
                    raw_value: values[i],
                });
                anomaly_count += 1;
            }

            if abs_score > highest_z {
                highest_z = abs_score;
            }
        }

        summaries.push(ExternalSummary {
            source: source.to_string(),
            dimension: dim_name.clone(),
            mean,
            stddev,
            anomalies: anomaly_count,
            highest_z,
        });
    }

    (anomalies, summaries)
}

/// Sorts a global external-anomaly list by descending absolute Z-score and a
/// summary list by `(source, dimension)`. Mirrors the two `sort.Slice` calls in
/// `runStoreEnrichment` (enrich_store.go) so callers that merge per-analyzer
/// results produce the same deterministic order as Go.
pub fn sort_external_results(anomalies: &mut [ExternalAnomaly], summaries: &mut [ExternalSummary]) {
    anomalies.sort_by(|a, b| {
        b.z_score
            .abs()
            .partial_cmp(&a.z_score.abs())
            .unwrap_or(Ordering::Equal)
    });

    summaries.sort_by(|a, b| match a.source.cmp(&b.source) {
        Ordering::Equal => a.dimension.cmp(&b.dimension),
        other => other,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_basic_detects_spike() {
        // Mirrors Go TestDetectExternalAnomalies.
        let ticks = vec![0, 1, 2, 3, 4];
        let dimensions = BTreeMap::from([("metric".to_string(), vec![1.0, 1.0, 1.0, 1.0, 100.0])]);

        let (anomalies, summaries) = detect_external_anomalies("src", &ticks, &dimensions, 3, 2.0);

        assert!(!anomalies.is_empty());
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].source, "src");
        assert_eq!(summaries[0].dimension, "metric");
        assert!(summaries[0].highest_z > 2.0);
    }

    #[test]
    fn external_mismatched_lengths_skipped() {
        // Mirrors Go TestDetectExternalAnomalies_MismatchedLengths.
        let ticks = vec![0, 1, 2];
        let dimensions = BTreeMap::from([("bad_dim".to_string(), vec![1.0, 2.0])]);

        let (anomalies, summaries) = detect_external_anomalies("src", &ticks, &dimensions, 3, 2.0);
        assert!(anomalies.is_empty());
        assert!(summaries.is_empty());
    }

    #[test]
    fn external_multiple_dimensions_sorted() {
        // Mirrors Go TestDetectExternalAnomalies_MultipleDimensions.
        let ticks = vec![0, 1, 2, 3, 4];
        let dimensions = BTreeMap::from([
            ("dim_a".to_string(), vec![1.0, 1.0, 1.0, 1.0, 1.0]),
            ("dim_b".to_string(), vec![1.0, 1.0, 1.0, 1.0, 50.0]),
        ]);

        let (_, summaries) = detect_external_anomalies("multi-dim", &ticks, &dimensions, 3, 2.0);
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].dimension, "dim_a");
        assert_eq!(summaries[0].anomalies, 0);
        assert_eq!(summaries[1].dimension, "dim_b");
        assert!(summaries[1].anomalies > 0);
    }

    #[test]
    fn external_no_anomalies_identical() {
        // Mirrors Go TestDetectExternalAnomalies_NoAnomalies.
        let ticks = vec![0, 1, 2, 3, 4];
        let dimensions = BTreeMap::from([("stable_metric".to_string(), vec![5.0, 5.0, 5.0, 5.0, 5.0])]);

        let (anomalies, summaries) = detect_external_anomalies("stable", &ticks, &dimensions, 3, 2.0);
        assert!(anomalies.is_empty());
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].anomalies, 0);
        assert!(summaries[0].highest_z.abs() < 0.001);
    }

    #[test]
    fn external_empty() {
        // Mirrors Go TestDetectExternalAnomalies_Empty.
        let empty: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let (anomalies, summaries) = detect_external_anomalies("src", &[], &empty, 3, 2.0);
        assert!(anomalies.is_empty());
        assert!(summaries.is_empty());
    }

    #[test]
    fn ticks_spike_is_detected_and_ranked_first() {
        // Build 10 stable ticks plus a spike, mirroring buildTestReportWithSpike.
        let mut tick_metrics = BTreeMap::new();
        for tick in 0..10_i64 {
            tick_metrics.insert(
                tick,
                TickMetrics {
                    files_changed: 5,
                    lines_added: 20,
                    lines_removed: 10,
                    net_churn: 10,
                    languages: BTreeMap::from([("Go".to_string(), 1)]),
                    ..Default::default()
                },
            );
        }
        tick_metrics.insert(
            10,
            TickMetrics {
                files_changed: 200,
                lines_added: 5000,
                lines_removed: 50,
                net_churn: 4950,
                languages: BTreeMap::from([
                    ("Go".to_string(), 1),
                    ("Python".to_string(), 1),
                    ("Shell".to_string(), 1),
                    ("YAML".to_string(), 1),
                ]),
                ..Default::default()
            },
        );

        let anomalies = detect_anomalies_from_ticks(&tick_metrics, 2.0, 5);
        assert!(!anomalies.is_empty(), "should detect the spike");
        // Most severe anomaly (tick 10 spike) sorts first.
        assert_eq!(anomalies[0].tick, 10);
    }

    #[test]
    fn empty_ticks_yield_no_anomalies() {
        let empty: BTreeMap<i64, TickMetrics> = BTreeMap::new();
        assert!(detect_anomalies_from_ticks(&empty, 2.0, 20).is_empty());
    }
}
