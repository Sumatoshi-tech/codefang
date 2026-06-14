//! Quality statistics: per-tick stats, time-series and aggregate computation.
//!
//! All numeric routines delegate to [`cf_alg_stats`] (the shared statistics
//! kernel) so that the mean / median / P95 / max / min / sum values flowing
//! into machine reports are computed operation-for-operation as in the
//! reference implementation.
//!
//! # Compatibility
//!
//! [`TickStats`], [`TimeSeriesEntry`], [`AggregateData`] and [`ComputedMetrics`]
//! are *wrapper* structs: their fields serialize in **declaration order**,
//! honoring `omitempty` on `start_time` / `end_time`. They are emitted through
//! the fixed-order `GoMap` builder in [`crate::serialize`], never via serde
//! defaults; output bytes are pinned by `tests/compat`.

use std::collections::BTreeMap;

use cf_alg_stats as stats;

use crate::data::TickQuality;

/// Dimension name: median cyclomatic complexity (`complexity_median`).
pub const DIM_COMPLEXITY_MEDIAN: &str = "complexity_median";
/// Dimension name: P95 cyclomatic complexity (`complexity_p95`).
pub const DIM_COMPLEXITY_P95: &str = "complexity_p95";
/// Dimension name: median Halstead volume (`halstead_vol_median`).
pub const DIM_HALSTEAD_VOL_MEDIAN: &str = "halstead_vol_median";
/// Dimension name: summed delivered bugs (`delivered_bugs_sum`).
pub const DIM_DELIVERED_BUGS_SUM: &str = "delivered_bugs_sum";
/// Dimension name: minimum comment score (`comment_score_min`).
pub const DIM_COMMENT_SCORE_MIN: &str = "comment_score_min";
/// Dimension name: minimum cohesion (`cohesion_min`).
pub const DIM_COHESION_MIN: &str = "cohesion_min";

/// Computed statistics for a single tick.
///
/// Field order is the declaration order and is the serialization order for
/// both JSON and YAML (the snake_case wire names are noted on each field).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TickStats {
    // Complexity.
    /// `complexity_mean`
    pub complexity_mean: f64,
    /// `complexity_median`
    pub complexity_median: f64,
    /// `complexity_p95`
    pub complexity_p95: f64,
    /// `complexity_max`
    pub complexity_max: f64,

    // Halstead.
    /// `halstead_vol_mean`
    pub halstead_vol_mean: f64,
    /// `halstead_vol_median`
    pub halstead_vol_median: f64,
    /// `halstead_vol_p95`
    pub halstead_vol_p95: f64,
    /// `halstead_vol_sum`
    pub halstead_vol_sum: f64,

    // Delivered bugs.
    /// `delivered_bugs_sum`
    pub delivered_bugs_sum: f64,

    // Comments.
    /// `comment_score_mean`
    pub comment_score_mean: f64,
    /// `comment_score_min`
    pub comment_score_min: f64,
    /// `doc_coverage_mean`
    pub doc_coverage_mean: f64,

    // Cohesion.
    /// `cohesion_mean`
    pub cohesion_mean: f64,
    /// `cohesion_min`
    pub cohesion_min: f64,

    // Bookkeeping.
    /// `files_analyzed`
    pub files_analyzed: i64,
    /// `total_functions`
    pub total_functions: i64,
    /// `max_complexity`
    pub max_complexity: i64,
}

/// Computes per-tick statistics.
///
/// Returns the zero value when no files were analyzed (pinned report
/// behaviour for empty ticks).
///
/// ```
/// use cf_quality::data::TickQuality;
/// use cf_quality::metrics::{compute_tick_stats, TickStats};
///
/// // Two files with complexities 10 and 20.
/// let tq = TickQuality { complexities: vec![10.0, 20.0], ..TickQuality::default() };
/// let stats = compute_tick_stats(&tq);
/// assert_eq!(stats.files_analyzed, 2);
/// assert_eq!(stats.complexity_mean, 15.0);
///
/// // An empty tick yields the zero value.
/// assert_eq!(compute_tick_stats(&TickQuality::default()), TickStats::default());
/// ```
#[must_use]
pub fn compute_tick_stats(tq: &TickQuality) -> TickStats {
    let n = tq.files_analyzed();
    if n == 0 {
        return TickStats::default();
    }

    TickStats {
        // Complexity.
        complexity_mean: stats::mean(&tq.complexities),
        complexity_median: stats::median(&tq.complexities),
        complexity_p95: stats::percentile(&tq.complexities, stats::PERCENTILE_P95),
        complexity_max: stats::max(&tq.complexities),

        // Halstead.
        halstead_vol_mean: stats::mean(&tq.halstead_volumes),
        halstead_vol_median: stats::median(&tq.halstead_volumes),
        halstead_vol_p95: stats::percentile(&tq.halstead_volumes, stats::PERCENTILE_P95),
        halstead_vol_sum: stats::sum(&tq.halstead_volumes),

        // Delivered bugs.
        delivered_bugs_sum: stats::sum(&tq.delivered_bugs),

        // Comments.
        comment_score_mean: stats::mean(&tq.comment_scores),
        comment_score_min: stats::min(&tq.comment_scores),
        doc_coverage_mean: stats::mean(&tq.doc_coverages),

        // Cohesion.
        cohesion_mean: stats::mean(&tq.cohesion_scores),
        cohesion_min: stats::min(&tq.cohesion_scores),

        // Bookkeeping.
        files_analyzed: n as i64,
        total_functions: stats::sum(&tq.functions),
        max_complexity: stats::max(&tq.max_complexities),
    }
}

/// Tick boundary timestamps used to populate `start_time` / `end_time`.
///
/// The strings are the already-formatted RFC3339 values; formatting is owned
/// by the framework layer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TickBounds {
    /// Pre-formatted start time (RFC3339), empty when unknown.
    pub start_time: String,
    /// Pre-formatted end time (RFC3339), empty when unknown.
    pub end_time: String,
}

/// Per-tick time-series entry.
///
/// `start_time` / `end_time` are `omitempty`: empty strings are omitted from the
/// machine output.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimeSeriesEntry {
    /// `tick`
    pub tick: i64,
    /// `start_time,omitempty`
    pub start_time: String,
    /// `end_time,omitempty`
    pub end_time: String,
    /// `stats`
    pub stats: TickStats,
}

/// Overall summary statistics.
///
/// Field order is the declaration order (= serialization order).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AggregateData {
    /// `total_ticks`
    pub total_ticks: i64,
    /// `total_files_analyzed`
    pub total_files_analyzed: i64,
    /// `complexity_median_mean`
    pub complexity_median_mean: f64,
    /// `complexity_p95_mean`
    pub complexity_p95_mean: f64,
    /// `halstead_vol_median_mean`
    pub halstead_vol_median_mean: f64,
    /// `total_delivered_bugs`
    pub total_delivered_bugs: f64,
    /// `comment_score_mean_mean`
    pub comment_score_mean_mean: f64,
    /// `min_comment_score`
    pub min_comment_score: f64,
    /// `cohesion_mean_mean`
    pub cohesion_mean_mean: f64,
    /// `min_cohesion`
    pub min_cohesion: f64,
}

/// All computed metric results.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    /// `time_series`
    pub time_series: Vec<TimeSeriesEntry>,
    /// `aggregate`
    pub aggregate: AggregateData,
}

/// Parsed input data for metrics computation.
#[derive(Debug, Clone, Default)]
pub struct ReportData {
    /// Per-tick merged quality (keyed by tick number).
    pub tick_quality: BTreeMap<i64, TickQuality>,
    /// Per-tick boundary timestamps.
    pub tick_bounds: BTreeMap<i64, TickBounds>,
}

/// Groups per-commit quality into per-tick quality.
///
/// For every tick, merges the [`TickQuality`] of each of its commit hashes (in
/// the order given by `commits_by_tick`), skipping hashes absent from
/// `commit_quality`. Ticks with no present commits are omitted. Returns an
/// empty map when either input is empty.
///
/// ```
/// use cf_quality::data::TickQuality;
/// use cf_quality::metrics::aggregate_commits_to_ticks;
/// use std::collections::BTreeMap;
///
/// let commit_quality = BTreeMap::from([
///     ("aaa".to_string(), TickQuality { complexities: vec![1.0], ..Default::default() }),
///     ("bbb".to_string(), TickQuality { complexities: vec![2.0], ..Default::default() }),
/// ]);
/// // Tick 0 references "aaa", "bbb", and an absent "ccc" (skipped).
/// let commits_by_tick = BTreeMap::from([(0i64, vec![
///     "aaa".to_string(), "bbb".to_string(), "ccc".to_string(),
/// ])]);
///
/// let per_tick = aggregate_commits_to_ticks(&commit_quality, &commits_by_tick);
/// assert_eq!(per_tick[&0].complexities, vec![1.0, 2.0]);
///
/// // Either input empty → empty result.
/// assert!(aggregate_commits_to_ticks(&BTreeMap::new(), &commits_by_tick).is_empty());
/// ```
#[must_use]
pub fn aggregate_commits_to_ticks(
    commit_quality: &BTreeMap<String, TickQuality>,
    commits_by_tick: &BTreeMap<i64, Vec<String>>,
) -> BTreeMap<i64, TickQuality> {
    let mut result = BTreeMap::new();

    if commit_quality.is_empty() || commits_by_tick.is_empty() {
        return result;
    }

    for (&tick, hashes) in commits_by_tick {
        let mut merged: Option<TickQuality> = None;

        for hash in hashes {
            let Some(cq) = commit_quality.get(hash) else {
                continue;
            };
            merged.get_or_insert_with(TickQuality::new).merge(cq);
        }

        if let Some(m) = merged {
            result.insert(tick, m);
        }
    }

    result
}

/// Builds [`ReportData`] from the canonical inputs.
///
/// Aggregates per-commit quality into per-tick quality only when
/// `commit_quality` is non-empty; otherwise `tick_quality` is empty.
#[must_use]
pub fn parse_report_data(
    commit_quality: &BTreeMap<String, TickQuality>,
    commits_by_tick: &BTreeMap<i64, Vec<String>>,
    tick_bounds: BTreeMap<i64, TickBounds>,
) -> ReportData {
    let tick_quality = if !commit_quality.is_empty() {
        aggregate_commits_to_ticks(commit_quality, commits_by_tick)
    } else {
        BTreeMap::new()
    };

    ReportData {
        tick_quality,
        tick_bounds,
    }
}

/// Runs all quality metrics.
///
/// Iterates ticks in ascending order. The global minimum comment-score and
/// cohesion are seeded to `+∞` and updated only for ticks with
/// `files_analyzed > 0` whose value is strictly smaller; if no such tick
/// exists the field is reset to `0` — this infinity-seed/reset sequence is
/// pinned reference behaviour and must be reproduced exactly.
#[must_use]
pub fn compute_all_metrics(input: &ReportData) -> ComputedMetrics {
    let ticks: Vec<i64> = input.tick_quality.keys().copied().collect();
    let mut time_series = Vec::with_capacity(ticks.len());

    let mut complexity_medians = Vec::with_capacity(ticks.len());
    let mut complexity_p95s = Vec::with_capacity(ticks.len());
    let mut halstead_medians = Vec::with_capacity(ticks.len());
    let mut comment_means = Vec::with_capacity(ticks.len());
    let mut cohesion_means = Vec::with_capacity(ticks.len());

    let mut total_files: i64 = 0;
    let mut total_bugs: f64 = 0.0;

    let mut global_min_comment = f64::INFINITY;
    let mut global_min_cohesion = f64::INFINITY;

    for &tick in &ticks {
        let tq = &input.tick_quality[&tick];
        let ts = compute_tick_stats(tq);

        let mut entry = TimeSeriesEntry {
            tick,
            stats: ts,
            ..TimeSeriesEntry::default()
        };

        if let Some(bounds) = input.tick_bounds.get(&tick) {
            entry.start_time = bounds.start_time.clone();
            entry.end_time = bounds.end_time.clone();
        }

        time_series.push(entry);

        complexity_medians.push(ts.complexity_median);
        complexity_p95s.push(ts.complexity_p95);
        halstead_medians.push(ts.halstead_vol_median);
        comment_means.push(ts.comment_score_mean);
        cohesion_means.push(ts.cohesion_mean);

        total_files += ts.files_analyzed;
        total_bugs += ts.delivered_bugs_sum;

        if ts.comment_score_min < global_min_comment && ts.files_analyzed > 0 {
            global_min_comment = ts.comment_score_min;
        }
        if ts.cohesion_min < global_min_cohesion && ts.files_analyzed > 0 {
            global_min_cohesion = ts.cohesion_min;
        }
    }

    if global_min_comment.is_infinite() && global_min_comment > 0.0 {
        global_min_comment = 0.0;
    }
    if global_min_cohesion.is_infinite() && global_min_cohesion > 0.0 {
        global_min_cohesion = 0.0;
    }

    let aggregate = AggregateData {
        total_ticks: ticks.len() as i64,
        total_files_analyzed: total_files,
        complexity_median_mean: stats::mean(&complexity_medians),
        complexity_p95_mean: stats::mean(&complexity_p95s),
        halstead_vol_median_mean: stats::mean(&halstead_medians),
        total_delivered_bugs: total_bugs,
        comment_score_mean_mean: stats::mean(&comment_means),
        min_comment_score: global_min_comment,
        cohesion_mean_mean: stats::mean(&cohesion_means),
        min_cohesion: global_min_cohesion,
    };

    ComputedMetrics {
        time_series,
        aggregate,
    }
}

/// Per-commit summary used by the unified timeseries / drain paths.
///
/// Field order here is irrelevant — the machine output for this map is
/// *map-origin* and therefore byte-sorts its keys on encode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommitSummary {
    /// `complexity_median`
    pub complexity_median: f64,
    /// `cognitive_median` = `median(cognitives)`
    pub cognitive_median: f64,
    /// `max_complexity`
    pub max_complexity: i64,
    /// `functions` = `total_functions`
    pub functions: i64,
    /// `halstead_vol_median`
    pub halstead_vol_median: f64,
    /// `halstead_effort_median` = `median(halstead_efforts)`
    pub halstead_effort_median: f64,
    /// `delivered_bugs_sum`
    pub delivered_bugs_sum: f64,
    /// `comment_score_min`
    pub comment_score_min: f64,
    /// `doc_coverage_mean`
    pub doc_coverage_mean: f64,
    /// `cohesion_min`
    pub cohesion_min: f64,
    /// `files_analyzed`
    pub files_analyzed: i64,
}

/// Computes the per-commit summary for one [`TickQuality`].
///
/// Shared by the commit-timeseries and drain paths, which build identical
/// maps.
#[must_use]
pub fn commit_summary(tq: &TickQuality) -> CommitSummary {
    let ts = compute_tick_stats(tq);
    CommitSummary {
        complexity_median: ts.complexity_median,
        cognitive_median: stats::median(&tq.cognitives),
        max_complexity: ts.max_complexity,
        functions: ts.total_functions,
        halstead_vol_median: ts.halstead_vol_median,
        halstead_effort_median: stats::median(&tq.halstead_efforts),
        delivered_bugs_sum: ts.delivered_bugs_sum,
        comment_score_min: ts.comment_score_min,
        doc_coverage_mean: ts.doc_coverage_mean,
        cohesion_min: ts.cohesion_min,
        files_analyzed: ts.files_analyzed,
    }
}

/// Builds the per-commit summary map.
///
/// Returns an empty map when `commit_quality` is empty.
#[must_use]
pub fn extract_commit_time_series(
    commit_quality: &BTreeMap<String, TickQuality>,
) -> BTreeMap<String, CommitSummary> {
    commit_quality
        .iter()
        .map(|(hash, tq)| (hash.clone(), commit_summary(tq)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(a: f64, b: f64, d: f64) {
        assert!((a - b).abs() <= d, "expected {b} got {a} (delta {d})");
    }

    fn cq(pairs: &[(&str, TickQuality)]) -> BTreeMap<String, TickQuality> {
        pairs.iter().map(|(h, q)| ((*h).into(), q.clone())).collect()
    }

    fn ct(pairs: &[(i64, &[&str])]) -> BTreeMap<i64, Vec<String>> {
        pairs
            .iter()
            .map(|(t, hs)| (*t, hs.iter().map(|s| (*s).to_string()).collect()))
            .collect()
    }

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    // Mirrors reference test TestComputeTickStats.
    #[test]
    fn compute_tick_stats_matches_go() {
        let tq = TickQuality {
            complexities: vec![2.0, 4.0, 6.0, 8.0, 10.0],
            cognitives: vec![1.0, 2.0, 3.0, 4.0, 5.0],
            max_complexities: vec![3, 5, 7, 8, 10],
            functions: vec![2, 3, 4, 5, 6],
            halstead_volumes: vec![100.0, 200.0, 300.0, 400.0, 500.0],
            halstead_efforts: vec![50.0, 100.0, 150.0, 200.0, 250.0],
            delivered_bugs: vec![0.1, 0.2, 0.3, 0.4, 0.5],
            comment_scores: vec![0.3, 0.5, 0.7, 0.8, 0.9],
            doc_coverages: vec![0.4, 0.5, 0.6, 0.7, 0.8],
            cohesion_scores: vec![0.8, 0.85, 0.9, 0.95, 1.0],
        };
        let ts = compute_tick_stats(&tq);

        assert_eq!(ts.files_analyzed, 5);
        delta(ts.complexity_mean, 6.0, 0.01);
        delta(ts.complexity_median, 6.0, 0.01);
        delta(ts.complexity_p95, 9.2, 0.5);
        delta(ts.complexity_max, 10.0, 0.01);
        delta(ts.halstead_vol_mean, 300.0, 0.01);
        delta(ts.halstead_vol_median, 300.0, 0.01);
        delta(ts.halstead_vol_sum, 1500.0, 0.01);
        delta(ts.delivered_bugs_sum, 1.5, 0.01);
        delta(ts.comment_score_mean, 0.64, 0.01);
        delta(ts.comment_score_min, 0.3, 0.01);
        delta(ts.cohesion_mean, 0.9, 0.01);
        delta(ts.cohesion_min, 0.8, 0.01);
        assert_eq!(ts.total_functions, 20);
        assert_eq!(ts.max_complexity, 10);
    }

    // Mirrors reference test TestComputeTickStats_ZeroFiles.
    #[test]
    fn compute_tick_stats_zero_files() {
        let ts = compute_tick_stats(&TickQuality::default());
        assert_eq!(ts.files_analyzed, 0);
        delta(ts.complexity_mean, 0.0, 0.01);
        delta(ts.complexity_median, 0.0, 0.01);
    }

    // Mirrors reference test TestComputeAllMetrics_FromCommitData.
    #[test]
    fn compute_all_metrics_from_commit_data() {
        let commit_quality = cq(&[
            (
                HASH_A,
                TickQuality {
                    complexities: vec![10.0, 20.0],
                    halstead_volumes: vec![100.0, 200.0],
                    delivered_bugs: vec![0.1, 0.2],
                    comment_scores: vec![0.5, 0.7],
                    cohesion_scores: vec![0.8, 0.9],
                    ..TickQuality::default()
                },
            ),
            (
                HASH_B,
                TickQuality {
                    complexities: vec![30.0],
                    halstead_volumes: vec![300.0],
                    delivered_bugs: vec![0.3],
                    comment_scores: vec![0.9],
                    cohesion_scores: vec![1.0],
                    ..TickQuality::default()
                },
            ),
        ]);
        let commits_by_tick = ct(&[(0, &[HASH_A]), (1, &[HASH_B])]);

        let input = parse_report_data(&commit_quality, &commits_by_tick, BTreeMap::new());
        let computed = compute_all_metrics(&input);

        assert_eq!(computed.time_series.len(), 2);
        assert_eq!(computed.aggregate.total_ticks, 2);
        assert_eq!(computed.aggregate.total_files_analyzed, 3);
    }

    // Mirrors reference test TestComputeAllMetrics_FromCanonical.
    #[test]
    fn compute_all_metrics_from_canonical() {
        let commit_quality = cq(&[(
            HASH_A,
            TickQuality {
                complexities: vec![10.0, 20.0],
                halstead_volumes: vec![100.0, 200.0],
                delivered_bugs: vec![0.1, 0.2],
                comment_scores: vec![0.5, 0.7],
                cohesion_scores: vec![0.8, 0.9],
                ..TickQuality::default()
            },
        )]);
        let commits_by_tick = ct(&[(0, &[HASH_A])]);

        let input = parse_report_data(&commit_quality, &commits_by_tick, BTreeMap::new());
        let computed = compute_all_metrics(&input);

        assert_eq!(computed.time_series.len(), 1);
        assert_eq!(computed.aggregate.total_ticks, 1);
        assert_eq!(computed.aggregate.total_files_analyzed, 2);
    }

    // Mirrors reference test TestComputeAllMetrics_Empty.
    #[test]
    fn compute_all_metrics_empty() {
        let input = parse_report_data(&BTreeMap::new(), &BTreeMap::new(), BTreeMap::new());
        let computed = compute_all_metrics(&input);
        assert!(computed.time_series.is_empty());
        assert_eq!(computed.aggregate.total_ticks, 0);
    }

    // Mirrors reference test TestComputeAllMetrics_Basic (buildTestQualityReport).
    #[test]
    fn compute_all_metrics_basic() {
        let commit_quality = cq(&[
            (
                "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1",
                TickQuality {
                    complexities: vec![10.0, 20.0, 30.0],
                    cognitives: vec![5.0, 10.0, 15.0],
                    max_complexities: vec![4, 8, 12],
                    functions: vec![2, 3, 5],
                    halstead_volumes: vec![100.0, 200.0, 300.0],
                    halstead_efforts: vec![50.0, 100.0, 150.0],
                    delivered_bugs: vec![0.1, 0.2, 0.3],
                    comment_scores: vec![0.5, 0.7, 0.9],
                    doc_coverages: vec![0.4, 0.6, 0.8],
                    cohesion_scores: vec![0.8, 0.9, 1.0],
                },
            ),
            (
                "c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2",
                TickQuality {
                    complexities: vec![15.0, 25.0],
                    cognitives: vec![7.0, 12.0],
                    max_complexities: vec![6, 10],
                    functions: vec![3, 4],
                    halstead_volumes: vec![150.0, 250.0],
                    halstead_efforts: vec![75.0, 125.0],
                    delivered_bugs: vec![0.2, 0.3],
                    comment_scores: vec![0.6, 0.8],
                    doc_coverages: vec![0.5, 0.7],
                    cohesion_scores: vec![0.85, 0.95],
                },
            ),
            (
                "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3",
                TickQuality {
                    complexities: vec![5.0, 15.0, 25.0, 35.0],
                    cognitives: vec![3.0, 8.0, 13.0, 18.0],
                    max_complexities: vec![3, 7, 10, 14],
                    functions: vec![1, 3, 5, 7],
                    halstead_volumes: vec![50.0, 150.0, 250.0, 350.0],
                    halstead_efforts: vec![25.0, 75.0, 125.0, 175.0],
                    delivered_bugs: vec![0.05, 0.15, 0.25, 0.35],
                    comment_scores: vec![0.4, 0.6, 0.8, 1.0],
                    doc_coverages: vec![0.3, 0.5, 0.7, 0.9],
                    cohesion_scores: vec![0.7, 0.8, 0.9, 1.0],
                },
            ),
        ]);
        let commits_by_tick = ct(&[
            (0, &["c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1"]),
            (1, &["c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2"]),
            (2, &["c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3"]),
        ]);

        let input = parse_report_data(&commit_quality, &commits_by_tick, BTreeMap::new());
        let computed = compute_all_metrics(&input);

        assert!(!computed.time_series.is_empty());
        assert!(computed.aggregate.total_ticks > 0);
        assert!(computed.aggregate.total_files_analyzed > 0);
        assert!(computed.aggregate.complexity_median_mean > 0.0);
        assert!(computed.aggregate.complexity_p95_mean > 0.0);
        assert!(computed.aggregate.halstead_vol_median_mean > 0.0);
        assert!(computed.aggregate.total_delivered_bugs > 0.0);
    }

    // Mirrors reference test TestAggregateCommitsToTicks_* .
    #[test]
    fn aggregate_single_commit_per_tick() {
        let commit_quality = cq(&[
            (
                HASH_A,
                TickQuality {
                    complexities: vec![10.0, 20.0],
                    ..TickQuality::default()
                },
            ),
            (
                HASH_B,
                TickQuality {
                    complexities: vec![30.0],
                    ..TickQuality::default()
                },
            ),
        ]);
        let commits_by_tick = ct(&[(0, &[HASH_A]), (1, &[HASH_B])]);
        let result = aggregate_commits_to_ticks(&commit_quality, &commits_by_tick);
        assert_eq!(result.len(), 2);
        assert_eq!(result[&0].complexities.len(), 2);
        assert_eq!(result[&1].complexities.len(), 1);
    }

    #[test]
    fn aggregate_multiple_commits_per_tick() {
        let commit_quality = cq(&[
            (
                HASH_A,
                TickQuality {
                    complexities: vec![10.0],
                    halstead_volumes: vec![100.0],
                    ..TickQuality::default()
                },
            ),
            (
                HASH_B,
                TickQuality {
                    complexities: vec![20.0],
                    halstead_volumes: vec![200.0],
                    ..TickQuality::default()
                },
            ),
        ]);
        let commits_by_tick = ct(&[(0, &[HASH_A, HASH_B])]);
        let result = aggregate_commits_to_ticks(&commit_quality, &commits_by_tick);
        assert_eq!(result.len(), 1);
        assert_eq!(result[&0].complexities.len(), 2);
        assert_eq!(result[&0].halstead_volumes.len(), 2);
    }

    #[test]
    fn aggregate_empty() {
        let result = aggregate_commits_to_ticks(&BTreeMap::new(), &BTreeMap::new());
        assert!(result.is_empty());
    }

    #[test]
    fn aggregate_missing_commit() {
        let commit_quality = cq(&[(
            HASH_A,
            TickQuality {
                complexities: vec![10.0],
                ..TickQuality::default()
            },
        )]);
        let commits_by_tick =
            ct(&[(0, &[HASH_A, "cccccccccccccccccccccccccccccccccccccccc"])]);
        let result = aggregate_commits_to_ticks(&commit_quality, &commits_by_tick);
        assert_eq!(result.len(), 1);
        assert_eq!(result[&0].complexities.len(), 1);
    }

    // Mirrors reference test TestExtractCommitTimeSeries.
    #[test]
    fn extract_commit_time_series_matches_go() {
        let commit_quality = cq(&[
            (
                HASH_A,
                TickQuality {
                    complexities: vec![10.0, 20.0],
                    cognitives: vec![5.0, 8.0],
                    max_complexities: vec![12, 18],
                    functions: vec![3, 5],
                    halstead_volumes: vec![100.0, 200.0],
                    halstead_efforts: vec![50.0, 100.0],
                    delivered_bugs: vec![0.1, 0.2],
                    comment_scores: vec![0.5, 0.7],
                    doc_coverages: vec![0.6, 0.8],
                    cohesion_scores: vec![0.8, 0.9],
                },
            ),
            (
                HASH_B,
                TickQuality {
                    complexities: vec![30.0],
                    cognitives: vec![15.0],
                    max_complexities: vec![25],
                    functions: vec![7],
                    halstead_volumes: vec![300.0],
                    halstead_efforts: vec![150.0],
                    delivered_bugs: vec![0.3],
                    comment_scores: vec![0.9],
                    doc_coverages: vec![0.95],
                    cohesion_scores: vec![1.0],
                },
            ),
        ]);

        let result = extract_commit_time_series(&commit_quality);
        assert_eq!(result.len(), 2);

        let a = result[HASH_A];
        delta(a.complexity_median, 15.0, 0.01);
        assert_eq!(a.max_complexity, 18);
        assert_eq!(a.functions, 8);
        delta(a.delivered_bugs_sum, 0.3, 0.01);
        delta(a.comment_score_min, 0.5, 0.01);
        delta(a.cohesion_min, 0.8, 0.01);
        assert_eq!(a.files_analyzed, 2);

        let b = result[HASH_B];
        delta(b.complexity_median, 30.0, 0.01);
        assert_eq!(b.files_analyzed, 1);
    }

    #[test]
    fn extract_commit_time_series_empty() {
        assert!(extract_commit_time_series(&BTreeMap::new()).is_empty());
    }
}
