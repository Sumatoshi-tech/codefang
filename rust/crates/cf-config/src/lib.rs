//! cf-config — configuration model + layered loader.
//!
//! Implements the layered precedence **flag > env `CODEFANG_` > file >
//! default** (CLI compatibility contract) and the analyzer / pipeline /
//! history / checkpoint configuration constants. The crate emits no
//! machine-format report bytes, so it does not route through the `cf-gojson` /
//! `cf-goyaml` serialization crates; `serde_yaml` is used purely to *parse*
//! the `.codefang.yaml` input file.
//!
//! # Layout
//! - [`defaults`] — every `DEFAULT_*` constant.
//! - [`Config`] and the per-section structs + [`Config::validate`].
//! - [`Config::apply_to_facts`] — merge into the analyzer facts map.
//! - [`load_config`] — the layered loader.
//!
//! # Example
//! ```
//! use cf_config::{load_from_yaml_and_env, defaults};
//!
//! // env overrides file overrides default.
//! let cfg = load_from_yaml_and_env("pipeline:\n  workers: 8\n", &|name| {
//!     (name == "CODEFANG_PIPELINE_GOGC").then(|| "200".to_owned())
//! })
//! .unwrap();
//! assert_eq!(cfg.pipeline.workers, 8); // from file
//! assert_eq!(cfg.pipeline.gogc, 200); // from env (overrides default 0)
//! assert_eq!(cfg.history.typos.max_distance, defaults::DEFAULT_TYPOS_MAX_DISTANCE);
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod defaults;
mod facts;
mod loader;
mod types;

pub use facts::{FactValue, Facts};
pub use loader::{
    apply_env_overrides, config_file_path, load_config, load_from_yaml_and_env, LoadError,
};
pub use types::{
    AnomalyConfig, BurndownConfig, ClonesConfig, Config, ConfigError, CouplesConfig, DevsConfig,
    FileHistoryConfig, HistoryConfig, ImportsConfig, PipelineConfig, SentimentConfig,
    ShotnessConfig, TyposConfig,
};
pub use types::CheckpointConfig;

impl Config {
    /// Returns the all-zero config (all numeric/string/bool fields zeroed).
    ///
    /// This is distinct from [`Config::default`], which returns the config
    /// populated with the registered defaults. Use `zero()` where unspecified
    /// fields must stay zero rather than fall back to defaults.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            analyzers: Vec::new(),
            pipeline: PipelineConfig {
                workers: 0,
                memory_budget: String::new(),
                blob_cache_size: String::new(),
                diff_cache_size: 0,
                blob_arena_size: String::new(),
                commit_batch_size: 0,
                gogc: 0,
                ballast_size: String::new(),
                memory_limit: String::new(),
                worker_timeout: String::new(),
                uast_spill_threshold: 0,
                intra_commit_parallel_threshold: 0,
                max_intra_commit_workers: 0,
                max_uast_blob_size: 0,
                uast_parse_timeout: String::new(),
                max_changes_per_commit: 0,
                max_diff_batch_size: 0,
                memory_budget_ratio: 0,
                memory_budget_cap: String::new(),
                memory_limit_ratio: 0,
                uast_spill_trim_interval: 0,
                native_trim_interval: 0,
                max_streaming_buffering: 0,
                drain_prefetch_timeout: String::new(),
                sampler_interval: String::new(),
                worker_ratio: 0,
                uast_worker_ratio: 0,
                leaf_worker_divisor: 0,
                min_leaf_workers: 0,
                buffer_size_multiplier: 0,
                budget_limit_ratio: 0,
                system_ram_limit_ratio: 0,
                static_max_workers: 0,
                malloc_trim_interval: 0,
                static_memory_limit_ratio: 0,
                diff_job_buffer_multiplier: 0,
            },
            history: HistoryConfig {
                burndown: BurndownConfig {
                    granularity: 0,
                    sampling: 0,
                    track_files: false,
                    track_people: false,
                    hibernation_threshold: 0,
                    hibernation_to_disk: false,
                    hibernation_directory: String::new(),
                    debug: false,
                    goroutines: 0,
                },
                couples: CouplesConfig {
                    coupling_threshold_high: 0,
                    ownership_few_threshold: 0,
                    ownership_moderate_threshold: 0,
                    batch_coupling_threshold: 0,
                    hll_precision: 0,
                    top_k_per_file: 0,
                    min_edge_weight: 0,
                },
                devs: DevsConfig {
                    consider_empty_commits: false,
                    anonymize: false,
                    bus_factor_threshold: 0.0,
                    risk_threshold_critical: 0.0,
                    risk_threshold_high: 0.0,
                    risk_threshold_medium: 0.0,
                    active_threshold_ratio: 0.0,
                    default_active_days: 0,
                    hll_precision: 0,
                },
                file_history: FileHistoryConfig {
                    hotspot_threshold_critical: 0,
                    hotspot_threshold_high: 0,
                    hotspot_threshold_medium: 0,
                },
                imports: ImportsConfig {
                    goroutines: 0,
                    max_file_size: 0,
                    max_dependency_risk_rows: 0,
                },
                sentiment: SentimentConfig {
                    min_comment_length: 0,
                    gap: 0.0,
                    neutralizer_weight: 0.0,
                    max_weight_ratio: 0.0,
                    positive_threshold: 0.0,
                    negative_threshold: 0.0,
                    trend_threshold: 0.0,
                    low_sentiment_risk_thresh: 0.0,
                },
                shotness: ShotnessConfig {
                    dsl_struct: String::new(),
                    dsl_name: String::new(),
                },
                typos: TyposConfig { max_distance: 0 },
                anomaly: AnomalyConfig {
                    threshold: 0.0,
                    window_size: 0,
                },
                clones: ClonesConfig {
                    max_clone_pairs: 0,
                    num_hashes: 0,
                    num_bands: 0,
                    num_rows: 0,
                    shingle_size: 0,
                    similarity_type2: 0.0,
                    similarity_type3: 0.0,
                    threshold_ratio_yellow: 0.0,
                    threshold_ratio_red: 0.0,
                    threshold_pairs_yellow: 0,
                    threshold_pairs_red: 0,
                },
            },
            checkpoint: CheckpointConfig {
                enabled: false,
                dir: String::new(),
                resume: false,
                clear_prev: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No env vars set.
    fn no_env(_name: &str) -> Option<String> {
        None
    }

    // ---- loader tests ---------------------------------------------------------

    #[test]
    fn load_config_no_file_uses_defaults() {
        // Mirrors TestLoadConfig_NoFile_UsesDefaults: an empty YAML file yields
        // the registered defaults.
        let cfg = load_from_yaml_and_env("", &no_env).unwrap();

        assert!(cfg.analyzers.is_empty());
        assert_eq!(cfg.pipeline.workers, defaults::DEFAULT_PIPELINE_WORKERS);
        assert_eq!(cfg.pipeline.gogc, defaults::DEFAULT_PIPELINE_GOGC);
        assert_eq!(cfg.pipeline.ballast_size, defaults::DEFAULT_PIPELINE_BALLAST_SIZE);
        assert_eq!(cfg.history.burndown.granularity, defaults::DEFAULT_BURNDOWN_GRANULARITY);
        assert_eq!(cfg.history.burndown.sampling, defaults::DEFAULT_BURNDOWN_SAMPLING);
        assert_eq!(cfg.history.burndown.track_files, defaults::DEFAULT_BURNDOWN_TRACK_FILES);
        assert_eq!(
            cfg.history.burndown.hibernation_threshold,
            defaults::DEFAULT_BURNDOWN_HIBERNATION_THRESHOLD
        );
        assert_eq!(
            cfg.history.devs.consider_empty_commits,
            defaults::DEFAULT_DEVS_CONSIDER_EMPTY_COMMITS
        );
        assert_eq!(cfg.history.devs.anonymize, defaults::DEFAULT_DEVS_ANONYMIZE);
        assert_eq!(cfg.history.imports.goroutines, defaults::DEFAULT_IMPORTS_GOROUTINES);
        assert_eq!(cfg.history.imports.max_file_size, defaults::DEFAULT_IMPORTS_MAX_FILE_SIZE);
        assert_eq!(
            cfg.history.sentiment.min_comment_length,
            defaults::DEFAULT_SENTIMENT_MIN_COMMENT_LENGTH
        );
        assert!((cfg.history.sentiment.gap - defaults::DEFAULT_SENTIMENT_GAP).abs() < 0.001);
        assert_eq!(cfg.history.shotness.dsl_struct, defaults::DEFAULT_SHOTNESS_DSL_STRUCT);
        assert_eq!(cfg.history.shotness.dsl_name, defaults::DEFAULT_SHOTNESS_DSL_NAME);
        assert_eq!(cfg.history.typos.max_distance, defaults::DEFAULT_TYPOS_MAX_DISTANCE);
        assert_eq!(cfg.checkpoint.enabled, defaults::DEFAULT_CHECKPOINT_ENABLED);
        assert_eq!(cfg.checkpoint.resume, defaults::DEFAULT_CHECKPOINT_RESUME);
    }

    #[test]
    fn load_config_valid_file_unmarshals() {
        // Mirrors TestLoadConfig_ValidFile_Unmarshals.
        let content = r#"analyzers:
  - burndown
  - complexity
pipeline:
  workers: 8
  memory_budget: "4GB"
  blob_cache_size: "512MB"
  diff_cache_size: 5000
  commit_batch_size: 200
  gogc: 200
  ballast_size: "256MB"
history:
  burndown:
    granularity: 15
    sampling: 15
    track_files: true
    track_people: true
    hibernation_threshold: 2000
  devs:
    consider_empty_commits: true
    anonymize: true
  imports:
    goroutines: 8
    max_file_size: 2097152
  sentiment:
    min_comment_length: 30
    gap: 0.7
  shotness:
    dsl_struct: 'filter(.roles has "Class")'
    dsl_name: ".props.identifier"
  typos:
    max_distance: 3
checkpoint:
  enabled: false
  dir: "/tmp/ckpt"
  resume: false
  clear_prev: true
"#;
        let cfg = load_from_yaml_and_env(content, &no_env).unwrap();

        assert_eq!(cfg.analyzers, vec!["burndown".to_owned(), "complexity".to_owned()]);
        assert_eq!(cfg.pipeline.workers, 8);
        assert_eq!(cfg.pipeline.memory_budget, "4GB");
        assert_eq!(cfg.pipeline.blob_cache_size, "512MB");
        assert_eq!(cfg.pipeline.diff_cache_size, 5000);
        assert_eq!(cfg.pipeline.commit_batch_size, 200);
        assert_eq!(cfg.pipeline.gogc, 200);
        assert_eq!(cfg.pipeline.ballast_size, "256MB");

        assert_eq!(cfg.history.burndown.granularity, 15);
        assert_eq!(cfg.history.burndown.sampling, 15);
        assert!(cfg.history.burndown.track_files);
        assert!(cfg.history.burndown.track_people);
        assert_eq!(cfg.history.burndown.hibernation_threshold, 2000);

        assert!(cfg.history.devs.consider_empty_commits);
        assert!(cfg.history.devs.anonymize);

        assert_eq!(cfg.history.imports.goroutines, 8);
        assert_eq!(cfg.history.imports.max_file_size, 2_097_152);

        assert_eq!(cfg.history.sentiment.min_comment_length, 30);
        assert!((cfg.history.sentiment.gap - 0.7).abs() < 0.001);

        assert_eq!(cfg.history.shotness.dsl_struct, r#"filter(.roles has "Class")"#);
        assert_eq!(cfg.history.shotness.dsl_name, ".props.identifier");

        assert_eq!(cfg.history.typos.max_distance, 3);

        assert!(!cfg.checkpoint.enabled);
        assert_eq!(cfg.checkpoint.dir, "/tmp/ckpt");
        assert!(!cfg.checkpoint.resume);
        assert!(cfg.checkpoint.clear_prev);
    }

    #[test]
    fn load_config_explicit_path_overrides() {
        // Mirrors TestLoadConfig_ExplicitPath_Overrides via the temp-file path.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom-config.yaml");
        std::fs::write(&path, "pipeline:\n  workers: 16\n").unwrap();

        let cfg = load_config(path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.pipeline.workers, 16);
    }

    #[test]
    fn load_config_malformed_yaml_returns_error() {
        // Mirrors TestLoadConfig_MalformedYAML_ReturnsError.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, "pipeline:\n  workers: [invalid yaml\n").unwrap();

        let err = load_config(path.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("read config"), "got: {err}");
    }

    #[test]
    fn load_config_unknown_keys_no_error() {
        // Mirrors TestLoadConfig_UnknownKeys_NoError: unknown keys are ignored
        // (serde ignores unknown fields by default, like the reference loader).
        let content = "unknown_section:\n  unknown_key: \"value\"\npipeline:\n  workers: 4\n";
        let cfg = load_from_yaml_and_env(content, &no_env).unwrap();
        assert_eq!(cfg.pipeline.workers, 4);
    }

    #[test]
    fn load_config_empty_analyzers_nil_slice() {
        // Mirrors TestLoadConfig_EmptyAnalyzers_NilSlice.
        let cfg = load_from_yaml_and_env("analyzers: []\n", &no_env).unwrap();
        assert!(cfg.analyzers.is_empty());
    }

    #[test]
    fn load_config_partial_config_merges_defaults() {
        // Mirrors TestLoadConfig_PartialConfig_MergesDefaults.
        let content = "history:\n  burndown:\n    granularity: 60\n";
        let cfg = load_from_yaml_and_env(content, &no_env).unwrap();

        assert_eq!(cfg.history.burndown.granularity, 60);
        assert_eq!(cfg.history.burndown.sampling, defaults::DEFAULT_BURNDOWN_SAMPLING);
        assert_eq!(cfg.pipeline.workers, defaults::DEFAULT_PIPELINE_WORKERS);
        assert_eq!(cfg.history.typos.max_distance, defaults::DEFAULT_TYPOS_MAX_DISTANCE);
    }

    #[test]
    fn load_config_env_override_pipeline() {
        // Mirrors TestLoadConfig_EnvOverride_Pipeline.
        let cfg = load_from_yaml_and_env("", &|name| {
            (name == "CODEFANG_PIPELINE_WORKERS").then(|| "32".to_owned())
        })
        .unwrap();
        assert_eq!(cfg.pipeline.workers, 32);
    }

    #[test]
    fn load_config_env_override_nested_key() {
        // Mirrors TestLoadConfig_EnvOverride_NestedKey.
        let cfg = load_from_yaml_and_env("", &|name| {
            (name == "CODEFANG_HISTORY_BURNDOWN_GRANULARITY").then(|| "60".to_owned())
        })
        .unwrap();
        assert_eq!(cfg.history.burndown.granularity, 60);
    }

    #[test]
    fn load_config_explicit_path_not_found_returns_error() {
        // Mirrors TestLoadConfig_ExplicitPath_NotFound_ReturnsError.
        let err = load_config("/nonexistent/path/config.yaml").unwrap_err();
        assert!(matches!(err, LoadError::Read(_)), "got: {err}");
    }

    #[test]
    fn env_overrides_file() {
        // Precedence check: env > file (flag > env > file > default chain).
        let cfg = load_from_yaml_and_env("pipeline:\n  workers: 8\n", &|name| {
            (name == "CODEFANG_PIPELINE_WORKERS").then(|| "32".to_owned())
        })
        .unwrap();
        assert_eq!(cfg.pipeline.workers, 32);
    }

    // ---- types tests ----------------------------------------------------------

    fn valid_config() -> Config {
        let mut c = Config::zero();
        c.pipeline.workers = 4;
        c.pipeline.diff_cache_size = 1000;
        c.pipeline.commit_batch_size = 100;
        c.pipeline.gogc = 200;
        c.pipeline.ballast_size = "0".to_owned();
        c.history.burndown.granularity = 30;
        c.history.burndown.sampling = 30;
        c.history.burndown.hibernation_threshold = 1000;
        c.history.imports.goroutines = 4;
        c.history.imports.max_file_size = 1 << 20;
        c.history.sentiment.min_comment_length = 20;
        c.history.sentiment.gap = 0.5;
        c.history.typos.max_distance = 4;
        c.checkpoint.enabled = true;
        c.checkpoint.resume = true;
        c
    }

    #[test]
    fn validate_valid_config_no_error() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn validate_zero_config_no_error() {
        // Mirrors TestValidate_ZeroConfig_NoError: Config{} is valid.
        assert!(Config::zero().validate().is_ok());
    }

    #[test]
    fn validate_invalid_workers() {
        let mut cfg = valid_config();
        cfg.pipeline.workers = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidWorkers));
    }

    #[test]
    fn validate_invalid_diff_cache_size() {
        let mut cfg = valid_config();
        cfg.pipeline.diff_cache_size = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidDiffCacheSize));
    }

    #[test]
    fn validate_invalid_commit_batch_size() {
        let mut cfg = valid_config();
        cfg.pipeline.commit_batch_size = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidCommitBatchSize));
    }

    #[test]
    fn validate_invalid_gogc() {
        let mut cfg = valid_config();
        cfg.pipeline.gogc = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidGogc));
    }

    #[test]
    fn validate_invalid_burndown_granularity() {
        let mut cfg = valid_config();
        cfg.history.burndown.granularity = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidBurndownGranularity));
    }

    #[test]
    fn validate_invalid_burndown_sampling() {
        let mut cfg = valid_config();
        cfg.history.burndown.sampling = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidBurndownSampling));
    }

    #[test]
    fn validate_invalid_sentiment_min_length() {
        let mut cfg = valid_config();
        cfg.history.sentiment.min_comment_length = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidSentimentMinLength));
    }

    #[test]
    fn validate_invalid_sentiment_gap_negative() {
        let mut cfg = valid_config();
        cfg.history.sentiment.gap = -0.1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidSentimentGap));
    }

    #[test]
    fn validate_invalid_sentiment_gap_too_high() {
        let mut cfg = valid_config();
        cfg.history.sentiment.gap = 1.1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidSentimentGap));
    }

    #[test]
    fn validate_invalid_typos_max_distance() {
        let mut cfg = valid_config();
        cfg.history.typos.max_distance = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidTyposMaxDistance));
    }

    #[test]
    fn validate_invalid_imports_goroutines() {
        let mut cfg = valid_config();
        cfg.history.imports.goroutines = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidImportsGoroutines));
    }

    #[test]
    fn validate_invalid_imports_max_file_size() {
        let mut cfg = valid_config();
        cfg.history.imports.max_file_size = -1;
        assert_eq!(cfg.validate(), Err(ConfigError::InvalidImportsMaxFileSize));
    }

    #[test]
    fn config_error_messages_are_frozen() {
        // Guards the byte-identical sentinel wording used by `validate config: ...`.
        assert_eq!(ConfigError::InvalidWorkers.message(), "pipeline.workers must be non-negative");
        assert_eq!(
            ConfigError::InvalidAnomalyWindowSize.message(),
            "history.anomaly.window_size must be at least 2"
        );
        assert_eq!(
            ConfigError::InvalidCouplesHllPrecision.message(),
            "history.couples.hll_precision must be between 4 and 18"
        );
        assert_eq!(
            LoadError::Validate(ConfigError::InvalidWorkers).to_string(),
            "validate config: pipeline.workers must be non-negative"
        );
    }

    // ---- apply_to_facts tests -------------------------------------------------

    const FACT_BURNDOWN_GRANULARITY: &str = "Burndown.Granularity";
    const FACT_BURNDOWN_SAMPLING: &str = "Burndown.Sampling";
    const FACT_BURNDOWN_TRACK_FILES: &str = "Burndown.TrackFiles";
    const FACT_BURNDOWN_TRACK_PEOPLE: &str = "Burndown.TrackPeople";
    const FACT_BURNDOWN_HIBERNATION_THRESHOLD: &str = "Burndown.HibernationThreshold";
    const FACT_BURNDOWN_HIBERNATION_ON_DISK: &str = "Burndown.HibernationOnDisk";
    const FACT_BURNDOWN_HIBERNATION_DIRECTORY: &str = "Burndown.HibernationDirectory";
    const FACT_BURNDOWN_DEBUG: &str = "Burndown.Debug";
    const FACT_BURNDOWN_GOROUTINES: &str = "Burndown.Goroutines";
    const FACT_DEVS_CONSIDER_EMPTY: &str = "Devs.ConsiderEmptyCommits";
    const FACT_DEVS_ANONYMIZE: &str = "Devs.Anonymize";
    const FACT_IMPORTS_GOROUTINES: &str = "Imports.Goroutines";
    const FACT_IMPORTS_MAX_FILE_SIZE: &str = "Imports.MaxFileSize";
    const FACT_SENTIMENT_MIN_LENGTH: &str = "CommentSentiment.MinLength";
    const FACT_SENTIMENT_GAP: &str = "CommentSentiment.Gap";
    const FACT_SHOTNESS_DSL_STRUCT: &str = "Shotness.DSLStruct";
    const FACT_SHOTNESS_DSL_NAME: &str = "Shotness.DSLName";
    const FACT_TYPOS_MAX_DISTANCE: &str = "TyposDatasetBuilder.MaximumAllowedDistance";
    const FACT_ANOMALY_THRESHOLD: &str = "TemporalAnomaly.Threshold";
    const FACT_ANOMALY_WINDOW_SIZE: &str = "TemporalAnomaly.WindowSize";

    #[test]
    fn apply_to_facts_burndown() {
        let mut cfg = Config::zero();
        cfg.history.burndown = BurndownConfig {
            granularity: 60,
            sampling: 60,
            track_files: true,
            track_people: true,
            hibernation_threshold: 2000,
            hibernation_to_disk: true,
            hibernation_directory: "/tmp/hib".to_owned(),
            debug: true,
            goroutines: 16,
        };

        let mut facts = Facts::new();
        cfg.apply_to_facts(&mut facts);

        assert_eq!(facts[FACT_BURNDOWN_GRANULARITY].as_int(), Some(60));
        assert_eq!(facts[FACT_BURNDOWN_SAMPLING].as_int(), Some(60));
        assert_eq!(facts[FACT_BURNDOWN_TRACK_FILES].as_bool(), Some(true));
        assert_eq!(facts[FACT_BURNDOWN_TRACK_PEOPLE].as_bool(), Some(true));
        assert_eq!(facts[FACT_BURNDOWN_HIBERNATION_THRESHOLD].as_int(), Some(2000));
        assert_eq!(facts[FACT_BURNDOWN_HIBERNATION_ON_DISK].as_bool(), Some(true));
        assert_eq!(facts[FACT_BURNDOWN_HIBERNATION_DIRECTORY].as_str(), Some("/tmp/hib"));
        assert_eq!(facts[FACT_BURNDOWN_DEBUG].as_bool(), Some(true));
        assert_eq!(facts[FACT_BURNDOWN_GOROUTINES].as_int(), Some(16));
    }

    #[test]
    fn apply_to_facts_devs() {
        let mut cfg = Config::zero();
        cfg.history.devs.consider_empty_commits = true;
        cfg.history.devs.anonymize = true;

        let mut facts = Facts::new();
        cfg.apply_to_facts(&mut facts);

        assert_eq!(facts[FACT_DEVS_CONSIDER_EMPTY].as_bool(), Some(true));
        assert_eq!(facts[FACT_DEVS_ANONYMIZE].as_bool(), Some(true));
    }

    #[test]
    fn apply_to_facts_imports() {
        let mut cfg = Config::zero();
        cfg.history.imports.goroutines = 16;
        cfg.history.imports.max_file_size = 4_194_304;

        let mut facts = Facts::new();
        cfg.apply_to_facts(&mut facts);

        assert_eq!(facts[FACT_IMPORTS_GOROUTINES].as_int(), Some(16));
        assert_eq!(facts[FACT_IMPORTS_MAX_FILE_SIZE].as_int(), Some(4_194_304));
    }

    #[test]
    fn apply_to_facts_sentiment() {
        let mut cfg = Config::zero();
        cfg.history.sentiment.min_comment_length = 40;
        cfg.history.sentiment.gap = 0.8;

        let mut facts = Facts::new();
        cfg.apply_to_facts(&mut facts);

        assert_eq!(facts[FACT_SENTIMENT_MIN_LENGTH].as_int(), Some(40));
        assert!((facts[FACT_SENTIMENT_GAP].as_float().unwrap() - 0.8).abs() < 0.001);
    }

    #[test]
    fn apply_to_facts_shotness() {
        let mut cfg = Config::zero();
        cfg.history.shotness.dsl_struct = r#"filter(.roles has "Class")"#.to_owned();
        cfg.history.shotness.dsl_name = ".props.identifier".to_owned();

        let mut facts = Facts::new();
        cfg.apply_to_facts(&mut facts);

        assert_eq!(facts[FACT_SHOTNESS_DSL_STRUCT].as_str(), Some(r#"filter(.roles has "Class")"#));
        assert_eq!(facts[FACT_SHOTNESS_DSL_NAME].as_str(), Some(".props.identifier"));
    }

    #[test]
    fn apply_to_facts_typos() {
        let mut cfg = Config::zero();
        cfg.history.typos.max_distance = 6;

        let mut facts = Facts::new();
        cfg.apply_to_facts(&mut facts);

        assert_eq!(facts[FACT_TYPOS_MAX_DISTANCE].as_int(), Some(6));
    }

    #[test]
    fn apply_to_facts_anomaly() {
        let mut cfg = Config::zero();
        cfg.history.anomaly.threshold = 3.5;
        cfg.history.anomaly.window_size = 30;

        let mut facts = Facts::new();
        cfg.apply_to_facts(&mut facts);

        assert!((facts[FACT_ANOMALY_THRESHOLD].as_float().unwrap() - 3.5).abs() < 0.001);
        assert_eq!(facts[FACT_ANOMALY_WINDOW_SIZE].as_int(), Some(30));
    }

    #[test]
    fn apply_to_facts_zero_values_skips_numeric_overrides() {
        // Mirrors TestApplyToFacts_ZeroValues_SkipsNumericOverrides.
        let cfg = Config::zero();
        let mut facts = Facts::new();
        facts.insert(FACT_BURNDOWN_GRANULARITY.to_owned(), FactValue::Int(30));
        facts.insert(FACT_TYPOS_MAX_DISTANCE.to_owned(), FactValue::Int(4));

        cfg.apply_to_facts(&mut facts);

        assert_eq!(facts[FACT_BURNDOWN_GRANULARITY].as_int(), Some(30));
        assert_eq!(facts[FACT_TYPOS_MAX_DISTANCE].as_int(), Some(4));
    }

    #[test]
    fn apply_to_facts_boolean_fields_always_applied() {
        // Mirrors TestApplyToFacts_BooleanFields_AlwaysApplied: false overrides true.
        let mut cfg = Config::zero();
        cfg.history.burndown.track_files = false;
        cfg.history.burndown.track_people = false;
        cfg.history.burndown.debug = false;
        cfg.history.devs.consider_empty_commits = false;
        cfg.history.devs.anonymize = false;

        let mut facts = Facts::new();
        facts.insert(FACT_BURNDOWN_TRACK_FILES.to_owned(), FactValue::Bool(true));
        facts.insert(FACT_BURNDOWN_TRACK_PEOPLE.to_owned(), FactValue::Bool(true));

        cfg.apply_to_facts(&mut facts);

        assert_eq!(facts[FACT_BURNDOWN_TRACK_FILES].as_bool(), Some(false));
        assert_eq!(facts[FACT_BURNDOWN_TRACK_PEOPLE].as_bool(), Some(false));
        assert_eq!(facts[FACT_DEVS_CONSIDER_EMPTY].as_bool(), Some(false));
        assert_eq!(facts[FACT_DEVS_ANONYMIZE].as_bool(), Some(false));
    }
}
