//! Layered configuration loader.
//!
//! Precedence is **flag > env (`CODEFANG_`) > file > default** (CLI
//! compatibility contract):
//!
//! 1. **default** — [`Config::default`] (the registered default values).
//! 2. **file** — `.codefang.yaml` (explicit path, else CWD then `$HOME`),
//!    deserialized over the defaults via `serde(default)`.
//! 3. **env** — every `CODEFANG_<UPPER_SNAKE>` variable overrides the matching
//!    dotted key (`.` maps to `_` under the `CODEFANG` prefix).
//! 4. **flag** — explicit programmatic overrides applied last (supplied by the
//!    caller, since clap lives in `cf-commands`).

use crate::types::{Config, ConfigError};
use std::env;
use std::path::{Path, PathBuf};

/// Config file base name without extension.
const CONFIG_NAME: &str = ".codefang";
/// Config file extension.
const CONFIG_EXT: &str = "yaml";
/// Environment variable prefix.
const ENV_PREFIX: &str = "CODEFANG";

/// Error returned by [`load_config`].
///
/// The `Display` wrapping (`read config: ...`, `unmarshal config: ...`,
/// `validate config: ...`) is part of the CLI compatibility contract, so any
/// error-substring assertions continue to hold.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Reading the config file failed (`read config: <cause>`).
    ///
    /// A *missing* file at a searched path is NOT an error (defaults are
    /// used), but an explicitly-set path that cannot be read, or a malformed
    /// YAML document, is wrapped here.
    #[error("read config: {0}")]
    Read(String),
    /// Deserializing the merged config into the struct failed
    /// (`unmarshal config: <cause>`).
    #[error("unmarshal config: {0}")]
    Unmarshal(String),
    /// Validation failed (`validate config: <cause>`).
    #[error("validate config: {0}")]
    Validate(#[source] ConfigError),
}

/// Loads configuration from file, environment variables, and defaults.
///
/// If `config_path` is non-empty it is used as the explicit config file path;
/// otherwise `.codefang.yaml` is searched in CWD and `$HOME`. A missing config
/// file (when searching) is not an error — defaults are used. An explicit
/// path that does not exist *is* an error (reference loader behavior).
///
/// # Errors
/// - [`LoadError::Read`] if an explicit config file cannot be read or any
///   located file is malformed YAML.
/// - [`LoadError::Unmarshal`] if the YAML cannot be deserialized into [`Config`].
/// - [`LoadError::Validate`] if [`Config::validate`] fails.
pub fn load_config(config_path: &str) -> Result<Config, LoadError> {
    // Layers 1+2: file over defaults (missing fields fall back via
    // serde(default)), or pure defaults when no file is found.
    let mut cfg = match read_config_source(config_path)? {
        Some(contents) => parse_yaml_over_defaults(&contents)?,
        None => Config::default(),
    };

    // Layer 3: environment (CODEFANG_*).
    apply_env_overrides(&mut cfg, &env_lookup);

    // Validate the merged result.
    cfg.validate().map_err(LoadError::Validate)?;

    Ok(cfg)
}

/// Reads the config file contents, or `None` when searching and no file exists.
///
/// Returns [`LoadError::Read`] for an explicit path that cannot be read.
fn read_config_source(config_path: &str) -> Result<Option<String>, LoadError> {
    if !config_path.is_empty() {
        // Explicit path: a missing/unreadable file is an error (reference
        // loader behavior).
        return std::fs::read_to_string(config_path)
            .map(Some)
            .map_err(|e| LoadError::Read(e.to_string()));
    }

    // Search CWD then $HOME for `.codefang.yaml`. A missing file is fine.
    for dir in search_paths() {
        let candidate = dir.join(format!("{CONFIG_NAME}.{CONFIG_EXT}"));
        match std::fs::read_to_string(&candidate) {
            Ok(contents) => return Ok(Some(contents)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(LoadError::Read(e.to_string())),
        }
    }

    Ok(None)
}

/// Returns the config search directories, in order: CWD, then `$HOME`.
fn search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(".")];
    if let Some(home) = home_dir() {
        paths.push(home);
    }
    paths
}

/// Best-effort `$HOME` resolution.
fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// Deserializes YAML over the registered defaults.
///
/// An empty document yields the all-defaults [`Config`]. Malformed YAML is
/// surfaced as [`LoadError::Read`] (`read config: ...`), matching the
/// reference loader's wrapping of parse errors.
fn parse_yaml_over_defaults(contents: &str) -> Result<Config, LoadError> {
    if contents.trim().is_empty() {
        return Ok(Config::default());
    }
    // serde(default) on every struct/field merges the file over the registered
    // defaults: absent keys fall back to the default values.
    serde_yaml::from_str::<Config>(contents).map_err(|e| LoadError::Read(e.to_string()))
}

/// Resolves a `CODEFANG_*` environment variable, returning its value if set.
fn env_lookup(name: &str) -> Option<String> {
    env::var(name).ok()
}

/// Applies `CODEFANG_*` environment overrides over `cfg` (env > file).
///
/// `lookup` resolves a full env-var name (e.g. `CODEFANG_PIPELINE_WORKERS`) to
/// its string value; the name is the dotted config key upper-cased with `.`
/// replaced by `_` under the `CODEFANG` prefix. Parameterizing the lookup
/// keeps the function testable without mutating the process environment.
pub fn apply_env_overrides<F>(cfg: &mut Config, lookup: &F)
where
    F: Fn(&str) -> Option<String>,
{
    for (key, setter) in ENV_FIELDS {
        let env_name = format!("{ENV_PREFIX}_{}", key.to_uppercase().replace('.', "_"));
        if let Some(raw) = lookup(&env_name) {
            setter(cfg, &raw);
        }
    }
    // analyzers (string list) — a single env var is split on whitespace
    // (reference loader behavior, kept for parity).
    let analyzers_name = format!("{ENV_PREFIX}_ANALYZERS");
    if let Some(raw) = lookup(&analyzers_name) {
        cfg.analyzers = parse_string_slice_env(&raw);
    }
}

/// Parses a string-list env value (whitespace-separated).
fn parse_string_slice_env(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}

/// Parses an integer env value (best-effort coercion).
///
/// Invalid integers leave the existing value unchanged: unparsable overrides
/// are conservatively ignored so a typo cannot silently zero a tuned value.
fn set_i64(target: &mut i64, raw: &str) {
    if let Ok(v) = raw.trim().parse::<i64>() {
        *target = v;
    }
}

/// Parses a float env value; ignores unparsable input.
fn set_f64(target: &mut f64, raw: &str) {
    if let Ok(v) = raw.trim().parse::<f64>() {
        *target = v;
    }
}

/// Parses a bool env value; ignores unparsable input.
fn set_bool(target: &mut bool, raw: &str) {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "t" | "true" | "yes" | "y" | "on" => *target = true,
        "0" | "f" | "false" | "no" | "n" | "off" => *target = false,
        _ => {}
    }
}

/// Sets a string field directly to the raw env value.
fn set_string(target: &mut String, raw: &str) {
    raw.clone_into(target);
}

/// Type of a per-key env override setter.
type EnvSetter = fn(&mut Config, &str);

/// Full table of `(dotted-key, setter)` pairs for env overrides.
///
/// [`apply_env_overrides`] derives the env-var name by upper-casing and
/// replacing `.` with `_` under the `CODEFANG_` prefix. Every scalar config
/// field appears here so the env layer is exhaustive.
#[rustfmt::skip]
const ENV_FIELDS: &[(&str, EnvSetter)] = &[
    // pipeline
    ("pipeline.workers", |c, v| set_i64(&mut c.pipeline.workers, v)),
    ("pipeline.memory_budget", |c, v| set_string(&mut c.pipeline.memory_budget, v)),
    ("pipeline.blob_cache_size", |c, v| set_string(&mut c.pipeline.blob_cache_size, v)),
    ("pipeline.diff_cache_size", |c, v| set_i64(&mut c.pipeline.diff_cache_size, v)),
    ("pipeline.blob_arena_size", |c, v| set_string(&mut c.pipeline.blob_arena_size, v)),
    ("pipeline.commit_batch_size", |c, v| set_i64(&mut c.pipeline.commit_batch_size, v)),
    ("pipeline.gogc", |c, v| set_i64(&mut c.pipeline.gogc, v)),
    ("pipeline.ballast_size", |c, v| set_string(&mut c.pipeline.ballast_size, v)),
    ("pipeline.memory_limit", |c, v| set_string(&mut c.pipeline.memory_limit, v)),
    ("pipeline.worker_timeout", |c, v| set_string(&mut c.pipeline.worker_timeout, v)),
    ("pipeline.uast_spill_threshold", |c, v| set_i64(&mut c.pipeline.uast_spill_threshold, v)),
    ("pipeline.intra_commit_parallel_threshold", |c, v| set_i64(&mut c.pipeline.intra_commit_parallel_threshold, v)),
    ("pipeline.max_intra_commit_workers", |c, v| set_i64(&mut c.pipeline.max_intra_commit_workers, v)),
    ("pipeline.max_uast_blob_size", |c, v| set_i64(&mut c.pipeline.max_uast_blob_size, v)),
    ("pipeline.uast_parse_timeout", |c, v| set_string(&mut c.pipeline.uast_parse_timeout, v)),
    ("pipeline.max_changes_per_commit", |c, v| set_i64(&mut c.pipeline.max_changes_per_commit, v)),
    ("pipeline.max_diff_batch_size", |c, v| set_i64(&mut c.pipeline.max_diff_batch_size, v)),
    ("pipeline.memory_budget_ratio", |c, v| set_i64(&mut c.pipeline.memory_budget_ratio, v)),
    ("pipeline.memory_budget_cap", |c, v| set_string(&mut c.pipeline.memory_budget_cap, v)),
    ("pipeline.memory_limit_ratio", |c, v| set_i64(&mut c.pipeline.memory_limit_ratio, v)),
    ("pipeline.uast_spill_trim_interval", |c, v| set_i64(&mut c.pipeline.uast_spill_trim_interval, v)),
    ("pipeline.native_trim_interval", |c, v| set_i64(&mut c.pipeline.native_trim_interval, v)),
    ("pipeline.max_streaming_buffering", |c, v| set_i64(&mut c.pipeline.max_streaming_buffering, v)),
    ("pipeline.drain_prefetch_timeout", |c, v| set_string(&mut c.pipeline.drain_prefetch_timeout, v)),
    ("pipeline.sampler_interval", |c, v| set_string(&mut c.pipeline.sampler_interval, v)),
    ("pipeline.worker_ratio", |c, v| set_i64(&mut c.pipeline.worker_ratio, v)),
    ("pipeline.uast_worker_ratio", |c, v| set_i64(&mut c.pipeline.uast_worker_ratio, v)),
    ("pipeline.leaf_worker_divisor", |c, v| set_i64(&mut c.pipeline.leaf_worker_divisor, v)),
    ("pipeline.min_leaf_workers", |c, v| set_i64(&mut c.pipeline.min_leaf_workers, v)),
    ("pipeline.buffer_size_multiplier", |c, v| set_i64(&mut c.pipeline.buffer_size_multiplier, v)),
    ("pipeline.budget_limit_ratio", |c, v| set_i64(&mut c.pipeline.budget_limit_ratio, v)),
    ("pipeline.system_ram_limit_ratio", |c, v| set_i64(&mut c.pipeline.system_ram_limit_ratio, v)),
    ("pipeline.static_max_workers", |c, v| set_i64(&mut c.pipeline.static_max_workers, v)),
    ("pipeline.malloc_trim_interval", |c, v| set_i64(&mut c.pipeline.malloc_trim_interval, v)),
    ("pipeline.static_memory_limit_ratio", |c, v| set_i64(&mut c.pipeline.static_memory_limit_ratio, v)),
    ("pipeline.diff_job_buffer_multiplier", |c, v| set_i64(&mut c.pipeline.diff_job_buffer_multiplier, v)),
    // history.burndown
    ("history.burndown.granularity", |c, v| set_i64(&mut c.history.burndown.granularity, v)),
    ("history.burndown.sampling", |c, v| set_i64(&mut c.history.burndown.sampling, v)),
    ("history.burndown.track_files", |c, v| set_bool(&mut c.history.burndown.track_files, v)),
    ("history.burndown.track_people", |c, v| set_bool(&mut c.history.burndown.track_people, v)),
    ("history.burndown.hibernation_threshold", |c, v| set_i64(&mut c.history.burndown.hibernation_threshold, v)),
    ("history.burndown.hibernation_to_disk", |c, v| set_bool(&mut c.history.burndown.hibernation_to_disk, v)),
    ("history.burndown.hibernation_directory", |c, v| set_string(&mut c.history.burndown.hibernation_directory, v)),
    ("history.burndown.debug", |c, v| set_bool(&mut c.history.burndown.debug, v)),
    ("history.burndown.goroutines", |c, v| set_i64(&mut c.history.burndown.goroutines, v)),
    // history.couples
    ("history.couples.coupling_threshold_high", |c, v| set_i64(&mut c.history.couples.coupling_threshold_high, v)),
    ("history.couples.ownership_few_threshold", |c, v| set_i64(&mut c.history.couples.ownership_few_threshold, v)),
    ("history.couples.ownership_moderate_threshold", |c, v| set_i64(&mut c.history.couples.ownership_moderate_threshold, v)),
    ("history.couples.batch_coupling_threshold", |c, v| set_i64(&mut c.history.couples.batch_coupling_threshold, v)),
    ("history.couples.hll_precision", |c, v| set_i64(&mut c.history.couples.hll_precision, v)),
    ("history.couples.top_k_per_file", |c, v| set_i64(&mut c.history.couples.top_k_per_file, v)),
    ("history.couples.min_edge_weight", |c, v| set_i64(&mut c.history.couples.min_edge_weight, v)),
    // history.devs
    ("history.devs.consider_empty_commits", |c, v| set_bool(&mut c.history.devs.consider_empty_commits, v)),
    ("history.devs.anonymize", |c, v| set_bool(&mut c.history.devs.anonymize, v)),
    ("history.devs.bus_factor_threshold", |c, v| set_f64(&mut c.history.devs.bus_factor_threshold, v)),
    ("history.devs.risk_threshold_critical", |c, v| set_f64(&mut c.history.devs.risk_threshold_critical, v)),
    ("history.devs.risk_threshold_high", |c, v| set_f64(&mut c.history.devs.risk_threshold_high, v)),
    ("history.devs.risk_threshold_medium", |c, v| set_f64(&mut c.history.devs.risk_threshold_medium, v)),
    ("history.devs.active_threshold_ratio", |c, v| set_f64(&mut c.history.devs.active_threshold_ratio, v)),
    ("history.devs.default_active_days", |c, v| set_i64(&mut c.history.devs.default_active_days, v)),
    ("history.devs.hll_precision", |c, v| set_i64(&mut c.history.devs.hll_precision, v)),
    // history.file_history
    ("history.file_history.hotspot_threshold_critical", |c, v| set_i64(&mut c.history.file_history.hotspot_threshold_critical, v)),
    ("history.file_history.hotspot_threshold_high", |c, v| set_i64(&mut c.history.file_history.hotspot_threshold_high, v)),
    ("history.file_history.hotspot_threshold_medium", |c, v| set_i64(&mut c.history.file_history.hotspot_threshold_medium, v)),
    // history.imports
    ("history.imports.goroutines", |c, v| set_i64(&mut c.history.imports.goroutines, v)),
    ("history.imports.max_file_size", |c, v| set_i64(&mut c.history.imports.max_file_size, v)),
    ("history.imports.max_dependency_risk_rows", |c, v| set_i64(&mut c.history.imports.max_dependency_risk_rows, v)),
    // history.sentiment
    ("history.sentiment.min_comment_length", |c, v| set_i64(&mut c.history.sentiment.min_comment_length, v)),
    ("history.sentiment.gap", |c, v| set_f64(&mut c.history.sentiment.gap, v)),
    ("history.sentiment.neutralizer_weight", |c, v| set_f64(&mut c.history.sentiment.neutralizer_weight, v)),
    ("history.sentiment.max_weight_ratio", |c, v| set_f64(&mut c.history.sentiment.max_weight_ratio, v)),
    ("history.sentiment.positive_threshold", |c, v| set_f64(&mut c.history.sentiment.positive_threshold, v)),
    ("history.sentiment.negative_threshold", |c, v| set_f64(&mut c.history.sentiment.negative_threshold, v)),
    ("history.sentiment.trend_threshold", |c, v| set_f64(&mut c.history.sentiment.trend_threshold, v)),
    ("history.sentiment.low_sentiment_risk_threshold", |c, v| set_f64(&mut c.history.sentiment.low_sentiment_risk_thresh, v)),
    // history.shotness
    ("history.shotness.dsl_struct", |c, v| set_string(&mut c.history.shotness.dsl_struct, v)),
    ("history.shotness.dsl_name", |c, v| set_string(&mut c.history.shotness.dsl_name, v)),
    // history.typos
    ("history.typos.max_distance", |c, v| set_i64(&mut c.history.typos.max_distance, v)),
    // history.anomaly
    ("history.anomaly.threshold", |c, v| set_f64(&mut c.history.anomaly.threshold, v)),
    ("history.anomaly.window_size", |c, v| set_i64(&mut c.history.anomaly.window_size, v)),
    // history.clones
    ("history.clones.max_clone_pairs", |c, v| set_i64(&mut c.history.clones.max_clone_pairs, v)),
    ("history.clones.num_hashes", |c, v| set_i64(&mut c.history.clones.num_hashes, v)),
    ("history.clones.num_bands", |c, v| set_i64(&mut c.history.clones.num_bands, v)),
    ("history.clones.num_rows", |c, v| set_i64(&mut c.history.clones.num_rows, v)),
    ("history.clones.shingle_size", |c, v| set_i64(&mut c.history.clones.shingle_size, v)),
    ("history.clones.similarity_type2", |c, v| set_f64(&mut c.history.clones.similarity_type2, v)),
    ("history.clones.similarity_type3", |c, v| set_f64(&mut c.history.clones.similarity_type3, v)),
    ("history.clones.threshold_ratio_yellow", |c, v| set_f64(&mut c.history.clones.threshold_ratio_yellow, v)),
    ("history.clones.threshold_ratio_red", |c, v| set_f64(&mut c.history.clones.threshold_ratio_red, v)),
    ("history.clones.threshold_pairs_yellow", |c, v| set_i64(&mut c.history.clones.threshold_pairs_yellow, v)),
    ("history.clones.threshold_pairs_red", |c, v| set_i64(&mut c.history.clones.threshold_pairs_red, v)),
    // checkpoint
    ("checkpoint.enabled", |c, v| set_bool(&mut c.checkpoint.enabled, v)),
    ("checkpoint.dir", |c, v| set_string(&mut c.checkpoint.dir, v)),
    ("checkpoint.resume", |c, v| set_bool(&mut c.checkpoint.resume, v)),
    ("checkpoint.clear_prev", |c, v| set_bool(&mut c.checkpoint.clear_prev, v)),
];

/// Loads configuration from a YAML string plus an env-lookup closure.
///
/// Test/host seam exposing the same layering as [`load_config`] (default → file
/// → env) without touching the filesystem or process environment. The `flag`
/// layer (highest precedence) is applied by the caller after this returns,
/// completing the `flag > env > file > default` chain.
///
/// # Errors
/// Same conditions as [`load_config`], minus the filesystem read.
pub fn load_from_yaml_and_env<F>(yaml: &str, lookup: &F) -> Result<Config, LoadError>
where
    F: Fn(&str) -> Option<String>,
{
    let mut cfg = parse_yaml_over_defaults(yaml)?;
    apply_env_overrides(&mut cfg, lookup);
    cfg.validate().map_err(LoadError::Validate)?;
    Ok(cfg)
}

/// Returns the canonical config file path within `dir` (`.codefang.yaml`).
#[must_use]
pub fn config_file_path(dir: &Path) -> PathBuf {
    dir.join(format!("{CONFIG_NAME}.{CONFIG_EXT}"))
}
