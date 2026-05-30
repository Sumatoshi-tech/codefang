//! Configuration model and validation.
//!
//! Direct port of `internal/config/config.go`. Field names, the YAML/`mapstructure`
//! keys, the validation order, and the sentinel-error messages all mirror the Go
//! source so behavior is reproduced exactly.

use serde::Deserialize;
use std::fmt;

/// Upper bound for the sentiment gap value (`sentimentGapMax`).
const SENTIMENT_GAP_MAX: f64 = 1.0;
/// Upper bound for ratio values, 0.0 to 1.0 (`ratioMax`).
const RATIO_MAX: f64 = 1.0;
/// Upper bound for percentage values, 0 to 100 (`percentMax`).
const PERCENT_MAX: f64 = 100.0;
/// Minimum valid HLL precision (`minHLLPrecision`).
const MIN_HLL_PRECISION: i64 = 4;
/// Maximum valid HLL precision (`maxHLLPrecision`).
const MAX_HLL_PRECISION: i64 = 18;
/// Minimum valid sliding window for anomaly detection (`minAnomalyWindowSize`).
const MIN_ANOMALY_WINDOW_SIZE: i64 = 2;

/// Sentinel errors returned by [`Config::validate`].
///
/// Each variant maps one-to-one to a Go `Err*` sentinel in `config.go`; the
/// [`fmt::Display`] text is byte-identical to the Go `errors.New` message so any
/// downstream wrapping (`validate config: <msg>`) matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigError {
    /// `pipeline.workers must be non-negative`
    InvalidWorkers,
    /// `pipeline.diff_cache_size must be non-negative`
    InvalidDiffCacheSize,
    /// `pipeline.commit_batch_size must be non-negative`
    InvalidCommitBatchSize,
    /// `pipeline.gogc must be non-negative`
    InvalidGogc,
    /// `pipeline.uast_spill_threshold must be non-negative`
    InvalidUastSpillThreshold,
    /// `pipeline.intra_commit_parallel_threshold must be non-negative`
    InvalidIntraCommitParallelThreshold,
    /// `pipeline.max_intra_commit_workers must be non-negative`
    InvalidMaxIntraCommitWorkers,
    /// `pipeline.max_uast_blob_size must be non-negative`
    InvalidMaxUastBlobSize,
    /// `pipeline.max_changes_per_commit must be non-negative`
    InvalidMaxChangesPerCommit,
    /// `pipeline.max_diff_batch_size must be non-negative`
    InvalidMaxDiffBatchSize,
    /// `pipeline.memory_budget_ratio must be between 0 and 100`
    InvalidMemoryBudgetRatio,
    /// `pipeline.memory_limit_ratio must be between 0 and 100`
    InvalidMemoryLimitRatio,
    /// `history.burndown.granularity must be positive`
    InvalidBurndownGranularity,
    /// `history.burndown.sampling must be positive`
    InvalidBurndownSampling,
    /// `history.couples.coupling_threshold_high must be non-negative`
    InvalidCouplingThreshold,
    /// `history.couples.ownership_few_threshold must be non-negative`
    InvalidOwnershipFewThreshold,
    /// `history.couples.ownership_moderate_threshold must be non-negative`
    InvalidOwnershipModerateThreshold,
    /// `history.couples.hll_precision must be between 4 and 18`
    InvalidCouplesHllPrecision,
    /// `history.devs.bus_factor_threshold must be between 0 and 1`
    InvalidBusFactorThreshold,
    /// `history.devs.risk_threshold_critical must be between 0 and 100`
    InvalidDevsRiskThresholdCritical,
    /// `history.devs.risk_threshold_high must be between 0 and 100`
    InvalidDevsRiskThresholdHigh,
    /// `history.devs.risk_threshold_medium must be between 0 and 100`
    InvalidDevsRiskThresholdMedium,
    /// `history.devs.active_threshold_ratio must be between 0 and 1`
    InvalidDevsActiveThresholdRatio,
    /// `history.devs.default_active_days must be non-negative`
    InvalidDevsDefaultActiveDays,
    /// `history.devs.hll_precision must be between 4 and 18`
    InvalidDevsHllPrecision,
    /// `history.file_history.hotspot_threshold_critical must be non-negative`
    InvalidHotspotThresholdCritical,
    /// `history.file_history.hotspot_threshold_high must be non-negative`
    InvalidHotspotThresholdHigh,
    /// `history.file_history.hotspot_threshold_medium must be non-negative`
    InvalidHotspotThresholdMedium,
    /// `history.sentiment.min_comment_length must be positive`
    InvalidSentimentMinLength,
    /// `history.sentiment.gap must be between 0 and 1`
    InvalidSentimentGap,
    /// `history.sentiment.neutralizer_weight must be between 0 and 1`
    InvalidNeutralizerWeight,
    /// `history.sentiment.max_weight_ratio must be non-negative`
    InvalidMaxWeightRatio,
    /// `history.typos.max_distance must be positive`
    InvalidTyposMaxDistance,
    /// `history.imports.goroutines must be positive`
    InvalidImportsGoroutines,
    /// `history.imports.max_file_size must be positive`
    InvalidImportsMaxFileSize,
    /// `history.imports.max_dependency_risk_rows must be non-negative`
    InvalidImportsMaxDependencyRiskRows,
    /// `history.anomaly.threshold must be positive`
    InvalidAnomalyThreshold,
    /// `history.anomaly.window_size must be at least 2`
    InvalidAnomalyWindowSize,
    /// `history.clones.max_clone_pairs must be non-negative`
    InvalidClonesMaxClonePairs,
}

impl ConfigError {
    /// Returns the byte-identical Go sentinel message for this error.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidWorkers => "pipeline.workers must be non-negative",
            Self::InvalidDiffCacheSize => "pipeline.diff_cache_size must be non-negative",
            Self::InvalidCommitBatchSize => "pipeline.commit_batch_size must be non-negative",
            Self::InvalidGogc => "pipeline.gogc must be non-negative",
            Self::InvalidUastSpillThreshold => "pipeline.uast_spill_threshold must be non-negative",
            Self::InvalidIntraCommitParallelThreshold => {
                "pipeline.intra_commit_parallel_threshold must be non-negative"
            }
            Self::InvalidMaxIntraCommitWorkers => {
                "pipeline.max_intra_commit_workers must be non-negative"
            }
            Self::InvalidMaxUastBlobSize => "pipeline.max_uast_blob_size must be non-negative",
            Self::InvalidMaxChangesPerCommit => "pipeline.max_changes_per_commit must be non-negative",
            Self::InvalidMaxDiffBatchSize => "pipeline.max_diff_batch_size must be non-negative",
            Self::InvalidMemoryBudgetRatio => "pipeline.memory_budget_ratio must be between 0 and 100",
            Self::InvalidMemoryLimitRatio => "pipeline.memory_limit_ratio must be between 0 and 100",
            Self::InvalidBurndownGranularity => "history.burndown.granularity must be positive",
            Self::InvalidBurndownSampling => "history.burndown.sampling must be positive",
            Self::InvalidCouplingThreshold => {
                "history.couples.coupling_threshold_high must be non-negative"
            }
            Self::InvalidOwnershipFewThreshold => {
                "history.couples.ownership_few_threshold must be non-negative"
            }
            Self::InvalidOwnershipModerateThreshold => {
                "history.couples.ownership_moderate_threshold must be non-negative"
            }
            Self::InvalidCouplesHllPrecision => {
                "history.couples.hll_precision must be between 4 and 18"
            }
            Self::InvalidBusFactorThreshold => {
                "history.devs.bus_factor_threshold must be between 0 and 1"
            }
            Self::InvalidDevsRiskThresholdCritical => {
                "history.devs.risk_threshold_critical must be between 0 and 100"
            }
            Self::InvalidDevsRiskThresholdHigh => {
                "history.devs.risk_threshold_high must be between 0 and 100"
            }
            Self::InvalidDevsRiskThresholdMedium => {
                "history.devs.risk_threshold_medium must be between 0 and 100"
            }
            Self::InvalidDevsActiveThresholdRatio => {
                "history.devs.active_threshold_ratio must be between 0 and 1"
            }
            Self::InvalidDevsDefaultActiveDays => {
                "history.devs.default_active_days must be non-negative"
            }
            Self::InvalidDevsHllPrecision => "history.devs.hll_precision must be between 4 and 18",
            Self::InvalidHotspotThresholdCritical => {
                "history.file_history.hotspot_threshold_critical must be non-negative"
            }
            Self::InvalidHotspotThresholdHigh => {
                "history.file_history.hotspot_threshold_high must be non-negative"
            }
            Self::InvalidHotspotThresholdMedium => {
                "history.file_history.hotspot_threshold_medium must be non-negative"
            }
            Self::InvalidSentimentMinLength => "history.sentiment.min_comment_length must be positive",
            Self::InvalidSentimentGap => "history.sentiment.gap must be between 0 and 1",
            Self::InvalidNeutralizerWeight => {
                "history.sentiment.neutralizer_weight must be between 0 and 1"
            }
            Self::InvalidMaxWeightRatio => "history.sentiment.max_weight_ratio must be non-negative",
            Self::InvalidTyposMaxDistance => "history.typos.max_distance must be positive",
            Self::InvalidImportsGoroutines => "history.imports.goroutines must be positive",
            Self::InvalidImportsMaxFileSize => "history.imports.max_file_size must be positive",
            Self::InvalidImportsMaxDependencyRiskRows => {
                "history.imports.max_dependency_risk_rows must be non-negative"
            }
            Self::InvalidAnomalyThreshold => "history.anomaly.threshold must be positive",
            Self::InvalidAnomalyWindowSize => "history.anomaly.window_size must be at least 2",
            Self::InvalidClonesMaxClonePairs => "history.clones.max_clone_pairs must be non-negative",
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for ConfigError {}

/// Top-level configuration for codefang (port of Go `Config`).
///
/// `serde(default)` makes every absent YAML key fall back to the Rust field
/// default, mirroring how viper merges a partial file over registered defaults.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Analyzer IDs or glob patterns selected for the run.
    pub analyzers: Vec<String>,
    /// Pipeline resource knobs.
    pub pipeline: PipelineConfig,
    /// Per-analyzer history configuration.
    pub history: HistoryConfig,
    /// Checkpoint settings.
    pub checkpoint: CheckpointConfig,
}

/// Pipeline resource knobs (port of Go `PipelineConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    /// Worker count.
    pub workers: i64,
    /// Memory budget string.
    pub memory_budget: String,
    /// Blob cache size string.
    pub blob_cache_size: String,
    /// Diff cache size.
    pub diff_cache_size: i64,
    /// Blob arena size string.
    pub blob_arena_size: String,
    /// Commit batch size.
    pub commit_batch_size: i64,
    /// GOGC value.
    pub gogc: i64,
    /// Ballast size string.
    pub ballast_size: String,
    /// Memory limit string.
    pub memory_limit: String,
    /// Worker timeout string.
    pub worker_timeout: String,

    /// UAST spill threshold.
    pub uast_spill_threshold: i64,
    /// Intra-commit parallel threshold.
    pub intra_commit_parallel_threshold: i64,
    /// Maximum intra-commit workers.
    pub max_intra_commit_workers: i64,
    /// Maximum UAST blob size.
    pub max_uast_blob_size: i64,
    /// UAST parse timeout string.
    pub uast_parse_timeout: String,
    /// Maximum changes per commit.
    pub max_changes_per_commit: i64,
    /// Maximum diff batch size.
    pub max_diff_batch_size: i64,
    /// Memory budget ratio (percent).
    pub memory_budget_ratio: i64,
    /// Memory budget cap string.
    pub memory_budget_cap: String,
    /// Memory limit ratio (percent).
    pub memory_limit_ratio: i64,
    /// UAST spill trim interval.
    pub uast_spill_trim_interval: i64,
    /// Native trim interval.
    pub native_trim_interval: i64,
    /// Maximum streaming buffering.
    pub max_streaming_buffering: i64,
    /// Drain prefetch timeout string.
    pub drain_prefetch_timeout: String,
    /// Sampler interval string.
    pub sampler_interval: String,
    /// Worker ratio (percent).
    pub worker_ratio: i64,
    /// UAST worker ratio (percent).
    pub uast_worker_ratio: i64,
    /// Leaf worker divisor.
    pub leaf_worker_divisor: i64,
    /// Minimum leaf workers.
    pub min_leaf_workers: i64,
    /// Buffer size multiplier.
    pub buffer_size_multiplier: i64,
    /// Budget limit ratio (percent).
    pub budget_limit_ratio: i64,
    /// System RAM limit ratio (percent).
    pub system_ram_limit_ratio: i64,
    /// Static analyzer maximum workers.
    pub static_max_workers: i64,
    /// Malloc trim interval.
    pub malloc_trim_interval: i64,
    /// Static memory limit ratio (percent).
    pub static_memory_limit_ratio: i64,
    /// Diff job buffer multiplier.
    pub diff_job_buffer_multiplier: i64,
}

/// Per-analyzer history configuration (port of Go `HistoryConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    /// Burndown analyzer settings.
    pub burndown: BurndownConfig,
    /// Couples analyzer settings.
    pub couples: CouplesConfig,
    /// Devs analyzer settings.
    pub devs: DevsConfig,
    /// File-history analyzer settings.
    pub file_history: FileHistoryConfig,
    /// Imports analyzer settings.
    pub imports: ImportsConfig,
    /// Sentiment analyzer settings.
    pub sentiment: SentimentConfig,
    /// Shotness analyzer settings.
    pub shotness: ShotnessConfig,
    /// Typos analyzer settings.
    pub typos: TyposConfig,
    /// Anomaly analyzer settings.
    pub anomaly: AnomalyConfig,
    /// Clones analyzer settings.
    pub clones: ClonesConfig,
}

/// Temporal anomaly detection settings (port of Go `AnomalyConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct AnomalyConfig {
    /// Z-score / deviation threshold.
    pub threshold: f64,
    /// Sliding window size.
    pub window_size: i64,
}

/// Burndown analyzer settings (port of Go `BurndownConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct BurndownConfig {
    /// Time-bucket granularity in days.
    pub granularity: i64,
    /// Sampling interval in days.
    pub sampling: i64,
    /// Track per-file burndown.
    pub track_files: bool,
    /// Track per-person burndown.
    pub track_people: bool,
    /// Hibernation threshold.
    pub hibernation_threshold: i64,
    /// Hibernate state to disk.
    pub hibernation_to_disk: bool,
    /// Hibernation directory.
    pub hibernation_directory: String,
    /// Debug logging.
    pub debug: bool,
    /// Goroutine count.
    pub goroutines: i64,
}

/// Couples analyzer settings (port of Go `CouplesConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct CouplesConfig {
    /// High coupling threshold.
    pub coupling_threshold_high: i64,
    /// Few-ownership threshold.
    pub ownership_few_threshold: i64,
    /// Moderate-ownership threshold.
    pub ownership_moderate_threshold: i64,
    /// Batch coupling threshold.
    pub batch_coupling_threshold: i64,
    /// HLL precision.
    pub hll_precision: i64,
    /// Top-K couples kept per file.
    pub top_k_per_file: i64,
    /// Minimum edge weight.
    pub min_edge_weight: i64,
}

/// Devs analyzer settings (port of Go `DevsConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct DevsConfig {
    /// Count empty commits.
    pub consider_empty_commits: bool,
    /// Anonymize developer identities.
    pub anonymize: bool,
    /// Bus-factor threshold.
    pub bus_factor_threshold: f64,
    /// Critical risk threshold (percent).
    pub risk_threshold_critical: f64,
    /// High risk threshold (percent).
    pub risk_threshold_high: f64,
    /// Medium risk threshold (percent).
    pub risk_threshold_medium: f64,
    /// Active threshold ratio.
    pub active_threshold_ratio: f64,
    /// Default active-days window.
    pub default_active_days: i64,
    /// HLL precision.
    pub hll_precision: i64,
}

/// File-history analyzer settings (port of Go `FileHistoryConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct FileHistoryConfig {
    /// Critical hotspot threshold.
    pub hotspot_threshold_critical: i64,
    /// High hotspot threshold.
    pub hotspot_threshold_high: i64,
    /// Medium hotspot threshold.
    pub hotspot_threshold_medium: i64,
}

/// Imports analyzer settings (port of Go `ImportsConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ImportsConfig {
    /// Goroutine count.
    pub goroutines: i64,
    /// Maximum analyzed file size.
    pub max_file_size: i64,
    /// Maximum dependency-risk rows.
    pub max_dependency_risk_rows: i64,
}

/// Sentiment analyzer settings (port of Go `SentimentConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct SentimentConfig {
    /// Minimum comment length.
    pub min_comment_length: i64,
    /// Gap.
    pub gap: f64,
    /// Neutralizer weight.
    pub neutralizer_weight: f64,
    /// Maximum weight ratio.
    pub max_weight_ratio: f64,
    /// Positive threshold.
    pub positive_threshold: f64,
    /// Negative threshold.
    pub negative_threshold: f64,
    /// Trend threshold.
    pub trend_threshold: f64,
    /// Low-sentiment risk threshold.
    #[serde(rename = "low_sentiment_risk_threshold")]
    pub low_sentiment_risk_thresh: f64,
}

/// Shotness analyzer settings (port of Go `ShotnessConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ShotnessConfig {
    /// Structural selection DSL.
    pub dsl_struct: String,
    /// Name extraction DSL.
    pub dsl_name: String,
}

/// Typos analyzer settings (port of Go `TyposConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct TyposConfig {
    /// Maximum edit distance.
    pub max_distance: i64,
}

/// Clones analyzer settings (port of Go `ClonesConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct ClonesConfig {
    /// Maximum clone pairs.
    pub max_clone_pairs: i64,
    /// Number of MinHash hashes.
    pub num_hashes: i64,
    /// Number of LSH bands.
    pub num_bands: i64,
    /// Number of LSH rows.
    pub num_rows: i64,
    /// Shingle size.
    pub shingle_size: i64,
    /// Type-2 similarity threshold.
    pub similarity_type2: f64,
    /// Type-3 similarity threshold.
    pub similarity_type3: f64,
    /// Yellow threshold ratio.
    pub threshold_ratio_yellow: f64,
    /// Red threshold ratio.
    pub threshold_ratio_red: f64,
    /// Yellow threshold pairs.
    pub threshold_pairs_yellow: i64,
    /// Red threshold pairs.
    pub threshold_pairs_red: i64,
}

/// Checkpoint settings (port of Go `CheckpointConfig`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct CheckpointConfig {
    /// Checkpointing enabled.
    pub enabled: bool,
    /// Checkpoint directory.
    pub dir: String,
    /// Resume from a previous checkpoint.
    pub resume: bool,
    /// Clear a previous checkpoint before running.
    pub clear_prev: bool,
}

impl Config {
    /// Validates configuration invariants and returns the first error found.
    ///
    /// Ports `Config.Validate`: pipeline checks first, then history. The
    /// zero-value config is valid (Go's `Config{}.Validate()` returns nil)
    /// because every bound is `< 0` (or a `!= 0 && out-of-range` guard).
    ///
    /// # Errors
    /// Returns the first [`ConfigError`] whose invariant is violated.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.validate_pipeline()?;
        self.validate_history()
    }

    fn validate_pipeline(&self) -> Result<(), ConfigError> {
        let p = &self.pipeline;
        let pct_max = PERCENT_MAX as i64;
        if p.workers < 0 {
            return Err(ConfigError::InvalidWorkers);
        }
        if p.diff_cache_size < 0 {
            return Err(ConfigError::InvalidDiffCacheSize);
        }
        if p.commit_batch_size < 0 {
            return Err(ConfigError::InvalidCommitBatchSize);
        }
        if p.gogc < 0 {
            return Err(ConfigError::InvalidGogc);
        }
        if p.uast_spill_threshold < 0 {
            return Err(ConfigError::InvalidUastSpillThreshold);
        }
        if p.intra_commit_parallel_threshold < 0 {
            return Err(ConfigError::InvalidIntraCommitParallelThreshold);
        }
        if p.max_intra_commit_workers < 0 {
            return Err(ConfigError::InvalidMaxIntraCommitWorkers);
        }
        if p.max_uast_blob_size < 0 {
            return Err(ConfigError::InvalidMaxUastBlobSize);
        }
        if p.max_changes_per_commit < 0 {
            return Err(ConfigError::InvalidMaxChangesPerCommit);
        }
        if p.max_diff_batch_size < 0 {
            return Err(ConfigError::InvalidMaxDiffBatchSize);
        }
        if p.memory_budget_ratio < 0 || p.memory_budget_ratio > pct_max {
            return Err(ConfigError::InvalidMemoryBudgetRatio);
        }
        if p.memory_limit_ratio < 0 || p.memory_limit_ratio > pct_max {
            return Err(ConfigError::InvalidMemoryLimitRatio);
        }
        Ok(())
    }

    fn validate_history(&self) -> Result<(), ConfigError> {
        let h = &self.history;
        if h.burndown.granularity < 0 {
            return Err(ConfigError::InvalidBurndownGranularity);
        }
        if h.burndown.sampling < 0 {
            return Err(ConfigError::InvalidBurndownSampling);
        }
        self.validate_couples()?;
        self.validate_devs()?;
        self.validate_file_history()?;
        self.validate_sentiment()?;
        if h.typos.max_distance < 0 {
            return Err(ConfigError::InvalidTyposMaxDistance);
        }
        if h.imports.goroutines < 0 {
            return Err(ConfigError::InvalidImportsGoroutines);
        }
        if h.imports.max_file_size < 0 {
            return Err(ConfigError::InvalidImportsMaxFileSize);
        }
        if h.imports.max_dependency_risk_rows < 0 {
            return Err(ConfigError::InvalidImportsMaxDependencyRiskRows);
        }
        if h.anomaly.threshold < 0.0 {
            return Err(ConfigError::InvalidAnomalyThreshold);
        }
        if h.anomaly.window_size != 0 && h.anomaly.window_size < MIN_ANOMALY_WINDOW_SIZE {
            return Err(ConfigError::InvalidAnomalyWindowSize);
        }
        if h.clones.max_clone_pairs < 0 {
            return Err(ConfigError::InvalidClonesMaxClonePairs);
        }
        Ok(())
    }

    fn validate_couples(&self) -> Result<(), ConfigError> {
        let cp = &self.history.couples;
        if cp.coupling_threshold_high < 0 {
            return Err(ConfigError::InvalidCouplingThreshold);
        }
        if cp.ownership_few_threshold < 0 {
            return Err(ConfigError::InvalidOwnershipFewThreshold);
        }
        if cp.ownership_moderate_threshold < 0 {
            return Err(ConfigError::InvalidOwnershipModerateThreshold);
        }
        if cp.hll_precision != 0
            && (cp.hll_precision < MIN_HLL_PRECISION || cp.hll_precision > MAX_HLL_PRECISION)
        {
            return Err(ConfigError::InvalidCouplesHllPrecision);
        }
        Ok(())
    }

    fn validate_devs(&self) -> Result<(), ConfigError> {
        let dv = &self.history.devs;
        if dv.bus_factor_threshold < 0.0 || dv.bus_factor_threshold > RATIO_MAX {
            return Err(ConfigError::InvalidBusFactorThreshold);
        }
        if dv.risk_threshold_critical < 0.0 || dv.risk_threshold_critical > PERCENT_MAX {
            return Err(ConfigError::InvalidDevsRiskThresholdCritical);
        }
        if dv.risk_threshold_high < 0.0 || dv.risk_threshold_high > PERCENT_MAX {
            return Err(ConfigError::InvalidDevsRiskThresholdHigh);
        }
        if dv.risk_threshold_medium < 0.0 || dv.risk_threshold_medium > PERCENT_MAX {
            return Err(ConfigError::InvalidDevsRiskThresholdMedium);
        }
        if dv.active_threshold_ratio < 0.0 || dv.active_threshold_ratio > RATIO_MAX {
            return Err(ConfigError::InvalidDevsActiveThresholdRatio);
        }
        if dv.default_active_days < 0 {
            return Err(ConfigError::InvalidDevsDefaultActiveDays);
        }
        if dv.hll_precision != 0
            && (dv.hll_precision < MIN_HLL_PRECISION || dv.hll_precision > MAX_HLL_PRECISION)
        {
            return Err(ConfigError::InvalidDevsHllPrecision);
        }
        Ok(())
    }

    fn validate_file_history(&self) -> Result<(), ConfigError> {
        let fh = &self.history.file_history;
        if fh.hotspot_threshold_critical < 0 {
            return Err(ConfigError::InvalidHotspotThresholdCritical);
        }
        if fh.hotspot_threshold_high < 0 {
            return Err(ConfigError::InvalidHotspotThresholdHigh);
        }
        if fh.hotspot_threshold_medium < 0 {
            return Err(ConfigError::InvalidHotspotThresholdMedium);
        }
        Ok(())
    }

    fn validate_sentiment(&self) -> Result<(), ConfigError> {
        let se = &self.history.sentiment;
        if se.min_comment_length < 0 {
            return Err(ConfigError::InvalidSentimentMinLength);
        }
        if se.gap < 0.0 || se.gap > SENTIMENT_GAP_MAX {
            return Err(ConfigError::InvalidSentimentGap);
        }
        if se.neutralizer_weight < 0.0 || se.neutralizer_weight > RATIO_MAX {
            return Err(ConfigError::InvalidNeutralizerWeight);
        }
        if se.max_weight_ratio < 0.0 {
            return Err(ConfigError::InvalidMaxWeightRatio);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Default impls
//
// These reproduce the values viper registers via SetDefault in loader.go, i.e.
// the config that results from an empty .codefang.yaml. Note this is NOT the Go
// zero value: the zero value (`Config{}`) is obtained with `Config::zero()`.
// ---------------------------------------------------------------------------

use crate::defaults::*;

impl Default for Config {
    fn default() -> Self {
        Self {
            analyzers: Vec::new(),
            pipeline: PipelineConfig::default(),
            history: HistoryConfig::default(),
            checkpoint: CheckpointConfig::default(),
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            workers: DEFAULT_PIPELINE_WORKERS,
            memory_budget: DEFAULT_PIPELINE_MEMORY_BUDGET.to_owned(),
            blob_cache_size: DEFAULT_PIPELINE_BLOB_CACHE_SIZE.to_owned(),
            diff_cache_size: DEFAULT_PIPELINE_DIFF_CACHE_SIZE,
            blob_arena_size: DEFAULT_PIPELINE_BLOB_ARENA_SIZE.to_owned(),
            commit_batch_size: DEFAULT_PIPELINE_COMMIT_BATCH_SIZE,
            gogc: DEFAULT_PIPELINE_GOGC,
            ballast_size: DEFAULT_PIPELINE_BALLAST_SIZE.to_owned(),
            // memory_limit and worker_timeout have no SetDefault in loader.go,
            // so viper leaves them as the Go zero value (empty string).
            memory_limit: String::new(),
            worker_timeout: String::new(),
            uast_spill_threshold: DEFAULT_PIPELINE_UAST_SPILL_THRESHOLD,
            intra_commit_parallel_threshold: DEFAULT_PIPELINE_INTRA_COMMIT_PARALLEL_THRESHOLD,
            max_intra_commit_workers: DEFAULT_PIPELINE_MAX_INTRA_COMMIT_WORKERS,
            max_uast_blob_size: DEFAULT_PIPELINE_MAX_UAST_BLOB_SIZE,
            uast_parse_timeout: DEFAULT_PIPELINE_UAST_PARSE_TIMEOUT.to_owned(),
            max_changes_per_commit: DEFAULT_PIPELINE_MAX_CHANGES_PER_COMMIT,
            max_diff_batch_size: DEFAULT_PIPELINE_MAX_DIFF_BATCH_SIZE,
            memory_budget_ratio: DEFAULT_PIPELINE_MEMORY_BUDGET_RATIO,
            memory_budget_cap: DEFAULT_PIPELINE_MEMORY_BUDGET_CAP.to_owned(),
            memory_limit_ratio: DEFAULT_PIPELINE_MEMORY_LIMIT_RATIO,
            uast_spill_trim_interval: DEFAULT_PIPELINE_UAST_SPILL_TRIM_INTERVAL,
            native_trim_interval: DEFAULT_PIPELINE_NATIVE_TRIM_INTERVAL,
            max_streaming_buffering: DEFAULT_PIPELINE_MAX_STREAMING_BUFFERING,
            drain_prefetch_timeout: DEFAULT_PIPELINE_DRAIN_PREFETCH_TIMEOUT.to_owned(),
            sampler_interval: DEFAULT_PIPELINE_SAMPLER_INTERVAL.to_owned(),
            worker_ratio: DEFAULT_PIPELINE_WORKER_RATIO,
            uast_worker_ratio: DEFAULT_PIPELINE_UAST_WORKER_RATIO,
            leaf_worker_divisor: DEFAULT_PIPELINE_LEAF_WORKER_DIVISOR,
            min_leaf_workers: DEFAULT_PIPELINE_MIN_LEAF_WORKERS,
            buffer_size_multiplier: DEFAULT_PIPELINE_BUFFER_SIZE_MULTIPLIER,
            budget_limit_ratio: DEFAULT_PIPELINE_BUDGET_LIMIT_RATIO,
            system_ram_limit_ratio: DEFAULT_PIPELINE_SYSTEM_RAM_LIMIT_RATIO,
            static_max_workers: DEFAULT_PIPELINE_STATIC_MAX_WORKERS,
            malloc_trim_interval: DEFAULT_PIPELINE_MALLOC_TRIM_INTERVAL,
            static_memory_limit_ratio: DEFAULT_PIPELINE_STATIC_MEMORY_LIMIT_RATIO,
            diff_job_buffer_multiplier: DEFAULT_PIPELINE_DIFF_JOB_BUFFER_MULTIPLIER,
        }
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            burndown: BurndownConfig::default(),
            couples: CouplesConfig::default(),
            devs: DevsConfig::default(),
            file_history: FileHistoryConfig::default(),
            imports: ImportsConfig::default(),
            sentiment: SentimentConfig::default(),
            shotness: ShotnessConfig::default(),
            typos: TyposConfig::default(),
            anomaly: AnomalyConfig::default(),
            clones: ClonesConfig::default(),
        }
    }
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_ANOMALY_THRESHOLD,
            window_size: DEFAULT_ANOMALY_WINDOW_SIZE,
        }
    }
}

impl Default for BurndownConfig {
    fn default() -> Self {
        Self {
            granularity: DEFAULT_BURNDOWN_GRANULARITY,
            sampling: DEFAULT_BURNDOWN_SAMPLING,
            track_files: DEFAULT_BURNDOWN_TRACK_FILES,
            track_people: DEFAULT_BURNDOWN_TRACK_PEOPLE,
            hibernation_threshold: DEFAULT_BURNDOWN_HIBERNATION_THRESHOLD,
            hibernation_to_disk: DEFAULT_BURNDOWN_HIBERNATION_TO_DISK,
            hibernation_directory: DEFAULT_BURNDOWN_HIBERNATION_DIRECTORY.to_owned(),
            debug: DEFAULT_BURNDOWN_DEBUG,
            goroutines: DEFAULT_BURNDOWN_GOROUTINES,
        }
    }
}

impl Default for CouplesConfig {
    fn default() -> Self {
        Self {
            coupling_threshold_high: DEFAULT_COUPLES_COUPLING_THRESHOLD_HIGH,
            ownership_few_threshold: DEFAULT_COUPLES_OWNERSHIP_FEW_THRESHOLD,
            ownership_moderate_threshold: DEFAULT_COUPLES_OWNERSHIP_MODERATE_THRESHOLD,
            batch_coupling_threshold: DEFAULT_COUPLES_BATCH_COUPLING_THRESHOLD,
            hll_precision: DEFAULT_COUPLES_HLL_PRECISION,
            top_k_per_file: DEFAULT_COUPLES_TOP_K_PER_FILE,
            min_edge_weight: DEFAULT_COUPLES_MIN_EDGE_WEIGHT,
        }
    }
}

impl Default for DevsConfig {
    fn default() -> Self {
        Self {
            consider_empty_commits: DEFAULT_DEVS_CONSIDER_EMPTY_COMMITS,
            anonymize: DEFAULT_DEVS_ANONYMIZE,
            bus_factor_threshold: DEFAULT_DEVS_BUS_FACTOR_THRESHOLD,
            risk_threshold_critical: DEFAULT_DEVS_RISK_THRESHOLD_CRITICAL,
            risk_threshold_high: DEFAULT_DEVS_RISK_THRESHOLD_HIGH,
            risk_threshold_medium: DEFAULT_DEVS_RISK_THRESHOLD_MEDIUM,
            active_threshold_ratio: DEFAULT_DEVS_ACTIVE_THRESHOLD_RATIO,
            default_active_days: DEFAULT_DEVS_DEFAULT_ACTIVE_DAYS,
            hll_precision: DEFAULT_DEVS_HLL_PRECISION,
        }
    }
}

impl Default for FileHistoryConfig {
    fn default() -> Self {
        Self {
            hotspot_threshold_critical: DEFAULT_FILE_HISTORY_HOTSPOT_CRITICAL,
            hotspot_threshold_high: DEFAULT_FILE_HISTORY_HOTSPOT_HIGH,
            hotspot_threshold_medium: DEFAULT_FILE_HISTORY_HOTSPOT_MEDIUM,
        }
    }
}

impl Default for ImportsConfig {
    fn default() -> Self {
        Self {
            goroutines: DEFAULT_IMPORTS_GOROUTINES,
            max_file_size: DEFAULT_IMPORTS_MAX_FILE_SIZE,
            max_dependency_risk_rows: DEFAULT_IMPORTS_MAX_DEPENDENCY_RISK_ROWS,
        }
    }
}

impl Default for SentimentConfig {
    fn default() -> Self {
        Self {
            min_comment_length: DEFAULT_SENTIMENT_MIN_COMMENT_LENGTH,
            gap: DEFAULT_SENTIMENT_GAP,
            neutralizer_weight: DEFAULT_SENTIMENT_NEUTRALIZER_WEIGHT,
            max_weight_ratio: DEFAULT_SENTIMENT_MAX_WEIGHT_RATIO,
            positive_threshold: DEFAULT_SENTIMENT_POSITIVE_THRESHOLD,
            negative_threshold: DEFAULT_SENTIMENT_NEGATIVE_THRESHOLD,
            trend_threshold: DEFAULT_SENTIMENT_TREND_THRESHOLD,
            low_sentiment_risk_thresh: DEFAULT_SENTIMENT_LOW_SENTIMENT_RISK_THRESH,
        }
    }
}

impl Default for ShotnessConfig {
    fn default() -> Self {
        Self {
            dsl_struct: DEFAULT_SHOTNESS_DSL_STRUCT.to_owned(),
            dsl_name: DEFAULT_SHOTNESS_DSL_NAME.to_owned(),
        }
    }
}

impl Default for TyposConfig {
    fn default() -> Self {
        Self {
            max_distance: DEFAULT_TYPOS_MAX_DISTANCE,
        }
    }
}

impl Default for ClonesConfig {
    fn default() -> Self {
        Self {
            max_clone_pairs: DEFAULT_CLONES_MAX_CLONE_PAIRS,
            num_hashes: DEFAULT_CLONES_NUM_HASHES,
            num_bands: DEFAULT_CLONES_NUM_BANDS,
            num_rows: DEFAULT_CLONES_NUM_ROWS,
            shingle_size: DEFAULT_CLONES_SHINGLE_SIZE,
            similarity_type2: DEFAULT_CLONES_SIMILARITY_TYPE2,
            similarity_type3: DEFAULT_CLONES_SIMILARITY_TYPE3,
            threshold_ratio_yellow: DEFAULT_CLONES_THRESHOLD_RATIO_YELLOW,
            threshold_ratio_red: DEFAULT_CLONES_THRESHOLD_RATIO_RED,
            threshold_pairs_yellow: DEFAULT_CLONES_THRESHOLD_PAIRS_YELLOW,
            threshold_pairs_red: DEFAULT_CLONES_THRESHOLD_PAIRS_RED,
        }
    }
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_CHECKPOINT_ENABLED,
            dir: DEFAULT_CHECKPOINT_DIR.to_owned(),
            resume: DEFAULT_CHECKPOINT_RESUME,
            clear_prev: DEFAULT_CHECKPOINT_CLEAR_PREV,
        }
    }
}
