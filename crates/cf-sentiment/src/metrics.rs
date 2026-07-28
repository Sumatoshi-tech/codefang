//! Sentiment metric computation.
//!
//! The input model ([`ReportData`]), the per-metric computations (time series,
//! trend, low-sentiment periods, aggregate), the commit→tick aggregation, and
//! [`compute_all_metrics_with_options`].
//!
//! Commit hashes are modeled as their hex strings (the analyzer keys and
//! aggregates by the hash's string form), so the pure computation needs no git
//! dependency. The untyped report-map parsing lives in the `cf-analyze`
//! conversion hub.

use std::collections::BTreeMap;

use cf_alg_stats::to_percent;

use crate::model::{
    AggregateData, ComputedMetrics, LowSentimentPeriodData, TimeSeriesData, TrendData,
};
use crate::scorer::compute_sentiment;

/// Time-series dimension name.
pub const DIM_SENTIMENT: &str = "sentiment";

// Sentiment thresholds.

/// Positive classification threshold.
pub const SENTIMENT_POSITIVE_THRESHOLD: f64 = 0.6;
/// Negative classification threshold.
pub const SENTIMENT_NEGATIVE_THRESHOLD: f64 = 0.4;
/// Trend-direction threshold.
pub const TREND_THRESHOLD: f64 = 0.1;
/// Low-sentiment HIGH-risk threshold.
pub const LOW_SENTIMENT_RISK_THRESHOLD: f64 = 0.2;

/// Configurable thresholds for metrics computation.
#[derive(Debug, Clone, Copy)]
pub struct MetricOptions {
    /// Positive classification threshold.
    pub positive_threshold: f64,
    /// Negative classification threshold.
    pub negative_threshold: f64,
    /// Trend-direction threshold.
    pub trend_threshold: f64,
    /// Low-sentiment HIGH-risk threshold.
    pub low_sentiment_risk_thresh: f64,
}

impl Default for MetricOptions {
    fn default() -> Self {
        Self {
            positive_threshold: SENTIMENT_POSITIVE_THRESHOLD,
            negative_threshold: SENTIMENT_NEGATIVE_THRESHOLD,
            trend_threshold: TREND_THRESHOLD,
            low_sentiment_risk_thresh: LOW_SENTIMENT_RISK_THRESHOLD,
        }
    }
}

/// Inclusive tick time bounds (pre-formatted RFC3339 strings).
///
/// `cf-analyze` owns the actual formatting. An empty string means "no bound",
/// which the serializers omit (`omitempty` contract).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickBounds {
    /// RFC3339 start time, or empty for none.
    pub start_time: String,
    /// RFC3339 end time, or empty for none.
    pub end_time: String,
}

/// Parsed input data for metrics computation.
///
/// `commits_by_tick` maps a tick to the hex hashes of its commits;
/// `comments_by_commit` (consumed by [`aggregate_commits_to_ticks`]) maps a hex
/// hash to its comments.
#[derive(Debug, Clone, Default)]
pub struct ReportData {
    /// Per-tick sentiment scores.
    pub emotions_by_tick: BTreeMap<i64, f32>,
    /// Per-tick comments.
    pub comments_by_tick: BTreeMap<i64, Vec<String>>,
    /// Per-tick commit hashes (hex).
    pub commits_by_tick: BTreeMap<i64, Vec<String>>,
    /// Per-tick time bounds.
    pub tick_bounds: BTreeMap<i64, TickBounds>,
}

impl ReportData {
    /// Builds [`ReportData`] from the canonical commit-level inputs.
    ///
    /// When both `comments_by_commit` and `commits_by_tick` are non-empty, the
    /// per-tick comments and emotions are derived via
    /// [`aggregate_commits_to_ticks`]; otherwise they are left empty.
    #[must_use]
    pub fn from_commit_data(
        comments_by_commit: &BTreeMap<String, Vec<String>>,
        commits_by_tick: BTreeMap<i64, Vec<String>>,
        tick_bounds: BTreeMap<i64, TickBounds>,
    ) -> Self {
        let mut data = ReportData {
            commits_by_tick,
            tick_bounds,
            ..Default::default()
        };

        if !comments_by_commit.is_empty() && !data.commits_by_tick.is_empty() {
            let (cbt, ebt) = aggregate_commits_to_ticks(comments_by_commit, &data.commits_by_tick);
            data.comments_by_tick = cbt;
            data.emotions_by_tick = ebt;
        }

        data
    }
}

/// Groups per-commit comments into per-tick comments and emotions.
///
/// Returns empty maps when either input is empty.
#[must_use]
pub fn aggregate_commits_to_ticks(
    comments_by_commit: &BTreeMap<String, Vec<String>>,
    commits_by_tick: &BTreeMap<i64, Vec<String>>,
) -> (BTreeMap<i64, Vec<String>>, BTreeMap<i64, f32>) {
    let mut cbt: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut ebt: BTreeMap<i64, f32> = BTreeMap::new();

    if comments_by_commit.is_empty() || commits_by_tick.is_empty() {
        return (cbt, ebt);
    }

    for (&tick, hashes) in commits_by_tick {
        for hash in hashes {
            if let Some(comments) = comments_by_commit.get(hash) {
                cbt.entry(tick)
                    .or_default()
                    .extend(comments.iter().cloned());
            }
        }
        let tick_comments = cbt.get(&tick).cloned().unwrap_or_default();
        ebt.insert(tick, compute_sentiment(&tick_comments));
    }

    (cbt, ebt)
}

/// Runs all metrics with default options.
#[must_use]
pub fn compute_all_metrics(input: &ReportData) -> ComputedMetrics {
    compute_all_metrics_with_options(input, MetricOptions::default())
}

/// Runs all metrics with configurable thresholds.
#[must_use]
pub fn compute_all_metrics_with_options(
    input: &ReportData,
    opts: MetricOptions,
) -> ComputedMetrics {
    ComputedMetrics {
        time_series: compute_time_series(input, opts),
        trend: compute_trend(input, opts),
        low_sentiment_periods: compute_low_sentiment_periods(input, opts),
        aggregate: compute_aggregate(input, opts),
    }
}

/// Classifies a trend direction.
fn classify_trend_direction(start: f32, end: f32, opts: MetricOptions) -> &'static str {
    let thresh = opts.trend_threshold as f32;
    if end > start + thresh {
        "improving"
    } else if end < start - thresh {
        "declining"
    } else {
        "stable"
    }
}

/// Classifies a sentiment value.
#[must_use]
pub fn classify_sentiment(sentiment: f32, opts: MetricOptions) -> &'static str {
    if sentiment >= opts.positive_threshold as f32 {
        "positive"
    } else if sentiment <= opts.negative_threshold as f32 {
        "negative"
    } else {
        "neutral"
    }
}

/// Computes the per-tick time series.
#[must_use]
pub fn compute_time_series(input: &ReportData, opts: MetricOptions) -> Vec<TimeSeriesData> {
    // BTreeMap iteration is already tick-sorted.
    let mut result = Vec::with_capacity(input.emotions_by_tick.len());

    for (&tick, &sentiment) in &input.emotions_by_tick {
        let comment_count = input
            .comments_by_tick
            .get(&tick)
            .map_or(0, |c| c.len() as i64);
        let commit_count = input
            .commits_by_tick
            .get(&tick)
            .map_or(0, |c| c.len() as i64);

        let classification = classify_sentiment(sentiment, opts).to_string();

        let mut entry = TimeSeriesData {
            tick,
            sentiment,
            comment_count,
            commit_count,
            classification,
            ..Default::default()
        };

        if let Some(bounds) = input.tick_bounds.get(&tick) {
            entry.start_time.clone_from(&bounds.start_time);
            entry.end_time.clone_from(&bounds.end_time);
        }

        result.push(entry);
    }

    result
}

/// Computes the trend.
#[must_use]
pub fn compute_trend(input: &ReportData, opts: MetricOptions) -> TrendData {
    if input.emotions_by_tick.is_empty() {
        return TrendData::default();
    }

    let ticks: Vec<i64> = input.emotions_by_tick.keys().copied().collect();
    let start_tick = ticks[0];
    let end_tick = ticks[ticks.len() - 1];

    let (regression_start, regression_end) =
        linear_regression_endpoints(&ticks, &input.emotions_by_tick);

    let mut change_percent = 0.0;
    if regression_start > 0.0 {
        change_percent =
            to_percent(f64::from(regression_end - regression_start) / f64::from(regression_start));
    }

    let direction = classify_trend_direction(regression_start, regression_end, opts).to_string();

    TrendData {
        start_tick,
        end_tick,
        start_sentiment: regression_start,
        end_sentiment: regression_end,
        trend_direction: direction,
        change_percent,
    }
}

/// Least-squares regression endpoints.
///
/// The float arithmetic (sums in `f64`, the `n*sumXY - sumX*sumY` form, and the
/// final `f32` casts of `intercept + slope*tick`) is part of the report
/// contract: the reported 32-bit endpoints depend on this exact operation
/// order.
#[must_use]
#[allow(clippy::similar_names)] // sum_x / sum_y / sum_xy are canonical regression names
pub fn linear_regression_endpoints(ticks: &[i64], emotions: &BTreeMap<i64, f32>) -> (f32, f32) {
    let n = ticks.len() as f64;
    if n == 0.0 {
        return (0.0, 0.0);
    }
    if ticks.len() == 1 {
        let v = emotions.get(&ticks[0]).copied().unwrap_or(0.0);
        return (v, v);
    }

    let (mut sum_x, mut sum_y, mut sum_xy, mut sum_x2) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for &t in ticks {
        let x = t as f64;
        let y = f64::from(emotions.get(&t).copied().unwrap_or(0.0));
        sum_x += x;
        sum_y += y;
        sum_xy += x * y;
        sum_x2 += x * x;
    }

    let denom = n * sum_x2 - sum_x * sum_x;
    if denom == 0.0 {
        let avg = (sum_y / n) as f32;
        return (avg, avg);
    }

    let slope = (n * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n;

    let start_val = (intercept + slope * ticks[0] as f64) as f32;
    let end_val = (intercept + slope * ticks[ticks.len() - 1] as f64) as f32;
    (start_val, end_val)
}

/// Computes negative-sentiment periods.
#[must_use]
pub fn compute_low_sentiment_periods(
    input: &ReportData,
    opts: MetricOptions,
) -> Vec<LowSentimentPeriodData> {
    let mut result: Vec<LowSentimentPeriodData> = Vec::new();

    for (&tick, &sentiment) in &input.emotions_by_tick {
        if sentiment > opts.negative_threshold as f32 {
            continue;
        }

        let risk_level = if sentiment <= opts.low_sentiment_risk_thresh as f32 {
            "HIGH"
        } else {
            "MEDIUM"
        };

        let comments = input
            .comments_by_tick
            .get(&tick)
            .cloned()
            .unwrap_or_default();

        result.push(LowSentimentPeriodData {
            tick,
            sentiment,
            comments,
            risk_level: risk_level.to_string(),
        });
    }

    // Sort by sentiment ascending (worst first). The iteration order above is
    // tick-ascending (BTreeMap), giving a deterministic order for
    // equal-sentiment ties.
    result.sort_by(|a, b| {
        a.sentiment
            .partial_cmp(&b.sentiment)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    result
}

/// Computes aggregate statistics.
#[must_use]
pub fn compute_aggregate(input: &ReportData, opts: MetricOptions) -> AggregateData {
    let mut agg = AggregateData {
        total_ticks: input.emotions_by_tick.len() as i64,
        ..Default::default()
    };

    if agg.total_ticks == 0 {
        return agg;
    }

    let mut tick_sentiments: Vec<f32> = Vec::with_capacity(input.emotions_by_tick.len());

    for (&tick, &sentiment) in &input.emotions_by_tick {
        tick_sentiments.push(sentiment);

        if sentiment >= opts.positive_threshold as f32 {
            agg.positive_ticks += 1;
        } else if sentiment <= opts.negative_threshold as f32 {
            agg.negative_ticks += 1;
        } else {
            agg.neutral_ticks += 1;
        }

        if let Some(comments) = input.comments_by_tick.get(&tick) {
            agg.total_comments += comments.len() as i64;
        }
        if let Some(commits) = input.commits_by_tick.get(&tick) {
            agg.total_commits += commits.len() as i64;
        }
    }

    agg.average_sentiment = modal_f32_mean(&tick_sentiments);

    agg
}

/// Modal float32 mean over summation orders.
///
/// Go computes the aggregate average by iterating `emotionsByTick` (a Go map,
/// randomized iteration order) and accumulating in `float32`; the resulting
/// value therefore varies by a few ULPs from run to run with a strongly modal
/// distribution (the most likely rounding outcome dominates). A deterministic
/// port cannot reproduce randomized iteration, so we compute the SAME
/// quantity — the f32 sum-then-divide of the same values — under a
/// deterministically sampled set of permutations and return the modal result,
/// i.e. the value the Go binary is most likely to print. Ties break toward the
/// smaller bit pattern so the result is fully deterministic.
fn modal_f32_mean(values: &[f32]) -> f32 {
    const SAMPLES: usize = 201;

    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    let count = n as f32;

    // Cheap exit: if ascending and descending index-order sums agree, the sum
    // is order-insensitive at f32 precision and sampling is unnecessary.
    let fwd: f32 = values.iter().fold(0.0_f32, |s, &v| s + v);
    let rev: f32 = values.iter().rev().fold(0.0_f32, |s, &v| s + v);
    if fwd.to_bits() == rev.to_bits() {
        return fwd / count;
    }

    // Deterministic xorshift64* PRNG with a fixed seed: the output depends only
    // on the input values, never on wall clock or address-space randomness.
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };

    let mut perm: Vec<f32> = values.to_vec();
    let mut tallies: Vec<(u32, u32)> = Vec::new(); // (f32 bits, count)

    for _ in 0..SAMPLES {
        // Fisher-Yates shuffle.
        for i in (1..n).rev() {
            #[allow(clippy::cast_possible_truncation)]
            let j = (next() % (i as u64 + 1)) as usize;
            perm.swap(i, j);
        }
        let sum: f32 = perm.iter().fold(0.0_f32, |s, &v| s + v);
        let bits = (sum / count).to_bits();
        match tallies.iter_mut().find(|(b, _)| *b == bits) {
            Some(entry) => entry.1 += 1,
            None => tallies.push((bits, 1)),
        }
    }

    // Modal value; ties break toward the smaller bit pattern.
    let (bits, _) = tallies
        .iter()
        .copied()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .unwrap_or((0, 0));
    f32::from_bits(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOAT_DELTA: f32 = 0.01;

    fn m_f32(pairs: &[(i64, f32)]) -> BTreeMap<i64, f32> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn classify_sentiment_table() {
        let o = MetricOptions::default();
        assert_eq!(classify_sentiment(0.9, o), "positive");
        assert_eq!(classify_sentiment(0.6, o), "positive");
        assert_eq!(classify_sentiment(0.59, o), "neutral");
        assert_eq!(classify_sentiment(0.5, o), "neutral");
        assert_eq!(classify_sentiment(0.41, o), "neutral");
        assert_eq!(classify_sentiment(0.4, o), "negative");
        assert_eq!(classify_sentiment(0.2, o), "negative");
        assert_eq!(classify_sentiment(0.0, o), "negative");
    }

    #[test]
    fn time_series_empty() {
        let r = compute_time_series(&ReportData::default(), MetricOptions::default());
        assert!(r.is_empty());
    }

    #[test]
    fn time_series_single_tick() {
        let input = ReportData {
            emotions_by_tick: m_f32(&[(0, 0.8)]),
            comments_by_tick: [(0, vec!["a".into(), "b".into()])].into_iter().collect(),
            commits_by_tick: [(0, vec!["abc".into(), "def".into()])]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let r = compute_time_series(&input, MetricOptions::default());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].tick, 0);
        assert!((r[0].sentiment - 0.8).abs() < FLOAT_DELTA);
        assert_eq!(r[0].comment_count, 2);
        assert_eq!(r[0].commit_count, 2);
        assert_eq!(r[0].classification, "positive");
    }

    #[test]
    fn time_series_sorted() {
        let input = ReportData {
            emotions_by_tick: m_f32(&[(2, 0.5), (0, 0.8), (1, 0.3)]),
            ..Default::default()
        };
        let r = compute_time_series(&input, MetricOptions::default());
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].tick, 0);
        assert_eq!(r[1].tick, 1);
        assert_eq!(r[2].tick, 2);
        assert_eq!(r[0].classification, "positive");
        assert_eq!(r[1].classification, "negative");
        assert_eq!(r[2].classification, "neutral");
    }

    #[test]
    fn time_series_tick_timestamps() {
        let input = ReportData {
            emotions_by_tick: m_f32(&[(0, 0.8)]),
            tick_bounds: [(
                0,
                TickBounds {
                    start_time: "2024-01-15T10:00:00Z".into(),
                    end_time: "2024-01-16T12:00:00Z".into(),
                },
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let r = compute_time_series(&input, MetricOptions::default());
        assert_eq!(r[0].start_time, "2024-01-15T10:00:00Z");
        assert_eq!(r[0].end_time, "2024-01-16T12:00:00Z");
    }

    #[test]
    fn trend_empty() {
        let r = compute_trend(&ReportData::default(), MetricOptions::default());
        assert_eq!(r.start_tick, 0);
        assert_eq!(r.end_tick, 0);
        assert!(r.trend_direction.is_empty());
    }

    #[test]
    fn trend_single_tick() {
        let input = ReportData {
            emotions_by_tick: m_f32(&[(0, 0.5)]),
            ..Default::default()
        };
        let r = compute_trend(&input, MetricOptions::default());
        assert_eq!(r.start_tick, 0);
        assert_eq!(r.end_tick, 0);
        assert_eq!(r.trend_direction, "stable");
    }

    #[test]
    fn trend_directions() {
        for (start, end, expected) in [
            (0.3_f32, 0.8_f32, "improving"),
            (0.8, 0.3, "declining"),
            (0.5, 0.5, "stable"),
            (0.5, 0.55, "stable"),
        ] {
            let input = ReportData {
                emotions_by_tick: m_f32(&[(0, start), (5, end)]),
                ..Default::default()
            };
            let r = compute_trend(&input, MetricOptions::default());
            assert_eq!(r.trend_direction, expected, "start={start} end={end}");
        }
    }

    #[test]
    fn trend_change_percent() {
        let input = ReportData {
            emotions_by_tick: m_f32(&[(0, 0.5), (1, 0.75)]),
            ..Default::default()
        };
        let r = compute_trend(&input, MetricOptions::default());
        assert!((r.change_percent - 50.0).abs() < 0.01);
    }

    #[test]
    fn trend_zero_start() {
        let input = ReportData {
            emotions_by_tick: m_f32(&[(0, 0.0), (1, 0.5)]),
            ..Default::default()
        };
        let r = compute_trend(&input, MetricOptions::default());
        assert!(r.change_percent.abs() < 0.01);
    }

    #[test]
    fn regression_empty() {
        let (s, e) = linear_regression_endpoints(&[], &BTreeMap::new());
        assert!(s.abs() < FLOAT_DELTA);
        assert!(e.abs() < FLOAT_DELTA);
    }

    #[test]
    fn regression_single_point() {
        let emotions = m_f32(&[(5, 0.7)]);
        let (s, e) = linear_regression_endpoints(&[5], &emotions);
        assert!((s - 0.7).abs() < FLOAT_DELTA);
        assert!((e - 0.7).abs() < FLOAT_DELTA);
    }

    #[test]
    fn regression_perfect_uptrend() {
        let emotions = m_f32(&[(0, 0.2), (1, 0.4), (2, 0.6), (3, 0.8)]);
        let (s, e) = linear_regression_endpoints(&[0, 1, 2, 3], &emotions);
        assert!((s - 0.2).abs() < FLOAT_DELTA);
        assert!((e - 0.8).abs() < FLOAT_DELTA);
    }

    #[test]
    fn low_sentiment_empty() {
        let r = compute_low_sentiment_periods(&ReportData::default(), MetricOptions::default());
        assert!(r.is_empty());
    }

    #[test]
    fn low_sentiment_none() {
        let input = ReportData {
            emotions_by_tick: m_f32(&[(0, 0.8), (1, 0.5)]),
            ..Default::default()
        };
        let r = compute_low_sentiment_periods(&input, MetricOptions::default());
        assert!(r.is_empty());
    }

    #[test]
    fn low_sentiment_risk_levels() {
        for (sentiment, risk) in [
            (0.1_f32, "HIGH"),
            (0.2, "HIGH"),
            (0.3, "MEDIUM"),
            (0.4, "MEDIUM"),
        ] {
            let input = ReportData {
                emotions_by_tick: m_f32(&[(0, sentiment)]),
                comments_by_tick: [(0, vec!["c".into()])].into_iter().collect(),
                ..Default::default()
            };
            let r = compute_low_sentiment_periods(&input, MetricOptions::default());
            assert_eq!(r.len(), 1);
            assert_eq!(r[0].risk_level, risk, "sentiment={sentiment}");
            assert_eq!(r[0].comments, vec!["c".to_string()]);
        }
    }

    #[test]
    fn low_sentiment_sorted() {
        let input = ReportData {
            emotions_by_tick: m_f32(&[(0, 0.3), (1, 0.1), (2, 0.2)]),
            ..Default::default()
        };
        let r = compute_low_sentiment_periods(&input, MetricOptions::default());
        assert_eq!(r.len(), 3);
        assert!((r[0].sentiment - 0.1).abs() < FLOAT_DELTA);
        assert!((r[1].sentiment - 0.2).abs() < FLOAT_DELTA);
        assert!((r[2].sentiment - 0.3).abs() < FLOAT_DELTA);
    }

    #[test]
    fn aggregate_empty() {
        let r = compute_aggregate(&ReportData::default(), MetricOptions::default());
        assert_eq!(r.total_ticks, 0);
        assert_eq!(r.total_comments, 0);
        assert_eq!(r.total_commits, 0);
        assert!(r.average_sentiment.abs() < FLOAT_DELTA);
    }

    #[test]
    fn aggregate_all_classifications() {
        let input = ReportData {
            emotions_by_tick: m_f32(&[(0, 0.8), (1, 0.5), (2, 0.3)]),
            comments_by_tick: [(0, vec!["a".into(), "b".into()]), (1, vec!["c".into()])]
                .into_iter()
                .collect(),
            commits_by_tick: [
                (0, vec!["abc".into()]),
                (2, vec!["def".into(), "ghi".into()]),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let r = compute_aggregate(&input, MetricOptions::default());
        assert_eq!(r.total_ticks, 3);
        assert_eq!(r.total_comments, 3);
        assert_eq!(r.total_commits, 3);
        assert_eq!(r.positive_ticks, 1);
        assert_eq!(r.neutral_ticks, 1);
        assert_eq!(r.negative_ticks, 1);
        let expected = (0.8 + 0.5 + 0.3) / 3.0_f32;
        assert!((r.average_sentiment - expected).abs() < FLOAT_DELTA);
    }

    #[test]
    fn aggregate_commits_empty() {
        let (cbt, ebt) = aggregate_commits_to_ticks(&BTreeMap::new(), &BTreeMap::new());
        assert!(cbt.is_empty());
        assert!(ebt.is_empty());
    }

    #[test]
    fn aggregate_commits_single() {
        let cbc: BTreeMap<String, Vec<String>> = [
            (
                "a".to_string(),
                vec!["comment 1".into(), "comment 2".into()],
            ),
            ("b".to_string(), vec!["comment 3".into()]),
        ]
        .into_iter()
        .collect();
        let cbt_in: BTreeMap<i64, Vec<String>> = [(0, vec!["a".into()]), (1, vec!["b".into()])]
            .into_iter()
            .collect();
        let (cbt, ebt) = aggregate_commits_to_ticks(&cbc, &cbt_in);
        assert_eq!(cbt.len(), 2);
        assert_eq!(cbt[&0].len(), 2);
        assert_eq!(cbt[&1].len(), 1);
        assert_eq!(ebt.len(), 2);
    }

    #[test]
    fn compute_all_empty() {
        let r = compute_all_metrics(&ReportData::default());
        assert!(r.time_series.is_empty());
        assert!(r.low_sentiment_periods.is_empty());
        assert!(r.trend.trend_direction.is_empty());
        assert_eq!(r.aggregate.total_ticks, 0);
    }

    #[test]
    fn compute_all_from_commit_data() {
        let cbc: BTreeMap<String, Vec<String>> = [
            (
                "a".to_string(),
                vec!["good work on this".into(), "nice refactor here".into()],
            ),
            ("b".to_string(), vec!["this code is broken".into()]),
        ]
        .into_iter()
        .collect();
        let cbt: BTreeMap<i64, Vec<String>> = [(0, vec!["a".into()]), (1, vec!["b".into()])]
            .into_iter()
            .collect();
        let input = ReportData::from_commit_data(&cbc, cbt, BTreeMap::new());
        let r = compute_all_metrics(&input);
        assert_eq!(r.time_series.len(), 2);
        assert_eq!(r.aggregate.total_ticks, 2);
        assert_eq!(r.aggregate.total_comments, 3);
    }
}
