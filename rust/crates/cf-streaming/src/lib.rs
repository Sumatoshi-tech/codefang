//! Streaming / windowed aggregation over commit history.
//!
//! Port of the Go package `internal/streaming`. Provides memory-bounded chunked
//! execution with analyzer hibernation:
//!
//! - [`Planner`] / [`AdaptivePlanner`] — compute chunk boundaries from a memory
//!   budget and adaptively re-plan based on observed per-commit growth.
//! - [`compute_schedule`] — the unified budget-aware scheduler decomposing the
//!   budget into pipeline / working-state / aggregator / chunk regions.
//! - [`check_memory_pressure`] — heap-vs-budget pressure detection.
//! - [`Hibernatable`] / [`SpillCleaner`] / [`SpillCleanupGuard`] — hibernation
//!   and spill-cleanup interfaces used by the framework, common, devs, and
//!   shotness analyzers.
//! - [`log_chunk_memory`] — per-chunk memory telemetry.
//!
//! Chunk ranges come from [`cf_alg`] ([`ChunkBounds`] aliases `cf_alg::Range`),
//! EMA smoothing and divergence detection from [`cf_alg_stats`], byte-unit
//! constants from [`cf_units`], and signal-driven spill cleanup from
//! [`cf_sigutil`].
//!
//! # Determinism
//!
//! All planning arithmetic is pure integer math on `i64` (Go `int`/`int64`), so
//! results are identical across platforms. No report serialization happens in
//! this crate — it produces plans and telemetry, not machine-format output —
//! so the shared `cf-gojson` / `cf-goyaml` encoders are not needed here.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod hibernatable;
mod memlog;
mod planner;

pub use hibernatable::{Hibernatable, HibernateError, SpillCleaner, SpillCleanupGuard};
pub use memlog::{log_chunk_memory, ChunkMemoryLog, CHUNK_MEMORY_MSG};
pub use planner::{
    check_memory_pressure, compute_schedule, AdaptivePlanner, AdaptiveStats, ChunkBounds,
    MemoryPressureLevel, Planner, ReplanObservation, Schedule, SchedulerConfig, AGG_STATE_PERCENT,
    BASE_OVERHEAD, CHUNK_MEM_PERCENT, DEFAULT_AVG_TC_SIZE, DEFAULT_EMA_ALPHA,
    DEFAULT_REPLAN_THRESHOLD, DEFAULT_STATE_GROWTH_PER_COMMIT, DEFAULT_WORKING_STATE_SIZE,
    MAX_CHUNK_SIZE, MIN_CHUNK_SIZE, PRESSURE_CRITICAL_RATIO, PRESSURE_WARNING_RATIO, USABLE_PERCENT,
    WORK_STATE_PERCENT,
};
