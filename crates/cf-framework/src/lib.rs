//! Analysis-framework configuration and shared pipeline plumbing.
//!
//! The crate provides the dependency-light core that the pipeline stages and
//! the CLI build on:
//!
//! - [`config`]        — `ConfigParams`, `CheckpointParams`, the
//!   `default_memory_budget*` helpers, `build_config_from_params`, and the
//!   size/duration string parsing (CLI input contract).
//! - [`coordinator`]   — `CoordinatorConfig` + defaults, `PipelineStats`, the
//!   memory-model overhead estimate, and the `/proc/meminfo` + memory-limit
//!   math.
//! - [`stage_metrics`] — atomic per-stage high-watermark counters + snapshot.
//! - [`diff_cache`]    — `DiffKey`, its key-byte layout, and
//!   `DiffCacheStats`/`hit_rate`.
//! - [`commit_streamer`] — batch streaming with bounded lookahead.
//! - [`sampler`]       — `SamplerConfig`/`PipelineSampler` (the periodic
//!   metrics logger).
//! - [`profiling`]     — CPU/heap profile entry points.
//! - [`budget`]        — the `BudgetHook` interface + `BudgetSnapshot`.
//!
//! Cross-crate boundary shapes (git hash, cache-stats provider) live in
//! [`interfaces`] so this crate stays unit-testable in isolation; they are
//! layout-identical to the owning crates' types. See
//! `specs/rust-rewrite/DESIGN.md` §1 (tier 3-5) for where this crate sits in
//! the workspace.
//!
//! Compatibility: configuration defaults and parsing semantics feed analyzer
//! output paths indirectly; output bytes are pinned against the reference
//! implementation by `tests/compat`.
//!
//! # Example
//!
//! The CLI-input parsers and the diff-cache key/stats helpers are pure and
//! self-contained:
//!
//! ```
//! use std::time::Duration;
//! use cf_framework::{parse_bytes, parse_go_duration};
//! use cf_framework::{DiffKey, diff_key_to_bytes, DiffCacheStats, HASH_SIZE};
//!
//! // Humanized byte sizes: SI (MB) vs IEC (MiB) multipliers.
//! assert_eq!(parse_bytes("42 MB").unwrap(), 42_000_000);
//! assert_eq!(parse_bytes("42 mib").unwrap(), 44_040_192);
//! assert!(parse_bytes("not-a-size").is_err());
//!
//! // Go-style durations.
//! assert_eq!(parse_go_duration("500ms").unwrap(), Duration::from_millis(500));
//! assert_eq!(parse_go_duration("0").unwrap(), Duration::ZERO);
//!
//! // A diff-cache key serializes old_hash then new_hash into a 2*HASH_SIZE buffer.
//! let key = DiffKey { old_hash: [0xAA; HASH_SIZE], new_hash: [0xBB; HASH_SIZE] };
//! let bytes = diff_key_to_bytes(&key);
//! assert_eq!(&bytes[..HASH_SIZE], &[0xAA; HASH_SIZE]);
//! assert_eq!(&bytes[HASH_SIZE..], &[0xBB; HASH_SIZE]);
//!
//! // Hit rate is hits / (hits + misses), or 0 with no lookups.
//! assert_eq!(DiffCacheStats::default().hit_rate(), 0.0);
//! let stats = DiffCacheStats { hits: 3, misses: 1, ..Default::default() };
//! assert_eq!(stats.hit_rate(), 0.75);
//! ```

#![forbid(unsafe_code)]

pub mod budget;
pub mod commit_streamer;
pub mod config;
pub mod coordinator;
pub mod diff_cache;
pub mod interfaces;
pub mod profiling;
pub mod sampler;
pub mod stage_metrics;

pub use budget::{BudgetHook, BudgetSnapshot};
pub use commit_streamer::{CommitBatch, CommitStreamer};
pub use config::{
    build_config_from_params, default_memory_budget, default_memory_budget_with_params,
    parse_bytes, parse_go_duration, parse_optional_size, BudgetSolver, CheckpointParams,
    ConfigError, ConfigParams,
};
pub use coordinator::{
    detect_total_memory_bytes, resolve_memory_limit_from_budget, resolve_memory_limit_with_ratio,
    CoordinatorConfig, PipelineStats,
};
pub use diff_cache::{diff_key_to_bytes, DiffCacheStats, DiffKey, DEFAULT_DIFF_CACHE_SIZE};
pub use interfaces::{cache_stats, CacheStatsProvider, Hash, HASH_SIZE};
pub use profiling::{
    maybe_start_cpu_profile, maybe_write_heap_profile, CpuProfileGuard, NoopProfiler, Profiler,
};
pub use sampler::{
    profile_path, NoopSink, PipelineSampler, SamplerConfig, SamplerSink, SAMPLER_INTERVAL,
};
pub use stage_metrics::{StageMetrics, StageMetricsSnapshot};
