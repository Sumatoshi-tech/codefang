//! Default configuration values.
//!
//! Direct port of `internal/config/defaults.go`. Every `Default*` constant here
//! mirrors its Go counterpart name-for-name and value-for-value, so a
//! [`crate::Config`] built from defaults is byte-for-byte equivalent to the Go
//! struct viper produces from an empty config file.

// Pipeline default values.

/// Default worker count (0 = auto-detect downstream).
pub const DEFAULT_PIPELINE_WORKERS: i64 = 0;
/// Default memory budget (empty = downstream auto).
pub const DEFAULT_PIPELINE_MEMORY_BUDGET: &str = "";
/// Default blob cache size (empty = downstream auto).
pub const DEFAULT_PIPELINE_BLOB_CACHE_SIZE: &str = "";
/// Default diff cache size.
pub const DEFAULT_PIPELINE_DIFF_CACHE_SIZE: i64 = 0;
/// Default blob arena size (empty = downstream auto).
pub const DEFAULT_PIPELINE_BLOB_ARENA_SIZE: &str = "";
/// Default commit batch size.
pub const DEFAULT_PIPELINE_COMMIT_BATCH_SIZE: i64 = 0;
/// Default GOGC value.
pub const DEFAULT_PIPELINE_GOGC: i64 = 0;
/// Default ballast size string.
pub const DEFAULT_PIPELINE_BALLAST_SIZE: &str = "0";

// Pipeline advanced tuning defaults.

/// Default UAST spill threshold.
pub const DEFAULT_PIPELINE_UAST_SPILL_THRESHOLD: i64 = 32;
/// Default intra-commit parallel threshold.
pub const DEFAULT_PIPELINE_INTRA_COMMIT_PARALLEL_THRESHOLD: i64 = 4;
/// Default maximum intra-commit workers.
pub const DEFAULT_PIPELINE_MAX_INTRA_COMMIT_WORKERS: i64 = 4;
/// Default maximum UAST blob size (256 KiB).
pub const DEFAULT_PIPELINE_MAX_UAST_BLOB_SIZE: i64 = 256 * 1024;
/// Default UAST parse timeout.
pub const DEFAULT_PIPELINE_UAST_PARSE_TIMEOUT: &str = "10s";
/// Default maximum changes per commit.
pub const DEFAULT_PIPELINE_MAX_CHANGES_PER_COMMIT: i64 = 10000;
/// Default maximum diff batch size.
pub const DEFAULT_PIPELINE_MAX_DIFF_BATCH_SIZE: i64 = 1000;
/// Default memory budget ratio (percent).
pub const DEFAULT_PIPELINE_MEMORY_BUDGET_RATIO: i64 = 50;
/// Default memory budget cap.
pub const DEFAULT_PIPELINE_MEMORY_BUDGET_CAP: &str = "2GiB";
/// Default memory limit ratio (percent).
pub const DEFAULT_PIPELINE_MEMORY_LIMIT_RATIO: i64 = 75;
/// Default UAST spill trim interval.
pub const DEFAULT_PIPELINE_UAST_SPILL_TRIM_INTERVAL: i64 = 16;
/// Default native trim interval.
pub const DEFAULT_PIPELINE_NATIVE_TRIM_INTERVAL: i64 = 10;
/// Default maximum streaming buffering.
pub const DEFAULT_PIPELINE_MAX_STREAMING_BUFFERING: i64 = 3;
/// Default drain prefetch timeout.
pub const DEFAULT_PIPELINE_DRAIN_PREFETCH_TIMEOUT: &str = "30s";
/// Default sampler interval.
pub const DEFAULT_PIPELINE_SAMPLER_INTERVAL: &str = "2s";
/// Default worker ratio (percent).
pub const DEFAULT_PIPELINE_WORKER_RATIO: i64 = 100;
/// Default UAST worker ratio (percent).
pub const DEFAULT_PIPELINE_UAST_WORKER_RATIO: i64 = 40;
/// Default leaf worker divisor.
pub const DEFAULT_PIPELINE_LEAF_WORKER_DIVISOR: i64 = 3;
/// Default minimum leaf workers.
pub const DEFAULT_PIPELINE_MIN_LEAF_WORKERS: i64 = 4;
/// Default buffer size multiplier.
pub const DEFAULT_PIPELINE_BUFFER_SIZE_MULTIPLIER: i64 = 2;
/// Default budget limit ratio (percent).
pub const DEFAULT_PIPELINE_BUDGET_LIMIT_RATIO: i64 = 95;
/// Default system RAM limit ratio (percent).
pub const DEFAULT_PIPELINE_SYSTEM_RAM_LIMIT_RATIO: i64 = 90;
/// Default static analyzer maximum workers.
pub const DEFAULT_PIPELINE_STATIC_MAX_WORKERS: i64 = 8;
/// Default malloc trim interval.
pub const DEFAULT_PIPELINE_MALLOC_TRIM_INTERVAL: i64 = 50;
/// Default static memory limit ratio (percent).
pub const DEFAULT_PIPELINE_STATIC_MEMORY_LIMIT_RATIO: i64 = 90;
/// Default diff job buffer multiplier.
pub const DEFAULT_PIPELINE_DIFF_JOB_BUFFER_MULTIPLIER: i64 = 10;

// Burndown analyzer defaults.

/// Default burndown granularity.
pub const DEFAULT_BURNDOWN_GRANULARITY: i64 = 30;
/// Default burndown sampling.
pub const DEFAULT_BURNDOWN_SAMPLING: i64 = 30;
/// Default burndown track-files flag.
pub const DEFAULT_BURNDOWN_TRACK_FILES: bool = false;
/// Default burndown track-people flag.
pub const DEFAULT_BURNDOWN_TRACK_PEOPLE: bool = false;
/// Default burndown hibernation threshold.
pub const DEFAULT_BURNDOWN_HIBERNATION_THRESHOLD: i64 = 1000;
/// Default burndown hibernation-to-disk flag.
pub const DEFAULT_BURNDOWN_HIBERNATION_TO_DISK: bool = true;
/// Default burndown hibernation directory.
pub const DEFAULT_BURNDOWN_HIBERNATION_DIRECTORY: &str = "";
/// Default burndown debug flag.
pub const DEFAULT_BURNDOWN_DEBUG: bool = false;
/// Default burndown goroutines.
pub const DEFAULT_BURNDOWN_GOROUTINES: i64 = 0;

// Couples analyzer defaults.

/// Default couples high coupling threshold.
pub const DEFAULT_COUPLES_COUPLING_THRESHOLD_HIGH: i64 = 10;
/// Default couples few-ownership threshold.
pub const DEFAULT_COUPLES_OWNERSHIP_FEW_THRESHOLD: i64 = 3;
/// Default couples moderate-ownership threshold.
pub const DEFAULT_COUPLES_OWNERSHIP_MODERATE_THRESHOLD: i64 = 5;
/// Default couples batch coupling threshold.
pub const DEFAULT_COUPLES_BATCH_COUPLING_THRESHOLD: i64 = 100;
/// Default couples HLL precision.
pub const DEFAULT_COUPLES_HLL_PRECISION: i64 = 10;
/// Default couples top-K per file.
pub const DEFAULT_COUPLES_TOP_K_PER_FILE: i64 = 100;
/// Default couples minimum edge weight.
pub const DEFAULT_COUPLES_MIN_EDGE_WEIGHT: i64 = 2;

// Devs analyzer defaults.

/// Default devs consider-empty-commits flag.
pub const DEFAULT_DEVS_CONSIDER_EMPTY_COMMITS: bool = false;
/// Default devs anonymize flag.
pub const DEFAULT_DEVS_ANONYMIZE: bool = false;
/// Default devs bus-factor threshold.
pub const DEFAULT_DEVS_BUS_FACTOR_THRESHOLD: f64 = 0.5;
/// Default devs critical risk threshold (percent).
pub const DEFAULT_DEVS_RISK_THRESHOLD_CRITICAL: f64 = 90.0;
/// Default devs high risk threshold (percent).
pub const DEFAULT_DEVS_RISK_THRESHOLD_HIGH: f64 = 80.0;
/// Default devs medium risk threshold (percent).
pub const DEFAULT_DEVS_RISK_THRESHOLD_MEDIUM: f64 = 60.0;
/// Default devs active threshold ratio.
pub const DEFAULT_DEVS_ACTIVE_THRESHOLD_RATIO: f64 = 0.7;
/// Default devs default-active-days.
pub const DEFAULT_DEVS_DEFAULT_ACTIVE_DAYS: i64 = 90;
/// Default devs HLL precision.
pub const DEFAULT_DEVS_HLL_PRECISION: i64 = 14;

// File history analyzer defaults.

/// Default file-history critical hotspot threshold.
pub const DEFAULT_FILE_HISTORY_HOTSPOT_CRITICAL: i64 = 50;
/// Default file-history high hotspot threshold.
pub const DEFAULT_FILE_HISTORY_HOTSPOT_HIGH: i64 = 30;
/// Default file-history medium hotspot threshold.
pub const DEFAULT_FILE_HISTORY_HOTSPOT_MEDIUM: i64 = 15;

// Imports analyzer defaults.

/// Default imports goroutines.
pub const DEFAULT_IMPORTS_GOROUTINES: i64 = 4;
/// Default imports maximum file size (1 MiB).
pub const DEFAULT_IMPORTS_MAX_FILE_SIZE: i64 = 1 << 20;
/// Default imports maximum dependency-risk rows.
pub const DEFAULT_IMPORTS_MAX_DEPENDENCY_RISK_ROWS: i64 = 30;

// Sentiment analyzer defaults.

/// Default sentiment minimum comment length.
pub const DEFAULT_SENTIMENT_MIN_COMMENT_LENGTH: i64 = 20;
/// Default sentiment gap.
pub const DEFAULT_SENTIMENT_GAP: f64 = 0.5;
/// Default sentiment neutralizer weight.
pub const DEFAULT_SENTIMENT_NEUTRALIZER_WEIGHT: f64 = 0.8;
/// Default sentiment maximum weight ratio.
pub const DEFAULT_SENTIMENT_MAX_WEIGHT_RATIO: f64 = 3.0;
/// Default sentiment positive threshold.
pub const DEFAULT_SENTIMENT_POSITIVE_THRESHOLD: f64 = 0.6;
/// Default sentiment negative threshold.
pub const DEFAULT_SENTIMENT_NEGATIVE_THRESHOLD: f64 = 0.4;
/// Default sentiment trend threshold.
pub const DEFAULT_SENTIMENT_TREND_THRESHOLD: f64 = 0.1;
/// Default low-sentiment risk threshold.
pub const DEFAULT_SENTIMENT_LOW_SENTIMENT_RISK_THRESH: f64 = 0.2;

// Shotness analyzer defaults.

/// Default shotness structural DSL.
pub const DEFAULT_SHOTNESS_DSL_STRUCT: &str = r#"filter(.roles has "Function")"#;
/// Default shotness name DSL.
pub const DEFAULT_SHOTNESS_DSL_NAME: &str = ".props.name";

// Typos analyzer defaults.

/// Default typos maximum edit distance.
pub const DEFAULT_TYPOS_MAX_DISTANCE: i64 = 4;

// Anomaly analyzer defaults.

/// Default anomaly threshold.
pub const DEFAULT_ANOMALY_THRESHOLD: f64 = 2.0;
/// Default anomaly window size.
pub const DEFAULT_ANOMALY_WINDOW_SIZE: i64 = 20;

// Clones analyzer defaults.

/// Default clones maximum clone pairs.
pub const DEFAULT_CLONES_MAX_CLONE_PAIRS: i64 = 1000;
/// Default clones number of hashes.
pub const DEFAULT_CLONES_NUM_HASHES: i64 = 128;
/// Default clones number of bands.
pub const DEFAULT_CLONES_NUM_BANDS: i64 = 16;
/// Default clones number of rows.
pub const DEFAULT_CLONES_NUM_ROWS: i64 = 8;
/// Default clones shingle size.
pub const DEFAULT_CLONES_SHINGLE_SIZE: i64 = 5;
/// Default clones type-2 similarity.
pub const DEFAULT_CLONES_SIMILARITY_TYPE2: f64 = 0.8;
/// Default clones type-3 similarity.
pub const DEFAULT_CLONES_SIMILARITY_TYPE3: f64 = 0.5;
/// Default clones yellow threshold ratio.
pub const DEFAULT_CLONES_THRESHOLD_RATIO_YELLOW: f64 = 0.1;
/// Default clones red threshold ratio.
pub const DEFAULT_CLONES_THRESHOLD_RATIO_RED: f64 = 0.3;
/// Default clones yellow threshold pairs.
pub const DEFAULT_CLONES_THRESHOLD_PAIRS_YELLOW: i64 = 5;
/// Default clones red threshold pairs.
pub const DEFAULT_CLONES_THRESHOLD_PAIRS_RED: i64 = 20;

// Checkpoint defaults.

/// Default checkpoint enabled flag.
pub const DEFAULT_CHECKPOINT_ENABLED: bool = true;
/// Default checkpoint directory.
pub const DEFAULT_CHECKPOINT_DIR: &str = "";
/// Default checkpoint resume flag.
pub const DEFAULT_CHECKPOINT_RESUME: bool = true;
/// Default checkpoint clear-previous flag.
pub const DEFAULT_CHECKPOINT_CLEAR_PREV: bool = false;
