//! Data types for the temporal anomaly analyzer.
//!
//! Ports the structs declared across `internal/analyzers/anomaly/analyzer.go`
//! and `metrics.go`. Each report-bearing type provides [`ToGoValue::to_go_value`]
//! so it can be serialized through [`cf_gojson`] (and, later, `cf-goyaml`) with
//! byte-identity to Go's `encoding/json` / `yaml.v3`.
//!
//! # Ordering rules (DESIGN §2.2)
//!
//! Wrapper structs serialize their fields in **declaration order** (via
//! [`cf_gojson::GoMap::new_struct`]), honoring `omitempty`. Dynamic report maps
//! such as `commit_metrics` (Go `map[string]*CommitAnomalyData`) and the
//! per-tick `languages` map serialize with **byte-sorted keys** (via
//! [`cf_gojson::GoMap::new_map`]).

use std::collections::BTreeMap;

use cf_gojson::{GoMap, GoValue};

/// Converts a value into a [`GoValue`] tree for byte-identical serialization.
pub trait ToGoValue {
    /// Builds the [`GoValue`] representation of `self`.
    fn to_go_value(&self) -> GoValue;
}

/// Raw metrics collected for a single tick.
///
/// Mirrors Go `TickMetrics` (analyzer.go). This is an internal accumulation
/// type — it is never serialized directly, so it carries no JSON tags. Maps use
/// [`BTreeMap`] so iteration order is deterministic (Go relies on map length and
/// `mapx.MergeAdditive`, never on iteration order here).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TickMetrics {
    /// Number of files changed across the tick.
    pub files_changed: i64,
    /// Lines added across the tick.
    pub lines_added: i64,
    /// Lines removed across the tick.
    pub lines_removed: i64,
    /// Net churn (`lines_added - lines_removed`).
    pub net_churn: i64,
    /// Files changed in the tick (concatenated across commits, like Go).
    pub files: Vec<String>,
    /// Language name -> file count for this tick.
    pub languages: BTreeMap<String, i64>,
    /// Unique author IDs seen in this tick.
    pub author_ids: std::collections::BTreeSet<i64>,
}

/// Raw metrics for a single commit.
///
/// Mirrors Go `CommitAnomalyData` (analyzer.go). JSON field order and
/// `omitempty` semantics match the Go struct tags exactly:
/// `files_changed, lines_added, lines_removed, net_churn,
/// files (omitempty), languages (omitempty), author_id`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommitAnomalyData {
    /// Files changed in the commit.
    pub files_changed: i64,
    /// Lines added in the commit.
    pub lines_added: i64,
    /// Lines removed in the commit.
    pub lines_removed: i64,
    /// Net churn (`lines_added - lines_removed`).
    pub net_churn: i64,
    /// File names touched by the commit (`omitempty`).
    pub files: Vec<String>,
    /// Language name -> file count (`omitempty`).
    pub languages: BTreeMap<String, i64>,
    /// Author identity ID.
    pub author_id: i64,
}

impl ToGoValue for CommitAnomalyData {
    fn to_go_value(&self) -> GoValue {
        // Struct origin: fields emitted in Go declaration order.
        let mut m = GoMap::new_struct();
        m.push("files_changed", GoValue::Int(self.files_changed));
        m.push("lines_added", GoValue::Int(self.lines_added));
        m.push("lines_removed", GoValue::Int(self.lines_removed));
        m.push("net_churn", GoValue::Int(self.net_churn));
        // Go `omitempty` on a nil/empty slice => key omitted entirely.
        if !self.files.is_empty() {
            m.push("files", string_array(&self.files));
        }
        // Go `omitempty` on a nil/empty map => key omitted entirely.
        if !self.languages.is_empty() {
            m.push("languages", lang_map(&self.languages));
        }
        m.push("author_id", GoValue::Int(self.author_id));
        GoValue::Object(m)
    }
}

/// Per-metric Z-scores for a single tick.
///
/// Mirrors Go `ZScoreSet` (metrics.go). Field order:
/// `net_churn, files_changed, lines_added, lines_removed,
/// language_diversity, author_count`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ZScoreSet {
    /// Net-churn Z-score.
    pub net_churn: f64,
    /// Files-changed Z-score.
    pub files_changed: f64,
    /// Lines-added Z-score.
    pub lines_added: f64,
    /// Lines-removed Z-score.
    pub lines_removed: f64,
    /// Language-diversity Z-score.
    pub language_diversity: f64,
    /// Author-count Z-score.
    pub author_count: f64,
}

impl ZScoreSet {
    /// Returns the maximum absolute Z-score across all metrics.
    ///
    /// Mirrors Go `ZScoreSet.MaxAbs` — a 6-way `max` over `math.Abs`.
    #[must_use]
    pub fn max_abs(&self) -> f64 {
        let vals = [
            self.net_churn.abs(),
            self.files_changed.abs(),
            self.lines_added.abs(),
            self.lines_removed.abs(),
            self.language_diversity.abs(),
            self.author_count.abs(),
        ];
        // Go's variadic `max` over float64 returns the largest; none are NaN here.
        vals.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    }
}

impl ToGoValue for ZScoreSet {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("net_churn", GoValue::Float(self.net_churn));
        m.push("files_changed", GoValue::Float(self.files_changed));
        m.push("lines_added", GoValue::Float(self.lines_added));
        m.push("lines_removed", GoValue::Float(self.lines_removed));
        m.push("language_diversity", GoValue::Float(self.language_diversity));
        m.push("author_count", GoValue::Float(self.author_count));
        GoValue::Object(m)
    }
}

/// Raw integer metric values for a single tick.
///
/// Mirrors Go `RawMetrics` (metrics.go). Field order:
/// `files_changed, lines_added, lines_removed, net_churn,
/// language_diversity, author_count`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RawMetrics {
    /// Files changed.
    pub files_changed: i64,
    /// Lines added.
    pub lines_added: i64,
    /// Lines removed.
    pub lines_removed: i64,
    /// Net churn.
    pub net_churn: i64,
    /// Distinct languages.
    pub language_diversity: i64,
    /// Distinct authors.
    pub author_count: i64,
}

impl ToGoValue for RawMetrics {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("files_changed", GoValue::Int(self.files_changed));
        m.push("lines_added", GoValue::Int(self.lines_added));
        m.push("lines_removed", GoValue::Int(self.lines_removed));
        m.push("net_churn", GoValue::Int(self.net_churn));
        m.push("language_diversity", GoValue::Int(self.language_diversity));
        m.push("author_count", GoValue::Int(self.author_count));
        GoValue::Object(m)
    }
}

/// A detected anomaly at a specific tick.
///
/// Mirrors Go `Record` (metrics.go). Field order:
/// `tick, z_scores, max_abs_z_score, metrics, files`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    /// Time-period index where the anomaly was detected.
    pub tick: i64,
    /// Per-metric Z-scores.
    pub z_scores: ZScoreSet,
    /// Maximum absolute Z-score across metrics (severity).
    pub max_abs_z_score: f64,
    /// Raw metric values for the tick.
    pub metrics: RawMetrics,
    /// Files changed in the anomalous tick.
    pub files: Vec<String>,
}

impl ToGoValue for Record {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("tick", GoValue::Int(self.tick));
        m.push("z_scores", self.z_scores.to_go_value());
        m.push("max_abs_z_score", GoValue::Float(self.max_abs_z_score));
        m.push("metrics", self.metrics.to_go_value());
        // `files` has no `omitempty`: Go emits `null` for a nil slice.
        m.push("files", string_array_or_null(&self.files));
        GoValue::Object(m)
    }
}

/// Summary statistics for the anomaly analysis.
///
/// Mirrors Go `AggregateData` (metrics.go). The `threshold` field is a Go
/// `float32`; serialization promotes it to `f64` exactly as Go does when the
/// `float32` flows through `encoding/json`'s float encoder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AggregateData {
    /// Number of time periods analyzed.
    pub total_ticks: i64,
    /// Number of ticks flagged as anomalous.
    pub total_anomalies: i64,
    /// Percentage of ticks that are anomalous.
    pub anomaly_rate: f64,
    /// Z-score threshold used (Go `float32`).
    pub threshold: f32,
    /// Sliding window size used.
    pub window_size: i64,
    /// Mean net churn across ticks.
    pub churn_mean: f64,
    /// Standard deviation of net churn.
    pub churn_stddev: f64,
    /// Mean files changed per tick.
    pub files_mean: f64,
    /// Standard deviation of files changed.
    pub files_stddev: f64,
    /// Mean language diversity per tick.
    pub lang_diversity_mean: f64,
    /// Standard deviation of language diversity.
    pub lang_diversity_stddev: f64,
    /// Mean author count per tick.
    pub author_count_mean: f64,
    /// Standard deviation of author count.
    pub author_count_stddev: f64,
}

impl ToGoValue for AggregateData {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("total_ticks", GoValue::Int(self.total_ticks));
        m.push("total_anomalies", GoValue::Int(self.total_anomalies));
        m.push("anomaly_rate", GoValue::Float(self.anomaly_rate));
        m.push("threshold", GoValue::Float(f64::from(self.threshold)));
        m.push("window_size", GoValue::Int(self.window_size));
        m.push("churn_mean", GoValue::Float(self.churn_mean));
        m.push("churn_stddev", GoValue::Float(self.churn_stddev));
        m.push("files_mean", GoValue::Float(self.files_mean));
        m.push("files_stddev", GoValue::Float(self.files_stddev));
        m.push("lang_diversity_mean", GoValue::Float(self.lang_diversity_mean));
        m.push("lang_diversity_stddev", GoValue::Float(self.lang_diversity_stddev));
        m.push("author_count_mean", GoValue::Float(self.author_count_mean));
        m.push("author_count_stddev", GoValue::Float(self.author_count_stddev));
        GoValue::Object(m)
    }
}

/// Per-tick entry for the time-series output.
///
/// Mirrors Go `TimeSeriesEntry` (metrics.go). Field order:
/// `tick, start_time (omitempty), end_time (omitempty), metrics,
/// is_anomaly, churn_z_score, language_diversity, author_count`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimeSeriesEntry {
    /// Time-period index.
    pub tick: i64,
    /// Tick start time, RFC3339 (`omitempty`).
    pub start_time: String,
    /// Tick end time, RFC3339 (`omitempty`).
    pub end_time: String,
    /// Raw metric values.
    pub metrics: RawMetrics,
    /// Whether the tick was flagged as anomalous.
    pub is_anomaly: bool,
    /// Net-churn Z-score for the tick.
    pub churn_z_score: f64,
    /// Distinct languages this tick.
    pub language_diversity: i64,
    /// Distinct authors this tick.
    pub author_count: i64,
}

impl ToGoValue for TimeSeriesEntry {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("tick", GoValue::Int(self.tick));
        if !self.start_time.is_empty() {
            m.push("start_time", GoValue::Str(self.start_time.clone()));
        }
        if !self.end_time.is_empty() {
            m.push("end_time", GoValue::Str(self.end_time.clone()));
        }
        m.push("metrics", self.metrics.to_go_value());
        m.push("is_anomaly", GoValue::Bool(self.is_anomaly));
        m.push("churn_z_score", GoValue::Float(self.churn_z_score));
        m.push("language_diversity", GoValue::Int(self.language_diversity));
        m.push("author_count", GoValue::Int(self.author_count));
        GoValue::Object(m)
    }
}

/// An anomaly detected on an external analyzer's time-series dimension.
///
/// Mirrors Go `ExternalAnomaly` (metrics.go). Field order:
/// `source, dimension, tick, z_score, raw_value`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExternalAnomaly {
    /// Source analyzer ID.
    pub source: String,
    /// Dimension name.
    pub dimension: String,
    /// Tick index.
    pub tick: i64,
    /// Z-score at the tick.
    pub z_score: f64,
    /// Raw value at the tick.
    pub raw_value: f64,
}

impl ToGoValue for ExternalAnomaly {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("source", GoValue::Str(self.source.clone()));
        m.push("dimension", GoValue::Str(self.dimension.clone()));
        m.push("tick", GoValue::Int(self.tick));
        m.push("z_score", GoValue::Float(self.z_score));
        m.push("raw_value", GoValue::Float(self.raw_value));
        GoValue::Object(m)
    }
}

/// Summary of anomaly detection for one external dimension.
///
/// Mirrors Go `ExternalSummary` (metrics.go). Field order:
/// `source, dimension, mean, stddev, anomalies, highest_z`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExternalSummary {
    /// Source analyzer ID.
    pub source: String,
    /// Dimension name.
    pub dimension: String,
    /// Mean of the dimension's values.
    pub mean: f64,
    /// Standard deviation of the dimension's values.
    pub stddev: f64,
    /// Count of anomalies detected in this dimension.
    pub anomalies: i64,
    /// Highest absolute Z-score observed.
    pub highest_z: f64,
}

impl ToGoValue for ExternalSummary {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("source", GoValue::Str(self.source.clone()));
        m.push("dimension", GoValue::Str(self.dimension.clone()));
        m.push("mean", GoValue::Float(self.mean));
        m.push("stddev", GoValue::Float(self.stddev));
        m.push("anomalies", GoValue::Int(self.anomalies));
        m.push("highest_z", GoValue::Float(self.highest_z));
        GoValue::Object(m)
    }
}

/// All computed metric results for the anomaly analyzer.
///
/// Mirrors Go `ComputedMetrics` (metrics.go). Field order:
/// `anomalies, time_series, aggregate,
/// external_anomalies (omitempty), external_summaries (omitempty)`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    /// Detected anomalies, sorted by severity.
    pub anomalies: Vec<Record>,
    /// Annotated per-tick time series.
    pub time_series: Vec<TimeSeriesEntry>,
    /// Aggregate statistics.
    pub aggregate: AggregateData,
    /// Cross-analyzer anomalies (`omitempty`).
    pub external_anomalies: Vec<ExternalAnomaly>,
    /// Cross-analyzer summaries (`omitempty`).
    pub external_summaries: Vec<ExternalSummary>,
}

impl ComputedMetrics {
    /// The analyzer name, mirroring Go `ComputedMetrics.AnalyzerName`.
    #[must_use]
    pub fn analyzer_name(&self) -> &'static str {
        crate::ANALYZER_NAME_ANOMALY
    }
}

impl ToGoValue for ComputedMetrics {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        // Go emits `null` for a nil slice and `[]` for an initialized-but-empty
        // slice. `computeList` returns `input.Anomalies`, which is whatever
        // `buildRecords` produced: a nil `[]Record` (`var anomalies []Record`)
        // when no tick is flagged, so an empty anomaly set marshals as `null`.
        // `computeTimeSeries` always allocates `make([]TimeSeriesEntry, …)`, a
        // non-nil slice, so an empty series marshals as `[]`.
        m.push("anomalies", records_array_or_null(&self.anomalies));
        m.push("time_series", time_series_array(&self.time_series));
        m.push("aggregate", self.aggregate.to_go_value());
        if !self.external_anomalies.is_empty() {
            m.push("external_anomalies", external_anomalies_array(&self.external_anomalies));
        }
        if !self.external_summaries.is_empty() {
            m.push("external_summaries", external_summaries_array(&self.external_summaries));
        }
        GoValue::Object(m)
    }
}

// --- serialization helpers ---

/// Encodes a `[]string` that Go would emit as a JSON array, never `null`
/// (used where `omitempty` already guarded against the empty case).
fn string_array(items: &[String]) -> GoValue {
    GoValue::Array(items.iter().cloned().map(GoValue::Str).collect())
}

/// Encodes a `[]string` mirroring Go's nil-slice → `null` behavior for fields
/// without `omitempty` (e.g. `Record.files`).
fn string_array_or_null(items: &[String]) -> GoValue {
    if items.is_empty() {
        GoValue::Null
    } else {
        string_array(items)
    }
}

/// Encodes a Go `map[string]int` with byte-sorted keys (map-origin object).
fn lang_map(langs: &BTreeMap<String, i64>) -> GoValue {
    let mut m = GoMap::new_map();
    for (k, v) in langs {
        m.push(k.clone(), GoValue::Int(*v));
    }
    GoValue::Object(m)
}

fn records_array(items: &[Record]) -> GoValue {
    GoValue::Array(items.iter().map(ToGoValue::to_go_value).collect())
}

/// Encodes a `[]Record` mirroring Go's nil-slice → `null` behavior. `computeList`
/// returns the nil slice `buildRecords` yields when no anomaly is flagged, so an
/// empty list marshals as `null` (not `[]`).
fn records_array_or_null(items: &[Record]) -> GoValue {
    if items.is_empty() {
        GoValue::Null
    } else {
        records_array(items)
    }
}

fn time_series_array(items: &[TimeSeriesEntry]) -> GoValue {
    GoValue::Array(items.iter().map(ToGoValue::to_go_value).collect())
}

fn external_anomalies_array(items: &[ExternalAnomaly]) -> GoValue {
    GoValue::Array(items.iter().map(ToGoValue::to_go_value).collect())
}

fn external_summaries_array(items: &[ExternalSummary]) -> GoValue {
    GoValue::Array(items.iter().map(ToGoValue::to_go_value).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::Encoder;

    #[test]
    fn max_abs_picks_largest_magnitude() {
        // Mirrors Go TestZScoreSet_MaxAbs.
        let zs = ZScoreSet {
            net_churn: 1.5,
            files_changed: -3.0,
            lines_added: 2.0,
            lines_removed: 0.5,
            language_diversity: 1.0,
            author_count: 0.3,
        };
        assert!((zs.max_abs() - 3.0).abs() < 1e-9);

        let zs2 = ZScoreSet {
            net_churn: 1.0,
            files_changed: 1.0,
            lines_added: 1.0,
            lines_removed: 1.0,
            language_diversity: -5.0,
            author_count: 0.5,
        };
        assert!((zs2.max_abs() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn commit_data_omits_empty_files_and_languages() {
        let cm = CommitAnomalyData {
            files_changed: 2,
            lines_added: 10,
            lines_removed: 3,
            net_churn: 7,
            ..Default::default()
        };
        let json = Encoder::marshal().encode_to_string(&cm.to_go_value());
        // No `files`/`languages` keys when empty; integer fields present.
        assert_eq!(
            json,
            r#"{"files_changed":2,"lines_added":10,"lines_removed":3,"net_churn":7,"author_id":0}"#
        );
    }

    #[test]
    fn commit_data_languages_byte_sorted() {
        let mut languages = BTreeMap::new();
        languages.insert("Python".to_string(), 1);
        languages.insert("Go".to_string(), 3);
        let cm = CommitAnomalyData {
            files_changed: 1,
            lines_added: 5,
            lines_removed: 0,
            net_churn: 5,
            files: vec!["main.go".to_string()],
            languages,
            author_id: 42,
        };
        let json = Encoder::marshal().encode_to_string(&cm.to_go_value());
        // "Go" < "Python" by UTF-8 byte order.
        assert_eq!(
            json,
            r#"{"files_changed":1,"lines_added":5,"lines_removed":0,"net_churn":5,"files":["main.go"],"languages":{"Go":3,"Python":1},"author_id":42}"#
        );
    }

    #[test]
    fn record_emits_null_files_when_empty() {
        let rec = Record {
            tick: 4,
            max_abs_z_score: 5.0,
            ..Default::default()
        };
        let json = Encoder::marshal().encode_to_string(&rec.to_go_value());
        // `files` has no omitempty => `null` for an empty slice.
        assert!(json.contains(r#""files":null"#), "got: {json}");
    }

    #[test]
    fn computed_metrics_omits_empty_external_sections() {
        let cm = ComputedMetrics::default();
        let json = Encoder::marshal().encode_to_string(&cm.to_go_value());
        assert!(!json.contains("external_anomalies"), "got: {json}");
        assert!(!json.contains("external_summaries"), "got: {json}");
        // `anomalies` is the nil slice `buildRecords` yields for no detections
        // (Go marshals nil `[]Record` -> `null`); `time_series` is a non-nil
        // `make([]TimeSeriesEntry, 0)` -> `[]`.
        assert!(json.contains(r#""anomalies":null"#), "got: {json}");
        assert!(json.contains(r#""time_series":[]"#), "got: {json}");
    }

    #[test]
    fn aggregate_threshold_promotes_float32_to_go_float() {
        let agg = AggregateData {
            threshold: 2.0,
            ..Default::default()
        };
        let json = Encoder::marshal().encode_to_string(&agg.to_go_value());
        // Go renders float32(2.0) -> 2 through the json float encoder.
        assert!(json.contains(r#""threshold":2"#), "got: {json}");
    }
}
