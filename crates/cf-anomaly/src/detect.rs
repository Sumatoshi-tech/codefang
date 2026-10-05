//! Anomaly detection over per-tick and external time-series data.

use std::collections::BTreeMap;

use cf_alg_stats::mean_std_dev;
use cf_gosort::go_sort_slice;

use crate::model::{ExternalAnomaly, ExternalSummary, Record, TickMetrics, ZScoreSet};
use crate::zscore::compute_z_scores;

/// Detects anomalies across all six per-tick metrics.
///
/// Computes independent trailing-window Z-scores for net churn, files
/// changed, lines added, lines removed, language diversity, and author count,
/// flags any tick whose maximum absolute Z-score strictly exceeds
/// `threshold`, and sorts the result by descending severity.
///
/// `tick_metrics` keys are consumed in ascending order. The `threshold` is
/// single-precision by contract and widened to `f64` for the comparison.
#[must_use]
#[allow(clippy::cast_precision_loss)] // contractual float math on counts
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
        files[i] = tm.files_changed() as f64;
        added[i] = tm.lines_added as f64;
        removed[i] = tm.lines_removed as f64;
        lang_div[i] = tm.language_diversity() as f64;
        authors[i] = tm.author_count() as f64;
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

    // Sort by max absolute Z-score descending; ties stay in tick order.
    anomalies.sort_by(|a, b| {
        b.max_abs_z_score
            .total_cmp(&a.max_abs_z_score)
            .then_with(|| a.tick.cmp(&b.tick))
    });

    anomalies
}

/// Builds anomaly records for ticks whose max-abs Z-score exceeds
/// `threshold` (strict).
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
            metrics: tm.raw(),
            files: tm.files.iter().cloned().collect(),
        });
    }

    anomalies
}

/// Detects anomalies on an external analyzer's per-dimension time series.
///
/// Dimensions are processed in sorted-name order for deterministic output. A
/// dimension is skipped when its value count does not match `ticks.len()`. A
/// value is flagged when its absolute Z-score strictly exceeds `threshold`.
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

    // BTreeMap already yields keys in sorted order.
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
/// summary list by `(source, dimension)`, so callers that merge per-analyzer
/// results produce the same deterministic order as the reference binary.
pub fn sort_external_results(anomalies: &mut [ExternalAnomaly], summaries: &mut [ExternalSummary]) {
    // Both lists are sorted with Go's unstable sort.Slice in the reference
    // binary; tie order must replicate pdqsort exactly.
    go_sort_slice(anomalies, |a, b| a.z_score.abs() > b.z_score.abs());

    go_sort_slice(summaries, |a, b| {
        if a.source == b.source {
            a.dimension < b.dimension
        } else {
            a.source < b.source
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_basic_detects_spike() {
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
        let ticks = vec![0, 1, 2];
        let dimensions = BTreeMap::from([("bad_dim".to_string(), vec![1.0, 2.0])]);

        let (anomalies, summaries) = detect_external_anomalies("src", &ticks, &dimensions, 3, 2.0);
        assert!(anomalies.is_empty());
        assert!(summaries.is_empty());
    }

    #[test]
    fn ticks_spike_is_detected_and_ranked_first() {
        let mut tick_metrics = BTreeMap::new();
        for tick in 0..10_i64 {
            tick_metrics.insert(
                tick,
                TickMetrics {
                    lines_added: 20,
                    lines_removed: 10,
                    net_churn: 10,
                    files: (0..5).map(|f| format!("f{f}.go")).collect(),
                    languages: BTreeMap::from([("Go".to_string(), 1)]),
                    ..Default::default()
                },
            );
        }
        tick_metrics.insert(
            10,
            TickMetrics {
                lines_added: 5000,
                lines_removed: 50,
                net_churn: 4950,
                files: (0..200).map(|f| format!("g{f}.go")).collect(),
                languages: BTreeMap::from([("Go".to_string(), 1), ("Python".to_string(), 1)]),
                ..Default::default()
            },
        );

        let anomalies = detect_anomalies_from_ticks(&tick_metrics, 2.0, 5);
        assert_eq!(anomalies.len(), 1, "only the spike is flagged");
        assert_eq!(anomalies[0].tick, 10);
        assert_eq!(anomalies[0].metrics.files_changed, 200);
        assert_eq!(anomalies[0].files.len(), 200);
        assert_eq!(anomalies[0].files[0], "g0.go");
    }

    #[test]
    fn ties_rank_by_tick_after_severity() {
        // Two spikes, each after three flat ticks (zero variance -> the same
        // sentinel severity): the tie resolves by tick.
        let mut tick_metrics = BTreeMap::new();
        for tick in 0..8_i64 {
            let churn = if tick == 3 || tick == 7 { 500 } else { 10 };
            tick_metrics.insert(
                tick,
                TickMetrics {
                    lines_added: churn,
                    net_churn: churn,
                    ..Default::default()
                },
            );
        }
        let anomalies = detect_anomalies_from_ticks(&tick_metrics, 2.0, 3);
        let ticks: Vec<i64> = anomalies.iter().map(|a| a.tick).collect();
        assert_eq!(ticks, vec![3, 7], "{anomalies:?}");
        assert!((anomalies[0].max_abs_z_score - anomalies[1].max_abs_z_score).abs() < 1e-9);
    }

    #[test]
    fn empty_ticks_yield_no_anomalies() {
        let empty: BTreeMap<i64, TickMetrics> = BTreeMap::new();
        assert!(detect_anomalies_from_ticks(&empty, 2.0, 20).is_empty());
    }
}
