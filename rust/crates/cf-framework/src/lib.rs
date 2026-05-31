//! Port of the Go `internal/framework` package — the CENTRAL analysis framework.
//!
//! The Go package provides: the `Item`/`Analyzer` interfaces, pipeline
//! orchestration (the [`coordinator`] of blob/diff/UAST stages), the plugin
//! registry + DAG execution (the runner), streaming + UAST pipeline
//! integration, and budget hooks. See `specs/rust-rewrite/DESIGN.md` §1 (tier
//! 3-5) for where this crate sits in the workspace.
//!
//! # Port status (read this)
//!
//! `cf-framework` is a high-tier crate. Its *concrete* pipeline stages
//! (`blob_pipeline`, `diff_pipeline`, `uast_pipeline`, `runner`, `streaming`)
//! depend directly on a stack of crates that are not yet ported:
//!
//! - `cf-gitlib`   (`pkg/gitlib`)            — git2-backed `Repository`, `Commit`, `Hash`, `Worker`.
//! - `cf-cache`    (`internal/cache`)        — `LRUBlobCache`.
//! - `cf-uast`     (`pkg/uast`)              — the UAST `Parser`.
//! - `cf-plumbing` (`internal/plumbing`)     — `CommitData`, `FileDiffData`.
//! - `cf-analyze`  (`internal/analyzers/analyze`) — `Analyzer`, `DependencyGraph`, `Report`.
//!
//! Per the port rules, the modules whose Go source is **self-contained** are
//! ported fully here, and the cross-crate boundaries are expressed as the
//! minimal traits/types in [`interfaces`] so this crate compiles and is
//! unit-testable in isolation. Once the upstream crates are available, the
//! concrete stages re-target those types instead of the local shims (the public
//! shapes are kept identical to the Go structs to make that swap mechanical).
//!
//! ## Ported (full, behavior-exact, with unit tests)
//!
//! - [`config`]        — `config.go`: `ConfigParams`, `CheckpointParams`, the
//!   `default_memory_budget*` helpers, `build_config_from_params`, size/duration
//!   parsing (faithful go-humanize `ParseBytes` + Go `time.ParseDuration` ports).
//! - [`coordinator`]   — `coordinator.go`: `CoordinatorConfig`,
//!   `DefaultCoordinatorConfig`, `PipelineStats`, the memory-model overhead
//!   estimate, and the `/proc/meminfo` + memory-limit math.
//! - [`stage_metrics`] — `stage_metrics.go`: atomic per-stage high-watermark
//!   counters + snapshot.
//! - [`diff_cache`]    — `diff_cache.go`: `DiffKey`, key-byte layout,
//!   `DiffCacheStats`/`hit_rate`.
//! - [`commit_streamer`] — `commit_streamer.go`: batch streaming.
//! - [`sampler`]       — `sampler.go`: `SamplerConfig`/`PipelineSampler` config
//!   surface (the periodic logger).
//! - [`profiling`]     — `profiling.go`: CPU/heap profile entry points.
//! - [`budget`]        — the `BudgetHook` interface + `BudgetSnapshot`
//!   (`runner.go`).
//!
//! ## Deferred (blocked on upstream crates; interface defined in [`interfaces`])
//!
//! `runner.go` (the plugin registry + DAG execution loop), `streaming.go`
//! (chunked/streaming run), `blob_pipeline.go`, `diff_pipeline.go`, and
//! `uast_pipeline.go` are deferred until the unported `cf-gitlib`/`cf-cache`/
//! `cf-uast`/`cf-plumbing`/`cf-analyze` crates are available. Their cross-crate
//! boundary types live in [`interfaces`]; the remaining work is tracked in the
//! crate's port notes.

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
