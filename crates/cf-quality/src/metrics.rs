//! Quality statistics: per-tick stats, time-series and aggregate computation.
//!
//! All numeric routines delegate to [`cf_alg_stats`] (the shared statistics
//! kernel).
//!
//! # Tick semantics
//!
//! A tick enters the time series and the aggregate only when at least one
//! file was analyzed in it ([`compute_all_metrics`]); the cohesion aggregates
//! (`cohesion_mean_mean`, `min_cohesion`) are taken over the ticks that hold
//! at least one cohesion sample, so `min <= mean` always holds. Within a
//! tick, `cohesion_mean` / `cohesion_min` are `0` when no file has a function
//! (no cohesion data).
//!
//! # Serialization
//!
//! [`TickStats`], [`TimeSeriesEntry`], [`AggregateData`] and [`ComputedMetrics`]
//! serialize their fields in **declaration order**, honoring `omitempty` on
//! `start_time` / `end_time`, through the fixed-order `GoMap` builder in
//! [`crate::serialize`].

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
/// Returns the zero value when no files were analyzed; `cohesion_mean` /
/// `cohesion_min` are `0` when the tick holds no cohesion sample.
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

/// Runs all quality metrics over the ticks that analyzed at least one file,
/// in ascending tick order. The aggregate means are the means of the per-tick
/// values; `min_comment_score` is the minimum over those ticks and
/// `min_cohesion` / `cohesion_mean_mean` are taken over the ticks that hold a
/// cohesion sample (`0` when no tick does).
#[must_use]
pub fn compute_all_metrics(input: &ReportData) -> ComputedMetrics {
    let analyzed: Vec<(i64, &TickQuality)> = input
        .tick_quality
        .iter()
        .filter(|(_, tq)| tq.files_analyzed() > 0)
        .map(|(tick, tq)| (*tick, tq))
        .collect();
    let mut time_series = Vec::with_capacity(analyzed.len());

    let mut complexity_medians = Vec::with_capacity(analyzed.len());
    let mut complexity_p95s = Vec::with_capacity(analyzed.len());
    let mut halstead_medians = Vec::with_capacity(analyzed.len());
    let mut comment_means = Vec::with_capacity(analyzed.len());
    let mut comment_mins = Vec::with_capacity(analyzed.len());
    let mut cohesion_means = Vec::with_capacity(analyzed.len());
    let mut cohesion_mins = Vec::with_capacity(analyzed.len());

    let mut total_files: i64 = 0;
    let mut total_bugs: f64 = 0.0;

    for (tick, tq) in analyzed {
        let ts = compute_tick_stats(tq);
        let bounds = input.tick_bounds.get(&tick).cloned().unwrap_or_default();
        time_series.push(TimeSeriesEntry {
            tick,
            start_time: bounds.start_time,
            end_time: bounds.end_time,
            stats: ts,
        });

        complexity_medians.push(ts.complexity_median);
        complexity_p95s.push(ts.complexity_p95);
        halstead_medians.push(ts.halstead_vol_median);
        comment_means.push(ts.comment_score_mean);
        comment_mins.push(ts.comment_score_min);
        if !tq.cohesion_scores.is_empty() {
            cohesion_means.push(ts.cohesion_mean);
            cohesion_mins.push(ts.cohesion_min);
        }

        total_files += ts.files_analyzed;
        total_bugs += ts.delivered_bugs_sum;
    }

    let aggregate = AggregateData {
        total_ticks: time_series.len() as i64,
        total_files_analyzed: total_files,
        complexity_median_mean: stats::mean(&complexity_medians),
        complexity_p95_mean: stats::mean(&complexity_p95s),
        halstead_vol_median_mean: stats::mean(&halstead_medians),
        total_delivered_bugs: total_bugs,
        comment_score_mean_mean: stats::mean(&comment_means),
        min_comment_score: stats::min(&comment_mins),
        cohesion_mean_mean: stats::mean(&cohesion_means),
        min_cohesion: stats::min(&cohesion_mins),
    };

    ComputedMetrics {
        time_series,
        aggregate,
    }
}

/// Per-commit summary used by the `--format timeseries` paths.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(a: f64, b: f64, d: f64) {
        assert!((a - b).abs() <= d, "expected {b} got {a} (delta {d})");
    }

    /// A [`ReportData`] over `(tick, quality)` pairs without bounds.
    fn report(ticks: Vec<(i64, TickQuality)>) -> ReportData {
        ReportData {
            tick_quality: ticks.into_iter().collect(),
            tick_bounds: BTreeMap::new(),
        }
    }

    #[test]
    fn compute_tick_stats_over_five_files() {
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

    #[test]
    fn compute_tick_stats_zero_files() {
        let ts = compute_tick_stats(&TickQuality::default());
        assert_eq!(ts.files_analyzed, 0);
        delta(ts.complexity_mean, 0.0, 0.01);
        delta(ts.complexity_median, 0.0, 0.01);
    }

    #[test]
    fn compute_all_metrics_over_two_ticks() {
        let input = report(vec![
            (
                0,
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
                1,
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
        let computed = compute_all_metrics(&input);

        assert_eq!(computed.time_series.len(), 2);
        let agg = computed.aggregate;
        assert_eq!(agg.total_ticks, 2);
        assert_eq!(agg.total_files_analyzed, 3);
        delta(agg.complexity_median_mean, 22.5, 1e-9);
        delta(agg.total_delivered_bugs, 0.6, 1e-9);
        delta(agg.min_comment_score, 0.5, 1e-9);
        delta(agg.min_cohesion, 0.8, 1e-9);
        delta(agg.cohesion_mean_mean, (0.85 + 1.0) / 2.0, 1e-9);
    }

    #[test]
    fn compute_all_metrics_carries_tick_bounds() {
        let mut input = report(vec![(
            3,
            TickQuality {
                complexities: vec![1.0],
                ..TickQuality::default()
            },
        )]);
        input.tick_bounds.insert(
            3,
            TickBounds {
                start_time: "2024-01-01T00:00:00Z".into(),
                end_time: "2024-01-01T12:00:00Z".into(),
            },
        );
        let computed = compute_all_metrics(&input);
        assert_eq!(computed.time_series[0].tick, 3);
        assert_eq!(computed.time_series[0].start_time, "2024-01-01T00:00:00Z");
        assert_eq!(computed.time_series[0].end_time, "2024-01-01T12:00:00Z");
    }

    #[test]
    fn compute_all_metrics_empty() {
        let computed = compute_all_metrics(&report(Vec::new()));
        assert!(computed.time_series.is_empty());
        assert_eq!(computed.aggregate, AggregateData::default());
    }

    #[test]
    fn commit_summary_of_two_files() {
        let tq = TickQuality {
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
        };
        let summary = commit_summary(&tq);
        delta(summary.complexity_median, 15.0, 0.01);
        delta(summary.cognitive_median, 6.5, 0.01);
        assert_eq!(summary.max_complexity, 18);
        assert_eq!(summary.functions, 8);
        delta(summary.halstead_effort_median, 75.0, 0.01);
        delta(summary.delivered_bugs_sum, 0.3, 0.01);
        delta(summary.comment_score_min, 0.5, 0.01);
        delta(summary.doc_coverage_mean, 0.7, 0.01);
        delta(summary.cohesion_min, 0.8, 0.01);
        assert_eq!(summary.files_analyzed, 2);
    }

    /// A tick whose commits yielded no analyzable file carries no statistics:
    /// it is absent from the time series and never drags an aggregate mean
    /// below the aggregate minimum.
    #[test]
    fn zero_file_ticks_are_excluded_from_series_and_aggregate() {
        let mut tick_quality = BTreeMap::new();
        tick_quality.insert(
            0,
            TickQuality {
                complexities: vec![2.0],
                comment_scores: vec![1.0],
                doc_coverages: vec![1.0],
                cohesion_scores: vec![1.0],
                ..TickQuality::default()
            },
        );
        tick_quality.insert(1, TickQuality::default());
        tick_quality.insert(
            2,
            TickQuality {
                complexities: vec![4.0],
                comment_scores: vec![1.0],
                doc_coverages: vec![1.0],
                cohesion_scores: vec![1.0],
                ..TickQuality::default()
            },
        );
        let input = ReportData {
            tick_quality,
            tick_bounds: BTreeMap::new(),
        };
        let computed = compute_all_metrics(&input);
        let ticks: Vec<i64> = computed.time_series.iter().map(|e| e.tick).collect();
        assert_eq!(ticks, vec![0, 2], "{:?}", computed.time_series);
        let agg = computed.aggregate;
        assert_eq!(agg.total_ticks, 2);
        assert_eq!(agg.total_files_analyzed, 2);
        delta(agg.complexity_median_mean, 3.0, 1e-9);
        delta(agg.cohesion_mean_mean, 1.0, 1e-9);
        delta(agg.min_cohesion, 1.0, 1e-9);
        delta(agg.comment_score_mean_mean, 1.0, 1e-9);
        assert!(agg.min_cohesion <= agg.cohesion_mean_mean);
        assert!(agg.min_comment_score <= agg.comment_score_mean_mean);
    }

    /// Cohesion aggregates are computed over the ticks that have cohesion
    /// data: a tick of function-free files (no cohesion sample) neither
    /// lowers `cohesion_mean_mean` nor sets `min_cohesion` to 0.
    #[test]
    fn ticks_without_cohesion_samples_do_not_zero_the_cohesion_aggregate() {
        let mut tick_quality = BTreeMap::new();
        tick_quality.insert(
            0,
            TickQuality {
                complexities: vec![3.0],
                cohesion_scores: vec![0.75],
                ..TickQuality::default()
            },
        );
        tick_quality.insert(
            1,
            TickQuality {
                complexities: vec![0.0],
                ..TickQuality::default()
            },
        );
        let input = ReportData {
            tick_quality,
            tick_bounds: BTreeMap::new(),
        };
        let agg = compute_all_metrics(&input).aggregate;
        assert_eq!(agg.total_ticks, 2);
        delta(agg.cohesion_mean_mean, 0.75, 1e-9);
        delta(agg.min_cohesion, 0.75, 1e-9);
    }
}
