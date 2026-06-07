//! Typos metrics computation.
//!
//! Direct port of Go `internal/analyzers/typos/metrics.go`. Given the analyzer
//! report (a list of [`Typo`]), it computes four metrics — `typo_list`,
//! `patterns`, `file_typos`, `aggregate` — exactly as Go does, including the
//! sort orders and the "frequency > 1" filter.

use std::collections::BTreeMap;

use crate::compat::{GoValue, Hash};
use crate::typos::Typo;

/// Analyzer name used for the MetricsOutput interface (Go `analyzerNameTypos`).
pub const ANALYZER_NAME_TYPOS: &str = "typos";

/// Metric name: per-typo list (Go `metricNameTypoList`).
pub const METRIC_NAME_TYPO_LIST: &str = "typo_list";
/// Metric name: recurring patterns (Go `metricNamePatterns`).
pub const METRIC_NAME_PATTERNS: &str = "patterns";
/// Metric name: per-file typo counts (Go `metricNameFileTypos`).
pub const METRIC_NAME_FILE_TYPOS: &str = "file_typos";
/// Metric name: aggregate summary (Go `metricNameAggregate`).
pub const METRIC_NAME_AGGREGATE: &str = "aggregate";

/// Parsed input data for metrics computation (Go `ReportData`).
#[derive(Debug, Clone, Default)]
pub struct ReportData {
    /// The typos to compute metrics over.
    pub typos: Vec<Typo>,
}

/// Information about a single typo fix.
///
/// Port of Go `TypoData`. JSON/YAML field order: wrong, correct, file, line,
/// commit. `commit` is the hex string form of the hash (Go `Hash.String()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypoData {
    /// Misspelled identifier.
    pub wrong: String,
    /// Corrected identifier.
    pub correct: String,
    /// File the fix appeared in.
    pub file: String,
    /// Zero-based line number.
    pub line: i64,
    /// Commit hash (hex string).
    pub commit: String,
}

impl TypoData {
    /// Encodes as a struct-origin object (field declaration order preserved).
    pub fn to_govalue(&self) -> GoValue {
        GoValue::Struct(vec![
            ("wrong".to_string(), GoValue::Str(self.wrong.clone())),
            ("correct".to_string(), GoValue::Str(self.correct.clone())),
            ("file".to_string(), GoValue::Str(self.file.clone())),
            ("line".to_string(), GoValue::Int(self.line)),
            ("commit".to_string(), GoValue::Str(self.commit.clone())),
        ])
    }
}

/// A recurring typo pattern with its frequency.
///
/// Port of Go `TypoPatternData`. Field order: wrong, correct, frequency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypoPatternData {
    /// Misspelled identifier.
    pub wrong: String,
    /// Corrected identifier.
    pub correct: String,
    /// Number of occurrences (always > 1, per Go's filter).
    pub frequency: i64,
}

impl TypoPatternData {
    /// Encodes as a struct-origin object.
    pub fn to_govalue(&self) -> GoValue {
        GoValue::Struct(vec![
            ("wrong".to_string(), GoValue::Str(self.wrong.clone())),
            ("correct".to_string(), GoValue::Str(self.correct.clone())),
            ("frequency".to_string(), GoValue::Int(self.frequency)),
        ])
    }
}

/// Typo statistics for a single file.
///
/// Port of Go `FileTypoData`. Field order: file, typo_count, fixed_typos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTypoData {
    /// File path.
    pub file: String,
    /// Number of typos detected in the file.
    pub typo_count: i64,
    /// Number of typos fixed (equal to `typo_count` in the Go source).
    pub fixed_typos: i64,
}

impl FileTypoData {
    /// Encodes as a struct-origin object.
    pub fn to_govalue(&self) -> GoValue {
        GoValue::Struct(vec![
            ("file".to_string(), GoValue::Str(self.file.clone())),
            ("typo_count".to_string(), GoValue::Int(self.typo_count)),
            ("fixed_typos".to_string(), GoValue::Int(self.fixed_typos)),
        ])
    }
}

/// Summary statistics across all typos.
///
/// Port of Go `AggregateData`. Field order: total_typos, unique_patterns,
/// affected_files, affected_commits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AggregateData {
    /// Total number of typos.
    pub total_typos: i64,
    /// Number of distinct `wrong|correct` patterns.
    pub unique_patterns: i64,
    /// Number of distinct files affected.
    pub affected_files: i64,
    /// Number of distinct commits affected.
    pub affected_commits: i64,
}

impl AggregateData {
    /// Encodes as a struct-origin object (field declaration order preserved).
    pub fn to_govalue(&self) -> GoValue {
        GoValue::Struct(vec![
            ("total_typos".to_string(), GoValue::Int(self.total_typos)),
            (
                "unique_patterns".to_string(),
                GoValue::Int(self.unique_patterns),
            ),
            (
                "affected_files".to_string(),
                GoValue::Int(self.affected_files),
            ),
            (
                "affected_commits".to_string(),
                GoValue::Int(self.affected_commits),
            ),
        ])
    }
}

/// Computes the `typo_list` metric (Go `computeTypoList`). Preserves input order.
pub fn compute_typo_list(input: &ReportData) -> Vec<TypoData> {
    input
        .typos
        .iter()
        .map(|t| TypoData {
            wrong: t.wrong.clone(),
            correct: t.correct.clone(),
            file: t.file.clone(),
            line: t.line,
            commit: t.commit.string(),
        })
        .collect()
}

/// Computes the `patterns` metric (Go `computeTypoPatterns`).
///
/// Counts `wrong|correct` occurrences, keeps only frequency > 1, and sorts by
/// frequency descending. Go's `sort.Slice` is unstable so equal-frequency ties
/// have unspecified order in Go; this port breaks ties by the `wrong|correct`
/// key for determinism (documented in the port notes).
pub fn compute_typo_patterns(input: &ReportData) -> Vec<TypoPatternData> {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    for t in &input.typos {
        let key = format!("{}|{}", t.wrong, t.correct);
        *counts.entry(key).or_insert(0) += 1;
    }

    let mut result: Vec<TypoPatternData> = counts
        .into_iter()
        .filter(|(_, freq)| *freq > 1)
        .filter_map(|(key, freq)| {
            key.find('|').map(|i| TypoPatternData {
                wrong: key[..i].to_string(),
                correct: key[i + 1..].to_string(),
                frequency: freq,
            })
        })
        .collect();

    result.sort_by(|a, b| {
        b.frequency
            .cmp(&a.frequency)
            .then_with(|| a.wrong.cmp(&b.wrong))
            .then_with(|| a.correct.cmp(&b.correct))
    });

    result
}

/// Computes the `file_typos` metric (Go `computeFileTypos`).
///
/// Counts typos per file and sorts by typo count descending. Ties are broken by
/// file name (ascending) for determinism (Go's `sort.Slice` is unstable).
pub fn compute_file_typos(input: &ReportData) -> Vec<FileTypoData> {
    let mut file_counts: BTreeMap<String, i64> = BTreeMap::new();
    for t in &input.typos {
        *file_counts.entry(t.file.clone()).or_insert(0) += 1;
    }

    let mut result: Vec<FileTypoData> = file_counts
        .into_iter()
        .map(|(file, count)| FileTypoData {
            file,
            typo_count: count,
            fixed_typos: count,
        })
        .collect();

    result.sort_by(|a, b| {
        b.typo_count
            .cmp(&a.typo_count)
            .then_with(|| a.file.cmp(&b.file))
    });

    result
}

/// Computes the `aggregate` metric (Go `computeAggregate`).
pub fn compute_aggregate(input: &ReportData) -> AggregateData {
    use std::collections::HashSet;

    let mut patterns: HashSet<String> = HashSet::new();
    let mut files: HashSet<String> = HashSet::new();
    let mut commits: HashSet<Hash> = HashSet::new();

    for t in &input.typos {
        patterns.insert(format!("{}|{}", t.wrong, t.correct));
        files.insert(t.file.clone());
        commits.insert(t.commit);
    }

    AggregateData {
        total_typos: input.typos.len() as i64,
        unique_patterns: patterns.len() as i64,
        affected_files: files.len() as i64,
        affected_commits: commits.len() as i64,
    }
}

/// A computed metric result (name + value), mirroring Go `common.MetricResult`.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricResult {
    /// Metric name.
    pub name: String,
    /// Metric value.
    pub value: GoValue,
}

/// Runs all typos metrics in the fixed Go order and returns the results.
///
/// Port of Go `ComputeAllMetrics`: typo_list, patterns, file_typos, aggregate.
pub fn compute_all_metrics(input: &ReportData) -> Vec<MetricResult> {
    vec![
        MetricResult {
            name: METRIC_NAME_TYPO_LIST.to_string(),
            value: GoValue::Array(
                compute_typo_list(input).iter().map(TypoData::to_govalue).collect(),
            ),
        },
        MetricResult {
            name: METRIC_NAME_PATTERNS.to_string(),
            // Go `computeTypoPatterns` returns a nil slice (`var result
            // []TypoPatternData`) until the "frequency > 1" filter appends to
            // it, so an empty result marshals to JSON `null`, NOT `[]`. The
            // `typo_list`/`file_typos` metrics use `make([]T, 0, …)` and so
            // marshal to `[]` when empty — keep that asymmetry exact.
            value: {
                let patterns = compute_typo_patterns(input);
                if patterns.is_empty() {
                    GoValue::Null
                } else {
                    GoValue::Array(patterns.iter().map(TypoPatternData::to_govalue).collect())
                }
            },
        },
        MetricResult {
            name: METRIC_NAME_FILE_TYPOS.to_string(),
            value: GoValue::Array(
                compute_file_typos(input)
                    .iter()
                    .map(FileTypoData::to_govalue)
                    .collect(),
            ),
        },
        MetricResult {
            name: METRIC_NAME_AGGREGATE.to_string(),
            value: compute_aggregate(input).to_govalue(),
        },
    ]
}

/// Builds the metrics report value: a **map-origin** object keyed by metric
/// name, so JSON encoding sorts the keys by raw UTF-8 bytes.
///
/// Go `common.MetricSet.ToJSON()` returns a `map[string]any` (one entry per
/// metric), and `json.Marshal` sorts map keys. So the top-level order is the
/// byte-sorted `aggregate`, `file_typos`, `patterns`, `typo_list` — NOT the
/// `typo_list`, `patterns`, `file_typos`, `aggregate` computation order. Using
/// [`GoValue::Map`] (sorted at encode time) reproduces that exactly.
pub fn metrics_report_value(input: &ReportData) -> GoValue {
    let metrics = compute_all_metrics(input);
    GoValue::Map(metrics.into_iter().map(|m| (m.name, m.value)).collect())
}

/// Builds the metrics value for **YAML** serialization (Go `MetricSet.ToYAML()`
/// marshaled by `gopkg.in/yaml.v3`).
///
/// `ToYAML()` returns the same `map[string]any` as [`metrics_report_value`], but
/// Go's YAML encoder renders a typed **nil slice** (`[]TypoPatternData(nil)`) as
/// `[]`, whereas `encoding/json` renders it as `null`. `computeTypoPatterns`
/// returns such a nil slice when no pattern repeats, so the only json/yaml shape
/// difference is the empty `patterns` metric: JSON `null` vs YAML `[]`. This
/// builder reproduces the YAML side by promoting an empty `patterns` value
/// ([`GoValue::Null`]) to an empty [`GoValue::Array`]; every other metric is
/// identical to the JSON value.
#[must_use]
pub fn metrics_yaml_value(input: &ReportData) -> GoValue {
    let metrics = compute_all_metrics(input);
    GoValue::Map(
        metrics
            .into_iter()
            .map(|m| {
                let value = if m.name == METRIC_NAME_PATTERNS && m.value == GoValue::Null {
                    GoValue::Array(Vec::new())
                } else {
                    m.value
                };
                (m.name, value)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typo(wrong: &str, correct: &str, file: &str, line: i64, commit: Hash) -> Typo {
        Typo {
            wrong: wrong.to_string(),
            correct: correct.to_string(),
            file: file.to_string(),
            commit,
            line,
        }
    }

    fn hash(byte: u8) -> Hash {
        let mut h = [0u8; 20];
        h[0] = byte;
        Hash(h)
    }

    fn data() -> ReportData {
        ReportData {
            typos: vec![
                typo("recieve", "receive", "a.go", 10, hash(1)),
                typo("recieve", "receive", "a.go", 20, hash(1)),
                typo("seperate", "separate", "b.go", 5, hash(2)),
            ],
        }
    }

    #[test]
    fn typo_list_preserves_order_and_hex_commit() {
        let list = compute_typo_list(&data());
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].wrong, "recieve");
        assert_eq!(list[0].commit, hash(1).string());
        assert_eq!(list[2].wrong, "seperate");
    }

    #[test]
    fn patterns_only_keeps_frequency_gt_one() {
        let patterns = compute_typo_patterns(&data());
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].wrong, "recieve");
        assert_eq!(patterns[0].correct, "receive");
        assert_eq!(patterns[0].frequency, 2);
    }

    #[test]
    fn file_typos_counts_and_sorts_desc() {
        let files = compute_file_typos(&data());
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file, "a.go");
        assert_eq!(files[0].typo_count, 2);
        assert_eq!(files[0].fixed_typos, 2);
        assert_eq!(files[1].file, "b.go");
        assert_eq!(files[1].typo_count, 1);
    }

    #[test]
    fn aggregate_counts_distinct() {
        let agg = compute_aggregate(&data());
        assert_eq!(agg.total_typos, 3);
        assert_eq!(agg.unique_patterns, 2);
        assert_eq!(agg.affected_files, 2);
        assert_eq!(agg.affected_commits, 2);
    }

    #[test]
    fn aggregate_empty() {
        assert_eq!(compute_aggregate(&ReportData::default()), AggregateData::default());
    }

    #[test]
    fn all_metrics_order() {
        let metrics = compute_all_metrics(&data());
        let names: Vec<&str> = metrics.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["typo_list", "patterns", "file_typos", "aggregate"]);
    }

    #[test]
    fn metrics_report_value_sorted_keys_and_json() {
        // Single typo -> typo_list has one entry, patterns empty (one
        // occurrence, filtered out -> nil -> null), file_typos one, aggregate
        // totals 1. Go `MetricSet.ToJSON()` is a map, so keys are byte-sorted:
        // aggregate, file_typos, patterns, typo_list.
        let input = ReportData {
            typos: vec![typo("tets", "test", "main.go", 10, Hash::default())],
        };
        let json = metrics_report_value(&input).to_json();
        assert!(json.starts_with("{\"aggregate\":{\"total_typos\":1,"));
        assert!(json.contains("\"file_typos\":[{\"file\":\"main.go\",\"typo_count\":1,\"fixed_typos\":1}]"));
        // A single occurrence is filtered (frequency > 1), leaving the nil
        // slice -> JSON null, not [].
        assert!(json.contains("\"patterns\":null"));
        assert!(json.ends_with("\"typo_list\":[{\"wrong\":\"tets\",\"correct\":\"test\",\"file\":\"main.go\",\"line\":10,\"commit\":\"0000000000000000000000000000000000000000\"}]}"));
    }

    #[test]
    fn metrics_report_value_empty_matches_golden_bytes() {
        // The run/history_typos.json golden: zero typos -> the exact 138-byte
        // compact JSON the Go binary emits for the empty typos metric set.
        let json = metrics_report_value(&ReportData::default()).to_json();
        assert_eq!(
            json,
            r#"{"aggregate":{"total_typos":0,"unique_patterns":0,"affected_files":0,"affected_commits":0},"file_typos":[],"patterns":null,"typo_list":[]}"#
        );
        assert_eq!(json.len(), 138);
    }
}
