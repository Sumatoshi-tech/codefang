//! The configuration model: one optional field per supported key.
//!
//! A field is `None` when neither the file nor the environment sets it; the
//! consumer then keeps its compiled default. Every supported key is listed
//! in [`KEYS`]; anything else in a file is reported as unsupported by the
//! loader.

use serde::Deserialize;
use std::fmt;

/// The dotted name of every supported key.
pub mod keys {
    /// Default analyzer selection (`-a`).
    pub const ANALYZERS: &str = "analyzers";
    /// Changeset cap (`--max-changes-per-commit`).
    pub const MAX_CHANGES_PER_COMMIT: &str = "pipeline.max_changes_per_commit";
    /// History memory budget (`--memory-budget`).
    pub const MEMORY_BUDGET: &str = "pipeline.memory_budget";
    /// Commits per history chunk (`--chunk-size`).
    pub const CHUNK_SIZE: &str = "pipeline.chunk_size";
    /// Tick width in hours (`--tick-size`).
    pub const TICK_SIZE: &str = "history.tick_size";
    /// Burndown ticks per age band (`--granularity`).
    pub const BURNDOWN_GRANULARITY: &str = "history.burndown.granularity";
    /// Burndown ticks per sample (`--sampling`).
    pub const BURNDOWN_SAMPLING: &str = "history.burndown.sampling";
    /// Burndown per-file survival (`--burndown-files`).
    pub const BURNDOWN_TRACK_FILES: &str = "history.burndown.track_files";
    /// Burndown per-developer survival (`--burndown-people`).
    pub const BURNDOWN_TRACK_PEOPLE: &str = "history.burndown.track_people";
    /// Devs counts empty commits (`--empty-commits`).
    pub const DEVS_CONSIDER_EMPTY_COMMITS: &str = "history.devs.consider_empty_commits";
    /// Anonymized developer names (`--anonymize`).
    pub const DEVS_ANONYMIZE: &str = "history.devs.anonymize";
    /// Sentiment minimum comment length (`--min-comment-len`).
    pub const SENTIMENT_MIN_COMMENT_LENGTH: &str = "history.sentiment.min_comment_length";
    /// Shotness structural DSL (`--shotness-dsl-struct`).
    pub const SHOTNESS_DSL_STRUCT: &str = "history.shotness.dsl_struct";
    /// Shotness name DSL (`--shotness-dsl-name`).
    pub const SHOTNESS_DSL_NAME: &str = "history.shotness.dsl_name";
    /// Shotness keeps ended entities (`--shotness-keep-ended`).
    pub const SHOTNESS_KEEP_ENDED: &str = "history.shotness.keep_ended";
    /// Typos maximum Levenshtein distance (`--typos-max-distance`).
    pub const TYPOS_MAX_DISTANCE: &str = "history.typos.max_distance";
    /// Anomaly Z-score threshold (`--anomaly-threshold`).
    pub const ANOMALY_THRESHOLD: &str = "history.anomaly.threshold";
    /// Anomaly trailing window in ticks (`--anomaly-window`).
    pub const ANOMALY_WINDOW_SIZE: &str = "history.anomaly.window_size";
}

/// Every supported key, in documentation order.
pub const KEYS: &[&str] = &[
    keys::ANALYZERS,
    keys::MAX_CHANGES_PER_COMMIT,
    keys::MEMORY_BUDGET,
    keys::CHUNK_SIZE,
    keys::TICK_SIZE,
    keys::BURNDOWN_GRANULARITY,
    keys::BURNDOWN_SAMPLING,
    keys::BURNDOWN_TRACK_FILES,
    keys::BURNDOWN_TRACK_PEOPLE,
    keys::DEVS_CONSIDER_EMPTY_COMMITS,
    keys::DEVS_ANONYMIZE,
    keys::SENTIMENT_MIN_COMMENT_LENGTH,
    keys::SHOTNESS_DSL_STRUCT,
    keys::SHOTNESS_DSL_NAME,
    keys::SHOTNESS_KEEP_ENDED,
    keys::TYPOS_MAX_DISTANCE,
    keys::ANOMALY_THRESHOLD,
    keys::ANOMALY_WINDOW_SIZE,
];

/// The smallest anomaly window that has a standard deviation.
const MIN_ANOMALY_WINDOW_SIZE: i64 = 2;

/// A configured value that violates its key's rule.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigError {
    /// The dotted key.
    pub key: &'static str,
    /// The rule, e.g. `must be > 0`.
    pub rule: &'static str,
    /// The rejected value as written.
    pub got: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}, got {}", self.key, self.rule, self.got)
    }
}

impl std::error::Error for ConfigError {}

/// One configured value, typed like the flag it defaults.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An integer.
    Int(i64),
    /// A floating-point number.
    Float(f64),
    /// A boolean.
    Bool(bool),
    /// A string.
    Text(String),
    /// A list of strings.
    List(Vec<String>),
}

impl Value {
    /// The value as command-line argument strings (one per list item).
    #[must_use]
    pub fn to_args(&self) -> Vec<String> {
        match self {
            Self::Int(v) => vec![v.to_string()],
            Self::Float(v) => vec![v.to_string()],
            Self::Bool(v) => vec![v.to_string()],
            Self::Text(v) => vec![v.clone()],
            Self::List(v) => v.clone(),
        }
    }
}

/// One key the configuration sets.
#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    /// The dotted key (one of [`KEYS`]).
    pub key: &'static str,
    /// The configured value.
    pub value: Value,
}

/// The configuration: every supported key, `None` when unset.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Analyzer ids or globs selected when `-a` is absent.
    pub analyzers: Option<Vec<String>>,
    /// The `pipeline` section.
    pub pipeline: PipelineConfig,
    /// The `history` section.
    pub history: HistoryConfig,
}

/// The `pipeline` section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    /// Commits whose tree diff exceeds this many changes are dropped (`0` =
    /// the default cap).
    pub max_changes_per_commit: Option<i64>,
    /// The history memory budget as a size (`2GB`, `512MiB`) or a plain
    /// byte count; the flag parses and checks it.
    #[serde(deserialize_with = "size_text")]
    pub memory_budget: Option<String>,
    /// Commits per history chunk.
    pub chunk_size: Option<i64>,
}

/// A size written as text (`2GB`) or as a plain byte count, kept as the
/// text the `--memory-budget` flag reads.
fn size_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Size {
        Text(String),
        Bytes(u64),
    }
    Ok(Option::<Size>::deserialize(d)?.map(|size| match size {
        Size::Text(text) => text,
        Size::Bytes(bytes) => bytes.to_string(),
    }))
}

/// The `history` section.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    /// Tick width in hours.
    pub tick_size: Option<i64>,
    /// Burndown settings.
    pub burndown: BurndownConfig,
    /// Devs settings.
    pub devs: DevsConfig,
    /// Sentiment settings.
    pub sentiment: SentimentConfig,
    /// Shotness settings.
    pub shotness: ShotnessConfig,
    /// Typos settings.
    pub typos: TyposConfig,
    /// Anomaly settings.
    pub anomaly: AnomalyConfig,
}

/// `history.burndown`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BurndownConfig {
    /// Ticks per age band.
    pub granularity: Option<i64>,
    /// Ticks per sample.
    pub sampling: Option<i64>,
    /// Per-file survival.
    pub track_files: Option<bool>,
    /// Per-developer survival and interactions.
    pub track_people: Option<bool>,
}

/// `history.devs`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct DevsConfig {
    /// Count commits whose filtered diff is empty.
    pub consider_empty_commits: Option<bool>,
    /// Replace developer names with `Developer-A`, `Developer-B`, ….
    pub anonymize: Option<bool>,
}

/// `history.sentiment`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct SentimentConfig {
    /// Minimum comment length in characters (`0` disables the filter).
    pub min_comment_length: Option<i64>,
}

/// `history.shotness`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ShotnessConfig {
    /// UAST DSL query selecting the structural nodes (empty = default).
    pub dsl_struct: Option<String>,
    /// UAST DSL query resolving node names (empty = default).
    pub dsl_name: Option<String>,
    /// Keep ended entities in the report, marked.
    pub keep_ended: Option<bool>,
}

/// `history.typos`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct TyposConfig {
    /// Maximum Levenshtein distance of a typo-fix pair.
    pub max_distance: Option<i64>,
}

/// `history.anomaly`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AnomalyConfig {
    /// Z-score threshold.
    pub threshold: Option<f64>,
    /// Trailing window in ticks.
    pub window_size: Option<i64>,
}

/// An integer rule: the key, the configured value, the rule text, and the
/// predicate the value must satisfy.
type IntRule = (&'static str, Option<i64>, &'static str, fn(i64) -> bool);

/// The [`ConfigError`] for the first integer in `rules` that breaks its rule.
fn first_int_violation(rules: &[IntRule]) -> Option<ConfigError> {
    rules.iter().find_map(|&(key, value, rule, ok)| {
        value.filter(|v| !ok(*v)).map(|v| ConfigError {
            key,
            rule,
            got: v.to_string(),
        })
    })
}

impl Config {
    /// Checks every set value against its key's rule (the same rule the
    /// matching flag enforces).
    ///
    /// # Errors
    ///
    /// The first [`ConfigError`] in [`KEYS`] order.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let h = &self.history;
        let before_threshold: [IntRule; 7] = [
            (
                keys::MAX_CHANGES_PER_COMMIT,
                self.pipeline.max_changes_per_commit,
                "must be >= 0",
                |v| v >= 0,
            ),
            (
                keys::CHUNK_SIZE,
                self.pipeline.chunk_size,
                "must be >= 1",
                |v| v >= 1,
            ),
            (keys::TICK_SIZE, h.tick_size, "must be > 0", |v| v > 0),
            (
                keys::BURNDOWN_GRANULARITY,
                h.burndown.granularity,
                "must be > 0",
                |v| v > 0,
            ),
            (
                keys::BURNDOWN_SAMPLING,
                h.burndown.sampling,
                "must be > 0",
                |v| v > 0,
            ),
            (
                keys::SENTIMENT_MIN_COMMENT_LENGTH,
                h.sentiment.min_comment_length,
                "must be >= 0",
                |v| v >= 0,
            ),
            (
                keys::TYPOS_MAX_DISTANCE,
                h.typos.max_distance,
                "must be > 0",
                |v| v > 0,
            ),
        ];
        if let Some(e) = first_int_violation(&before_threshold) {
            return Err(e);
        }
        if let Some(t) = h
            .anomaly
            .threshold
            .filter(|t| t.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater))
        {
            return Err(ConfigError {
                key: keys::ANOMALY_THRESHOLD,
                rule: "must be > 0",
                got: t.to_string(),
            });
        }
        let window: [IntRule; 1] = [(
            keys::ANOMALY_WINDOW_SIZE,
            h.anomaly.window_size,
            "must be >= 2",
            |v| v >= MIN_ANOMALY_WINDOW_SIZE,
        )];
        first_int_violation(&window).map_or(Ok(()), Err)
    }

    /// Every key this configuration sets, in [`KEYS`] order. An empty
    /// `analyzers` list and an empty shotness query count as unset.
    #[must_use]
    pub fn settings(&self) -> Vec<Setting> {
        let h = &self.history;
        let non_empty = |s: &Option<String>| s.clone().filter(|s| !s.trim().is_empty());
        let candidates = [
            (
                keys::ANALYZERS,
                self.analyzers
                    .clone()
                    .filter(|a| !a.is_empty())
                    .map(Value::List),
            ),
            (
                keys::MAX_CHANGES_PER_COMMIT,
                self.pipeline.max_changes_per_commit.map(Value::Int),
            ),
            (
                keys::MEMORY_BUDGET,
                non_empty(&self.pipeline.memory_budget).map(Value::Text),
            ),
            (keys::CHUNK_SIZE, self.pipeline.chunk_size.map(Value::Int)),
            (keys::TICK_SIZE, h.tick_size.map(Value::Int)),
            (
                keys::BURNDOWN_GRANULARITY,
                h.burndown.granularity.map(Value::Int),
            ),
            (keys::BURNDOWN_SAMPLING, h.burndown.sampling.map(Value::Int)),
            (
                keys::BURNDOWN_TRACK_FILES,
                h.burndown.track_files.map(Value::Bool),
            ),
            (
                keys::BURNDOWN_TRACK_PEOPLE,
                h.burndown.track_people.map(Value::Bool),
            ),
            (
                keys::DEVS_CONSIDER_EMPTY_COMMITS,
                h.devs.consider_empty_commits.map(Value::Bool),
            ),
            (keys::DEVS_ANONYMIZE, h.devs.anonymize.map(Value::Bool)),
            (
                keys::SENTIMENT_MIN_COMMENT_LENGTH,
                h.sentiment.min_comment_length.map(Value::Int),
            ),
            (
                keys::SHOTNESS_DSL_STRUCT,
                non_empty(&h.shotness.dsl_struct).map(Value::Text),
            ),
            (
                keys::SHOTNESS_DSL_NAME,
                non_empty(&h.shotness.dsl_name).map(Value::Text),
            ),
            (
                keys::SHOTNESS_KEEP_ENDED,
                h.shotness.keep_ended.map(Value::Bool),
            ),
            (
                keys::TYPOS_MAX_DISTANCE,
                h.typos.max_distance.map(Value::Int),
            ),
            (
                keys::ANOMALY_THRESHOLD,
                h.anomaly.threshold.map(Value::Float),
            ),
            (
                keys::ANOMALY_WINDOW_SIZE,
                h.anomaly.window_size.map(Value::Int),
            ),
        ];
        candidates
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| Setting { key, value }))
            .collect()
    }
}
