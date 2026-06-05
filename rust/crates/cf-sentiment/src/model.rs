//! Report-bearing data types for the sentiment analyzer.
//!
//! Ports the output structs from `metrics.go` (`TimeSeriesData`, `TrendData`,
//! `LowSentimentPeriodData`, `AggregateData`, `ComputedMetrics`). Each provides
//! [`ToGoValue::to_go_value`] so it serializes through [`cf_gojson`] (and, later,
//! `cf-goyaml`) byte-identically to Go's `encoding/json` / `yaml.v3`.
//!
//! # Ordering (DESIGN §2.2)
//!
//! These are all **wrapper structs**: fields are emitted in Go declaration order
//! via [`cf_gojson::GoMap::new_struct`], honoring `omitempty` on the
//! `start_time` / `end_time` string fields.
//!
//! # `float32` parity (DESIGN §2.1, §7)
//!
//! Several fields are Go `float32` (`Sentiment`, `AverageSentiment`,
//! `StartSentiment`, `EndSentiment`). Go's `encoding/json` formats `float32`
//! fields with 32-bit shortest precision (`strconv.AppendFloat(_, 'g', -1, 32)`),
//! which differs from 64-bit shortest. [`f32_float`] reproduces this by rounding
//! the value to its `f32`-shortest decimal before handing it to `cf-gojson`'s
//! 64-bit float formatter, so the rendered digits match Go.

use cf_gojson::{GoMap, GoValue};

/// Converts a value into a [`GoValue`] tree for byte-identical serialization.
pub trait ToGoValue {
    /// Builds the [`GoValue`] representation of `self`.
    fn to_go_value(&self) -> GoValue;
}

/// Builds a [`GoValue::Float`] for a Go `float32` field with 32-bit shortest
/// precision.
///
/// Go marshals `float32` via the float encoder with `bits == 32`, i.e.
/// `strconv.AppendFloat(b, float64(f), 'g', -1, 32)`. Rust's `f32` `Display`
/// produces the same shortest round-trip digit sequence; re-parsing that string
/// into `f64` yields the exact decimal value whose `f64`-shortest representation
/// equals the `f32`-shortest one, so [`cf_gojson`]'s `f64` formatter then renders
/// the digits Go would. (Residual edge cases in Go's `'g'` exponent thresholds
/// are tracked as a workspace-wide float risk in DESIGN §7.)
#[must_use]
pub fn f32_float(f: f32) -> GoValue {
    // `{}` on f32 = shortest round-trip for f32 (same digits as Go's bits=32).
    let s = format!("{f}");
    let as_f64: f64 = s.parse().unwrap_or(f64::from(f));
    GoValue::Float(as_f64)
}

/// Builds a [`GoValue::Array`] of strings.
fn string_array(items: &[String]) -> GoValue {
    GoValue::Array(items.iter().map(|s| GoValue::Str(s.clone())).collect())
}

/// Sentiment data for a single time period. Mirrors Go `TimeSeriesData`.
///
/// JSON/YAML field order: `tick, start_time (omitempty), end_time (omitempty),
/// sentiment, comment_count, commit_count, classification`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimeSeriesData {
    /// Tick index.
    pub tick: i64,
    /// RFC3339 start time, empty when absent (`omitempty`).
    pub start_time: String,
    /// RFC3339 end time, empty when absent (`omitempty`).
    pub end_time: String,
    /// Sentiment score in `[0,1]` (Go `float32`).
    pub sentiment: f32,
    /// Number of comments in the tick.
    pub comment_count: i64,
    /// Number of commits in the tick.
    pub commit_count: i64,
    /// `positive` / `neutral` / `negative`.
    pub classification: String,
}

impl ToGoValue for TimeSeriesData {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("tick", GoValue::Int(self.tick));
        if !self.start_time.is_empty() {
            m.push("start_time", GoValue::Str(self.start_time.clone()));
        }
        if !self.end_time.is_empty() {
            m.push("end_time", GoValue::Str(self.end_time.clone()));
        }
        m.push("sentiment", f32_float(self.sentiment));
        m.push("comment_count", GoValue::Int(self.comment_count));
        m.push("commit_count", GoValue::Int(self.commit_count));
        m.push("classification", GoValue::Str(self.classification.clone()));
        GoValue::Object(m)
    }
}

/// Trend information. Mirrors Go `TrendData`.
///
/// Field order: `start_tick, end_tick, start_sentiment, end_sentiment,
/// trend_direction, change_percent`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrendData {
    /// First tick.
    pub start_tick: i64,
    /// Last tick.
    pub end_tick: i64,
    /// Regression-fitted sentiment at `start_tick` (Go `float32`).
    pub start_sentiment: f32,
    /// Regression-fitted sentiment at `end_tick` (Go `float32`).
    pub end_sentiment: f32,
    /// `improving` / `declining` / `stable` (empty for an empty report).
    pub trend_direction: String,
    /// Percentage change between endpoints (Go `float64`).
    pub change_percent: f64,
}

impl ToGoValue for TrendData {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("start_tick", GoValue::Int(self.start_tick));
        m.push("end_tick", GoValue::Int(self.end_tick));
        m.push("start_sentiment", f32_float(self.start_sentiment));
        m.push("end_sentiment", f32_float(self.end_sentiment));
        m.push("trend_direction", GoValue::Str(self.trend_direction.clone()));
        m.push("change_percent", GoValue::Float(self.change_percent));
        GoValue::Object(m)
    }
}

/// A period with negative sentiment. Mirrors Go `LowSentimentPeriodData`.
///
/// Field order: `tick, sentiment, comments, risk_level`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LowSentimentPeriodData {
    /// Tick index.
    pub tick: i64,
    /// Sentiment score (Go `float32`).
    pub sentiment: f32,
    /// Comments observed in this period.
    pub comments: Vec<String>,
    /// `HIGH` or `MEDIUM`.
    pub risk_level: String,
}

impl ToGoValue for LowSentimentPeriodData {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("tick", GoValue::Int(self.tick));
        m.push("sentiment", f32_float(self.sentiment));
        // Go has no omitempty on `comments`: a nil slice marshals to JSON `null`.
        if self.comments.is_empty() {
            m.push("comments", GoValue::Null);
        } else {
            m.push("comments", string_array(&self.comments));
        }
        m.push("risk_level", GoValue::Str(self.risk_level.clone()));
        GoValue::Object(m)
    }
}

/// Summary statistics. Mirrors Go `AggregateData`.
///
/// Field order: `total_ticks, total_comments, total_commits, average_sentiment,
/// positive_ticks, neutral_ticks, negative_ticks`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AggregateData {
    /// Total number of ticks.
    pub total_ticks: i64,
    /// Total comment count across ticks.
    pub total_comments: i64,
    /// Total commit count across ticks.
    pub total_commits: i64,
    /// Mean sentiment across ticks (Go `float32`).
    pub average_sentiment: f32,
    /// Number of positive ticks.
    pub positive_ticks: i64,
    /// Number of neutral ticks.
    pub neutral_ticks: i64,
    /// Number of negative ticks.
    pub negative_ticks: i64,
}

impl ToGoValue for AggregateData {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("total_ticks", GoValue::Int(self.total_ticks));
        m.push("total_comments", GoValue::Int(self.total_comments));
        m.push("total_commits", GoValue::Int(self.total_commits));
        m.push("average_sentiment", f32_float(self.average_sentiment));
        m.push("positive_ticks", GoValue::Int(self.positive_ticks));
        m.push("neutral_ticks", GoValue::Int(self.neutral_ticks));
        m.push("negative_ticks", GoValue::Int(self.negative_ticks));
        GoValue::Object(m)
    }
}

/// All computed metric results. Mirrors Go `ComputedMetrics`.
///
/// Field order: `time_series, trend, low_sentiment_periods, aggregate`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    /// Per-tick time series.
    pub time_series: Vec<TimeSeriesData>,
    /// Trend summary.
    pub trend: TrendData,
    /// Negative-sentiment periods.
    pub low_sentiment_periods: Vec<LowSentimentPeriodData>,
    /// Aggregate statistics.
    pub aggregate: AggregateData,
}

/// Analyzer name reported by `ComputedMetrics.AnalyzerName`.
pub const ANALYZER_NAME_SENTIMENT: &str = "sentiment";

impl ComputedMetrics {
    /// Returns the analyzer name. Mirrors `ComputedMetrics.AnalyzerName`.
    #[must_use]
    pub fn analyzer_name(&self) -> &'static str {
        ANALYZER_NAME_SENTIMENT
    }
}

impl ToGoValue for ComputedMetrics {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push(
            "time_series",
            GoValue::Array(self.time_series.iter().map(ToGoValue::to_go_value).collect()),
        );
        m.push("trend", self.trend.to_go_value());
        // `low_sentiment_periods` has no omitempty: nil slice => JSON null.
        if self.low_sentiment_periods.is_empty() {
            m.push("low_sentiment_periods", GoValue::Null);
        } else {
            m.push(
                "low_sentiment_periods",
                GoValue::Array(
                    self.low_sentiment_periods
                        .iter()
                        .map(ToGoValue::to_go_value)
                        .collect(),
                ),
            );
        }
        m.push("aggregate", self.aggregate.to_go_value());
        GoValue::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marshal(v: &impl ToGoValue) -> String {
        String::from_utf8(cf_gojson::marshal(&v.to_go_value())).unwrap()
    }

    #[test]
    fn time_series_field_order_and_omitempty() {
        let ts = TimeSeriesData {
            tick: 0,
            sentiment: 0.5,
            comment_count: 2,
            commit_count: 1,
            classification: "neutral".into(),
            ..Default::default()
        };
        // start_time/end_time omitted; field order preserved.
        assert_eq!(
            marshal(&ts),
            r#"{"tick":0,"sentiment":0.5,"comment_count":2,"commit_count":1,"classification":"neutral"}"#
        );
    }

    #[test]
    fn time_series_with_bounds() {
        let ts = TimeSeriesData {
            tick: 3,
            start_time: "2024-01-15T10:00:00Z".into(),
            end_time: "2024-01-16T12:00:00Z".into(),
            sentiment: 0.8,
            comment_count: 0,
            commit_count: 0,
            classification: "positive".into(),
        };
        assert_eq!(
            marshal(&ts),
            r#"{"tick":3,"start_time":"2024-01-15T10:00:00Z","end_time":"2024-01-16T12:00:00Z","sentiment":0.8,"comment_count":0,"commit_count":0,"classification":"positive"}"#
        );
    }

    #[test]
    fn f32_float_renders_short() {
        // 0.6 as f32 must render "0.6", not the f64-promoted long form.
        assert_eq!(
            String::from_utf8(cf_gojson::marshal(&f32_float(0.6))).unwrap(),
            "0.6"
        );
        assert_eq!(
            String::from_utf8(cf_gojson::marshal(&f32_float(0.5))).unwrap(),
            "0.5"
        );
    }

    #[test]
    fn low_sentiment_nil_comments_is_null() {
        let p = LowSentimentPeriodData {
            tick: 1,
            sentiment: 0.1,
            comments: vec![],
            risk_level: "HIGH".into(),
        };
        assert_eq!(
            marshal(&p),
            r#"{"tick":1,"sentiment":0.1,"comments":null,"risk_level":"HIGH"}"#
        );
    }

    #[test]
    fn aggregate_field_order() {
        let a = AggregateData {
            total_ticks: 5,
            average_sentiment: 0.6,
            ..Default::default()
        };
        assert_eq!(
            marshal(&a),
            r#"{"total_ticks":5,"total_comments":0,"total_commits":0,"average_sentiment":0.6,"positive_ticks":0,"neutral_ticks":0,"negative_ticks":0}"#
        );
    }
}
