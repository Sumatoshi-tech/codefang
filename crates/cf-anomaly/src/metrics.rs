//! Metric computation for the temporal anomaly analyzer.

use std::collections::BTreeMap;

use cf_alg_stats::{mean_std_dev, to_percent};

use crate::config::Config;
use crate::detect::detect_anomalies_from_ticks;
use crate::model::{
    AggregateData, ComputedMetrics, ExternalAnomaly, ExternalSummary, Record, TickMetrics,
    TimeSeriesEntry,
};
use crate::zscore::compute_z_scores;

/// Inclusive tick time bounds (start/end) used to annotate time-series
/// entries: pre-formatted RFC3339 strings; an empty string means "no bound"
/// and is omitted from the [`TimeSeriesEntry`] output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickBounds {
    /// Pre-formatted RFC3339 start time, or empty for none.
    pub start_time: String,
    /// Pre-formatted RFC3339 end time, or empty for none.
    pub end_time: String,
}

/// The typed input of the anomaly metrics.
#[derive(Debug, Clone, Default)]
pub struct ReportData {
    /// Pre-detected anomalies.
    pub anomalies: Vec<Record>,
    /// Per-tick metrics (derived from per-commit data).
    pub tick_metrics: BTreeMap<i64, TickMetrics>,
    /// Optional per-tick time bounds.
    pub tick_bounds: BTreeMap<i64, TickBounds>,
    /// Z-score threshold the anomalies were detected with.
    pub threshold: f32,
    /// Trailing window size the anomalies were detected with.
    pub window_size: usize,
    /// Cross-analyzer anomalies passed through to the output.
    pub external_anomalies: Vec<ExternalAnomaly>,
    /// Cross-analyzer summaries passed through to the output.
    pub external_summaries: Vec<ExternalSummary>,
}

/// Extracts the anomaly list.
#[must_use]
fn compute_list(input: &ReportData) -> Vec<Record> {
    input.anomalies.clone()
}

/// Calculates aggregate statistics.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)] // contractual count math
fn compute_aggregate(input: &ReportData) -> AggregateData {
    let total_ticks = input.tick_metrics.len() as i64;
    let total_anomalies = input.anomalies.len() as i64;

    let anomaly_rate = if total_ticks > 0 {
        to_percent(total_anomalies as f64 / total_ticks as f64)
    } else {
        0.0
    };

    let ticks: Vec<i64> = input.tick_metrics.keys().copied().collect();

    let mut churn = vec![0.0_f64; ticks.len()];
    let mut files = vec![0.0_f64; ticks.len()];
    let mut lang_div = vec![0.0_f64; ticks.len()];
    let mut authors = vec![0.0_f64; ticks.len()];

    for (i, tick) in ticks.iter().enumerate() {
        let tm = &input.tick_metrics[tick];
        churn[i] = tm.net_churn as f64;
        files[i] = tm.files_changed() as f64;
        lang_div[i] = tm.language_diversity() as f64;
        authors[i] = tm.author_count() as f64;
    }

    let (churn_mean, churn_stddev) = mean_std_dev(&churn);
    let (files_mean, files_stddev) = mean_std_dev(&files);
    let (lang_div_mean, lang_div_stddev) = mean_std_dev(&lang_div);
    let (author_mean, author_stddev) = mean_std_dev(&authors);

    AggregateData {
        total_ticks,
        total_anomalies,
        anomaly_rate,
        threshold: input.threshold,
        window_size: input.window_size as i64,
        churn_mean,
        churn_stddev,
        files_mean,
        files_stddev,
        lang_diversity_mean: lang_div_mean,
        lang_diversity_stddev: lang_div_stddev,
        author_count_mean: author_mean,
        author_count_stddev: author_stddev,
    }
}

/// Builds the annotated time series.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)] // contractual count math
fn compute_time_series(input: &ReportData) -> Vec<TimeSeriesEntry> {
    let ticks: Vec<i64> = input.tick_metrics.keys().copied().collect();

    // Anomalous-tick set for O(log n) lookup.
    let anomaly_set: std::collections::BTreeSet<i64> =
        input.anomalies.iter().map(|a| a.tick).collect();

    // Churn Z-scores in tick order.
    let churn: Vec<f64> = ticks
        .iter()
        .map(|tick| input.tick_metrics[tick].net_churn as f64)
        .collect();
    let churn_scores = compute_z_scores(&churn, input.window_size);

    let mut entries = Vec::with_capacity(ticks.len());

    for (i, tick) in ticks.iter().enumerate() {
        let tm = &input.tick_metrics[tick];
        let is_anomaly = anomaly_set.contains(tick);

        let churn_z = churn_scores.get(i).copied().unwrap_or(0.0);

        let mut entry = TimeSeriesEntry {
            tick: *tick,
            metrics: tm.raw(),
            is_anomaly,
            churn_z_score: churn_z,
            ..Default::default()
        };

        if let Some(bounds) = input.tick_bounds.get(tick) {
            entry.start_time.clone_from(&bounds.start_time);
            entry.end_time.clone_from(&bounds.end_time);
        }

        entries.push(entry);
    }

    entries
}

/// Runs all anomaly metrics over typed [`ReportData`].
///
/// The returned [`ComputedMetrics`] is what the store/report paths serialize
/// through `cf-gojson` for the machine formats.
#[must_use]
pub fn compute_all_metrics(input: &ReportData) -> ComputedMetrics {
    ComputedMetrics {
        anomalies: compute_list(input),
        time_series: compute_time_series(input),
        aggregate: compute_aggregate(input),
        external_anomalies: input.external_anomalies.clone(),
        external_summaries: input.external_summaries.clone(),
    }
}

/// Builds [`ReportData`] from per-commit metrics, the commits-by-tick
/// mapping, and the validated [`Config`], running anomaly detection over the
/// aggregated ticks.
///
/// This is the canonical path: aggregate commits to ticks, detect anomalies,
/// and package the typed inputs [`compute_all_metrics`] consumes.
#[must_use]
pub fn build_report_data(
    commit_metrics: &BTreeMap<String, crate::model::CommitAnomalyData>,
    commits_by_tick: &BTreeMap<i64, Vec<String>>,
    tick_bounds: BTreeMap<i64, TickBounds>,
    config: Config,
) -> ReportData {
    let tick_metrics =
        crate::aggregate::aggregate_commits_to_ticks(commit_metrics, commits_by_tick);
    let anomalies =
        detect_anomalies_from_ticks(&tick_metrics, config.threshold, config.window_size);

    ReportData {
        anomalies,
        tick_metrics,
        tick_bounds,
        threshold: config.threshold,
        window_size: config.window_size,
        external_anomalies: Vec::new(),
        external_summaries: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CommitAnomalyData;

    fn hash(c: char) -> String {
        std::iter::repeat(c).take(40).collect()
    }

    fn build_test_report() -> ReportData {
        // Mirrors the reference suite's buildTestReport: 3 ticks, no
        // pre-detected anomalies.
        let mut commit_metrics = BTreeMap::new();
        commit_metrics.insert(
            hash('a'),
            CommitAnomalyData {
                files_changed: 5,
                lines_added: 20,
                lines_removed: 10,
                net_churn: 10,
                files: vec!["main.go".into()],
                languages: BTreeMap::from([("Go".to_string(), 3), ("Python".to_string(), 2)]),
                author_id: 0,
            },
        );
        commit_metrics.insert(
            hash('b'),
            CommitAnomalyData {
                files_changed: 3,
                lines_added: 15,
                lines_removed: 8,
                net_churn: 7,
                files: vec!["util.go".into()],
                languages: BTreeMap::from([("Go".to_string(), 3)]),
                author_id: 1,
            },
        );
        commit_metrics.insert(
            hash('c'),
            CommitAnomalyData {
                files_changed: 4,
                lines_added: 18,
                lines_removed: 9,
                net_churn: 9,
                files: vec!["lib.go".into()],
                languages: BTreeMap::from([("Go".to_string(), 2), ("Rust".to_string(), 2)]),
                author_id: 0,
            },
        );
        let commits_by_tick = BTreeMap::from([
            (0_i64, vec![hash('a')]),
            (1_i64, vec![hash('b')]),
            (2_i64, vec![hash('c')]),
        ]);

        build_report_data(
            &commit_metrics,
            &commits_by_tick,
            BTreeMap::new(),
            Config::default(),
        )
    }

    #[test]
    fn compute_all_metrics_basic() {
        // Mirrors reference test TestComputeAllMetrics_Basic.
        let input = build_test_report();
        let computed = compute_all_metrics(&input);
        assert!(computed.aggregate.total_ticks > 0);
        assert_eq!(computed.time_series.len(), 3);
    }

    #[test]
    fn compute_all_metrics_from_commit_data_three_ticks() {
        // Mirrors reference test TestComputeAllMetrics_FromCommitData.
        let input = build_test_report();
        let computed = compute_all_metrics(&input);
        assert_eq!(computed.aggregate.total_ticks, 3);
    }

    #[test]
    fn compute_all_metrics_with_spike() {
        // Mirrors reference test TestComputeAllMetrics_WithAnomaly (window=5 spike).
        let mut commit_metrics = BTreeMap::new();
        let mut commits_by_tick = BTreeMap::new();
        for tick in 0..10_i64 {
            let h = format!("{tick:040x}");
            commit_metrics.insert(
                h.clone(),
                CommitAnomalyData {
                    files_changed: 5,
                    lines_added: 20,
                    lines_removed: 10,
                    net_churn: 10,
                    files: vec!["main.go".into()],
                    languages: BTreeMap::from([("Go".to_string(), 5)]),
                    author_id: 0,
                },
            );
            commits_by_tick.insert(tick, vec![h]);
        }
        let spike = format!("{:040x}", 10);
        commit_metrics.insert(
            spike.clone(),
            CommitAnomalyData {
                files_changed: 200,
                lines_added: 5000,
                lines_removed: 50,
                net_churn: 4950,
                files: vec!["huge.go".into()],
                languages: BTreeMap::from([
                    ("Go".to_string(), 50),
                    ("Python".to_string(), 30),
                    ("Shell".to_string(), 20),
                    ("YAML".to_string(), 100),
                ]),
                author_id: 0,
            },
        );
        commits_by_tick.insert(10, vec![spike]);

        let config = Config::new(2.0, 5).expect("valid config");
        let input = build_report_data(&commit_metrics, &commits_by_tick, BTreeMap::new(), config);
        let computed = compute_all_metrics(&input);

        assert!(!computed.anomalies.is_empty(), "should detect the spike");
        assert!(computed.aggregate.total_anomalies > 0);
        assert!(computed.aggregate.anomaly_rate > 0.0);
        assert_eq!(computed.aggregate.window_size, 5);
    }

    #[test]
    fn quiet_then_busy_ticks_match_the_hand_computation() {
        // Ten quiet ticks (one file each, 10/12 lines alternating) and a busy
        // tick: 11 distinct files, 200 added, 10 removed.
        let mut commit_metrics = BTreeMap::new();
        let mut commits_by_tick = BTreeMap::new();
        for tick in 0..10_i64 {
            let h = format!("{tick:040x}");
            commit_metrics.insert(
                h.clone(),
                CommitAnomalyData {
                    files_changed: 1,
                    lines_added: if tick % 2 == 0 { 10 } else { 12 },
                    files: vec![format!("q{tick}.txt")],
                    ..Default::default()
                },
            );
            commits_by_tick.insert(tick, vec![h]);
        }
        let mut busy = Vec::new();
        for k in 0..20_i64 {
            let h = format!("{:040x}", 100 + k);
            let file = if k < 10 {
                format!("burst{k}.txt")
            } else {
                "burst0.txt".to_string()
            };
            commit_metrics.insert(
                h.clone(),
                CommitAnomalyData {
                    files_changed: 1,
                    lines_added: 10,
                    files: vec![file],
                    ..Default::default()
                },
            );
            busy.push(h);
        }
        let del = format!("{:040x}", 200);
        commit_metrics.insert(
            del.clone(),
            CommitAnomalyData {
                files_changed: 1,
                lines_removed: 10,
                files: vec!["q0.txt".to_string()],
                ..Default::default()
            },
        );
        busy.push(del);
        commits_by_tick.insert(10, busy);

        let input = build_report_data(
            &commit_metrics,
            &commits_by_tick,
            BTreeMap::new(),
            Config::default(),
        );
        let computed = compute_all_metrics(&input);
        assert_eq!(computed.anomalies.len(), 1, "{:?}", computed.anomalies);
        let busy = &computed.anomalies[0];
        assert_eq!(busy.tick, 10);
        // Window mean 11, population std 1: (190 - 11) / 1 and (200 - 11) / 1.
        assert!((busy.z_scores.net_churn - 179.0).abs() < 1e-9);
        assert!((busy.z_scores.lines_added - 189.0).abs() < 1e-9);
        // Every quiet tick touched one file: zero variance -> sentinel.
        assert!((busy.z_scores.files_changed - 100.0).abs() < 1e-9);
        assert_eq!(busy.metrics.files_changed, 11);
        assert_eq!(busy.files.len(), 11);
        assert!(busy.files.contains(&"q0.txt".to_string()));
        // The second quiet tick is NOT flagged (one observation, no sentinel).
        assert!(!computed.time_series[1].is_anomaly);
        assert_eq!(computed.aggregate.total_anomalies, 1);
    }

    #[test]
    fn time_series_marks_anomalous_ticks() {
        let input = build_test_report();
        let computed = compute_all_metrics(&input);
        // Every tick is present in the series.
        assert_eq!(computed.time_series.len(), 3);
        // is_anomaly is driven by the pre-detected anomaly set, so the number
        // of flagged series entries equals the number of detected anomalies.
        let flagged = computed.time_series.iter().filter(|e| e.is_anomaly).count();
        assert_eq!(flagged, computed.anomalies.len());
    }
}
