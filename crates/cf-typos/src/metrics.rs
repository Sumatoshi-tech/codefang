//! The `history/typos` report metrics.
//!
//! Given every typo-fix occurrence a history walk detected ([`ReportData`],
//! chronological), the report carries four metrics:
//!
//! * `typo_list` — every distinct `(wrong, correct)` pair once, attributed
//!   to its first fix (file, 1-based line, commit);
//! * `patterns` — the pairs fixed more than once, with their occurrence
//!   count, most frequent first;
//! * `file_typos` — occurrences per file, most affected first;
//! * `aggregate` — occurrences, distinct pairs, distinct files, distinct
//!   commits.
//!
//! [`metrics_report_value`] is the ONE value every machine format encodes:
//! json marshals it, yaml renders it under the analyzer header, bin wraps the
//! json bytes.

use std::collections::{BTreeMap, HashSet};

use cf_gojson::{GoMap, GoValue};

use crate::hash::Hash;
use crate::typos::{deduplicate_typos, Typo};

/// Metric name: the distinct typo-fix pairs.
pub const METRIC_NAME_TYPO_LIST: &str = "typo_list";
/// Metric name: recurring pairs.
pub const METRIC_NAME_PATTERNS: &str = "patterns";
/// Metric name: per-file occurrence counts.
pub const METRIC_NAME_FILE_TYPOS: &str = "file_typos";
/// Metric name: aggregate summary.
pub const METRIC_NAME_AGGREGATE: &str = "aggregate";

/// The input of the metrics: every typo-fix occurrence, chronological.
#[derive(Debug, Clone, Default)]
pub struct ReportData {
    /// Every occurrence, in walk order (oldest commit first, detection order
    /// within a commit).
    pub typos: Vec<Typo>,
}

/// One `typo_list` entry.
///
/// Field order: wrong, correct, file, line, commit (lowercase hex).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypoData {
    /// Misspelled identifier.
    pub wrong: String,
    /// Corrected identifier.
    pub correct: String,
    /// File the first fix appeared in.
    pub file: String,
    /// 1-based line of the first fix.
    pub line: i64,
    /// Commit of the first fix (hex string).
    pub commit: String,
}

impl TypoData {
    /// Encodes as a struct-origin object (field declaration order).
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("wrong", GoValue::Str(self.wrong.clone()));
        m.push("correct", GoValue::Str(self.correct.clone()));
        m.push("file", GoValue::Str(self.file.clone()));
        m.push("line", GoValue::Int(self.line));
        m.push("commit", GoValue::Str(self.commit.clone()));
        GoValue::Map(m)
    }
}

/// A recurring pair with its occurrence count.
///
/// Field order: wrong, correct, frequency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypoPatternData {
    /// Misspelled identifier.
    pub wrong: String,
    /// Corrected identifier.
    pub correct: String,
    /// Number of occurrences (always > 1).
    pub frequency: i64,
}

impl TypoPatternData {
    /// Encodes as a struct-origin object.
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("wrong", GoValue::Str(self.wrong.clone()));
        m.push("correct", GoValue::Str(self.correct.clone()));
        m.push("frequency", GoValue::Int(self.frequency));
        GoValue::Map(m)
    }
}

/// Occurrences in a single file.
///
/// Field order: file, typo_count, fixed_typos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTypoData {
    /// File path.
    pub file: String,
    /// Number of typo-fix occurrences in the file.
    pub typo_count: i64,
    /// Number of fixed typos — every detected occurrence IS a fix, so this
    /// equals `typo_count`.
    pub fixed_typos: i64,
}

impl FileTypoData {
    /// Encodes as a struct-origin object.
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("file", GoValue::Str(self.file.clone()));
        m.push("typo_count", GoValue::Int(self.typo_count));
        m.push("fixed_typos", GoValue::Int(self.fixed_typos));
        GoValue::Map(m)
    }
}

/// Summary statistics.
///
/// Field order: total_typos, unique_patterns, affected_files,
/// affected_commits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AggregateData {
    /// Number of typo-fix occurrences.
    pub total_typos: i64,
    /// Number of distinct `(wrong, correct)` pairs.
    pub unique_patterns: i64,
    /// Number of distinct files with an occurrence.
    pub affected_files: i64,
    /// Number of distinct commits with an occurrence.
    pub affected_commits: i64,
}

impl AggregateData {
    /// Encodes as a struct-origin object.
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("total_typos", GoValue::Int(self.total_typos));
        m.push("unique_patterns", GoValue::Int(self.unique_patterns));
        m.push("affected_files", GoValue::Int(self.affected_files));
        m.push("affected_commits", GoValue::Int(self.affected_commits));
        GoValue::Map(m)
    }
}

/// The `typo_list` metric: every distinct pair once, attributed to its first
/// occurrence, in first-seen order.
#[must_use]
pub fn compute_typo_list(input: &ReportData) -> Vec<TypoData> {
    deduplicate_typos(&input.typos)
        .into_iter()
        .map(|t| TypoData {
            wrong: t.wrong,
            correct: t.correct,
            file: t.file,
            line: t.line,
            commit: t.commit.string(),
        })
        .collect()
}

/// The `patterns` metric: the pairs with more than one occurrence, sorted by
/// frequency descending, then by `wrong` and `correct` ascending.
///
/// ```
/// use cf_typos::metrics::{compute_typo_patterns, ReportData};
/// use cf_typos::{Hash, Typo};
///
/// let mk = |wrong: &str, correct: &str| Typo {
///     wrong: wrong.to_string(),
///     correct: correct.to_string(),
///     file: "x.go".to_string(),
///     commit: Hash::default(),
///     line: 1,
/// };
/// let input = ReportData {
///     typos: vec![
///         mk("recieve", "receive"),
///         mk("recieve", "receive"),   // frequency 2 -> kept
///         mk("seperate", "separate"), // frequency 1 -> filtered out
///     ],
/// };
/// let patterns = compute_typo_patterns(&input);
/// assert_eq!(patterns.len(), 1);
/// assert_eq!(patterns[0].wrong, "recieve");
/// assert_eq!(patterns[0].frequency, 2);
/// ```
#[must_use]
pub fn compute_typo_patterns(input: &ReportData) -> Vec<TypoPatternData> {
    let mut counts: BTreeMap<(&str, &str), i64> = BTreeMap::new();
    for t in &input.typos {
        *counts.entry((&t.wrong, &t.correct)).or_insert(0) += 1;
    }
    let mut result: Vec<TypoPatternData> = counts
        .into_iter()
        .filter(|(_, frequency)| *frequency > 1)
        .map(|((wrong, correct), frequency)| TypoPatternData {
            wrong: wrong.to_string(),
            correct: correct.to_string(),
            frequency,
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

/// The `file_typos` metric: occurrences per file, sorted by count
/// descending, then by file name ascending.
#[must_use]
pub fn compute_file_typos(input: &ReportData) -> Vec<FileTypoData> {
    let mut file_counts: BTreeMap<&str, i64> = BTreeMap::new();
    for t in &input.typos {
        *file_counts.entry(&t.file).or_insert(0) += 1;
    }
    let mut result: Vec<FileTypoData> = file_counts
        .into_iter()
        .map(|(file, count)| FileTypoData {
            file: file.to_string(),
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

/// The `aggregate` metric.
#[must_use]
pub fn compute_aggregate(input: &ReportData) -> AggregateData {
    let mut pairs: HashSet<(&str, &str)> = HashSet::new();
    let mut files: HashSet<&str> = HashSet::new();
    let mut commits: HashSet<Hash> = HashSet::new();
    for t in &input.typos {
        pairs.insert((&t.wrong, &t.correct));
        files.insert(&t.file);
        commits.insert(t.commit);
    }
    AggregateData {
        total_typos: input.typos.len() as i64,
        unique_patterns: pairs.len() as i64,
        affected_files: files.len() as i64,
        affected_commits: commits.len() as i64,
    }
}

/// The report value: a map-origin object keyed by metric name (byte-sorted
/// on encode: `aggregate`, `file_typos`, `patterns`, `typo_list`); every
/// list is present, `[]` when empty.
#[must_use]
pub fn metrics_report_value(input: &ReportData) -> GoValue {
    let array = |items: Vec<GoValue>| GoValue::Array(items);
    let mut m = GoMap::new_map();
    m.insert(
        METRIC_NAME_TYPO_LIST,
        array(
            compute_typo_list(input)
                .iter()
                .map(TypoData::to_go_value)
                .collect(),
        ),
    );
    m.insert(
        METRIC_NAME_PATTERNS,
        array(
            compute_typo_patterns(input)
                .iter()
                .map(TypoPatternData::to_go_value)
                .collect(),
        ),
    );
    m.insert(
        METRIC_NAME_FILE_TYPOS,
        array(
            compute_file_typos(input)
                .iter()
                .map(FileTypoData::to_go_value)
                .collect(),
        ),
    );
    m.insert(
        METRIC_NAME_AGGREGATE,
        compute_aggregate(input).to_go_value(),
    );
    GoValue::Map(m)
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

    /// `recieve → receive` fixed on two lines of `a.go` in commit 1, and a
    /// second pair in `b.go` in commit 2.
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
    fn typo_list_lists_each_pair_once_at_its_first_fix() {
        let list = compute_typo_list(&data());
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].wrong, "recieve");
        assert_eq!(list[0].line, 10);
        assert_eq!(list[0].commit, hash(1).string());
        assert_eq!(list[1].wrong, "seperate");
    }

    #[test]
    fn patterns_count_every_occurrence_and_keep_frequency_above_one() {
        let patterns = compute_typo_patterns(&data());
        assert_eq!(
            patterns,
            vec![TypoPatternData {
                wrong: "recieve".to_string(),
                correct: "receive".to_string(),
                frequency: 2,
            }]
        );
    }

    #[test]
    fn patterns_sort_by_frequency_then_pair() {
        let input = ReportData {
            typos: vec![
                typo("b", "bb", "f", 1, hash(1)),
                typo("b", "bb", "f", 2, hash(1)),
                typo("a", "aa", "f", 3, hash(1)),
                typo("a", "aa", "f", 4, hash(1)),
                typo("c", "cc", "f", 5, hash(1)),
                typo("c", "cc", "f", 6, hash(1)),
                typo("c", "cc", "f", 7, hash(1)),
            ],
        };
        let names: Vec<(String, i64)> = compute_typo_patterns(&input)
            .into_iter()
            .map(|p| (p.wrong, p.frequency))
            .collect();
        assert_eq!(
            names,
            vec![
                ("c".to_string(), 3),
                ("a".to_string(), 2),
                ("b".to_string(), 2)
            ]
        );
    }

    #[test]
    fn file_typos_count_occurrences_and_sort_desc() {
        let files = compute_file_typos(&data());
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file, "a.go");
        assert_eq!(files[0].typo_count, 2);
        assert_eq!(files[0].fixed_typos, 2);
        assert_eq!(files[1].file, "b.go");
        assert_eq!(files[1].typo_count, 1);
    }

    #[test]
    fn aggregate_counts_occurrences_and_distinct_pairs() {
        let agg = compute_aggregate(&data());
        assert_eq!(
            agg,
            AggregateData {
                total_typos: 3,
                unique_patterns: 2,
                affected_files: 2,
                affected_commits: 2,
            }
        );
        assert_eq!(
            compute_aggregate(&ReportData::default()),
            AggregateData::default()
        );
    }

    #[test]
    fn report_value_json_shape() {
        let json = cf_gojson::marshal(&metrics_report_value(&data()));
        let json = String::from_utf8(json).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"aggregate":{"total_typos":3,"unique_patterns":2,"affected_files":2,"affected_commits":2},"#,
                r#""file_typos":[{"file":"a.go","typo_count":2,"fixed_typos":2},{"file":"b.go","typo_count":1,"fixed_typos":1}],"#,
                r#""patterns":[{"wrong":"recieve","correct":"receive","frequency":2}],"#,
                r#""typo_list":[{"wrong":"recieve","correct":"receive","file":"a.go","line":10,"commit":"0100000000000000000000000000000000000000"},"#,
                r#"{"wrong":"seperate","correct":"separate","file":"b.go","line":5,"commit":"0200000000000000000000000000000000000000"}]}"#
            )
        );
    }

    #[test]
    fn empty_report_has_every_list_present() {
        let json = cf_gojson::marshal(&metrics_report_value(&ReportData::default()));
        assert_eq!(
            String::from_utf8(json).unwrap(),
            r#"{"aggregate":{"total_typos":0,"unique_patterns":0,"affected_files":0,"affected_commits":0},"file_typos":[],"patterns":[],"typo_list":[]}"#
        );
    }
}
