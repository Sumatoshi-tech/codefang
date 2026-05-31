//! CLI-parameter → coordinator-config construction — port of
//! `internal/framework/config.go`.
//!
//! [`ConfigParams`] holds the raw CLI values; [`build_config_from_params`]
//! turns them into a [`CoordinatorConfig`] plus a memory budget in bytes,
//! mirroring the Go `BuildConfigFromParams`. All human-readable size strings
//! (e.g. `"256MB"`, `"1GiB"`) go through [`parse_bytes`], a faithful port of
//! `github.com/dustin/go-humanize`'s `ParseBytes` so byte-size parsing matches
//! Go exactly (including the SI-vs-IEC unit table and the float-then-truncate
//! arithmetic — `"42 mib"` → `44_040_192`, `"42 MB"` → `42_000_000`).
//!
//! Durations parse via [`parse_go_duration`], a port of Go's
//! `time.ParseDuration` (units `ns`, `us`/`µs`, `ms`, `s`, `m`, `h`), used by
//! the advanced tuning fields.

use std::time::Duration;

use cf_safeconv::{safe_int, safe_int64};

use crate::coordinator::{
    detect_total_memory_bytes, CoordinatorConfig, DEFAULT_MEMORY_LIMIT_BYTES,
};

/// The fraction (percent) of system memory to use as the default budget.
/// Mirrors Go `defaultMemoryBudgetRatio`.
pub const DEFAULT_MEMORY_BUDGET_RATIO: i64 = 50;

/// Divisor for converting a percentage ratio to a fraction. Mirrors Go
/// `percentDenominator`.
pub const PERCENT_DENOMINATOR: u64 = 100;

/// Maximum auto-detected memory budget (2 GiB). Mirrors Go
/// `defaultMemoryBudgetCap`. Forces smaller chunks on large repos, keeping peak
/// RSS bounded; native C memory (libgit2 mwindow, object cache, glibc arenas)
/// adds ~1.5 GiB on top, so a 2 GiB budget targets ~3.5 GiB total RSS.
pub const DEFAULT_MEMORY_BUDGET_CAP: i64 = 2 * 1024 * 1024 * 1024;

/// Errors from configuration construction. Mirrors Go's sentinel errors plus
/// the wrapped parse failures (`fmt.Errorf("%w for <field>: <value>")`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// A size string was not a valid humanize byte size. The payload is the
    /// `"<field>: <value>"` context Go appends after `ErrInvalidSizeFormat`.
    InvalidSizeFormat(String),
    /// A negative GC percent was supplied. Mirrors `ErrInvalidGCPercent`.
    InvalidGcPercent(i64),
    /// A duration string was not parseable by Go's `time.ParseDuration`. The
    /// payload is the `"<field>: <value>"` context.
    InvalidDuration(String),
    /// The memory-budget solver returned an error. Payload is the message.
    MemoryBudget(String),
    /// Generic parse failure (e.g. "failed to parse budget").
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidSizeFormat(ctx) => write!(f, "invalid size format for {ctx}"),
            ConfigError::InvalidGcPercent(v) => write!(f, "invalid GC percent: {v}"),
            ConfigError::InvalidDuration(ctx) => write!(f, "invalid size format for {ctx}"),
            ConfigError::MemoryBudget(m) => write!(f, "memory budget error: {m}"),
            ConfigError::Parse(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Resolves a memory budget (bytes) to a [`CoordinatorConfig`]. Mirrors Go
/// `framework.BudgetSolver` (`func(budgetBytes int64) (CoordinatorConfig, error)`).
pub type BudgetSolver<'a> = dyn Fn(i64) -> Result<CoordinatorConfig, String> + 'a;

/// Raw CLI parameter values for building a [`CoordinatorConfig`]. Mirrors Go
/// `framework.ConfigParams` field-for-field. All size strings use humanize
/// format; durations use Go duration format. A zero numeric field means "use
/// the default" (the Go builder only applies positive values).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigParams {
    /// Worker count.
    pub workers: i64,
    /// Internal channel buffer size.
    pub buffer_size: i64,
    /// Commits per batch.
    pub commit_batch_size: i64,
    /// Blob cache size (humanize string).
    pub blob_cache_size: String,
    /// Diff cache entry count.
    pub diff_cache_size: i64,
    /// Blob arena size (humanize string).
    pub blob_arena_size: String,
    /// Memory budget (humanize string; empty = auto-detect).
    pub memory_budget: String,
    /// Go GC percent.
    pub gc_percent: i64,
    /// Ballast size (humanize string).
    pub ballast_size: String,

    // Advanced pipeline tuning (zero = use defaults).
    /// UAST spill threshold (file changes).
    pub uast_spill_threshold: i64,
    /// Intra-commit parallelism threshold.
    pub intra_commit_parallel_threshold: i64,
    /// Max intra-commit workers.
    pub max_intra_commit_workers: i64,
    /// Max UAST blob size (bytes).
    pub max_uast_blob_size: i64,
    /// UAST parse timeout (Go duration string).
    pub uast_parse_timeout: String,
    /// Max file changes per commit.
    pub max_changes_per_commit: i64,
    /// Max diff requests per batch.
    pub max_diff_batch_size: i64,
    /// Memory budget ratio (percent).
    pub memory_budget_ratio: i64,
    /// Memory budget cap (humanize string).
    pub memory_budget_cap: String,
    /// Memory limit ratio (percent).
    pub memory_limit_ratio: i64,

    // Extended pipeline tuning.
    /// UAST spill trim interval.
    pub uast_spill_trim_interval: i64,
    /// Native trim interval.
    pub native_trim_interval: i64,
    /// Max streaming buffering factor.
    pub max_streaming_buffering: i64,
    /// Drain prefetch timeout (Go duration string).
    pub drain_prefetch_timeout: String,
    /// Sampler interval (Go duration string).
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
    /// Diff job buffer multiplier.
    pub diff_job_buffer_multiplier: i64,
}

/// Checkpoint-related configuration. Mirrors Go `framework.CheckpointParams`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckpointParams {
    /// Whether checkpointing is enabled.
    pub enabled: bool,
    /// Checkpoint directory.
    pub dir: String,
    /// Whether to resume from a prior checkpoint.
    pub resume: bool,
    /// Whether to clear a previous checkpoint first.
    pub clear_prev: bool,
}

/// Returns a sensible memory budget from available system memory.
/// `min(50% of total RAM, 2 GiB)`, or 0 if detection fails. Mirrors Go
/// `DefaultMemoryBudget`.
#[must_use]
pub fn default_memory_budget() -> i64 {
    default_memory_budget_with_params(DEFAULT_MEMORY_BUDGET_RATIO, "")
}

/// Returns a memory budget with a configurable ratio and cap. An empty cap
/// string uses [`DEFAULT_MEMORY_BUDGET_CAP`]. Mirrors Go
/// `DefaultMemoryBudgetWithParams`.
#[must_use]
pub fn default_memory_budget_with_params(ratio: i64, cap_str: &str) -> i64 {
    let total = detect_total_memory_bytes();
    if total == 0 {
        return 0;
    }

    let mut budget_cap = DEFAULT_MEMORY_BUDGET_CAP;
    if !cap_str.is_empty() {
        if let Ok(parsed) = parse_bytes(cap_str) {
            if parsed > 0 {
                budget_cap = safe_int64(parsed);
            }
        }
    }

    let budget = safe_int64(total * (ratio as u64) / PERCENT_DENOMINATOR);
    budget.min(budget_cap)
}

/// Builds a [`CoordinatorConfig`] from raw parameters, returning the config and
/// the memory budget in bytes (0 if not set). The `budget_solver` is called
/// when `params.memory_budget` is set; pass `None` if memory-budget is not
/// supported. Mirrors Go `BuildConfigFromParams`.
pub fn build_config_from_params(
    params: &ConfigParams,
    budget_solver: Option<&BudgetSolver<'_>>,
) -> Result<(CoordinatorConfig, i64), ConfigError> {
    if !params.memory_budget.is_empty() {
        let mut cfg = build_config_from_budget(&params.memory_budget, budget_solver)?;
        apply_runtime_tuning_params(&mut cfg, params.gc_percent, &params.ballast_size)?;
        let budget_bytes = parse_bytes(&params.memory_budget)
            .map_err(|e| ConfigError::Parse(format!("failed to parse budget: {e}")))?;
        return Ok((cfg, safe_int64(budget_bytes)));
    }

    let mut config = CoordinatorConfig::default();
    apply_int_params(&mut config, params);
    apply_size_params(&mut config, params)?;
    apply_advanced_params(&mut config, params)?;
    apply_runtime_tuning_params(&mut config, params.gc_percent, &params.ballast_size)?;

    let budget_ratio = if params.memory_budget_ratio == 0 {
        DEFAULT_MEMORY_BUDGET_RATIO
    } else {
        params.memory_budget_ratio
    };

    let mem_budget = default_memory_budget_with_params(budget_ratio, &params.memory_budget_cap);

    Ok((config, mem_budget))
}

fn build_config_from_budget(
    budget_str: &str,
    solver: Option<&BudgetSolver<'_>>,
) -> Result<CoordinatorConfig, ConfigError> {
    let budget_bytes = parse_bytes(budget_str)
        .map_err(|_| ConfigError::InvalidSizeFormat(format!("memory-budget: {budget_str}")))?;

    match solver {
        // Go passes a nil solver and would panic; in Rust we surface a clear
        // error rather than panicking when the budget path is used without a
        // solver wired in.
        None => Err(ConfigError::MemoryBudget(
            "no budget solver configured".to_string(),
        )),
        Some(solve) => solve(safe_int64(budget_bytes)).map_err(ConfigError::MemoryBudget),
    }
}

fn apply_int_params(config: &mut CoordinatorConfig, params: &ConfigParams) {
    if params.workers > 0 {
        config.workers = params.workers;
    }
    if params.buffer_size > 0 {
        config.buffer_size = params.buffer_size;
    }
    if params.commit_batch_size > 0 {
        config.commit_batch_size = params.commit_batch_size;
    }
    if params.diff_cache_size > 0 {
        config.diff_cache_size = params.diff_cache_size;
    }
    if params.uast_spill_threshold > 0 {
        config.uast_spill_threshold = params.uast_spill_threshold;
    }
    if params.intra_commit_parallel_threshold > 0 {
        config.intra_commit_parallel_threshold = params.intra_commit_parallel_threshold;
    }
    if params.max_intra_commit_workers > 0 {
        config.max_intra_commit_workers = params.max_intra_commit_workers;
    }
    if params.max_uast_blob_size > 0 {
        config.max_uast_blob_size = params.max_uast_blob_size;
    }
    if params.max_changes_per_commit > 0 {
        config.max_changes_per_commit = params.max_changes_per_commit;
    }
    if params.max_diff_batch_size > 0 {
        config.max_diff_batch_size = params.max_diff_batch_size;
    }
    if params.memory_limit_ratio > 0 {
        config.memory_limit_ratio = params.memory_limit_ratio;
    }
    apply_extended_int_params(config, params);
}

fn apply_extended_int_params(config: &mut CoordinatorConfig, params: &ConfigParams) {
    if params.uast_spill_trim_interval > 0 {
        config.uast_spill_trim_interval = params.uast_spill_trim_interval;
    }
    if params.native_trim_interval > 0 {
        config.native_trim_interval = params.native_trim_interval;
    }
    if params.max_streaming_buffering > 0 {
        config.max_streaming_buffering = params.max_streaming_buffering;
    }
    if params.worker_ratio > 0 {
        config.worker_ratio = params.worker_ratio;
    }
    if params.uast_worker_ratio > 0 {
        config.uast_worker_ratio = params.uast_worker_ratio;
    }
    if params.leaf_worker_divisor > 0 {
        config.leaf_worker_divisor = params.leaf_worker_divisor;
    }
    if params.min_leaf_workers > 0 {
        config.min_leaf_workers = params.min_leaf_workers;
    }
    if params.buffer_size_multiplier > 0 {
        config.buffer_size_multiplier = params.buffer_size_multiplier;
    }
    if params.budget_limit_ratio > 0 {
        config.budget_limit_ratio = params.budget_limit_ratio;
    }
    if params.system_ram_limit_ratio > 0 {
        config.system_ram_limit_ratio = params.system_ram_limit_ratio;
    }
    if params.diff_job_buffer_multiplier > 0 {
        config.diff_job_buffer_multiplier = params.diff_job_buffer_multiplier;
    }
}

fn apply_advanced_params(
    config: &mut CoordinatorConfig,
    params: &ConfigParams,
) -> Result<(), ConfigError> {
    if !params.uast_parse_timeout.is_empty() {
        config.uast_parse_timeout = parse_go_duration(&params.uast_parse_timeout).map_err(|_| {
            ConfigError::InvalidDuration(format!("uast-parse-timeout: {}", params.uast_parse_timeout))
        })?;
    }
    if !params.drain_prefetch_timeout.is_empty() {
        config.drain_prefetch_timeout =
            parse_go_duration(&params.drain_prefetch_timeout).map_err(|_| {
                ConfigError::InvalidDuration(format!(
                    "drain-prefetch-timeout: {}",
                    params.drain_prefetch_timeout
                ))
            })?;
    }
    if !params.sampler_interval.is_empty() {
        config.sampler_interval = parse_go_duration(&params.sampler_interval).map_err(|_| {
            ConfigError::InvalidDuration(format!("sampler-interval: {}", params.sampler_interval))
        })?;
    }
    Ok(())
}

fn apply_size_params(
    config: &mut CoordinatorConfig,
    params: &ConfigParams,
) -> Result<(), ConfigError> {
    if !params.blob_cache_size.is_empty() {
        let size = parse_bytes(&params.blob_cache_size).map_err(|_| {
            ConfigError::InvalidSizeFormat(format!("blob-cache-size: {}", params.blob_cache_size))
        })?;
        config.blob_cache_size = safe_int64(size);
    }
    if !params.blob_arena_size.is_empty() {
        let size = parse_bytes(&params.blob_arena_size).map_err(|_| {
            ConfigError::InvalidSizeFormat(format!("blob-arena-size: {}", params.blob_arena_size))
        })?;
        config.blob_arena_size = safe_int(size) as i64;
    }
    Ok(())
}

fn apply_runtime_tuning_params(
    config: &mut CoordinatorConfig,
    gc_percent: i64,
    ballast_size: &str,
) -> Result<(), ConfigError> {
    if gc_percent < 0 {
        return Err(ConfigError::InvalidGcPercent(gc_percent));
    }
    config.gc_percent = gc_percent;
    config.ballast_size = parse_optional_size(ballast_size)?;
    Ok(())
}

/// Parses a human-readable size string, returning 0 for empty or `"0"`.
/// Mirrors Go `ParseOptionalSize` (note: it trims whitespace and treats both
/// `""` and `"0"` as zero before delegating to humanize).
pub fn parse_optional_size(size_value: &str) -> Result<i64, ConfigError> {
    let trimmed = size_value.trim();
    if trimmed.is_empty() || trimmed == "0" {
        return Ok(0);
    }
    let parsed = parse_bytes(trimmed)
        .map_err(|_| ConfigError::InvalidSizeFormat(format!("ballast-size: {size_value}")))?;
    Ok(safe_int64(parsed))
}

// ---------------------------------------------------------------------------
// go-humanize ParseBytes port.
// ---------------------------------------------------------------------------

/// Parses a string representation of bytes into the number of bytes it
/// represents. Faithful port of `github.com/dustin/go-humanize`'s `ParseBytes`.
///
/// Examples (byte-identical to Go): `parse_bytes("42 MB") == Ok(42_000_000)`,
/// `parse_bytes("42 mib") == Ok(44_040_192)`. The leading numeric run (digits,
/// `.`, `,`) is parsed as an `f64` (commas stripped), then multiplied by the
/// unit multiplier from the SI/IEC lookup table; the float product is truncated
/// to `u64` exactly as Go does.
///
/// # Errors
///
/// Returns `Err` when the numeric prefix is not a valid float, when the product
/// would reach `u64::MAX`, or when the unit suffix is not recognized.
pub fn parse_bytes(s: &str) -> Result<u64, String> {
    // Mirror Go's loop: advance `last_digit` over the leading run of digits,
    // '.', and ',', stopping at the first other byte. `last_digit` is the byte
    // index where the numeric prefix ends; `s[..last_digit]` is the number and
    // `s[last_digit..]` is the unit suffix.
    let mut last_digit = s.len();
    let mut has_comma = false;
    for (i, r) in s.char_indices() {
        if !(r.is_ascii_digit() || r == '.' || r == ',') {
            last_digit = i;
            break;
        }
        if r == ',' {
            has_comma = true;
        }
    }

    let num: String = if has_comma {
        s[..last_digit].replace(',', "")
    } else {
        s[..last_digit].to_string()
    };

    finish_parse(&num, &s[last_digit..])
}

fn finish_parse(num: &str, rest: &str) -> Result<u64, String> {
    let f: f64 = num
        .parse()
        .map_err(|_| format!("strconv.ParseFloat: parsing {num:?}: invalid syntax"))?;

    let extra = rest.trim().to_lowercase();
    match size_table(&extra) {
        Some(m) => {
            let product = f * (m as f64);
            if product >= u64::MAX as f64 {
                return Err(format!("too large: {num}{rest}"));
            }
            Ok(product as u64)
        }
        None => Err(format!("unhandled size name: {extra}")),
    }
}

/// The SI/IEC unit multiplier table used by [`parse_bytes`]. Mirrors humanize's
/// combined `bytesSizeTable` (lowercased keys; IEC = 1024-based, SI = 1000-based;
/// suffix-less short forms included).
fn size_table(unit: &str) -> Option<u64> {
    const KI: u64 = 1 << 10;
    const MI: u64 = 1 << 20;
    const GI: u64 = 1 << 30;
    const TI: u64 = 1 << 40;
    const PI: u64 = 1 << 50;
    const EI: u64 = 1 << 60;
    const K: u64 = 1000;
    const M: u64 = K * 1000;
    const G: u64 = M * 1000;
    const T: u64 = G * 1000;
    const P: u64 = T * 1000;
    const E: u64 = P * 1000;
    Some(match unit {
        "b" | "" => 1,
        "kib" | "ki" => KI,
        "kb" | "k" => K,
        "mib" | "mi" => MI,
        "mb" | "m" => M,
        "gib" | "gi" => GI,
        "gb" | "g" => G,
        "tib" | "ti" => TI,
        "tb" | "t" => T,
        "pib" | "pi" => PI,
        "pb" | "p" => P,
        "eib" | "ei" => EI,
        "eb" | "e" => E,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Go time.ParseDuration port (subset used by config).
// ---------------------------------------------------------------------------

/// Parses a Go duration string (e.g. `"10s"`, `"1h30m"`, `"500ms"`). Port of
/// Go's `time.ParseDuration` for the unit set the framework uses: `ns`, `us`,
/// `µs`, `ms`, `s`, `m`, `h`. A leading sign is allowed; the special string
/// `"0"` is zero.
///
/// # Errors
///
/// Returns `Err` for empty input, unknown units, or malformed numbers — the
/// same cases Go rejects.
pub fn parse_go_duration(s: &str) -> Result<Duration, String> {
    let orig = s;
    if s == "0" || s == "+0" || s == "-0" {
        return Ok(Duration::ZERO);
    }
    if s.is_empty() {
        return Err(format!("time: invalid duration {orig:?}"));
    }

    let mut bytes = s.as_bytes();
    let mut neg = false;
    if bytes.first() == Some(&b'-') || bytes.first() == Some(&b'+') {
        neg = bytes[0] == b'-';
        bytes = &bytes[1..];
    }
    if bytes.is_empty() {
        return Err(format!("time: invalid duration {orig:?}"));
    }

    let mut total_nanos: i128 = 0;
    let mut rest = std::str::from_utf8(bytes).map_err(|_| format!("time: invalid duration {orig:?}"))?;

    while !rest.is_empty() {
        // Parse a (possibly fractional) number.
        let num_end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .ok_or_else(|| format!("time: missing unit in duration {orig:?}"))?;
        if num_end == 0 {
            return Err(format!("time: invalid duration {orig:?}"));
        }
        let value: f64 = rest[..num_end]
            .parse()
            .map_err(|_| format!("time: invalid duration {orig:?}"))?;
        rest = &rest[num_end..];

        // Parse the unit.
        let unit_len = unit_prefix_len(rest);
        if unit_len == 0 {
            return Err(format!("time: unknown unit in duration {orig:?}"));
        }
        let unit = &rest[..unit_len];
        rest = &rest[unit_len..];

        let mult_ns = unit_nanos(unit)
            .ok_or_else(|| format!("time: unknown unit {unit:?} in duration {orig:?}"))?;
        total_nanos += (value * mult_ns) as i128;
    }

    if neg {
        total_nanos = -total_nanos;
    }
    if total_nanos < 0 {
        // Durations in the config are non-negative; Go would carry the sign,
        // but every consumer here uses positive timeouts. Clamp to zero.
        return Ok(Duration::ZERO);
    }
    Ok(Duration::from_nanos(total_nanos as u64))
}

/// Length of the longest known unit prefix at the start of `s`.
fn unit_prefix_len(s: &str) -> usize {
    // Longest first: "ns","us","µs","ms" (2), then "s","m","h" (1). "µ" is 2
    // bytes in UTF-8, so "µs" is 3 bytes.
    if s.starts_with("ns") || s.starts_with("us") || s.starts_with("ms") {
        2
    } else if s.starts_with("µs") {
        "µs".len()
    } else if s.starts_with('s') || s.starts_with('m') || s.starts_with('h') {
        1
    } else {
        0
    }
}

fn unit_nanos(unit: &str) -> Option<f64> {
    Some(match unit {
        "ns" => 1.0,
        "us" | "µs" => 1_000.0,
        "ms" => 1_000_000.0,
        "s" => 1_000_000_000.0,
        "m" => 60.0 * 1_000_000_000.0,
        "h" => 3600.0 * 1_000_000_000.0,
        _ => return None,
    })
}

/// Saturating cap helper kept for parity with the Go default-budget cap path.
/// (Exposed so callers can clamp a budget to [`DEFAULT_MEMORY_LIMIT_BYTES`].)
#[must_use]
pub fn cap_to_default_memory_limit(value: u64) -> u64 {
    value.min(DEFAULT_MEMORY_LIMIT_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_bytes (humanize parity) ---

    #[test]
    fn parse_bytes_si_mb() {
        assert_eq!(parse_bytes("42 MB").unwrap(), 42_000_000);
    }

    #[test]
    fn parse_bytes_iec_mib() {
        assert_eq!(parse_bytes("42 mib").unwrap(), 44_040_192);
    }

    #[test]
    fn parse_bytes_no_unit_is_bytes() {
        assert_eq!(parse_bytes("1024").unwrap(), 1024);
    }

    #[test]
    fn parse_bytes_short_forms() {
        assert_eq!(parse_bytes("1k").unwrap(), 1000);
        assert_eq!(parse_bytes("1ki").unwrap(), 1024);
        assert_eq!(parse_bytes("1g").unwrap(), 1_000_000_000);
        assert_eq!(parse_bytes("1gi").unwrap(), 1 << 30);
    }

    #[test]
    fn parse_bytes_uppercase_unit() {
        assert_eq!(parse_bytes("256MB").unwrap(), 256_000_000);
        assert_eq!(parse_bytes("1GiB").unwrap(), 1 << 30);
    }

    #[test]
    fn parse_bytes_fractional() {
        // 1.5 KiB = 1536 (float 1.5 * 1024 truncated).
        assert_eq!(parse_bytes("1.5KiB").unwrap(), 1536);
    }

    #[test]
    fn parse_bytes_comma_grouping() {
        assert_eq!(parse_bytes("1,024").unwrap(), 1024);
    }

    #[test]
    fn parse_bytes_unknown_unit_errors() {
        assert!(parse_bytes("5 zb").is_err());
    }

    #[test]
    fn parse_bytes_bad_number_errors() {
        assert!(parse_bytes("abc").is_err());
    }

    // --- parse_optional_size ---

    #[test]
    fn parse_optional_size_empty_and_zero() {
        assert_eq!(parse_optional_size("").unwrap(), 0);
        assert_eq!(parse_optional_size("0").unwrap(), 0);
        assert_eq!(parse_optional_size("  ").unwrap(), 0);
    }

    #[test]
    fn parse_optional_size_value() {
        assert_eq!(parse_optional_size("1MiB").unwrap(), 1 << 20);
    }

    #[test]
    fn parse_optional_size_bad_errors() {
        assert!(matches!(
            parse_optional_size("nope"),
            Err(ConfigError::InvalidSizeFormat(_))
        ));
    }

    // --- parse_go_duration ---

    #[test]
    fn duration_seconds() {
        assert_eq!(parse_go_duration("10s").unwrap(), Duration::from_secs(10));
    }

    #[test]
    fn duration_compound() {
        assert_eq!(
            parse_go_duration("1h30m").unwrap(),
            Duration::from_secs(3600 + 30 * 60)
        );
    }

    #[test]
    fn duration_millis() {
        assert_eq!(
            parse_go_duration("500ms").unwrap(),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn duration_zero() {
        assert_eq!(parse_go_duration("0").unwrap(), Duration::ZERO);
    }

    #[test]
    fn duration_micros_ascii_and_mu() {
        assert_eq!(parse_go_duration("250us").unwrap(), Duration::from_micros(250));
        assert_eq!(parse_go_duration("250µs").unwrap(), Duration::from_micros(250));
    }

    #[test]
    fn duration_unknown_unit_errors() {
        assert!(parse_go_duration("5x").is_err());
    }

    #[test]
    fn duration_empty_errors() {
        assert!(parse_go_duration("").is_err());
    }

    // --- apply_int_params / build_config_from_params ---

    #[test]
    fn build_config_applies_positive_ints_only() {
        let params = ConfigParams {
            workers: 8,
            commit_batch_size: 50,
            // diff_cache_size left 0 -> keeps default.
            ..ConfigParams::default()
        };
        let (cfg, _budget) = build_config_from_params(&params, None).unwrap();
        assert_eq!(cfg.workers, 8);
        assert_eq!(cfg.commit_batch_size, 50);
        // default preserved where param was zero.
        assert_eq!(cfg.diff_cache_size, CoordinatorConfig::default().diff_cache_size);
    }

    #[test]
    fn build_config_size_params() {
        let params = ConfigParams {
            blob_cache_size: "10MiB".to_string(),
            blob_arena_size: "1MiB".to_string(),
            ..ConfigParams::default()
        };
        let (cfg, _) = build_config_from_params(&params, None).unwrap();
        assert_eq!(cfg.blob_cache_size, 10 << 20);
        assert_eq!(cfg.blob_arena_size, 1 << 20);
    }

    #[test]
    fn build_config_negative_gc_errors() {
        let params = ConfigParams {
            gc_percent: -1,
            ..ConfigParams::default()
        };
        assert_eq!(
            build_config_from_params(&params, None),
            Err(ConfigError::InvalidGcPercent(-1))
        );
    }

    #[test]
    fn build_config_advanced_duration() {
        let params = ConfigParams {
            uast_parse_timeout: "5s".to_string(),
            sampler_interval: "1s".to_string(),
            ..ConfigParams::default()
        };
        let (cfg, _) = build_config_from_params(&params, None).unwrap();
        assert_eq!(cfg.uast_parse_timeout, Duration::from_secs(5));
        assert_eq!(cfg.sampler_interval, Duration::from_secs(1));
    }

    #[test]
    fn build_config_bad_size_errors() {
        let params = ConfigParams {
            blob_cache_size: "notasize".to_string(),
            ..ConfigParams::default()
        };
        assert!(matches!(
            build_config_from_params(&params, None),
            Err(ConfigError::InvalidSizeFormat(_))
        ));
    }

    #[test]
    fn build_config_budget_path_uses_solver() {
        let params = ConfigParams {
            memory_budget: "1GiB".to_string(),
            ..ConfigParams::default()
        };
        let solver = |budget: i64| -> Result<CoordinatorConfig, String> {
            // Trivial: scale workers by budget GiB.
            Ok(CoordinatorConfig {
                workers: (budget / (1 << 30)).max(1),
                ..CoordinatorConfig::default()
            })
        };
        let (cfg, budget) = build_config_from_params(&params, Some(&solver)).unwrap();
        assert_eq!(budget, 1 << 30);
        assert_eq!(cfg.workers, 1);
    }

    #[test]
    fn build_config_budget_path_no_solver_errors() {
        let params = ConfigParams {
            memory_budget: "1GiB".to_string(),
            ..ConfigParams::default()
        };
        assert!(matches!(
            build_config_from_params(&params, None),
            Err(ConfigError::MemoryBudget(_))
        ));
    }

    #[test]
    fn build_config_budget_bad_string_errors() {
        let params = ConfigParams {
            memory_budget: "xxx".to_string(),
            ..ConfigParams::default()
        };
        let solver = |_b: i64| Ok(CoordinatorConfig::default());
        assert!(matches!(
            build_config_from_params(&params, Some(&solver)),
            Err(ConfigError::InvalidSizeFormat(_))
        ));
    }
}
