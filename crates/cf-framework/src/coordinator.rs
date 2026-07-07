//! Pipeline coordinator configuration and memory model.
//!
//! Provides:
//!
//! - [`CoordinatorConfig`] (every tuning field) and
//!   [`CoordinatorConfig::default`], including the CPU-ratio worker math.
//! - [`PipelineStats`] and [`PipelineStats::add`] (cross-chunk aggregation).
//! - [`CoordinatorConfig::estimated_overhead`] — the memory model used by the
//!   streaming planner (must stay in sync with `cf-budget`'s model).
//! - The system-memory detection (`/proc/meminfo`) and soft-memory-limit math
//!   ([`detect_total_memory_bytes`], [`resolve_memory_limit_from_budget`],
//!   [`resolve_memory_limit_with_ratio`]).
//!
//! Time-valued tuning fields use [`std::time::Duration`]; a value of
//! `Duration::ZERO` means "use the package default" (the config builder only
//! applies positive values).

use std::time::Duration;

use cf_safeconv::safe_int64;

// ---------------------------------------------------------------------------
// Worker-sizing ratios.
// ---------------------------------------------------------------------------

/// Factor by which buffer size scales with worker count.
pub const BUFFER_SIZE_MULTIPLIER: i64 = 2;

/// Fraction (percent) of CPU cores to use for workers. With parallel leaf
/// consumption the pipeline is the bottleneck, so all cores are used.
pub const OPTIMAL_WORKER_RATIO: i64 = 100;

/// Fraction (percent) of CPU cores to use for UAST pipeline workers.
pub const UAST_PIPELINE_WORKER_RATIO: i64 = 40;

/// Default leaf workers = `NumCPU / divisor`.
pub const LEAF_WORKER_DIVISOR: i64 = 3;

/// Minimum number of leaf workers when enabled.
pub const MIN_LEAF_WORKERS: i64 = 4;

/// Default number of commits to process per batch.
pub const DEFAULT_COMMIT_BATCH_SIZE: i64 = 100;

/// Default arena size for blob loading (4 MiB).
pub const DEFAULT_BLOB_ARENA_BYTES: i64 = 4 * 1024 * 1024;

/// Default soft memory limit (8 GiB).
pub const DEFAULT_MEMORY_LIMIT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Fraction (percent) of system memory to use as the soft limit.
pub const MEMORY_LIMIT_RATIO: i64 = 75;

/// Divisor converting percentage ratios (e.g. 60, 75) to fractions.
pub const PERCENT_DIVISOR: u64 = 100;

/// Fraction (percent) of the user's memory budget to use as the soft limit.
pub const BUDGET_LIMIT_RATIO: i64 = 95;

/// Caps the soft memory limit at this fraction (percent) of system RAM.
pub const SYSTEM_RAM_LIMIT_RATIO: i64 = 90;

const PROC_MEMINFO_PATH: &str = "/proc/meminfo";
const MEM_TOTAL_PREFIX: &str = "MemTotal:";
const MEM_TOTAL_UNIT_KIB: &str = "kB";
const KIBIBYTE: u64 = 1024;
const MIN_MEMINFO_FIELDS: usize = 2;

// ---------------------------------------------------------------------------
// Pipeline memory model constants (duplicated from cf-budget's model to avoid
// a circular dependency; keep the two in sync).
// ---------------------------------------------------------------------------

/// Runtime + libgit2 base + shared mmap.
pub const RUNTIME_OVERHEAD: i64 = 250 * 1024 * 1024;
/// Per-worker libgit2 repository handle.
pub const REPO_HANDLE_SIZE: i64 = 10 * 1024 * 1024;
/// Per-worker C/mmap overhead from libgit2.
pub const WORKER_NATIVE_OVERHEAD: i64 = 50 * 1024 * 1024;
/// Average cached diff entry.
pub const AVG_DIFF_ENTRY_SIZE: i64 = 2 * 1024;
/// Average in-flight commit data.
pub const AVG_COMMIT_DATA_SIZE: i64 = 64 * 1024;

// Pipeline-tuning stage defaults referenced by `default()`. These values are
// reference-implementation behavior (they steer chunk planning and worker
// sizing, which the differential gate pins indirectly); change them only with
// a corresponding gate run.
const UAST_SPILL_THRESHOLD: i64 = 32;
const INTRA_COMMIT_PARALLEL_THRESHOLD: i64 = 4;
const DEFAULT_MAX_INTRA_COMMIT_WORKERS: i64 = 4;
const MAX_UAST_BLOB_SIZE: i64 = 256 * 1024;
const MAX_CHANGES_PER_COMMIT: i64 = 10000;
const DEFAULT_MAX_DIFF_BATCH_SIZE: i64 = 1000;
const UAST_SPILL_TRIM_INTERVAL: i64 = 16;
const NATIVE_TRIM_INTERVAL: i64 = 10;
const MAX_STREAMING_BUFFERING: i64 = 3;
const DIFF_JOB_BUFFER_MULTIPLIER: i64 = 10;
const DEFAULT_PARSE_TIMEOUT: Duration = Duration::from_secs(10);
const DRAIN_PREFETCH_TIMEOUT: Duration = Duration::from_secs(30);
const SAMPLER_INTERVAL: Duration = Duration::from_secs(2);

/// Cumulative pipeline metrics for a single coordinator run. Valid after the
/// output channel is fully drained.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PipelineStats {
    /// Total time spent in the blob stage.
    pub blob_duration: Duration,
    /// Total time spent in the diff stage.
    pub diff_duration: Duration,
    /// Total time spent in the UAST stage.
    pub uast_duration: Duration,
    /// Blob cache hits during the run.
    pub blob_cache_hits: i64,
    /// Blob cache misses during the run.
    pub blob_cache_misses: i64,
    /// Diff cache hits during the run.
    pub diff_cache_hits: i64,
    /// Diff cache misses during the run.
    pub diff_cache_misses: i64,
}

impl PipelineStats {
    /// Accumulates another `PipelineStats` into this one (cross-chunk
    /// aggregation).
    pub fn add(&mut self, other: &Self) {
        self.blob_duration += other.blob_duration;
        self.diff_duration += other.diff_duration;
        self.uast_duration += other.uast_duration;
        self.blob_cache_hits += other.blob_cache_hits;
        self.blob_cache_misses += other.blob_cache_misses;
        self.diff_cache_hits += other.diff_cache_hits;
        self.diff_cache_misses += other.diff_cache_misses;
    }
}

/// Configures the pipeline coordinator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorConfig {
    /// Number of commits to process in each batch.
    pub commit_batch_size: i64,
    /// Number of parallel workers for processing.
    pub workers: i64,
    /// Size of internal channels.
    pub buffer_size: i64,
    /// Maximum size of the global blob cache in bytes (0 disables caching).
    pub blob_cache_size: i64,
    /// Maximum number of diff results to cache (0 disables caching).
    pub diff_cache_size: i64,
    /// Size of the memory arena for blob loading.
    pub blob_arena_size: i64,
    /// Number of workers for parallel UAST parsing (0 disables the stage).
    pub uast_pipeline_workers: i64,
    /// Number of workers for parallel leaf analyzer consumption.
    pub leaf_workers: i64,
    /// GC aggressiveness (0 = auto; retained for CLI-flag parity).
    pub gc_percent: i64,
    /// Bytes reserved in a long-lived slice to smooth GC (0 disables ballast).
    pub ballast_size: i64,
    /// Whether the history walk is restricted to the first parent.
    pub first_parent: bool,
    /// Max time to wait for a worker response before considering it stalled
    /// (0 disables the watchdog).
    pub worker_timeout: Duration,

    // Advanced pipeline tuning.
    /// File-change count above which the UAST pipeline spills to disk.
    pub uast_spill_threshold: i64,
    /// Minimum file changes for intra-commit parallelism.
    pub intra_commit_parallel_threshold: i64,
    /// Caps workers for parsing files within a single commit.
    pub max_intra_commit_workers: i64,
    /// Maximum blob size (bytes) for UAST parsing.
    pub max_uast_blob_size: i64,
    /// Per-file UAST parse timeout.
    pub uast_parse_timeout: Duration,
    /// Caps file changes per commit for blob loading.
    pub max_changes_per_commit: i64,
    /// libgit2 pathspec pre-filter for tree diffs (empty = no filtering).
    pub tree_diff_pathspec: Vec<String>,
    /// Maximum number of diff requests per batch.
    pub max_diff_batch_size: i64,
    /// Fraction of system memory to use as the soft limit.
    pub memory_limit_ratio: i64,
    /// `MallocTrim` frequency during UAST spill-mode parsing.
    pub uast_spill_trim_interval: i64,
    /// `malloc_trim` frequency within a chunk.
    pub native_trim_interval: i64,
    /// Maximum buffering factor for streaming (triple-buffering).
    pub max_streaming_buffering: i64,
    /// Timeout for abandoning prefetch workers.
    pub drain_prefetch_timeout: Duration,
    /// Polling interval for the pipeline sampler.
    pub sampler_interval: Duration,
    /// Fraction (percent) of CPU cores to use for workers.
    pub worker_ratio: i64,
    /// Fraction (percent) of CPU cores to use for UAST pipeline workers.
    pub uast_worker_ratio: i64,
    /// Default leaf workers: `NumCPU / divisor`.
    pub leaf_worker_divisor: i64,
    /// Minimum number of leaf workers when enabled.
    pub min_leaf_workers: i64,
    /// Scales buffer size with worker count.
    pub buffer_size_multiplier: i64,
    /// Budget-to-memory-limit conversion ratio (percent).
    pub budget_limit_ratio: i64,
    /// Caps the memory limit at this fraction of system RAM (percent).
    pub system_ram_limit_ratio: i64,
    /// Scales the diff job queue buffer.
    pub diff_job_buffer_multiplier: i64,
}

impl Default for CoordinatorConfig {
    /// Worker counts derive from the CPU count using integer-ratio math:
    /// `max(num_cpu * ratio / 100, 1)` for workers, and
    /// `max(num_cpu / divisor, min)` for leaf workers.
    fn default() -> Self {
        let num_cpu = num_cpus() as i64;
        let workers = (num_cpu * OPTIMAL_WORKER_RATIO / (PERCENT_DIVISOR as i64)).max(1);
        let uast_workers = (num_cpu * UAST_PIPELINE_WORKER_RATIO / (PERCENT_DIVISOR as i64)).max(1);
        let leaf_workers = (num_cpu / LEAF_WORKER_DIVISOR).max(MIN_LEAF_WORKERS);

        Self {
            commit_batch_size: DEFAULT_COMMIT_BATCH_SIZE,
            workers,
            buffer_size: workers * BUFFER_SIZE_MULTIPLIER,
            blob_cache_size: DEFAULT_LRU_CACHE_SIZE,
            diff_cache_size: crate::diff_cache::DEFAULT_DIFF_CACHE_SIZE as i64,
            uast_pipeline_workers: uast_workers,
            leaf_workers,
            blob_arena_size: DEFAULT_BLOB_ARENA_BYTES,
            gc_percent: 0,
            ballast_size: 0,
            first_parent: false,
            worker_timeout: Duration::ZERO,

            uast_spill_threshold: UAST_SPILL_THRESHOLD,
            intra_commit_parallel_threshold: INTRA_COMMIT_PARALLEL_THRESHOLD,
            max_intra_commit_workers: DEFAULT_MAX_INTRA_COMMIT_WORKERS,
            max_uast_blob_size: MAX_UAST_BLOB_SIZE,
            uast_parse_timeout: DEFAULT_PARSE_TIMEOUT,
            max_changes_per_commit: MAX_CHANGES_PER_COMMIT,
            tree_diff_pathspec: Vec::new(),
            max_diff_batch_size: DEFAULT_MAX_DIFF_BATCH_SIZE,
            memory_limit_ratio: MEMORY_LIMIT_RATIO,
            uast_spill_trim_interval: UAST_SPILL_TRIM_INTERVAL,
            native_trim_interval: NATIVE_TRIM_INTERVAL,
            max_streaming_buffering: MAX_STREAMING_BUFFERING,
            drain_prefetch_timeout: DRAIN_PREFETCH_TIMEOUT,
            sampler_interval: SAMPLER_INTERVAL,
            worker_ratio: OPTIMAL_WORKER_RATIO,
            uast_worker_ratio: UAST_PIPELINE_WORKER_RATIO,
            leaf_worker_divisor: LEAF_WORKER_DIVISOR,
            min_leaf_workers: MIN_LEAF_WORKERS,
            buffer_size_multiplier: BUFFER_SIZE_MULTIPLIER,
            budget_limit_ratio: BUDGET_LIMIT_RATIO,
            system_ram_limit_ratio: SYSTEM_RAM_LIMIT_RATIO,
            diff_job_buffer_multiplier: DIFF_JOB_BUFFER_MULTIPLIER,
        }
    }
}

/// Default blob LRU cache size (256 MiB). Duplicated from the blob cache's
/// default so `default()` is self-contained; keep the two in sync.
pub const DEFAULT_LRU_CACHE_SIZE: i64 = 256 * 1024 * 1024;

impl CoordinatorConfig {
    /// Returns the estimated memory consumed by the pipeline infrastructure
    /// (runtime, workers, caches, buffers, native/mmap overhead) — everything
    /// except analyzer state.
    #[must_use]
    pub const fn estimated_overhead(&self) -> i64 {
        let workers =
            self.workers * (REPO_HANDLE_SIZE + self.blob_arena_size + WORKER_NATIVE_OVERHEAD);
        let caches = self.blob_cache_size + self.diff_cache_size * AVG_DIFF_ENTRY_SIZE;
        let buffers = self.buffer_size * AVG_COMMIT_DATA_SIZE;
        RUNTIME_OVERHEAD + workers + caches + buffers
    }
}

/// Detects total system memory in bytes from `/proc/meminfo`, returning 0 on
/// non-Linux or any failure.
#[must_use]
pub fn detect_total_memory_bytes() -> u64 {
    if !cfg!(target_os = "linux") {
        return 0;
    }
    std::fs::read(PROC_MEMINFO_PATH).map_or(0, |bytes| parse_mem_total_bytes(&bytes))
}

/// Parses the `MemTotal:` line out of `/proc/meminfo` contents: scans for the
/// prefix, reads the numeric field, and scales by the unit (`kB` → KiB,
/// otherwise raw bytes).
#[must_use]
pub fn parse_mem_total_bytes(mem_info: &[u8]) -> u64 {
    for line in mem_info.split(|&b| b == b'\n') {
        if !line.starts_with(MEM_TOTAL_PREFIX.as_bytes()) {
            continue;
        }
        let fields: Vec<&[u8]> = line
            .split(u8::is_ascii_whitespace)
            .filter(|f| !f.is_empty())
            .collect();
        if fields.len() < MIN_MEMINFO_FIELDS {
            return 0;
        }
        let Some(value) = std::str::from_utf8(fields[1])
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
        else {
            return 0;
        };
        let unit = if fields.len() > MIN_MEMINFO_FIELDS {
            std::str::from_utf8(fields[2]).unwrap_or("")
        } else {
            MEM_TOTAL_UNIT_KIB
        };
        return scale_bytes_by_unit(value, unit);
    }
    0
}

fn scale_bytes_by_unit(value: u64, unit: &str) -> u64 {
    match unit {
        MEM_TOTAL_UNIT_KIB => value * KIBIBYTE,
        _ => value,
    }
}

/// Computes the soft memory limit from a user budget, capped at a fraction of
/// system RAM.
#[must_use]
pub fn resolve_memory_limit_from_budget(
    budget: i64,
    total_memory_bytes: u64,
    budget_ratio: i64,
    system_ratio: i64,
) -> u64 {
    let budget_based = (budget as u64) * (budget_ratio as u64) / PERCENT_DIVISOR;
    if total_memory_bytes > 0 {
        let system_cap = total_memory_bytes * (system_ratio as u64) / PERCENT_DIVISOR;
        return budget_based.min(system_cap);
    }
    budget_based
}

/// Computes the soft memory limit from a fraction of system RAM, capped at
/// [`DEFAULT_MEMORY_LIMIT_BYTES`]. A zero total falls back to the default
/// limit.
#[must_use]
pub fn resolve_memory_limit_with_ratio(total_memory_bytes: u64, ratio: i64) -> u64 {
    if total_memory_bytes == 0 {
        return DEFAULT_MEMORY_LIMIT_BYTES;
    }
    let system_based = total_memory_bytes * (ratio as u64) / PERCENT_DIVISOR;
    system_based.min(DEFAULT_MEMORY_LIMIT_BYTES)
}

/// Saturating cast of a memory limit (u64) to the `i64` the runtime
/// soft-memory-limit setter expects.
#[must_use]
pub fn memory_limit_as_i64(limit: u64) -> i64 {
    safe_int64(limit)
}

/// Logical CPU count: the std parallelism hint, falling back to 1.
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_stats_add_accumulates() {
        let mut a = PipelineStats {
            blob_duration: Duration::from_secs(1),
            blob_cache_hits: 2,
            ..PipelineStats::default()
        };
        let b = PipelineStats {
            blob_duration: Duration::from_secs(3),
            blob_cache_hits: 5,
            diff_cache_misses: 4,
            ..PipelineStats::default()
        };
        a.add(&b);
        assert_eq!(a.blob_duration, Duration::from_secs(4));
        assert_eq!(a.blob_cache_hits, 7);
        assert_eq!(a.diff_cache_misses, 4);
    }

    #[test]
    fn default_config_has_sane_workers() {
        let c = CoordinatorConfig::default();
        assert!(c.workers >= 1);
        assert_eq!(c.buffer_size, c.workers * BUFFER_SIZE_MULTIPLIER);
        assert!(c.leaf_workers >= MIN_LEAF_WORKERS);
        assert_eq!(c.commit_batch_size, DEFAULT_COMMIT_BATCH_SIZE);
        assert_eq!(c.memory_limit_ratio, MEMORY_LIMIT_RATIO);
        assert!(c.tree_diff_pathspec.is_empty());
    }

    #[test]
    fn estimated_overhead_matches_formula() {
        let c = CoordinatorConfig {
            workers: 2,
            blob_arena_size: 4 * 1024 * 1024,
            blob_cache_size: 100,
            diff_cache_size: 10,
            buffer_size: 3,
            ..CoordinatorConfig::default()
        };

        let workers = 2 * (REPO_HANDLE_SIZE + 4 * 1024 * 1024 + WORKER_NATIVE_OVERHEAD);
        let caches = 100 + 10 * AVG_DIFF_ENTRY_SIZE;
        let buffers = 3 * AVG_COMMIT_DATA_SIZE;
        assert_eq!(
            c.estimated_overhead(),
            RUNTIME_OVERHEAD + workers + caches + buffers
        );
    }

    #[test]
    fn parse_mem_total_kib() {
        let info = b"MemFree: 100 kB\nMemTotal:       16384 kB\nSwapTotal: 0 kB\n";
        assert_eq!(parse_mem_total_bytes(info), 16384 * 1024);
    }

    #[test]
    fn parse_mem_total_two_fields_defaults_to_kib() {
        // When there are exactly MIN_MEMINFO_FIELDS fields the unit is NOT
        // read from fields[2] and stays the default "kB", so a bare
        // "MemTotal: 2048" is scaled by 1024 (reference-implementation
        // behavior).
        let info = b"MemTotal: 2048\n";
        assert_eq!(parse_mem_total_bytes(info), 2048 * 1024);
    }

    #[test]
    fn parse_mem_total_explicit_non_kib_unit_is_raw() {
        // A third field that is not "kB" scales by the unit's factor; an
        // unrecognized unit ("B") falls through to the raw value.
        let info = b"MemTotal: 2048 B\n";
        assert_eq!(parse_mem_total_bytes(info), 2048);
    }

    #[test]
    fn parse_mem_total_missing_is_zero() {
        assert_eq!(parse_mem_total_bytes(b"Foo: 1 kB\n"), 0);
    }

    #[test]
    fn parse_mem_total_bad_number_is_zero() {
        assert_eq!(parse_mem_total_bytes(b"MemTotal: notanum kB\n"), 0);
    }

    #[test]
    fn limit_from_budget_capped_by_system() {
        // budget 1000 * 95% = 950; system 100 * 90% = 90 -> min = 90.
        assert_eq!(resolve_memory_limit_from_budget(1000, 100, 95, 90), 90);
    }

    #[test]
    fn limit_from_budget_no_system_info() {
        assert_eq!(resolve_memory_limit_from_budget(1000, 0, 95, 90), 950);
    }

    #[test]
    fn limit_with_ratio_zero_total_is_default() {
        assert_eq!(
            resolve_memory_limit_with_ratio(0, 75),
            DEFAULT_MEMORY_LIMIT_BYTES
        );
    }

    #[test]
    fn limit_with_ratio_caps_at_default() {
        // Huge total -> capped at DEFAULT_MEMORY_LIMIT_BYTES.
        let huge = 1_000 * 1024 * 1024 * 1024u64;
        assert_eq!(
            resolve_memory_limit_with_ratio(huge, 75),
            DEFAULT_MEMORY_LIMIT_BYTES
        );
    }

    #[test]
    fn limit_with_ratio_below_cap() {
        // 4 GiB * 75% = 3 GiB, below the 8 GiB cap.
        let four_gib = 4 * 1024 * 1024 * 1024u64;
        assert_eq!(
            resolve_memory_limit_with_ratio(four_gib, 75),
            3 * 1024 * 1024 * 1024
        );
    }
}
